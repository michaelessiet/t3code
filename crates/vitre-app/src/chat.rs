//! M1 chat core UI: thread list sidebar + chat view + composer, rendered
//! from the `vitre-client` watch channels.
//!
//! Detail views merge SHELL data with the detail projection: `subscribeThread`
//! only carries the six detail event kinds, so title/meta always come from
//! the thread shell (the TS client's `mergeEnvironmentThread` split).

use std::sync::Arc;

use gpui::{Context, Entity, SharedString, Subscription, Window, div, prelude::*, px};
use gpui_component::{
    ActiveTheme as _, StyledExt as _, h_flex,
    input::{InputEvent, Textarea, TextareaState},
    v_flex,
};
use gpui_tokio::Tokio;
use tokio::sync::watch;
use vitre_client::{EnvironmentClient, ShellState, SyncPhase, ThreadHandle, ThreadState};
use vitre_contracts::{
    ClientOrchestrationCommand, CommandId, MessageId, OrchestrationMessageRole,
    OrchestrationSessionStatus, OrchestrationThreadShell, ProviderInteractionMode, ThreadId,
    TrimmedNonEmptyString,
};
use vitre_sidecar::SupervisorStatus;

pub struct ChatApp {
    client: Option<Arc<EnvironmentClient>>,
    sidecar_status: SharedString,
    shell: ShellState,
    thread: Option<OpenThread>,
    composer: Entity<TextareaState>,
    last_error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
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

impl ChatApp {
    pub fn new(
        status_rx: watch::Receiver<SupervisorStatus>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Message the agent — Enter to send, Shift+Enter for a newline")
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
            _subscriptions: subscriptions,
        }
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
        self.thread = Some(OpenThread {
            id: id.clone(),
            _handle: handle,
            state: ThreadState::default(),
        });
        cx.spawn(async move |this, cx| {
            loop {
                let state = state_rx.borrow_and_update().clone();
                let stop = this
                    .update(cx, |app, cx| match &mut app.thread {
                        Some(open) if open.id == id => {
                            open.state = state;
                            cx.notify();
                            false
                        }
                        _ => true,
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

    /// Canonical title comes from the SHELL (meta events are shell-scoped).
    fn thread_title(&self, id: &ThreadId) -> SharedString {
        self.shell
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.threads.iter().find(|thread| thread.id == *id))
            .map(|thread| thread.title.0.clone().into())
            .unwrap_or_else(|| "(untitled)".into())
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.thread.as_ref().map(|open| open.id.clone());
        let phase = match self.shell.phase {
            SyncPhase::Disconnected => "offline",
            SyncPhase::Synchronizing => "syncing…",
            SyncPhase::Live => "live",
        };
        let mut list = v_flex().gap_1().p_2();
        for thread in self.shell_threads() {
            let id = thread.id.clone();
            let is_selected = selected.as_ref() == Some(&id);
            let running = thread
                .session
                .as_ref()
                .is_some_and(|session| session.status == OrchestrationSessionStatus::Running);
            let mut row = h_flex()
                .id(SharedString::from(format!("thread-{}", id.0)))
                .px_2()
                .py_1p5()
                .gap_2()
                .rounded(cx.theme().radius)
                .cursor_pointer()
                .text_sm()
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .child(SharedString::from(thread.title.0.clone())),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.select_thread(id.clone(), cx);
                }));
            if is_selected {
                row = row
                    .bg(cx.theme().accent)
                    .text_color(cx.theme().accent_foreground);
            } else {
                row = row.hover(|style| style.bg(cx.theme().muted));
            }
            if running {
                row = row.child(div().text_xs().child("●"));
            }
            list = list.child(row);
        }

        v_flex()
            .w(px(260.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .p_3()
                    .justify_between()
                    .items_baseline()
                    .child(div().font_semibold().child("Threads"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(phase),
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
                    .text_color(cx.theme().muted_foreground)
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(self.sidecar_status.clone()),
            )
    }

    fn render_chat(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(open) = &self.thread else {
            return v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child("Select a thread to start chatting")
                .into_any_element();
        };
        let title = self.thread_title(&open.id);
        let session_line: Option<SharedString> = open.state.view.as_ref().and_then(|view| {
            view.session.as_ref().map(|session| {
                let status = format!("session: {:?}", session.status).to_lowercase();
                match &session.last_error {
                    Some(error) => format!("{status} — {}", error.0).into(),
                    None => status.into(),
                }
            })
        });

        let mut messages = v_flex().gap_3().p_4();
        if let Some(view) = &open.state.view {
            for message in &view.messages {
                let is_user = message.role == OrchestrationMessageRole::User;
                let label = if is_user { "you" } else { "agent" };
                let mut bubble = v_flex()
                    .gap_1()
                    .p_3()
                    .rounded(cx.theme().radius)
                    .text_sm()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(format!(
                                "{label}{}",
                                if message.streaming {
                                    " · streaming…"
                                } else {
                                    ""
                                }
                            ))),
                    )
                    .child(SharedString::from(message.text.0.clone()));
                bubble = if is_user {
                    bubble.bg(cx.theme().muted)
                } else {
                    bubble.bg(cx.theme().secondary)
                };
                messages = messages.child(bubble);
            }
        }

        let mut main = v_flex()
            .flex_1()
            .h_full()
            .child(
                h_flex()
                    .p_3()
                    .gap_3()
                    .items_baseline()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().font_semibold().truncate().child(title))
                    .children(session_line.map(|line| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(line)
                    })),
            )
            .child(
                div()
                    .id("messages")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(messages),
            );
        if let Some(error) = &self.last_error {
            main = main.child(
                div()
                    .px_4()
                    .py_2()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        main.child(
            div()
                .p_3()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(Textarea::new(&self.composer)),
        )
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
        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_sidebar(cx))
            .child(self.render_chat(cx))
    }
}
