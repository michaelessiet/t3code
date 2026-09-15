//! The right-panel dock: Electron's multi-surface tab strip
//! (`RightPanelTabs.tsx`) plus the ChatView glue around it — add handlers,
//! the close pipeline, activation side effects, and keyboard commands.
//!
//! Store semantics live in [`vitre_state::right_panel`] (a pure port of
//! `rightPanelStore.ts`); this module is persistence + render + interactions.
//! Surfaces are keyed per thread as `${environmentId}:${threadId}`; Vitre
//! adds a `:home` pseudo-thread so QuickSearch can still open files from the
//! home view, where Electron simply has no right panel.
//!
//! This slice hosts the `files`/`file` surfaces on the existing
//! [`crate::files::FilesPanel`] (Electron mounts one `FilePreviewPanel` for
//! both kinds the same way). The diff, terminal, search and browser surfaces
//! land with their own M3 slices; until then their menu entries are disabled
//! with a reason, mirroring Electron's disabled-with-tooltip pattern.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, ClickEvent, ClipboardItem, Context, Focusable as _, MouseButton, MouseDownEvent,
    SharedString, WeakEntity, Window, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    v_flex,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vitre_contracts::methods::PreviewClose;
use vitre_contracts::{PreviewCloseInput, PreviewTabId, TurnId};
use vitre_state::right_panel::{PERSISTED_VERSION, RightPanelMap, RightPanelSurface, SurfaceKind};

use crate::assets::VitreIcon;
use crate::files::RevealTarget;
use crate::lsp::positions::WirePosition;

use super::ChatApp;

#[cfg(debug_assertions)]
mod verification;

const FILE_NAME: &str = "right-panel-state.json";

fn clamp_mini(
    mut bounds: gpui::Bounds<gpui::Pixels>,
    size: gpui::Size<gpui::Pixels>,
) -> gpui::Bounds<gpui::Pixels> {
    bounds.size.width = bounds.size.width.max(px(360.)).min(size.width);
    bounds.size.height = bounds.size.height.max(px(240.)).min(size.height);
    bounds.origin.x = bounds
        .origin
        .x
        .max(px(0.))
        .min(size.width - bounds.size.width);
    bounds.origin.y = bounds
        .origin
        .y
        .max(px(0.))
        .min(size.height - bounds.size.height);
    bounds
}

/// Electron's `t3code:preview-panel-width` default / min. The max is 70% of
/// the viewport, applied at render time.
pub(super) const DEFAULT_PANEL_WIDTH: f32 = 640.;
pub(super) const MIN_PANEL_WIDTH: f32 = 360.;

/// The dock store plus its persistence. Electron persists on every store
/// change (zustand `persist`, no debounce); writes here are the same
/// best-effort as [`crate::sidebar_prefs::SidebarPrefs`].
pub(super) struct RightPanelPrefs {
    pub map: RightPanelMap,
    pub active_roots: HashMap<String, String>,
    /// Inline panel width in px, committed on drag end only.
    pub width: f32,
    path: PathBuf,
}

#[cfg(test)]
mod workspace_preferences_tests {
    use super::*;
    #[test]
    fn active_folder_survives_restart() {
        let directory = std::env::temp_dir().join(crate::chat::fresh_id("vitre-root-test"));
        let mut prefs = RightPanelPrefs::load(&directory);
        prefs
            .active_roots
            .insert("thread-1".into(), "/work/secondary".into());
        prefs.save();
        let restored = RightPanelPrefs::load(&directory);
        assert_eq!(
            restored.active_roots.get("thread-1").map(String::as_str),
            Some("/work/secondary")
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}

/// On-disk shape: Electron's persisted blob (`version` + `byThreadKey`) plus
/// the panel width Electron keeps in a separate localStorage key.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StoredRightPanel {
    version: u64,
    panel_width: Option<f32>,
    by_thread_key: Value,
    active_roots: HashMap<String, String>,
}

impl RightPanelPrefs {
    pub fn load(home: &Path) -> Self {
        let path = home.join(FILE_NAME);
        let stored = std::fs::read_to_string(&path)
            .ok()
            .and_then(
                |contents| match serde_json::from_str::<StoredRightPanel>(&contents) {
                    Ok(stored) => Some(stored),
                    Err(error) => {
                        eprintln!("[vitre] ignoring unreadable {FILE_NAME}: {error}");
                        None
                    }
                },
            )
            .unwrap_or_default();
        Self {
            map: RightPanelMap::from_persisted(&stored.by_thread_key),
            active_roots: stored.active_roots,
            width: stored
                .panel_width
                .filter(|width| width.is_finite())
                .unwrap_or(DEFAULT_PANEL_WIDTH)
                .max(MIN_PANEL_WIDTH),
            path,
        }
    }

    pub fn save(&self) {
        let stored = StoredRightPanel {
            version: PERSISTED_VERSION,
            panel_width: Some(self.width),
            by_thread_key: self.map.to_persisted(),
            active_roots: self.active_roots.clone(),
        };
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

/// A dock surface the add-menu / empty-state cards can offer.
struct SurfaceOffer {
    kind: SurfaceKind,
    label: &'static str,
    description: &'static str,
    icon: fn() -> Icon,
    /// `None` = available; `Some(reason)` renders disabled with the reason.
    disabled: Option<SharedString>,
}

impl ChatApp {
    pub(super) fn open_workspace_surface(
        &mut self,
        kind: SurfaceKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        self.right_panel.map.open(&key, kind);
        self.after_dock_change(true, window, cx);
    }
    /// Electron's `scopedThreadKey`: `${environmentId}:${threadId}`. `None`
    /// until the environment session exists. The home view (no open thread)
    /// uses a `:home` pseudo-thread — a Vitre extension, see module docs.
    pub(super) fn dock_thread_key(&self) -> Option<String> {
        let client = self.client.as_ref()?;
        let session = client.sessions().borrow().clone()?;
        let environment = session.config.environment.environment_id.0.clone();
        let thread = self
            .thread
            .as_ref()
            .map(|open| open.id.0.clone())
            .unwrap_or_else(|| "home".into());
        Some(format!("{environment}:{thread}"))
    }

    pub(super) fn dock_open(&self) -> bool {
        self.dock_thread_key()
            .is_some_and(|key| self.right_panel.map.is_open(&key))
    }

    pub(super) fn active_preview_panel(
        &self,
        _cx: &Context<Self>,
    ) -> Option<gpui::Entity<super::preview::PreviewPanel>> {
        let thread_key = self.dock_thread_key()?;
        let surface = self.right_panel.map.active_surface(&thread_key)?;
        if surface.kind() != SurfaceKind::Preview {
            return None;
        }
        self.preview_panels
            .get(&format!("{thread_key}|{}", surface.id()))
            .cloned()
    }

    /// Ensure the active browser surface owns a native child view and hide
    /// every inactive one. Child webviews live above GPUI in the native view
    /// hierarchy, so omitting them from the element tree is not sufficient.
    pub(super) fn sync_active_preview_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.is_some()
            || self.quick_search.is_some()
            || self.model_picker.is_some()
            || self.command_palette.is_some()
            || window.has_active_dialog(cx)
            || window.has_active_sheet(cx)
        {
            for panel in self.preview_panels.values() {
                panel.update(cx, |panel, cx| panel.hide(cx));
            }
            return;
        }
        if !self.dock_open() {
            let mini = self.preview_mini.as_ref().filter(|key| {
                self.dock_thread_key()
                    .is_some_and(|thread| key.starts_with(&format!("{thread}|")))
            });
            for (key, panel) in &self.preview_panels {
                panel.update(cx, |panel, cx| {
                    if Some(key) == mini {
                        panel.show(cx)
                    } else {
                        panel.hide(cx)
                    }
                });
            }
            return;
        }
        let Some(thread_key) = self.dock_thread_key() else {
            for panel in self.preview_panels.values() {
                panel.update(cx, |panel, cx| panel.hide(cx));
            }
            return;
        };
        let Some(RightPanelSurface::Preview { id, resource_id }) =
            self.right_panel.map.active_surface(&thread_key).cloned()
        else {
            for panel in self.preview_panels.values() {
                panel.update(cx, |panel, cx| panel.hide(cx));
            }
            return;
        };
        let cache_key = format!("{thread_key}|{id}");
        for (key, panel) in &self.preview_panels {
            panel.update(cx, |panel, cx| {
                if key == &cache_key {
                    panel.show(cx);
                } else {
                    panel.hide(cx);
                }
            });
        }
        let panel = if let Some(panel) = self.preview_panels.get(&cache_key).cloned() {
            panel
        } else {
            // Retain session metadata in the dock, but bound native guests.
            // Reopening an evicted surface restores it through preview.list.
            if !self.reserve_preview_guest(&cache_key, cx) {
                return;
            }
            let Some(client) = self.client.clone() else {
                return;
            };
            let Some(thread_id) = self.thread.as_ref().map(|thread| thread.id.clone()) else {
                return;
            };
            let tab_id = resource_id.map(vitre_contracts::PreviewTabId);
            let panel = cx.new(|cx| {
                super::preview::PreviewPanel::new(client, thread_id, tab_id, None, window, cx)
            });
            let event_key = cache_key.clone();
            let event_thread_key = thread_key.clone();
            self.register_preview_events(&panel, event_key, event_thread_key, window, cx);
            self.preview_panels.insert(cache_key, panel.clone());
            panel
        };
        panel.update(cx, |panel, cx| panel.show(cx));
    }

    pub(super) fn register_preview_events(
        &mut self,
        panel: &gpui::Entity<super::preview::PreviewPanel>,
        event_key: String,
        event_thread_key: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview_subscriptions
            .retain(|id, _| self.preview_panels.values().any(|p| p.entity_id() == *id));
        self.preview_subscriptions.insert(
            panel.entity_id(),
            cx.subscribe_in(
                panel,
                window,
                move |this, panel, event: &super::preview::PreviewPanelEvent, window, cx| {
                    match event {
                        super::preview::PreviewPanelEvent::SessionClosed => {
                            let keys = this
                                .preview_panels
                                .iter()
                                .filter(|(_, p)| *p == panel)
                                .map(|(key, _)| key.clone())
                                .collect::<Vec<_>>();
                            for key in keys {
                                if let Some((thread, surface)) = key.split_once('|') {
                                    this.right_panel.map.close_surface(thread, surface);
                                }
                                if this.preview_mini.as_ref() == Some(&key) {
                                    this.preview_mini = None;
                                }
                                this.preview_panels.remove(&key);
                            }
                            panel.update(cx, |p, cx| p.hide(cx));
                            this.right_panel.save();
                            cx.notify();
                        }
                        super::preview::PreviewPanelEvent::ToggleMiniPlayer => {
                            let key = this
                                .preview_panels
                                .iter()
                                .find(|(_, p)| *p == panel)
                                .map(|(k, _)| k.clone())
                                .unwrap_or_else(|| event_key.clone());
                            if this.preview_mini.as_ref() == Some(&key) {
                                this.preview_mini = None;
                                let tab = key.split_once("|browser:").map(|(_, tab)| tab);
                                this.right_panel.map.open_browser(&event_thread_key, tab);
                            } else {
                                this.preview_mini = Some(key);
                                this.right_panel.map.close(&event_thread_key);
                            }
                            this.right_panel.save();
                            cx.notify();
                        }
                        super::preview::PreviewPanelEvent::Picked(text) => {
                            if this.dock_thread_key().as_ref() != Some(&event_thread_key) {
                                return;
                            }
                            let existing = this.composer.read(cx).value().to_string();
                            this.composer.update(cx, |input, cx| {
                                input.set_value(format!("{existing}\n\n{text}\n\n"), window, cx)
                            });
                            this.composer.read(cx).focus_handle(cx).focus(window, cx);
                        }
                        super::preview::PreviewPanelEvent::PickedImage(png) => {
                            if this.dock_thread_key().as_ref() != Some(&event_thread_key) {
                                return;
                            }
                            use base64::Engine as _;
                            if this.pending_attachments.len() < super::MAX_ATTACHMENTS
                                && (png.len() as u64) <= super::MAX_ATTACHMENT_BYTES
                            {
                                this.pending_attachments.push(super::PendingAttachment {
                                    name: "preview-annotation.png".into(),
                                    mime_type: "image/png".into(),
                                    size_bytes: png.len() as i64,
                                    data_url: format!(
                                        "data:image/png;base64,{}",
                                        base64::engine::general_purpose::STANDARD.encode(png)
                                    ),
                                    is_image: true,
                                });
                                cx.notify();
                            }
                        }
                        super::preview::PreviewPanelEvent::SessionOpened(tab_id) => {
                            this.right_panel
                                .map
                                .open_browser(&event_thread_key, Some(&tab_id.0));
                            this.right_panel.save();
                            let new_key = format!("{event_thread_key}|browser:{}", tab_id.0);
                            this.preview_panels.retain(|_, existing| existing != panel);
                            this.preview_panels.insert(new_key, panel.clone());
                            cx.notify();
                        }
                    }
                },
            ),
        );
    }

    pub(super) fn reserve_preview_guest(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        if self.preview_panels.contains_key(key) || self.preview_panels.len() < 4 {
            return true;
        }
        let active = self.active_preview_panel(cx);
        let evict = self
            .preview_panels
            .iter()
            .find(|(k, p)| {
                Some(*k) != self.preview_mini.as_ref()
                    && active.as_ref() != Some(*p)
                    && !p.read(cx).is_recording()
            })
            .map(|(k, _)| k.clone());
        if let Some(evict) = evict {
            if let Some(p) = self.preview_panels.remove(&evict) {
                p.update(cx, |p, cx| p.hide(cx));
            }
            true
        } else {
            false
        }
    }

    pub(super) fn move_preview_mini(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some((start, mut bounds, resize)) = self.preview_mini_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.preview_mini_drag = None;
            cx.notify();
            return;
        }
        let delta = event.position - start;
        if resize {
            bounds.size.width += delta.x;
            bounds.size.height += delta.y;
        } else {
            bounds.origin += delta;
        }
        self.preview_mini_bounds = Some(clamp_mini(bounds, window.viewport_size()));
        cx.notify();
    }

    pub(super) fn render_preview_mini(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if self.settings.is_some()
            || self.quick_search.is_some()
            || self.model_picker.is_some()
            || self.command_palette.is_some()
            || window.has_active_dialog(cx)
            || window.has_active_sheet(cx)
        {
            return None;
        }
        let key = self.preview_mini.as_ref()?;
        if !key.starts_with(&format!("{}|", self.dock_thread_key()?)) || self.dock_open() {
            return None;
        }
        let panel = self.preview_panels.get(key)?.clone();
        let size = window.viewport_size();
        let bounds = clamp_mini(
            self.preview_mini_bounds.unwrap_or_else(|| {
                gpui::Bounds::new(
                    gpui::point(size.width - px(476.), size.height - px(470.)),
                    gpui::size(px(460.), px(320.)),
                )
            }),
            size,
        );
        self.preview_mini_bounds = Some(bounds);
        panel.update(cx, |p, cx| p.show(cx));
        Some(
            div()
                .absolute()
                .left(bounds.origin.x)
                .top(bounds.origin.y)
                .w(bounds.size.width)
                .h(bounds.size.height)
                .shadow_2xl()
                .bg(cx.theme().background)
                .border_1()
                .border_color(cx.theme().border)
                .overflow_hidden()
                .child(
                    v_flex()
                        .size_full()
                        .child(
                            h_flex()
                                .h(px(24.))
                                .flex_none()
                                .px_2()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .flex_1()
                                        .text_xs()
                                        .cursor_move()
                                        .child("Browser mini player")
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                move |app, event: &MouseDownEvent, _, cx| {
                                                    app.preview_mini_drag =
                                                        Some((event.position, bounds, false));
                                                    cx.notify();
                                                },
                                            ),
                                        ),
                                )
                                .child(
                                    Button::new("mini-close")
                                        .label("×")
                                        .ghost()
                                        .xsmall()
                                        .tooltip("Hide mini player")
                                        .on_click(cx.listener(|app, _, _, cx| {
                                            app.preview_mini = None;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(div().flex_1().min_h_0().child(panel))
                        .child(
                            div()
                                .h(px(12.))
                                .flex_none()
                                .cursor_nwse_resize()
                                .text_xs()
                                .text_right()
                                .child("◢")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |app, event: &MouseDownEvent, _, cx| {
                                        app.preview_mini_drag =
                                            Some((event.position, bounds, true));
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    /// `rightPanel.toggle` (`mod+j` / `mod+alt+b`): hide when open (surfaces
    /// retained), show when hidden — with zero surfaces that shows the
    /// empty-state cards.
    pub(super) fn toggle_right_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        let changed = if self.right_panel.map.is_open(&key) {
            self.right_panel.map.close(&key)
        } else {
            self.right_panel.map.toggle_visibility(&key)
        };
        if changed {
            self.after_dock_change(false, window, cx);
        }
    }

    pub(super) fn preview_toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        if self.right_panel.map.toggle(&key, SurfaceKind::Preview) {
            self.after_dock_change(true, window, cx);
        }
    }

    pub(super) fn diff_toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_root().is_none() {
            return;
        }
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        if self.right_panel.map.toggle(&key, SurfaceKind::Diff) {
            self.after_dock_change(true, window, cx);
        }
    }

    pub(super) fn preview_refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_preview_panel(cx) {
            panel.update(cx, |panel, cx| panel.refresh(cx));
        } else {
            cx.propagate();
        }
    }

    pub(super) fn preview_focus_url(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_preview_panel(cx) {
            panel.update(cx, |panel, cx| panel.focus_url(window, cx));
        } else {
            cx.propagate();
        }
    }

    pub(super) fn preview_zoom(&mut self, delta: i32, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_preview_panel(cx) {
            panel.update(cx, |panel, cx| panel.change_zoom(delta, cx));
        } else {
            cx.propagate();
        }
    }

    pub(super) fn preview_reset_zoom(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_preview_panel(cx) {
            panel.update(cx, |panel, cx| panel.set_zoom(1.0, cx));
        } else {
            cx.propagate();
        }
    }

    /// Open a file tab (QuickSearch, palette). `line` is one-based. Always
    /// bumps the reveal request, so re-opening an open file re-scrolls.
    pub(super) fn dock_open_file(
        &mut self,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        let root = self.root_for_file_tab();
        self.right_panel
            .map
            .open_file(&key, &path, line, None, root.as_deref());
        self.after_dock_change(true, window, cx);
    }

    /// Open the standalone files explorer surface (palette New file/folder,
    /// add menu, empty-state card).
    pub(super) fn dock_open_files_surface(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        self.right_panel.map.open(&key, SurfaceKind::Files);
        self.after_dock_change(false, window, cx);
    }

    /// Open the diff surface scoped to one turn — the changed-files card's
    /// "Open diff" button, file rows and preview chips. Ports ChatView's
    /// `onOpenTurnDiff` (selectTurn + rightPanel.open("diff")).
    pub(super) fn dock_open_turn_diff(
        &mut self,
        turn_id: TurnId,
        file_path: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        self.right_panel.map.open(&key, SurfaceKind::Diff);
        // Creates/reuses the DiffPanel via sync_active_file_surface.
        self.after_dock_change(false, window, cx);
        if let Some(panel) = self.diff.clone() {
            panel.update(cx, |panel, cx| panel.open_turn(turn_id, file_path, cx));
        }
    }

    fn dock_add_surface(&mut self, kind: SurfaceKind, window: &mut Window, cx: &mut Context<Self>) {
        // Only kinds the landed slices can host; the menu disables the rest,
        // so an unknown kind arriving here is a bug, not user input.
        match kind {
            SurfaceKind::Files => self.dock_open_files_surface(window, cx),
            SurfaceKind::Diff => {
                let Some(key) = self.dock_thread_key() else {
                    return;
                };
                self.right_panel.map.open(&key, SurfaceKind::Diff);
                self.after_dock_change(false, window, cx);
            }
            SurfaceKind::Search | SurfaceKind::Graph => {
                let Some(key) = self.dock_thread_key() else {
                    return;
                };
                self.right_panel.map.open(&key, kind);
                self.after_dock_change(true, window, cx);
            }
            SurfaceKind::Terminal => {
                self.dock_terminal_create(cx);
                self.after_dock_change(true, window, cx);
            }
            SurfaceKind::Preview => {
                let Some(key) = self.dock_thread_key() else {
                    return;
                };
                self.right_panel.map.open(&key, SurfaceKind::Preview);
                self.after_dock_change(true, window, cx);
            }
            _ => {}
        }
    }

    fn dock_activate_surface(
        &mut self,
        surface_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        if self.right_panel.map.activate_surface(&key, surface_id) {
            self.after_dock_change(true, window, cx);
        }
    }

    /// The close pipeline. Electron runs unsaved-guard → resource teardown →
    /// store mutation; Vitre's editor autosaves (the guard proceeds
    /// immediately) and no closeable surface owns server resources yet, so
    /// only the store mutation remains in this slice.
    fn dock_close_surface(
        &mut self,
        surface_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        self.teardown_preview_surface(&key, surface_id, cx);
        if self.right_panel.map.close_surface(&key, surface_id) {
            self.after_dock_change(false, window, cx);
        }
    }

    fn teardown_preview_surface(
        &mut self,
        thread_key: &str,
        surface_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(RightPanelSurface::Preview { resource_id, .. }) = self
            .right_panel
            .map
            .thread(thread_key)
            .surfaces
            .iter()
            .find(|surface| surface.id() == surface_id)
            .cloned()
        else {
            return;
        };
        let cache_key = format!("{thread_key}|{surface_id}");
        if let Some(panel) = self.preview_panels.remove(&cache_key) {
            panel.update(cx, |panel, cx| panel.hide(cx));
        }
        let (Some(client), Some(thread_id), Some(tab_id)) = (
            self.client.clone(),
            self.thread.as_ref().map(|thread| thread.id.clone()),
            resource_id,
        ) else {
            return;
        };
        cx.spawn(async move |_, _| {
            let _ = client
                .call::<PreviewClose>(&PreviewCloseInput {
                    thread_id,
                    tab_id: Some(Some(PreviewTabId(tab_id))),
                })
                .await;
        })
        .detach();
    }

    fn dock_close_others(&mut self, surface_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        let closing: Vec<String> = self
            .right_panel
            .map
            .thread(&key)
            .surfaces
            .iter()
            .filter(|surface| surface.id() != surface_id)
            .filter(|surface| surface.kind() == SurfaceKind::Preview)
            .map(|surface| surface.id().to_string())
            .collect();
        for id in closing {
            self.teardown_preview_surface(&key, &id, cx);
        }
        if self.right_panel.map.close_other_surfaces(&key, surface_id) {
            self.after_dock_change(false, window, cx);
        }
    }

    fn dock_close_to_right(
        &mut self,
        surface_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        let closing: Vec<String> = self
            .right_panel
            .map
            .thread(&key)
            .surfaces
            .iter()
            .skip_while(|surface| surface.id() != surface_id)
            .skip(1)
            .filter(|surface| surface.kind() == SurfaceKind::Preview)
            .map(|surface| surface.id().to_string())
            .collect();
        for id in closing {
            self.teardown_preview_surface(&key, &id, cx);
        }
        if self
            .right_panel
            .map
            .close_surfaces_to_right(&key, surface_id)
        {
            self.after_dock_change(false, window, cx);
        }
    }

    fn dock_close_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        let closing: Vec<String> = self
            .right_panel
            .map
            .thread(&key)
            .surfaces
            .iter()
            .filter(|surface| surface.kind() == SurfaceKind::Preview)
            .map(|surface| surface.id().to_string())
            .collect();
        for id in closing {
            self.teardown_preview_surface(&key, &id, cx);
        }
        if self.right_panel.map.close_all_surfaces(&key) {
            self.after_dock_change(false, window, cx);
        }
    }

    /// `rightPanel.closeSurface` (`mod+w`). Propagates when there is no
    /// active surface so any platform default still runs (Electron reports
    /// the command unhandled the same way).
    pub(super) fn dock_close_active_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.dock_thread_key() else {
            cx.propagate();
            return;
        };
        let Some(surface) = self.right_panel.map.active_surface(&key) else {
            cx.propagate();
            return;
        };
        let id = surface.id().to_string();
        self.dock_close_surface(&id, window, cx);
    }

    /// `rightPanel.nextSurface` / `previousSurface` (`mod+shift+]` / `[`),
    /// wrapping. Falls through with no active surface; claims the chord as a
    /// no-op with a single tab (Electron does both).
    pub(super) fn dock_cycle_surface(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.dock_thread_key() else {
            cx.propagate();
            return;
        };
        let state = self.right_panel.map.thread(&key);
        let Some(active) = state.active_surface_id.clone() else {
            cx.propagate();
            return;
        };
        let Some(index) = state.surfaces.iter().position(|s| s.id() == active) else {
            cx.propagate();
            return;
        };
        let len = state.surfaces.len() as isize;
        let next = (index as isize + delta).rem_euclid(len) as usize;
        let next_id = state.surfaces[next].id().to_string();
        if next_id != active {
            self.dock_activate_surface(&next_id, window, cx);
        }
    }

    /// Persist, apply activation side effects (file reveal), repaint.
    pub(super) fn after_dock_change(
        &mut self,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.right_panel.save();
        self.sync_active_file_surface(focus, window, cx);
        cx.notify();
    }

    /// Bring the shared files panel in line with the active surface: mount it
    /// for `files`/`file`, and apply a file surface's un-applied reveal
    /// request. Applied requests are tracked per `(thread, surface)` so
    /// restores and thread switches replay the open without re-scrolling on
    /// every frame. `focus` distinguishes a user-initiated open (focus the
    /// editor, like Electron arming editor focus) from a restore (leave focus
    /// where it is).
    pub(super) fn sync_active_file_surface(
        &mut self,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
        let Some(surface) = self.right_panel.map.active_surface(&key).cloned() else {
            return;
        };
        // This also runs per frame from `render`, so it must converge: after
        // the first application it does nothing until a *transition* — a new
        // reveal request, a surface activation (including a close fallback),
        // or the dock's thread changing under it. It must not re-open the
        // surface's file on every frame, or browsing the tree while a file
        // tab is active would snap straight back.
        let key_changed = self.last_dock_sync_key.as_deref() != Some(key.as_str());
        self.last_dock_sync_key = Some(key.clone());
        // The files listing revalidates on the activation EDGE (this runs per
        // frame): another surface was active before, the dock's thread
        // changed, or the dock reopened (render clears the marker while the
        // dock is closed). Electron gets the same effect from the panel
        // remounting with `revalidateOnMount` when its surface is selected.
        let surface_marker = format!("{key}|{}", surface.id());
        let surface_activated =
            self.last_dock_active_surface.as_deref() != Some(surface_marker.as_str());
        self.last_dock_active_surface = Some(surface_marker);
        match surface {
            RightPanelSurface::Graph { .. } => {
                if let (Some(client), Some(cwd)) = (self.client.clone(), self.search_root())
                    && !self.graph_panels.contains_key(&cwd)
                {
                    let panel =
                        cx.new(|cx| crate::graph::GraphPanel::new(client, cwd.clone(), window, cx));
                    self._subscriptions.push(cx.subscribe_in(
                        &panel,
                        window,
                        |app, _, event: &crate::graph::GraphEvent, w, cx| match event {
                            crate::graph::GraphEvent::OpenFile { path, line } => {
                                app.dock_open_file(path.clone(), *line, w, cx)
                            }
                        },
                    ));
                    self.graph_panels.insert(cwd, panel);
                }
            }
            RightPanelSurface::Search { .. } => {
                if let (Some(client), Some(cwd)) = (self.client.clone(), self.search_root()) {
                    if !self.search_panels.contains_key(&cwd) {
                        let panel = cx.new(|cx| {
                            crate::search::SearchPanel::new(client, cwd.clone(), window, cx)
                        });
                        self._subscriptions.push(cx.subscribe_in(
                            &panel,
                            window,
                            |app, _, event: &crate::search::SearchEvent, w, cx| match event {
                                crate::search::SearchEvent::OpenFile { path, line } => {
                                    app.dock_open_file(path.clone(), Some(*line), w, cx)
                                }
                            },
                        ));
                        self.search_panels.insert(cwd.clone(), panel);
                    }
                    if focus {
                        self.search_panels[&cwd].update(cx, |panel, cx| panel.focus(window, cx));
                    }
                }
            }
            RightPanelSurface::Files { .. } => {
                self.ensure_files_panel(window, cx);
                if surface_activated && let Some(files) = self.files.clone() {
                    files.update(cx, |files, cx| files.revalidate_if_stale(cx));
                }
            }
            RightPanelSurface::Diff { .. } => self.ensure_diff_panel(window, cx),
            RightPanelSurface::File {
                id,
                relative_path,
                reveal_line,
                reveal_end_line,
                reveal_request_id,
                root_path,
                ..
            } => {
                let previous_root = self
                    .thread
                    .as_ref()
                    .and_then(|t| self.active_roots.get(&t.id.0))
                    .cloned();
                if let Some(thread) = &self.thread {
                    if let Some(root) = root_path {
                        self.active_roots.insert(thread.id.0.clone(), root);
                    } else {
                        self.active_roots.remove(&thread.id.0);
                    }
                }
                if previous_root.as_ref()
                    != self
                        .thread
                        .as_ref()
                        .and_then(|t| self.active_roots.get(&t.id.0))
                {
                    self.save_workspace_selection();
                    self.sync_git_status(cx);
                    self.sync_branch_toolbar(cx);
                }
                self.ensure_files_panel(window, cx);
                if surface_activated && let Some(files) = self.files.clone() {
                    files.update(cx, |files, cx| files.revalidate_if_stale(cx));
                }
                let applied_key = format!("{key}|{id}");
                let fresh = self.applied_reveals.get(&applied_key) != Some(&reveal_request_id);
                if !fresh && !focus && !key_changed && !surface_activated {
                    return;
                }
                let Some(files) = self.files.clone() else {
                    return;
                };
                // The stored position is applied once per request; re-showing
                // an already-revealed tab re-opens the file where it was.
                let position = if fresh {
                    self.applied_reveals.insert(applied_key, reveal_request_id);
                    // The store keeps both bounds one-based, as Electron's
                    // surface does; the editor works in zero-based lines.
                    reveal_line.map(|line| RevealTarget {
                        position: WirePosition {
                            line: line.saturating_sub(1),
                            character: 0,
                        },
                        end_line: reveal_end_line.map(|line| line.saturating_sub(1)),
                    })
                } else {
                    None
                };
                files.update(cx, |files, cx| {
                    if focus {
                        files.reveal(relative_path, position, window, cx);
                    } else {
                        files.reveal_unfocused(relative_path, position, window, cx);
                    }
                });
            }
            _ => {}
        }
    }

    /// What the add-menu and the empty state offer, in Electron's order.
    /// Graph is *hidden* (not disabled): Vitre does not surface the
    /// knowledge-graph setting, and Electron hides the entry when it is off.
    fn dock_surface_offers(&self) -> Vec<SurfaceOffer> {
        let project_open = self.search_root().is_some();
        vec![
            SurfaceOffer {
                kind: SurfaceKind::Preview,
                label: "Browser",
                description: "Open a local app or URL.",
                icon: || Icon::new(IconName::Globe),
                disabled: None,
            },
            SurfaceOffer {
                kind: SurfaceKind::Terminal,
                label: "Terminal",
                description: "Start a shell in this workspace.",
                icon: || Icon::new(IconName::SquareTerminal),
                disabled: (!project_open)
                    .then(|| "The terminal is only available when a project is open.".into()),
            },
            SurfaceOffer {
                kind: SurfaceKind::Files,
                label: "Files",
                description: "Browse and read workspace files.",
                icon: || Icon::new(VitreIcon::Files),
                disabled: (!project_open)
                    .then(|| "Files are only available when a project is open.".into()),
            },
            SurfaceOffer {
                kind: SurfaceKind::Search,
                label: "Search",
                description: "Find and replace across files.",
                icon: || Icon::new(VitreIcon::TextSearch),
                disabled: (!project_open).then(|| "Open a project to search its files.".into()),
            },
            SurfaceOffer {
                kind: SurfaceKind::Graph,
                label: "Graph",
                description: "Explore code relationships.",
                icon: || Icon::new(IconName::Globe),
                disabled: (!project_open).then(|| "Open a project to explore its graph.".into()),
            },
            SurfaceOffer {
                kind: SurfaceKind::Diff,
                label: "Diff",
                description: "Review changes in this thread.",
                icon: || Icon::new(VitreIcon::FileDiff),
                disabled: (!project_open)
                    .then(|| "The diff panel is only available when a project is open.".into()),
            },
        ]
    }

    /// The dock: tab strip + the active surface (or the empty-state cards).
    pub(super) fn render_right_panel(&mut self, key: &str, cx: &mut Context<Self>) -> AnyElement {
        let state = self.right_panel.map.thread(key).clone();
        let active_id = state.active_surface_id.clone();
        let count = state.surfaces.len();
        let selected = active_id.as_ref().map(|id| (key.to_owned(), id.clone()));
        if selected != self.dock_tabs_active {
            self.dock_tabs_active = selected;
            if let Some(index) = state
                .surfaces
                .iter()
                .position(|s| active_id.as_deref() == Some(s.id()))
            {
                self.dock_tabs_scroll.scroll_to_item(index);
            }
        }

        let mut tabs = h_flex()
            .id("dock-tab-scroll")
            .track_scroll(&self.dock_tabs_scroll)
            .gap_1()
            .min_w_0()
            .flex_1()
            .overflow_x_scroll();
        for (index, surface) in state.surfaces.iter().enumerate() {
            let is_active = active_id.as_deref() == Some(surface.id());
            tabs =
                tabs.child(self.render_dock_tab(surface, is_active, index + 1 == count, count, cx));
        }

        let mut bar = h_flex()
            .h(px(crate::ui::CHROME_HEIGHT))
            .px_2()
            .gap_1()
            .items_center()
            .flex_shrink_0()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(tabs);
        // Electron renders the "+" only when at least one surface exists; an
        // empty panel adds through the cards instead.
        if count > 0 {
            bar = bar.child(self.render_dock_add_button(cx));
        }
        bar = bar.child(
            Button::new("dock-maximize")
                .icon(if self.right_panel_maximized {
                    IconName::Minimize
                } else {
                    IconName::Maximize
                })
                .ghost()
                .xsmall()
                .tooltip(if self.right_panel_maximized {
                    "Restore panel"
                } else {
                    "Maximize panel"
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.right_panel_maximized = !this.right_panel_maximized;
                    cx.notify();
                })),
        );

        bar = bar.child(
            Button::new("dock-hide")
                .icon(IconName::PanelRightClose)
                .ghost()
                .xsmall()
                .tooltip("Hide panel")
                .on_click(cx.listener(|this, _, window, cx| this.toggle_right_panel(window, cx))),
        );

        let content = self.render_dock_content(&state.surfaces, active_id.as_deref(), cx);

        v_flex()
            .size_full()
            .min_w_0()
            .key_context("RightPanel")
            .bg(crate::glass::elevated(cx))
            .border_l_1()
            .border_color(cx.theme().border)
            .child(bar)
            .child(div().flex_1().min_h_0().child(content))
            .into_any_element()
    }

    fn render_dock_tab(
        &self,
        surface: &RightPanelSurface,
        is_active: bool,
        is_last: bool,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let group: SharedString = format!("dock-tab-{}", surface.id()).into();
        let id = surface.id().to_string();
        let title = surface_title(surface);
        let dirty = matches!(surface, RightPanelSurface::File { relative_path, .. }
            if self.files.as_ref().and_then(|files| files.read(cx).active_file_status())
                .is_some_and(|(path, dirty)| dirty && path == relative_path));
        let running = matches!(surface, RightPanelSurface::Terminal { terminal_ids, .. }
        if terminal_ids.iter().any(|id| self.terminal_metadata.iter().any(|summary| {
            summary.terminal_id == *id && summary.has_running_subprocess
                && self.thread.as_ref().is_some_and(|thread| summary.thread_id == thread.id.0)
        })));
        let copy_path = match surface {
            RightPanelSurface::File {
                relative_path,
                root_path,
                ..
            } => Some(match root_path {
                Some(root) => format!("{}/{relative_path}", root.trim_end_matches(['/', '\\'])),
                None => relative_path.clone(),
            }),
            _ => None,
        };
        let chat = cx.entity().downgrade();

        let activate_id = id.clone();
        let middle_id = id.clone();
        let button_close_id = id.clone();

        h_flex()
            .id(SharedString::from(format!(
                "dock-tab-item-{}",
                surface.id()
            )))
            .group(group.clone())
            .h(px(32.))
            .min_w(px(112.))
            .max_w(px(216.))
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .rounded(px(8.))
            .px_2p5()
            .cursor_pointer()
            .border_1()
            .border_color(if is_active {
                cx.theme().border
            } else {
                gpui::transparent_black()
            })
            .text_sm()
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
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.dock_activate_surface(&activate_id, window, cx);
            }))
            // Middle-click closes without activating.
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    this.dock_close_surface(&middle_id, window, cx);
                }),
            )
            .context_menu(move |menu, _, _| {
                dock_tab_context_menu(menu, &chat, &id, copy_path.as_deref(), is_last, count)
            })
            .child(match surface {
                RightPanelSurface::File { relative_path, .. } => {
                    crate::icons::file_icon(relative_path, cx)
                }
                _ => surface_icon(surface)
                    .size(px(crate::ui::ICON))
                    .flex_shrink_0()
                    .into_any_element(),
            })
            .child(div().flex_1().min_w_0().truncate().child(title))
            .children(dirty.then(|| div().size(px(6.)).rounded_full().bg(cx.theme().warning)))
            .children(running.then(|| {
                div()
                    .id(SharedString::from(format!(
                        "terminal-running-{}",
                        surface.id()
                    )))
                    .size(px(6.))
                    .rounded_full()
                    .bg(cx.theme().success)
                    .tooltip(|window, cx| {
                        gpui_component::tooltip::Tooltip::new("Process running").build(window, cx)
                    })
            }))
            .child(
                // Keep the active tab's close affordance discoverable.
                div()
                    .when(!is_active, |this| this.invisible())
                    .group_hover(group, |style| style.visible())
                    .child(
                        Button::new(SharedString::from(format!(
                            "dock-tab-close-{}",
                            surface.id()
                        )))
                        .icon(Icon::new(IconName::Close).size_3())
                        .ghost()
                        .xsmall()
                        .tooltip("Close tab")
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.dock_close_surface(&button_close_id, window, cx);
                            },
                        )),
                    ),
            )
            .into_any_element()
    }

    fn render_dock_add_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let offers = self.dock_surface_offers();
        let chat = cx.entity().downgrade();
        Button::new("dock-add-surface")
            .icon(Icon::new(IconName::Plus).size_4())
            .ghost()
            .xsmall()
            .tooltip("Add panel surface")
            .dropdown_menu(move |mut menu, _, _| {
                for offer in &offers {
                    let chat = chat.clone();
                    let kind = offer.kind;
                    menu = menu.item(
                        PopupMenuItem::new(offer.label)
                            .icon((offer.icon)())
                            .disabled(offer.disabled.is_some())
                            .on_click(move |_, window, cx| {
                                let _ = chat.update(cx, |this, cx| {
                                    this.dock_add_surface(kind, window, cx);
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    fn render_dock_content(
        &mut self,
        surfaces: &[RightPanelSurface],
        active_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = active_id.and_then(|id| surfaces.iter().find(|s| s.id() == id));
        let Some(surface) = active else {
            return self.render_dock_empty_state(cx);
        };
        match surface.kind() {
            SurfaceKind::Files | SurfaceKind::File => match self.files.clone() {
                Some(panel) => div().size_full().child(panel).into_any_element(),
                None => dock_placeholder("Files are only available when a project is open.", cx),
            },
            SurfaceKind::Diff => match self.diff.clone() {
                Some(panel) => div().size_full().child(panel).into_any_element(),
                None => dock_placeholder(
                    "The diff panel is only available when a project is open.",
                    cx,
                ),
            },
            SurfaceKind::Terminal => match surface {
                RightPanelSurface::Terminal {
                    active_terminal_id, ..
                } => self.render_dock_terminal(
                    &self.dock_thread_key().unwrap_or_default(),
                    active_terminal_id,
                    cx,
                ),
                _ => unreachable!(),
            },
            SurfaceKind::Search => {
                match self
                    .search_root()
                    .and_then(|cwd| self.search_panels.get(&cwd).cloned())
                {
                    Some(panel) => div().size_full().child(panel).into_any_element(),
                    None => dock_placeholder("Open a project to search its files.", cx),
                }
            }
            SurfaceKind::Preview => match self.active_preview_panel(cx) {
                Some(panel) => div().size_full().child(panel).into_any_element(),
                None => dock_placeholder("Connecting browser preview…", cx),
            },
            SurfaceKind::Graph => match self
                .search_root()
                .and_then(|cwd| self.graph_panels.get(&cwd).cloned())
            {
                Some(panel) => div().size_full().child(panel).into_any_element(),
                None => dock_placeholder("Open a project to explore its graph.", cx),
            },
            SurfaceKind::Plan => {
                dock_placeholder("This surface is not yet available in Vitre.", cx)
            }
        }
    }

    /// Electron's empty state: header + a two-column grid of action cards.
    fn render_dock_empty_state(&self, cx: &mut Context<Self>) -> AnyElement {
        let offers = self.dock_surface_offers();
        let mut grid = v_flex().gap_2().w_full().max_w(px(576.));
        for pair in offers.chunks(2) {
            // Plain flex row, not `h_flex`: its `items_center` would defeat
            // the default cross-axis stretch that keeps both cards in a row
            // the same height (Electron's CSS grid rows stretch likewise).
            let mut row = div().flex().gap_2().w_full().min_w_0();
            for offer in pair {
                row = row.child(self.render_dock_card(offer, cx));
            }
            if pair.len() == 1 {
                row = row.child(div().flex_1().min_w_0());
            }
            grid = grid.child(row);
        }
        v_flex()
            .size_full()
            .min_w_0()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                v_flex()
                    .items_center()
                    .mb_5()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_medium()
                            .text_color(cx.theme().foreground)
                            .child("Open a surface"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Choose what to show in the right panel."),
                    ),
            )
            .child(grid)
            .into_any_element()
    }

    fn render_dock_card(&self, offer: &SurfaceOffer, cx: &mut Context<Self>) -> AnyElement {
        let kind = offer.kind;
        let mut card = v_flex()
            .id(SharedString::from(format!("dock-card-{}", offer.label)))
            .flex_1()
            // Without this, a row's min-content width is the sum of its
            // unwrapped description lines, overflowing the panel.
            .min_w_0()
            .min_h(px(112.))
            .items_start()
            .rounded(px(8.))
            .border_1()
            .border_color(cx.theme().border.opacity(0.8))
            // Electron's `bg-card`; the Vitre theme transcribes that token to
            // `muted.background`.
            .bg(cx.theme().muted)
            .p_4()
            .child((offer.icon)().size_5().mb_3())
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .text_color(cx.theme().foreground)
                    .child(offer.label),
            );
        // Electron always renders the short description; a disabled card
        // keeps it and shows the reason only in a tooltip over the dimmed
        // card, so every card's content (and height) stays uniform.
        card = card.child(
            div()
                .mt_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(offer.description),
        );
        match offer.disabled.clone() {
            Some(reason) => {
                card = card
                    .opacity(0.4)
                    .cursor_not_allowed()
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(reason.clone()).build(window, cx)
                    });
            }
            None => {
                card = card
                    .hover(|style| {
                        style
                            .border_color(cx.theme().border)
                            .bg(cx.theme().accent.opacity(0.6))
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.dock_add_surface(kind, window, cx);
                    }));
            }
        }
        card.into_any_element()
    }
}

/// The tab's right-click menu, Electron's item set: Copy path (file surfaces),
/// Close, Close others, Close to the right, Close all.
fn dock_tab_context_menu(
    menu: gpui_component::menu::PopupMenu,
    chat: &WeakEntity<ChatApp>,
    surface_id: &str,
    copy_path: Option<&str>,
    is_last: bool,
    count: usize,
) -> gpui_component::menu::PopupMenu {
    let mut menu = menu;
    if let Some(path) = copy_path {
        let path = path.to_string();
        menu = menu.item(
            PopupMenuItem::new("Copy path").on_click(move |_, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                window.push_notification(Notification::info(format!("Path copied: {path}")), cx);
            }),
        );
    }
    let close_id = surface_id.to_string();
    let close_chat = chat.clone();
    let others_id = surface_id.to_string();
    let others_chat = chat.clone();
    let right_id = surface_id.to_string();
    let right_chat = chat.clone();
    let all_chat = chat.clone();
    menu.item(PopupMenuItem::new("Close").on_click(move |_, window, cx| {
        let close_id = close_id.clone();
        let _ = close_chat.update(cx, |this, cx| {
            this.dock_close_surface(&close_id, window, cx);
        });
    }))
    .item(
        PopupMenuItem::new("Close others")
            .disabled(count <= 1)
            .on_click(move |_, window, cx| {
                let others_id = others_id.clone();
                let _ = others_chat.update(cx, |this, cx| {
                    this.dock_close_others(&others_id, window, cx);
                });
            }),
    )
    .item(
        PopupMenuItem::new("Close to the right")
            .disabled(is_last)
            .on_click(move |_, window, cx| {
                let right_id = right_id.clone();
                let _ = right_chat.update(cx, |this, cx| {
                    this.dock_close_to_right(&right_id, window, cx);
                });
            }),
    )
    .item(
        PopupMenuItem::new("Close all").on_click(move |_, window, cx| {
            let _ = all_chat.update(cx, |this, cx| {
                this.dock_close_all(window, cx);
            });
        }),
    )
}

fn surface_title(surface: &RightPanelSurface) -> SharedString {
    match surface {
        RightPanelSurface::Diff { .. } => "Diff".into(),
        RightPanelSurface::Files { .. } => "Files".into(),
        RightPanelSurface::Search { .. } => "Search".into(),
        RightPanelSurface::Plan { .. } => "Plan".into(),
        RightPanelSurface::Graph { .. } => "Graph".into(),
        RightPanelSurface::Preview { .. } => "Browser".into(),
        RightPanelSurface::Terminal {
            active_terminal_id, ..
        } => vitre_state::terminal::terminal_fallback_label(active_terminal_id).into(),
        RightPanelSurface::File { relative_path, .. } => relative_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(relative_path)
            .to_string()
            .into(),
    }
}

fn surface_icon(surface: &RightPanelSurface) -> Icon {
    match surface.kind() {
        SurfaceKind::Diff => Icon::new(VitreIcon::FileDiff),
        SurfaceKind::Files => Icon::new(VitreIcon::Files),
        SurfaceKind::Search => Icon::new(VitreIcon::TextSearch),
        SurfaceKind::Plan => Icon::new(IconName::BookOpen),
        SurfaceKind::Graph => Icon::new(IconName::Network),
        SurfaceKind::Preview => Icon::new(IconName::Globe),
        SurfaceKind::Terminal => Icon::new(IconName::SquareTerminal),
        SurfaceKind::File => Icon::new(IconName::File),
    }
}

fn dock_placeholder(message: &'static str, cx: &mut Context<ChatApp>) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .p_6()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(message)
        .into_any_element()
}
