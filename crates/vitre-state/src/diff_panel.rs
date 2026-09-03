//! Diff-panel selection state and pure helpers, ported from Electron's
//! `diffPanelStore.ts`, `useTurnDiffSummaries.ts` and `baseRefChoices.ts`.
//!
//! The store keeps, per thread key (`${environmentId}:${threadId}`), which
//! diff the panel shows: the working tree, the branch range against a base
//! ref, or one turn's checkpoint diff. A second map remembers the last
//! explicitly chosen branch base ref so flipping to the working tree and back
//! does not lose it. Rendering, queries and toggles live in the app layer.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vitre_contracts::{OrchestrationCheckpointSummary, TurnId, VcsRef};

/// Electron's persisted store version (`t3code:diff-panel-state:v1`).
pub const DIFF_PANEL_PERSISTED_VERSION: u64 = 1;

/// What the panel is pointed at for one thread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum DiffPanelSelection {
    /// Branch changes against `base_ref`; `None` = Automatic (server picks).
    Branch { base_ref: Option<String> },
    /// Working-tree changes.
    Unstaged,
    /// One turn's checkpoint diff. `file_path` + `reveal_request_id` carry a
    /// deep link from chat ("open this file's diff"); the panel scrolls to
    /// the file at most once per request id.
    Turn {
        turn_id: TurnId,
        file_path: Option<String>,
        reveal_request_id: u64,
    },
}

/// The two git scopes the scope dropdown flips between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitScope {
    Branch,
    Unstaged,
}

/// Per-thread diff-panel selections plus the branch-base-ref memory.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiffPanelMap {
    by_thread_key: BTreeMap<String, DiffPanelSelection>,
    branch_base_ref_by_thread_key: BTreeMap<String, Option<String>>,
}

/// The persisted shape (Electron's partialized zustand state).
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StoredDiffPanel {
    by_thread_key: BTreeMap<String, serde_json::Value>,
    branch_base_ref_by_thread_key: BTreeMap<String, Option<String>>,
}

impl DiffPanelMap {
    /// The stored selection, or Electron's two-layered default: `unstaged`
    /// when the thread has working-tree changes, else automatic branch mode.
    pub fn selection(&self, key: &str, has_working_tree_changes: bool) -> DiffPanelSelection {
        if let Some(selection) = self.by_thread_key.get(key) {
            return selection.clone();
        }
        if has_working_tree_changes {
            DiffPanelSelection::Unstaged
        } else {
            DiffPanelSelection::Branch { base_ref: None }
        }
    }

    /// `selectGitScope`: switching to branch restores the remembered base
    /// ref; leaving branch writes the current base ref into the memory map.
    pub fn select_git_scope(&mut self, key: &str, scope: GitScope) {
        let previous_branch_base_ref = match self.by_thread_key.get(key) {
            Some(DiffPanelSelection::Branch { base_ref }) => {
                let remembered = base_ref.clone();
                self.branch_base_ref_by_thread_key
                    .insert(key.to_string(), remembered.clone());
                remembered
            }
            _ => self
                .branch_base_ref_by_thread_key
                .get(key)
                .cloned()
                .unwrap_or(None),
        };
        let next = match scope {
            GitScope::Branch => DiffPanelSelection::Branch {
                base_ref: previous_branch_base_ref,
            },
            GitScope::Unstaged => DiffPanelSelection::Unstaged,
        };
        self.by_thread_key.insert(key.to_string(), next);
    }

    /// `selectBranchBaseRef`: trims, empty → Automatic; writes both maps.
    pub fn select_branch_base_ref(&mut self, key: &str, base_ref: Option<&str>) {
        let normalized = base_ref
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        self.by_thread_key.insert(
            key.to_string(),
            DiffPanelSelection::Branch {
                base_ref: normalized.clone(),
            },
        );
        self.branch_base_ref_by_thread_key
            .insert(key.to_string(), normalized);
    }

    /// `selectTurn`: the reveal request id increments only when the previous
    /// selection was already a turn (else it resets to 1), so "open file X in
    /// this turn's diff" scrolls once per request.
    pub fn select_turn(&mut self, key: &str, turn_id: TurnId, file_path: Option<&str>) {
        let reveal_request_id = match self.by_thread_key.get(key) {
            Some(DiffPanelSelection::Turn {
                reveal_request_id, ..
            }) => reveal_request_id + 1,
            _ => 1,
        };
        let file_path = file_path
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_string);
        self.by_thread_key.insert(
            key.to_string(),
            DiffPanelSelection::Turn {
                turn_id,
                file_path,
                reveal_request_id,
            },
        );
    }

    /// `reconcileTurnSelection`: heal a persisted turn that no longer exists
    /// to the first (latest) available turn. Returns true when changed.
    pub fn reconcile_turn_selection(&mut self, key: &str, available: &[TurnId]) -> bool {
        let Some(DiffPanelSelection::Turn { turn_id, .. }) = self.by_thread_key.get(key) else {
            return false;
        };
        if available.is_empty() || available.contains(turn_id) {
            return false;
        }
        let healed = available[0].clone();
        if let Some(DiffPanelSelection::Turn { turn_id, .. }) = self.by_thread_key.get_mut(key) {
            *turn_id = healed;
            return true;
        }
        false
    }

    pub fn remove_thread(&mut self, key: &str) {
        self.by_thread_key.remove(key);
        self.branch_base_ref_by_thread_key.remove(key);
    }

    pub fn from_persisted(value: &serde_json::Value) -> Self {
        let stored: StoredDiffPanel = serde_json::from_value(value.clone()).unwrap_or_default();
        let mut map = Self {
            by_thread_key: BTreeMap::new(),
            branch_base_ref_by_thread_key: stored.branch_base_ref_by_thread_key,
        };
        for (key, raw) in stored.by_thread_key {
            // Unknown selection kinds from a newer build are dropped; the
            // thread falls back to the default selection.
            if let Ok(selection) = serde_json::from_value::<DiffPanelSelection>(raw) {
                map.by_thread_key.insert(key, selection);
            }
        }
        map
    }

    pub fn to_persisted(&self) -> serde_json::Value {
        serde_json::json!({
            "byThreadKey": self.by_thread_key,
            "branchBaseRefByThreadKey": self.branch_base_ref_by_thread_key,
        })
    }
}

/// One turn the scope dropdown can offer, derived from thread checkpoints.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnDiffSummary {
    pub turn_id: TurnId,
    pub turn_count: i64,
    pub completed_at: String,
}

/// Electron's `orderedTurnDiffSummaries`: descending by turn count, ties
/// broken newest-`completedAt` first. (Electron also infers missing turn
/// counts from completion order; on this wire `checkpointTurnCount` is a
/// required field, so the inference path cannot occur.)
pub fn ordered_turn_diff_summaries(
    checkpoints: &[OrchestrationCheckpointSummary],
) -> Vec<TurnDiffSummary> {
    let mut summaries: Vec<TurnDiffSummary> = checkpoints
        .iter()
        .map(|checkpoint| TurnDiffSummary {
            turn_id: checkpoint.turn_id.clone(),
            turn_count: checkpoint.checkpoint_turn_count.0,
            completed_at: checkpoint.completed_at.0.clone(),
        })
        .collect();
    summaries.sort_by(|a, b| {
        b.turn_count
            .cmp(&a.turn_count)
            .then_with(|| b.completed_at.cmp(&a.completed_at))
    });
    summaries
}

/// One row of the base-ref combobox: a local branch, its matching remote
/// branch, or both.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseRefChoice {
    pub id: String,
    pub label: String,
    pub local: Option<VcsRef>,
    pub remote: Option<VcsRef>,
}

fn remote_branch_name(remote: &VcsRef) -> &str {
    let name = remote.name.0.as_str();
    if let Some(Some(remote_name)) = &remote.remote_name
        && let Some(stripped) = name.strip_prefix(&format!("{}/", remote_name.0))
    {
        return stripped;
    }
    name
}

fn is_origin(remote: &VcsRef) -> bool {
    matches!(&remote.remote_name, Some(Some(name)) if name.0 == "origin")
}

/// Electron's `buildBaseRefChoices`: pair each local ref with at most one
/// remote ref of the same branch name (preferring `origin`), then append the
/// unmatched remotes as remote-only choices. Callers filter the current head
/// ref out of `local_refs` first — you can't diff a branch against itself.
pub fn build_base_ref_choices(local_refs: &[VcsRef], remote_refs: &[VcsRef]) -> Vec<BaseRefChoice> {
    let mut used = vec![false; remote_refs.len()];
    let mut choices: Vec<BaseRefChoice> = local_refs
        .iter()
        .map(|local| {
            let mut matched: Option<usize> = None;
            for (index, remote) in remote_refs.iter().enumerate() {
                if used[index] || remote_branch_name(remote) != local.name.0 {
                    continue;
                }
                match matched {
                    None => matched = Some(index),
                    Some(current) if !is_origin(&remote_refs[current]) && is_origin(remote) => {
                        matched = Some(index)
                    }
                    _ => {}
                }
            }
            if let Some(index) = matched {
                used[index] = true;
            }
            BaseRefChoice {
                id: format!("local:{}", local.name.0),
                label: local.name.0.clone(),
                local: Some(local.clone()),
                remote: matched.map(|index| remote_refs[index].clone()),
            }
        })
        .collect();
    for (index, remote) in remote_refs.iter().enumerate() {
        if !used[index] {
            choices.push(BaseRefChoice {
                id: format!("remote:{}", remote.name.0),
                label: remote.name.0.clone(),
                local: None,
                remote: Some(remote.clone()),
            });
        }
    }
    choices
}

/// Electron's `filterBaseRefChoices`: case-insensitive substring match on the
/// label or either ref name; empty query keeps everything.
pub fn filter_base_ref_choices<'a>(
    choices: &'a [BaseRefChoice],
    query: &str,
) -> Vec<&'a BaseRefChoice> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return choices.iter().collect();
    }
    choices
        .iter()
        .filter(|choice| {
            let mut haystacks = vec![choice.label.as_str()];
            if let Some(local) = &choice.local {
                haystacks.push(local.name.0.as_str());
            }
            if let Some(remote) = &choice.remote {
                haystacks.push(remote.name.0.as_str());
            }
            haystacks
                .iter()
                .any(|value| value.to_lowercase().contains(&needle))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use vitre_contracts::{
        CheckpointRef, NonNegativeInt, OrchestrationCheckpointStatus, TrimmedNonEmptyString,
    };

    use super::*;

    fn tnes(value: &str) -> TrimmedNonEmptyString {
        TrimmedNonEmptyString(value.to_string())
    }

    fn checkpoint(turn_id: &str, count: i64, completed_at: &str) -> OrchestrationCheckpointSummary {
        OrchestrationCheckpointSummary {
            assistant_message_id: None,
            checkpoint_ref: CheckpointRef(format!("ref-{turn_id}")),
            checkpoint_turn_count: NonNegativeInt(count),
            completed_at: tnes(completed_at),
            files: vec![],
            status: OrchestrationCheckpointStatus::Ready,
            turn_id: TurnId(turn_id.to_string()),
        }
    }

    fn vcs_ref(name: &str, remote: Option<&str>) -> VcsRef {
        VcsRef {
            current: false,
            is_default: false,
            is_remote: Some(Some(remote.is_some())),
            name: tnes(name),
            remote_name: remote.map(|remote| Some(tnes(remote))),
            worktree_path: None,
        }
    }

    #[test]
    fn branch_base_ref_survives_scope_flips() {
        let mut map = DiffPanelMap::default();
        map.select_branch_base_ref("k", Some("  main  "));
        assert_eq!(
            map.selection("k", false),
            DiffPanelSelection::Branch {
                base_ref: Some("main".into())
            }
        );
        map.select_git_scope("k", GitScope::Unstaged);
        assert_eq!(map.selection("k", false), DiffPanelSelection::Unstaged);
        map.select_git_scope("k", GitScope::Branch);
        assert_eq!(
            map.selection("k", false),
            DiffPanelSelection::Branch {
                base_ref: Some("main".into())
            }
        );
    }

    #[test]
    fn empty_base_ref_normalizes_to_automatic() {
        let mut map = DiffPanelMap::default();
        map.select_branch_base_ref("k", Some("   "));
        assert_eq!(
            map.selection("k", false),
            DiffPanelSelection::Branch { base_ref: None }
        );
    }

    #[test]
    fn default_selection_follows_working_tree_changes() {
        let map = DiffPanelMap::default();
        assert_eq!(map.selection("k", true), DiffPanelSelection::Unstaged);
        assert_eq!(
            map.selection("k", false),
            DiffPanelSelection::Branch { base_ref: None }
        );
    }

    #[test]
    fn reveal_request_id_increments_only_within_turn_mode() {
        let mut map = DiffPanelMap::default();
        map.select_turn("k", TurnId("t1".into()), Some("a.rs"));
        map.select_turn("k", TurnId("t2".into()), None);
        match map.selection("k", false) {
            DiffPanelSelection::Turn {
                reveal_request_id,
                file_path,
                ..
            } => {
                assert_eq!(reveal_request_id, 2);
                assert_eq!(file_path, None);
            }
            other => panic!("unexpected: {other:?}"),
        }
        map.select_git_scope("k", GitScope::Unstaged);
        map.select_turn("k", TurnId("t3".into()), None);
        match map.selection("k", false) {
            DiffPanelSelection::Turn {
                reveal_request_id, ..
            } => assert_eq!(reveal_request_id, 1),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn reconcile_rewrites_vanished_turns_only() {
        let mut map = DiffPanelMap::default();
        map.select_turn("k", TurnId("gone".into()), Some("keep.rs"));
        let available = vec![TurnId("t9".into()), TurnId("t8".into())];
        assert!(map.reconcile_turn_selection("k", &available));
        match map.selection("k", false) {
            DiffPanelSelection::Turn {
                turn_id, file_path, ..
            } => {
                assert_eq!(turn_id.0, "t9");
                assert_eq!(file_path.as_deref(), Some("keep.rs"));
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(!map.reconcile_turn_selection("k", &available));
        assert!(!map.reconcile_turn_selection("k", &[]));
    }

    #[test]
    fn persistence_round_trips() {
        let mut map = DiffPanelMap::default();
        map.select_branch_base_ref("a", Some("develop"));
        map.select_turn("b", TurnId("t1".into()), Some("src/x.rs"));
        let restored = DiffPanelMap::from_persisted(&map.to_persisted());
        assert_eq!(restored, map);
    }

    #[test]
    fn turn_summaries_order_descending_with_completed_at_ties() {
        let ordered = ordered_turn_diff_summaries(&[
            checkpoint("t1", 1, "2026-01-01T00:00:00Z"),
            checkpoint("t3a", 3, "2026-01-03T00:00:00Z"),
            checkpoint("t3b", 3, "2026-01-04T00:00:00Z"),
            checkpoint("t2", 2, "2026-01-02T00:00:00Z"),
        ]);
        let ids: Vec<&str> = ordered.iter().map(|s| s.turn_id.0.as_str()).collect();
        assert_eq!(ids, ["t3b", "t3a", "t2", "t1"]);
    }

    #[test]
    fn base_ref_choices_pair_locals_with_origin_remotes_first() {
        let locals = vec![vcs_ref("main", None), vcs_ref("feature", None)];
        let remotes = vec![
            vcs_ref("upstream/main", Some("upstream")),
            vcs_ref("origin/main", Some("origin")),
            vcs_ref("origin/other", Some("origin")),
        ];
        let choices = build_base_ref_choices(&locals, &remotes);
        assert_eq!(choices.len(), 4);
        assert_eq!(choices[0].id, "local:main");
        assert_eq!(
            choices[0].remote.as_ref().map(|r| r.name.0.as_str()),
            Some("origin/main")
        );
        assert_eq!(choices[1].id, "local:feature");
        assert!(choices[1].remote.is_none());
        // Unmatched remotes trail, each consumed at most once.
        assert_eq!(choices[2].id, "remote:upstream/main");
        assert_eq!(choices[3].id, "remote:origin/other");
    }

    #[test]
    fn base_ref_filter_matches_any_name_case_insensitively() {
        let choices = build_base_ref_choices(
            &[vcs_ref("Main", None)],
            &[vcs_ref("origin/fixup", Some("origin"))],
        );
        assert_eq!(filter_base_ref_choices(&choices, "  ").len(), 2);
        assert_eq!(filter_base_ref_choices(&choices, "main").len(), 1);
        assert_eq!(filter_base_ref_choices(&choices, "FIX").len(), 1);
        assert_eq!(filter_base_ref_choices(&choices, "zzz").len(), 0);
    }
}
