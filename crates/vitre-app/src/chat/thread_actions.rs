//! One action model for secondary-click and the overflow button in both sidebars.
use super::*;
use gpui::ClipboardItem;
use gpui_component::{
    input::{Input, InputEvent, InputState},
    menu::{PopupMenu, PopupMenuItem},
};

pub(super) struct ThreadRename {
    id: ThreadId,
    original: String,
    input: Entity<InputState>,
    _subscription: Subscription,
}

#[derive(Clone)]
pub(super) struct ThreadMenuTarget {
    thread: vitre_contracts::OrchestrationThreadShell,
    path: Option<String>,
    running: bool,
    settlement: bool,
    snooze: bool,
}

impl ChatApp {
    pub(super) fn is_thread_renaming(&self, id: &ThreadId) -> bool {
        self.thread_rename.as_ref().is_some_and(|r| r.id == *id)
    }
    #[cfg(debug_assertions)]
    pub(super) fn verification_rename_input(&self) -> Option<Entity<InputState>> {
        self.thread_rename.as_ref().map(|r| r.input.clone())
    }
    pub(super) fn thread_menu_target(&self, id: &ThreadId) -> Option<ThreadMenuTarget> {
        let thread = self.shell_thread(id)?.clone();
        let path = thread
            .worktree_path
            .as_ref()
            .map(|p| p.0.clone())
            .or_else(|| {
                self.shell
                    .snapshot
                    .as_ref()?
                    .projects
                    .iter()
                    .find(|p| p.id == thread.project_id)
                    .map(|p| p.workspace_root.0.clone())
            });
        let running = matches!(
            vitre_state::sidebar::resolve_thread_status(&thread, None),
            Some(
                vitre_state::sidebar::ThreadStatus::Working
                    | vitre_state::sidebar::ThreadStatus::Connecting
                    | vitre_state::sidebar::ThreadStatus::PendingApproval
                    | vitre_state::sidebar::ThreadStatus::AwaitingInput
            )
        );
        let capabilities = self.client.as_ref().and_then(|c| {
            c.sessions()
                .borrow()
                .as_ref()
                .map(|s| s.config.environment.capabilities.clone())
        });
        Some(ThreadMenuTarget {
            thread,
            path,
            running,
            settlement: capabilities
                .as_ref()
                .and_then(|c| c.thread_settlement)
                .unwrap_or(false),
            snooze: capabilities
                .as_ref()
                .and_then(|c| c.thread_snooze)
                .unwrap_or(false),
        })
    }

    fn start_thread_rename(&mut self, id: ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.shell_thread(&id) else {
            return;
        };
        let original = thread.title.0.clone();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(original.clone()));
        let subscription = cx.subscribe_in(&input, window, |app, _, event, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                app.finish_thread_rename(cx);
            }
        });
        input.update(cx, |s, cx| {
            s.set_selected_range(0..original.len(), cx);
            s.focus(window, cx);
        });
        self.thread_rename = Some(ThreadRename {
            id,
            original,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn finish_thread_rename(&mut self, cx: &mut Context<Self>) {
        let Some(rename) = self.thread_rename.take() else {
            return;
        };
        cx.notify();
        let title = rename.input.read(cx).value().trim().to_string();
        if title.is_empty() || title == rename.original {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let command = ClientOrchestrationCommand::ThreadMetaUpdate {
            additional_roots: None,
            branch: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            expected_branch: None,
            model_selection: None,
            thread_id: rename.id,
            title: Some(Some(tnes(title))),
            r#type: Default::default(),
            worktree_path: None,
        };
        cx.spawn(async move |this, cx| {
            if let Err(error) = client.dispatch(&command).await {
                let _ = this.update(cx, |a, cx| {
                    a.last_error = Some(format!("Rename failed: {}", error.user_message()).into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(super) fn render_thread_title(
        &self,
        id: &ThreadId,
        title: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(rename) = self.thread_rename.as_ref().filter(|r| r.id == *id) {
            return div()
                .flex_1()
                .min_w_0()
                .on_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        app.thread_rename = None;
                        cx.stop_propagation();
                        cx.notify();
                    }
                }))
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(Input::new(&rename.input).small())
                .into_any_element();
        }
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .child(title)
            .into_any_element()
    }

    fn new_thread_on_branch(
        &mut self,
        source: &vitre_contracts::OrchestrationThreadShell,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let id = ThreadId(fresh_id("vitre-thread"));
        let command = ClientOrchestrationCommand::ThreadCreate {
            additional_roots: None,
            branch: source.branch.clone(),
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            interaction_mode: source.interaction_mode.clone(),
            model_selection: source.model_selection.clone(),
            project_id: source.project_id.clone(),
            runtime_mode: source.runtime_mode.clone(),
            thread_id: id.clone(),
            title: tnes("New thread"),
            r#type: Default::default(),
            worktree_path: source.worktree_path.clone(),
        };
        let selected = self.thread.as_ref().map(|t| t.id.clone());
        cx.spawn(async move |this, cx| {
            let result = client.dispatch(&command).await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(_) if app.thread.as_ref().map(|t| t.id.clone()) == selected => {
                        app.select_thread(id, cx)
                    }
                    Ok(_) => {}
                    Err(error) => {
                        app.last_error = Some(
                            format!("Could not create thread: {}", error.user_message()).into(),
                        )
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

pub(super) fn thread_context_menu(
    mut menu: PopupMenu,
    owner: &WeakEntity<ChatApp>,
    target: &ThreadMenuTarget,
    beta: bool,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    if let Some(branch) = &target.thread.branch {
        let source = target.thread.clone();
        let owner = owner.clone();
        menu = menu
            .item(
                PopupMenuItem::new(format!("New thread on {}", branch.0)).on_click(
                    move |_, _, cx| {
                        let _ = owner.update(cx, |a, cx| a.new_thread_on_branch(&source, cx));
                    },
                ),
            )
            .separator();
    }
    if beta && target.settlement {
        let settled = super::sidebar_beta::settled(&target.thread);
        let owner = owner.clone();
        let id = target.thread.id.clone();
        menu = menu.item(
            PopupMenuItem::new(if settled {
                "Un-settle thread"
            } else {
                "Settle thread"
            })
            .disabled(target.running)
            .on_click(move |_, _, cx| {
                let _ = owner.update(cx, |a, cx| a.set_thread_settled(id.clone(), !settled, cx));
            }),
        );
    }
    let rename_owner = owner.clone();
    let id = target.thread.id.clone();
    menu = menu.item(
        PopupMenuItem::new("Rename thread").on_click(move |_, window, cx| {
            // Wait for menu dismissal before mounting/focusing the inline input.
            // Otherwise the menu's final layout can steal focus and submit Blur.
            let owner = rename_owner.clone();
            let id = id.clone();
            window.defer(cx, move |window, cx| {
                let _ = owner.update(cx, |a, cx| a.start_thread_rename(id, window, cx));
            });
        }),
    );
    let unread_owner = owner.clone();
    let thread = target.thread.clone();
    menu = menu.item(PopupMenuItem::new("Mark unread").on_click(move |_, _, cx| {
        let _ = unread_owner.update(cx, |a, cx| {
            let completed = thread
                .latest_turn
                .as_ref()
                .and_then(|t| t.completed_at.as_ref())
                .map(|at| at.0.as_str());
            if a.sidebar.mark_thread_unread(&thread.id.0, completed) {
                a.rebuild_sidebar();
                cx.notify();
            }
        });
    }));
    let path = target.path.clone();
    let id = target.thread.id.0.clone();
    menu = menu
        .item(
            PopupMenuItem::new("Copy Path")
                .disabled(path.is_none())
                .on_click(move |_, _, cx| {
                    if let Some(path) = &path {
                        cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                    }
                }),
        )
        .item(
            PopupMenuItem::new("Copy Thread ID").on_click(move |_, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(id.clone()));
            }),
        )
        .separator();
    let snooze_owner = owner.clone();
    let id = target.thread.id.clone();
    if target.snooze && !target.running {
        menu = menu.submenu("Snooze", window, cx, move |mut menu, _, _| {
            for (label, hours) in [
                ("For 1 hour", 1),
                ("For 4 hours", 4),
                ("Until tomorrow", 24),
            ] {
                let owner = snooze_owner.clone();
                let id = id.clone();
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |a, cx| a.snooze_thread(id.clone(), Some(hours), cx));
                }));
            }
            menu
        });
    }
    let wake_owner = owner.clone();
    let id = target.thread.id.clone();
    if target.snooze
        && target
            .thread
            .snoozed_until
            .as_ref()
            .and_then(|v| v.as_ref())
            .and_then(|v| v.as_ref())
            .is_some()
    {
        menu = menu.item(PopupMenuItem::new("Wake thread").on_click(move |_, _, cx| {
            let _ = wake_owner.update(cx, |a, cx| a.snooze_thread(id.clone(), None, cx));
        }));
    }
    let archive_owner = owner.clone();
    let id = target.thread.id.clone();
    menu = menu.item(
        PopupMenuItem::new("Archive thread")
            .disabled(target.running)
            .on_click(move |_, _, cx| {
                let _ = archive_owner.update(cx, |a, cx| a.archive_thread(id.clone(), cx));
            }),
    );
    let owner = owner.clone();
    let id = target.thread.id.clone();
    menu.separator().item(
        PopupMenuItem::new("Delete")
            .icon(crate::assets::VitreIcon::Trash2)
            .on_click(move |_, window, cx| {
                let _ = owner.update(cx, |a, cx| a.delete_thread_request(id.clone(), window, cx));
            }),
    )
}
