//! The bottom terminal drawer: Electron's `ThreadTerminalDrawer.tsx` plus the
//! ChatView glue — persisted per-thread UI state, the metadata subscription,
//! terminal lifecycle (new/split/close, server reconcile), drag-resize, and
//! the tabs/groups sidebar.
//!
//! Store semantics live in [`vitre_state::terminal_ui`] (a pure port of
//! `terminalUiStateStore.ts`); the per-session buffer views are
//! [`super::terminal_view::TerminalView`]. The right-panel dock's terminal
//! surfaces are a separate follow-up — their pane ids are excluded from the
//! drawer's server reconcile exactly as Electron partitions ownership.
//!
//! The selection "Add to chat" flow bubbles from the views:
//! [`TerminalViewEvent::AddToChat`] → `ChatApp::add_terminal_context`
//! (Electron's `onAddTerminalContext` prop chain).
//!
//! Deferred (matrix-noted): project scripts (M3 slice 2.8b),
//! running-subprocess indicators, and the dock terminal surface content.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::{
    AnyElement, Context, MouseButton, MouseDownEvent, MouseMoveEvent, SharedString, Window, div,
    prelude::*, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};
use serde_json::Value;
use vitre_contracts::methods::{
    SubscribeTerminalMetadata, TerminalClose, TerminalOpen, TerminalWrite,
};
use vitre_contracts::{TerminalCloseInput, TerminalOpenInput, TerminalWriteInput};
use vitre_rpc::TypedStreamEvent;
use vitre_state::right_panel::{
    MAX_TERMINALS_PER_GROUP, RightPanelSurface, SplitDirection, next_terminal_id,
};
use vitre_state::terminal::{apply_metadata_event, compare_terminal_ids, terminal_fallback_label};
use vitre_state::terminal_ui::{
    TERMINAL_UI_PERSISTED_VERSION, TerminalUiMap, ThreadTerminalUiState, clamp_drawer_height,
    sorted_groups_for_display,
};

use crate::assets::VitreIcon;

use super::terminal_view::{TerminalLaunch, TerminalView, TerminalViewEvent};
use super::{ChatApp, tnes};

const FILE_NAME: &str = "terminal-ui-state.json";

/// A server-completed metadata stream must not resubscribe in a hot loop
/// (mirrors vitre-client's `RESUBSCRIBE_AFTER_COMPLETION`).
const RESUBSCRIBE_AFTER_COMPLETION: Duration = Duration::from_secs(2);

/// Electron persists the store as localStorage `t3code:terminal-state:v1`;
/// Vitre keeps the same versioned blob in `~/.vitre/terminal-ui-state.json`.
pub(super) struct TerminalPrefs {
    pub map: TerminalUiMap,
    path: PathBuf,
}

impl TerminalPrefs {
    pub fn load(home: &Path) -> Self {
        let path = home.join(FILE_NAME);
        let map = std::fs::read_to_string(&path)
            .ok()
            .and_then(|contents| match serde_json::from_str::<Value>(&contents) {
                Ok(stored) => Some(stored),
                Err(error) => {
                    eprintln!("[vitre] ignoring unreadable {FILE_NAME}: {error}");
                    None
                }
            })
            .map(|stored| TerminalUiMap::from_persisted(&stored))
            .unwrap_or_default();
        Self { map, path }
    }

    pub fn save(&self) {
        let stored = serde_json::json!({
            "version": TERMINAL_UI_PERSISTED_VERSION,
            "terminalUiStateByThreadKey": self.map.to_persisted(),
        });
        let write = serde_json::to_string_pretty(&stored)
            .map_err(std::io::Error::other)
            .and_then(|json| {
                if let Some(parent) = self.path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&self.path, json)
            });
        if let Err(error) = write {
            eprintln!("[vitre] failed to persist {FILE_NAME}: {error}");
        }
    }
}

/// An in-flight drag on the drawer's top handle. The height is local until
/// pointer-up, when it is clamped once more and persisted (Electron commits to
/// the store on `pointerup` only, resizing locally per `pointermove`).
pub(super) struct TerminalDrag {
    start_y: f32,
    start_height: f64,
    pub current_height: f64,
}

/// Everything a `terminal.open`/attach launch needs, resolved from the open
/// thread: cwd (worktree checkout when the thread has one), the wire
/// worktree field, and the canonical runtime env. A changed env silently
/// restarts the server shell, so this must stay deterministic.
struct ThreadLaunchDefaults {
    thread_id: String,
    cwd: String,
    worktree: Option<String>,
    env: BTreeMap<String, String>,
}

impl ChatApp {
    /// The drawer's scoped key: `${environmentId}:${threadId}`. Unlike the
    /// dock there is no `:home` pseudo-thread — Electron's drawer only exists
    /// inside a thread view.
    pub(super) fn terminal_thread_key(&self) -> Option<String> {
        let client = self.client.as_ref()?;
        let session = client.sessions().borrow().clone()?;
        let environment = session.config.environment.environment_id.0.clone();
        let thread = self.thread.as_ref()?;
        Some(format!("{environment}:{}", thread.id.0))
    }

    fn thread_launch_defaults(&self) -> Option<ThreadLaunchDefaults> {
        let open = self.thread.as_ref()?;
        let shell_thread = self.shell_thread(&open.id)?;
        let worktree = shell_thread
            .worktree_path
            .as_ref()
            .map(|path| path.0.clone());
        let workspace_root = self.project_root(&shell_thread.project_id)?;
        let cwd = worktree.clone().unwrap_or_else(|| workspace_root.clone());
        let mut env = BTreeMap::new();
        env.insert("T3CODE_PROJECT_ROOT".to_string(), workspace_root);
        if let Some(worktree) = &worktree {
            env.insert("T3CODE_WORKTREE_PATH".to_string(), worktree.clone());
        }
        Some(ThreadLaunchDefaults {
            thread_id: open.id.0.clone(),
            cwd,
            worktree,
            env,
        })
    }

    // ---- metadata subscription ---------------------------------------------

    /// Durable `subscribeTerminalMetadata` loop (environment-wide), the same
    /// session-watch shape as the diff panel's vcs stream. Every event batch
    /// folds into `terminal_metadata` and re-runs the drawer reconcile.
    pub(super) fn spawn_terminal_metadata_loop(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let payload: Value = serde_json::json!({});
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
                    .subscribe_typed::<SubscribeTerminalMetadata>(&payload)
                else {
                    if sessions.changed().await.is_err() {
                        return;
                    }
                    continue;
                };
                let completed = loop {
                    tokio::select! {
                        event = subscription.next() => match event {
                            Some(TypedStreamEvent::Values(events)) => {
                                if this
                                    .update(cx, |app, cx| {
                                        for event in &events {
                                            apply_metadata_event(
                                                &mut app.terminal_metadata,
                                                event,
                                            );
                                        }
                                        app.reconcile_drawer_terminals(cx);
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
                            Some(TypedStreamEvent::Completed(result)) => break result.is_ok(),
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
        })
        .detach();
    }

    /// Adopt the server's terminal set for the open thread (Electron's
    /// metadata-reconcile effect): dock-owned pane ids excluded, locally
    /// closed (suppressed) ids filtered, ids sorted numerically. Skipped when
    /// membership already matches (server MRU order must not reshuffle the
    /// user's tab order) and when the server set is a strict subset of the
    /// client's — a just-created tab hasn't reached the metadata stream yet.
    pub(super) fn reconcile_drawer_terminals(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.terminal_thread_key() else {
            return;
        };
        let Some(open) = &self.thread else {
            return;
        };
        let dock_owned: HashSet<&str> = self
            .right_panel
            .map
            .thread(&key)
            .surfaces
            .iter()
            .filter_map(|surface| match surface {
                RightPanelSurface::Terminal { terminal_ids, .. } => {
                    Some(terminal_ids.iter().map(String::as_str))
                }
                _ => None,
            })
            .flatten()
            .collect();
        let suppressed = self.terminal_ui.map.suppressed_ids(&key);
        let mut server_ids: Vec<String> = self
            .terminal_metadata
            .iter()
            .filter(|summary| summary.thread_id == open.id.0)
            .map(|summary| summary.terminal_id.clone())
            .filter(|id| !dock_owned.contains(id.as_str()))
            .filter(|id| !suppressed.contains(id))
            .collect();
        server_ids.sort_by(|a, b| compare_terminal_ids(a, b));
        server_ids.dedup();

        let state = self.terminal_ui.map.thread(&key);
        let server_set: HashSet<&str> = server_ids.iter().map(String::as_str).collect();
        let client_set: HashSet<&str> = state.terminal_ids.iter().map(String::as_str).collect();
        if server_set == client_set {
            return;
        }
        if server_set.is_subset(&client_set) {
            return;
        }
        if self
            .terminal_ui
            .map
            .reconcile_terminal_ids(&key, &server_ids)
        {
            self.terminal_ui.save();
            cx.notify();
        }
    }

    // ---- lifecycle ---------------------------------------------------------

    /// `terminal.toggle` (`` ctrl+` `` / `mod+r`). Opening with no terminals
    /// seeds `term-1` in the store; the view's attach (with cwd) auto-opens
    /// the server session.
    pub(super) fn terminal_toggle(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.terminal_thread_key() else {
            return;
        };
        let state = self.terminal_ui.map.thread(&key);
        let opening = !state.terminal_open;
        if self.terminal_ui.map.set_terminal_open(&key, opening) {
            self.terminal_ui.save();
        }
        if opening {
            self.focus_active_terminal(&key, cx);
        }
        cx.notify();
    }

    fn focus_active_terminal(&mut self, key: &str, cx: &mut Context<Self>) {
        let state = self.terminal_ui.map.thread(key);
        if state.active_terminal_id.is_empty() {
            return;
        }
        let map_key = (key.to_string(), state.active_terminal_id.clone());
        if let Some(view) = self.terminal_views.get(&map_key) {
            view.update(cx, |view, cx| view.request_focus(cx));
        }
    }

    pub(super) fn terminal_new(&mut self, cx: &mut Context<Self>) {
        self.terminal_create(None, cx);
    }

    pub(super) fn terminal_split(&mut self, vertical: bool, cx: &mut Context<Self>) {
        self.terminal_create(Some(vertical), cx);
    }

    /// Shared create path: allocate the lowest unused `term-N` across every
    /// id the environment knows for this thread (store ∪ server ∪ dock
    /// panes), mutate the store, then fire `terminal.open` (no cols/rows —
    /// the first canvas layout sends the real size, like Electron's fit).
    fn terminal_create(&mut self, split_vertical: Option<bool>, cx: &mut Context<Self>) {
        let Some(key) = self.terminal_thread_key() else {
            return;
        };
        let Some(defaults) = self.thread_launch_defaults() else {
            return;
        };
        let state = self.terminal_ui.map.thread(&key);
        let dock_ids: Vec<String> = self
            .right_panel
            .map
            .thread(&key)
            .surfaces
            .iter()
            .filter_map(|surface| match surface {
                RightPanelSurface::Terminal { terminal_ids, .. } => Some(terminal_ids.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        let server_ids: Vec<String> = self
            .terminal_metadata
            .iter()
            .filter(|summary| summary.thread_id == defaults.thread_id)
            .map(|summary| summary.terminal_id.clone())
            .collect();
        let existing: Vec<&str> = state
            .terminal_ids
            .iter()
            .chain(server_ids.iter())
            .chain(dock_ids.iter())
            .map(String::as_str)
            .collect();
        let id = next_terminal_id(existing.iter().copied());
        let changed = match split_vertical {
            None => self.terminal_ui.map.new_terminal(&key, &id),
            // The store rejects a split past the 4-per-group cap.
            Some(vertical) => self.terminal_ui.map.split_terminal(&key, &id, vertical),
        };
        if !changed {
            return;
        }
        self.terminal_ui.save();
        if let Some(client) = self.client.clone() {
            let payload = TerminalOpenInput {
                cols: None,
                rows: None,
                cwd: tnes(&defaults.cwd),
                env: Some(Some(
                    defaults
                        .env
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                        .collect(),
                )),
                terminal_id: tnes(&id),
                thread_id: tnes(&defaults.thread_id),
                worktree_path: Some(Some(defaults.worktree.as_deref().map(tnes))),
            };
            cx.spawn(async move |_, _| {
                // Fire-and-forget: attach re-opens on failure anyway.
                let _ = client.call::<TerminalOpen>(&payload).await;
            })
            .detach();
        }
        self.focus_active_terminal(&key, cx);
        cx.notify();
    }

    /// The close flow (tab ×, trash button, `mod+w`, session exit): optimistic
    /// store removal + suppression, then `terminal.close {deleteHistory}` with
    /// Electron's `exit\n` write fallback. Idempotent — the exited path and a
    /// user close can race harmlessly.
    pub(super) fn terminal_close(&mut self, terminal_id: &str, cx: &mut Context<Self>) {
        let Some(key) = self.terminal_thread_key() else {
            return;
        };
        let Some(open) = &self.thread else {
            return;
        };
        let thread_id = open.id.0.clone();
        if self.terminal_ui.map.close_terminal(&key, terminal_id) {
            self.terminal_ui.save();
        }
        let map_key = (key, terminal_id.to_string());
        self.terminal_views.remove(&map_key);
        self.terminal_view_subs.remove(&map_key);
        if let Some(client) = self.client.clone() {
            let terminal_id = terminal_id.to_string();
            cx.spawn(async move |_, _| {
                let payload = TerminalCloseInput {
                    delete_history: Some(Some(true)),
                    terminal_id: Some(Some(tnes(&terminal_id))),
                    thread_id: tnes(&thread_id),
                };
                if client.call::<TerminalClose>(&payload).await.is_err() {
                    let fallback = TerminalWriteInput {
                        data: "exit\n".to_string(),
                        terminal_id: tnes(&terminal_id),
                        thread_id: tnes(&thread_id),
                    };
                    let _ = client.call::<TerminalWrite>(&fallback).await;
                }
            })
            .detach();
        }
        cx.notify();
    }

    /// Terminal-context `mod+w`.
    pub(super) fn terminal_close_active(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.terminal_thread_key() else {
            cx.propagate();
            return;
        };
        let state = self.terminal_ui.map.thread(&key);
        if !state.terminal_open || state.active_terminal_id.is_empty() {
            cx.propagate();
            return;
        }
        let id = state.active_terminal_id.clone();
        self.terminal_close(&id, cx);
    }

    fn terminal_set_active(&mut self, terminal_id: &str, cx: &mut Context<Self>) {
        let Some(key) = self.terminal_thread_key() else {
            return;
        };
        if self.terminal_ui.map.set_active_terminal(&key, terminal_id) {
            self.terminal_ui.save();
        }
        let map_key = (key, terminal_id.to_string());
        if let Some(view) = self.terminal_views.get(&map_key) {
            view.update(cx, |view, cx| view.request_focus(cx));
        }
        cx.notify();
    }

    /// Create/refresh the `TerminalView` entities for the drawer's ids, drop
    /// the ones whose tab is gone, and recreate any whose launch parameters
    /// changed (Electron's remount-on-deps effect).
    fn ensure_terminal_views(
        &mut self,
        key: &str,
        state: &ThreadTerminalUiState,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(defaults) = self.thread_launch_defaults() else {
            return;
        };
        let live: HashSet<(String, String)> = state
            .terminal_ids
            .iter()
            .map(|id| (key.to_string(), id.clone()))
            .collect();
        self.terminal_views
            .retain(|map_key, _| live.contains(map_key));
        self.terminal_view_subs
            .retain(|map_key, _| live.contains(map_key));

        for id in &state.terminal_ids {
            // Prefer the server's recorded cwd for an existing session so
            // re-attaching cannot move (and thereby restart) the shell.
            let summary = self.terminal_metadata.iter().find(|summary| {
                summary.thread_id == defaults.thread_id && summary.terminal_id == *id
            });
            let cwd = summary
                .map(|summary| summary.cwd.clone())
                .filter(|cwd| !cwd.is_empty())
                .unwrap_or_else(|| defaults.cwd.clone());
            let launch = TerminalLaunch {
                thread_id: defaults.thread_id.clone(),
                terminal_id: id.clone(),
                cwd,
                worktree_path: Some(defaults.worktree.clone()),
                env: defaults.env.clone(),
            };
            let map_key = (key.to_string(), id.clone());
            let stale = self
                .terminal_views
                .get(&map_key)
                .is_some_and(|view| view.read(cx).launch() != &launch);
            if stale {
                self.terminal_views.remove(&map_key);
                self.terminal_view_subs.remove(&map_key);
            }
            if !self.terminal_views.contains_key(&map_key) {
                let view = cx.new(|cx| TerminalView::new(client.clone(), launch, cx));
                let exited_id = id.clone();
                let subscription = cx.subscribe(&view, move |this, _, event, cx| match event {
                    TerminalViewEvent::SessionExited => {
                        this.terminal_close(&exited_id, cx);
                    }
                    TerminalViewEvent::AddToChat(selection) => {
                        this.add_terminal_context(selection, cx);
                    }
                });
                self.terminal_views.insert(map_key.clone(), view);
                self.terminal_view_subs
                    .insert(map_key.clone(), subscription);
            }
            let label = self.terminal_label(key, id);
            if let Some(view) = self.terminal_views.get(&map_key) {
                let active = state.active_terminal_id == *id;
                view.update(cx, |view, _| {
                    view.set_active(active);
                    view.set_label(label);
                });
            }
        }
    }

    // ---- drag resize -------------------------------------------------------

    pub(super) fn terminal_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let viewport_height = f64::from(f32::from(window.viewport_size().height));
        let Some(drag) = &mut self.terminal_drag else {
            return;
        };
        // Dragging the handle up grows the drawer.
        let delta = f64::from(drag.start_y - f32::from(event.position.y));
        let next = clamp_drawer_height(drag.start_height + delta, viewport_height);
        if next != drag.current_height {
            drag.current_height = next;
            cx.notify();
        }
    }

    pub(super) fn terminal_drag_end(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.terminal_drag.take() else {
            return;
        };
        if let Some(key) = self.terminal_thread_key()
            && self
                .terminal_ui
                .map
                .set_terminal_height(&key, drag.current_height)
        {
            self.terminal_ui.save();
        }
        cx.notify();
    }

    // ---- render ------------------------------------------------------------

    /// The drawer, mounted at the bottom of the chat column when the open
    /// thread has it toggled on (and the workspace root is known — there is
    /// nowhere to launch a shell before the shell snapshot arrives).
    pub(super) fn render_terminal_drawer(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let key = self.terminal_thread_key()?;
        let state = self.terminal_ui.map.thread(&key);
        if !state.terminal_open {
            return None;
        }
        self.thread_launch_defaults()?;
        self.ensure_terminal_views(&key, &state, cx);

        let height = self
            .terminal_drag
            .as_ref()
            .map(|drag| drag.current_height)
            .unwrap_or(state.terminal_height);
        let height = clamp_drawer_height(height, f64::from(self.viewport_height));

        let body = if state.terminal_ids.is_empty() {
            self.render_drawer_empty_state(cx)
        } else {
            let mut row = h_flex().size_full().min_h_0();
            if state.terminal_ids.len() > 1 {
                row = row.child(self.render_drawer_sidebar(&key, &state, cx));
            }
            let mut viewport = div()
                .relative()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .h_full()
                .child(self.render_drawer_panes(&key, &state, cx));
            // With the sidebar hidden the actions float over the viewport.
            if state.terminal_ids.len() <= 1 {
                viewport = viewport.child(
                    h_flex()
                        .absolute()
                        .top_2()
                        .right_2()
                        .gap_0p5()
                        .items_center()
                        .rounded(px(6.))
                        .bg(cx.theme().background.opacity(0.85))
                        .px_1()
                        .children(self.render_drawer_actions(&state, cx)),
                );
            }
            row.child(viewport).into_any_element()
        };

        let start_height = height;
        Some(
            div()
                .id("terminal-drawer")
                .relative()
                .w_full()
                .flex_shrink_0()
                .h(px(height as f32))
                .border_t_1()
                .border_color(cx.theme().border.opacity(0.8))
                .bg(cx.theme().background)
                .child(div().size_full().min_h_0().child(body))
                .child(
                    // The 6px grab strip along the top edge (Electron's
                    // cursor-row-resize handle); z-order comes from paint
                    // order — it is the container's last child.
                    div()
                        .id("terminal-drawer-handle")
                        .absolute()
                        .top(px(-2.))
                        .left_0()
                        .right_0()
                        .h(px(6.))
                        .cursor_row_resize()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                this.terminal_drag = Some(TerminalDrag {
                                    start_y: f32::from(event.position.y),
                                    start_height,
                                    current_height: start_height,
                                });
                                cx.notify();
                            }),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_drawer_empty_state(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No terminal sessions for this thread yet."),
            )
            .child(
                Button::new("terminal-empty-new")
                    .label("New Terminal")
                    .outline()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.terminal_new(cx))),
            )
            .into_any_element()
    }

    /// The four session actions (split right, split down, new, close),
    /// shared between the floating cluster and the sidebar header. Splits
    /// disable at Electron's 4-per-group cap with the reason in the tooltip.
    fn render_drawer_actions(
        &self,
        state: &ThreadTerminalUiState,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let groups = sorted_groups_for_display(state);
        let active_group_len = groups
            .iter()
            .find(|group| group.id == state.active_terminal_group_id)
            .or_else(|| groups.first())
            .map(|group| group.terminal_ids.len())
            .unwrap_or(0);
        let split_full = active_group_len >= MAX_TERMINALS_PER_GROUP;
        let split_tooltip = |label: &str| -> SharedString {
            if split_full {
                format!("{label} (max {MAX_TERMINALS_PER_GROUP} per group)").into()
            } else {
                label.to_string().into()
            }
        };
        let divider = || {
            div()
                .w(px(1.))
                .h_4()
                .bg(cx.theme().border.opacity(0.8))
                .into_any_element()
        };
        vec![
            Button::new("terminal-split-h")
                .icon(Icon::new(VitreIcon::SquareSplitHorizontal).with_size(px(13.)))
                .ghost()
                .xsmall()
                .disabled(split_full)
                .tooltip(split_tooltip("Split terminal right"))
                .on_click(cx.listener(|this, _, _, cx| this.terminal_split(false, cx)))
                .into_any_element(),
            Button::new("terminal-split-v")
                .icon(Icon::new(VitreIcon::SquareSplitVertical).with_size(px(13.)))
                .ghost()
                .xsmall()
                .disabled(split_full)
                .tooltip(split_tooltip("Split terminal down"))
                .on_click(cx.listener(|this, _, _, cx| this.terminal_split(true, cx)))
                .into_any_element(),
            divider(),
            Button::new("terminal-new")
                .icon(Icon::new(IconName::Plus).with_size(px(13.)))
                .ghost()
                .xsmall()
                .tooltip("New terminal")
                .on_click(cx.listener(|this, _, _, cx| this.terminal_new(cx)))
                .into_any_element(),
            divider(),
            Button::new("terminal-close-active")
                .icon(Icon::new(VitreIcon::Trash2).with_size(px(13.)))
                .ghost()
                .xsmall()
                .tooltip("Close terminal")
                .on_click(cx.listener(|this, _, _, cx| this.terminal_close_active(cx)))
                .into_any_element(),
        ]
    }

    /// The `w-36` session list shown once a second terminal exists: header
    /// action row, then each display group under a "Group n" heading.
    fn render_drawer_sidebar(
        &self,
        key: &str,
        state: &ThreadTerminalUiState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let groups = sorted_groups_for_display(state);
        let multi_group = groups.len() > 1;
        let mut list = v_flex()
            .id("terminal-drawer-list")
            .flex_1()
            .min_h_0()
            .px_1()
            .pb_1()
            .gap_0p5()
            .overflow_y_scroll();
        let mut row_index = 0usize;
        for (group_index, group) in groups.iter().enumerate() {
            if multi_group {
                list = list.child(
                    div()
                        .px_1p5()
                        .pt_1()
                        .text_size(px(10.))
                        .font_medium()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Group {}", group_index + 1)),
                );
            }
            for id in &group.terminal_ids {
                list = list.child(self.render_drawer_row(key, state, id, row_index, cx));
                row_index += 1;
            }
        }
        v_flex()
            .w(px(144.))
            .flex_shrink_0()
            .h_full()
            .min_h_0()
            .border_r_1()
            .border_color(cx.theme().border.opacity(0.8))
            .child(
                h_flex()
                    .h(px(26.))
                    .px_1()
                    .items_center()
                    .justify_end()
                    .gap_0p5()
                    .flex_shrink_0()
                    .children(self.render_drawer_actions(state, cx)),
            )
            .child(list)
            .into_any_element()
    }

    fn render_drawer_row(
        &self,
        key: &str,
        state: &ThreadTerminalUiState,
        terminal_id: &str,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_active = state.active_terminal_id == terminal_id;
        let label = self.terminal_label(key, terminal_id);
        let group: SharedString = format!("terminal-row-{index}").into();
        let activate_id = terminal_id.to_string();
        let close_id = terminal_id.to_string();
        h_flex()
            .id(SharedString::from(format!("terminal-row-item-{index}")))
            .group(group.clone())
            .h(px(24.))
            .px_1p5()
            .gap_1()
            .items_center()
            .rounded(px(4.))
            .text_size(px(11.))
            .map(|this| {
                if is_active {
                    this.bg(cx.theme().accent).text_color(cx.theme().foreground)
                } else {
                    this.text_color(cx.theme().muted_foreground).hover(|style| {
                        style
                            .bg(cx.theme().accent.opacity(0.6))
                            .text_color(cx.theme().foreground)
                    })
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    this.terminal_set_active(&activate_id, cx);
                }),
            )
            .child(Icon::new(IconName::SquareTerminal).size_3().flex_shrink_0())
            .child(div().flex_1().min_w_0().truncate().child(label))
            .child(
                div()
                    .invisible()
                    .group_hover(group, |style| style.visible())
                    .child(
                        Button::new(SharedString::from(format!("terminal-row-close-{index}")))
                            .icon(Icon::new(IconName::Close).size_3())
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.terminal_close(&close_id, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    /// Server label when the session reported one, `Terminal N` fallback.
    fn terminal_label(&self, _key: &str, terminal_id: &str) -> String {
        let thread_id = self.thread.as_ref().map(|open| open.id.0.as_str());
        self.terminal_metadata
            .iter()
            .find(|summary| {
                Some(summary.thread_id.as_str()) == thread_id && summary.terminal_id == terminal_id
            })
            .map(|summary| summary.label.trim())
            .filter(|label| !label.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| terminal_fallback_label(terminal_id))
    }

    /// The active group's panes: an equal-fraction split (row for horizontal,
    /// column for vertical), each pane hosting its `TerminalView`. The active
    /// pane carries the stronger border; pressing a pane activates it.
    fn render_drawer_panes(
        &self,
        key: &str,
        state: &ThreadTerminalUiState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let groups = sorted_groups_for_display(state);
        let active_group = groups
            .iter()
            .find(|group| group.id == state.active_terminal_group_id)
            .or_else(|| groups.first());
        let Some(active_group) = active_group else {
            return div().size_full().into_any_element();
        };
        let vertical = active_group.split_direction == Some(SplitDirection::Vertical);
        let split = active_group.terminal_ids.len() > 1;
        let mut panes = if vertical {
            v_flex().size_full().min_h_0().gap_1()
        } else {
            h_flex().size_full().min_w_0().gap_1()
        };
        for (index, id) in active_group.terminal_ids.iter().enumerate() {
            let view = self
                .terminal_views
                .get(&(key.to_string(), id.clone()))
                .cloned();
            let is_active = state.active_terminal_id == *id;
            let activate_id = id.clone();
            let mut pane = div()
                .id(SharedString::from(format!("terminal-pane-{index}")))
                .flex_1()
                // The row container is `h_flex` (items_center), so without an
                // explicit cross-axis size a pane collapses to zero height.
                .w_full()
                .h_full()
                .min_w_0()
                .min_h_0()
                .rounded(px(6.))
                .overflow_hidden()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        if let Some(drawer_key) = this.terminal_thread_key()
                            && this
                                .terminal_ui
                                .map
                                .set_active_terminal(&drawer_key, &activate_id)
                        {
                            this.terminal_ui.save();
                            cx.notify();
                        }
                    }),
                );
            if split {
                pane = pane.border_1().border_color(if is_active {
                    cx.theme().border
                } else {
                    cx.theme().border.opacity(0.7)
                });
            }
            panes = panes.child(pane.children(view.map(|view| div().size_full().child(view))));
        }
        div().size_full().p_1().child(panes).into_any_element()
    }
}
