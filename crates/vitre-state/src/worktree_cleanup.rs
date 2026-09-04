//! Worktree cleanup on thread deletion.
//!
//! Ports `apps/web/src/worktreeCleanup.ts` and the fallback-selection half of
//! `Sidebar.logic.ts::getFallbackThreadIdAfterDelete`: deleting the last
//! thread linked to a worktree offers to remove the worktree too, and the
//! sidebar selection falls back to the deleted thread's project.

use vitre_contracts::{OrchestrationThreadShell, ThreadId};

use crate::sidebar::{ThreadSortOrder, sort_threads};

fn normalize_worktree_path(path: Option<&str>) -> Option<&str> {
    let trimmed = path?.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// `getOrphanedWorktreePathForThread`: the target thread's worktree path when
/// no other thread links to the same worktree — deleting the thread would
/// orphan it. `None` when the thread is unknown, has no worktree, or shares
/// its worktree.
pub fn orphaned_worktree_path_for_thread(
    threads: &[&OrchestrationThreadShell],
    thread_id: &ThreadId,
) -> Option<String> {
    let target = threads.iter().find(|thread| thread.id == *thread_id)?;
    let target_path =
        normalize_worktree_path(target.worktree_path.as_ref().map(|path| path.0.as_str()))?;
    let shared = threads.iter().any(|thread| {
        thread.id != *thread_id
            && normalize_worktree_path(thread.worktree_path.as_ref().map(|path| path.0.as_str()))
                == Some(target_path)
    });
    (!shared).then(|| target_path.to_owned())
}

/// `formatWorktreePathForDisplay`: the last path segment, tolerant of Windows
/// separators and trailing slashes; falls back to the input when there is no
/// usable segment.
pub fn format_worktree_path_for_display(worktree_path: &str) -> String {
    let trimmed = worktree_path.trim();
    if trimmed.is_empty() {
        return worktree_path.to_owned();
    }
    let normalized = trimmed.replace('\\', "/");
    let normalized = normalized.trim_end_matches('/');
    let last_part = normalized.rsplit('/').next().unwrap_or("").trim();
    if last_part.is_empty() {
        trimmed.to_owned()
    } else {
        last_part.to_owned()
    }
}

/// `getFallbackThreadIdAfterDelete`: after deleting the open thread, the
/// selection moves to the deleted thread's project — its first remaining
/// thread in the sidebar's sort order (or `None`, which means home).
pub fn fallback_thread_id_after_delete(
    threads: &[&OrchestrationThreadShell],
    deleted_thread_id: &ThreadId,
    sort_order: ThreadSortOrder,
) -> Option<ThreadId> {
    let deleted = threads
        .iter()
        .find(|thread| thread.id == *deleted_thread_id)?;
    let mut candidates: Vec<&OrchestrationThreadShell> = threads
        .iter()
        .copied()
        .filter(|thread| thread.project_id == deleted.project_id && thread.id != *deleted_thread_id)
        .collect();
    sort_threads(&mut candidates, sort_order);
    candidates.first().map(|thread| thread.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitre_contracts::{ModelSelection, ProjectId, RuntimeMode, TrimmedNonEmptyString};

    fn tnes(value: &str) -> TrimmedNonEmptyString {
        TrimmedNonEmptyString(value.to_owned())
    }

    fn thread(id: &str, worktree_path: Option<&str>) -> OrchestrationThreadShell {
        OrchestrationThreadShell {
            additional_roots: None,
            archived_at: None,
            branch: None,
            created_at: tnes("2026-02-13T00:00:00.000Z"),
            has_actionable_proposed_plan: false,
            has_pending_approvals: false,
            has_pending_user_input: false,
            id: ThreadId(id.to_owned()),
            interaction_mode: None,
            latest_turn: None,
            latest_user_message_at: None,
            model_selection: ModelSelection {
                instance_id: None,
                model: serde_json::Value::String("claude-opus-5".to_owned()),
                options: None,
                provider: None,
            },
            project_id: ProjectId("project-1".to_owned()),
            resolved_additional_roots: None,
            runtime_mode: RuntimeMode::FullAccess,
            session: None,
            settled_at: None,
            settled_override: None,
            snoozed_at: None,
            snoozed_until: None,
            title: tnes(id),
            updated_at: tnes("2026-02-13T00:00:00.000Z"),
            worktree_path: worktree_path.map(tnes),
        }
    }

    #[test]
    fn orphan_check_returns_none_when_the_target_thread_does_not_exist() {
        assert_eq!(
            orphaned_worktree_path_for_thread(&[], &ThreadId("missing-thread".into())),
            None
        );
    }

    #[test]
    fn orphan_check_returns_none_when_the_target_thread_has_no_worktree() {
        let threads = [thread("thread-1", None)];
        let refs: Vec<_> = threads.iter().collect();
        assert_eq!(
            orphaned_worktree_path_for_thread(&refs, &ThreadId("thread-1".into())),
            None
        );
    }

    #[test]
    fn orphan_check_returns_the_path_when_no_other_thread_links_to_that_worktree() {
        let threads = [thread("thread-1", Some("/tmp/repo/worktrees/feature-a"))];
        let refs: Vec<_> = threads.iter().collect();
        assert_eq!(
            orphaned_worktree_path_for_thread(&refs, &ThreadId("thread-1".into())),
            Some("/tmp/repo/worktrees/feature-a".to_owned())
        );
    }

    #[test]
    fn orphan_check_returns_none_when_another_thread_links_to_the_same_worktree() {
        let threads = [
            thread("thread-1", Some("/tmp/repo/worktrees/feature-a")),
            thread("thread-2", Some("/tmp/repo/worktrees/feature-a")),
        ];
        let refs: Vec<_> = threads.iter().collect();
        assert_eq!(
            orphaned_worktree_path_for_thread(&refs, &ThreadId("thread-1".into())),
            None
        );
    }

    #[test]
    fn orphan_check_ignores_threads_linked_to_different_worktrees() {
        let threads = [
            thread("thread-1", Some("/tmp/repo/worktrees/feature-a")),
            thread("thread-2", Some("/tmp/repo/worktrees/feature-b")),
        ];
        let refs: Vec<_> = threads.iter().collect();
        assert_eq!(
            orphaned_worktree_path_for_thread(&refs, &ThreadId("thread-1".into())),
            Some("/tmp/repo/worktrees/feature-a".to_owned())
        );
    }

    #[test]
    fn display_path_shows_only_the_last_segment_for_unix_like_paths() {
        assert_eq!(
            format_worktree_path_for_display(
                "/Users/julius/.t3/worktrees/t3code-mvp/t3code-4e609bb8"
            ),
            "t3code-4e609bb8"
        );
    }

    #[test]
    fn display_path_normalizes_windows_separators_before_selecting_the_final_segment() {
        assert_eq!(
            format_worktree_path_for_display(
                "C:\\Users\\julius\\.t3\\worktrees\\t3code-mvp\\t3code-4e609bb8"
            ),
            "t3code-4e609bb8"
        );
    }

    #[test]
    fn display_path_uses_the_final_segment_even_outside_the_default_worktree_home() {
        assert_eq!(
            format_worktree_path_for_display("/tmp/custom-worktrees/my-worktree"),
            "my-worktree"
        );
    }

    #[test]
    fn display_path_ignores_trailing_slashes() {
        assert_eq!(
            format_worktree_path_for_display("/tmp/custom-worktrees/my-worktree/"),
            "my-worktree"
        );
    }

    #[test]
    fn fallback_picks_the_most_recent_remaining_thread_of_the_same_project() {
        let mut older = thread("thread-old", None);
        older.updated_at = tnes("2026-02-10T00:00:00.000Z");
        let mut newer = thread("thread-new", None);
        newer.updated_at = tnes("2026-02-12T00:00:00.000Z");
        let mut other_project = thread("thread-other", None);
        other_project.project_id = ProjectId("project-2".to_owned());
        other_project.updated_at = tnes("2026-02-13T00:00:00.000Z");
        let deleted = thread("thread-deleted", None);
        let threads = [older, newer, other_project, deleted];
        let refs: Vec<_> = threads.iter().collect();
        assert_eq!(
            fallback_thread_id_after_delete(
                &refs,
                &ThreadId("thread-deleted".into()),
                ThreadSortOrder::UpdatedAt
            ),
            Some(ThreadId("thread-new".into()))
        );
    }

    #[test]
    fn fallback_is_none_when_the_project_has_no_other_threads() {
        let deleted = thread("thread-deleted", None);
        let mut other_project = thread("thread-other", None);
        other_project.project_id = ProjectId("project-2".to_owned());
        let threads = [deleted, other_project];
        let refs: Vec<_> = threads.iter().collect();
        assert_eq!(
            fallback_thread_id_after_delete(
                &refs,
                &ThreadId("thread-deleted".into()),
                ThreadSortOrder::UpdatedAt
            ),
            None
        );
    }

    #[test]
    fn fallback_is_none_when_the_deleted_thread_is_unknown() {
        let threads = [thread("thread-1", None)];
        let refs: Vec<_> = threads.iter().collect();
        assert_eq!(
            fallback_thread_id_after_delete(
                &refs,
                &ThreadId("missing".into()),
                ThreadSortOrder::UpdatedAt
            ),
            None
        );
    }
}
