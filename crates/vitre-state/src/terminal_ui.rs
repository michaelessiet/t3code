//! Per-thread terminal drawer UI state: a pure port of
//! `apps/web/src/terminalUiStateStore.ts` (zustand store, storage key
//! `t3code:terminal-state:v1`, version 4).
//!
//! Thread keys are Electron's scoped `${environmentId}:${threadId}`. Entries
//! that normalize to the default state are deleted rather than stored, and
//! the suppression map (locally closed ids hidden from stale server metadata)
//! is session-only — it never persists.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::right_panel::{MAX_TERMINALS_PER_GROUP, SplitDirection};

/// Electron persisted-store version.
pub const TERMINAL_UI_PERSISTED_VERSION: u64 = 4;

pub const DEFAULT_THREAD_TERMINAL_ID: &str = "term-1";
pub const DEFAULT_THREAD_TERMINAL_HEIGHT: f64 = 280.;
pub const MIN_DRAWER_HEIGHT: f64 = 180.;
pub const MAX_DRAWER_HEIGHT_RATIO: f64 = 0.75;

/// Clamp a drawer height: `min(max(round(h), 180), max(180, floor(75% of the
/// window)))`; non-finite input falls back to the default height.
pub fn clamp_drawer_height(height: f64, window_height: f64) -> f64 {
    if !height.is_finite() {
        return DEFAULT_THREAD_TERMINAL_HEIGHT;
    }
    let max = (window_height * MAX_DRAWER_HEIGHT_RATIO)
        .floor()
        .max(MIN_DRAWER_HEIGHT);
    height.round().max(MIN_DRAWER_HEIGHT).min(max)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalGroup {
    pub id: String,
    pub terminal_ids: Vec<String>,
    /// Persisted only when `"vertical"` — Electron deletes the property for
    /// the horizontal default.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub split_direction: Option<SplitDirection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThreadTerminalUiState {
    pub terminal_open: bool,
    pub terminal_height: f64,
    pub terminal_ids: Vec<String>,
    pub active_terminal_id: String,
    pub terminal_groups: Vec<TerminalGroup>,
    pub active_terminal_group_id: String,
}

impl Default for ThreadTerminalUiState {
    fn default() -> Self {
        Self {
            terminal_open: false,
            terminal_height: DEFAULT_THREAD_TERMINAL_HEIGHT,
            terminal_ids: Vec::new(),
            active_terminal_id: String::new(),
            terminal_groups: Vec::new(),
            active_terminal_group_id: String::new(),
        }
    }
}

impl ThreadTerminalUiState {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// `normalizeThreadTerminalUiState`, applied before/after every transition.
pub fn normalize(state: &ThreadTerminalUiState) -> ThreadTerminalUiState {
    // Ids: trimmed, de-duped, empties dropped.
    let mut ids: Vec<String> = Vec::new();
    for id in &state.terminal_ids {
        let id = id.trim();
        if !id.is_empty() && !ids.iter().any(|seen| seen == id) {
            ids.push(id.to_string());
        }
    }

    let active_terminal_id = if ids.contains(&state.active_terminal_id) {
        state.active_terminal_id.clone()
    } else {
        ids.first().cloned().unwrap_or_default()
    };

    // Groups: keep only valid, not-yet-assigned ids; drop emptied groups;
    // fall back and uniquify group ids; then one singleton group per
    // unassigned terminal.
    let mut assigned: Vec<String> = Vec::new();
    let mut group_ids: Vec<String> = Vec::new();
    let mut groups: Vec<TerminalGroup> = Vec::new();
    for group in &state.terminal_groups {
        let members: Vec<String> = group
            .terminal_ids
            .iter()
            .map(|id| id.trim().to_string())
            .filter(|id| ids.iter().any(|known| known == id) && !assigned.contains(id))
            .collect();
        if members.is_empty() {
            continue;
        }
        assigned.extend(members.iter().cloned());
        let base = {
            let trimmed = group.id.trim();
            if trimmed.is_empty() {
                format!("group-{}", members[0])
            } else {
                trimmed.to_string()
            }
        };
        let id = unique_group_id(base, &group_ids);
        group_ids.push(id.clone());
        groups.push(TerminalGroup {
            id,
            terminal_ids: members,
            split_direction: match group.split_direction {
                Some(SplitDirection::Vertical) => Some(SplitDirection::Vertical),
                _ => None,
            },
        });
    }
    for id in &ids {
        if assigned.contains(id) {
            continue;
        }
        let group_id = unique_group_id(format!("group-{id}"), &group_ids);
        group_ids.push(group_id.clone());
        groups.push(TerminalGroup {
            id: group_id,
            terminal_ids: vec![id.clone()],
            split_direction: None,
        });
    }

    let active_terminal_group_id = if groups
        .iter()
        .any(|g| g.id == state.active_terminal_group_id)
    {
        state.active_terminal_group_id.clone()
    } else {
        groups
            .iter()
            .find(|g| g.terminal_ids.contains(&active_terminal_id))
            .or(groups.first())
            .map(|g| g.id.clone())
            .unwrap_or_default()
    };

    ThreadTerminalUiState {
        terminal_open: state.terminal_open,
        terminal_height: if state.terminal_height.is_finite() && state.terminal_height > 0. {
            state.terminal_height
        } else {
            DEFAULT_THREAD_TERMINAL_HEIGHT
        },
        terminal_ids: ids,
        active_terminal_id,
        terminal_groups: groups,
        active_terminal_group_id,
    }
}

fn unique_group_id(base: String, taken: &[String]) -> String {
    if !taken.contains(&base) {
        return base;
    }
    for n in 2u64.. {
        let candidate = format!("{base}-{n}");
        if !taken.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!()
}

/// Drawer presentation order (`ThreadTerminalDrawer`'s defensive mirror):
/// groups sorted by the minimum index of their terminals in the id list.
pub fn sorted_groups_for_display(state: &ThreadTerminalUiState) -> Vec<TerminalGroup> {
    let index_of = |id: &str| {
        state
            .terminal_ids
            .iter()
            .position(|known| known == id)
            .unwrap_or(usize::MAX)
    };
    let mut groups = state.terminal_groups.clone();
    groups.sort_by_key(|group| {
        group
            .terminal_ids
            .iter()
            .map(|id| index_of(id))
            .min()
            .unwrap_or(usize::MAX)
    });
    groups
}

enum UpsertMode {
    Split { vertical: bool },
    New,
}

/// `upsertTerminalIntoGroups`. Returns the next state; a split that would
/// push a NEW terminal past [`MAX_TERMINALS_PER_GROUP`] rejects the whole op
/// (normalized input returned unchanged).
fn upsert_terminal(
    state: &ThreadTerminalUiState,
    terminal_id: &str,
    mode: UpsertMode,
) -> ThreadTerminalUiState {
    let mut state = normalize(state);
    // With no terminals at all, a split degrades to "new" (Electron coerces).
    let mode = if state.terminal_ids.is_empty() {
        UpsertMode::New
    } else {
        mode
    };
    let is_new = !state.terminal_ids.iter().any(|id| id == terminal_id);

    match mode {
        UpsertMode::New => {
            if is_new {
                state.terminal_ids.push(terminal_id.to_string());
            }
            for group in &mut state.terminal_groups {
                group.terminal_ids.retain(|id| id != terminal_id);
            }
            state.terminal_groups.retain(|g| !g.terminal_ids.is_empty());
            let group_id = unique_group_id(
                format!("group-{terminal_id}"),
                &state
                    .terminal_groups
                    .iter()
                    .map(|g| g.id.clone())
                    .collect::<Vec<_>>(),
            );
            state.terminal_groups.push(TerminalGroup {
                id: group_id.clone(),
                terminal_ids: vec![terminal_id.to_string()],
                split_direction: None,
            });
            state.active_terminal_group_id = group_id;
        }
        UpsertMode::Split { vertical } => {
            // Destination: the active group, else the group containing the
            // active terminal (normalize guarantees one exists when any
            // terminal does).
            let dest_index = state
                .terminal_groups
                .iter()
                .position(|g| g.id == state.active_terminal_group_id)
                .or_else(|| {
                    state
                        .terminal_groups
                        .iter()
                        .position(|g| g.terminal_ids.contains(&state.active_terminal_id))
                });
            let Some(dest_index) = dest_index else {
                return state;
            };
            if is_new
                && state.terminal_groups[dest_index].terminal_ids.len() >= MAX_TERMINALS_PER_GROUP
            {
                return state;
            }
            if is_new {
                state.terminal_ids.push(terminal_id.to_string());
            }
            // Re-find the destination by id: removing the id from other
            // groups below can drop emptied groups and shift indices.
            let dest_id = state.terminal_groups[dest_index].id.clone();
            for group in &mut state.terminal_groups {
                if group.id != dest_id {
                    group.terminal_ids.retain(|id| id != terminal_id);
                }
            }
            state
                .terminal_groups
                .retain(|g| !g.terminal_ids.is_empty() || g.id == dest_id);
            let active_id = state.active_terminal_id.clone();
            let group = state
                .terminal_groups
                .iter_mut()
                .find(|g| g.id == dest_id)
                .expect("destination group survives the prune");
            if !group.terminal_ids.iter().any(|id| id == terminal_id) {
                let insert_at = group
                    .terminal_ids
                    .iter()
                    .position(|id| *id == active_id)
                    .map(|index| index + 1)
                    .unwrap_or(group.terminal_ids.len());
                group
                    .terminal_ids
                    .insert(insert_at, terminal_id.to_string());
            }
            group.split_direction = vertical.then_some(SplitDirection::Vertical);
            state.active_terminal_group_id = dest_id;
        }
    }

    state.terminal_open = true;
    state.active_terminal_id = terminal_id.to_string();
    normalize(&state)
}

/// The whole store: per-thread UI states plus the session-only suppression
/// map.
#[derive(Debug, Default)]
pub struct TerminalUiMap {
    by_thread_key: HashMap<String, ThreadTerminalUiState>,
    suppressed: HashMap<String, Vec<String>>,
}

impl TerminalUiMap {
    /// Normalized state for a thread (default when absent).
    pub fn thread(&self, thread_key: &str) -> ThreadTerminalUiState {
        self.by_thread_key
            .get(thread_key)
            .map(normalize)
            .unwrap_or_default()
    }

    fn put(&mut self, thread_key: &str, state: ThreadTerminalUiState) -> bool {
        let next = normalize(&state);
        let changed = self.thread(thread_key) != next;
        if next.is_default() {
            self.by_thread_key.remove(thread_key);
        } else {
            self.by_thread_key.insert(thread_key.to_string(), next);
        }
        changed
    }

    fn unsuppress(&mut self, thread_key: &str, terminal_id: &str) {
        if let Some(ids) = self.suppressed.get_mut(thread_key) {
            ids.retain(|id| id != terminal_id);
            if ids.is_empty() {
                self.suppressed.remove(thread_key);
            }
        }
    }

    /// `setTerminalOpen`: opening with zero terminals seeds `term-1`.
    pub fn set_terminal_open(&mut self, thread_key: &str, open: bool) -> bool {
        let state = self.thread(thread_key);
        if open && state.terminal_ids.is_empty() {
            return self.new_terminal(thread_key, DEFAULT_THREAD_TERMINAL_ID);
        }
        if state.terminal_open == open {
            return false;
        }
        let mut state = state;
        state.terminal_open = open;
        self.put(thread_key, state)
    }

    /// Ignores non-finite, non-positive and unchanged heights.
    pub fn set_terminal_height(&mut self, thread_key: &str, height: f64) -> bool {
        if !height.is_finite() || height <= 0. {
            return false;
        }
        let mut state = self.thread(thread_key);
        if state.terminal_height == height {
            return false;
        }
        state.terminal_height = height;
        self.put(thread_key, state)
    }

    pub fn new_terminal(&mut self, thread_key: &str, terminal_id: &str) -> bool {
        self.unsuppress(thread_key, terminal_id);
        let next = upsert_terminal(&self.thread(thread_key), terminal_id, UpsertMode::New);
        self.put(thread_key, next)
    }

    pub fn split_terminal(&mut self, thread_key: &str, terminal_id: &str, vertical: bool) -> bool {
        self.unsuppress(thread_key, terminal_id);
        let next = upsert_terminal(
            &self.thread(thread_key),
            terminal_id,
            UpsertMode::Split { vertical },
        );
        self.put(thread_key, next)
    }

    pub fn set_active_terminal(&mut self, thread_key: &str, terminal_id: &str) -> bool {
        let mut state = self.thread(thread_key);
        if !state.terminal_ids.iter().any(|id| id == terminal_id) {
            return false;
        }
        state.active_terminal_id = terminal_id.to_string();
        if let Some(group) = state
            .terminal_groups
            .iter()
            .find(|g| g.terminal_ids.iter().any(|id| id == terminal_id))
        {
            state.active_terminal_group_id = group.id.clone();
        }
        self.put(thread_key, state)
    }

    /// `closeTerminal`: removes the id, elects the next active tab by index,
    /// resets to default when it was the last one — and marks the id
    /// suppressed so stale metadata cannot resurrect it.
    pub fn close_terminal(&mut self, thread_key: &str, terminal_id: &str) -> bool {
        let state = self.thread(thread_key);
        let Some(closed_index) = state.terminal_ids.iter().position(|id| id == terminal_id) else {
            return false;
        };
        let suppressed = self.suppressed.entry(thread_key.to_string()).or_default();
        if !suppressed.iter().any(|id| id == terminal_id) {
            suppressed.push(terminal_id.to_string());
        }
        let mut state = state;
        state.terminal_ids.remove(closed_index);
        if state.terminal_ids.is_empty() {
            return self.put(thread_key, ThreadTerminalUiState::default());
        }
        for group in &mut state.terminal_groups {
            group.terminal_ids.retain(|id| id != terminal_id);
        }
        state.terminal_groups.retain(|g| !g.terminal_ids.is_empty());
        if state.active_terminal_id == terminal_id {
            let next_index = closed_index.min(state.terminal_ids.len() - 1);
            state.active_terminal_id = state.terminal_ids[next_index].clone();
            state.active_terminal_group_id.clear();
        }
        self.put(thread_key, state)
    }

    /// `reconcileTerminalIds`: adopt the server's id set (suppressed ids
    /// filtered first), keeping order, active tab and surviving groups.
    pub fn reconcile_terminal_ids(&mut self, thread_key: &str, next_ids: &[String]) -> bool {
        let suppressed = self.suppressed.get(thread_key).cloned().unwrap_or_default();
        let next_ids: Vec<String> = next_ids
            .iter()
            .filter(|id| !suppressed.iter().any(|s| s == *id))
            .cloned()
            .collect();
        let state = self.thread(thread_key);
        if state.terminal_ids == next_ids {
            return false;
        }
        let mut state = state;
        state.terminal_ids = next_ids;
        self.put(thread_key, state)
    }

    /// Session-only view of the locally closed ids (for callers that
    /// pre-filter, like the drawer's server-order derivation).
    pub fn suppressed_ids(&self, thread_key: &str) -> Vec<String> {
        self.suppressed.get(thread_key).cloned().unwrap_or_default()
    }

    pub fn remove_thread(&mut self, thread_key: &str) -> bool {
        let removed = self.by_thread_key.remove(thread_key).is_some();
        self.suppressed.remove(thread_key);
        removed
    }

    /// Electron's persisted blob content: `terminalUiStateByThreadKey` only
    /// (suppression is deliberately not persisted), default entries deleted.
    pub fn to_persisted(&self) -> Value {
        let mut map = serde_json::Map::new();
        let mut keys: Vec<&String> = self.by_thread_key.keys().collect();
        keys.sort();
        for key in keys {
            let state = normalize(&self.by_thread_key[key]);
            if state.is_default() {
                continue;
            }
            if let Ok(value) = serde_json::to_value(&state) {
                map.insert(key.clone(), value);
            }
        }
        Value::Object(map)
    }

    /// Electron's migrate: accept `terminalUiStateByThreadKey` or the legacy
    /// `terminalStateByThreadKey`, dropping entries whose key is not a scoped
    /// `${environmentId}:${threadId}` and entries that fail to decode.
    pub fn from_persisted(stored: &Value) -> Self {
        let entries = stored
            .get("terminalUiStateByThreadKey")
            .or_else(|| stored.get("terminalStateByThreadKey"))
            .and_then(Value::as_object);
        let mut by_thread_key = HashMap::new();
        if let Some(entries) = entries {
            for (key, value) in entries {
                if !is_scoped_thread_key(key) {
                    continue;
                }
                let Ok(state) = serde_json::from_value::<ThreadTerminalUiState>(value.clone())
                else {
                    continue;
                };
                let state = normalize(&state);
                if !state.is_default() {
                    by_thread_key.insert(key.clone(), state);
                }
            }
        }
        Self {
            by_thread_key,
            suppressed: HashMap::new(),
        }
    }
}

/// `parseScopedThreadKey`: the first `:` splits environment from thread; both
/// halves must be non-empty.
fn is_scoped_thread_key(key: &str) -> bool {
    match key.split_once(':') {
        Some((environment, thread)) => !environment.is_empty() && !thread.is_empty(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const KEY: &str = "env:thread";

    #[test]
    fn open_with_no_terminals_seeds_term_1() {
        let mut map = TerminalUiMap::default();
        assert!(map.set_terminal_open(KEY, true));
        let state = map.thread(KEY);
        assert!(state.terminal_open);
        assert_eq!(state.terminal_ids, vec!["term-1"]);
        assert_eq!(state.active_terminal_id, "term-1");
        assert_eq!(state.terminal_groups.len(), 1);
        assert_eq!(state.active_terminal_group_id, state.terminal_groups[0].id);
    }

    #[test]
    fn split_inserts_after_active_and_caps_at_four() {
        let mut map = TerminalUiMap::default();
        map.new_terminal(KEY, "term-1");
        map.split_terminal(KEY, "term-2", false);
        map.split_terminal(KEY, "term-3", true);
        let state = map.thread(KEY);
        assert_eq!(state.terminal_groups.len(), 1);
        // term-3 splits while term-2 is active → lands right after it.
        assert_eq!(
            state.terminal_groups[0].terminal_ids,
            vec!["term-1", "term-2", "term-3"]
        );
        assert_eq!(
            state.terminal_groups[0].split_direction,
            Some(SplitDirection::Vertical)
        );
        map.split_terminal(KEY, "term-4", false);
        assert_eq!(
            map.thread(KEY).terminal_groups[0].split_direction,
            None,
            "a horizontal split clears the vertical flag"
        );
        // Fifth pane rejected: state unchanged.
        assert!(!map.split_terminal(KEY, "term-5", false));
        let state = map.thread(KEY);
        assert_eq!(state.terminal_ids.len(), 4);
        assert_eq!(state.active_terminal_id, "term-4");
    }

    #[test]
    fn new_terminal_leaves_the_split_and_starts_its_own_group() {
        let mut map = TerminalUiMap::default();
        map.new_terminal(KEY, "term-1");
        map.split_terminal(KEY, "term-2", false);
        map.new_terminal(KEY, "term-3");
        let state = map.thread(KEY);
        assert_eq!(state.terminal_groups.len(), 2);
        assert_eq!(state.terminal_groups[1].terminal_ids, vec!["term-3"]);
        assert_eq!(state.active_terminal_id, "term-3");
        assert_eq!(state.active_terminal_group_id, state.terminal_groups[1].id);
    }

    #[test]
    fn close_elects_the_neighbor_and_last_close_resets() {
        let mut map = TerminalUiMap::default();
        map.new_terminal(KEY, "term-1");
        map.new_terminal(KEY, "term-2");
        map.new_terminal(KEY, "term-3");
        map.set_active_terminal(KEY, "term-2");
        map.close_terminal(KEY, "term-2");
        let state = map.thread(KEY);
        assert_eq!(state.terminal_ids, vec!["term-1", "term-3"]);
        assert_eq!(
            state.active_terminal_id, "term-3",
            "closed index elects the id now at that index"
        );
        map.close_terminal(KEY, "term-3");
        assert_eq!(map.thread(KEY).active_terminal_id, "term-1");
        map.close_terminal(KEY, "term-1");
        assert!(map.thread(KEY).is_default(), "last close resets the entry");
        assert_eq!(
            map.suppressed_ids(KEY),
            vec!["term-2", "term-3", "term-1"],
            "closed ids stay suppressed"
        );
    }

    #[test]
    fn reconcile_filters_suppressed_and_keeps_groups() {
        let mut map = TerminalUiMap::default();
        map.new_terminal(KEY, "term-1");
        map.split_terminal(KEY, "term-2", false);
        map.close_terminal(KEY, "term-2");
        // Stale server metadata still lists term-2: reconcile must not
        // resurrect it.
        let ids: Vec<String> = vec!["term-1".into(), "term-2".into()];
        assert!(!map.reconcile_terminal_ids(KEY, &ids));
        assert_eq!(map.thread(KEY).terminal_ids, vec!["term-1"]);
        // A genuinely new server id lands as its own singleton group.
        let ids: Vec<String> = vec!["term-1".into(), "term-3".into()];
        assert!(map.reconcile_terminal_ids(KEY, &ids));
        let state = map.thread(KEY);
        assert_eq!(state.terminal_ids, vec!["term-1", "term-3"]);
        assert_eq!(state.terminal_groups.len(), 2);
        // Re-creating a suppressed id un-suppresses it.
        map.new_terminal(KEY, "term-2");
        assert!(!map.suppressed_ids(KEY).iter().any(|id| id == "term-2"));
    }

    #[test]
    fn normalize_dedups_uniquifies_and_appends_singletons() {
        let state = ThreadTerminalUiState {
            terminal_open: true,
            terminal_height: f64::NAN,
            terminal_ids: vec![
                " term-1 ".into(),
                "term-1".into(),
                String::new(),
                "term-2".into(),
                "term-3".into(),
            ],
            active_terminal_id: "gone".into(),
            terminal_groups: vec![
                TerminalGroup {
                    id: "  ".into(),
                    terminal_ids: vec!["term-1".into(), "missing".into()],
                    split_direction: None,
                },
                TerminalGroup {
                    id: "group-term-1".into(),
                    terminal_ids: vec!["term-2".into()],
                    split_direction: Some(SplitDirection::Vertical),
                },
            ],
            active_terminal_group_id: "nope".into(),
        };
        let normalized = normalize(&state);
        assert_eq!(normalized.terminal_ids, vec!["term-1", "term-2", "term-3"]);
        assert_eq!(normalized.active_terminal_id, "term-1");
        assert_eq!(normalized.terminal_height, DEFAULT_THREAD_TERMINAL_HEIGHT);
        let group_ids: Vec<&str> = normalized
            .terminal_groups
            .iter()
            .map(|g| g.id.as_str())
            .collect();
        // Blank id falls back to group-term-1; the literal group-term-1 that
        // follows is uniquified; term-3 gets its singleton appended.
        assert_eq!(
            group_ids,
            vec!["group-term-1", "group-term-1-2", "group-term-3"]
        );
        assert_eq!(
            normalized.active_terminal_group_id, "group-term-1",
            "falls back to the group containing the active terminal"
        );
    }

    #[test]
    fn persistence_round_trips_and_drops_bad_keys() {
        let mut map = TerminalUiMap::default();
        map.new_terminal(KEY, "term-1");
        map.split_terminal(KEY, "term-2", true);
        map.set_terminal_height(KEY, 320.);
        let persisted = map.to_persisted();
        let entry = &persisted[KEY];
        assert_eq!(entry["terminalHeight"], json!(320.0));
        assert_eq!(
            entry["terminalGroups"][0]["splitDirection"],
            json!("vertical")
        );

        let restored = TerminalUiMap::from_persisted(&json!({
            "terminalUiStateByThreadKey": persisted,
        }));
        assert_eq!(restored.thread(KEY), map.thread(KEY));

        // Legacy field name + junk keys/values.
        let legacy = TerminalUiMap::from_persisted(&json!({
            "terminalStateByThreadKey": {
                KEY: entry,
                "not-scoped": entry,
                ":missing-env": entry,
                "env:junk-value": 42,
            },
        }));
        assert_eq!(legacy.thread(KEY), map.thread(KEY));
        assert!(legacy.thread("not-scoped").is_default());
        assert!(legacy.thread("env:junk-value").is_default());
    }

    #[test]
    fn display_order_sorts_groups_by_first_terminal_index() {
        let state = normalize(&ThreadTerminalUiState {
            terminal_open: true,
            terminal_height: 280.,
            terminal_ids: vec!["term-1".into(), "term-2".into(), "term-3".into()],
            active_terminal_id: "term-1".into(),
            terminal_groups: vec![
                TerminalGroup {
                    id: "late".into(),
                    terminal_ids: vec!["term-3".into()],
                    split_direction: None,
                },
                TerminalGroup {
                    id: "early".into(),
                    terminal_ids: vec!["term-1".into(), "term-2".into()],
                    split_direction: None,
                },
            ],
            active_terminal_group_id: String::new(),
        });
        let display = sorted_groups_for_display(&state);
        assert_eq!(display[0].id, "early");
        assert_eq!(display[1].id, "late");
    }
}
