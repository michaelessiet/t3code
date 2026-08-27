//! M1 chat core UI: thread list sidebar + chat view + composer, rendered
//! from the `vitre-client` watch channels.
//!
//! Detail views merge SHELL data with the detail projection: `subscribeThread`
//! only carries the six detail event kinds, so title/meta always come from
//! the thread shell (the TS client's `mergeEnvironmentThread` split).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui::{
    Context, Entity, ScrollHandle, SharedString, Subscription, Window, div, prelude::*, px,
    relative,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, ResizableState, Sizable as _,
    StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex, h_resizable,
    input::{InputEvent, Textarea, TextareaState},
    menu::{DropdownMenu as _, PopupMenuItem},
    resizable_panel,
    text::TextView,
    v_flex,
};
use gpui_tokio::Tokio;
use tokio::sync::watch;
use vitre_client::{EnvironmentClient, ShellState, SyncPhase, ThreadHandle, ThreadState};
use vitre_contracts::{
    ApprovalRequestId, ClientOrchestrationCommand, CommandId, MessageId, ModelSelection,
    NonNegativeInt, OrchestrationMessage, OrchestrationMessageRole, OrchestrationSessionStatus,
    OrchestrationThread, OrchestrationThreadActivity, OrchestrationThreadActivityTone,
    OrchestrationThreadShell, ProviderApprovalDecision, ProviderInteractionMode, RuntimeMode,
    ServerConfig, ThreadId, TrimmedNonEmptyString,
};
use vitre_sidecar::SupervisorStatus;
use vitre_state::session_logic::{
    ActivePlanState, ApprovalRequestKind, PendingApproval, PendingUserInput, PlanStepStatus,
    derive_active_plan_state, derive_pending_approvals, derive_pending_user_inputs,
};

pub struct ChatApp {
    client: Option<Arc<EnvironmentClient>>,
    sidecar_status: SharedString,
    shell: ShellState,
    thread: Option<OpenThread>,
    composer: Entity<TextareaState>,
    last_error: Option<SharedString>,
    /// Request ids with an approval / user-input response in flight.
    responding: HashSet<String>,
    /// Local draft state for the active pending user-input request.
    input_draft: Option<InputDraft>,
    /// Two-step revert confirm: the user message armed for revert.
    pending_revert: Option<MessageId>,
    /// A `ThreadCheckpointRevert` is in flight.
    reverting: bool,
    /// Timeline scroll anchoring: keep pinned to the bottom while streaming
    /// unless the user scrolled away (Electron's `timelineScrollAnchoring`).
    timeline_scroll: ScrollHandle,
    stick_to_bottom: bool,
    /// Sidebar ⟷ chat split state (drag-resizable, Electron parity).
    sidebar_resize: Entity<ResizableState>,
    _subscriptions: Vec<Subscription>,
}

/// Draft answers for the front pending user-input request, keyed by its
/// request id so a new request starts clean (Electron keys drafts the same
/// way, per request id).
struct InputDraft {
    request_id: String,
    question_index: usize,
    selections: HashMap<String, Vec<String>>,
}

struct OpenThread {
    id: ThreadId,
    /// Keeps the sync task alive; dropped (aborted) on reselection.
    _handle: ThreadHandle,
    state: ThreadState,
}

fn tnes(text: impl Into<String>) -> TrimmedNonEmptyString {
    TrimmedNonEmptyString(text.into())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn fresh_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    )
}

/// Default model for a new thread: the first enabled+installed provider's
/// default model (else its first model). Used when the project carries no
/// `default_model_selection` of its own.
fn default_model_selection(config: &ServerConfig) -> Option<ModelSelection> {
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.enabled && provider.installed)
        .or_else(|| config.providers.first())?;
    let model = provider
        .models
        .iter()
        .find(|model| matches!(model.is_default, Some(Some(true))))
        .or_else(|| provider.models.first())?;
    Some(ModelSelection {
        instance_id: Some(Some(serde_json::Value::String(
            provider.instance_id.0.clone(),
        ))),
        model: serde_json::Value::String(model.slug.0.clone()),
        options: None,
        provider: None,
    })
}

/// Compact relative timestamp for sidebar rows ("now", "5m", "2h", "3d").
fn relative_time(iso: &str) -> Option<String> {
    let then = chrono::DateTime::parse_from_rfc3339(iso).ok()?;
    let delta = chrono::Utc::now().signed_duration_since(then.with_timezone(&chrono::Utc));
    Some(if delta.num_minutes() < 1 {
        "now".into()
    } else if delta.num_hours() < 1 {
        format!("{}m", delta.num_minutes())
    } else if delta.num_days() < 1 {
        format!("{}h", delta.num_hours())
    } else {
        format!("{}d", delta.num_days())
    })
}

fn activity_icon(kind: &str) -> IconName {
    if kind.contains("terminal") || kind.contains("bash") || kind.contains("shell") {
        IconName::SquareTerminal
    } else if kind.contains("web") || kind.contains("fetch") || kind.contains("url") {
        IconName::Globe
    } else if kind.contains("read") || kind.contains("view") || kind.contains("search") {
        IconName::Eye
    } else if kind.contains("edit") || kind.contains("write") || kind.contains("file") {
        IconName::File
    } else {
        IconName::Settings2
    }
}

/// A turn's timeline interleaves messages and activity (tool) rows in
/// creation order, exactly like the Electron `MessagesTimeline`.
enum TimelineEntry<'a> {
    Message(&'a OrchestrationMessage),
    Activity(&'a OrchestrationThreadActivity),
}

impl TimelineEntry<'_> {
    fn created_at(&self) -> &str {
        match self {
            TimelineEntry::Message(message) => &message.created_at.0,
            TimelineEntry::Activity(activity) => &activity.created_at.0,
        }
    }
}

fn timeline_entries(view: &OrchestrationThread) -> Vec<TimelineEntry<'_>> {
    let mut entries: Vec<TimelineEntry> = view
        .messages
        .iter()
        .map(TimelineEntry::Message)
        .chain(view.activities.iter().map(TimelineEntry::Activity))
        .collect();
    // RFC3339 timestamps with fixed millisecond precision sort lexically.
    entries.sort_by(|a, b| a.created_at().cmp(b.created_at()));
    entries
}

impl ChatApp {
    pub fn new(
        status_rx: watch::Receiver<SupervisorStatus>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Ask anything, @tag files/folders, $use skills, or / for commands")
                .auto_grow(1, 8)
        });
        let subscriptions = vec![cx.subscribe_in(
            &composer,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { shift: false, .. } = event {
                    this.send(window, cx);
                }
            },
        )];

        // Sidecar status line for the footer.
        cx.spawn({
            let mut status_rx = status_rx.clone();
            async move |this, cx| {
                loop {
                    let line: SharedString = describe_status(&status_rx.borrow_and_update()).into();
                    if this
                        .update(cx, |app, cx| {
                            app.sidecar_status = line;
                            cx.notify();
                        })
                        .is_err()
                    {
                        return;
                    }
                    if status_rx.changed().await.is_err() {
                        return;
                    }
                }
            }
        })
        .detach();

        // Start the environment client on the tokio runtime, then follow the
        // shell channel for the lifetime of the window.
        cx.spawn(async move |this, cx| {
            let client = cx
                .update(|cx| {
                    Tokio::spawn_result(cx, async move {
                        Ok(Arc::new(EnvironmentClient::start(status_rx)))
                    })
                })
                .await;
            let client = match client {
                Ok(client) => client,
                Err(error) => {
                    let _ = this.update(cx, |app, cx| {
                        app.last_error = Some(format!("client start failed: {error:#}").into());
                        cx.notify();
                    });
                    return;
                }
            };
            let mut shell_rx = client.shell();
            if this
                .update(cx, |app, cx| {
                    app.client = Some(client);
                    cx.notify();
                })
                .is_err()
            {
                return;
            }
            loop {
                let state = shell_rx.borrow_and_update().clone();
                if this
                    .update(cx, |app, cx| {
                        app.shell = state;
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
                if shell_rx.changed().await.is_err() {
                    return;
                }
            }
        })
        .detach();

        Self {
            client: None,
            sidecar_status: "starting…".into(),
            shell: ShellState::default(),
            thread: None,
            composer,
            last_error: None,
            responding: HashSet::new(),
            input_draft: None,
            pending_revert: None,
            reverting: false,
            timeline_scroll: ScrollHandle::new(),
            stick_to_bottom: true,
            sidebar_resize: cx.new(|_| ResizableState::default()),
            _subscriptions: subscriptions,
        }
    }

    fn new_thread(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(project) = self
            .shell
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.projects.first().cloned())
        else {
            self.last_error = Some("no project yet — open a folder first".into());
            cx.notify();
            return;
        };
        let session = client.sessions().borrow().clone();
        let Some(session) = session else {
            return;
        };
        let Some(model_selection) = project
            .default_model_selection
            .clone()
            .or_else(|| default_model_selection(&session.config))
        else {
            self.last_error = Some("no provider configured".into());
            cx.notify();
            return;
        };
        let thread_id = ThreadId(fresh_id("vitre-thread"));
        let command = ClientOrchestrationCommand::ThreadCreate {
            additional_roots: None,
            branch: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            interaction_mode: None,
            model_selection,
            project_id: project.id.clone(),
            runtime_mode: RuntimeMode::FullAccess,
            thread_id: thread_id.clone(),
            title: tnes("New thread"),
            r#type: Default::default(),
            worktree_path: None,
        };
        self.last_error = None;
        cx.spawn(
            async move |this, cx| match client.dispatch(&command).await {
                Ok(_) => {
                    let _ = this.update(cx, |app, cx| app.select_thread(thread_id, cx));
                }
                Err(error) => {
                    let _ = this.update(cx, |app, cx| {
                        app.last_error = Some(format!("new thread failed: {error:?}").into());
                        cx.notify();
                    });
                }
            },
        )
        .detach();
    }

    fn select_thread(&mut self, id: ThreadId, cx: &mut Context<Self>) {
        if self.thread.as_ref().is_some_and(|open| open.id == id) {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let handle = client.open_thread(id.clone());
        let mut state_rx = handle.state();
        self.input_draft = None;
        self.pending_revert = None;
        self.stick_to_bottom = true;
        self.timeline_scroll.scroll_to_bottom();
        self.thread = Some(OpenThread {
            id: id.clone(),
            _handle: handle,
            state: ThreadState::default(),
        });
        cx.spawn(async move |this, cx| {
            loop {
                let state = state_rx.borrow_and_update().clone();
                let stop = this
                    .update(cx, |app, cx| {
                        if app.thread.as_ref().is_none_or(|open| open.id != id) {
                            return true;
                        }
                        if let Some(open) = &mut app.thread {
                            open.state = state;
                        }
                        // New content keeps the timeline pinned to the bottom
                        // unless the user scrolled away.
                        if app.stick_to_bottom {
                            app.timeline_scroll.scroll_to_bottom();
                        }
                        cx.notify();
                        false
                    })
                    .unwrap_or(true);
                if stop || state_rx.changed().await.is_err() {
                    return;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(open) = &self.thread else {
            return;
        };
        let Some(view) = open.state.view.clone() else {
            return;
        };
        // While an approval or user-input request is pending, the composer's
        // primary action belongs to that request — don't start a new turn.
        // (Custom free-text answers are a post-parity iteration.)
        if !derive_pending_approvals(&view.activities).is_empty()
            || !derive_pending_user_inputs(&view.activities).is_empty()
        {
            return;
        }
        let text = self.composer.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        self.composer
            .update(cx, |input, cx| input.clean(window, cx));
        self.last_error = None;

        let command = ClientOrchestrationCommand::ThreadTurnStart {
            bootstrap: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            interaction_mode: view
                .interaction_mode
                .clone()
                .flatten()
                .unwrap_or(ProviderInteractionMode::Default),
            message: vitre_contracts::ClientOrchestrationCommandThreadTurnStartMessage {
                attachments: vec![],
                message_id: MessageId(fresh_id("vitre-msg")),
                role: Default::default(),
                text: tnes(text),
            },
            model_selection: None,
            runtime_mode: view.runtime_mode.clone(),
            source_proposed_plan: None,
            thread_id: open.id.clone(),
            title_seed: None,
            r#type: Default::default(),
        };
        cx.spawn(async move |this, cx| {
            if let Err(error) = client.dispatch(&command).await {
                let _ = this.update(cx, |app, cx| {
                    app.last_error = Some(format!("send failed: {error:?}").into());
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn stop_turn(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(open) = &self.thread else {
            return;
        };
        let command = ClientOrchestrationCommand::ThreadTurnInterrupt {
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            thread_id: open.id.clone(),
            turn_id: None,
            r#type: Default::default(),
        };
        cx.spawn(async move |this, cx| {
            if let Err(error) = client.dispatch(&command).await {
                let _ = this.update(cx, |app, cx| {
                    app.last_error = Some(format!("stop failed: {error:?}").into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Dispatch an approval / user-input response, tracking the request id so
    /// its buttons disable while the command is in flight.
    fn dispatch_response(
        &mut self,
        request_id: String,
        command: ClientOrchestrationCommand,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        if !self.responding.insert(request_id.clone()) {
            return;
        }
        self.last_error = None;
        cx.spawn(async move |this, cx| {
            let result = client.dispatch(&command).await;
            let _ = this.update(cx, |app, cx| {
                app.responding.remove(&request_id);
                if let Err(error) = result {
                    app.last_error = Some(format!("response failed: {error:?}").into());
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn respond_approval(
        &mut self,
        request_id: String,
        decision: ProviderApprovalDecision,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = &self.thread else {
            return;
        };
        let command = ClientOrchestrationCommand::ThreadApprovalRespond {
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            decision,
            request_id: ApprovalRequestId(request_id.clone()),
            thread_id: open.id.clone(),
            r#type: Default::default(),
        };
        self.dispatch_response(request_id, command, cx);
    }

    /// The front pending user-input request (the one the panel shows).
    fn active_pending_user_input(&self) -> Option<PendingUserInput> {
        let view = self.thread.as_ref()?.state.view.as_ref()?;
        derive_pending_user_inputs(&view.activities)
            .into_iter()
            .next()
    }

    /// Draft for `request_id`, resetting whenever the front request changes.
    fn input_draft_mut(&mut self, request_id: &str) -> &mut InputDraft {
        if self
            .input_draft
            .as_ref()
            .is_none_or(|draft| draft.request_id != request_id)
        {
            self.input_draft = Some(InputDraft {
                request_id: request_id.to_string(),
                question_index: 0,
                selections: HashMap::new(),
            });
        }
        self.input_draft.as_mut().expect("draft just ensured")
    }

    fn toggle_user_input_option(&mut self, option_label: String, cx: &mut Context<Self>) {
        let Some(pending) = self.active_pending_user_input() else {
            return;
        };
        if self.responding.contains(&pending.request_id) {
            return;
        }
        let draft = self.input_draft_mut(&pending.request_id);
        let index = draft.question_index.min(pending.questions.len() - 1);
        let question = &pending.questions[index];
        let selections = draft.selections.entry(question.id.clone()).or_default();
        if question.multi_select {
            if let Some(position) = selections.iter().position(|label| *label == option_label) {
                selections.remove(position);
            } else {
                selections.push(option_label);
            }
            cx.notify();
        } else {
            // Single-select answers advance immediately (Electron auto-advances
            // 200ms after the click).
            *selections = vec![option_label];
            self.advance_user_input(cx);
        }
    }

    /// Move to the next question, or submit `ThreadUserInputRespond` from the
    /// last one. Answers mirror `resolvePendingUserInputAnswer`: label array
    /// for multi-select questions, single label string otherwise. (Custom
    /// free-text answers are a post-parity iteration.)
    fn advance_user_input(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.active_pending_user_input() else {
            return;
        };
        if self.responding.contains(&pending.request_id) {
            return;
        }
        let Some(open) = &self.thread else {
            return;
        };
        let thread_id = open.id.clone();
        let draft = self.input_draft_mut(&pending.request_id);
        let index = draft.question_index.min(pending.questions.len() - 1);
        if index + 1 < pending.questions.len() {
            draft.question_index = index + 1;
            cx.notify();
            return;
        }
        let mut answers = serde_json::Map::new();
        for question in &pending.questions {
            let labels = draft
                .selections
                .get(&question.id)
                .cloned()
                .unwrap_or_default();
            let value = if question.multi_select {
                serde_json::Value::Array(
                    labels.into_iter().map(serde_json::Value::String).collect(),
                )
            } else if let Some(label) = labels.into_iter().next() {
                serde_json::Value::String(label)
            } else {
                serde_json::Value::Null
            };
            answers.insert(question.id.clone(), value);
        }
        let command = ClientOrchestrationCommand::ThreadUserInputRespond {
            answers,
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            request_id: ApprovalRequestId(pending.request_id.clone()),
            thread_id,
            r#type: Default::default(),
        };
        self.dispatch_response(pending.request_id, command, cx);
    }

    /// Two-step checkpoint revert: the first click arms the message's button
    /// ("Revert?"), the second dispatches `ThreadCheckpointRevert` — standing
    /// in for Electron's native confirm dialog.
    fn revert_user_message(
        &mut self,
        message_id: MessageId,
        turn_count: i64,
        cx: &mut Context<Self>,
    ) {
        if self.reverting {
            return;
        }
        if self.pending_revert.as_ref() != Some(&message_id) {
            self.pending_revert = Some(message_id);
            cx.notify();
            return;
        }
        self.pending_revert = None;
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(open) = &self.thread else {
            return;
        };
        let running = open
            .state
            .view
            .as_ref()
            .and_then(|view| view.session.as_ref())
            .is_some_and(|session| session.status == OrchestrationSessionStatus::Running);
        if running {
            self.last_error =
                Some("Interrupt the current turn before reverting checkpoints.".into());
            cx.notify();
            return;
        }
        let command = ClientOrchestrationCommand::ThreadCheckpointRevert {
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            thread_id: open.id.clone(),
            turn_count: NonNegativeInt(turn_count),
            r#type: Default::default(),
        };
        self.reverting = true;
        self.last_error = None;
        cx.spawn(async move |this, cx| {
            let result = client.dispatch(&command).await;
            let _ = this.update(cx, |app, cx| {
                app.reverting = false;
                if let Err(error) = result {
                    app.last_error = Some(format!("revert failed: {error:?}").into());
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn shell_threads(&self) -> Vec<OrchestrationThreadShell> {
        let Some(snapshot) = &self.shell.snapshot else {
            return Vec::new();
        };
        let mut threads: Vec<_> = snapshot
            .threads
            .iter()
            .filter(|thread| {
                // archived_at: triple Option — only Some(Some(Some(_))) is set.
                !matches!(&thread.archived_at, Some(Some(Some(_))))
            })
            .cloned()
            .collect();
        threads.sort_by(|a, b| b.created_at.0.cmp(&a.created_at.0));
        threads
    }

    /// The SHELL's copy of a thread — canonical for title/meta (meta events
    /// are shell-scoped, so the detail projection never sees them).
    fn shell_thread(&self, id: &ThreadId) -> Option<&OrchestrationThreadShell> {
        self.shell
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.threads.iter().find(|thread| thread.id == *id))
    }

    /// Canonical title comes from the SHELL (meta events are shell-scoped).
    fn thread_title(&self, id: &ThreadId) -> SharedString {
        self.shell_thread(id)
            .map(|thread| thread.title.0.clone().into())
            .unwrap_or_else(|| "(untitled)".into())
    }

    /// Change the open thread's model (`ThreadMetaUpdate`). Electron persists
    /// lazily on next turn start; persisting immediately is equivalent for the
    /// thread's stored selection and keeps the picker stateless.
    fn set_model(&mut self, selection: ModelSelection, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(open) = &self.thread else {
            return;
        };
        let command = ClientOrchestrationCommand::ThreadMetaUpdate {
            additional_roots: None,
            branch: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            expected_branch: None,
            model_selection: Some(Some(selection)),
            thread_id: open.id.clone(),
            title: None,
            r#type: Default::default(),
            worktree_path: None,
        };
        self.last_error = None;
        cx.spawn(async move |this, cx| {
            if let Err(error) = client.dispatch(&command).await {
                let _ = this.update(cx, |app, cx| {
                    app.last_error = Some(format!("model change failed: {error:?}").into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Composer-top approval panel (`ComposerPendingApprovalPanel`): PENDING
    /// APPROVAL eyebrow + kind summary + count, and a mono detail box.
    fn render_approval_panel(
        &self,
        approval: &PendingApproval,
        pending_count: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (summary, detail_label) = match approval.request_kind {
            ApprovalRequestKind::Command => ("Command approval requested", "Command"),
            ApprovalRequestKind::FileRead => ("File-read approval requested", "File to read"),
            ApprovalRequestKind::FileChange => ("File-change approval requested", "File change"),
        };
        let mut panel = v_flex().px_4().py_3p5().child(
            h_flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child("PENDING APPROVAL"),
                )
                .child(div().text_sm().font_medium().child(summary))
                .when(pending_count > 1, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(format!("1/{pending_count}"))),
                    )
                }),
        );
        if let Some(detail) = &approval.detail {
            panel = panel.child(
                v_flex()
                    .mt_3()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background.opacity(0.7))
                    .p_3()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().muted_foreground)
                            .child(detail_label),
                    )
                    .child(
                        div()
                            .id("approval-detail")
                            .mt_2()
                            .max_h(px(160.))
                            .overflow_y_scroll()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_xs()
                            .text_color(cx.theme().foreground)
                            .whitespace_normal()
                            .child(SharedString::from(detail.clone())),
                    ),
            );
        }
        panel.into_any_element()
    }

    /// Composer footer replacement while an approval is pending
    /// (`ComposerPendingApprovalActions`): Cancel turn / Decline / Always
    /// allow this session / Approve once.
    fn render_approval_actions(
        &self,
        approval: &PendingApproval,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let responding = self.responding.contains(&approval.request_id);
        let respond = |decision: ProviderApprovalDecision| {
            let request_id = approval.request_id.clone();
            cx.listener(move |this: &mut Self, _, _, cx| {
                this.respond_approval(request_id.clone(), decision.clone(), cx);
            })
        };
        h_flex()
            .px_3()
            .pb_3()
            .gap_2()
            .items_center()
            .justify_end()
            .child(
                Button::new("approval-cancel")
                    .label("Cancel turn")
                    .ghost()
                    .small()
                    .disabled(responding)
                    .on_click(respond(ProviderApprovalDecision::Cancel)),
            )
            .child(
                Button::new("approval-decline")
                    .label("Decline")
                    .danger()
                    .outline()
                    .small()
                    .disabled(responding)
                    .on_click(respond(ProviderApprovalDecision::Decline)),
            )
            .child(
                Button::new("approval-accept-session")
                    .label("Always allow this session")
                    .outline()
                    .small()
                    .disabled(responding)
                    .on_click(respond(ProviderApprovalDecision::AcceptForSession)),
            )
            .child(
                Button::new("approval-accept")
                    .label("Approve once")
                    .primary()
                    .small()
                    .disabled(responding)
                    .on_click(respond(ProviderApprovalDecision::Accept)),
            )
            .into_any_element()
    }

    /// Composer-top user-input panel (`ComposerPendingUserInputPanel`): one
    /// question at a time — header eyebrow + n/N chip, question text, option
    /// rows with selection state and number-key chips.
    fn render_user_input_panel(
        &self,
        pending: &PendingUserInput,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let responding = self.responding.contains(&pending.request_id);
        let (question_index, selected): (usize, Vec<String>) = match &self.input_draft {
            Some(draft) if draft.request_id == pending.request_id => {
                let index = draft.question_index.min(pending.questions.len() - 1);
                (
                    index,
                    draft
                        .selections
                        .get(&pending.questions[index].id)
                        .cloned()
                        .unwrap_or_default(),
                )
            }
            _ => (0, Vec::new()),
        };
        let question = &pending.questions[question_index];

        let mut options = v_flex().mt_3().gap_1p5();
        for (index, option) in question.options.iter().enumerate() {
            let is_selected = selected.contains(&option.label);
            let label = option.label.clone();
            let mut labels = v_flex().min_w_0().flex_1().gap_0p5().child(
                div()
                    .text_sm()
                    .font_medium()
                    .child(SharedString::from(option.label.clone())),
            );
            if !option.description.is_empty() && option.description != option.label {
                labels = labels.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(option.description.clone())),
                );
            }
            let trailing: gpui::AnyElement = if is_selected {
                Icon::new(IconName::Check)
                    .size_3p5()
                    .text_color(cx.theme().primary)
                    .into_any_element()
            } else {
                div()
                    .size_5()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .text_size(px(11.))
                    .text_color(cx.theme().muted_foreground.opacity(0.7))
                    .child(SharedString::from(format!("{}", index + 1)))
                    .into_any_element()
            };
            let mut row = h_flex()
                .id(("ui-option", index))
                .w_full()
                .items_center()
                .gap_3()
                .rounded(cx.theme().radius)
                .border_1()
                .px_3()
                .py_2()
                .child(labels)
                .child(trailing);
            row = if is_selected {
                row.border_color(cx.theme().primary.opacity(0.3))
                    .bg(cx.theme().primary.opacity(0.08))
            } else {
                row.border_color(gpui::transparent_black())
                    .bg(cx.theme().secondary)
                    .hover(|style| style.bg(cx.theme().accent))
            };
            row = if responding {
                row.opacity(0.5)
            } else {
                row.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_user_input_option(label.clone(), cx);
                    }))
            };
            options = options.child(row);
        }

        v_flex()
            .px_4()
            .py_3()
            .child(
                h_flex()
                    .mb_2()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground.opacity(0.55))
                            .child(SharedString::from(question.header.to_uppercase())),
                    )
                    .when(pending.questions.len() > 1, |this| {
                        this.child(
                            div()
                                .h_5()
                                .px_1p5()
                                .flex()
                                .items_center()
                                .rounded(px(6.))
                                .bg(cx.theme().muted.opacity(0.6))
                                .text_size(px(10.))
                                .font_medium()
                                .text_color(cx.theme().muted_foreground.opacity(0.6))
                                .child(SharedString::from(format!(
                                    "{}/{}",
                                    question_index + 1,
                                    pending.questions.len()
                                ))),
                        )
                    }),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().foreground.opacity(0.9))
                    .child(SharedString::from(question.question.clone())),
            )
            .when(question.multi_select, |this| {
                this.child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground.opacity(0.65))
                        .child("Select one or more options."),
                )
            })
            .child(options)
            .when(question.multi_select, |this| {
                let is_last = question_index + 1 >= pending.questions.len();
                this.child(
                    h_flex().mt_3().justify_end().child(
                        Button::new("ui-advance")
                            .label(if is_last { "Submit" } else { "Continue" })
                            .primary()
                            .small()
                            .disabled(responding || selected.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.advance_user_input(cx);
                            })),
                    ),
                )
            })
            .into_any_element()
    }

    /// Right-hand plan panel (`PlanSidebar`): TASKS badge header + the active
    /// TodoWrite plan's steps with status glyphs.
    fn render_plan_sidebar(
        &self,
        plan: &ActivePlanState,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let mut steps = v_flex().gap_1();
        for (index, step) in plan.steps.iter().enumerate() {
            let glyph: gpui::AnyElement = match step.status {
                PlanStepStatus::Pending => div()
                    .size_3p5()
                    .flex_shrink_0()
                    .rounded_full()
                    .border_1()
                    .border_color(cx.theme().muted_foreground.opacity(0.4))
                    .into_any_element(),
                PlanStepStatus::InProgress => Icon::new(IconName::LoaderCircle)
                    .size_3p5()
                    .text_color(cx.theme().info)
                    .into_any_element(),
                PlanStepStatus::Completed => Icon::new(IconName::CircleCheck)
                    .size_3p5()
                    .text_color(cx.theme().success)
                    .into_any_element(),
            };
            let text = div()
                .text_size(px(13.))
                .map(|this| match step.status {
                    PlanStepStatus::Completed => this
                        .text_color(cx.theme().muted_foreground.opacity(0.5))
                        .line_through(),
                    PlanStepStatus::InProgress => {
                        this.text_color(cx.theme().foreground.opacity(0.9))
                    }
                    PlanStepStatus::Pending => {
                        this.text_color(cx.theme().muted_foreground.opacity(0.7))
                    }
                })
                .child(SharedString::from(step.step.clone()));
            let mut row = h_flex()
                .id(("plan-step", index))
                .items_center()
                .gap_2p5()
                .rounded(cx.theme().radius)
                .px_2p5()
                .py_2()
                .child(glyph)
                .child(text);
            row = match step.status {
                PlanStepStatus::InProgress => row.bg(cx.theme().info.opacity(0.05)),
                PlanStepStatus::Completed => row.bg(cx.theme().success.opacity(0.05)),
                PlanStepStatus::Pending => row,
            };
            steps = steps.child(row);
        }

        let mut content = v_flex().p_3().gap_4();
        if let Some(explanation) = &plan.explanation {
            content = content.child(
                div()
                    .text_size(px(13.))
                    .text_color(cx.theme().muted_foreground.opacity(0.8))
                    .child(SharedString::from(explanation.clone())),
            );
        }
        content = content.child(
            v_flex()
                .child(
                    div()
                        .mb_2()
                        .text_size(px(10.))
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground.opacity(0.4))
                        .child("STEPS"),
                )
                .child(steps),
        );

        v_flex()
            .w(px(340.))
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .h(px(48.))
                    .px_3()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .px_1p5()
                            .rounded(px(6.))
                            .bg(cx.theme().info.opacity(0.15))
                            .text_size(px(10.))
                            .font_semibold()
                            .text_color(cx.theme().info)
                            .child("TASKS"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(cx.theme().muted_foreground.opacity(0.6))
                            .children(relative_time(&plan.created_at)),
                    ),
            )
            .child(
                div()
                    .id("plan-steps")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(content),
            )
            .into_any_element()
    }

    /// Sidebar mirroring the Electron `SidebarV2`: 256px, sidebar tokens,
    /// top controls (search affordance + new-thread), two-line card rows.
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.thread.as_ref().map(|open| open.id.clone());
        let phase = match self.shell.phase {
            SyncPhase::Disconnected => "offline",
            SyncPhase::Synchronizing => "syncing…",
            SyncPhase::Live => "live",
        };
        let project_title = |thread: &OrchestrationThreadShell| -> SharedString {
            self.shell
                .snapshot
                .as_ref()
                .and_then(|snapshot| {
                    snapshot
                        .projects
                        .iter()
                        .find(|project| project.id == thread.project_id)
                })
                .map(|project| project.title.0.clone().into())
                .unwrap_or_else(|| "—".into())
        };

        let mut list = v_flex().gap_0p5().px_2();
        for thread in self.shell_threads() {
            let id = thread.id.clone();
            let is_selected = selected.as_ref() == Some(&id);
            let running = thread
                .session
                .as_ref()
                .is_some_and(|session| session.status == OrchestrationSessionStatus::Running);
            let status: gpui::AnyElement = if running {
                h_flex()
                    .gap_1()
                    .items_center()
                    .text_xs()
                    .font_medium()
                    .text_color(cx.theme().info)
                    .child("Working")
                    .into_any_element()
            } else {
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.75))
                    .children(relative_time(&thread.updated_at.0))
                    .into_any_element()
            };
            let mut row = v_flex()
                .id(SharedString::from(format!("thread-{}", id.0)))
                .px_2p5()
                .py_2()
                .rounded(cx.theme().radius)
                .cursor_pointer()
                .child(
                    h_flex()
                        .h_5()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground.opacity(0.85))
                                .truncate()
                                .child(project_title(&thread)),
                        )
                        .child(status),
                )
                .child(
                    div()
                        .mt_1()
                        .text_sm()
                        .font_medium()
                        .text_color(cx.theme().sidebar_foreground.opacity(0.9))
                        .truncate()
                        .child(SharedString::from(thread.title.0.clone())),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.select_thread(id.clone(), cx);
                }));
            row = if is_selected {
                row.bg(cx.theme().list_active)
            } else {
                row.hover(|style| style.bg(cx.theme().list_hover))
            };
            list = list.child(row);
        }

        v_flex()
            .w_full()
            .h_full()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            // Clear the hiddenInset traffic lights.
            .pt(px(44.))
            .child(
                h_flex()
                    .px_2()
                    .pb_2()
                    .gap_1p5()
                    .items_center()
                    .child(
                        h_flex()
                            .flex_1()
                            .h_8()
                            .px_2()
                            .gap_2()
                            .items_center()
                            .rounded(cx.theme().radius)
                            .bg(cx.theme().muted)
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(Icon::new(IconName::Search).size_4())
                            .child("Search")
                            .child(
                                div()
                                    .ml_auto()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground.opacity(0.6))
                                    .child(phase),
                            ),
                    )
                    .child(
                        div()
                            .id("new-thread")
                            .size_8()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(cx.theme().radius)
                            .cursor_pointer()
                            .text_color(cx.theme().muted_foreground)
                            .hover(|style| style.bg(cx.theme().sidebar_accent))
                            .child(Icon::new(IconName::Plus).size_4())
                            .on_click(cx.listener(|this, _, _, cx| this.new_thread(cx))),
                    ),
            )
            .child(
                div()
                    .id("thread-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(list),
            )
            .child(
                div()
                    .p_3()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.75))
                    .border_t_1()
                    .border_color(cx.theme().sidebar_border)
                    .child(self.sidecar_status.clone()),
            )
    }

    /// Chat column mirroring the Electron `ChatView`: header, error banner,
    /// centered max-w-3xl timeline (user bubbles right, assistant plain,
    /// low-weight activity rows), rounded-22 composer with circular send.
    fn render_chat(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(open) = &self.thread else {
            return v_flex()
                .flex_1()
                .h_full()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Select a thread to start chatting")
                .into_any_element();
        };
        let title = self.thread_title(&open.id);
        let view = open.state.view.as_ref();
        let running = view.is_some_and(|view| {
            view.session
                .as_ref()
                .is_some_and(|session| session.status == OrchestrationSessionStatus::Running)
        });
        let session_error: Option<SharedString> = view.and_then(|view| {
            view.session
                .as_ref()
                .and_then(|session| session.last_error.as_ref())
                .map(|error| error.0.clone().into())
        });
        // Model picker: current selection comes from the SHELL copy (meta
        // events are shell-scoped); options come from the server config.
        let current_selection: Option<ModelSelection> = self
            .shell_thread(&open.id)
            .map(|thread| thread.model_selection.clone())
            .or_else(|| view.map(|view| view.model_selection.clone()));
        let current_slug: Option<String> = current_selection
            .as_ref()
            .and_then(|selection| selection.model.as_str().map(str::to_string));
        let current_instance: Option<String> = current_selection
            .as_ref()
            .and_then(|selection| selection.instance_id.clone().flatten())
            .and_then(|value| value.as_str().map(str::to_string));
        let providers = self
            .client
            .as_ref()
            .and_then(|client| client.sessions().borrow().clone())
            .map(|session| session.config.providers.clone())
            .unwrap_or_default();
        let model_label: SharedString = providers
            .iter()
            .flat_map(|provider| provider.models.iter().map(move |model| (provider, model)))
            .find(|(provider, model)| {
                Some(&model.slug.0) == current_slug.as_ref()
                    && (current_instance.is_none()
                        || Some(&provider.instance_id.0) == current_instance.as_ref())
            })
            .map(|(_, model)| SharedString::from(model.name.0.clone()))
            .or_else(|| current_slug.clone().map(SharedString::from))
            .unwrap_or_else(|| "Model".into());

        // Revert targets: a user message reverts to the checkpoint BEFORE the
        // next assistant turn's checkpoint (`checkpointTurnCount - 1`) —
        // ported from ChatView's `revertTurnCountByUserMessageId`.
        let revert_turn_counts: HashMap<MessageId, i64> = view
            .map(|view| {
                let by_assistant: HashMap<&MessageId, i64> = view
                    .checkpoints
                    .iter()
                    .filter_map(|checkpoint| {
                        checkpoint
                            .assistant_message_id
                            .as_ref()
                            .map(|id| (id, checkpoint.checkpoint_turn_count.0))
                    })
                    .collect();
                let mut messages: Vec<&OrchestrationMessage> = view.messages.iter().collect();
                messages.sort_by(|a, b| a.created_at.0.cmp(&b.created_at.0));
                let mut map = HashMap::new();
                for (index, message) in messages.iter().enumerate() {
                    if message.role != OrchestrationMessageRole::User {
                        continue;
                    }
                    for next in &messages[index + 1..] {
                        if next.role == OrchestrationMessageRole::User {
                            break;
                        }
                        if let Some(count) = by_assistant.get(&next.id) {
                            map.insert(message.id.clone(), (count - 1).max(0));
                            break;
                        }
                    }
                }
                map
            })
            .unwrap_or_default();

        // Timeline rows (centered, max-w-3xl like the web timeline).
        let mut rows = v_flex()
            .w_full()
            .max_w(px(768.))
            .mx_auto()
            .px_5()
            .py_4()
            .gap_2();
        if let Some(view) = view {
            for entry in timeline_entries(view) {
                match entry {
                    TimelineEntry::Message(message) => {
                        let is_user = message.role == OrchestrationMessageRole::User;
                        if is_user {
                            let mut line = h_flex().w_full().justify_end().items_center().gap_2();
                            if let Some(turn_count) = revert_turn_counts.get(&message.id).copied() {
                                let armed = self.pending_revert.as_ref() == Some(&message.id);
                                let message_id = message.id.clone();
                                let button: gpui::AnyElement = if armed {
                                    Button::new(SharedString::from(format!(
                                        "revert-{}",
                                        message.id.0
                                    )))
                                    .label("Revert?")
                                    .danger()
                                    .outline()
                                    .xsmall()
                                    .disabled(self.reverting || running)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.revert_user_message(
                                            message_id.clone(),
                                            turn_count,
                                            cx,
                                        );
                                    }))
                                    .into_any_element()
                                } else {
                                    div()
                                        .id(SharedString::from(format!("revert-{}", message.id.0)))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .size_6()
                                        .rounded(cx.theme().radius)
                                        .cursor_pointer()
                                        .text_color(cx.theme().muted_foreground.opacity(0.5))
                                        .hover(|style| style.bg(cx.theme().secondary))
                                        .child(Icon::new(IconName::Undo2).size_3())
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.revert_user_message(
                                                message_id.clone(),
                                                turn_count,
                                                cx,
                                            );
                                        }))
                                        .into_any_element()
                                };
                                line = line.child(button);
                            }
                            rows = rows.child(
                                line.child(
                                    div()
                                        .max_w(relative(0.8))
                                        .rounded(px(16.))
                                        .bg(cx.theme().accent)
                                        .p_3()
                                        .text_sm()
                                        .text_color(cx.theme().foreground)
                                        .child(SharedString::from(message.text.0.clone())),
                                ),
                            );
                        } else {
                            let text = message.text.0.clone();
                            let streaming = message.streaming;
                            rows = rows.child(
                                div()
                                    .px_1()
                                    .py_0p5()
                                    .text_sm()
                                    .text_color(cx.theme().foreground)
                                    .map(|this| {
                                        if text.is_empty() && streaming {
                                            this.text_color(cx.theme().muted_foreground)
                                                .child("(empty response)")
                                        } else {
                                            // Keyed per message: the free
                                            // `markdown()` helper keys by call
                                            // site and would collide in this
                                            // loop.
                                            this.child(
                                                TextView::markdown(
                                                    SharedString::from(format!(
                                                        "md-{}",
                                                        message.id.0
                                                    )),
                                                    SharedString::from(text),
                                                )
                                                .selectable(true),
                                            )
                                        }
                                    }),
                            );
                        }
                    }
                    TimelineEntry::Activity(activity) => {
                        let tone_color = match activity.tone {
                            OrchestrationThreadActivityTone::Error => cx.theme().danger,
                            OrchestrationThreadActivityTone::Approval => cx.theme().warning,
                            _ => cx.theme().foreground.opacity(0.82),
                        };
                        rows = rows.child(
                            h_flex()
                                .px_0p5()
                                .py_0p5()
                                .gap_1p5()
                                .items_center()
                                .rounded(cx.theme().radius)
                                .child(
                                    Icon::new(activity_icon(&activity.kind.0))
                                        .size_3p5()
                                        .text_color(cx.theme().muted_foreground.opacity(0.8)),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .font_medium()
                                        .text_color(tone_color)
                                        .truncate()
                                        .child(SharedString::from(activity.summary.0.clone())),
                                ),
                        );
                    }
                }
            }
        }
        if running {
            rows = rows.child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .px_0p5()
                    .py_1()
                    .child(h_flex().gap_1().children((0..3).map(|_| {
                        div()
                            .size(px(4.))
                            .rounded_full()
                            .bg(cx.theme().muted_foreground.opacity(0.3))
                    })))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(cx.theme().muted_foreground.opacity(0.7))
                            .child("Working…"),
                    ),
            );
        }

        // Pending approvals / user-input requests are derived from the
        // activity log (the reducer deliberately skips their events); the
        // approval panel takes precedence, like the Electron composer.
        let pending_approvals = view
            .map(|view| derive_pending_approvals(&view.activities))
            .unwrap_or_default();
        let pending_inputs = view
            .map(|view| derive_pending_user_inputs(&view.activities))
            .unwrap_or_default();
        let active_approval = pending_approvals.first().cloned();
        let active_input = if active_approval.is_none() {
            pending_inputs.first().cloned()
        } else {
            None
        };
        let panel: Option<gpui::AnyElement> = {
            let inner = if let Some(approval) = &active_approval {
                Some(self.render_approval_panel(approval, pending_approvals.len(), cx))
            } else {
                active_input
                    .as_ref()
                    .map(|input| self.render_user_input_panel(input, cx))
            };
            inner.map(|inner| {
                div()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().secondary)
                    .child(inner)
                    .into_any_element()
            })
        };

        // Composer: rounded-22 shell, textarea, footer (model label + send).
        let send_button: gpui::AnyElement = if running {
            div()
                .id("stop")
                .size_8()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(cx.theme().danger.opacity(0.9))
                .hover(|style| style.bg(cx.theme().danger))
                .child(
                    div()
                        .size(px(10.))
                        .rounded(px(2.))
                        .bg(cx.theme().danger_foreground),
                )
                .on_click(cx.listener(|this, _, _, cx| this.stop_turn(cx)))
                .into_any_element()
        } else {
            div()
                .id("send")
                .size_8()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(cx.theme().primary.opacity(0.9))
                .hover(|style| style.bg(cx.theme().primary))
                .child(
                    Icon::new(IconName::ArrowUp)
                        .size_4()
                        .text_color(cx.theme().primary_foreground),
                )
                .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))
                .into_any_element()
        };
        // The picker lists every enabled+installed provider's models; picking
        // one dispatches `ThreadMetaUpdate` (see `set_model`).
        let model_picker: gpui::AnyElement = {
            let chat = cx.entity().downgrade();
            let menu_providers: Vec<_> = providers
                .iter()
                .filter(|provider| provider.enabled && provider.installed)
                .cloned()
                .collect();
            let multiple_providers = menu_providers.len() > 1;
            let current_slug = current_slug.clone();
            let current_instance = current_instance.clone();
            Button::new("model-picker")
                .label(model_label)
                .ghost()
                .small()
                .dropdown_menu(move |mut menu, _window, _cx| {
                    for provider in &menu_providers {
                        if multiple_providers {
                            let name = provider
                                .display_name
                                .clone()
                                .flatten()
                                .map(|name| name.0)
                                .unwrap_or_else(|| provider.instance_id.0.clone());
                            menu = menu.item(PopupMenuItem::label(SharedString::from(name)));
                        }
                        for model in &provider.models {
                            let checked = Some(&model.slug.0) == current_slug.as_ref()
                                && (current_instance.is_none()
                                    || Some(&provider.instance_id.0) == current_instance.as_ref());
                            let selection = ModelSelection {
                                instance_id: Some(Some(serde_json::Value::String(
                                    provider.instance_id.0.clone(),
                                ))),
                                model: serde_json::Value::String(model.slug.0.clone()),
                                options: None,
                                provider: None,
                            };
                            let chat = chat.clone();
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(model.name.0.clone()))
                                    .checked(checked)
                                    .on_click(move |_, _, cx| {
                                        let selection = selection.clone();
                                        let _ = chat.update(cx, |this, cx| {
                                            this.set_model(selection, cx);
                                        });
                                    }),
                            );
                        }
                    }
                    menu
                })
                .into_any_element()
        };
        let footer: gpui::AnyElement = if let Some(approval) = &active_approval {
            // The bottom toolbar is replaced by the approval actions while an
            // approval is pending (Electron's ChatComposer does the same).
            self.render_approval_actions(approval, cx)
        } else {
            h_flex()
                .px_3()
                .pb_3()
                .pt_1()
                .justify_between()
                .items_center()
                .child(model_picker)
                .child(send_button)
                .into_any_element()
        };
        let composer = div().px_5().pb_4().child(
            v_flex()
                .w_full()
                .max_w(px(768.))
                .mx_auto()
                .rounded(px(22.))
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().muted)
                .overflow_hidden()
                .children(panel)
                .child(
                    div()
                        .px_2()
                        .pt_2()
                        .child(Textarea::new(&self.composer).appearance(false)),
                )
                .child(footer),
        );

        let mut main = v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .h(px(40.))
                    .px_5()
                    .items_center()
                    .flex_shrink_0()
                    .child(div().text_sm().font_medium().truncate().child(title)),
            );
        let banner: Option<SharedString> = self.last_error.clone().or(session_error);
        if let Some(error) = banner {
            main = main.child(
                div().px_5().pt_1().child(
                    div()
                        .mx_auto()
                        .w_auto()
                        .max_w(px(768.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(cx.theme().danger.opacity(0.32))
                        .bg(cx.theme().danger.opacity(0.04))
                        .px_3p5()
                        .py_3()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error),
                ),
            );
        }
        main.child(
            div()
                .id("messages")
                .flex_1()
                .overflow_y_scroll()
                .track_scroll(&self.timeline_scroll)
                .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                    // Offsets grow negative downward; "at bottom" within 60px
                    // re-arms anchoring, scrolling further up releases it.
                    let offset = this.timeline_scroll.offset();
                    let max = this.timeline_scroll.max_offset();
                    let at_bottom = offset.y <= -(max.y - px(60.));
                    if this.stick_to_bottom != at_bottom {
                        this.stick_to_bottom = at_bottom;
                        cx.notify();
                    }
                }))
                .child(rows),
        )
        .child(composer)
        .into_any_element()
    }
}
fn describe_status(status: &SupervisorStatus) -> String {
    match status {
        SupervisorStatus::Idle => "sidecar: idle".into(),
        SupervisorStatus::Starting { attempt } => format!("sidecar: starting (attempt {attempt})"),
        SupervisorStatus::Ready { info } => format!("sidecar: 127.0.0.1:{}", info.port),
        SupervisorStatus::Backoff { delay_ms, .. } => {
            format!("sidecar: restarting in {delay_ms}ms")
        }
        SupervisorStatus::Stopped { reason } => format!("sidecar: stopped ({reason})"),
    }
}

impl Render for ChatApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_plan = self
            .thread
            .as_ref()
            .and_then(|open| open.state.view.as_ref())
            .and_then(|view| {
                derive_active_plan_state(
                    &view.activities,
                    view.latest_turn.as_ref().map(|turn| &turn.turn_id),
                )
            });
        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div().flex_1().min_w_0().h_full().child(
                    h_resizable("workspace")
                        .with_state(&self.sidebar_resize)
                        .child(
                            resizable_panel()
                                .size(px(256.))
                                .size_range(px(208.)..px(480.))
                                .child(self.render_sidebar(cx).into_any_element()),
                        )
                        .child(resizable_panel().child(self.render_chat(cx))),
                ),
            )
            .children(
                active_plan
                    .as_ref()
                    .map(|plan| self.render_plan_sidebar(plan, cx)),
            )
    }
}
