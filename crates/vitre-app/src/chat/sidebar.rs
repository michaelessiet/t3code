//! The grouped project sidebar: Electron's `Sidebar.tsx` (the V1 sidebar,
//! which is what ships — `sidebarV2Enabled` defaults to `false`).
//!
//! Projects that check out the same repository collapse into one row
//! ([`vitre_state::project_grouping`]); each row expands into its threads,
//! capped at the preview count with a "Show more" tail. The derived model —
//! grouping, ordering, status pills, the preview window — lives in
//! `vitre-state` and is unit-tested there; this module is the render and the
//! interactions.
//!
//! The row model is cached in [`super::ChatApp::sidebar_rows`] and rebuilt on
//! the transitions that can change it (shell snapshot, thread selection,
//! preference edits) rather than per frame, for the same reason the timeline
//! is: it walks every project and thread.

use gpui::{Context, div};
use gpui::{CursorStyle, Hsla, SharedString, Window, prelude::*, px, rgb};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    v_flex,
};
use std::collections::HashMap;
use vitre_contracts::{
    ClientOrchestrationCommand, CommandId, OrchestrationSessionStatus, OrchestrationThreadShell,
    ProjectId, ThreadId,
};
use vitre_state::project_grouping::{
    ProjectGroupingMode, build_project_groups, derive_physical_project_key,
    expansion_preference_keys,
};
use vitre_state::sidebar::{
    MAX_THREAD_PREVIEW_COUNT, MIN_THREAD_PREVIEW_COUNT, ProjectSortOrder, ThreadSortOrder,
    ThreadStatus, ThreadWindow, is_archived, resolve_project_status, resolve_thread_status,
    sort_project_groups, sort_threads, thread_window,
};

use super::project_actions::{ProjectMember, project_context_menu};
use super::{ChatApp, SyncPhase, fresh_id, relative_time};

/// Hover group that reveals a project header's "new thread" button, and a
/// thread row's archive button. Electron does the same with
/// `group/project-header` and `group/menu-sub-item`.
const PROJECT_HEADER_GROUP: &str = "project-header";
const THREAD_ROW_GROUP: &str = "thread-row";

/// One project row's render-ready state.
pub(super) struct SidebarProjectRow {
    /// Logical project key: the row's identity.
    key: String,
    display_name: SharedString,
    /// `groupedProjectCount` — the "{n} projects" chip appears above 1.
    grouped_count: usize,
    /// The row's physical projects, in row order: what the context menu's
    /// per-member actions target, and what a manual drag moves as a block.
    members: Vec<ProjectMember>,
    /// Representative project: what "New thread" targets.
    project_id: ProjectId,
    /// Keys the expand/collapse preference is recorded under.
    preference_keys: Vec<String>,
    expanded: bool,
    list_expanded: bool,
    /// Status dot a collapsed row shows.
    status: Option<ThreadStatus>,
    window: ThreadWindow,
    /// The row's unarchived threads, already ordered.
    threads: Vec<SidebarThreadRow>,
}

/// One thread row's render-ready state.
pub(super) struct SidebarThreadRow {
    id: ThreadId,
    title: SharedString,
    status: Option<ThreadStatus>,
    /// Relative timestamp label, from the same candidate chain Electron uses.
    time: Option<SharedString>,
    /// A running turn suppresses the archive affordance.
    running: bool,
}

/// The two colours a status pill paints with: the label/glyph colour and the
/// dot's fill. Transcribed from the Tailwind utilities Electron's
/// `resolveThreadStatusPill` hands each state (amber / indigo / sky / violet /
/// emerald), resolved out of the v4 oklch palette to sRGB. They are raw
/// palette values there too, not design tokens, so there is nothing in the
/// Vitre theme to reach for instead.
struct StatusColors {
    text: Hsla,
    dot: Hsla,
}

fn status_colors(status: ThreadStatus, dark: bool) -> StatusColors {
    let (light_text, light_dot, dark_shade, dark_alpha) = match status {
        // amber-600 / amber-500, dark amber-300 at 90%
        ThreadStatus::PendingApproval => (0xe17100, 0xfe9a00, 0xffd230, 0.9),
        // indigo-600 / indigo-500, dark indigo-300 at 90%
        ThreadStatus::AwaitingInput => (0x4f39f6, 0x615fff, 0xa3b3ff, 0.9),
        // sky-600 / sky-500, dark sky-300 at 80%
        ThreadStatus::Working | ThreadStatus::Connecting => (0x0084d1, 0x00a6f4, 0x74d4ff, 0.8),
        // violet-600 / violet-500, dark violet-300 at 90%
        ThreadStatus::PlanReady => (0x7f22fe, 0x8e51ff, 0xc4b4ff, 0.9),
        // emerald-600 / emerald-500, dark emerald-300 at 90%
        ThreadStatus::Completed => (0x009966, 0x00bc7d, 0x5ee9b5, 0.9),
    };
    if dark {
        let shade = Hsla::from(rgb(dark_shade)).opacity(dark_alpha);
        StatusColors {
            text: shade,
            dot: shade,
        }
    } else {
        StatusColors {
            text: Hsla::from(rgb(light_text)),
            dot: Hsla::from(rgb(light_dot)),
        }
    }
}

/// A project row in flight during a manual reorder. It is both the drag
/// payload and the ghost that follows the cursor.
#[derive(Clone)]
struct DraggedProjectRow {
    /// Every physical key the row stands for — a grouped repository's
    /// worktrees move together.
    keys: Vec<String>,
    label: SharedString,
}

impl Render for DraggedProjectRow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .h_8()
            .px_2()
            .gap_2()
            .items_center()
            .rounded(cx.theme().radius)
            .bg(cx.theme().sidebar)
            .border_1()
            .border_color(cx.theme().primary.opacity(0.4))
            .shadow_md()
            .text_sm()
            .text_color(cx.theme().sidebar_foreground)
            .child(
                Icon::new(IconName::Folder)
                    .size_3p5()
                    .text_color(cx.theme().muted_foreground.opacity(0.5)),
            )
            .child(self.label.clone())
    }
}

impl ChatApp {
    /// Recompute the cached sidebar rows.
    ///
    /// Called on shell updates, thread selection and preference edits — never
    /// from `render`, which is the point: this groups every project and walks
    /// every thread.
    pub(super) fn rebuild_sidebar(&mut self) {
        let Some(snapshot) = self.shell.snapshot.clone() else {
            self.sidebar_rows = Vec::new();
            self.sidebar_project_order = Vec::new();
            return;
        };
        let projects = &snapshot.projects;

        // Electron orders the raw project list by the persisted manual order
        // first (`orderItemsByPreferredIds`) and only then groups, so a manual
        // drag survives a grouping-mode change. The activity sort below is a
        // no-op in `Manual` mode, leaving that order in place.
        let physical_keys: Vec<String> = projects.iter().map(derive_physical_project_key).collect();
        let ordered: Vec<vitre_contracts::OrchestrationProjectShell> = self
            .sidebar
            .order_projects(&physical_keys)
            .into_iter()
            .map(|index| projects[index].clone())
            .collect();
        // The seed a manual drag reorders against, exactly as Electron passes
        // `orderedProjects.map(getProjectOrderKey)`.
        self.sidebar_project_order = ordered.iter().map(derive_physical_project_key).collect();

        let mut groups = build_project_groups(&ordered, &self.sidebar.grouping);

        // Every project id in a group maps to that group's row, so a thread
        // finds its row through its own project.
        let mut row_of_project: HashMap<&ProjectId, String> = HashMap::new();
        for group in &groups {
            for member in &group.members {
                row_of_project.insert(&ordered[*member].id, group.key.clone());
            }
        }

        let mut threads_by_row: HashMap<String, Vec<&OrchestrationThreadShell>> = HashMap::new();
        for thread in &snapshot.threads {
            if is_archived(thread) {
                continue;
            }
            let Some(row) = row_of_project.get(&thread.project_id) else {
                continue;
            };
            threads_by_row.entry(row.clone()).or_default().push(thread);
        }
        for threads in threads_by_row.values_mut() {
            sort_threads(threads, self.sidebar.thread_sort_order);
        }

        sort_project_groups(
            &mut groups,
            &ordered,
            self.sidebar.project_sort_order,
            |group| {
                threads_by_row
                    .get(group.key.as_str())
                    .cloned()
                    .unwrap_or_default()
            },
        );

        let open = self.thread.as_ref().map(|open| open.id.clone());
        self.sidebar_rows = groups
            .iter()
            .map(|group| {
                let threads = threads_by_row
                    .get(group.key.as_str())
                    .cloned()
                    .unwrap_or_default();
                let statuses: Vec<Option<ThreadStatus>> = threads
                    .iter()
                    .map(|thread| {
                        resolve_thread_status(
                            thread,
                            self.sidebar.thread_last_visited(&thread.id.0),
                        )
                    })
                    .collect();
                let preference_keys = expansion_preference_keys(group, &ordered);
                let expanded = self.sidebar.project_expanded(&preference_keys);
                let list_expanded = self.sidebar.thread_list_expanded(&group.key);
                let active = open
                    .as_ref()
                    .and_then(|id| threads.iter().position(|thread| thread.id == *id));
                let window = thread_window(
                    &threads,
                    &statuses,
                    active,
                    expanded,
                    list_expanded,
                    self.sidebar.thread_preview_count,
                );
                SidebarProjectRow {
                    key: group.key.clone(),
                    display_name: group.display_name.clone().into(),
                    grouped_count: group.grouped_project_count(),
                    members: group
                        .members
                        .iter()
                        .map(|member| {
                            let project = &ordered[*member];
                            ProjectMember {
                                physical_key: derive_physical_project_key(project),
                                project_id: project.id.clone(),
                                title: project.title.0.clone().into(),
                                workspace_root: project.workspace_root.0.clone().into(),
                            }
                        })
                        .collect(),
                    project_id: ordered[group.representative].id.clone(),
                    preference_keys,
                    expanded,
                    list_expanded,
                    status: resolve_project_status(statuses.iter().copied()),
                    window,
                    threads: threads
                        .iter()
                        .zip(statuses)
                        .map(|(thread, status)| SidebarThreadRow {
                            id: thread.id.clone(),
                            title: thread.title.0.clone().into(),
                            status,
                            // Electron labels a row with the same candidate
                            // chain it sorts by, falling back to creation.
                            time: thread
                                .latest_user_message_at
                                .as_ref()
                                .map(|at| at.0.as_str())
                                .or(Some(thread.updated_at.0.as_str()))
                                .and_then(relative_time)
                                .map(SharedString::from),
                            running: thread.session.as_ref().is_some_and(|session| {
                                session.status == OrchestrationSessionStatus::Running
                                    && session.active_turn_id.is_some()
                            }),
                        })
                        .collect(),
                }
            })
            .collect();
    }

    /// `ChatView`'s visit effect: while a thread is open, its last-visited
    /// stamp tracks the thread's `updatedAt`, which is what keeps the
    /// "Completed" pill off the row you are looking at.
    pub(super) fn sync_thread_visit(&mut self) -> bool {
        let Some(open) = self.thread.as_ref() else {
            return false;
        };
        let Some(thread) = self.shell_thread(&open.id) else {
            return false;
        };
        let (id, updated_at) = (thread.id.0.clone(), thread.updated_at.0.clone());
        self.sidebar.mark_thread_visited(&id, &updated_at)
    }

    /// Land a manual reorder drag: move `dragged`'s keys to where `target`'s
    /// keys sit in the current on-screen order.
    fn reorder_projects(&mut self, dragged: &[String], target: &[String], cx: &mut Context<Self>) {
        let current = self.sidebar_project_order.clone();
        if self.sidebar.reorder_projects(&current, dragged, target) {
            self.rebuild_sidebar();
            cx.notify();
        }
    }

    fn set_project_expanded(&mut self, row_key: &str, expanded: bool, cx: &mut Context<Self>) {
        let Some(row) = self.sidebar_rows.iter().find(|row| row.key == row_key) else {
            return;
        };
        let keys = row.preference_keys.clone();
        self.sidebar.set_project_expanded(&keys, expanded);
        self.rebuild_sidebar();
        cx.notify();
    }

    fn set_thread_list_expanded(&mut self, row_key: &str, expanded: bool, cx: &mut Context<Self>) {
        self.sidebar.set_thread_list_expanded(row_key, expanded);
        self.rebuild_sidebar();
        cx.notify();
    }

    fn set_project_grouping_mode(&mut self, mode: ProjectGroupingMode, cx: &mut Context<Self>) {
        self.sidebar.set_project_grouping_mode(mode);
        self.rebuild_sidebar();
        cx.notify();
    }

    fn set_project_sort_order(&mut self, order: ProjectSortOrder, cx: &mut Context<Self>) {
        self.sidebar.set_project_sort_order(order);
        self.rebuild_sidebar();
        cx.notify();
    }

    fn set_thread_sort_order(&mut self, order: ThreadSortOrder, cx: &mut Context<Self>) {
        self.sidebar.set_thread_sort_order(order);
        self.rebuild_sidebar();
        cx.notify();
    }

    fn set_thread_preview_count(&mut self, count: usize, cx: &mut Context<Self>) {
        self.sidebar.set_thread_preview_count(count);
        self.rebuild_sidebar();
        cx.notify();
    }

    /// Archive a thread (`ThreadArchive`). The shell drops it from the row on
    /// the resulting event, so nothing is removed optimistically.
    fn archive_thread(&mut self, id: ThreadId, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let command = ClientOrchestrationCommand::ThreadArchive {
            command_id: CommandId(fresh_id("vitre-cmd")),
            thread_id: id,
            r#type: Default::default(),
        };
        self.last_error = None;
        cx.spawn(async move |this, cx| {
            if let Err(error) = client.dispatch(&command).await {
                let _ = this.update(cx, |app, cx| {
                    app.last_error = Some(format!("archive failed: {error:?}").into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// The status dot Electron paints beside a collapsed project and inside a
    /// thread row.
    fn status_dot(&self, status: ThreadStatus, cx: &Context<Self>) -> impl IntoElement {
        div()
            .size(px(9.))
            .rounded_full()
            .bg(status_colors(status, cx.theme().is_dark()).dot)
    }

    /// `ThreadStatusLabel`: dot plus label, in the status colour.
    fn status_label(&self, status: ThreadStatus, cx: &Context<Self>) -> impl IntoElement {
        let colors = status_colors(status, cx.theme().is_dark());
        h_flex()
            .flex_shrink_0()
            .gap_1()
            .items_center()
            .text_size(px(10.))
            .text_color(colors.text)
            .child(div().size(px(6.)).rounded_full().bg(colors.dot))
            .child(status.label())
    }

    fn render_thread_row(
        &self,
        thread: &SidebarThreadRow,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let selected = self
            .thread
            .as_ref()
            .is_some_and(|open| open.id == thread.id);
        let id = thread.id.clone();
        let archive_id = thread.id.clone();
        let archive_label = format!("Archive {}", thread.title);

        let meta: gpui::AnyElement = if thread.running {
            // A running turn keeps its timestamp visible; there is no archive
            // affordance to swap it out for.
            div()
                .text_size(px(10.))
                .text_color(cx.theme().muted_foreground.opacity(0.4))
                .children(thread.time.clone())
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .justify_end()
                .min_w(px(48.))
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(if selected {
                            cx.theme().foreground.opacity(0.72)
                        } else {
                            cx.theme().muted_foreground.opacity(0.4)
                        })
                        .group_hover(THREAD_ROW_GROUP, |style| style.invisible())
                        .children(thread.time.clone()),
                )
                .child(
                    div()
                        .absolute()
                        .right_0p5()
                        .invisible()
                        .group_hover(THREAD_ROW_GROUP, |style| style.visible())
                        .child(
                            Button::new(SharedString::from(format!("archive-{}", thread.id.0)))
                                .icon(Icon::new(IconName::Archive).size_3p5())
                                .ghost()
                                .xsmall()
                                .tooltip(archive_label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.archive_thread(archive_id.clone(), cx);
                                })),
                        ),
                )
                .into_any_element()
        };

        let mut row = h_flex()
            .id(SharedString::from(format!("thread-{}", thread.id.0)))
            .group(THREAD_ROW_GROUP)
            .relative()
            .h_8()
            .w_full()
            .px_2()
            .gap_1p5()
            .items_center()
            .rounded(cx.theme().radius)
            .cursor_pointer()
            .text_sm()
            .children(
                thread
                    .status
                    .map(|status| self.status_label(status, cx).into_any_element()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(thread.title.clone()),
            )
            .child(div().ml_auto().flex().flex_shrink_0().child(meta))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_thread(id.clone(), cx);
            }));
        row = if selected {
            row.bg(cx.theme().list_active)
                .font_medium()
                .text_color(cx.theme().sidebar_foreground)
        } else {
            row.text_color(cx.theme().sidebar_foreground.opacity(0.8))
                .hover(|style| style.bg(cx.theme().list_hover))
        };
        row.into_any_element()
    }

    fn render_project_row(
        &self,
        row: &SidebarProjectRow,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let row_key = row.key.clone();
        let project_id = row.project_id.clone();
        let expanded = row.expanded;

        // Collapsed rows carry their most urgent thread status as a dot that
        // crossfades to the chevron on hover; expanded rows just show the
        // chevron, since the statuses are all visible below.
        let leading: gpui::AnyElement = match (expanded, row.status) {
            (false, Some(status)) => div()
                .relative()
                .size_3p5()
                .flex_shrink_0()
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .group_hover(PROJECT_HEADER_GROUP, |style| style.invisible())
                        .child(self.status_dot(status, cx)),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .invisible()
                        .group_hover(PROJECT_HEADER_GROUP, |style| style.visible())
                        .child(
                            Icon::new(IconName::ChevronRight)
                                .size_3p5()
                                .text_color(cx.theme().muted_foreground.opacity(0.7)),
                        ),
                )
                .into_any_element(),
            _ => Icon::new(if expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .size_3p5()
            .flex_shrink_0()
            .text_color(cx.theme().muted_foreground.opacity(0.7))
            .into_any_element(),
        };

        let members = row.members.clone();
        let chat = cx.entity().downgrade();

        let header = h_flex()
            .id(SharedString::from(format!("project-{}", row.key)))
            .group(PROJECT_HEADER_GROUP)
            .h_8()
            .w_full()
            .gap_2()
            .px_2()
            .pr_8()
            .items_center()
            .rounded(cx.theme().radius)
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().list_hover))
            // Manual order is set by dragging, so the rows only become
            // draggable in that mode — same gate Electron puts on its
            // sortable context. A grouped row carries every member key, so
            // the whole repository moves as one block.
            .when(
                self.sidebar.project_sort_order == ProjectSortOrder::Manual,
                |this| {
                    let member_keys: Vec<String> = row
                        .members
                        .iter()
                        .map(|member| member.physical_key.clone())
                        .collect();
                    this.cursor(CursorStyle::OpenHand)
                        .on_drag(
                            DraggedProjectRow {
                                keys: member_keys.clone(),
                                label: row.display_name.clone(),
                            },
                            |dragged, _, _, cx| {
                                let dragged = dragged.clone();
                                cx.new(|_| dragged)
                            },
                        )
                        .drag_over::<DraggedProjectRow>(|style, _, _, cx| {
                            style.bg(cx.theme().drop_target)
                        })
                        .on_drop(
                            cx.listener(move |this, dragged: &DraggedProjectRow, _, cx| {
                                this.reorder_projects(&dragged.keys, &member_keys, cx);
                            }),
                        )
                },
            )
            .child(leading)
            // Electron renders the project's favicon here and falls back to a
            // folder glyph; Vitre has no asset pipeline for the favicon yet, so
            // it always renders the fallback.
            .child(
                Icon::new(IconName::Folder)
                    .size_3p5()
                    .flex_shrink_0()
                    .text_color(cx.theme().muted_foreground.opacity(0.5)),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_medium()
                            .text_color(cx.theme().sidebar_foreground.opacity(0.9))
                            .child(row.display_name.clone()),
                    )
                    .when(row.grouped_count > 1, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(10.))
                                .text_color(cx.theme().muted_foreground.opacity(0.6))
                                .child(format!("{} projects", row.grouped_count)),
                        )
                    }),
            )
            .child(
                div()
                    .absolute()
                    .right_0p5()
                    .invisible()
                    .group_hover(PROJECT_HEADER_GROUP, |style| style.visible())
                    .child(
                        Button::new(SharedString::from(format!("new-thread-{}", row.key)))
                            .icon(Icon::new(IconName::SquarePen).size_3p5())
                            .ghost()
                            .xsmall()
                            .tooltip(format!("Create new thread in {}", row.display_name))
                            .on_click(cx.listener({
                                let project_id = project_id.clone();
                                move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.new_thread(Some(project_id.clone()), window, cx);
                                }
                            })),
                    ),
            )
            .on_click(cx.listener({
                let row_key = row_key.clone();
                move |this, _, _, cx| {
                    this.set_project_expanded(&row_key, !expanded, cx);
                }
            }))
            .context_menu(move |menu, window, cx| {
                project_context_menu(menu, &chat, &members, window, cx)
            });

        v_flex()
            .w_full()
            .child(header)
            .children(self.render_thread_panel(row, cx))
            .into_any_element()
    }

    /// `SidebarProjectThreadList`: the empty state, the windowed thread rows,
    /// and the "Show more"/"Show less" tail.
    fn render_thread_panel(
        &self,
        row: &SidebarProjectRow,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !row.window.show_panel && !row.window.show_empty_state {
            return None;
        }
        let mut list = v_flex().w_full().mx_1().px_1p5().gap_0p5();
        if row.window.show_empty_state {
            list = list.child(
                div()
                    .h_8()
                    .w_full()
                    .flex()
                    .items_center()
                    .px_2()
                    .text_xs()
                    .text_color(cx.theme().sidebar_foreground.opacity(0.5))
                    .child("No threads yet"),
            );
        }
        if row.window.show_panel {
            for index in &row.window.rendered {
                let Some(thread) = row.threads.get(*index) else {
                    continue;
                };
                list = list.child(self.render_thread_row(thread, cx));
            }
        }

        // The tail only exists while the row itself is open — a collapsed row
        // pinning the active thread shows that one row and nothing else.
        if row.expanded && row.window.has_overflow {
            let row_key = row.key.clone();
            let expand = !row.list_expanded;
            let hidden_status = row.window.hidden_status;
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("show-more-{}", row.key)))
                    .h_8()
                    .w_full()
                    .px_2()
                    .gap_2()
                    .items_center()
                    .rounded(cx.theme().radius)
                    .cursor_pointer()
                    .text_xs()
                    .text_color(cx.theme().sidebar_foreground.opacity(0.5))
                    .hover(|style| {
                        style
                            .bg(cx.theme().list_hover)
                            .text_color(cx.theme().sidebar_foreground)
                    })
                    .when(expand, |this| {
                        this.children(hidden_status.map(|status| {
                            div()
                                .flex()
                                .size_3p5()
                                .items_center()
                                .justify_center()
                                .child(self.status_dot(status, cx))
                        }))
                    })
                    .child(if expand { "Show more" } else { "Show less" })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_thread_list_expanded(&row_key, expand, cx);
                    })),
            );
        }
        Some(list.into_any_element())
    }

    /// `ProjectSortMenu`: project order, thread order, visible-thread count.
    fn render_sort_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let project_order = self.sidebar.project_sort_order;
        let thread_order = self.sidebar.thread_sort_order;
        let preview_count = self.sidebar.thread_preview_count;
        let grouping_mode = self.sidebar.grouping.mode;
        let chat = cx.entity().downgrade();

        Button::new("sidebar-options")
            .icon(Icon::new(IconName::ArrowUpDown).size_3p5())
            .ghost()
            .xsmall()
            .tooltip("Sidebar options")
            .dropdown_menu(move |mut menu, window, cx| {
                menu = menu.item(PopupMenuItem::label("Sort projects"));
                for order in ProjectSortOrder::ALL {
                    let chat = chat.clone();
                    menu = menu.item(
                        PopupMenuItem::new(order.label())
                            .checked(order == project_order)
                            .on_click(move |_, _, cx| {
                                let _ = chat.update(cx, |this, cx| {
                                    this.set_project_sort_order(order, cx);
                                });
                            }),
                    );
                }
                menu = menu.separator().item(PopupMenuItem::label("Sort threads"));
                for order in ThreadSortOrder::ALL {
                    let chat = chat.clone();
                    menu = menu.item(
                        PopupMenuItem::new(order.label())
                            .checked(order == thread_order)
                            .on_click(move |_, _, cx| {
                                let _ = chat.update(cx, |this, cx| {
                                    this.set_thread_sort_order(order, cx);
                                });
                            }),
                    );
                }
                menu = menu
                    .separator()
                    .item(PopupMenuItem::label("Group projects"));
                for mode in ProjectGroupingMode::ALL {
                    let chat = chat.clone();
                    menu = menu.item(
                        PopupMenuItem::new(mode.label())
                            .checked(mode == grouping_mode)
                            .on_click(move |_, _, cx| {
                                let _ = chat.update(cx, |this, cx| {
                                    this.set_project_grouping_mode(mode, cx);
                                });
                            }),
                    );
                }
                // Electron's NumberField spans exactly this range; a submenu of
                // the whole range keeps every value reachable without an
                // editable field inside a popup menu.
                let chat = chat.clone();
                menu.separator().submenu(
                    format!("Visible threads: {preview_count}"),
                    window,
                    cx,
                    move |mut menu, _, _| {
                        for count in MIN_THREAD_PREVIEW_COUNT..=MAX_THREAD_PREVIEW_COUNT {
                            let chat = chat.clone();
                            menu = menu.item(
                                PopupMenuItem::new(count.to_string())
                                    .checked(count == preview_count)
                                    .on_click(move |_, _, cx| {
                                        let _ = chat.update(cx, |this, cx| {
                                            this.set_thread_preview_count(count, cx);
                                        });
                                    }),
                            );
                        }
                        menu
                    },
                )
            })
    }

    /// The sidebar: search affordance + options, then the grouped project
    /// rows, then the sidecar status footer.
    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let phase = match self.shell.phase {
            SyncPhase::Disconnected => "offline",
            SyncPhase::Synchronizing => "syncing…",
            SyncPhase::Live => "live",
        };

        let mut list = v_flex().gap_0p5().px_1();
        for row in &self.sidebar_rows {
            list = list.child(self.render_project_row(row, cx));
        }
        if self.sidebar_rows.is_empty() {
            list = list.child(
                div()
                    .px_2()
                    .py_2()
                    .text_xs()
                    .text_color(cx.theme().sidebar_foreground.opacity(0.5))
                    .child("No projects yet"),
            );
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
                            .min_w_0()
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
                    .child(self.render_sort_menu(cx))
                    // Electron's header button is add-project, not new-thread
                    // — per-project rows carry their own new-thread buttons.
                    .child(
                        Button::new("add-project")
                            .icon(Icon::new(IconName::FolderPlus).size_4())
                            .ghost()
                            .xsmall()
                            .tooltip("Add project")
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.open_add_project(window, cx)
                                }),
                            ),
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
}
