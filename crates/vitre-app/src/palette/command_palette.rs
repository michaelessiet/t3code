//! The ⇧⌘P command palette, ported from `apps/web/src/components/CommandPalette.tsx`.
//!
//! Like [`super::quick_search`], the surface itself is gpui-component's
//! [`Command`] inside a [`Dialog`](gpui_component::dialog::Dialog); what is
//! ours is the item model, the group assembly, and the Electron ranking rules
//! in [`super::rank`].
//!
//! Scope: this is the **root** palette. Electron's add-project flow (its
//! environments → sources → path-browsing sub-views) belongs to the
//! connections work and is deliberately absent, as are the commands whose
//! subsystems Vitre does not have yet — terminal, knowledge graph, workspace
//! roots, settings. Everything present here behaves as Electron does,
//! including the one submenu it keeps ("New thread in…").

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, IntoElement, KeyDownEvent, SharedString,
    Window, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName,
    command::{Command, CommandGroup, CommandItem, CommandState},
    h_flex,
    kbd::Kbd,
    v_flex,
};
use gpui_component::{IndexPath, WindowExt as _};
use vitre_contracts::{OrchestrationProjectShell, OrchestrationThreadShell, ProjectId, ThreadId};

use crate::chat::{
    CommandPaletteToggle, NewThread, QuickSearchContent, QuickSearchOpen, ToggleFilesPanel,
};

use super::rank::{normalize_search_text, rank_indices};
use super::relative_time;

/// Electron's `RECENT_THREAD_LIMIT`.
const RECENT_THREAD_LIMIT: usize = 12;
/// Electron enumerates only the project picker with ⌘1..⌘9.
const POSITIONAL_JUMP_LIMIT: usize = 9;

/// What the palette asks the shell to do once a row is confirmed. Everything
/// the palette can run is a shell capability, so the palette itself stays a
/// pure view over the snapshot.
#[derive(Clone, Debug, PartialEq)]
pub enum PaletteAction {
    /// Start a thread in `project_id`, or in the contextual project when it is
    /// `None` (Electron's `startNewThreadFromContext`).
    NewThread {
        project_id: Option<ProjectId>,
    },
    OpenThread(ThreadId),
    /// Electron's `openProjectFromSearch`: jump to the project's most recent
    /// thread, or start one when it has none.
    OpenProject(ProjectId),
    QuickSearchOpen,
    QuickSearchContent,
    ToggleFilesPanel,
    NewFile,
    NewFolder,
}

pub enum CommandPaletteEvent {
    Run(PaletteAction),
    /// The dialog closed; the shell should drop its handle.
    Dismissed,
}

/// One palette row. `terms` is the only thing matched — Electron never
/// searches the rendered title or description either.
#[derive(Clone)]
struct PaletteItem {
    terms: Vec<String>,
    title: SharedString,
    description: Option<SharedString>,
    timestamp: Option<SharedString>,
    icon: IconName,
    /// Rendered chord, already formatted for the platform.
    shortcut: Option<SharedString>,
    /// A row that pushes [`Self::submenu`] instead of running an action.
    submenu: Option<PaletteGroup>,
    action: Option<PaletteAction>,
}

impl PaletteItem {
    fn new(title: impl Into<SharedString>, icon: IconName, terms: &[&str]) -> Self {
        Self {
            terms: terms.iter().map(|term| (*term).to_string()).collect(),
            title: title.into(),
            description: None,
            timestamp: None,
            icon,
            shortcut: None,
            submenu: None,
            action: None,
        }
    }

    fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    fn timestamp(mut self, timestamp: impl Into<SharedString>) -> Self {
        self.timestamp = Some(timestamp.into());
        self
    }

    fn shortcut(mut self, shortcut: Option<SharedString>) -> Self {
        self.shortcut = shortcut;
        self
    }

    fn action(mut self, action: PaletteAction) -> Self {
        self.action = Some(action);
        self
    }

    fn submenu(mut self, group: PaletteGroup) -> Self {
        self.submenu = Some(group);
        self
    }
}

#[derive(Clone)]
struct PaletteGroup {
    label: SharedString,
    items: Vec<PaletteItem>,
}

impl PaletteGroup {
    fn new(label: impl Into<SharedString>, items: Vec<PaletteItem>) -> Self {
        Self {
            label: label.into(),
            items,
        }
    }
}

pub struct CommandPalette {
    state: Entity<CommandState>,
    query: String,
    /// Pushed sub-views, innermost last. Empty means the root palette.
    stack: Vec<PaletteGroup>,
    /// The groups the last render displayed, so a confirmed [`IndexPath`]
    /// resolves against exactly what the user was looking at.
    displayed: Vec<PaletteGroup>,
    actions: Vec<PaletteItem>,
    threads: Vec<PaletteItem>,
    projects: Vec<PaletteItem>,
}

impl EventEmitter<CommandPaletteEvent> for CommandPalette {}

/// Everything the palette needs from the shell, snapshotted at open time —
/// Electron's palette likewise renders from the store as it stood when the
/// dialog mounted.
pub struct PaletteContext {
    pub projects: Vec<OrchestrationProjectShell>,
    pub threads: Vec<OrchestrationThreadShell>,
    pub active_thread: Option<ThreadId>,
    pub active_project: Option<ProjectId>,
}

impl CommandPalette {
    /// Build the palette and open its dialog.
    ///
    /// The caller keeps the returned handle alive for as long as the dialog is
    /// up — the dialog's content closure only holds a weak reference to it.
    pub fn open(context: PaletteContext, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let projects = project_items(&context, PaletteAction::OpenProject);
        let project_targets = project_items(&context, |project_id| PaletteAction::NewThread {
            project_id: Some(project_id),
        });
        let actions = action_items(&context, project_targets, window);
        let threads = thread_items(&context);

        let palette = cx.new(|cx| Self {
            state: cx.new(|cx| CommandState::new(window, cx)),
            query: String::new(),
            stack: Vec::new(),
            displayed: Vec::new(),
            actions,
            threads,
            projects,
        });

        let dialog_owner = palette.downgrade();
        let close_owner = palette.downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let content_owner = dialog_owner.clone();
            let close_owner = close_owner.clone();
            dialog
                .close_button(false)
                .p_0()
                // Electron: `sm:py-[10vh]`, `max-w-xl`, `max-h-105`.
                .margin_top(px(72.))
                .w(px(576.))
                .max_w(px(576.))
                .on_close(move |_, _, cx| {
                    _ = close_owner.update(cx, |_, cx| cx.emit(CommandPaletteEvent::Dismissed));
                })
                .content(move |content, window, cx| {
                    let Some(palette) = content_owner.upgrade() else {
                        return content;
                    };
                    content.child(palette.update(cx, |palette, cx| palette.render(window, cx)))
                })
        });

        // The dialog mounts on the next frame; focus the query field once it
        // exists, or the first keystroke goes to whatever had focus before.
        let focus_owner = palette.downgrade();
        window.defer(cx, move |window, cx| {
            _ = focus_owner.update(cx, |palette, cx| {
                palette
                    .state
                    .update(cx, |state, cx| state.focus(window, cx));
            });
        });

        palette
    }

    fn visible_groups(&self) -> Vec<PaletteGroup> {
        let groups = match self.stack.last() {
            Some(view) => vec![view.clone()],
            None => root_groups(&self.actions, &self.projects, &self.threads, &self.query),
        };
        filter_groups(groups, &self.query)
    }

    fn confirm(&mut self, index: IndexPath, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self
            .displayed
            .get(index.section)
            .and_then(|group| group.items.get(index.row))
            .cloned()
        else {
            return;
        };
        if let Some(submenu) = item.submenu {
            self.push_view(submenu, window, cx);
            return;
        }
        let Some(action) = item.action else {
            return;
        };
        cx.emit(CommandPaletteEvent::Run(action));
        self.dismiss(window, cx);
    }

    /// Close the palette and tell the shell we are gone.
    ///
    /// `close_dialog` only pops the dialog off `Root`; it never runs the
    /// dialog's own `on_close`, which is invoked solely by the Dialog
    /// element's Escape/confirm/backdrop handlers. So every close path that
    /// goes through `close_dialog` has to emit `Dismissed` itself, or the
    /// shell keeps holding this entity and refuses to open a fresh one.
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.close_dialog(cx);
        cx.emit(CommandPaletteEvent::Dismissed);
    }

    fn push_view(&mut self, group: PaletteGroup, window: &mut Window, cx: &mut Context<Self>) {
        self.stack.push(group);
        self.query.clear();
        self.state
            .update(cx, |state, cx| state.set_query("", window, cx));
        cx.notify();
    }

    /// Backspace at an empty query pops one level, as Electron's does.
    fn pop_view(&mut self, cx: &mut Context<Self>) {
        if self.stack.pop().is_some() {
            cx.notify();
        }
    }

    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let groups = self.visible_groups();
        self.displayed = groups.clone();

        let in_submenu = !self.stack.is_empty();
        let placeholder = if in_submenu {
            "Search..."
        } else {
            "Search commands, projects, and threads..."
        };
        let empty_copy: SharedString = if self.query.starts_with('>') {
            "No matching actions.".into()
        } else {
            "No matching commands, projects, or threads.".into()
        };

        let query_owner = cx.weak_entity();
        let confirm_owner = cx.weak_entity();

        let mut command = Command::new(&self.state)
            .bordered(false)
            // Every row on screen already passed the Electron ranking; the
            // built-in substring filter would re-filter on the rendered label,
            // which our rows do not even carry.
            .filterable(false)
            // Electron's dialog closes on the first Escape however much is
            // typed — its clear-then-close branch is gated off there.
            .cancel_clears_query(false)
            .placeholder(placeholder)
            // Electron: `max-h-105` on the popup, `max-h-[min(28rem,70vh)]` on
            // the list. Fixed, so arriving results do not make it jump.
            .min_h(px(300.))
            .max_h(px(300.))
            .empty(move |_, _, cx| {
                v_flex()
                    .w_full()
                    .items_center()
                    .py_10()
                    .px_2()
                    .text_size(px(12.))
                    .text_color(cx.theme().muted_foreground)
                    .child(empty_copy.clone())
            })
            .footer(move |_, _, cx| footer(in_submenu, cx))
            .on_query(move |query, _, cx| {
                let query = query.to_string();
                _ = query_owner.update(cx, |this, cx| {
                    this.query = query;
                    cx.notify();
                });
            })
            .on_confirm(move |index, window, cx| {
                _ = confirm_owner.update(cx, |this, cx| this.confirm(index, window, cx));
            });

        for group in &groups {
            let mut entry = CommandGroup::new().label(group.label.clone());
            for item in &group.items {
                entry = entry.item(command_item(item));
            }
            command = command.group(entry);
        }

        // A deferred draw keeps its parent dispatch node, so this element sits
        // *below* the shell's handler for the same action and shadows it on
        // the way up. That is what gives Electron's toggle semantics: the
        // chord closes the palette that is already showing.
        div()
            .size_full()
            .on_action(cx.listener(|this, _: &CommandPaletteToggle, window, cx| {
                this.dismiss(window, cx);
            }))
            // Electron pops a sub-view on Backspace at an empty query. The
            // capture phase is the only place to see it: the query field would
            // otherwise swallow the key on its way down.
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "backspace"
                    && this.query.is_empty()
                    && !this.stack.is_empty()
                {
                    this.pop_view(cx);
                    cx.stop_propagation();
                }
            }))
            .child(command)
            .into_any_element()
    }
}

/// Which groups are on offer for `query` at the root — the first half of
/// Electron's `filterCommandPaletteGroups`.
///
/// A `>` prefix narrows to actions. An empty query lists the recent threads;
/// any other query swaps that for the full thread and project corpora, so a
/// typed query reaches threads outside the recent 12.
fn root_groups(
    actions: &[PaletteItem],
    projects: &[PaletteItem],
    threads: &[PaletteItem],
    query: &str,
) -> Vec<PaletteGroup> {
    let actions_group = || PaletteGroup::new("Actions", actions.to_vec());
    if query.starts_with('>') {
        return vec![actions_group()];
    }
    if normalize_search_text(query).is_empty() {
        return vec![
            actions_group(),
            PaletteGroup::new(
                "Recent Threads",
                threads.iter().take(RECENT_THREAD_LIMIT).cloned().collect(),
            ),
        ];
    }
    vec![
        actions_group(),
        PaletteGroup::new("Projects", projects.to_vec()),
        PaletteGroup::new("Threads", threads.to_vec()),
    ]
}

/// Rank each group's items against `query` and drop the groups that empty out
/// — the second half of `filterCommandPaletteGroups`.
fn filter_groups(groups: Vec<PaletteGroup>, query: &str) -> Vec<PaletteGroup> {
    let query = normalize_search_text(query.strip_prefix('>').unwrap_or(query));
    groups
        .into_iter()
        .filter_map(|group| {
            let terms: Vec<Vec<String>> =
                group.items.iter().map(|item| item.terms.clone()).collect();
            let items: Vec<PaletteItem> = rank_indices(&terms, &query)
                .into_iter()
                .map(|index| group.items[index].clone())
                .collect();
            (!items.is_empty()).then(|| PaletteGroup::new(group.label, items))
        })
        .collect()
}

/// Electron's row: icon, then title over an optional description, then the
/// timestamp, then the shortcut chip or the submenu chevron.
fn command_item(item: &PaletteItem) -> CommandItem {
    let item = item.clone();
    CommandItem::new().child(move |_, cx| {
        let muted = cx.theme().muted_foreground;
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(Icon::new(item.icon.clone()).size_4().text_color(muted))
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .child(div().truncate().child(item.title.clone()))
                    .children(item.description.clone().map(|description| {
                        div()
                            .truncate()
                            .text_size(px(10.))
                            .text_color(muted)
                            .child(description)
                    })),
            )
            .children(item.timestamp.clone().map(|timestamp| {
                div()
                    .flex_shrink_0()
                    .text_size(px(10.))
                    .text_color(muted)
                    .child(timestamp)
            }))
            .children(item.shortcut.clone().map(|shortcut| {
                div()
                    .flex_shrink_0()
                    .text_size(px(11.))
                    .text_color(muted)
                    .child(shortcut)
            }))
            .when(item.submenu.is_some(), |this| {
                this.child(
                    Icon::new(IconName::ChevronRight)
                        .size_4()
                        .flex_shrink_0()
                        .text_color(muted),
                )
            })
    })
}

/// Electron's `CommandFooter`, minus the browse-specific hints.
fn footer(in_submenu: bool, cx: &App) -> AnyElement {
    let hint = |keys: &'static str, label: &'static str, cx: &App| {
        h_flex()
            .gap_1()
            .items_center()
            .child(
                div()
                    .rounded(px(3.))
                    .px_1()
                    .bg(cx.theme().muted)
                    .text_color(cx.theme().foreground)
                    .child(keys),
            )
            .child(label)
    };
    h_flex()
        .w_full()
        .gap_3()
        .px_5()
        .py_3()
        .text_size(px(11.))
        .text_color(cx.theme().muted_foreground)
        .border_t_1()
        .border_color(cx.theme().border)
        .child(hint("↑↓", "Navigate", cx))
        .child(hint("Enter", "Select", cx))
        .when(in_submenu, |this| this.child(hint("Backspace", "Back", cx)))
        .child(hint("Esc", "Close", cx))
        .into_any_element()
}

/// The chord the live keymap resolves for `action`, formatted the way Electron
/// formats it (⌃⌥⇧⌘ order, no separator on macOS). `None` renders no chip,
/// which is also what Electron does for an unbound command.
fn shortcut_for(action: &dyn gpui::Action, window: &Window) -> Option<SharedString> {
    let binding = window.highest_precedence_binding_for_action(action)?;
    let keystroke = binding.keystrokes().first()?;
    Some(Kbd::format(&gpui::AsKeystroke::as_keystroke(keystroke).clone()).into())
}

/// The "Actions" group, in Electron's order. Commands whose subsystem Vitre
/// does not have yet are simply absent rather than shown disabled — a palette
/// row that cannot run is worse than no row.
fn action_items(
    context: &PaletteContext,
    project_targets: Vec<PaletteItem>,
    window: &Window,
) -> Vec<PaletteItem> {
    let mut items = Vec::new();

    let active_project_title = context.active_project.as_ref().and_then(|id| {
        context
            .projects
            .iter()
            .find(|project| project.id == *id)
            .map(|project| project.title.0.clone())
    });
    if let Some(title) = active_project_title {
        items.push(
            PaletteItem::new(
                format!("New thread in {title}"),
                IconName::SquarePen,
                &["new thread", "chat", "create", "draft"],
            )
            .shortcut(shortcut_for(&NewThread, window))
            .action(PaletteAction::NewThread { project_id: None }),
        );
    }
    if !project_targets.is_empty() {
        items.push(
            PaletteItem::new(
                "New thread in...",
                IconName::SquarePen,
                &["new thread", "project", "pick", "choose", "select"],
            )
            .submenu(PaletteGroup::new("Projects", project_targets)),
        );
    }

    // Electron gates the workspace block on an active thread, because every
    // command in it addresses the open workspace.
    if context.active_thread.is_some() {
        items.push(
            PaletteItem::new(
                "Quick open chat or file",
                IconName::Search,
                &[
                    "quick open",
                    "go to file",
                    "jump",
                    "find file",
                    "open file",
                    "chat",
                ],
            )
            .shortcut(shortcut_for(&QuickSearchOpen, window))
            .action(PaletteAction::QuickSearchOpen),
        );
        items.push(
            PaletteItem::new(
                "Search chats and files",
                IconName::Search,
                &[
                    "search",
                    "content",
                    "grep",
                    "find in files",
                    "chats",
                    "messages",
                ],
            )
            .shortcut(shortcut_for(&QuickSearchContent, window))
            .action(PaletteAction::QuickSearchContent),
        );
        items.push(
            PaletteItem::new(
                "Toggle right panel",
                IconName::PanelRight,
                &["right panel", "dock", "sidebar", "toggle", "editor panel"],
            )
            .shortcut(shortcut_for(&ToggleFilesPanel, window))
            .action(PaletteAction::ToggleFilesPanel),
        );
        items.push(
            PaletteItem::new(
                "New file",
                IconName::FilePlus2,
                &["new file", "create file", "file tree", "explorer"],
            )
            .action(PaletteAction::NewFile),
        );
        items.push(
            PaletteItem::new(
                "New folder",
                IconName::FolderPlus,
                &[
                    "new folder",
                    "new directory",
                    "create folder",
                    "file tree",
                    "explorer",
                ],
            )
            .action(PaletteAction::NewFolder),
        );
    }

    items
}

/// Electron's `buildProjectActionItems`. `run` decides which corpus this is:
/// the search results open the project, the submenu starts a thread in it.
fn project_items(
    context: &PaletteContext,
    run: impl Fn(ProjectId) -> PaletteAction,
) -> Vec<PaletteItem> {
    context
        .projects
        .iter()
        .enumerate()
        .map(|(index, project)| {
            let item = PaletteItem::new(
                project.title.0.clone(),
                IconName::Folder,
                &[project.title.0.as_str(), project.workspace_root.0.as_str()],
            )
            .description(project.workspace_root.0.clone())
            .action(run(project.id.clone()));
            // Electron enumerates the project picker positionally; the tenth
            // and beyond carry no chip.
            if index < POSITIONAL_JUMP_LIMIT {
                item.shortcut(Some(format!("⌘{}", index + 1).into()))
            } else {
                item
            }
        })
        .collect()
}

/// Electron's `buildThreadActionItems`: every non-archived thread, newest
/// first, described by its project and branch.
fn thread_items(context: &PaletteContext) -> Vec<PaletteItem> {
    let project_title = |id: &ProjectId| {
        context
            .projects
            .iter()
            .find(|project| project.id == *id)
            .map(|project| project.title.0.clone())
    };
    context
        .threads
        .iter()
        .map(|thread| {
            let mut parts: Vec<String> = Vec::new();
            if let Some(title) = project_title(&thread.project_id) {
                parts.push(title);
            }
            if let Some(branch) = &thread.branch {
                parts.push(format!("#{}", branch.0));
            }
            if context.active_thread.as_ref() == Some(&thread.id) {
                parts.push("Current thread".into());
            }
            let stamp = thread
                .latest_user_message_at
                .as_ref()
                .map(|at| at.0.as_str())
                .unwrap_or(thread.updated_at.0.as_str());
            let mut item = PaletteItem::new(
                thread.title.0.clone(),
                IconName::MessageSquare,
                &[
                    thread.title.0.as_str(),
                    project_title(&thread.project_id)
                        .unwrap_or_default()
                        .as_str(),
                    thread.branch.as_ref().map(|b| b.0.as_str()).unwrap_or(""),
                ],
            )
            .action(PaletteAction::OpenThread(thread.id.clone()));
            if !parts.is_empty() {
                item = item.description(parts.join(" · "));
            }
            if let Some(stamp) = relative_time(stamp) {
                item = item.timestamp(stamp);
            }
            item
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, terms: &[&str]) -> PaletteItem {
        PaletteItem::new(title.to_string(), IconName::Search, terms)
    }

    fn labels(groups: &[PaletteGroup]) -> Vec<(String, Vec<String>)> {
        groups
            .iter()
            .map(|group| {
                (
                    group.label.to_string(),
                    group
                        .items
                        .iter()
                        .map(|item| item.title.to_string())
                        .collect(),
                )
            })
            .collect()
    }

    fn corpus() -> (Vec<PaletteItem>, Vec<PaletteItem>, Vec<PaletteItem>) {
        let actions = vec![
            item("New file", &["new file", "create file"]),
            item("Toggle right panel", &["right panel", "dock"]),
        ];
        let projects = vec![item("panel-lab", &["panel-lab", "/src/panel-lab"])];
        let threads: Vec<PaletteItem> = (0..15)
            .map(|index| {
                let title = format!("thread {index}");
                item(&title, &[title.as_str()])
            })
            .collect();
        (actions, projects, threads)
    }

    #[test]
    fn empty_query_lists_actions_then_the_recent_twelve() {
        let (actions, projects, threads) = corpus();
        let groups = root_groups(&actions, &projects, &threads, "");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].label, "Actions");
        assert_eq!(groups[1].label, "Recent Threads");
        assert_eq!(groups[1].items.len(), RECENT_THREAD_LIMIT);
    }

    #[test]
    fn a_search_swaps_recent_threads_for_the_full_corpora() {
        let (actions, projects, threads) = corpus();
        let groups = root_groups(&actions, &projects, &threads, "panel");
        let labels: Vec<&str> = groups.iter().map(|group| group.label.as_ref()).collect();
        assert_eq!(labels, ["Actions", "Projects", "Threads"]);
        assert_eq!(groups[2].items.len(), threads.len());
    }

    #[test]
    fn the_actions_prefix_narrows_to_actions_with_or_without_a_query() {
        let (actions, projects, threads) = corpus();
        for query in [">", ">panel"] {
            let groups = root_groups(&actions, &projects, &threads, query);
            assert_eq!(groups.len(), 1, "{query}");
            assert_eq!(groups[0].label, "Actions");
        }
    }

    #[test]
    fn filtering_drops_empty_groups_and_strips_the_actions_prefix() {
        let (actions, projects, threads) = corpus();
        let groups = filter_groups(root_groups(&actions, &projects, &threads, "panel"), "panel");
        assert_eq!(
            labels(&groups),
            [
                ("Actions".into(), vec!["Toggle right panel".to_string()]),
                ("Projects".into(), vec!["panel-lab".to_string()]),
            ]
        );

        // `>panel` matches the same action, and the prefix never reaches the
        // matcher.
        let groups = filter_groups(
            root_groups(&actions, &projects, &threads, ">panel"),
            ">panel",
        );
        assert_eq!(
            labels(&groups),
            [("Actions".into(), vec!["Toggle right panel".to_string()])]
        );
    }

    #[test]
    fn a_pushed_view_replaces_the_root_groups() {
        let (actions, projects, threads) = corpus();
        let view = PaletteGroup::new("Projects", projects.clone());
        let groups = filter_groups(vec![view], "");
        assert_eq!(
            labels(&groups),
            [("Projects".into(), vec!["panel-lab".to_string()])]
        );
        let _ = (actions, threads);
    }
}
