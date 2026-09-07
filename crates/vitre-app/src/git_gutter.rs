//! Editor git diff gutter: the panel-side half of Electron's
//! `gitDiffGutter.ts` + `gitDiffBaselineState.ts`.
//!
//! [`vitre_state::git_gutter`] owns the diff itself; this owns the baseline
//! the diff runs against, the debounce that keeps it off the typing path, and
//! the open hunk peek. [`FilesPanel`](crate::files::FilesPanel) wires it to
//! the RPC, the editor and the keymap.

use gpui_component::input::{DiffGutter, DiffMarker, DiffMarkerKind, DiffWedge};
use vitre_contracts::{
    VcsBaselineBlobStatus, VcsFileBaselineResult, VcsFileBaselineResultRepository,
};
use vitre_state::git_gutter::{
    DiffChunk, DiffText, MAX_DIFF_LINES, MarkerKind, MarkerSpec, MarkerWedge, build_chunks,
    chunk_anchor_line, chunk_index_at_line, chunk_original_text, chunk_revert_edit,
    compute_marker_specs, goto_chunk_index,
};

/// Electron's `RECOMPUTE_DEBOUNCE_MS`: markers track *unsaved* edits, but not
/// on every keystroke.
pub(crate) const RECOMPUTE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);

/// The HEAD and index blobs the buffer is diffed against.
#[derive(Debug)]
struct Baseline {
    /// `head_oid:index_oid`, the key a redundant refetch is skipped on.
    key: String,
    head: DiffText,
    index: Option<DiffText>,
}

/// Everything the gutter needs for the open file. Empty until a baseline
/// arrives, which is also what a file outside a repository stays.
#[derive(Debug, Default)]
pub(crate) struct GitGutterState {
    baseline: Option<Baseline>,
    /// The document the chunks and specs below were computed from.
    doc: DiffText,
    chunks: Vec<DiffChunk>,
    specs: Vec<MarkerSpec>,
    /// Chunk index of the open hunk peek.
    peek: Option<usize>,
    /// Debounce generation: each edit bumps it, only the latest recomputes.
    debounce: u64,
}

impl GitGutterState {
    /// Forget everything — a different file is being opened.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn has_baseline(&self) -> bool {
        self.baseline.is_some()
    }

    pub(crate) fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub(crate) fn peek_index(&self) -> Option<usize> {
        self.peek
    }

    /// Bump and return the debounce generation. A recompute whose generation
    /// is stale by the time its timer fires drops.
    pub(crate) fn arm_debounce(&mut self) -> u64 {
        self.debounce += 1;
        self.debounce
    }

    pub(crate) fn debounce_is_current(&self, generation: u64) -> bool {
        self.debounce == generation
    }

    /// Adopt a `vcs.getFileBaseline` result.
    ///
    /// Returns whether anything changed. The oids are the cache key, so a
    /// refetch that lands on the same commit and index is dropped here before
    /// the blobs are even indexed — Electron's `setGitDiffBaseline` no-op
    /// guard, which its baseline atom leans on hard: the VCS status stream
    /// carries no HEAD sha, so it refetches on *every* status emission.
    ///
    /// Anything short of a readable HEAD blob — no repository, untracked,
    /// binary, too large — leaves no baseline, and the marker column with it.
    pub(crate) fn set_baseline(&mut self, result: Option<&VcsFileBaselineResult>) -> bool {
        let blobs = result.and_then(Self::readable_blobs);
        let key = blobs
            .map(|(head_oid, _, index_oid, _)| format!("{head_oid}:{}", index_oid.unwrap_or("")));
        if self.baseline.as_ref().map(|baseline| baseline.key.as_str()) == key.as_deref() {
            return false;
        }
        self.baseline = key.zip(blobs).map(|(key, (_, head, _, index))| Baseline {
            key,
            head: DiffText::new(head),
            index: index.map(DiffText::new),
        });
        self.peek = None;
        true
    }

    /// The blobs the gutter can diff against, as
    /// `(head_oid, head_contents, index_oid, index_contents)`.
    ///
    /// `contents` is non-null only for status "ok": "binary" and "too-large"
    /// carry an oid but nothing to diff against, and "absent" — untracked, or
    /// added in the index — not even that. An unreadable *index* is not
    /// fatal, it only means nothing can be classified as staged.
    fn readable_blobs(
        result: &VcsFileBaselineResult,
    ) -> Option<(&str, &str, Option<&str>, Option<&str>)> {
        if result.repository != VcsFileBaselineResultRepository::Ok
            || result.head.status != VcsBaselineBlobStatus::Ok
        {
            return None;
        }
        let index = (result.index.status == VcsBaselineBlobStatus::Ok).then_some(&result.index);
        Some((
            result.head.oid.as_ref()?.0.as_str(),
            result.head.contents.as_ref()?.0.as_str(),
            index
                .and_then(|index| index.oid.as_ref())
                .map(|oid| oid.0.as_str()),
            index
                .and_then(|index| index.contents.as_ref())
                .map(|contents| contents.0.as_str()),
        ))
    }

    /// Re-diff `doc` against the baseline.
    ///
    /// A document (or baseline) past [`MAX_DIFF_LINES`] leaves the gutter
    /// empty rather than spending a frame on it.
    pub(crate) fn recompute(&mut self, doc: &str) {
        let doc = DiffText::new(doc);
        let Some(baseline) = self.baseline.as_ref() else {
            self.doc = doc;
            self.chunks = Vec::new();
            self.specs = Vec::new();
            self.peek = None;
            return;
        };
        if doc.lines_len() > MAX_DIFF_LINES || baseline.head.lines_len() > MAX_DIFF_LINES {
            self.doc = doc;
            self.chunks = Vec::new();
            self.specs = Vec::new();
            self.peek = None;
            return;
        }
        self.chunks = build_chunks(&baseline.head, &doc);
        self.specs = compute_marker_specs(&baseline.head, &doc, baseline.index.as_ref());
        self.doc = doc;
        // Whatever moved the buffer or the baseline invalidated the captured
        // hunk ranges, so the peek closes — the same two reasons CodeMirror's
        // peek field drops (`docChanged`, `setBaselineEffect`).
        self.peek = None;
    }

    /// The gutter to hand the editor, or `None` when the file has no baseline
    /// at all (outside a repository, untracked, binary): the marker column
    /// itself goes away, rather than sitting there permanently empty.
    pub(crate) fn gutter(
        &self,
        added: gpui::Hsla,
        modified: gpui::Hsla,
        deleted: gpui::Hsla,
    ) -> Option<DiffGutter> {
        self.baseline.as_ref()?;
        let markers = self
            .specs
            .iter()
            .map(|spec| {
                // The editor counts buffer lines from zero.
                DiffMarker::new(spec.line.saturating_sub(1))
                    .kind(spec.kind.map(|kind| match kind {
                        MarkerKind::Added => DiffMarkerKind::Added,
                        MarkerKind::Modified => DiffMarkerKind::Modified,
                    }))
                    .staged(spec.staged)
                    .wedge(spec.wedge.map(|wedge| match wedge {
                        MarkerWedge::Above => DiffWedge::Above,
                        MarkerWedge::Below => DiffWedge::Below,
                    }))
            })
            .collect();
        Some(DiffGutter::new(added, modified, deleted).markers(markers))
    }

    /// Toggle the peek for the chunk owning zero-based buffer `line`.
    ///
    /// Returns the newly open chunk index, or `None` when the click closed
    /// the peek or landed on no chunk at all.
    pub(crate) fn toggle_peek_at_line(&mut self, line: usize) -> Option<usize> {
        let index = chunk_index_at_line(&self.doc, &self.chunks, line + 1)?;
        if self.peek == Some(index) {
            self.peek = None;
            return None;
        }
        self.peek = Some(index);
        Some(index)
    }

    /// Move the peek `step` chunks along, wrapping. Returns the new index.
    pub(crate) fn step_peek(&mut self, step: isize) -> Option<usize> {
        let count = self.chunks.len();
        if count == 0 {
            return None;
        }
        let from = self.peek? as isize;
        let index = (from + step).rem_euclid(count as isize) as usize;
        self.peek = Some(index);
        Some(index)
    }

    /// Close the peek. Returns whether one was open.
    pub(crate) fn close_peek(&mut self) -> bool {
        self.peek.take().is_some()
    }

    /// Zero-based buffer line the peek card anchors under.
    pub(crate) fn peek_anchor_line(&self) -> Option<usize> {
        let chunk = self.chunks.get(self.peek?)?;
        Some(chunk_anchor_line(&self.doc, chunk).saturating_sub(1))
    }

    /// The peek body: the hunk's HEAD lines, empty for a pure addition.
    pub(crate) fn peek_original_text(&self) -> Option<String> {
        let baseline = self.baseline.as_ref()?;
        let chunk = self.chunks.get(self.peek?)?;
        Some(chunk_original_text(&baseline.head, chunk))
    }

    /// Byte offset a chunk starts at, for scrolling it into view.
    pub(crate) fn chunk_offset(&self, index: usize) -> Option<usize> {
        let chunk = self.chunks.get(index)?;
        Some(chunk.from_b.min(self.doc.len()))
    }

    /// The buffer edit that reverts the open peek's hunk to HEAD.
    pub(crate) fn peek_revert_edit(&self) -> Option<(std::ops::Range<usize>, String)> {
        let baseline = self.baseline.as_ref()?;
        let chunk = self.chunks.get(self.peek?)?;
        Some(chunk_revert_edit(&baseline.head, &self.doc, chunk))
    }

    /// Offset of the next/previous hunk from `offset`, wrapping at both ends.
    pub(crate) fn goto_offset(&self, offset: usize, forward: bool) -> Option<usize> {
        let index = goto_chunk_index(&self.doc, &self.chunks, offset, forward)?;
        self.chunk_offset(index)
    }
}
