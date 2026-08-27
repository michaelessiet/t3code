//! Open-file save/conflict lifecycle for editable buffers.
//!
//! Port of the semantics of
//! `apps/web/src/components/files/fileSaveCoordinator.ts` (save coalescing),
//! `fileSaveState.ts` (base + self-written revision bookkeeping), and
//! `fileBufferConflict.ts` (conflict detection). Pure decision core: the UI
//! layer owns the autosave debounce clock and the `projects.readFile` /
//! `projects.writeFile` RPCs — this module decides what should happen next.
//!
//! Revisions are the server's content-hash tokens
//! (`ProjectReadFileResult.revision` / `ProjectWriteFileResult.revision`,
//! flattened from the wire `Option<Option<_>>`). Unlike Electron's
//! `fileContentRevision.ts`, Vitre does no client-side hashing: self-written
//! detection works on revisions returned by `writeFile`. The
//! watcher-vs-write-confirmation race Electron closes by recording the hash
//! *before* the write RPC is closed here by deferring any watcher-triggered
//! re-read that lands while a save is in flight and replaying it once the
//! save resolves.

use std::collections::VecDeque;

/// Bound on remembered self-written revisions (Electron's
/// `SELF_WRITTEN_REVISION_LIMIT` in `fileSaveState.ts`).
const SELF_WRITTEN_REVISION_LIMIT: usize = 32;

/// Why the buffer stopped persisting. Mirrors Electron's two conflict kinds
/// (`FileBufferConflictReason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferConflict {
    /// The server rejected a save because the buffer's base revision no
    /// longer matches the disk contents (`writeFile` failure
    /// `stale_revision`).
    StaleSave,
    /// The workspace watcher reported the file changed on disk while the
    /// buffer held unsaved edits.
    ExternalChange,
}

/// What the caller must do after a state transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskChange {
    None,
    /// Re-read the file and complete via [`FileBuffer::resolve_reload`] with
    /// the freshly read revision.
    Reload,
    /// Show the conflict banner; the reason is in [`FileBuffer::conflict`].
    Conflict,
}

/// Result of [`FileBuffer::save_succeeded`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveOutcome {
    /// Caller should start another save immediately (edits landed mid-save).
    pub resave: bool,
    /// A deferred watcher event resolved to this outcome after the save.
    pub disk: DiskChange,
}

/// Save/conflict state machine for one open file.
///
/// Invariants: a conflicted buffer never has a save in flight (conflicts only
/// arise when no write is pending), and a conflicted buffer is always dirty.
#[derive(Debug)]
pub struct FileBuffer {
    /// Revision the buffer's edits are based on; sent as the `baseRevision`
    /// write guard. `None` = unknown (older server, or the last write
    /// returned no revision) → unconditional writes, and — per Electron's
    /// `detectsExternalConflict` — no external-change conflicts.
    base_revision: Option<String>,
    dirty: bool,
    /// A `writeFile` is in flight (from `begin_save`/`resolve_keep_mine`
    /// until `save_succeeded`/`save_failed`/`save_failed_stale`).
    saving: bool,
    /// Edits landed while the in-flight save was running, so its snapshot is
    /// stale and another cycle must run once it resolves (Electron's
    /// `latestRevision` counter check).
    pending_save: bool,
    conflict: Option<BufferConflict>,
    /// Watcher re-read deferred while a save was in flight, replayed on save
    /// resolution. Latest event wins (revisions are content hashes; only the
    /// newest reflects the current disk). Outer = deferred, inner = the disk
    /// revision it carried.
    deferred_disk: Option<Option<String>>,
    /// Revisions this editor wrote, oldest first (bounded FIFO like
    /// Electron's `selfWrittenRevisions`). A matching disk revision means the
    /// disk holds exactly what this editor saved — never worth a conflict.
    self_written: VecDeque<String>,
}

impl FileBuffer {
    /// Buffer for a freshly read file. `revision` =
    /// `ProjectReadFileResult.revision` (flattened: only `Some(Some(_))` on
    /// the wire carries a value).
    pub fn open(revision: Option<String>) -> Self {
        Self {
            base_revision: revision,
            dirty: false,
            saving: false,
            pending_save: false,
            conflict: None,
            deferred_disk: None,
            self_written: VecDeque::new(),
        }
    }

    /// The editor content changed (user edit). Marks dirty. Returns true when
    /// the caller should (re)arm the autosave debounce timer (i.e. not
    /// conflicted).
    pub fn edited(&mut self) -> bool {
        self.dirty = true;
        if self.saving {
            self.pending_save = true;
        }
        self.conflict.is_none()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn conflict(&self) -> Option<BufferConflict> {
        self.conflict
    }

    /// Debounce fired or ⌘S: returns the baseRevision to send iff a save
    /// should start now (never while one is in flight; never while
    /// conflicted; never when clean). `Some(None)` = save with no
    /// baseRevision guard.
    ///
    /// A request while a save is in flight needs no extra flag: the in-flight
    /// write carries every edit made before it started, and later edits set
    /// the pending flag via [`Self::edited`] — the same outcome as Electron's
    /// `flushRequested` + `latestRevision` check.
    pub fn begin_save(&mut self) -> Option<Option<String>> {
        if self.conflict.is_some() || !self.dirty || self.saving {
            return None;
        }
        self.saving = true;
        self.pending_save = false;
        Some(self.base_revision.clone())
    }

    /// writeFile succeeded. Records the new revision as base + self-written,
    /// clears dirty UNLESS edits arrived during the save (then stays dirty
    /// and the return value tells the caller to immediately begin another
    /// save cycle). Also replays any watcher event deferred during the save;
    /// a resulting `Conflict` gates the resave via [`Self::begin_save`].
    pub fn save_succeeded(&mut self, new_revision: Option<String>) -> SaveOutcome {
        self.saving = false;
        if let Some(revision) = &new_revision {
            self.record_self_written(revision.clone());
        }
        // Electron parity: a write result without a revision clears the base
        // guard (`setFileBaseRevision(..., null)`).
        self.base_revision = new_revision;
        let resave = self.pending_save;
        self.pending_save = false;
        self.dirty = resave;
        let disk = match self.deferred_disk.take() {
            Some(disk_revision) => self.evaluate_disk(disk_revision),
            None => DiskChange::None,
        };
        SaveOutcome { resave, disk }
    }

    /// writeFile failed with failure=stale_revision → StaleSave conflict
    /// (stop persisting until resolved). Any deferred watcher event is
    /// dropped: the buffer is already conflicted and both reasons resolve the
    /// same way.
    pub fn save_failed_stale(&mut self) {
        self.saving = false;
        self.pending_save = false;
        self.deferred_disk = None;
        self.conflict = Some(BufferConflict::StaleSave);
    }

    /// writeFile failed for any other reason (buffer stays dirty, may retry
    /// later). A deferred watcher event still replays: a foreign disk
    /// revision surfaces as an ExternalChange conflict, observable via
    /// [`Self::conflict`].
    pub fn save_failed(&mut self) {
        self.saving = false;
        self.pending_save = false;
        if let Some(disk_revision) = self.deferred_disk.take() {
            self.evaluate_disk(disk_revision);
        }
    }

    /// Watcher-triggered re-read finished with this disk revision.
    /// Self-written or unchanged revisions → `None`. New foreign revision →
    /// `Reload` when clean, `Conflict(ExternalChange)` when dirty. Deferred
    /// (returns `None` now, replayed from
    /// [`Self::save_succeeded`]/[`Self::save_failed`]) while a save is in
    /// flight.
    pub fn disk_changed(&mut self, disk_revision: Option<String>) -> DiskChange {
        if self.saving {
            self.deferred_disk = Some(disk_revision);
            return DiskChange::None;
        }
        self.evaluate_disk(disk_revision)
    }

    /// Conflict resolution: reload from disk (buffer becomes clean at that
    /// revision). Also completes a clean [`DiskChange::Reload`] after the
    /// caller re-read the file. Assumes no save in flight (conflicted
    /// buffers never have one).
    pub fn resolve_reload(&mut self, disk_revision: Option<String>) {
        self.base_revision = disk_revision;
        self.dirty = false;
        self.pending_save = false;
        self.conflict = None;
        self.deferred_disk = None;
    }

    /// Conflict resolution: keep my version → returns the marker for an
    /// unconditional save (no baseRevision) and clears the conflict. On
    /// `true` the caller must issue the write immediately — it counts as in
    /// flight from this call (watcher events defer) and its outcome is
    /// reported via [`Self::save_succeeded`]/[`Self::save_failed`]. `false`
    /// = no conflict to resolve, nothing to write.
    pub fn resolve_keep_mine(&mut self) -> bool {
        if self.conflict.is_none() {
            return false;
        }
        self.conflict = None;
        self.deferred_disk = None;
        self.pending_save = false;
        self.saving = true;
        true
    }

    /// `detectsExternalConflict` port plus the clean-buffer reload decision.
    /// Must only run while no save is in flight.
    fn evaluate_disk(&mut self, disk_revision: Option<String>) -> DiskChange {
        let Some(revision) = disk_revision else {
            // Unknown disk revision (older server): never a conflict; a
            // clean buffer still reloads because the watcher saw a change.
            return if self.dirty {
                DiskChange::None
            } else {
                DiskChange::Reload
            };
        };
        if self.self_written.contains(&revision)
            || self.base_revision.as_deref() == Some(revision.as_str())
        {
            return DiskChange::None;
        }
        if !self.dirty {
            return DiskChange::Reload;
        }
        if self.base_revision.is_none() {
            // Electron: an unknown base revision never conflicts.
            return DiskChange::None;
        }
        // Electron's conflict effect overwrites the banner reason, so a
        // stale-save conflict upgrades to external-change here too.
        self.conflict = Some(BufferConflict::ExternalChange);
        DiskChange::Conflict
    }

    /// Bounded FIFO with move-to-back dedupe, like Electron's
    /// `recordSelfWrittenRevision`.
    fn record_self_written(&mut self, revision: String) {
        if let Some(index) = self.self_written.iter().position(|r| *r == revision) {
            self.self_written.remove(index);
        }
        self.self_written.push_back(revision);
        if self.self_written.len() > SELF_WRITTEN_REVISION_LIMIT {
            self.self_written.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rev(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    #[test]
    fn edit_save_success_happy_path() {
        let mut buffer = FileBuffer::open(rev("r1"));
        assert!(!buffer.is_dirty());
        assert_eq!(buffer.begin_save(), None); // clean → nothing to save

        assert!(buffer.edited());
        assert!(buffer.is_dirty());
        assert_eq!(buffer.begin_save(), Some(rev("r1")));
        assert!(buffer.is_dirty()); // unconfirmed until success

        let outcome = buffer.save_succeeded(rev("r2"));
        assert_eq!(
            outcome,
            SaveOutcome {
                resave: false,
                disk: DiskChange::None
            }
        );
        assert!(!buffer.is_dirty());
        assert_eq!(buffer.begin_save(), None);

        // The next save is guarded by the written revision.
        assert!(buffer.edited());
        assert_eq!(buffer.begin_save(), Some(rev("r2")));
    }

    #[test]
    fn edits_during_in_flight_save_coalesce_into_resave() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        assert_eq!(buffer.begin_save(), Some(rev("r1")));
        assert!(buffer.edited());
        assert!(buffer.edited()); // multiple edits coalesce into one cycle
        assert_eq!(buffer.begin_save(), None); // debounce fired mid-flight

        let outcome = buffer.save_succeeded(rev("r2"));
        assert_eq!(
            outcome,
            SaveOutcome {
                resave: true,
                disk: DiskChange::None
            }
        );
        assert!(buffer.is_dirty());

        // The follow-up save uses the fresh revision and settles clean.
        assert_eq!(buffer.begin_save(), Some(rev("r2")));
        let outcome = buffer.save_succeeded(rev("r3"));
        assert!(!outcome.resave);
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn save_request_while_in_flight_without_edits_does_not_resave() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        assert_eq!(buffer.begin_save(), Some(rev("r1")));
        // ⌘S while the autosave write is in flight: no second concurrent
        // save, and the in-flight write already carries every edit
        // (Electron: flushRequested with an unchanged latestRevision).
        assert_eq!(buffer.begin_save(), None);

        let outcome = buffer.save_succeeded(rev("r2"));
        assert!(!outcome.resave);
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn stale_failure_conflicts_and_blocks_persisting_until_resolved() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        assert_eq!(buffer.begin_save(), Some(rev("r1")));
        buffer.save_failed_stale();

        assert_eq!(buffer.conflict(), Some(BufferConflict::StaleSave));
        assert!(buffer.is_dirty());
        assert_eq!(buffer.begin_save(), None); // stop persisting
        assert!(!buffer.edited()); // no debounce rearm while conflicted
        assert_eq!(buffer.begin_save(), None);

        buffer.resolve_reload(rev("r9"));
        assert_eq!(buffer.conflict(), None);
        assert!(!buffer.is_dirty());
        assert!(buffer.edited()); // rearm works again
        assert_eq!(buffer.begin_save(), Some(rev("r9")));
    }

    #[test]
    fn external_change_reloads_when_clean_and_conflicts_when_dirty() {
        let mut buffer = FileBuffer::open(rev("r1"));
        assert_eq!(buffer.disk_changed(rev("r1")), DiskChange::None); // unchanged
        assert_eq!(buffer.disk_changed(rev("r2")), DiskChange::Reload);
        assert_eq!(buffer.conflict(), None);

        buffer.edited();
        assert_eq!(buffer.disk_changed(rev("r1")), DiskChange::None); // still on base
        assert_eq!(buffer.disk_changed(rev("r2")), DiskChange::Conflict);
        assert_eq!(buffer.conflict(), Some(BufferConflict::ExternalChange));
    }

    #[test]
    fn self_written_revisions_from_earlier_saves_are_ignored() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        buffer.begin_save();
        buffer.save_succeeded(rev("r2"));
        buffer.edited();
        buffer.begin_save();
        buffer.save_succeeded(rev("r3"));

        // A watcher re-read delivering either of our own writes: no change.
        assert_eq!(buffer.disk_changed(rev("r2")), DiskChange::None);
        assert_eq!(buffer.disk_changed(rev("r3")), DiskChange::None);
        // Even while dirty.
        buffer.edited();
        assert_eq!(buffer.disk_changed(rev("r2")), DiskChange::None);
        assert_eq!(buffer.conflict(), None);
    }

    #[test]
    fn watcher_event_during_save_defers_and_replays_self_written_as_none() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        assert_eq!(buffer.begin_save(), Some(rev("r1")));
        // The watcher already sees our in-flight write on disk; the revision
        // is only recognizable once writeFile returns → deferred.
        assert_eq!(buffer.disk_changed(rev("r2")), DiskChange::None);

        let outcome = buffer.save_succeeded(rev("r2"));
        assert_eq!(
            outcome,
            SaveOutcome {
                resave: false,
                disk: DiskChange::None
            }
        );
        assert_eq!(buffer.conflict(), None);
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn deferred_foreign_revision_conflicts_when_still_dirty_after_save() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        buffer.begin_save();
        buffer.edited(); // stays dirty through the save
        assert_eq!(buffer.disk_changed(rev("ext")), DiskChange::None); // deferred

        let outcome = buffer.save_succeeded(rev("r2"));
        assert!(outcome.resave);
        assert_eq!(outcome.disk, DiskChange::Conflict);
        assert_eq!(buffer.conflict(), Some(BufferConflict::ExternalChange));
        assert_eq!(buffer.begin_save(), None); // conflict gates the resave
    }

    #[test]
    fn deferred_foreign_revision_reloads_when_clean_after_save() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        buffer.begin_save();
        assert_eq!(buffer.disk_changed(rev("ext")), DiskChange::None); // deferred

        let outcome = buffer.save_succeeded(rev("r2"));
        assert!(!outcome.resave);
        assert_eq!(outcome.disk, DiskChange::Reload);
        assert_eq!(buffer.conflict(), None);
    }

    #[test]
    fn latest_deferred_watcher_event_wins() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        buffer.begin_save();
        // Two watcher events land during one save; only the newest reflects
        // the current disk. If the first won this would replay as Reload.
        assert_eq!(buffer.disk_changed(rev("ext")), DiskChange::None);
        assert_eq!(buffer.disk_changed(rev("r2")), DiskChange::None);

        let outcome = buffer.save_succeeded(rev("r2"));
        assert_eq!(outcome.disk, DiskChange::None);
    }

    #[test]
    fn deferred_event_replays_after_failed_save() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        buffer.begin_save();
        assert_eq!(buffer.disk_changed(rev("ext")), DiskChange::None); // deferred
        buffer.save_failed();

        assert!(buffer.is_dirty());
        assert_eq!(buffer.conflict(), Some(BufferConflict::ExternalChange));
        assert_eq!(buffer.begin_save(), None);
    }

    #[test]
    fn failed_save_keeps_dirty_and_allows_retry() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        assert_eq!(buffer.begin_save(), Some(rev("r1")));
        // Deferred event matching the base is not a change.
        assert_eq!(buffer.disk_changed(rev("r1")), DiskChange::None);
        buffer.save_failed();

        assert!(buffer.is_dirty());
        assert_eq!(buffer.conflict(), None);
        // Retry keeps the same base guard.
        assert_eq!(buffer.begin_save(), Some(rev("r1")));
    }

    #[test]
    fn resolve_keep_mine_starts_unconditional_save() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        buffer.begin_save();
        buffer.save_failed_stale();

        assert!(buffer.resolve_keep_mine());
        assert_eq!(buffer.conflict(), None);
        // The forced write is in flight: no second save, watcher defers.
        assert_eq!(buffer.begin_save(), None);
        assert_eq!(buffer.disk_changed(rev("r5")), DiskChange::None);

        let outcome = buffer.save_succeeded(rev("r5"));
        assert_eq!(
            outcome,
            SaveOutcome {
                resave: false,
                disk: DiskChange::None
            }
        );
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn resolve_keep_mine_without_conflict_is_a_no_op() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        assert!(!buffer.resolve_keep_mine());
        assert!(buffer.is_dirty());
        assert_eq!(buffer.begin_save(), Some(rev("r1"))); // normal path intact
    }

    #[test]
    fn unknown_revisions_never_conflict() {
        // Older server: readFile returned no revision → unconditional writes.
        let mut buffer = FileBuffer::open(None);
        buffer.edited();
        assert_eq!(buffer.begin_save(), Some(None));
        buffer.save_failed();
        // Foreign disk revision over an unknown base: never a conflict.
        assert_eq!(buffer.disk_changed(rev("r2")), DiskChange::None);
        assert_eq!(buffer.conflict(), None);

        // Unknown disk revision: reload when clean, ignore when dirty.
        let mut clean = FileBuffer::open(rev("r1"));
        assert_eq!(clean.disk_changed(None), DiskChange::Reload);
        clean.edited();
        assert_eq!(clean.disk_changed(None), DiskChange::None);
        assert_eq!(clean.conflict(), None);
    }

    #[test]
    fn write_without_returned_revision_drops_the_base_guard() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        assert_eq!(buffer.begin_save(), Some(rev("r1")));
        buffer.save_succeeded(None);
        buffer.edited();
        assert_eq!(buffer.begin_save(), Some(None));
    }

    #[test]
    fn later_external_change_updates_a_stale_save_conflict_reason() {
        let mut buffer = FileBuffer::open(rev("r1"));
        buffer.edited();
        buffer.begin_save();
        buffer.save_failed_stale();
        assert_eq!(buffer.conflict(), Some(BufferConflict::StaleSave));

        // The watcher then delivers the foreign revision that caused the
        // staleness; Electron's conflict effect overwrites the banner reason.
        assert_eq!(buffer.disk_changed(rev("ext")), DiskChange::Conflict);
        assert_eq!(buffer.conflict(), Some(BufferConflict::ExternalChange));
    }

    #[test]
    fn self_written_memory_is_bounded_fifo() {
        let mut buffer = FileBuffer::open(rev("w0"));
        for i in 1..=SELF_WRITTEN_REVISION_LIMIT + 1 {
            buffer.edited();
            assert!(buffer.begin_save().is_some());
            buffer.save_succeeded(rev(&format!("w{i}")));
        }
        // w1 was evicted (33rd write), w2 is still remembered.
        assert_eq!(buffer.disk_changed(rev("w1")), DiskChange::Reload);
        assert_eq!(buffer.disk_changed(rev("w2")), DiskChange::None);
    }
}
