//! The chat-header git controls: Electron's `GitActionsControl.tsx` — quick
//! action + dropdown menu, the commit dialog, the default-branch confirm
//! dialog, and the `git.runStackedAction` progress-toast pipeline — plus the
//! `subscribeVcsStatus` fold that feeds all of it.
//!
//! Decision tables and copy live in [`vitre_state::source_control`] (a pure
//! port of `GitActionsControl.logic.ts`). Deferred, matrix-noted: the publish
//! wizard (the menu entry shows an info toast), GitRootSwitcher (Vitre drives
//! a single root), the per-(env,cwd) command queue (Vitre disables the
//! controls while busy instead), provider brand icons (generic PR icon), and
//! opening a commit-dialog file row in the editor.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use gpui::{AnyElement, Context, Entity, SharedString, Window, div, prelude::*, px};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, StyledExt as _,
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Textarea, TextareaState},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    v_flex,
};
use vitre_contracts::methods::{
    GitRunStackedAction, SubscribeVcsStatus, VcsInit, VcsPull, VcsRefreshStatus,
};
use vitre_contracts::{
    ClientOrchestrationCommand, CommandId, GitActionProgressEvent, GitRunStackedActionInput,
    GitRunStackedActionResult, GitRunStackedActionResultToastCta, GitStackedAction, VcsInitInput,
    VcsPullInput, VcsPullResultStatus, VcsStatusInput,
};
use vitre_rpc::TypedStreamEvent;
use vitre_state::source_control::{
    DefaultBranchDialogCopy, GitMenuEntry, GitStatus, QuickActionRun, action_includes_commit,
    apply_vcs_status_event, behind_upstream_note, build_menu_items, build_progress_stages,
    default_branch_dialog_copy, detached_head_note, publish_entry_visible,
    requires_default_branch_confirmation, resolve_progress_description, resolve_quick_action,
    resolve_thread_branch_update, status_terminology, transport_action_id,
};

use crate::assets::VitreIcon;

use super::{ChatApp, fresh_id, tnes};

/// Mirrors vitre-client's completed-stream backoff.
const RESUBSCRIBE_AFTER_COMPLETION: Duration = Duration::from_secs(2);

/// The single progress-toast slot per control (Electron keeps one
/// `activeGitActionProgressRef` per `GitActionsControl`). Pushing with the
/// same id replaces the toast in place.
struct GitToastTag;
const GIT_TOAST_KEY: &str = "git-action";

/// All source-control state on the chat view.
#[derive(Default)]
pub(super) struct GitState {
    /// The status-stream target (`activeThread.worktreePath ?? workspaceRoot`).
    cwd: Option<String>,
    /// The folded `subscribeVcsStatus` result. Kept renderable across
    /// resubscribes; the fold accumulator itself lives in the task.
    status: Option<GitStatus>,
    /// Status-stream failure, shown as a menu footer row.
    status_error: Option<SharedString>,
    /// A stacked action, pull, or init is running (`isGitActionRunning`).
    busy: bool,
    /// Slot ownership: teardown and the ticker check they still own the
    /// toast slot, since a newer action may have re-armed it.
    action_seq: u64,
    progress: Option<GitProgress>,
    commit_dialog: Option<CommitDialogState>,
    pending_default_branch: Option<PendingDefaultBranchAction>,
    _status_task: Option<gpui::Task<()>>,
}

/// Live progress of the running stacked action, rendered into the toast.
struct GitProgress {
    seq: u64,
    title: SharedString,
    current_phase_label: Option<SharedString>,
    last_output_line: Option<String>,
    phase_started_at: Option<Instant>,
    hook_started_at: Option<Instant>,
}

/// The open "Commit changes" dialog. Held on the view because the dialog's
/// content closure only keeps a weak reference to it.
struct CommitDialogState {
    message: Entity<TextareaState>,
    /// Excluded file paths (default empty = everything included).
    excluded: HashSet<String>,
    editing: bool,
}

/// The action stashed while the default-branch confirm dialog is up
/// (`PendingDefaultBranchAction`).
struct PendingDefaultBranchAction {
    action: GitStackedAction,
    commit_message: Option<String>,
    file_paths: Option<Vec<String>>,
    from_commit_dialog: bool,
}

#[derive(Default)]
struct GitRunOptions {
    commit_message: Option<String>,
    skip_default_branch_prompt: bool,
    feature_branch: bool,
    file_paths: Option<Vec<String>>,
    /// Close the commit dialog once the action actually starts (Electron's
    /// `onConfirmed` — the dialog stays open while the default-branch gate
    /// is showing).
    from_commit_dialog: bool,
}

/// Transport id + cwd of a progress event — every kind carries both.
fn progress_event_ids(event: &GitActionProgressEvent) -> Option<(&str, &str)> {
    match event {
        GitActionProgressEvent::ActionStarted { action_id, cwd, .. }
        | GitActionProgressEvent::PhaseStarted { action_id, cwd, .. }
        | GitActionProgressEvent::HookStarted { action_id, cwd, .. }
        | GitActionProgressEvent::HookOutput { action_id, cwd, .. }
        | GitActionProgressEvent::HookFinished { action_id, cwd, .. }
        | GitActionProgressEvent::ActionFinished { action_id, cwd, .. }
        | GitActionProgressEvent::ActionFailed { action_id, cwd, .. } => {
            Some((&action_id.0, &cwd.0))
        }
        GitActionProgressEvent::Unknown(_) => None,
    }
}

impl ChatApp {
    // ------------------------------------------------------------ the stream

    /// (Re)target the `subscribeVcsStatus` loop at the open thread's git cwd.
    /// Cheap when nothing changed — safe to call from render.
    pub(super) fn sync_git_status(&mut self, cx: &mut Context<Self>) {
        let desired = if self.thread.is_some() {
            self.open_project_root()
        } else {
            None
        };
        if desired == self.git.cwd {
            return;
        }
        self.git.cwd = desired.clone();
        self.git.status = None;
        self.git.status_error = None;
        self.git._status_task = None;
        let Some(cwd) = desired else {
            cx.notify();
            return;
        };
        let Some(client) = self.client.clone() else {
            cx.notify();
            return;
        };
        let task = cx.spawn(async move |this, cx| {
            let payload = VcsStatusInput { cwd: tnes(&cwd) };
            let mut sessions = client.sessions();
            loop {
                let Some(handle) = sessions.borrow_and_update().clone() else {
                    if sessions.changed().await.is_err() {
                        return;
                    }
                    continue;
                };
                let Ok(mut subscription) = handle
                    .session
                    .subscribe_typed::<SubscribeVcsStatus>(&payload)
                else {
                    if sessions.changed().await.is_err() {
                        return;
                    }
                    continue;
                };
                // The fold restarts at None per (re)subscription (`mapAccum`
                // from null); the rendered status keeps its last value until
                // the fresh snapshot lands, like Electron's atom.
                let mut folded: Option<GitStatus> = None;
                let completed = loop {
                    tokio::select! {
                        event = subscription.next() => match event {
                            Some(TypedStreamEvent::Values(events)) => {
                                for event in &events {
                                    apply_vcs_status_event(&mut folded, event);
                                }
                                let latest = folded.clone();
                                if this
                                    .update(cx, |app, cx| {
                                        app.git.status = latest;
                                        app.git.status_error = None;
                                        cx.notify();
                                    })
                                    .is_err()
                                {
                                    return;
                                }
                                if subscription.ack().is_err() {
                                    break false;
                                }
                            }
                            Some(TypedStreamEvent::Completed(result)) => {
                                if let Err(error) = result {
                                    let message: SharedString =
                                        format!("Git status failed: {error:?}").into();
                                    if this
                                        .update(cx, |app, cx| {
                                            app.git.status_error = Some(message);
                                            cx.notify();
                                        })
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                                break true;
                            }
                            None => break false,
                        },
                        changed = sessions.changed() => {
                            if changed.is_err() {
                                return;
                            }
                            let replaced = sessions
                                .borrow()
                                .as_ref()
                                .is_none_or(|current| current.generation != handle.generation);
                            if replaced {
                                break false;
                            }
                        }
                    }
                };
                if completed {
                    cx.background_executor()
                        .timer(RESUBSCRIBE_AFTER_COMPLETION)
                        .await;
                }
            }
        });
        self.git._status_task = Some(task);
        cx.notify();
    }

    /// Fire-and-forget `vcs.refreshStatus` — the stream delivers the update.
    fn git_refresh_status(&self, cx: &mut Context<Self>) {
        let (Some(client), Some(cwd)) = (self.client.clone(), self.git.cwd.clone()) else {
            return;
        };
        cx.spawn(async move |_, _| {
            let payload = VcsStatusInput { cwd: tnes(&cwd) };
            let _ = client.call::<VcsRefreshStatus>(&payload).await;
        })
        .detach();
    }

    // -------------------------------------------------------------- rendering

    /// The header control: quick action + menu, or "Initialize Git" when the
    /// cwd is not a repo. `isRepo` defaults to true while status loads, so
    /// the init button never flashes first.
    pub(super) fn render_git_actions(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.sync_git_status(cx);
        self.git.cwd.as_ref()?;
        let is_repo = self.git.status.as_ref().is_none_or(|status| status.is_repo);
        if !is_repo {
            return Some(
                Button::new("git-init")
                    .outline()
                    .xsmall()
                    .icon(Icon::new(VitreIcon::GitBranchPlus).with_size(px(14.)))
                    .label(if self.git.busy {
                        "Initializing..."
                    } else {
                        "Initialize Git"
                    })
                    .disabled(self.git.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.git_init(window, cx)))
                    .into_any_element(),
            );
        }

        let quick = resolve_quick_action(self.git.status.as_ref(), self.git.busy);
        let quick_icon = match &quick.run {
            QuickActionRun::OpenPr => Icon::new(VitreIcon::GitPullRequest),
            QuickActionRun::OpenPublish => Icon::new(VitreIcon::CloudUpload),
            QuickActionRun::Pull => Icon::new(VitreIcon::Info),
            QuickActionRun::StackedAction(GitStackedAction::Commit) => {
                Icon::new(VitreIcon::GitCommitHorizontal)
            }
            QuickActionRun::StackedAction(GitStackedAction::CreatePr) => {
                Icon::new(VitreIcon::GitPullRequest)
            }
            QuickActionRun::StackedAction(_) => Icon::new(VitreIcon::CloudUpload),
            QuickActionRun::ShowHint => {
                if quick.label == "Commit" {
                    Icon::new(VitreIcon::GitCommitHorizontal)
                } else {
                    Icon::new(VitreIcon::Info)
                }
            }
        };
        // Electron shows the reason in a hover popover on the disabled quick
        // action; the fork's tooltip renders on disabled buttons, so that is
        // the closest affordance.
        let hint: Option<SharedString> = quick.hint.clone().map(SharedString::from);
        let quick_button = Button::new("git-quick-action")
            .outline()
            .xsmall()
            .icon(quick_icon.with_size(px(14.)))
            .label(SharedString::from(quick.label.clone()))
            .disabled(quick.disabled)
            .when_some(hint, |this, hint| this.tooltip(hint))
            .on_click(cx.listener(|this, _, window, cx| this.git_quick_action(window, cx)));

        let menu_items = build_menu_items(self.git.status.as_ref(), self.git.busy);
        let show_publish = publish_entry_visible(self.git.status.as_ref());
        let detached_note = detached_head_note(self.git.status.as_ref());
        let behind_note = behind_upstream_note(self.git.status.as_ref());
        let status_error = self.git.status_error.clone();
        let busy = self.git.busy;
        let chat = cx.entity().downgrade();
        let menu_trigger = Button::new("git-menu")
            .outline()
            .xsmall()
            .icon(Icon::new(IconName::ChevronDown).with_size(px(16.)))
            .tooltip("Git action options")
            .disabled(busy)
            .dropdown_menu(move |mut menu, _window, cx| {
                // Opening the menu fires a status refresh, per Electron.
                let _ = chat.update(cx, |this, cx| this.git_refresh_status(cx));
                for item in &menu_items {
                    let entry = item.entry;
                    let icon = match entry {
                        GitMenuEntry::Commit => Icon::new(VitreIcon::GitCommitHorizontal),
                        GitMenuEntry::Push => Icon::new(VitreIcon::CloudUpload),
                        GitMenuEntry::ViewPr | GitMenuEntry::CreatePr => {
                            Icon::new(VitreIcon::GitPullRequest)
                        }
                    };
                    let chat = chat.clone();
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(item.label.clone()))
                            .icon(icon)
                            .disabled(!item.enabled)
                            .on_click(move |_, window, cx| {
                                let _ = chat.update(cx, |this, cx| {
                                    this.git_menu_action(entry, window, cx);
                                });
                            }),
                    );
                }
                if show_publish {
                    menu = menu.item(
                        PopupMenuItem::new("Publish repository...")
                            .icon(Icon::new(VitreIcon::CloudUpload))
                            .disabled(busy)
                            .on_click(move |_, window, cx| {
                                window.push_notification(
                                    Notification::info(
                                        "Publish repository is not yet supported in Vitre.",
                                    ),
                                    cx,
                                );
                            }),
                    );
                }
                if detached_note {
                    menu = menu.item(PopupMenuItem::label(
                        "Detached HEAD: create and checkout a refName to enable push and pull request actions.",
                    ));
                }
                if behind_note {
                    menu = menu.item(PopupMenuItem::label("Behind upstream. Pull/rebase first."));
                }
                if let Some(error) = &status_error {
                    menu = menu.item(PopupMenuItem::label(error.clone()));
                }
                menu
            });

        Some(
            h_flex()
                .gap_1()
                .flex_shrink_0()
                .items_center()
                .child(quick_button)
                .child(menu_trigger)
                .into_any_element(),
        )
    }

    // ---------------------------------------------------------------- actions

    fn git_quick_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let quick = resolve_quick_action(self.git.status.as_ref(), self.git.busy);
        match quick.run {
            QuickActionRun::ShowHint => {
                window.push_notification(
                    Notification::info(SharedString::from(
                        quick
                            .hint
                            .unwrap_or_else(|| "This action is currently unavailable.".into()),
                    ))
                    .title(SharedString::from(quick.label)),
                    cx,
                );
            }
            QuickActionRun::OpenPr => self.git_open_pr(window, cx),
            QuickActionRun::OpenPublish => {
                window.push_notification(
                    Notification::info("Publish repository is not yet supported in Vitre."),
                    cx,
                );
            }
            QuickActionRun::Pull => self.run_git_pull(cx),
            QuickActionRun::StackedAction(action) => {
                self.run_git_action(action, GitRunOptions::default(), window, cx);
            }
        }
    }

    fn git_menu_action(
        &mut self,
        entry: GitMenuEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match entry {
            GitMenuEntry::Commit => self.open_commit_dialog(window, cx),
            GitMenuEntry::Push => {
                self.run_git_action(GitStackedAction::Push, GitRunOptions::default(), window, cx)
            }
            GitMenuEntry::ViewPr => self.git_open_pr(window, cx),
            GitMenuEntry::CreatePr => self.run_git_action(
                GitStackedAction::CreatePr,
                GitRunOptions::default(),
                window,
                cx,
            ),
        }
    }

    fn git_open_pr(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self
            .git
            .status
            .as_ref()
            .filter(|status| status.has_open_pr())
            .and_then(|status| status.pr.as_ref())
            .map(|pr| pr.url.clone());
        match url {
            Some(url) => cx.open_url(&url),
            None => {
                window.push_notification(Notification::error("No open pull request found."), cx)
            }
        }
    }

    fn git_init(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let (Some(client), Some(cwd)) = (self.client.clone(), self.git.cwd.clone()) else {
            return;
        };
        if self.git.busy {
            return;
        }
        self.git.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let payload = VcsInitInput {
                cwd: tnes(&cwd),
                kind: None,
            };
            let result = client.call::<VcsInit>(&payload).await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.git.busy = false;
                if let Err(error) = result {
                    window.push_notification(
                        Notification::error(SharedString::from(format!("{error:?}")))
                            .title("Git initialization failed"),
                        cx,
                    );
                }
                // Success is silent: the status stream flips the UI.
                cx.notify();
            });
        })
        .detach();
    }

    fn run_git_pull(&mut self, cx: &mut Context<Self>) {
        let (Some(client), Some(cwd)) = (self.client.clone(), self.git.cwd.clone()) else {
            return;
        };
        if self.git.busy {
            return;
        }
        self.git.busy = true;
        self.git.action_seq += 1;
        let seq = self.git.action_seq;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let _ = this.update_in(cx, |_, window, cx| {
                window.push_notification(
                    Notification::info("Waiting for Git...")
                        .title("Pulling...")
                        .id1::<GitToastTag>(GIT_TOAST_KEY)
                        .autohide(false),
                    cx,
                );
            });
            let payload = VcsPullInput { cwd: tnes(&cwd) };
            let result = client.call::<VcsPull>(&payload).await;
            let _ = this.update_in(cx, |app, window, cx| {
                if app.git.action_seq == seq {
                    app.git.busy = false;
                }
                let note = match result {
                    Ok(result) => {
                        app.git_refresh_status(cx);
                        let ref_name = result.ref_name.0;
                        match result.status {
                            VcsPullResultStatus::Pulled => {
                                let upstream = result
                                    .upstream_ref
                                    .map(|upstream| upstream.0)
                                    .unwrap_or_else(|| "upstream".into());
                                Notification::success(SharedString::from(format!(
                                    "Updated {ref_name} from {upstream}"
                                )))
                                .title("Pulled")
                            }
                            _ => Notification::success(SharedString::from(format!(
                                "{ref_name} is already synchronized."
                            )))
                            .title("Already up to date"),
                        }
                    }
                    Err(error) => Notification::error(SharedString::from(format!("{error:?}")))
                        .title("Pull failed"),
                };
                window.push_notification(note.id1::<GitToastTag>(GIT_TOAST_KEY), cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// `runGitActionWithToast`, minus the per-repo command queue (Vitre
    /// disables the controls while busy instead).
    fn run_git_action(
        &mut self,
        action: GitStackedAction,
        opts: GitRunOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(client), Some(cwd)) = (self.client.clone(), self.git.cwd.clone()) else {
            return;
        };
        if self.git.busy {
            return;
        }
        let status = self.git.status.clone();
        let terminology = status_terminology(status.as_ref());
        let tree_dirty = status
            .as_ref()
            .is_some_and(|status| status.has_working_tree_changes);
        // `featureBranch: true` forces the default-branch answer to "no".
        let is_default_ref =
            !opts.feature_branch && status.as_ref().is_some_and(|status| status.is_default_ref);
        let includes_commit = action_includes_commit(&action, tree_dirty, opts.feature_branch);

        // Default-branch gate: stash the action and confirm instead of
        // running. The commit dialog (if any) stays open beneath.
        if !opts.skip_default_branch_prompt
            && requires_default_branch_confirmation(&action, is_default_ref)
            && let Some(branch) = status.as_ref().and_then(|status| status.ref_name.clone())
        {
            let copy = default_branch_dialog_copy(&action, &branch, includes_commit, &terminology);
            self.git.pending_default_branch = Some(PendingDefaultBranchAction {
                action,
                commit_message: opts.commit_message,
                file_paths: opts.file_paths,
                from_commit_dialog: opts.from_commit_dialog,
            });
            self.open_default_branch_dialog(copy, window, cx);
            return;
        }
        // The action really starts now: close the commit dialog if it
        // initiated this run (Electron's `onConfirmed`).
        if opts.from_commit_dialog {
            self.git.commit_dialog = None;
            window.close_dialog(cx);
        }

        let should_push_before_pr = status
            .as_ref()
            .is_none_or(|status| !status.has_upstream || status.ahead_count > 0);
        let stages = build_progress_stages(
            &action,
            opts.feature_branch,
            includes_commit,
            opts.commit_message.is_some(),
            should_push_before_pr,
            &terminology,
        );
        self.git.busy = true;
        self.git.action_seq += 1;
        let seq = self.git.action_seq;
        let title: SharedString = stages
            .first()
            .cloned()
            .unwrap_or_else(|| "Running git action...".into())
            .into();
        self.git.progress = Some(GitProgress {
            seq,
            title: title.clone(),
            current_phase_label: None,
            last_output_line: None,
            phase_started_at: Some(Instant::now()),
            hook_started_at: None,
        });
        window.push_notification(
            Notification::info("Waiting for Git...")
                .title(title)
                .id1::<GitToastTag>(GIT_TOAST_KEY)
                .autohide(false),
            cx,
        );
        cx.notify();

        // 1s ticker rewriting the toast description while this action owns
        // the toast slot.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let live = this
                    .update_in(cx, |app, window, cx| {
                        let owns = app
                            .git
                            .progress
                            .as_ref()
                            .is_some_and(|progress| progress.seq == seq);
                        if owns {
                            app.push_git_progress_toast(window, cx);
                        }
                        owns
                    })
                    .unwrap_or(false);
                if !live {
                    return;
                }
            }
        })
        .detach();

        // The action itself.
        let local_id = fresh_id("git-action");
        cx.spawn(async move |this, cx| {
            let handle = client.sessions().borrow().clone();
            let Some(handle) = handle else {
                let _ = this.update_in(cx, |app, window, cx| {
                    app.finish_git_action(seq, Err("Not connected.".into()), window, cx);
                });
                return;
            };
            let environment_id = handle.config.environment.environment_id.0.clone();
            let transport_id = transport_action_id(&environment_id, &cwd, &local_id);
            let input = GitRunStackedActionInput {
                action,
                action_id: tnes(&transport_id),
                commit_message: opts
                    .commit_message
                    .as_deref()
                    .map(|message| Some(tnes(message))),
                cwd: tnes(&cwd),
                feature_branch: opts.feature_branch.then_some(Some(true)),
                file_paths: opts
                    .file_paths
                    .as_ref()
                    .map(|paths| Some(paths.iter().map(tnes).collect())),
            };
            let Ok(mut subscription) = handle
                .session
                .subscribe_typed::<GitRunStackedAction>(&input)
            else {
                let _ = this.update_in(cx, |app, window, cx| {
                    app.finish_git_action(
                        seq,
                        Err("The environment request failed.".into()),
                        window,
                        cx,
                    );
                });
                return;
            };
            let mut outcome: Option<Result<GitRunStackedActionResult, String>> = None;
            loop {
                match subscription.next().await {
                    Some(TypedStreamEvent::Values(events)) => {
                        let mut relevant: Vec<GitActionProgressEvent> = Vec::new();
                        for event in events {
                            let Some((event_id, event_cwd)) = progress_event_ids(&event) else {
                                continue;
                            };
                            if event_id != transport_id || event_cwd != cwd {
                                continue;
                            }
                            match &event {
                                // Terminal events never touch the toast — the
                                // settled outcome publishes the final state.
                                GitActionProgressEvent::ActionFinished { result, .. } => {
                                    outcome = Some(Ok(result.clone()));
                                }
                                GitActionProgressEvent::ActionFailed { message, .. } => {
                                    outcome = Some(Err(message.0.clone()));
                                }
                                _ => relevant.push(event),
                            }
                        }
                        if !relevant.is_empty()
                            && this
                                .update_in(cx, |app, window, cx| {
                                    for event in &relevant {
                                        app.apply_git_progress_event(seq, event);
                                    }
                                    app.push_git_progress_toast(window, cx);
                                })
                                .is_err()
                        {
                            return;
                        }
                        if subscription.ack().is_err() {
                            break;
                        }
                    }
                    Some(TypedStreamEvent::Completed(result)) => {
                        if let Err(error) = result
                            && outcome.is_none()
                        {
                            outcome = Some(Err(format!("{error:?}")));
                        }
                        break;
                    }
                    None => break,
                }
            }
            let outcome =
                outcome.unwrap_or_else(|| Err("Git action did not report a result.".into()));
            let _ = this.update_in(cx, |app, window, cx| {
                app.finish_git_action(seq, outcome, window, cx);
            });
        })
        .detach();
    }

    /// The progress-event switch (`applyProgressEvent`).
    fn apply_git_progress_event(&mut self, seq: u64, event: &GitActionProgressEvent) {
        let Some(progress) = self.git.progress.as_mut() else {
            return;
        };
        if progress.seq != seq {
            return;
        }
        match event {
            GitActionProgressEvent::ActionStarted { .. } => {
                progress.phase_started_at = Some(Instant::now());
                progress.hook_started_at = None;
                progress.last_output_line = None;
            }
            GitActionProgressEvent::PhaseStarted { label, .. } => {
                let label: SharedString = label.0.clone().into();
                progress.title = label.clone();
                progress.current_phase_label = Some(label);
                progress.phase_started_at = Some(Instant::now());
                progress.hook_started_at = None;
                progress.last_output_line = None;
            }
            GitActionProgressEvent::HookStarted { hook_name, .. } => {
                progress.title = format!("Running {}...", hook_name.0).into();
                progress.hook_started_at = Some(Instant::now());
            }
            GitActionProgressEvent::HookOutput { text, .. } => {
                progress.last_output_line = Some(text.0.clone());
            }
            GitActionProgressEvent::HookFinished { .. } => {
                progress.title = progress
                    .current_phase_label
                    .clone()
                    .unwrap_or_else(|| "Committing...".into());
                progress.hook_started_at = None;
            }
            _ => {}
        }
    }

    fn push_git_progress_toast(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(progress) = self.git.progress.as_ref() else {
            return;
        };
        let elapsed = progress
            .hook_started_at
            .or(progress.phase_started_at)
            .map(|instant| instant.elapsed().as_secs());
        let description =
            resolve_progress_description(progress.last_output_line.as_deref(), elapsed)
                .unwrap_or_else(|| "Waiting for Git...".into());
        window.push_notification(
            Notification::info(SharedString::from(description))
                .title(progress.title.clone())
                .id1::<GitToastTag>(GIT_TOAST_KEY)
                .autohide(false),
            cx,
        );
    }

    fn finish_git_action(
        &mut self,
        seq: u64,
        outcome: Result<GitRunStackedActionResult, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Tear down only when this action still owns the slot.
        if self.git.action_seq == seq {
            self.git.busy = false;
            self.git.progress = None;
        }
        match outcome {
            Ok(result) => {
                if let Some(branch) = resolve_thread_branch_update(&result) {
                    self.persist_thread_branch(branch, cx);
                }
                self.git_refresh_status(cx);
                // Success copy comes from the SERVER (`result.toast`).
                let title: SharedString = result.toast.title.0.clone().into();
                let description: SharedString = result
                    .toast
                    .description
                    .clone()
                    .flatten()
                    .map(|description| description.0)
                    .unwrap_or_default()
                    .into();
                let mut note = Notification::success(description)
                    .title(title)
                    .id1::<GitToastTag>(GIT_TOAST_KEY);
                match &result.toast.cta {
                    GitRunStackedActionResultToastCta::OpenPr { label, url } => {
                        let label: SharedString = label.0.clone().into();
                        let url = url.0.clone();
                        note = note.action(move |_, _, _| {
                            let url = url.clone();
                            Button::new("git-toast-cta")
                                .outline()
                                .small()
                                .label(label.clone())
                                .on_click(move |_, window, cx| {
                                    window.remove_notification1::<GitToastTag>(GIT_TOAST_KEY, cx);
                                    cx.open_url(&url);
                                })
                        });
                    }
                    GitRunStackedActionResultToastCta::RunAction { action, label } => {
                        let label: SharedString = label.0.clone().into();
                        let kind = action.kind.clone();
                        let chat = cx.entity().downgrade();
                        note = note.action(move |_, _, _| {
                            let kind = kind.clone();
                            let chat = chat.clone();
                            Button::new("git-toast-cta")
                                .outline()
                                .small()
                                .label(label.clone())
                                .on_click(move |_, window, cx| {
                                    window.remove_notification1::<GitToastTag>(GIT_TOAST_KEY, cx);
                                    let kind = kind.clone();
                                    let _ = chat.update(cx, |this, cx| {
                                        this.run_git_action(
                                            kind,
                                            GitRunOptions::default(),
                                            window,
                                            cx,
                                        );
                                    });
                                })
                        });
                    }
                    _ => {}
                }
                window.push_notification(note, cx);
            }
            Err(message) => {
                window.push_notification(
                    Notification::error(SharedString::from(message))
                        .title("Action failed")
                        .id1::<GitToastTag>(GIT_TOAST_KEY),
                    cx,
                );
            }
        }
        cx.notify();
    }

    /// `persistThreadBranchSync`: adopt a created feature branch on the open
    /// thread (primary root only — the only root Vitre drives today).
    fn persist_thread_branch(&mut self, branch: String, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(open) = self.thread.as_ref() else {
            return;
        };
        let previous = self
            .shell_thread(&open.id)
            .and_then(|thread| thread.branch.as_ref().map(|branch| branch.0.clone()));
        if previous.as_deref() == Some(branch.as_str()) {
            return;
        }
        let command = ClientOrchestrationCommand::ThreadMetaUpdate {
            additional_roots: None,
            branch: Some(Some(Some(tnes(&branch)))),
            command_id: CommandId(fresh_id("vitre-cmd")),
            expected_branch: Some(Some(previous.as_deref().map(tnes))),
            model_selection: None,
            thread_id: open.id.clone(),
            title: None,
            r#type: Default::default(),
            worktree_path: None,
        };
        cx.spawn(async move |_, _| {
            let _ = client.dispatch(&command).await;
        })
        .detach();
    }

    // ---------------------------------------------------------------- dialogs

    fn open_commit_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let message =
            cx.new(|cx| TextareaState::new(window, cx).placeholder("Leave empty to auto-generate"));
        self.git.commit_dialog = Some(CommitDialogState {
            message,
            excluded: HashSet::new(),
            editing: false,
        });
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let content_owner = owner.clone();
            let close_owner = owner.clone();
            let cancel_owner = owner.clone();
            let feature_owner = owner.clone();
            let commit_owner = owner.clone();
            // The footer's enablement tracks the live selection.
            let none_selected = owner
                .upgrade()
                .map(|chat| {
                    let chat = chat.read(cx);
                    let Some(dialog) = chat.git.commit_dialog.as_ref() else {
                        return true;
                    };
                    let Some(status) = chat.git.status.as_ref() else {
                        return true;
                    };
                    status
                        .working_tree
                        .files
                        .iter()
                        .all(|file| dialog.excluded.contains(&file.path))
                })
                .unwrap_or(true);
            dialog
                .title("Commit changes")
                .w(px(560.))
                .on_close(move |_, _, cx| {
                    let _ = close_owner.update(cx, |this, _| this.git.commit_dialog = None);
                })
                .content(move |content, _window, cx| {
                    let Some(chat) = content_owner.upgrade() else {
                        return content;
                    };
                    let body = chat.update(cx, |this, cx| this.render_commit_dialog_body(cx));
                    content.child(body)
                })
                .footer(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .justify_end()
                        .child(
                            Button::new("commit-cancel")
                                .outline()
                                .small()
                                .label("Cancel")
                                .on_click(move |_, window, cx| {
                                    let _ = cancel_owner
                                        .update(cx, |this, _| this.git.commit_dialog = None);
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("commit-feature")
                                .outline()
                                .small()
                                .label("Commit on new refName")
                                .disabled(none_selected)
                                .on_click(move |_, window, cx| {
                                    let _ = feature_owner.update(cx, |this, cx| {
                                        this.submit_commit_dialog(true, window, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("commit-run")
                                .primary()
                                .small()
                                .label("Commit")
                                .disabled(none_selected)
                                .on_click(move |_, window, cx| {
                                    let _ = commit_owner.update(cx, |this, cx| {
                                        this.submit_commit_dialog(false, window, cx);
                                    });
                                }),
                        ),
                )
        });
    }

    fn render_commit_dialog_body(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.git.commit_dialog.as_ref() else {
            return div().into_any_element();
        };
        let theme = cx.theme().clone();
        let status = self.git.status.as_ref();
        let ref_name: SharedString = status
            .and_then(|status| status.ref_name.clone())
            .unwrap_or_else(|| "(detached HEAD)".into())
            .into();
        let is_default_ref = status.is_some_and(|status| status.is_default_ref);
        let files: Vec<(String, i64, i64)> = status
            .map(|status| {
                status
                    .working_tree
                    .files
                    .iter()
                    .map(|file| (file.path.clone(), file.insertions, file.deletions))
                    .collect()
            })
            .unwrap_or_default();
        let excluded = dialog.excluded.clone();
        let editing = dialog.editing;
        let selected: Vec<&(String, i64, i64)> = files
            .iter()
            .filter(|(path, ..)| !excluded.contains(path))
            .collect();
        let (sel_ins, sel_del) = selected
            .iter()
            .fold((0, 0), |(ins, del), (_, i, d)| (ins + i, del + d));
        let partial = selected.len() < files.len();
        let selected_count = selected.len();
        let total_count = files.len();
        let message_state = dialog.message.clone();

        let files_header = h_flex()
            .items_center()
            .gap_2()
            .child(div().text_xs().font_medium().child("Files"))
            .when(partial && !editing, |this| {
                this.child(div().text_xs().text_color(theme.muted_foreground).child(
                    SharedString::from(format!("({selected_count} of {total_count})")),
                ))
            })
            .child(div().flex_1())
            .when(!files.is_empty(), |this| {
                this.child(
                    Button::new("commit-files-edit")
                        .ghost()
                        .xsmall()
                        .label(if editing { "Done" } else { "Edit" })
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(dialog) = this.git.commit_dialog.as_mut() {
                                dialog.editing = !dialog.editing;
                                cx.notify();
                            }
                        })),
                )
            });

        let mut list = v_flex()
            .max_h(px(176.))
            .id("commit-file-list")
            .overflow_y_scroll()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border.opacity(0.6))
            .bg(theme.secondary)
            .p_1();
        if files.is_empty() {
            list = list.child(
                div()
                    .px_2()
                    .py_1()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("none"),
            );
        }
        for (index, (path, insertions, deletions)) in files.iter().enumerate() {
            let path = path.clone();
            let is_excluded = excluded.contains(&path);
            let toggle_path = path.clone();
            let row = h_flex()
                .id(("commit-file", index))
                .px_2()
                .py_0p5()
                .gap_2()
                .items_center()
                .rounded(px(6.))
                .hover(|style| style.bg(theme.accent.opacity(0.5)))
                .font_family(theme.mono_font_family.clone())
                .when(editing, |this| {
                    this.child(
                        Checkbox::new(("commit-file-check", index))
                            .checked(!is_excluded)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(dialog) = this.git.commit_dialog.as_mut() {
                                    if !dialog.excluded.remove(&toggle_path) {
                                        dialog.excluded.insert(toggle_path.clone());
                                    }
                                    cx.notify();
                                }
                            })),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .when(is_excluded, |this| this.text_color(theme.muted_foreground))
                        .child(SharedString::from(path.clone())),
                )
                .map(|this| {
                    if is_excluded {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Excluded"),
                        )
                    } else {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme.success)
                                .child(SharedString::from(format!("+{insertions}"))),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground.opacity(0.6))
                                .child("/"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.danger)
                                .child(SharedString::from(format!("-{deletions}"))),
                        )
                    }
                });
            list = list.child(row);
        }

        v_flex()
            .gap_3()
            .child(div().text_sm().text_color(theme.muted_foreground).child(
                "Review and confirm your commit. Leave the message blank to auto-generate one.",
            ))
            .child(
                // Summary card: the branch row (+ default-ref warning).
                h_flex()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(theme.border.opacity(0.6))
                    .bg(theme.muted.opacity(0.4))
                    .px_3()
                    .py_2()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("Branch"),
                    )
                    .child(div().text_sm().font_medium().child(ref_name))
                    .when(is_default_ref, |this| {
                        this.child(div().flex_1()).child(
                            div()
                                .text_xs()
                                .text_color(theme.warning)
                                .child("Warning: default refName"),
                        )
                    }),
            )
            .child(
                v_flex().gap_1p5().child(files_header).child(list).child(
                    h_flex().justify_end().child(
                        div()
                            .text_xs()
                            .font_family(theme.mono_font_family.clone())
                            .text_color(theme.muted_foreground)
                            .child(SharedString::from(format!("+{sel_ins} / -{sel_del}"))),
                    ),
                ),
            )
            .child(
                v_flex()
                    .gap_1p5()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .child("Commit message (optional)"),
                    )
                    .child(Textarea::new(&message_state)),
            )
            .into_any_element()
    }

    fn submit_commit_dialog(
        &mut self,
        feature_branch: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.git.commit_dialog.as_ref() else {
            return;
        };
        let files: Vec<String> = self
            .git
            .status
            .as_ref()
            .map(|status| {
                status
                    .working_tree
                    .files
                    .iter()
                    .map(|file| file.path.clone())
                    .collect()
            })
            .unwrap_or_default();
        let selected: Vec<String> = files
            .iter()
            .filter(|path| !dialog.excluded.contains(*path))
            .cloned()
            .collect();
        if selected.is_empty() {
            return;
        }
        // filePaths only when a strict subset is selected.
        let file_paths = (selected.len() < files.len()).then_some(selected);
        let message = dialog.message.read(cx).value().trim().to_string();
        let commit_message = (!message.is_empty()).then_some(message);
        self.run_git_action(
            GitStackedAction::Commit,
            GitRunOptions {
                commit_message,
                skip_default_branch_prompt: feature_branch,
                feature_branch,
                file_paths,
                from_commit_dialog: true,
            },
            window,
            cx,
        );
    }

    fn open_default_branch_dialog(
        &mut self,
        copy: DefaultBranchDialogCopy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = cx.entity().downgrade();
        let title: SharedString = copy.title.into();
        let description: SharedString = copy.description.into();
        let continue_label: SharedString = copy.continue_label.into();
        window.open_dialog(cx, move |dialog, _, _| {
            let abort_owner = owner.clone();
            let continue_owner = owner.clone();
            let feature_owner = owner.clone();
            let description = description.clone();
            dialog
                .title(title.clone())
                .w(px(576.))
                .content(move |content, _, cx| {
                    let description = description.clone();
                    content.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(description),
                    )
                })
                .footer(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .child(
                            Button::new("default-branch-abort")
                                .outline()
                                .small()
                                .label("Abort")
                                .on_click(move |_, window, cx| {
                                    let _ = abort_owner.update(cx, |this, _| {
                                        this.git.pending_default_branch = None;
                                    });
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("default-branch-continue")
                                .outline()
                                .small()
                                .label(continue_label.clone())
                                .on_click({
                                    let owner = continue_owner.clone();
                                    move |_, window, cx| {
                                        window.close_dialog(cx);
                                        let _ = owner.update(cx, |this, cx| {
                                            this.resume_default_branch_action(false, window, cx);
                                        });
                                    }
                                }),
                        )
                        .child(
                            Button::new("default-branch-feature")
                                .primary()
                                .small()
                                .label("Checkout feature branch & continue")
                                .on_click({
                                    let owner = feature_owner.clone();
                                    move |_, window, cx| {
                                        window.close_dialog(cx);
                                        let _ = owner.update(cx, |this, cx| {
                                            this.resume_default_branch_action(true, window, cx);
                                        });
                                    }
                                }),
                        ),
                )
        });
    }

    fn resume_default_branch_action(
        &mut self,
        feature_branch: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.git.pending_default_branch.take() else {
            return;
        };
        self.run_git_action(
            pending.action,
            GitRunOptions {
                commit_message: pending.commit_message,
                skip_default_branch_prompt: true,
                feature_branch,
                file_paths: pending.file_paths,
                from_commit_dialog: pending.from_commit_dialog,
            },
            window,
            cx,
        );
    }
}
