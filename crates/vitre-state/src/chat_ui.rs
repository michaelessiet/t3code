//! Persisted chat UI state: the changed-files card expansion map and the
//! per-project last-invoked script.
//!
//! Ports the relevant slices of Electron's `apps/web/src/uiStateStore.ts`
//! (localStorage key `t3code:ui-state:v1`): per-thread, per-turn expansion
//! choices for the changed-files card, plus the separate
//! `lastInvokedScriptByProjectId` localStorage record backing the scripts
//! control's preferred script. Vitre persists both as
//! `~/.vitre/ui-state.json`. The expansion map is only honored when
//! `threadChangedFilesExpansionVersion` matches; loading sanitizes shapes the
//! same way Electron does (drop non-object entries, non-boolean/non-string
//! values, and empty per-thread maps).

use std::collections::HashMap;

use serde_json::{Map, Value, json};

/// `THREAD_CHANGED_FILES_EXPANSION_VERSION`.
pub const CHANGED_FILES_EXPANSION_VERSION: u64 = 1;

/// Thread key (`"{environmentId}:{threadId}"`) → turn id → expanded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatUiState {
    changed_files_expanded: HashMap<String, HashMap<String, bool>>,
    last_invoked_script_by_project: HashMap<String, String>,
}

impl ChatUiState {
    pub fn from_persisted(value: &Value) -> Self {
        let mut state = Self::default();
        if let Some(by_project) = value
            .get("lastInvokedScriptByProjectId")
            .and_then(Value::as_object)
        {
            state.last_invoked_script_by_project = by_project
                .iter()
                .filter_map(|(project_id, script_id)| {
                    script_id
                        .as_str()
                        .map(|script_id| (project_id.clone(), script_id.to_string()))
                })
                .collect();
        }
        if value
            .get("threadChangedFilesExpansionVersion")
            .and_then(Value::as_u64)
            != Some(CHANGED_FILES_EXPANSION_VERSION)
        {
            return state;
        }
        let Some(by_thread) = value
            .get("threadChangedFilesExpandedById")
            .and_then(Value::as_object)
        else {
            return state;
        };
        for (thread_key, turns) in by_thread {
            let Some(turns) = turns.as_object() else {
                continue;
            };
            let cleaned: HashMap<String, bool> = turns
                .iter()
                .filter_map(|(turn_id, expanded)| {
                    expanded.as_bool().map(|value| (turn_id.clone(), value))
                })
                .collect();
            if !cleaned.is_empty() {
                state
                    .changed_files_expanded
                    .insert(thread_key.clone(), cleaned);
            }
        }
        state
    }

    pub fn to_persisted(&self) -> Value {
        let by_thread: Map<String, Value> = self
            .changed_files_expanded
            .iter()
            .filter(|(_, turns)| !turns.is_empty())
            .map(|(thread_key, turns)| {
                let turns: Map<String, Value> = turns
                    .iter()
                    .map(|(turn_id, expanded)| (turn_id.clone(), Value::Bool(*expanded)))
                    .collect();
                (thread_key.clone(), Value::Object(turns))
            })
            .collect();
        let by_project: Map<String, Value> = self
            .last_invoked_script_by_project
            .iter()
            .map(|(project_id, script_id)| (project_id.clone(), Value::String(script_id.clone())))
            .collect();
        json!({
            "threadChangedFilesExpansionVersion": CHANGED_FILES_EXPANSION_VERSION,
            "threadChangedFilesExpandedById": by_thread,
            "lastInvokedScriptByProjectId": by_project,
        })
    }

    /// The persisted user choice, if any — `None` falls back to auto-expand.
    pub fn changed_files_expanded(&self, thread_key: &str, turn_id: &str) -> Option<bool> {
        self.changed_files_expanded
            .get(thread_key)?
            .get(turn_id)
            .copied()
    }

    pub fn set_changed_files_expanded(&mut self, thread_key: &str, turn_id: &str, expanded: bool) {
        self.changed_files_expanded
            .entry(thread_key.to_string())
            .or_default()
            .insert(turn_id.to_string(), expanded);
    }

    /// `removeThreadUiState`.
    pub fn remove_thread(&mut self, thread_key: &str) {
        self.changed_files_expanded.remove(thread_key);
    }

    /// `lastInvokedScriptByProjectId[projectId]`.
    pub fn last_invoked_script(&self, project_id: &str) -> Option<&str> {
        self.last_invoked_script_by_project
            .get(project_id)
            .map(String::as_str)
    }

    /// Returns `false` when the value was already set (Electron skips the
    /// state update, so callers can skip the save).
    pub fn set_last_invoked_script(&mut self, project_id: &str, script_id: &str) -> bool {
        if self.last_invoked_script(project_id) == Some(script_id) {
            return false;
        }
        self.last_invoked_script_by_project
            .insert(project_id.to_string(), script_id.to_string());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_sanitizes() {
        let mut state = ChatUiState::default();
        state.set_changed_files_expanded("env:thread", "turn-1", true);
        state.set_changed_files_expanded("env:thread", "turn-2", false);
        let restored = ChatUiState::from_persisted(&state.to_persisted());
        assert_eq!(restored, state);
        assert_eq!(
            restored.changed_files_expanded("env:thread", "turn-1"),
            Some(true)
        );
        assert_eq!(
            restored.changed_files_expanded("env:thread", "missing"),
            None
        );
    }

    #[test]
    fn wrong_version_discards_the_whole_map() {
        let value = json!({
            "threadChangedFilesExpansionVersion": 2,
            "threadChangedFilesExpandedById": { "env:thread": { "turn": true } },
        });
        assert_eq!(ChatUiState::from_persisted(&value), ChatUiState::default());
    }

    #[test]
    fn malformed_entries_are_dropped() {
        let value = json!({
            "threadChangedFilesExpansionVersion": 1,
            "threadChangedFilesExpandedById": {
                "bad": 7,
                "empty": {},
                "mixed": { "turn": true, "junk": "yes" },
            },
        });
        let state = ChatUiState::from_persisted(&value);
        assert_eq!(state.changed_files_expanded("mixed", "turn"), Some(true));
        assert_eq!(state.changed_files_expanded("mixed", "junk"), None);
        assert_eq!(state.changed_files_expanded("bad", "turn"), None);
        assert_eq!(state.changed_files_expanded("empty", "turn"), None);
    }
}
