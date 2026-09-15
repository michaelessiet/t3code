//! Source-control logic behind the git header controls (`GitActionsControl`)
//! and the branch toolbar: the `subscribeVcsStatus` fold, the quick-action
//! decision table, git-menu enablement + disabled reasons, stacked-action
//! progress stage labels, and the default-branch confirmation copy.
//!
//! Everything here is a pure port of the Electron logic modules
//! (`GitActionsControl.logic.ts`, `packages/shared/src/git.ts`,
//! `sourceControlActions.ts`); the awkward user-facing strings ("checkout a
//! refName", "Commit on new refName") are reproduced verbatim per the parity
//! directive.

use vitre_contracts::{
    GitRunStackedActionResult, GitRunStackedActionResultBranchStatus, GitStackedAction,
    SourceControlProviderInfo, SourceControlProviderKind, VcsStatusLocalResult,
    VcsStatusRemoteResult, VcsStatusRemoteResultPrState, VcsStatusStreamEvent,
};

// ------------------------------------------------------------------- status

/// One changed file in the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWorkingTreeFile {
    pub path: String,
    pub insertions: i64,
    pub deletions: i64,
}

/// Working-tree diffstat.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitWorkingTree {
    pub files: Vec<GitWorkingTreeFile>,
    pub insertions: i64,
    pub deletions: i64,
}

/// The open change request attached to the current branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusPr {
    pub number: i64,
    pub title: String,
    pub url: String,
    pub base_ref: String,
    pub head_ref: String,
    pub state: VcsStatusRemoteResultPrState,
}

/// The merged local+remote git status the UI consumes — the client-side fold
/// of the `subscribeVcsStatus` stream (`applyGitStatusStreamEvent`).
#[derive(Debug, Clone, PartialEq)]
pub struct GitStatus {
    pub is_repo: bool,
    pub source_control_provider: Option<SourceControlProviderInfo>,
    pub has_primary_remote: bool,
    pub is_default_ref: bool,
    pub ref_name: Option<String>,
    pub has_working_tree_changes: bool,
    pub working_tree: GitWorkingTree,
    pub has_upstream: bool,
    pub ahead_count: i64,
    pub behind_count: i64,
    pub ahead_of_default_count: Option<i64>,
    pub pr: Option<GitStatusPr>,
}

impl GitStatus {
    /// Is there an open PR/MR on this branch?
    pub fn has_open_pr(&self) -> bool {
        self.pr
            .as_ref()
            .is_some_and(|pr| pr.state == VcsStatusRemoteResultPrState::Open)
    }
}

fn local_part(local: &VcsStatusLocalResult) -> (GitStatus, ()) {
    let status = GitStatus {
        is_repo: local.is_repo,
        source_control_provider: local.source_control_provider.clone().flatten(),
        has_primary_remote: local.has_primary_remote,
        is_default_ref: local.is_default_ref,
        ref_name: local.ref_name.as_ref().map(|name| name.0.clone()),
        has_working_tree_changes: local.has_working_tree_changes,
        working_tree: GitWorkingTree {
            files: local
                .working_tree
                .files
                .iter()
                .map(|file| GitWorkingTreeFile {
                    path: file.path.0.clone(),
                    insertions: file.insertions.0,
                    deletions: file.deletions.0,
                })
                .collect(),
            insertions: local.working_tree.insertions.0,
            deletions: local.working_tree.deletions.0,
        },
        // Remote part defaults ("no remote data yet" is zeros, not loading).
        has_upstream: false,
        ahead_count: 0,
        behind_count: 0,
        ahead_of_default_count: None,
        pr: None,
    };
    (status, ())
}

/// The synthesized local part used when a `remoteUpdated` delta arrives
/// before any snapshot (possible on reconnect races).
fn placeholder_local() -> GitStatus {
    GitStatus {
        is_repo: true,
        source_control_provider: None,
        has_primary_remote: false,
        is_default_ref: false,
        ref_name: None,
        has_working_tree_changes: false,
        working_tree: GitWorkingTree::default(),
        has_upstream: false,
        ahead_count: 0,
        behind_count: 0,
        ahead_of_default_count: None,
        pr: None,
    }
}

fn apply_remote(status: &mut GitStatus, remote: Option<&VcsStatusRemoteResult>) {
    match remote {
        Some(remote) => {
            status.has_upstream = remote.has_upstream;
            status.ahead_count = remote.ahead_count.0;
            status.behind_count = remote.behind_count.0;
            status.ahead_of_default_count =
                remote.ahead_of_default_count.flatten().map(|count| count.0);
            status.pr = remote.pr.as_ref().map(|pr| GitStatusPr {
                number: pr.number.0,
                title: pr.title.0.clone(),
                url: pr.url.0.clone(),
                base_ref: pr.base_ref.0.clone(),
                head_ref: pr.head_ref.0.clone(),
                state: pr.state.clone(),
            });
        }
        None => {
            status.has_upstream = false;
            status.ahead_count = 0;
            status.behind_count = 0;
            status.ahead_of_default_count = None;
            status.pr = None;
        }
    }
}

/// Fold one stream event over the current status (`applyGitStatusStreamEvent`
/// via `Stream.mapAccum` starting from `None`). The state must reset to
/// `None` on every (re)subscription — the stream always restarts with a
/// fresh snapshot.
pub fn apply_vcs_status_event(state: &mut Option<GitStatus>, event: &VcsStatusStreamEvent) {
    match event {
        VcsStatusStreamEvent::Snapshot { local, remote } => {
            let (mut status, ()) = local_part(local);
            apply_remote(&mut status, remote.as_ref());
            *state = Some(status);
        }
        VcsStatusStreamEvent::LocalUpdated { local } => {
            let previous_remote = state.clone();
            let (mut status, ()) = local_part(local);
            if let Some(previous) = previous_remote {
                status.has_upstream = previous.has_upstream;
                status.ahead_count = previous.ahead_count;
                status.behind_count = previous.behind_count;
                status.ahead_of_default_count = previous.ahead_of_default_count;
                status.pr = previous.pr;
            }
            *state = Some(status);
        }
        VcsStatusStreamEvent::RemoteUpdated { remote } => {
            let mut status = state.take().unwrap_or_else(placeholder_local);
            apply_remote(&mut status, remote.as_ref());
            *state = Some(status);
        }
        VcsStatusStreamEvent::Unknown(_) => {}
    }
}

// -------------------------------------------------------------- terminology

/// PR vs MR vs "change request" (`getChangeRequestTerminology`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeRequestTerminology {
    pub short_label: &'static str,
    pub singular: &'static str,
}

pub const TERMINOLOGY_DEFAULT: ChangeRequestTerminology = ChangeRequestTerminology {
    short_label: "PR",
    singular: "pull request",
};

/// A *missing* provider gets the default PR terminology; an explicitly
/// `unknown` provider says "change request" for both forms.
pub fn change_request_terminology(
    provider: Option<&SourceControlProviderKind>,
) -> ChangeRequestTerminology {
    match provider {
        Some(SourceControlProviderKind::Gitlab) => ChangeRequestTerminology {
            short_label: "MR",
            singular: "merge request",
        },
        Some(SourceControlProviderKind::UnknownX) => ChangeRequestTerminology {
            short_label: "change request",
            singular: "change request",
        },
        _ => TERMINOLOGY_DEFAULT,
    }
}

/// Terminology for a folded status (missing provider → defaults).
pub fn status_terminology(status: Option<&GitStatus>) -> ChangeRequestTerminology {
    change_request_terminology(
        status
            .and_then(|status| status.source_control_provider.as_ref())
            .map(|provider| &provider.kind),
    )
}

// -------------------------------------------------------------- quick action

/// What clicking the quick-action button does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickActionRun {
    StackedAction(GitStackedAction),
    OpenPr,
    OpenPublish,
    Pull,
    /// Disabled/informational: clicking shows the hint as an info toast.
    ShowHint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickAction {
    pub label: String,
    pub run: QuickActionRun,
    pub disabled: bool,
    pub hint: Option<String>,
}

impl QuickAction {
    fn enabled(label: impl Into<String>, run: QuickActionRun) -> Self {
        Self {
            label: label.into(),
            run,
            disabled: false,
            hint: None,
        }
    }

    fn disabled(label: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            run: QuickActionRun::ShowHint,
            disabled: true,
            hint: Some(hint.into()),
        }
    }
}

/// The Electron `resolveQuickAction` decision table, row for row. Order is
/// load-bearing; note rows 5 and 8 push the *default* ref via `commit_push`.
pub fn resolve_quick_action(status: Option<&GitStatus>, is_busy: bool) -> QuickAction {
    let terminology = status_terminology(status);
    let short = terminology.short_label;
    let singular = terminology.singular;
    if is_busy {
        return QuickAction::disabled("Commit", "Git action in progress.");
    }
    let Some(status) = status else {
        return QuickAction::disabled("Commit", "Git status is unavailable.");
    };
    if status.ref_name.is_none() {
        return QuickAction::disabled(
            "Commit",
            format!("Create and checkout a ref before pushing or opening a {singular}."),
        );
    }
    let open_pr = status.has_open_pr();
    if status.has_working_tree_changes {
        if !status.has_upstream && !status.has_primary_remote {
            return QuickAction::enabled(
                "Commit",
                QuickActionRun::StackedAction(GitStackedAction::Commit),
            );
        }
        if open_pr || status.is_default_ref {
            return QuickAction::enabled(
                "Commit & push",
                QuickActionRun::StackedAction(GitStackedAction::CommitPush),
            );
        }
        return QuickAction::enabled(
            format!("Commit, push & {short}"),
            QuickActionRun::StackedAction(GitStackedAction::CommitPushPr),
        );
    }
    if !status.has_upstream {
        if !status.has_primary_remote {
            if open_pr && status.ahead_count == 0 {
                return QuickAction::enabled(format!("View {short}"), QuickActionRun::OpenPr);
            }
            return QuickAction::enabled("Publish repository", QuickActionRun::OpenPublish);
        }
        if status.ahead_count == 0 {
            if open_pr {
                return QuickAction::enabled(format!("View {short}"), QuickActionRun::OpenPr);
            }
            return QuickAction::disabled("Push", "No local commits to push.");
        }
        if open_pr || status.is_default_ref {
            let action = if status.is_default_ref {
                GitStackedAction::CommitPush
            } else {
                GitStackedAction::Push
            };
            return QuickAction::enabled("Push", QuickActionRun::StackedAction(action));
        }
        return QuickAction::enabled(
            format!("Push & create {short}"),
            QuickActionRun::StackedAction(GitStackedAction::CreatePr),
        );
    }
    if status.ahead_count > 0 && status.behind_count > 0 {
        return QuickAction::disabled(
            "Sync ref",
            "Branch has diverged from upstream. Rebase/merge first.",
        );
    }
    if status.behind_count > 0 {
        return QuickAction::enabled("Pull", QuickActionRun::Pull);
    }
    if status.ahead_count > 0 {
        if open_pr || status.is_default_ref {
            let action = if status.is_default_ref {
                GitStackedAction::CommitPush
            } else {
                GitStackedAction::Push
            };
            return QuickAction::enabled("Push", QuickActionRun::StackedAction(action));
        }
        return QuickAction::enabled(
            format!("Push & create {short}"),
            QuickActionRun::StackedAction(GitStackedAction::CreatePr),
        );
    }
    if open_pr {
        return QuickAction::enabled(format!("View {short}"), QuickActionRun::OpenPr);
    }
    if status.ahead_of_default_count.unwrap_or(status.ahead_count) > 0 && !status.is_default_ref {
        return QuickAction::enabled(
            format!("Create {short}"),
            QuickActionRun::StackedAction(GitStackedAction::CreatePr),
        );
    }
    QuickAction::disabled("Commit", "Branch is up to date. No action needed.")
}

// --------------------------------------------------------------------- menu

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitMenuEntry {
    Commit,
    Push,
    ViewPr,
    CreatePr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitMenuItem {
    pub entry: GitMenuEntry,
    pub label: String,
    pub enabled: bool,
    /// Present only when disabled (`getMenuActionDisabledReason`).
    pub disabled_reason: Option<String>,
}

/// The dropdown items (`buildMenuItems`). No status → empty; no primary
/// remote → Commit only (Publish repository is appended separately by the
/// view).
// The enablement expressions keep Electron's exact (redundant) boolean shape
// so a diff against `GitActionsControl.logic.ts` stays term-for-term.
#[allow(clippy::nonminimal_bool)]
pub fn build_menu_items(status: Option<&GitStatus>, is_busy: bool) -> Vec<GitMenuItem> {
    let Some(status) = status else {
        return Vec::new();
    };
    let terminology = status_terminology(Some(status));
    let short = terminology.short_label;
    let has_changes = status.has_working_tree_changes;
    let has_branch = status.ref_name.is_some();
    let open_pr = status.has_open_pr();
    let mut items = Vec::new();

    let commit_enabled = !is_busy && has_changes;
    items.push(GitMenuItem {
        entry: GitMenuEntry::Commit,
        label: "Commit".into(),
        enabled: commit_enabled,
        disabled_reason: (!commit_enabled).then(|| {
            menu_action_disabled_reason(GitMenuEntry::Commit, Some(status), is_busy, &terminology)
        }),
    });
    if !status.has_primary_remote {
        return items;
    }

    let push_enabled = !is_busy
        && has_branch
        && status.behind_count == 0
        && status.ahead_count > 0
        && (status.has_upstream || (status.has_primary_remote && !status.has_upstream));
    items.push(GitMenuItem {
        entry: GitMenuEntry::Push,
        label: "Push".into(),
        enabled: push_enabled,
        disabled_reason: (!push_enabled).then(|| {
            menu_action_disabled_reason(GitMenuEntry::Push, Some(status), is_busy, &terminology)
        }),
    });

    if open_pr {
        let enabled = !is_busy;
        items.push(GitMenuItem {
            entry: GitMenuEntry::ViewPr,
            label: format!("View {short}"),
            enabled,
            disabled_reason: (!enabled).then(|| {
                menu_action_disabled_reason(
                    GitMenuEntry::ViewPr,
                    Some(status),
                    is_busy,
                    &terminology,
                )
            }),
        });
    } else {
        let can_push_without_upstream = status.has_primary_remote && !status.has_upstream;
        let enabled = !is_busy
            && has_branch
            && !has_changes
            && !open_pr
            && status.ahead_of_default_count.unwrap_or(status.ahead_count) > 0
            && status.behind_count == 0
            && (status.has_upstream || can_push_without_upstream);
        items.push(GitMenuItem {
            entry: GitMenuEntry::CreatePr,
            label: format!("Create {short}"),
            enabled,
            disabled_reason: (!enabled).then(|| {
                menu_action_disabled_reason(
                    GitMenuEntry::CreatePr,
                    Some(status),
                    is_busy,
                    &terminology,
                )
            }),
        });
    }
    items
}

/// Why a menu row is disabled — the reasons are evaluated in the exact order
/// Electron lists them (`getMenuActionDisabledReason`), busy before missing
/// status before per-item reasons.
pub fn menu_action_disabled_reason(
    entry: GitMenuEntry,
    status: Option<&GitStatus>,
    is_busy: bool,
    terminology: &ChangeRequestTerminology,
) -> String {
    let singular = terminology.singular;
    if is_busy {
        return "Git action in progress.".into();
    }
    let Some(status) = status else {
        return "Git status is unavailable.".into();
    };
    let has_branch = status.ref_name.is_some();
    match entry {
        GitMenuEntry::Commit => {
            if !status.has_working_tree_changes {
                "Worktree is clean. Make changes before committing.".into()
            } else {
                "Commit is currently unavailable.".into()
            }
        }
        GitMenuEntry::Push => {
            if !has_branch {
                "Detached HEAD: checkout a refName before pushing.".into()
            } else if status.has_working_tree_changes {
                "Commit or stash local changes before pushing.".into()
            } else if status.behind_count > 0 {
                "Branch is behind upstream. Pull/rebase before pushing.".into()
            } else if !status.has_upstream && !status.has_primary_remote {
                "Add an \"origin\" remote before pushing.".into()
            } else if status.ahead_count == 0 {
                "No local commits to push.".into()
            } else {
                "Push is currently unavailable.".into()
            }
        }
        GitMenuEntry::ViewPr => format!("View {singular} is currently unavailable."),
        GitMenuEntry::CreatePr => {
            if !has_branch {
                format!("Detached HEAD: checkout a refName before creating a {singular}.")
            } else if status.has_working_tree_changes {
                format!("Commit local changes before creating a {singular}.")
            } else if !status.has_upstream && !status.has_primary_remote {
                format!("Add an \"origin\" remote before creating a {singular}.")
            } else if status.ahead_of_default_count.unwrap_or(status.ahead_count) == 0 {
                format!("No local commits to include in a {singular}.")
            } else if status.behind_count > 0 {
                format!("Branch is behind upstream. Pull/rebase before creating a {singular}.")
            } else {
                format!("Create {singular} is currently unavailable.")
            }
        }
    }
}

/// "Publish repository..." menu entry visibility.
pub fn publish_entry_visible(status: Option<&GitStatus>) -> bool {
    status.is_some_and(|status| status.is_repo && !status.has_primary_remote)
}

/// Detached-HEAD footnote in the menu.
pub fn detached_head_note(status: Option<&GitStatus>) -> bool {
    status.is_some_and(|status| status.is_repo && status.ref_name.is_none())
}

/// "Behind upstream. Pull/rebase first." footnote.
pub fn behind_upstream_note(status: Option<&GitStatus>) -> bool {
    status.is_some_and(|status| {
        status.ref_name.is_some()
            && !status.has_working_tree_changes
            && status.behind_count > 0
            && status.ahead_count == 0
    })
}

// -------------------------------------------------------- stacked-action UX

/// Does this action commit as part of its run (`includesCommit`)?
pub fn action_includes_commit(
    action: &GitStackedAction,
    tree_dirty: bool,
    feature_branch: bool,
) -> bool {
    let can_commit = matches!(
        action,
        GitStackedAction::Commit | GitStackedAction::CommitPush | GitStackedAction::CommitPushPr
    );
    can_commit && (*action == GitStackedAction::Commit || tree_dirty || feature_branch)
}

/// Progress stage labels for the loading toast (`buildGitActionProgressStages`).
/// Only the first stage is shown as the initial title; the server's
/// `phase_started` labels take over from there.
pub fn build_progress_stages(
    action: &GitStackedAction,
    feature_branch: bool,
    includes_commit: bool,
    has_custom_message: bool,
    should_push_before_pr: bool,
    terminology: &ChangeRequestTerminology,
) -> Vec<String> {
    let short = terminology.short_label;
    let singular = terminology.singular;
    let branch_stage = feature_branch.then(|| "Preparing feature ref...".to_string());
    let commit_stages = || -> Vec<String> {
        if !includes_commit {
            return Vec::new();
        }
        if has_custom_message {
            vec!["Committing...".into()]
        } else {
            vec![
                "Generating commit message...".into(),
                "Committing...".into(),
            ]
        }
    };
    let push_stage = "Pushing...".to_string();
    let pr_stages = || {
        vec![
            format!("Preparing {short}..."),
            format!("Generating {short} content..."),
            format!("Creating {singular}..."),
        ]
    };
    let mut stages = Vec::new();
    match action {
        GitStackedAction::Push => stages.push(push_stage),
        GitStackedAction::CreatePr => {
            if should_push_before_pr {
                stages.push(push_stage);
            }
            stages.extend(pr_stages());
        }
        GitStackedAction::Commit => {
            stages.extend(branch_stage);
            stages.extend(commit_stages());
        }
        GitStackedAction::CommitPush => {
            stages.extend(branch_stage);
            stages.extend(commit_stages());
            stages.push(push_stage);
        }
        GitStackedAction::CommitPushPr => {
            stages.extend(branch_stage);
            stages.extend(commit_stages());
            stages.push(push_stage);
            stages.extend(pr_stages());
        }
        GitStackedAction::Unknown(_) => {}
    }
    stages
}

/// Actions that gate on the default branch (`requiresDefaultBranchConfirmation`).
pub fn requires_default_branch_confirmation(
    action: &GitStackedAction,
    is_default_ref: bool,
) -> bool {
    is_default_ref
        && matches!(
            action,
            GitStackedAction::Push
                | GitStackedAction::CreatePr
                | GitStackedAction::CommitPush
                | GitStackedAction::CommitPushPr
        )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultBranchDialogCopy {
    pub title: String,
    pub description: String,
    pub continue_label: String,
}

/// Copy for the default-branch confirmation (`resolveDefaultBranchActionDialogCopy`).
pub fn default_branch_dialog_copy(
    action: &GitStackedAction,
    branch: &str,
    includes_commit: bool,
    terminology: &ChangeRequestTerminology,
) -> DefaultBranchDialogCopy {
    let short = terminology.short_label;
    let singular = terminology.singular;
    let suffix = format!(
        " on \"{branch}\". You can continue on this ref or create a feature ref and run the same action there."
    );
    let creates_pr = matches!(
        action,
        GitStackedAction::CreatePr | GitStackedAction::CommitPushPr
    );
    if creates_pr {
        if includes_commit {
            DefaultBranchDialogCopy {
                title: format!("Commit, push & create {short} from default ref?"),
                description: format!(
                    "This action will commit, push, and create a {singular}{suffix}"
                ),
                continue_label: format!("Commit, push & create {short}"),
            }
        } else {
            DefaultBranchDialogCopy {
                title: format!("Push & create {short} from default ref?"),
                description: format!(
                    "This action will push local commits and create a {singular}{suffix}"
                ),
                continue_label: format!("Push & create {short}"),
            }
        }
    } else if includes_commit {
        DefaultBranchDialogCopy {
            title: "Commit & push to default ref?".into(),
            description: format!("This action will commit and push changes{suffix}"),
            continue_label: format!("Commit & push to {branch}"),
        }
    } else {
        DefaultBranchDialogCopy {
            title: "Push to default ref?".into(),
            description: format!("This action will push local commits{suffix}"),
            continue_label: format!("Push to {branch}"),
        }
    }
}

/// The wire `actionId` for `git.runStackedAction`:
/// `` `${len(targetKey)}:${targetKey}${localId}` `` with
/// `targetKey = JSON.stringify([environmentId, cwd])`. Progress events are
/// matched on this transport id (plus cwd) before re-tagging to the local id.
pub fn transport_action_id(environment_id: &str, cwd: &str, local_id: &str) -> String {
    let target_key = serde_json::to_string(&(environment_id, cwd))
        .expect("two strings always serialize to JSON");
    format!("{}:{}{}", target_key.len(), target_key, local_id)
}

/// Progress-toast description: the last hook output line wins; otherwise the
/// elapsed time since the hook/phase started ("Running for 5s" /
/// "Running for 1m 5s"); otherwise the caller's "Waiting for Git..." default.
pub fn resolve_progress_description(
    last_output_line: Option<&str>,
    elapsed_secs: Option<u64>,
) -> Option<String> {
    if let Some(line) = last_output_line {
        let line = line.trim();
        if !line.is_empty() {
            return Some(line.to_string());
        }
    }
    let elapsed = elapsed_secs?;
    if elapsed >= 60 {
        Some(format!("Running for {}m {}s", elapsed / 60, elapsed % 60))
    } else {
        Some(format!("Running for {elapsed}s"))
    }
}

// -------------------------------------------------------- branch metadata

/// `t3code/<8 hex>` or the legacy `t3code/<uuid-v4>` — the auto-named
/// worktree branches that must never clobber a real thread branch.
pub fn is_temporary_worktree_branch(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("t3code/") else {
        return false;
    };
    let is_hex = |value: &str| !value.is_empty() && value.bytes().all(|b| b.is_ascii_hexdigit());
    if rest.len() == 8 && is_hex(rest) {
        return true;
    }
    // uuid v4: 8-4-4-4-12 hex groups.
    let groups: Vec<&str> = rest.split('-').collect();
    groups.len() == 5
        && [8, 4, 4, 4, 12]
            .iter()
            .zip(&groups)
            .all(|(len, group)| group.len() == *len && is_hex(group))
}

/// After a stacked action: adopt the created feature branch, if any
/// (`resolveThreadBranchUpdate`).
pub fn resolve_thread_branch_update(result: &GitRunStackedActionResult) -> Option<String> {
    if result.branch.status != GitRunStackedActionResultBranchStatus::Created {
        return None;
    }
    result
        .branch
        .name
        .clone()
        .flatten()
        .map(|name| name.0)
        .filter(|name| !name.is_empty())
}

/// Live thread-branch sync (`resolveLiveThreadBranchUpdate`): `Some(branch)`
/// when the thread metadata should adopt git's current branch, `None` to
/// leave it alone.
pub fn resolve_live_thread_branch_update(
    status: Option<&GitStatus>,
    thread_branch: Option<&str>,
) -> Option<String> {
    let status = status?;
    let git_branch = status.ref_name.as_deref()?;
    if Some(git_branch) == thread_branch {
        return None;
    }
    // A real thread branch is never clobbered by a temporary worktree branch.
    if thread_branch.is_some_and(|branch| !is_temporary_worktree_branch(branch))
        && is_temporary_worktree_branch(git_branch)
    {
        return None;
    }
    Some(git_branch.to_string())
}

/// `deriveLocalBranchNameFromRemoteRef`: strip through the first `/`
/// (`origin/feat/x` → `feat/x`).
pub fn derive_local_branch_name_from_remote_ref(name: &str) -> String {
    match name.split_once('/') {
        Some((_, rest)) => rest.to_string(),
        None => name.to_string(),
    }
}

/// The PR pill next to the branch selector (`resolveThreadPr`): with a
/// dedicated worktree the status's PR is trusted; otherwise only when git is
/// actually on the thread's branch.
pub fn resolve_thread_pr<'a>(
    thread_branch: Option<&str>,
    status: Option<&'a GitStatus>,
    has_dedicated_worktree: bool,
) -> Option<&'a GitStatusPr> {
    let status = status?;
    let pr = status.pr.as_ref()?;
    if has_dedicated_worktree {
        return Some(pr);
    }
    (status.ref_name.as_deref() == thread_branch && thread_branch.is_some()).then_some(pr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitre_contracts::{
        GitRunStackedActionResultBranch, GitRunStackedActionResultCommit,
        GitRunStackedActionResultCommitStatus, GitRunStackedActionResultPr,
        GitRunStackedActionResultPrStatus, GitRunStackedActionResultPush,
        GitRunStackedActionResultPushStatus, GitRunStackedActionResultToast,
        GitRunStackedActionResultToastCta, NonNegativeInt, PositiveInt, TrimmedNonEmptyString,
        VcsStatusLocalResultWorkingTree, VcsStatusRemoteResultPr,
    };

    fn tnes(value: &str) -> TrimmedNonEmptyString {
        TrimmedNonEmptyString(value.to_string())
    }

    fn base_status() -> GitStatus {
        GitStatus {
            is_repo: true,
            source_control_provider: None,
            has_primary_remote: true,
            is_default_ref: false,
            ref_name: Some("feat/x".into()),
            has_working_tree_changes: false,
            working_tree: GitWorkingTree::default(),
            has_upstream: true,
            ahead_count: 0,
            behind_count: 0,
            ahead_of_default_count: None,
            pr: None,
        }
    }

    fn open_pr() -> GitStatusPr {
        GitStatusPr {
            number: 7,
            title: "t".into(),
            url: "https://example.com/pr/7".into(),
            base_ref: "main".into(),
            head_ref: "feat/x".into(),
            state: VcsStatusRemoteResultPrState::Open,
        }
    }

    fn local(ref_name: Option<&str>, dirty: bool) -> VcsStatusLocalResult {
        VcsStatusLocalResult {
            has_primary_remote: true,
            has_working_tree_changes: dirty,
            is_default_ref: false,
            is_repo: true,
            ref_name: ref_name.map(tnes),
            source_control_provider: None,
            working_tree: VcsStatusLocalResultWorkingTree {
                deletions: NonNegativeInt(0),
                files: vec![],
                insertions: NonNegativeInt(0),
            },
        }
    }

    fn remote(ahead: i64, behind: i64, upstream: bool) -> VcsStatusRemoteResult {
        VcsStatusRemoteResult {
            ahead_count: NonNegativeInt(ahead),
            ahead_of_default_count: None,
            behind_count: NonNegativeInt(behind),
            has_upstream: upstream,
            pr: None,
        }
    }

    // ------------------------------------------------------------- the fold

    #[test]
    fn snapshot_with_null_remote_fills_zeros() {
        let mut state = None;
        apply_vcs_status_event(
            &mut state,
            &VcsStatusStreamEvent::Snapshot {
                local: local(Some("main"), false),
                remote: None,
            },
        );
        let status = state.unwrap();
        assert_eq!(status.ref_name.as_deref(), Some("main"));
        assert!(!status.has_upstream);
        assert_eq!((status.ahead_count, status.behind_count), (0, 0));
        assert!(status.pr.is_none());
    }

    #[test]
    fn local_update_keeps_the_previous_remote_part() {
        let mut state = None;
        apply_vcs_status_event(
            &mut state,
            &VcsStatusStreamEvent::Snapshot {
                local: local(Some("main"), false),
                remote: Some(remote(3, 1, true)),
            },
        );
        apply_vcs_status_event(
            &mut state,
            &VcsStatusStreamEvent::LocalUpdated {
                local: local(Some("main"), true),
            },
        );
        let status = state.unwrap();
        assert!(status.has_working_tree_changes);
        assert_eq!((status.ahead_count, status.behind_count), (3, 1));
        assert!(status.has_upstream);
    }

    #[test]
    fn remote_update_before_any_snapshot_synthesizes_the_local_part() {
        let mut state = None;
        apply_vcs_status_event(
            &mut state,
            &VcsStatusStreamEvent::RemoteUpdated {
                remote: Some(remote(2, 0, true)),
            },
        );
        let status = state.unwrap();
        assert!(status.is_repo, "placeholder local says isRepo: true");
        assert!(!status.has_primary_remote);
        assert!(status.ref_name.is_none());
        assert_eq!(status.ahead_count, 2);
    }

    #[test]
    fn remote_update_with_null_remote_resets_the_remote_part() {
        let mut state = None;
        apply_vcs_status_event(
            &mut state,
            &VcsStatusStreamEvent::Snapshot {
                local: local(Some("main"), false),
                remote: Some(VcsStatusRemoteResult {
                    ahead_count: NonNegativeInt(1),
                    ahead_of_default_count: Some(Some(NonNegativeInt(4))),
                    behind_count: NonNegativeInt(0),
                    has_upstream: true,
                    pr: Some(VcsStatusRemoteResultPr {
                        base_ref: tnes("main"),
                        head_ref: tnes("feat"),
                        number: PositiveInt(3),
                        state: VcsStatusRemoteResultPrState::Open,
                        title: tnes("t"),
                        url: tnes("https://x"),
                    }),
                }),
            },
        );
        apply_vcs_status_event(
            &mut state,
            &VcsStatusStreamEvent::RemoteUpdated { remote: None },
        );
        let status = state.unwrap();
        assert!(!status.has_upstream);
        assert_eq!(status.ahead_of_default_count, None);
        assert!(status.pr.is_none());
        assert_eq!(status.ref_name.as_deref(), Some("main"), "local part kept");
    }

    // ------------------------------------------------------------ terminology

    #[test]
    fn terminology_per_provider() {
        use SourceControlProviderKind as Kind;
        assert_eq!(change_request_terminology(None).short_label, "PR");
        assert_eq!(
            change_request_terminology(Some(&Kind::Github)).singular,
            "pull request"
        );
        assert_eq!(
            change_request_terminology(Some(&Kind::Gitlab)).short_label,
            "MR"
        );
        assert_eq!(
            change_request_terminology(Some(&Kind::Gitlab)).singular,
            "merge request"
        );
        assert_eq!(
            change_request_terminology(Some(&Kind::UnknownX)).short_label,
            "change request"
        );
        assert_eq!(
            change_request_terminology(Some(&Kind::Bitbucket)).short_label,
            "PR"
        );
    }

    // ----------------------------------------------------------- quick action

    #[test]
    fn quick_action_busy_and_missing_status_rows() {
        let busy = resolve_quick_action(Some(&base_status()), true);
        assert!(busy.disabled);
        assert_eq!(busy.hint.as_deref(), Some("Git action in progress."));
        let missing = resolve_quick_action(None, false);
        assert_eq!(missing.hint.as_deref(), Some("Git status is unavailable."));
    }

    #[test]
    fn quick_action_detached_head() {
        let mut status = base_status();
        status.ref_name = None;
        let action = resolve_quick_action(Some(&status), false);
        assert!(action.disabled);
        assert_eq!(
            action.hint.as_deref(),
            Some("Create and checkout a ref before pushing or opening a pull request.")
        );
    }

    #[test]
    fn quick_action_dirty_tree_variants() {
        let mut status = base_status();
        status.has_working_tree_changes = true;
        status.has_upstream = false;
        status.has_primary_remote = false;
        assert_eq!(
            resolve_quick_action(Some(&status), false).run,
            QuickActionRun::StackedAction(GitStackedAction::Commit)
        );
        let mut status = base_status();
        status.has_working_tree_changes = true;
        status.is_default_ref = true;
        let action = resolve_quick_action(Some(&status), false);
        assert_eq!(action.label, "Commit & push");
        assert_eq!(
            action.run,
            QuickActionRun::StackedAction(GitStackedAction::CommitPush)
        );
        let mut status = base_status();
        status.has_working_tree_changes = true;
        let action = resolve_quick_action(Some(&status), false);
        assert_eq!(action.label, "Commit, push & PR");
        assert_eq!(
            action.run,
            QuickActionRun::StackedAction(GitStackedAction::CommitPushPr)
        );
    }

    #[test]
    fn quick_action_no_upstream_variants() {
        // No primary remote → publish.
        let mut status = base_status();
        status.has_upstream = false;
        status.has_primary_remote = false;
        assert_eq!(
            resolve_quick_action(Some(&status), false).run,
            QuickActionRun::OpenPublish
        );
        // Not ahead, no PR → disabled Push.
        let mut status = base_status();
        status.has_upstream = false;
        let action = resolve_quick_action(Some(&status), false);
        assert!(action.disabled);
        assert_eq!(action.label, "Push");
        assert_eq!(action.hint.as_deref(), Some("No local commits to push."));
        // Ahead on the DEFAULT ref pushes via commit_push (gotcha 9).
        let mut status = base_status();
        status.has_upstream = false;
        status.ahead_count = 2;
        status.is_default_ref = true;
        assert_eq!(
            resolve_quick_action(Some(&status), false).run,
            QuickActionRun::StackedAction(GitStackedAction::CommitPush)
        );
        // Ahead on a feature ref → push & create PR.
        let mut status = base_status();
        status.has_upstream = false;
        status.ahead_count = 2;
        let action = resolve_quick_action(Some(&status), false);
        assert_eq!(action.label, "Push & create PR");
        assert_eq!(
            action.run,
            QuickActionRun::StackedAction(GitStackedAction::CreatePr)
        );
    }

    #[test]
    fn quick_action_sync_pull_push_rows() {
        let mut status = base_status();
        status.ahead_count = 1;
        status.behind_count = 1;
        let action = resolve_quick_action(Some(&status), false);
        assert!(action.disabled);
        assert_eq!(action.label, "Sync ref");
        let mut status = base_status();
        status.behind_count = 2;
        assert_eq!(
            resolve_quick_action(Some(&status), false).run,
            QuickActionRun::Pull
        );
        let mut status = base_status();
        status.ahead_count = 2;
        let action = resolve_quick_action(Some(&status), false);
        assert_eq!(action.label, "Push & create PR");
        let mut status = base_status();
        status.ahead_count = 2;
        status.pr = Some(open_pr());
        assert_eq!(
            resolve_quick_action(Some(&status), false).run,
            QuickActionRun::StackedAction(GitStackedAction::Push)
        );
    }

    #[test]
    fn quick_action_view_create_and_fallback_rows() {
        let mut status = base_status();
        status.pr = Some(open_pr());
        let action = resolve_quick_action(Some(&status), false);
        assert_eq!(action.label, "View PR");
        assert_eq!(action.run, QuickActionRun::OpenPr);
        // aheadOfDefault drives Create PR even when in sync with upstream.
        let mut status = base_status();
        status.ahead_of_default_count = Some(3);
        let action = resolve_quick_action(Some(&status), false);
        assert_eq!(action.label, "Create PR");
        // Fully in sync → disabled Commit.
        let status = base_status();
        let action = resolve_quick_action(Some(&status), false);
        assert!(action.disabled);
        assert_eq!(
            action.hint.as_deref(),
            Some("Branch is up to date. No action needed.")
        );
    }

    #[test]
    fn quick_action_gitlab_terminology() {
        let mut status = base_status();
        status.has_working_tree_changes = true;
        status.source_control_provider = Some(SourceControlProviderInfo {
            base_url: tnes("https://gitlab.com"),
            kind: SourceControlProviderKind::Gitlab,
            name: tnes("GitLab"),
        });
        assert_eq!(
            resolve_quick_action(Some(&status), false).label,
            "Commit, push & MR"
        );
    }

    // ------------------------------------------------------------------ menu

    #[test]
    fn menu_no_status_is_empty_and_no_remote_is_commit_only() {
        assert!(build_menu_items(None, false).is_empty());
        let mut status = base_status();
        status.has_primary_remote = false;
        let items = build_menu_items(Some(&status), false);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].entry, GitMenuEntry::Commit);
    }

    #[test]
    fn menu_enablement_and_reasons() {
        let mut status = base_status();
        status.ahead_count = 1;
        let items = build_menu_items(Some(&status), false);
        assert_eq!(items.len(), 3);
        let commit = &items[0];
        assert!(!commit.enabled);
        assert_eq!(
            commit.disabled_reason.as_deref(),
            Some("Worktree is clean. Make changes before committing.")
        );
        let push = &items[1];
        assert!(push.enabled);
        let create = &items[2];
        assert_eq!(create.entry, GitMenuEntry::CreatePr);
        assert!(create.enabled);

        // Behind → push disabled with the behind reason.
        let mut status = base_status();
        status.ahead_count = 1;
        status.behind_count = 1;
        let items = build_menu_items(Some(&status), false);
        assert_eq!(
            items[1].disabled_reason.as_deref(),
            Some("Branch is behind upstream. Pull/rebase before pushing.")
        );

        // Open PR swaps Create for View.
        let mut status = base_status();
        status.pr = Some(open_pr());
        let items = build_menu_items(Some(&status), false);
        assert_eq!(items[2].entry, GitMenuEntry::ViewPr);
        assert!(items[2].enabled);

        // Busy beats everything in the reason order.
        let items = build_menu_items(Some(&base_status()), true);
        assert_eq!(
            items[0].disabled_reason.as_deref(),
            Some("Git action in progress.")
        );
    }

    #[test]
    fn menu_create_pr_reason_order() {
        let terminology = TERMINOLOGY_DEFAULT;
        let mut status = base_status();
        status.ref_name = None;
        assert_eq!(
            menu_action_disabled_reason(GitMenuEntry::CreatePr, Some(&status), false, &terminology),
            "Detached HEAD: checkout a refName before creating a pull request."
        );
        let mut status = base_status();
        status.has_working_tree_changes = true;
        assert_eq!(
            menu_action_disabled_reason(GitMenuEntry::CreatePr, Some(&status), false, &terminology),
            "Commit local changes before creating a pull request."
        );
        let status = base_status();
        assert_eq!(
            menu_action_disabled_reason(GitMenuEntry::CreatePr, Some(&status), false, &terminology),
            "No local commits to include in a pull request."
        );
    }

    #[test]
    fn menu_notes_and_publish_visibility() {
        let mut status = base_status();
        status.has_primary_remote = false;
        assert!(publish_entry_visible(Some(&status)));
        assert!(!publish_entry_visible(Some(&base_status())));
        let mut status = base_status();
        status.ref_name = None;
        assert!(detached_head_note(Some(&status)));
        let mut status = base_status();
        status.behind_count = 2;
        assert!(behind_upstream_note(Some(&status)));
        status.ahead_count = 1;
        assert!(!behind_upstream_note(Some(&status)));
    }

    // -------------------------------------------------------------- stages

    #[test]
    fn progress_stages_compose_per_action() {
        let t = TERMINOLOGY_DEFAULT;
        assert_eq!(
            build_progress_stages(&GitStackedAction::Push, false, false, false, false, &t),
            vec!["Pushing..."]
        );
        assert_eq!(
            build_progress_stages(&GitStackedAction::CreatePr, false, false, false, true, &t),
            vec![
                "Pushing...",
                "Preparing PR...",
                "Generating PR content...",
                "Creating pull request..."
            ]
        );
        assert_eq!(
            build_progress_stages(&GitStackedAction::CreatePr, false, false, false, false, &t)[0],
            "Preparing PR..."
        );
        assert_eq!(
            build_progress_stages(&GitStackedAction::Commit, true, true, false, false, &t),
            vec![
                "Preparing feature ref...",
                "Generating commit message...",
                "Committing..."
            ]
        );
        assert_eq!(
            build_progress_stages(&GitStackedAction::CommitPush, false, true, true, false, &t),
            vec!["Committing...", "Pushing..."]
        );
        // commit_push with a clean tree skips the commit stages.
        assert_eq!(
            build_progress_stages(
                &GitStackedAction::CommitPush,
                false,
                false,
                false,
                false,
                &t
            ),
            vec!["Pushing..."]
        );
    }

    #[test]
    fn includes_commit_formula() {
        assert!(action_includes_commit(
            &GitStackedAction::Commit,
            false,
            false
        ));
        assert!(action_includes_commit(
            &GitStackedAction::CommitPush,
            true,
            false
        ));
        assert!(action_includes_commit(
            &GitStackedAction::CommitPush,
            false,
            true
        ));
        assert!(!action_includes_commit(
            &GitStackedAction::CommitPush,
            false,
            false
        ));
        assert!(!action_includes_commit(&GitStackedAction::Push, true, true));
    }

    // ------------------------------------------------- default-branch dialog

    #[test]
    fn default_branch_dialog_copy_variants() {
        let t = TERMINOLOGY_DEFAULT;
        let copy = default_branch_dialog_copy(&GitStackedAction::CommitPush, "main", true, &t);
        assert_eq!(copy.title, "Commit & push to default ref?");
        assert_eq!(copy.continue_label, "Commit & push to main");
        assert!(
            copy.description
                .starts_with("This action will commit and push changes on \"main\".")
        );
        let copy = default_branch_dialog_copy(&GitStackedAction::Push, "main", false, &t);
        assert_eq!(copy.title, "Push to default ref?");
        assert_eq!(copy.continue_label, "Push to main");
        let copy = default_branch_dialog_copy(&GitStackedAction::CommitPushPr, "main", true, &t);
        assert_eq!(copy.title, "Commit, push & create PR from default ref?");
        let copy = default_branch_dialog_copy(&GitStackedAction::CreatePr, "main", false, &t);
        assert_eq!(copy.title, "Push & create PR from default ref?");
        assert!(copy.description.starts_with(
            "This action will push local commits and create a pull request on \"main\"."
        ));
    }

    #[test]
    fn gate_only_fires_for_pushing_actions_on_the_default_ref() {
        assert!(requires_default_branch_confirmation(
            &GitStackedAction::Push,
            true
        ));
        assert!(requires_default_branch_confirmation(
            &GitStackedAction::CommitPushPr,
            true
        ));
        assert!(!requires_default_branch_confirmation(
            &GitStackedAction::Commit,
            true
        ));
        assert!(!requires_default_branch_confirmation(
            &GitStackedAction::Push,
            false
        ));
    }

    // ------------------------------------------------------------ transport

    #[test]
    fn transport_action_id_matches_the_js_scheme() {
        // targetKey = JSON.stringify(["env","/repo"]) = `["env","/repo"]` (15 chars)
        assert_eq!(
            transport_action_id("env", "/repo", "abc"),
            "15:[\"env\",\"/repo\"]abc"
        );
    }

    #[test]
    fn progress_description_prefers_output_then_elapsed() {
        assert_eq!(
            resolve_progress_description(Some("hook says hi"), Some(5)).as_deref(),
            Some("hook says hi")
        );
        assert_eq!(
            resolve_progress_description(None, Some(5)).as_deref(),
            Some("Running for 5s")
        );
        assert_eq!(
            resolve_progress_description(None, Some(65)).as_deref(),
            Some("Running for 1m 5s")
        );
        assert_eq!(resolve_progress_description(None, None), None);
    }

    // ------------------------------------------------------ branch metadata

    #[test]
    fn temporary_worktree_branch_matcher() {
        assert!(is_temporary_worktree_branch("t3code/1a2b3c4d"));
        assert!(is_temporary_worktree_branch(
            "t3code/123e4567-e89b-42d3-a456-426614174000"
        ));
        assert!(!is_temporary_worktree_branch("t3code/feature"));
        assert!(!is_temporary_worktree_branch("t3code/1a2b3c"));
        assert!(!is_temporary_worktree_branch("feature/1a2b3c4d"));
    }

    #[test]
    fn live_branch_update_rules() {
        let mut status = base_status();
        status.ref_name = Some("feat/y".into());
        assert_eq!(
            resolve_live_thread_branch_update(Some(&status), Some("feat/x")),
            Some("feat/y".into())
        );
        // Equal → no update.
        assert_eq!(
            resolve_live_thread_branch_update(Some(&status), Some("feat/y")),
            None
        );
        // Detached keeps the thread branch.
        let mut detached = base_status();
        detached.ref_name = None;
        assert_eq!(
            resolve_live_thread_branch_update(Some(&detached), Some("feat/x")),
            None
        );
        // A temporary worktree branch never clobbers a real one...
        let mut temp = base_status();
        temp.ref_name = Some("t3code/1a2b3c4d".into());
        assert_eq!(
            resolve_live_thread_branch_update(Some(&temp), Some("feat/x")),
            None
        );
        // ...but is adopted when the thread has no branch yet.
        assert_eq!(
            resolve_live_thread_branch_update(Some(&temp), None),
            Some("t3code/1a2b3c4d".into())
        );
    }

    #[test]
    fn stacked_result_branch_adoption() {
        let result = |status: GitRunStackedActionResultBranchStatus,
                      name: Option<&str>|
         -> GitRunStackedActionResult {
            GitRunStackedActionResult {
                action: GitStackedAction::CommitPush,
                branch: GitRunStackedActionResultBranch {
                    name: name.map(|n| Some(tnes(n))),
                    status,
                },
                commit: GitRunStackedActionResultCommit {
                    commit_sha: None,
                    status: GitRunStackedActionResultCommitStatus::Created,
                    subject: None,
                },
                pr: GitRunStackedActionResultPr {
                    base_branch: None,
                    head_branch: None,
                    number: None,
                    status: GitRunStackedActionResultPrStatus::SkippedNotRequested,
                    title: None,
                    url: None,
                },
                push: GitRunStackedActionResultPush {
                    branch: None,
                    set_upstream: None,
                    status: GitRunStackedActionResultPushStatus::Pushed,
                    upstream_branch: None,
                },
                toast: GitRunStackedActionResultToast {
                    cta: GitRunStackedActionResultToastCta::None {},
                    description: None,
                    title: tnes("Pushed"),
                },
            }
        };
        assert_eq!(
            resolve_thread_branch_update(&result(
                GitRunStackedActionResultBranchStatus::Created,
                Some("feature/z")
            )),
            Some("feature/z".into())
        );
        assert_eq!(
            resolve_thread_branch_update(&result(
                GitRunStackedActionResultBranchStatus::SkippedNotRequested,
                Some("feature/z")
            )),
            None
        );
        assert_eq!(
            resolve_thread_branch_update(&result(
                GitRunStackedActionResultBranchStatus::Created,
                None
            )),
            None
        );
    }

    #[test]
    fn remote_ref_local_name_derivation() {
        assert_eq!(
            derive_local_branch_name_from_remote_ref("origin/feat/x"),
            "feat/x"
        );
        assert_eq!(derive_local_branch_name_from_remote_ref("main"), "main");
    }

    #[test]
    fn thread_pr_resolution() {
        let mut status = base_status();
        status.pr = Some(open_pr());
        status.ref_name = Some("feat/x".into());
        // Dedicated worktree trusts the status PR outright.
        assert!(resolve_thread_pr(Some("other"), Some(&status), true).is_some());
        // Shared checkout requires git to be on the thread branch.
        assert!(resolve_thread_pr(Some("feat/x"), Some(&status), false).is_some());
        assert!(resolve_thread_pr(Some("other"), Some(&status), false).is_none());
        assert!(resolve_thread_pr(None, Some(&status), false).is_none());
    }
}
