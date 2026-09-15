//! The sidebar's derived model: thread/project ordering, per-thread status
//! pills, and the preview window a collapsed project shows.
//!
//! Port of `packages/client-runtime/src/state/threadSort.ts` plus the
//! sidebar-status half of `apps/web/src/components/Sidebar.logic.ts`
//! (`resolveThreadStatusPill`, `resolveProjectStatusIndicator`,
//! `sortProjectsForSidebar`) and the windowing `useMemo` in
//! `SidebarProjectItem`.

use vitre_contracts::{
    OrchestrationLatestTurn, OrchestrationSession, OrchestrationSessionStatus,
    OrchestrationThreadShell, ProviderInteractionMode,
};

use crate::project_grouping::ProjectGroup;
use crate::wire_opt::{defined2, defined3};

/// `SidebarThreadSortOrder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreadSortOrder {
    /// Latest user message (the setting's name is historical).
    #[default]
    UpdatedAt,
    CreatedAt,
}

/// `SidebarProjectSortOrder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProjectSortOrder {
    #[default]
    UpdatedAt,
    CreatedAt,
    /// Drag-ordered; [`sort_project_groups`] leaves the input order alone.
    Manual,
}

impl ThreadSortOrder {
    pub const ALL: [Self; 2] = [Self::UpdatedAt, Self::CreatedAt];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::UpdatedAt => "updated_at",
            Self::CreatedAt => "created_at",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "updated_at" => Some(Self::UpdatedAt),
            "created_at" => Some(Self::CreatedAt),
            _ => None,
        }
    }

    /// Electron's `SIDEBAR_THREAD_SORT_LABELS`.
    pub fn label(self) -> &'static str {
        match self {
            Self::UpdatedAt => "Last user message",
            Self::CreatedAt => "Created at",
        }
    }
}

impl ProjectSortOrder {
    pub const ALL: [Self; 3] = [Self::UpdatedAt, Self::CreatedAt, Self::Manual];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::UpdatedAt => "updated_at",
            Self::CreatedAt => "created_at",
            Self::Manual => "manual",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "updated_at" => Some(Self::UpdatedAt),
            "created_at" => Some(Self::CreatedAt),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }

    /// Electron's `SIDEBAR_SORT_LABELS`.
    pub fn label(self) -> &'static str {
        match self {
            Self::UpdatedAt => "Last user message",
            Self::CreatedAt => "Created at",
            Self::Manual => "Manual",
        }
    }
}

/// `SidebarThreadPreviewCount` bounds and default.
pub const MIN_THREAD_PREVIEW_COUNT: usize = 1;
pub const MAX_THREAD_PREVIEW_COUNT: usize = 15;
pub const DEFAULT_THREAD_PREVIEW_COUNT: usize = 6;

/// `Date.parse` for the ISO timestamps the server emits.
pub fn parse_timestamp_ms(iso: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|parsed| parsed.timestamp_millis())
}

fn first_timestamp_ms(candidates: &[Option<&str>]) -> Option<i64> {
    candidates
        .iter()
        .flatten()
        .find_map(|value| parse_timestamp_ms(value))
}

/// `getLatestUserMessageTimestamp`. The shell carries no message list, so the
/// TS scan over `thread.messages` has nothing to walk here and the stamped
/// `latestUserMessageAt` (with the updated/created fallback) is the whole
/// computation.
fn latest_user_message_ms(thread: &OrchestrationThreadShell) -> i64 {
    if let Some(latest) = thread
        .latest_user_message_at
        .as_ref()
        .and_then(|value| parse_timestamp_ms(&value.0))
    {
        return latest;
    }
    first_timestamp_ms(&[
        Some(thread.updated_at.0.as_str()),
        Some(thread.created_at.0.as_str()),
    ])
    .unwrap_or(i64::MIN)
}

/// `getThreadSortTimestamp`.
pub fn thread_sort_timestamp(thread: &OrchestrationThreadShell, order: ThreadSortOrder) -> i64 {
    match order {
        ThreadSortOrder::CreatedAt => first_timestamp_ms(&[
            Some(thread.created_at.0.as_str()),
            Some(thread.updated_at.0.as_str()),
        ])
        .unwrap_or(i64::MIN),
        ThreadSortOrder::UpdatedAt => latest_user_message_ms(thread),
    }
}

/// `sortThreads`: newest first, ties broken by descending id.
pub fn sort_threads(threads: &mut [&OrchestrationThreadShell], order: ThreadSortOrder) {
    threads.sort_by(|left, right| {
        thread_sort_timestamp(right, order)
            .cmp(&thread_sort_timestamp(left, order))
            .then_with(|| right.id.0.cmp(&left.id.0))
    });
}

/// `getProjectSortTimestamp`: a project sorts by its most recent thread, and
/// only falls back to its own timestamps while it has none.
fn project_sort_timestamp(
    group: &ProjectGroup,
    projects: &[vitre_contracts::OrchestrationProjectShell],
    threads: &[&OrchestrationThreadShell],
    order: ProjectSortOrder,
) -> i64 {
    let thread_order = match order {
        ProjectSortOrder::CreatedAt => ThreadSortOrder::CreatedAt,
        _ => ThreadSortOrder::UpdatedAt,
    };
    if !threads.is_empty() {
        return threads
            .iter()
            .map(|thread| thread_sort_timestamp(thread, thread_order))
            .max()
            .unwrap_or(i64::MIN);
    }
    let project = &projects[group.representative];
    match order {
        ProjectSortOrder::CreatedAt => parse_timestamp_ms(&project.created_at.0),
        _ => parse_timestamp_ms(&project.updated_at.0)
            .or_else(|| parse_timestamp_ms(&project.created_at.0)),
    }
    .unwrap_or(i64::MIN)
}

/// `sortProjectsForSidebar` over logical rows.
///
/// `threads_for` supplies each row's *unarchived* threads; `Manual` order is
/// the caller's persisted order and is returned untouched.
pub fn sort_project_groups<'a, F>(
    groups: &mut [ProjectGroup],
    projects: &[vitre_contracts::OrchestrationProjectShell],
    order: ProjectSortOrder,
    mut threads_for: F,
) where
    F: FnMut(&ProjectGroup) -> Vec<&'a OrchestrationThreadShell>,
{
    if order == ProjectSortOrder::Manual {
        return;
    }
    let stamped: Vec<(String, i64)> = groups
        .iter()
        .map(|group| {
            let threads = threads_for(group);
            (
                group.key.clone(),
                project_sort_timestamp(group, projects, &threads, order),
            )
        })
        .collect();
    let timestamp_of = |key: &str| {
        stamped
            .iter()
            .find(|(stamped_key, _)| stamped_key == key)
            .map(|(_, timestamp)| *timestamp)
            .unwrap_or(i64::MIN)
    };
    groups.sort_by(|left, right| {
        timestamp_of(&right.key)
            .cmp(&timestamp_of(&left.key))
            // Ties fall back to the representative title, then the row key —
            // both ascending, matching `localeCompare`'s direction.
            .then_with(|| {
                projects[left.representative]
                    .title
                    .0
                    .cmp(&projects[right.representative].title.0)
            })
            .then_with(|| left.key.cmp(&right.key))
    });
}

/// `ThreadStatusPill["label"]` — the five states a sidebar row can advertise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadStatus {
    PendingApproval,
    AwaitingInput,
    Working,
    Connecting,
    PlanReady,
    Completed,
}

impl ThreadStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::PendingApproval => "Pending Approval",
            Self::AwaitingInput => "Awaiting Input",
            Self::Working => "Working",
            Self::Connecting => "Connecting",
            Self::PlanReady => "Plan Ready",
            Self::Completed => "Completed",
        }
    }

    /// `THREAD_STATUS_PRIORITY`: what a collapsed project's single dot shows
    /// when its threads disagree.
    pub fn priority(self) -> u8 {
        match self {
            Self::PendingApproval => 5,
            Self::AwaitingInput => 4,
            Self::Working | Self::Connecting => 3,
            Self::PlanReady => 2,
            Self::Completed => 1,
        }
    }

    /// Only the in-motion states pulse.
    pub fn pulse(self) -> bool {
        matches!(self, Self::Working | Self::Connecting)
    }
}

/// `isLatestTurnSettled` (`packages/shared/src/orchestrationTiming.ts`).
fn is_latest_turn_settled(
    latest_turn: Option<&OrchestrationLatestTurn>,
    session: Option<&OrchestrationSession>,
) -> bool {
    let Some(turn) = latest_turn else {
        return false;
    };
    if turn.started_at.is_none() || turn.completed_at.is_none() {
        return false;
    }
    match session {
        None => true,
        Some(session) => session.status != OrchestrationSessionStatus::Running,
    }
}

/// `hasUnseenCompletion`: the turn finished after the last time this thread
/// was opened. An unparseable visit stamp counts as "never seen it since".
fn has_unseen_completion(thread: &OrchestrationThreadShell, last_visited_at: Option<&str>) -> bool {
    let Some(completed_at) = thread
        .latest_turn
        .as_ref()
        .and_then(|turn| turn.completed_at.as_ref())
        .and_then(|value| parse_timestamp_ms(&value.0))
    else {
        return false;
    };
    let Some(last_visited_at) = last_visited_at else {
        return false;
    };
    match parse_timestamp_ms(last_visited_at) {
        None => true,
        Some(visited) => completed_at > visited,
    }
}

/// `resolveThreadStatusPill`.
pub fn resolve_thread_status(
    thread: &OrchestrationThreadShell,
    last_visited_at: Option<&str>,
) -> Option<ThreadStatus> {
    if thread.has_pending_approvals {
        return Some(ThreadStatus::PendingApproval);
    }
    if thread.has_pending_user_input {
        return Some(ThreadStatus::AwaitingInput);
    }
    match thread.session.as_ref().map(|session| &session.status) {
        Some(OrchestrationSessionStatus::Running) => return Some(ThreadStatus::Working),
        Some(OrchestrationSessionStatus::Starting) => return Some(ThreadStatus::Connecting),
        _ => {}
    }
    let in_plan_mode = matches!(
        defined2(&thread.interaction_mode),
        Some(ProviderInteractionMode::Plan)
    );
    if in_plan_mode
        && thread.has_actionable_proposed_plan
        && is_latest_turn_settled(thread.latest_turn.as_ref(), thread.session.as_ref())
    {
        return Some(ThreadStatus::PlanReady);
    }
    has_unseen_completion(thread, last_visited_at).then_some(ThreadStatus::Completed)
}

/// `resolveProjectStatusIndicator`: the most urgent status among a set of
/// threads, ties going to the first seen.
pub fn resolve_project_status(
    statuses: impl IntoIterator<Item = Option<ThreadStatus>>,
) -> Option<ThreadStatus> {
    let mut highest: Option<ThreadStatus> = None;
    for status in statuses.into_iter().flatten() {
        if highest.is_none_or(|current| status.priority() > current.priority()) {
            highest = Some(status);
        }
    }
    highest
}

/// A thread is archived when `archivedAt` carries a value; the wire's absent
/// and `null` forms both mean "not archived".
pub fn is_archived(thread: &OrchestrationThreadShell) -> bool {
    matches!(defined3(&thread.archived_at), Some(Some(_)))
}

/// What one project row renders, from `SidebarProjectItem`'s windowing memo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadWindow {
    /// Indices into the row's ordered thread list that actually render.
    pub rendered: Vec<usize>,
    /// True when the row has more threads than the preview count.
    pub has_overflow: bool,
    /// Most urgent status among the threads this window hides — the dot the
    /// "Show more" affordance carries.
    pub hidden_status: Option<ThreadStatus>,
    /// Expanded with nothing to show: render "No threads yet".
    pub show_empty_state: bool,
    /// Whether the thread panel renders at all (expanded, or collapsed with
    /// the active thread pinned into view).
    pub show_panel: bool,
}

/// The preview window for one project row.
///
/// `threads` must already be filtered to unarchived and sorted. `active`
/// is the index of the open thread within `threads`, if it belongs to this
/// row: a *collapsed* row still shows that one thread, so the sidebar never
/// hides where you are.
pub fn thread_window(
    threads: &[&OrchestrationThreadShell],
    statuses: &[Option<ThreadStatus>],
    active: Option<usize>,
    expanded: bool,
    list_expanded: bool,
    preview_count: usize,
) -> ThreadWindow {
    let pinned = if expanded { None } else { active };
    let has_overflow = threads.len() > preview_count;
    let preview_len = if list_expanded || !has_overflow {
        threads.len()
    } else {
        preview_count
    };

    let visible: Vec<usize> = match pinned {
        Some(pinned) => {
            let mut visible: Vec<usize> = (0..preview_len).collect();
            if !visible.contains(&pinned) {
                visible.push(pinned);
            }
            visible
        }
        None => (0..preview_len).collect(),
    };
    let rendered = match pinned {
        Some(pinned) => vec![pinned],
        None => visible.clone(),
    };
    let hidden_status = resolve_project_status(
        (0..threads.len())
            .filter(|index| !visible.contains(index))
            .map(|index| statuses.get(index).copied().flatten()),
    );

    ThreadWindow {
        rendered,
        has_overflow,
        hidden_status,
        show_empty_state: expanded && threads.is_empty(),
        show_panel: expanded || pinned.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitre_contracts::{
        ModelSelection, OrchestrationLatestTurnState, ProjectId, RuntimeMode, ThreadId,
        TrimmedNonEmptyString, TurnId,
    };

    fn tnes(value: &str) -> TrimmedNonEmptyString {
        TrimmedNonEmptyString(value.to_owned())
    }

    fn thread(id: &str, created_at: &str, updated_at: &str) -> OrchestrationThreadShell {
        OrchestrationThreadShell {
            additional_roots: None,
            archived_at: None,
            branch: None,
            created_at: tnes(created_at),
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
            project_id: ProjectId("project".to_owned()),
            resolved_additional_roots: None,
            runtime_mode: RuntimeMode::FullAccess,
            session: None,
            settled_at: None,
            settled_override: None,
            snoozed_at: None,
            snoozed_until: None,
            title: tnes(id),
            updated_at: tnes(updated_at),
            worktree_path: None,
        }
    }

    fn session(status: OrchestrationSessionStatus) -> OrchestrationSession {
        OrchestrationSession {
            active_turn_id: None,
            last_error: None,
            provider_instance_id: None,
            provider_name: None,
            runtime_mode: None,
            status,
            thread_id: ThreadId("thread".to_owned()),
            updated_at: tnes("2026-01-01T00:00:00.000Z"),
        }
    }

    fn turn(started_at: Option<&str>, completed_at: Option<&str>) -> OrchestrationLatestTurn {
        OrchestrationLatestTurn {
            assistant_message_id: None,
            completed_at: completed_at.map(tnes),
            requested_at: tnes("2026-01-01T00:00:00.000Z"),
            source_proposed_plan: None,
            started_at: started_at.map(tnes),
            state: OrchestrationLatestTurnState::Completed,
            turn_id: TurnId("turn".to_owned()),
        }
    }

    #[test]
    fn threads_sort_by_latest_user_message_then_descending_id() {
        let mut old = thread("a", "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z");
        old.latest_user_message_at = Some(tnes("2026-01-05T00:00:00.000Z"));
        let recent = thread("b", "2026-01-02T00:00:00.000Z", "2026-01-09T00:00:00.000Z");
        let mut threads = vec![&old, &recent];
        sort_threads(&mut threads, ThreadSortOrder::UpdatedAt);
        assert_eq!(threads[0].id.0, "b");

        // created_at order ignores activity entirely.
        let mut threads = vec![&old, &recent];
        sort_threads(&mut threads, ThreadSortOrder::CreatedAt);
        assert_eq!(threads[0].id.0, "b");

        // Same timestamp: the larger id wins.
        let left = thread("a", "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z");
        let right = thread("z", "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z");
        let mut threads = vec![&left, &right];
        sort_threads(&mut threads, ThreadSortOrder::CreatedAt);
        assert_eq!(threads[0].id.0, "z");
    }

    #[test]
    fn status_priority_runs_approval_over_input_over_working() {
        let mut approval = thread("a", "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z");
        approval.has_pending_approvals = true;
        approval.has_pending_user_input = true;
        approval.session = Some(session(OrchestrationSessionStatus::Running));
        assert_eq!(
            resolve_thread_status(&approval, None),
            Some(ThreadStatus::PendingApproval)
        );

        let mut input = approval.clone();
        input.has_pending_approvals = false;
        assert_eq!(
            resolve_thread_status(&input, None),
            Some(ThreadStatus::AwaitingInput)
        );

        let mut working = input.clone();
        working.has_pending_user_input = false;
        assert_eq!(
            resolve_thread_status(&working, None),
            Some(ThreadStatus::Working)
        );

        let mut connecting = working.clone();
        connecting.session = Some(session(OrchestrationSessionStatus::Starting));
        assert_eq!(
            resolve_thread_status(&connecting, None),
            Some(ThreadStatus::Connecting)
        );
    }

    #[test]
    fn plan_ready_needs_plan_mode_a_settled_turn_and_an_actionable_plan() {
        let mut planned = thread("a", "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z");
        planned.interaction_mode = Some(Some(ProviderInteractionMode::Plan));
        planned.has_actionable_proposed_plan = true;
        planned.latest_turn = Some(turn(
            Some("2026-01-01T00:00:00.000Z"),
            Some("2026-01-01T00:01:00.000Z"),
        ));
        planned.session = Some(session(OrchestrationSessionStatus::Idle));
        assert_eq!(
            resolve_thread_status(&planned, None),
            Some(ThreadStatus::PlanReady)
        );

        // A running session means the turn is not settled yet.
        let mut running = planned.clone();
        running.session = Some(session(OrchestrationSessionStatus::Running));
        assert_eq!(
            resolve_thread_status(&running, None),
            Some(ThreadStatus::Working)
        );

        // Default interaction mode never shows the plan prompt.
        let mut default_mode = planned.clone();
        default_mode.interaction_mode = Some(Some(ProviderInteractionMode::Default));
        assert_eq!(resolve_thread_status(&default_mode, None), None);
    }

    #[test]
    fn completed_only_shows_for_a_turn_that_finished_after_the_last_visit() {
        let mut finished = thread("a", "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z");
        finished.latest_turn = Some(turn(
            Some("2026-01-01T00:00:00.000Z"),
            Some("2026-01-01T00:05:00.000Z"),
        ));
        // Never opened: nothing to be unseen relative to.
        assert_eq!(resolve_thread_status(&finished, None), None);
        assert_eq!(
            resolve_thread_status(&finished, Some("2026-01-01T00:04:00.000Z")),
            Some(ThreadStatus::Completed)
        );
        assert_eq!(
            resolve_thread_status(&finished, Some("2026-01-01T00:06:00.000Z")),
            None
        );
    }

    #[test]
    fn project_status_takes_the_most_urgent_thread() {
        assert_eq!(
            resolve_project_status([
                Some(ThreadStatus::Completed),
                None,
                Some(ThreadStatus::PendingApproval),
                Some(ThreadStatus::Working),
            ]),
            Some(ThreadStatus::PendingApproval)
        );
        assert_eq!(resolve_project_status([None, None]), None);
    }

    #[test]
    fn expanded_rows_cap_at_the_preview_count_and_report_hidden_status() {
        let threads: Vec<OrchestrationThreadShell> = (0..5)
            .map(|index| {
                thread(
                    &format!("t{index}"),
                    "2026-01-01T00:00:00.000Z",
                    "2026-01-01T00:00:00.000Z",
                )
            })
            .collect();
        let refs: Vec<&OrchestrationThreadShell> = threads.iter().collect();
        let statuses = vec![
            None,
            None,
            Some(ThreadStatus::Completed),
            Some(ThreadStatus::PendingApproval),
            None,
        ];

        let window = thread_window(&refs, &statuses, None, true, false, 2);
        assert_eq!(window.rendered, vec![0, 1]);
        assert!(window.has_overflow);
        assert_eq!(window.hidden_status, Some(ThreadStatus::PendingApproval));
        assert!(window.show_panel);
        assert!(!window.show_empty_state);

        // "Show more" reveals the rest and empties the hidden set.
        let window = thread_window(&refs, &statuses, None, true, true, 2);
        assert_eq!(window.rendered, vec![0, 1, 2, 3, 4]);
        assert_eq!(window.hidden_status, None);
    }

    #[test]
    fn a_collapsed_row_still_pins_the_open_thread() {
        let threads: Vec<OrchestrationThreadShell> = (0..5)
            .map(|index| {
                thread(
                    &format!("t{index}"),
                    "2026-01-01T00:00:00.000Z",
                    "2026-01-01T00:00:00.000Z",
                )
            })
            .collect();
        let refs: Vec<&OrchestrationThreadShell> = threads.iter().collect();
        let statuses = vec![None; 5];

        let window = thread_window(&refs, &statuses, Some(4), false, false, 2);
        assert_eq!(window.rendered, vec![4]);
        assert!(window.show_panel);

        // Collapsed with the open thread elsewhere: no panel at all.
        let window = thread_window(&refs, &statuses, None, false, false, 2);
        assert!(!window.show_panel);
        assert!(!window.show_empty_state);
    }

    #[test]
    fn an_expanded_empty_row_shows_the_empty_state() {
        let window = thread_window(&[], &[], None, true, false, 6);
        assert!(window.show_empty_state);
        assert!(window.show_panel);
        assert!(!window.has_overflow);
    }
}
