//! Sidebar preferences and UI state, persisted under the Vitre home.
//!
//! Electron splits these across two stores — the sidebar keys of
//! `ClientSettings` (`packages/contracts/src/settings.ts`, persisted by
//! `useSettings.ts`) and the browser-local `uiStateStore` (expansion,
//! per-project order, last-visited stamps). Neither is server state: the
//! sidecar's `settings.json` holds `ServerSettings` only. Vitre keeps both in
//! one file, `<home>/sidebar-state.json`, with the same defaults so a fresh
//! profile renders the sidebar identically to a fresh Electron profile.
//!
//! Writes are best-effort: a sidebar preference is not worth failing a render
//! over, so a persist error is logged and dropped.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vitre_state::project_grouping::{ProjectGroupingMode, ProjectGroupingSettings};
use vitre_state::sidebar::{
    DEFAULT_THREAD_PREVIEW_COUNT, MAX_THREAD_PREVIEW_COUNT, MIN_THREAD_PREVIEW_COUNT,
    ProjectSortOrder, ThreadSortOrder,
};

const FILE_NAME: &str = "sidebar-state.json";

/// The sidebar's persisted preferences plus its per-row UI state.
#[derive(Debug)]
pub struct SidebarPrefs {
    /// `sidebarProjectGroupingMode` + `sidebarProjectGroupingOverrides`.
    pub grouping: ProjectGroupingSettings,
    /// `sidebarProjectSortOrder`.
    pub project_sort_order: ProjectSortOrder,
    /// `sidebarThreadSortOrder`.
    pub thread_sort_order: ThreadSortOrder,
    /// `sidebarThreadPreviewCount`, already clamped to its 1..=15 range.
    pub thread_preview_count: usize,
    /// Explicit expand/collapse decisions, keyed by any of a row's
    /// preference keys (`projectExpansionPreferenceKeys`). Rows with no
    /// recorded decision default to expanded.
    project_expanded: HashMap<String, bool>,
    /// Rows whose "Show more" is currently on, keyed by logical project key.
    thread_lists_expanded: HashSet<String>,
    /// `manual` project order: physical project keys, first to last.
    project_order: Vec<String>,
    /// `threadLastVisitedAtById` — what makes a finished turn "unseen".
    thread_last_visited: HashMap<String, String>,
    path: PathBuf,
}

/// On-disk shape. Enums persist as their settings literals so the file stays
/// readable and forward-compatible with the TS contracts.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StoredPrefs {
    project_grouping_mode: Option<String>,
    project_grouping_overrides: HashMap<String, String>,
    project_sort_order: Option<String>,
    thread_sort_order: Option<String>,
    thread_preview_count: Option<u32>,
    project_expanded: HashMap<String, bool>,
    thread_lists_expanded: Vec<String>,
    project_order: Vec<String>,
    thread_last_visited: HashMap<String, String>,
}

impl SidebarPrefs {
    /// Load from `<home>/sidebar-state.json`, falling back to defaults for a
    /// missing or unreadable file (a corrupt file is not worth a startup
    /// failure — the user just gets the default sidebar back).
    pub fn load(home: &Path) -> Self {
        let path = home.join(FILE_NAME);
        let stored = std::fs::read_to_string(&path)
            .ok()
            .and_then(
                |contents| match serde_json::from_str::<StoredPrefs>(&contents) {
                    Ok(stored) => Some(stored),
                    Err(error) => {
                        eprintln!("[vitre] ignoring unreadable {FILE_NAME}: {error}");
                        None
                    }
                },
            )
            .unwrap_or_default();

        Self {
            grouping: ProjectGroupingSettings {
                mode: stored
                    .project_grouping_mode
                    .as_deref()
                    .and_then(ProjectGroupingMode::parse)
                    .unwrap_or_default(),
                overrides: stored
                    .project_grouping_overrides
                    .iter()
                    .filter_map(|(key, value)| {
                        Some((key.clone(), ProjectGroupingMode::parse(value)?))
                    })
                    .collect(),
            },
            project_sort_order: stored
                .project_sort_order
                .as_deref()
                .and_then(ProjectSortOrder::parse)
                .unwrap_or_default(),
            thread_sort_order: stored
                .thread_sort_order
                .as_deref()
                .and_then(ThreadSortOrder::parse)
                .unwrap_or_default(),
            thread_preview_count: stored
                .thread_preview_count
                .map(|count| clamp_preview_count(count as usize))
                .unwrap_or(DEFAULT_THREAD_PREVIEW_COUNT),
            project_expanded: stored.project_expanded,
            thread_lists_expanded: stored.thread_lists_expanded.into_iter().collect(),
            project_order: stored.project_order,
            thread_last_visited: stored.thread_last_visited,
            path,
        }
    }

    fn persist(&self) {
        let stored = StoredPrefs {
            project_grouping_mode: Some(self.grouping.mode.as_str().to_owned()),
            project_grouping_overrides: self
                .grouping
                .overrides
                .iter()
                .map(|(key, mode)| (key.clone(), mode.as_str().to_owned()))
                .collect(),
            project_sort_order: Some(self.project_sort_order.as_str().to_owned()),
            thread_sort_order: Some(self.thread_sort_order.as_str().to_owned()),
            thread_preview_count: Some(self.thread_preview_count as u32),
            project_expanded: self.project_expanded.clone(),
            thread_lists_expanded: self.thread_lists_expanded.iter().cloned().collect(),
            project_order: self.project_order.clone(),
            thread_last_visited: self.thread_last_visited.clone(),
        };
        let write = serde_json::to_string_pretty(&stored)
            .map_err(|error| error.to_string())
            .and_then(|contents| {
                if let Some(parent) = self.path.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                std::fs::write(&self.path, contents).map_err(|error| error.to_string())
            });
        if let Err(error) = write {
            eprintln!("[vitre] failed to persist {FILE_NAME}: {error}");
        }
    }

    /// `resolveProjectExpanded`: the first preference key with a recorded
    /// decision wins; rows with none are expanded.
    pub fn project_expanded(&self, preference_keys: &[String]) -> bool {
        preference_keys
            .iter()
            .find_map(|key| self.project_expanded.get(key).copied())
            .unwrap_or(true)
    }

    /// Record an expand/collapse decision against every key the row answers
    /// to, so it survives a grouping-mode change.
    pub fn set_project_expanded(&mut self, preference_keys: &[String], expanded: bool) {
        for key in preference_keys {
            self.project_expanded.insert(key.clone(), expanded);
        }
        self.persist();
    }

    pub fn thread_list_expanded(&self, project_key: &str) -> bool {
        self.thread_lists_expanded.contains(project_key)
    }

    pub fn set_thread_list_expanded(&mut self, project_key: &str, expanded: bool) {
        if expanded {
            self.thread_lists_expanded.insert(project_key.to_owned());
        } else {
            self.thread_lists_expanded.remove(project_key);
        }
        self.persist();
    }

    pub fn set_project_grouping_mode(&mut self, mode: ProjectGroupingMode) {
        self.grouping.mode = mode;
        self.persist();
    }

    /// `None` clears the override so the row inherits the global mode.
    pub fn set_project_grouping_override(
        &mut self,
        physical_key: &str,
        mode: Option<ProjectGroupingMode>,
    ) {
        match mode {
            Some(mode) => {
                self.grouping
                    .overrides
                    .insert(physical_key.to_owned(), mode);
            }
            None => {
                self.grouping.overrides.remove(physical_key);
            }
        }
        self.persist();
    }

    pub fn project_grouping_override(&self, physical_key: &str) -> Option<ProjectGroupingMode> {
        self.grouping.overrides.get(physical_key).copied()
    }

    pub fn set_project_sort_order(&mut self, order: ProjectSortOrder) {
        self.project_sort_order = order;
        self.persist();
    }

    pub fn set_thread_sort_order(&mut self, order: ThreadSortOrder) {
        self.thread_sort_order = order;
        self.persist();
    }

    pub fn set_thread_preview_count(&mut self, count: usize) {
        self.thread_preview_count = clamp_preview_count(count);
        self.persist();
    }

    /// `orderItemsByPreferredIds`: projects the persisted order names come
    /// first, in that order; everything else follows in its incoming order.
    /// Returns indices into `physical_keys`.
    pub fn order_projects(&self, physical_keys: &[String]) -> Vec<usize> {
        if self.project_order.is_empty() {
            return (0..physical_keys.len()).collect();
        }
        let mut ordered = Vec::with_capacity(physical_keys.len());
        let mut emitted = vec![false; physical_keys.len()];
        for preferred in &self.project_order {
            let Some(index) = physical_keys
                .iter()
                .enumerate()
                .position(|(index, key)| key == preferred && !emitted[index])
            else {
                continue;
            };
            emitted[index] = true;
            ordered.push(index);
        }
        ordered.extend((0..physical_keys.len()).filter(|index| !emitted[*index]));
        ordered
    }

    /// `reorderProjects`: move the dragged row's physical keys, as a block, to
    /// the target row's position.
    ///
    /// A sidebar row can stand for several physical projects (one per worktree
    /// of a grouped repository), so both ends are key *sets*, and the whole
    /// block lands where the target's first key was. `current` is the
    /// on-screen order, which is what seeds the stored order on the first
    /// drag. Returns whether anything moved.
    pub fn reorder_projects(
        &mut self,
        current: &[String],
        dragged: &[String],
        target: &[String],
    ) -> bool {
        if dragged.is_empty() || dragged.iter().all(|key| target.contains(key)) {
            return false;
        }
        let Some(target_index) = current.iter().position(|key| target.contains(key)) else {
            return false;
        };

        // Lifting the dragged keys out shifts everything after them left, so
        // the insert point backs off by however many were already in front of
        // the target — minus one, because the block itself reoccupies a slot.
        let mut order: Vec<String> = Vec::with_capacity(current.len());
        let mut removed: Vec<String> = Vec::new();
        let mut dragged_before_target = 0usize;
        for (index, key) in current.iter().enumerate() {
            if dragged.contains(key) {
                removed.push(key.clone());
                if index < target_index {
                    dragged_before_target += 1;
                }
            } else {
                order.push(key.clone());
            }
        }
        if removed.is_empty() {
            return false;
        }
        let insert_index = target_index - dragged_before_target.saturating_sub(1);
        let insert_index = insert_index.min(order.len());
        order.splice(insert_index..insert_index, removed);
        self.project_order = order;
        self.persist();
        true
    }

    pub fn thread_last_visited(&self, thread_id: &str) -> Option<&str> {
        self.thread_last_visited.get(thread_id).map(String::as_str)
    }

    /// `markThreadVisited`: the stamp only ever moves forward, and an
    /// unparseable timestamp is ignored. Returns whether anything changed, so
    /// callers can skip the re-render (and the write) on a repeat visit.
    pub fn mark_thread_visited(&mut self, thread_id: &str, at: &str) -> bool {
        let Some(visited_ms) = vitre_state::sidebar::parse_timestamp_ms(at) else {
            return false;
        };
        let previous_ms = self
            .thread_last_visited
            .get(thread_id)
            .and_then(|previous| vitre_state::sidebar::parse_timestamp_ms(previous));
        if previous_ms.is_some_and(|previous| previous >= visited_ms) {
            return false;
        }
        self.thread_last_visited
            .insert(thread_id.to_owned(), at.to_owned());
        self.persist();
        true
    }
}

/// `clampSidebarThreadPreviewCount`.
pub fn clamp_preview_count(count: usize) -> usize {
    count.clamp(MIN_THREAD_PREVIEW_COUNT, MAX_THREAD_PREVIEW_COUNT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "vitre-sidebar-prefs-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock is after the epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).expect("temp dir is creatable");
        base
    }

    #[test]
    fn defaults_match_the_client_settings_defaults() {
        let prefs = SidebarPrefs::load(&temp_home());
        assert_eq!(prefs.grouping.mode, ProjectGroupingMode::Repository);
        assert_eq!(prefs.project_sort_order, ProjectSortOrder::UpdatedAt);
        assert_eq!(prefs.thread_sort_order, ThreadSortOrder::UpdatedAt);
        assert_eq!(prefs.thread_preview_count, 6);
        // An untouched row is expanded.
        assert!(prefs.project_expanded(&["repo".to_owned()]));
    }

    #[test]
    fn preferences_round_trip_through_the_file() {
        let home = temp_home();
        {
            let mut prefs = SidebarPrefs::load(&home);
            prefs.set_project_grouping_mode(ProjectGroupingMode::Separate);
            prefs.set_project_grouping_override(
                "local:/repo",
                Some(ProjectGroupingMode::Repository),
            );
            prefs.set_project_sort_order(ProjectSortOrder::Manual);
            prefs.set_thread_sort_order(ThreadSortOrder::CreatedAt);
            prefs.set_thread_preview_count(99);
            prefs.set_project_expanded(&["repo".to_owned(), "local:/repo".to_owned()], false);
            prefs.set_thread_list_expanded("repo", true);
            prefs.mark_thread_visited("thread-1", "2026-09-02T00:00:00.000Z");
        }

        let prefs = SidebarPrefs::load(&home);
        assert_eq!(prefs.grouping.mode, ProjectGroupingMode::Separate);
        assert_eq!(
            prefs.project_grouping_override("local:/repo"),
            Some(ProjectGroupingMode::Repository)
        );
        assert_eq!(prefs.project_sort_order, ProjectSortOrder::Manual);
        assert_eq!(prefs.thread_sort_order, ThreadSortOrder::CreatedAt);
        // Out-of-range counts clamp rather than round-trip verbatim.
        assert_eq!(prefs.thread_preview_count, 15);
        assert!(!prefs.project_expanded(&["local:/repo".to_owned()]));
        assert!(prefs.thread_list_expanded("repo"));
        assert_eq!(
            prefs.thread_last_visited("thread-1"),
            Some("2026-09-02T00:00:00.000Z")
        );
    }

    #[test]
    fn manual_reorder_seeds_from_the_on_screen_order() {
        let home = temp_home();
        let mut prefs = SidebarPrefs::load(&home);
        let all = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        // No recorded order: the on-screen order is the order.
        assert_eq!(prefs.order_projects(&all), vec![0, 1, 2]);

        assert!(prefs.reorder_projects(&all, &["c".to_owned()], &["a".to_owned()]));
        assert_eq!(prefs.order_projects(&all), vec![2, 0, 1]);

        // A project the recorded order has never seen lands after the ones it
        // has, in its incoming order.
        let wider = vec![
            "a".to_owned(),
            "b".to_owned(),
            "c".to_owned(),
            "d".to_owned(),
        ];
        assert_eq!(prefs.order_projects(&wider), vec![2, 0, 1, 3]);
    }

    #[test]
    fn a_grouped_row_reorders_as_one_block() {
        let home = temp_home();
        let mut prefs = SidebarPrefs::load(&home);
        // `b` and `d` are two worktrees of one repository, so they share a row
        // and travel together. Dropped on `c`'s row they land as a block where
        // `c` sat — and because one of them was already ahead of `c`, the
        // block settles just after it.
        let all: Vec<String> = ["a", "b", "c", "d"]
            .iter()
            .map(|k| (*k).to_owned())
            .collect();
        assert!(prefs.reorder_projects(&all, &["b".to_owned(), "d".to_owned()], &["c".to_owned()]));
        assert_eq!(
            prefs.order_projects(&all),
            // a, c, b, d
            vec![0, 2, 1, 3]
        );

        // Dropping a row on itself is not a move.
        let all: Vec<String> = ["a", "b"].iter().map(|k| (*k).to_owned()).collect();
        assert!(!prefs.reorder_projects(&all, &["a".to_owned()], &["a".to_owned()]));
        // Neither is dropping on a row that is no longer on screen.
        assert!(!prefs.reorder_projects(&all, &["a".to_owned()], &["gone".to_owned()]));
    }

    #[test]
    fn visit_stamps_only_move_forward() {
        let home = temp_home();
        let mut prefs = SidebarPrefs::load(&home);
        assert!(prefs.mark_thread_visited("thread-1", "2026-09-02T00:00:00.000Z"));
        // An older (or repeated) stamp is a no-op, so a stale shell snapshot
        // cannot un-see a thread.
        assert!(!prefs.mark_thread_visited("thread-1", "2026-09-01T00:00:00.000Z"));
        assert!(!prefs.mark_thread_visited("thread-1", "2026-09-02T00:00:00.000Z"));
        assert!(!prefs.mark_thread_visited("thread-1", "not-a-timestamp"));
        assert!(prefs.mark_thread_visited("thread-1", "2026-09-03T00:00:00.000Z"));
        assert_eq!(
            prefs.thread_last_visited("thread-1"),
            Some("2026-09-03T00:00:00.000Z")
        );
    }
}
