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

use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, ClickEvent, ClipboardItem, Context, MouseButton, MouseDownEvent, SharedString,
    WeakEntity, Window, div, prelude::*, px,
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
use vitre_state::right_panel::{PERSISTED_VERSION, RightPanelMap, RightPanelSurface, SurfaceKind};

use crate::assets::VitreIcon;
use crate::lsp::positions::WirePosition;

use super::ChatApp;

const FILE_NAME: &str = "right-panel-state.json";

/// Electron's `t3code:preview-panel-width` default / min. The max is 70% of
/// the viewport, applied at render time.
pub(super) const DEFAULT_PANEL_WIDTH: f32 = 540.;
pub(super) const MIN_PANEL_WIDTH: f32 = 360.;

/// The dock store plus its persistence. Electron persists on every store
/// change (zustand `persist`, no debounce); writes here are the same
/// best-effort as [`crate::sidebar_prefs::SidebarPrefs`].
pub(super) struct RightPanelPrefs {
    pub map: RightPanelMap,
    /// Inline panel width in px, committed on drag end only.
    pub width: f32,
    path: PathBuf,
}

/// On-disk shape: Electron's persisted blob (`version` + `byThreadKey`) plus
/// the panel width Electron keeps in a separate localStorage key.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StoredRightPanel {
    version: u64,
    panel_width: Option<f32>,
    by_thread_key: Value,
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
        self.right_panel
            .map
            .open_file(&key, &path, line, None, None);
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
        if self.right_panel.map.close_surface(&key, surface_id) {
            self.after_dock_change(false, window, cx);
        }
    }

    fn dock_close_others(&mut self, surface_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.dock_thread_key() else {
            return;
        };
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
    fn after_dock_change(&mut self, focus: bool, window: &mut Window, cx: &mut Context<Self>) {
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
        // reveal request, an explicit activation (`focus`), or the dock's
        // thread changing under it. In particular it must not re-open the
        // surface's file on every frame, or browsing the tree while a file
        // tab is active would snap straight back.
        let key_changed = self.last_dock_sync_key.as_deref() != Some(key.as_str());
        self.last_dock_sync_key = Some(key.clone());
        match surface {
            RightPanelSurface::Files { .. } => self.ensure_files_panel(window, cx),
            RightPanelSurface::Diff { .. } => self.ensure_diff_panel(window, cx),
            RightPanelSurface::File {
                id,
                relative_path,
                reveal_line,
                reveal_request_id,
                ..
            } => {
                self.ensure_files_panel(window, cx);
                let applied_key = format!("{key}|{id}");
                let fresh = self.applied_reveals.get(&applied_key) != Some(&reveal_request_id);
                if !fresh && !focus && !key_changed {
                    return;
                }
                let Some(files) = self.files.clone() else {
                    return;
                };
                // The stored position is applied once per request; re-showing
                // an already-revealed tab re-opens the file where it was.
                let position = if fresh {
                    self.applied_reveals.insert(applied_key, reveal_request_id);
                    reveal_line.map(|line| WirePosition {
                        line: line.saturating_sub(1),
                        character: 0,
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
                disabled: Some("Browser previews are not yet available in Vitre.".into()),
            },
            SurfaceOffer {
                kind: SurfaceKind::Terminal,
                label: "Terminal",
                description: "Start a shell in this workspace.",
                icon: || Icon::new(IconName::SquareTerminal),
                disabled: Some("The terminal is not yet available in Vitre.".into()),
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
                disabled: Some("The search panel is not yet available in Vitre.".into()),
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

        let mut tabs = h_flex().gap_1().min_w_0().flex_1().overflow_hidden();
        for (index, surface) in state.surfaces.iter().enumerate() {
            let is_active = active_id.as_deref() == Some(surface.id());
            tabs = tabs.child(self.render_dock_tab(
                index,
                surface,
                is_active,
                index + 1 == count,
                count,
                cx,
            ));
        }

        let mut bar = h_flex()
            .h(px(40.))
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

        let content = self.render_dock_content(&state.surfaces, active_id.as_deref(), cx);

        v_flex()
            .size_full()
            .min_w_0()
            .bg(cx.theme().background)
            .border_l_1()
            .border_color(cx.theme().border)
            .child(bar)
            .child(div().flex_1().min_h_0().child(content))
            .into_any_element()
    }

    fn render_dock_tab(
        &self,
        index: usize,
        surface: &RightPanelSurface,
        is_active: bool,
        is_last: bool,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let group: SharedString = format!("dock-tab-{index}").into();
        let id = surface.id().to_string();
        let title = surface_title(surface);
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
            .id(SharedString::from(format!("dock-tab-item-{index}")))
            .group(group.clone())
            .h(px(28.))
            .min_w(px(100.))
            .max_w(px(176.))
            .flex_shrink_0()
            .items_center()
            .gap_1p5()
            .rounded(px(6.))
            .px_2()
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
            .child(surface_icon(surface).size_3p5().flex_shrink_0())
            .child(div().flex_1().min_w_0().truncate().child(title))
            .child(
                // Hidden until the tab is hovered, like Electron's
                // opacity-0/group-hover close button.
                div()
                    .invisible()
                    .group_hover(group, |style| style.visible())
                    .child(
                        Button::new(SharedString::from(format!("dock-tab-close-{index}")))
                            .icon(Icon::new(IconName::Close).size_3())
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.dock_close_surface(&button_close_id, window, cx);
                            })),
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
            SurfaceKind::Terminal => {
                dock_placeholder("The terminal is not yet available in Vitre.", cx)
            }
            SurfaceKind::Search => {
                dock_placeholder("The search panel is not yet available in Vitre.", cx)
            }
            SurfaceKind::Preview => {
                dock_placeholder("Browser previews are not yet available in Vitre.", cx)
            }
            SurfaceKind::Plan | SurfaceKind::Graph => {
                dock_placeholder("This surface is not yet available in Vitre.", cx)
            }
        }
    }

    /// Electron's empty state: header + a two-column grid of action cards.
    fn render_dock_empty_state(&self, cx: &mut Context<Self>) -> AnyElement {
        let offers = self.dock_surface_offers();
        let mut grid = v_flex().gap_2().w_full().max_w(px(576.));
        for pair in offers.chunks(2) {
            let mut row = h_flex().gap_2().w_full().min_w_0();
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
        match offer.disabled.clone() {
            // Electron shows the reason in a tooltip over the dimmed card;
            // Vitre prints it in the description slot instead.
            Some(reason) => {
                card = card.opacity(0.4).child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(reason),
                );
            }
            None => {
                card = card
                    .child(
                        div()
                            .mt_1()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(offer.description),
                    )
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
        } => terminal_label(active_terminal_id).into(),
        RightPanelSurface::File { relative_path, .. } => relative_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(relative_path)
            .to_string()
            .into(),
    }
}

/// Electron's `getTerminalLabel`: `term-3` / `terminal-3` → `Terminal 3`,
/// anything else verbatim.
fn terminal_label(terminal_id: &str) -> String {
    let lower = terminal_id.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("terminal-")
        .or_else(|| lower.strip_prefix("term-"));
    match rest {
        Some(digits) if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => {
            format!("Terminal {digits}")
        }
        _ => terminal_id.to_string(),
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

#[cfg(test)]
mod tests {
    use super::terminal_label;

    #[test]
    fn terminal_labels_follow_electron() {
        assert_eq!(terminal_label("term-3"), "Terminal 3");
        assert_eq!(terminal_label("Terminal-12"), "Terminal 12");
        assert_eq!(terminal_label("term-"), "term-");
        assert_eq!(terminal_label("zsh"), "zsh");
    }
}
