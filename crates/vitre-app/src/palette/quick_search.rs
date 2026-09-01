//! QuickSearch: the Electron `QuickSearch` overlay, in two modes.
//!
//! `Open` (⌘P) ranks open threads and workspace file paths; `Content` (⌘⇧F)
//! searches chat messages and file contents. Both are the same dialog with a
//! different corpus, which is how `apps/web/src/components/QuickSearch.tsx`
//! models it too.
//!
//! The overlay itself is gpui-component's [`Command`] hosted in a
//! [`Dialog`](gpui_component::dialog::Dialog): it already owns the query
//! field, the virtualized result list, arrow/Enter/Escape handling and the
//! empty slot. Only the corpus, the row bodies and the mode switch are ours.

use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, IntoElement, MouseButton, SharedString, Task,
    WeakEntity, Window, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, StyledExt as _,
    command::{Command, CommandGroup, CommandItem, CommandState},
    h_flex, v_flex,
};
use gpui_component::{IndexPath, WindowExt as _};
use vitre_client::EnvironmentClient;
use vitre_contracts::methods::{
    OrchestrationSearchMessages, ProjectsSearchContent, ProjectsSearchEntries,
};
use vitre_contracts::{
    OrchestrationMessageSearchMatch, OrchestrationSearchMessagesInput, OrchestrationThreadShell,
    ProjectEntryKind, ProjectSearchContentInput, ProjectSearchContentMatch,
    ProjectSearchEntriesInput, ThreadId, TrimmedNonEmptyString,
};

use crate::chat::{QuickSearchContent, QuickSearchOpen};

use super::rank::{match_line_segments, name_segments, split_search_result_path};
use super::relative_time;

/// Electron's `QUERY_DEBOUNCE_MS`.
const QUERY_DEBOUNCE: Duration = Duration::from_millis(200);
/// Electron's `THREAD_RESULT_LIMIT`.
const THREAD_RESULT_LIMIT: usize = 8;
/// Electron's `FILE_NAME_RESULT_LIMIT`.
const FILE_NAME_RESULT_LIMIT: i64 = 15;
/// Electron's `FILE_CONTENT_RESULT_LIMIT` — asked for.
const FILE_CONTENT_RESULT_LIMIT: i64 = 100;
/// Electron's `FILE_CONTENT_DISPLAY_LIMIT` — shown.
const FILE_CONTENT_DISPLAY_LIMIT: usize = 30;
/// Electron's `MESSAGE_RESULT_LIMIT`.
const MESSAGE_RESULT_LIMIT: i64 = 15;
/// Content search does not fire below this length (Electron: `>= 2`).
const CONTENT_MIN_QUERY: usize = 2;

/// Which corpus the overlay searches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QuickSearchMode {
    /// Threads and file paths — Electron's `"open"`.
    Open,
    /// Chat messages and file contents — Electron's `"content"`.
    Content,
}

impl QuickSearchMode {
    fn placeholder(self) -> &'static str {
        match self {
            QuickSearchMode::Open => "Jump to a chat or file…",
            QuickSearchMode::Content => "Search chat and file contents…",
        }
    }
}

/// What the overlay asks the shell to do once a row is confirmed.
pub enum QuickSearchEvent {
    /// Switch the chat view to this thread.
    OpenThread(ThreadId),
    /// Open a workspace file, optionally on a one-based line.
    OpenFile { path: String, line: Option<u32> },
    /// The dialog closed; the shell should drop its handle.
    Dismissed,
}

/// One result row.
#[derive(Clone)]
enum QuickItem {
    Thread {
        id: ThreadId,
        title: String,
        updated_at: String,
    },
    File {
        path: String,
    },
    FileMatch(ProjectSearchContentMatch),
    Message {
        thread_id: ThreadId,
        thread_title: String,
        snippet: String,
    },
}

/// A labelled run of rows ("Chats", "Files"), in Electron's order.
#[derive(Clone)]
struct QuickGroup {
    label: SharedString,
    items: Vec<QuickItem>,
}

pub struct QuickSearch {
    /// The palette's own state: query text, highlight, loading spinner.
    state: Entity<CommandState>,
    mode: QuickSearchMode,
    /// The last query the results correspond to, used for match highlighting.
    settled_query: String,
    groups: Vec<QuickGroup>,
    error: Option<SharedString>,
    /// The in-flight debounce + request. Dropping it cancels both, which is
    /// how a superseded query is discarded.
    _search: Option<Task<()>>,
    /// `None` until the environment connects. The overlay still opens then —
    /// Electron's does, and shows its empty state rather than swallowing the
    /// shortcut.
    client: Option<Arc<EnvironmentClient>>,
    /// Workspace root of the open thread's project; `None` on the home view,
    /// where Electron shows "Add a project to search."
    cwd: Option<String>,
    /// Whether a query is in flight; pushed into [`CommandState`] from
    /// `render`, which is the only place with a `Window` to push it with.
    loading: bool,
    threads: Vec<OrchestrationThreadShell>,
}

impl EventEmitter<QuickSearchEvent> for QuickSearch {}

fn tnes(text: impl Into<String>) -> TrimmedNonEmptyString {
    TrimmedNonEmptyString(text.into())
}

impl QuickSearch {
    /// Build the overlay and open its dialog.
    ///
    /// The caller keeps the returned handle alive for as long as the dialog is
    /// up — the dialog's content closure only holds a weak reference to it.
    pub fn open(
        mode: QuickSearchMode,
        client: Option<Arc<EnvironmentClient>>,
        cwd: Option<String>,
        threads: Vec<OrchestrationThreadShell>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let search = cx.new(|cx| {
            let state = cx.new(|cx| CommandState::new(window, cx));
            let mut this = Self {
                state,
                mode,
                settled_query: String::new(),
                groups: Vec::new(),
                error: None,
                _search: None,
                client,
                cwd,
                loading: false,
                threads,
            };
            // An empty query is not an empty result set in Open mode: it lists
            // recent threads, exactly as Electron's `rankThreads` does.
            this.run_query(String::new(), false, cx);
            this
        });

        let dialog_owner = search.downgrade();
        let close_owner = search.downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let content_owner = dialog_owner.clone();
            let close_owner = close_owner.clone();
            dialog
                .close_button(false)
                .p_0()
                // Electron: `pt-[12vh]`, `h-[26rem]`, `max-w-xl` with no
                // preview pane. The preview pane is deliberately not ported in
                // this pass, so the width stays at the list-only measure.
                .margin_top(px(96.))
                .w(px(576.))
                .max_w(px(576.))
                .on_close(move |_, _, cx| {
                    _ = close_owner.update(cx, |_, cx| cx.emit(QuickSearchEvent::Dismissed));
                })
                .content(move |content, window, cx| {
                    let Some(search) = content_owner.upgrade() else {
                        return content;
                    };
                    content.child(search.update(cx, |search, cx| search.render(window, cx)))
                })
        });

        // The dialog mounts on the next frame; focus the query field once it
        // exists, or the first keystroke goes to whatever had focus before.
        let focus_owner = search.downgrade();
        window.defer(cx, move |window, cx| {
            _ = focus_owner.update(cx, |search, cx| {
                search.state.update(cx, |state, cx| state.focus(window, cx));
            });
        });

        search
    }

    /// Switch corpus without losing the typed query (Electron re-runs the
    /// search on mode change and resets the highlight to the first row).
    pub fn set_mode(&mut self, mode: QuickSearchMode, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        let query = self.state.read(cx).query(cx).to_string();
        self.run_query(query, false, cx);
        cx.notify();
    }

    pub fn mode(&self) -> QuickSearchMode {
        self.mode
    }

    /// Debounce, then fetch. `debounce` is false for the seeding query and for
    /// a mode switch, where the delay would only add lag.
    fn run_query(&mut self, query: String, debounce: bool, cx: &mut Context<Self>) {
        let query = query.trim().to_string();
        let mode = self.mode;
        let client = self.client.clone();
        let cwd = self.cwd.clone();
        let threads = self.threads.clone();

        if mode == QuickSearchMode::Content && query.chars().count() < CONTENT_MIN_QUERY {
            self._search = None;
            self.settled_query = query;
            self.groups = Vec::new();
            self.error = None;
            self.set_loading(false, cx);
            cx.notify();
            return;
        }

        self.set_loading(true, cx);
        // Assigning over the previous task drops it, which cancels both the
        // pending timer and any request it had already issued.
        self._search = Some(cx.spawn(async move |this, cx| {
            if debounce {
                cx.background_executor().timer(QUERY_DEBOUNCE).await;
            }
            let (groups, error) = fetch(mode, &query, client, cwd, &threads).await;
            _ = this.update(cx, |this, cx| {
                this.settled_query = query;
                this.groups = groups;
                this.error = error;
                this._search = None;
                this.set_loading(false, cx);
                cx.notify();
            });
        }));
    }

    fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.loading = loading;
        cx.notify();
    }

    /// Resolve a confirmed [`IndexPath`] against the un-filtered corpus.
    ///
    /// Safe because the palette runs with `filterable(false)`: every supplied
    /// row is visible, so group/row coordinates are ours.
    fn confirm(&mut self, index: IndexPath, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self
            .groups
            .get(index.section)
            .and_then(|group| group.items.get(index.row))
        else {
            return;
        };
        let event = match item {
            QuickItem::Thread { id, .. } => QuickSearchEvent::OpenThread(id.clone()),
            QuickItem::Message { thread_id, .. } => QuickSearchEvent::OpenThread(thread_id.clone()),
            QuickItem::File { path } => QuickSearchEvent::OpenFile {
                path: path.clone(),
                line: None,
            },
            QuickItem::FileMatch(hit) => QuickSearchEvent::OpenFile {
                path: hit.path.0.clone(),
                line: Some(hit.line.0 as u32),
            },
        };
        cx.emit(event);
        window.close_dialog(cx);
    }

    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // `set_loading` notifies unconditionally, so only push a change.
        if self.state.read(cx).is_loading() != self.loading {
            let loading = self.loading;
            self.state
                .update(cx, |state, cx| state.set_loading(loading, window, cx));
        }

        let query = self.settled_query.clone();
        let mode = self.mode;
        let error = self.error.clone();
        let empty_copy: SharedString = if mode == QuickSearchMode::Content
            && self.settled_query.chars().count() < CONTENT_MIN_QUERY
        {
            "Type at least 2 characters to search.".into()
        } else if self.cwd.is_none() {
            "Add a project to search.".into()
        } else {
            "No results.".into()
        };

        let query_owner = cx.weak_entity();
        let confirm_owner = cx.weak_entity();
        let open_owner = cx.weak_entity();
        let content_owner = cx.weak_entity();

        let mut command = Command::new(&self.state)
            .bordered(false)
            // Every row we supply is already the answer to the query; the
            // built-in substring filter would re-filter results the server
            // ranked (and would hide content matches whose label is a path).
            .filterable(false)
            // Electron's overlay closes on the first Escape however much is
            // typed — its clear-then-close branch is gated off there.
            .cancel_clears_query(false)
            .placeholder(mode.placeholder())
            // Electron's dialog is a fixed `h-[26rem]` so arriving results do
            // not make it jump; minus the query row, the list gets the rest.
            .min_h(px(360.))
            .max_h(px(360.))
            .suffix(move |_, _, cx| {
                h_flex()
                    .gap_1()
                    .text_size(px(10.))
                    .text_color(cx.theme().muted_foreground)
                    .child(mode_chip(
                        "Open",
                        mode == QuickSearchMode::Open,
                        open_owner.clone(),
                        QuickSearchMode::Open,
                        cx,
                    ))
                    .child(mode_chip(
                        "Content",
                        mode == QuickSearchMode::Content,
                        content_owner.clone(),
                        QuickSearchMode::Content,
                        cx,
                    ))
            })
            .empty(move |_, _, cx| {
                v_flex()
                    .w_full()
                    .items_center()
                    .py_6()
                    .px_2()
                    .text_size(px(12.))
                    .text_color(cx.theme().muted_foreground)
                    .child(empty_copy.clone())
            })
            .on_query(move |query, _, cx| {
                let query = query.to_string();
                _ = query_owner.update(cx, |this, cx| this.run_query(query, true, cx));
            })
            .on_confirm(move |index, window, cx| {
                _ = confirm_owner.update(cx, |this, cx| this.confirm(index, window, cx));
            });

        // A failed ripgrep spawn must not read as "No results" — Electron
        // renders the error above the list for the same reason.
        if let Some(error) = error {
            command = command.header(move |_, _, cx| {
                div()
                    .px_3()
                    .py_1p5()
                    .text_size(px(12.))
                    .text_color(cx.theme().danger)
                    .child(error.clone())
            });
        }

        for group in &self.groups {
            let mut entry = CommandGroup::new().label(group.label.clone());
            for item in &group.items {
                entry = entry.item(command_item(item, &query));
            }
            command = command.group(entry);
        }

        // The dialog renders outside the shell's element tree, so while it
        // holds focus the shell's copies of these handlers are not on the
        // dispatch path. Electron's toggle semantics live here instead: the
        // showing mode's shortcut closes, the other one switches corpus.
        div()
            .size_full()
            .on_action(cx.listener(|this, _: &QuickSearchOpen, window, cx| {
                this.toggle(QuickSearchMode::Open, window, cx);
            }))
            .on_action(cx.listener(|this, _: &QuickSearchContent, window, cx| {
                this.toggle(QuickSearchMode::Content, window, cx);
            }))
            .child(command)
            .into_any_element()
    }

    /// The shortcut pressed while the overlay is already up.
    fn toggle(&mut self, mode: QuickSearchMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == mode {
            window.close_dialog(cx);
        } else {
            self.set_mode(mode, cx);
        }
    }
}

/// The inline "Open"/"Content" switch Electron renders at the trailing end of
/// the query field.
fn mode_chip(
    label: &'static str,
    active: bool,
    owner: WeakEntity<QuickSearch>,
    mode: QuickSearchMode,
    cx: &App,
) -> impl IntoElement {
    div()
        .id(label)
        .px_1p5()
        .py_0p5()
        .rounded(cx.theme().radius)
        .when(active, |this| {
            this.bg(cx.theme().accent)
                .text_color(cx.theme().accent_foreground)
        })
        .when(!active, |this| {
            this.hover(|style| style.bg(cx.theme().accent.opacity(0.6)))
        })
        .cursor_pointer()
        .child(label)
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            _ = owner.update(cx, |search, cx| search.set_mode(mode, cx));
        })
}

/// Build the palette row for one result.
///
/// The label is what a screen reader and the (disabled) local filter see; the
/// body is the custom child, since every row kind lays out differently.
fn command_item(item: &QuickItem, query: &str) -> CommandItem {
    let label: SharedString = match item {
        QuickItem::Thread { title, .. } => title.clone().into(),
        QuickItem::File { path } => path.clone().into(),
        QuickItem::FileMatch(hit) => hit.path.0.clone().into(),
        QuickItem::Message { thread_title, .. } => thread_title.clone().into(),
    };
    let item = item.clone();
    let query = query.to_string();
    CommandItem::new()
        .label(label)
        .child(move |_, cx| render_row(&item, &query, cx))
}

fn render_row(item: &QuickItem, query: &str, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let icon = match item {
        QuickItem::Thread { .. } | QuickItem::Message { .. } => IconName::MessageSquare,
        QuickItem::File { .. } | QuickItem::FileMatch(_) => IconName::File,
    };
    let body: AnyElement = match item {
        QuickItem::Thread {
            title, updated_at, ..
        } => h_flex()
            .min_w_0()
            .flex_1()
            .gap_2()
            .items_center()
            .child(highlighted_name(title, query, cx))
            .child(
                div()
                    .ml_auto()
                    .flex_shrink_0()
                    .text_size(px(10.))
                    .text_color(muted)
                    .child(relative_time(updated_at).unwrap_or_default()),
            )
            .into_any_element(),
        QuickItem::File { path } => {
            let (name, directory) = split_search_result_path(path);
            h_flex()
                .min_w_0()
                .flex_1()
                .gap_2()
                .items_baseline()
                .child(highlighted_name(name, query, cx))
                .when(!directory.is_empty(), |this| {
                    this.child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(10.))
                            .text_color(muted)
                            .child(directory.to_string()),
                    )
                })
                .into_any_element()
        }
        QuickItem::FileMatch(hit) => {
            let (name, _) = split_search_result_path(&hit.path.0);
            let segments = match_line_segments(
                &hit.line_text.0,
                hit.match_start.0 as usize,
                hit.match_end.0 as usize,
            );
            v_flex()
                .min_w_0()
                .flex_1()
                .child(
                    h_flex()
                        .min_w_0()
                        .items_baseline()
                        .child(div().truncate().font_medium().child(name.to_string()))
                        .child(
                            div()
                                .ml_1()
                                .flex_shrink_0()
                                .text_size(px(10.))
                                .text_color(muted)
                                .child(format!(":{}", hit.line.0)),
                        ),
                )
                .child(
                    h_flex()
                        .min_w_0()
                        .text_color(muted)
                        .when(segments.before_clipped, |this| this.child("…"))
                        .child(segments.before)
                        .child(highlight_span(segments.matched, cx))
                        .child(div().min_w_0().truncate().child(segments.after)),
                )
                .into_any_element()
        }
        QuickItem::Message {
            thread_title,
            snippet,
            ..
        } => v_flex()
            .min_w_0()
            .flex_1()
            .child(
                div()
                    .truncate()
                    .font_medium()
                    .child(thread_title.to_string()),
            )
            .child(
                div()
                    .truncate()
                    .text_color(muted)
                    .child(snippet.to_string()),
            )
            .into_any_element(),
    };

    h_flex()
        .w_full()
        .gap_2()
        .items_center()
        .text_size(px(12.))
        .child(Icon::new(icon).size_3p5().text_color(muted))
        .child(body)
        .into_any_element()
}

/// `HighlightedName`: the first case-insensitive hit gets the amber wash.
fn highlighted_name(text: &str, query: &str, cx: &App) -> impl IntoElement {
    let (before, matched, after) = name_segments(text, query);
    // Every text run needs its own truncating box: a bare string child of a
    // flex row keeps its intrinsic width and paints over the next sibling
    // (a long unmatched filename would sit on top of its directory suffix).
    h_flex()
        .min_w_0()
        .overflow_hidden()
        .child(div().min_w_0().truncate().child(before.to_string()))
        .when(!matched.is_empty(), |this| {
            this.child(highlight_span(matched.to_string(), cx))
        })
        .when(!after.is_empty(), |this| {
            this.child(div().min_w_0().truncate().child(after.to_string()))
        })
}

/// Electron's `rounded-xs bg-amber-400/25 text-foreground` match wash.
fn highlight_span(text: String, cx: &App) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .rounded(px(2.))
        .bg(gpui::rgba(0xfbbf2440))
        .text_color(cx.theme().foreground)
        .child(text)
}

/// Query the corpus for `mode`. Returns the groups Electron would render, plus
/// the first error worth surfacing.
async fn fetch(
    mode: QuickSearchMode,
    query: &str,
    client: Option<Arc<EnvironmentClient>>,
    cwd: Option<String>,
    threads: &[OrchestrationThreadShell],
) -> (Vec<QuickGroup>, Option<SharedString>) {
    let Some(client) = client else {
        // No environment yet: the thread list is empty too, so every mode
        // lands on the empty slot.
        return (Vec::new(), None);
    };
    match mode {
        QuickSearchMode::Open => {
            let thread_items = rank_threads(threads, query);
            let (file_items, error) = match cwd {
                None => (Vec::new(), None),
                Some(cwd) if query.is_empty() => {
                    let _ = cwd;
                    (Vec::new(), None)
                }
                Some(cwd) => {
                    let payload = ProjectSearchEntriesInput {
                        cwd: tnes(cwd),
                        limit: FILE_NAME_RESULT_LIMIT,
                        query: tnes(query),
                    };
                    match client.call::<ProjectsSearchEntries>(&payload).await {
                        Ok(result) => (
                            result
                                .entries
                                .into_iter()
                                .filter(|entry| entry.kind == ProjectEntryKind::File)
                                .map(|entry| QuickItem::File { path: entry.path.0 })
                                .collect(),
                            None,
                        ),
                        Err(error) => (Vec::new(), Some(format!("{error}").into())),
                    }
                }
            };
            (
                groups(&[("Chats", thread_items), ("Files", file_items)]),
                error,
            )
        }
        QuickSearchMode::Content => {
            let messages = client
                .call::<OrchestrationSearchMessages>(&OrchestrationSearchMessagesInput {
                    limit: MESSAGE_RESULT_LIMIT,
                    query: tnes(query),
                })
                .await;
            let (message_items, message_error) = match messages {
                Ok(result) => (result.matches.into_iter().map(message_item).collect(), None),
                Err(error) => (Vec::new(), Some(SharedString::from(format!("{error}")))),
            };

            let (file_items, file_error) = match cwd {
                None => (Vec::new(), None),
                Some(cwd) => {
                    let payload = ProjectSearchContentInput {
                        case_sensitive: None,
                        cwd: tnes(cwd),
                        exclude_glob: None,
                        include_glob: None,
                        max_results: Some(Some(FILE_CONTENT_RESULT_LIMIT)),
                        query: query.to_string(),
                        regex: None,
                        whole_word: None,
                    };
                    match client.call::<ProjectsSearchContent>(&payload).await {
                        Ok(result) => (
                            result
                                .matches
                                .into_iter()
                                .take(FILE_CONTENT_DISPLAY_LIMIT)
                                .map(QuickItem::FileMatch)
                                .collect(),
                            None,
                        ),
                        Err(error) => (Vec::new(), Some(SharedString::from(format!("{error}")))),
                    }
                }
            };

            (
                groups(&[("Chats", message_items), ("Files", file_items)]),
                file_error.or(message_error),
            )
        }
    }
}

fn message_item(hit: OrchestrationMessageSearchMatch) -> QuickItem {
    QuickItem::Message {
        thread_id: hit.thread_id,
        thread_title: hit.thread_title.0,
        snippet: hit.snippet.0,
    }
}

/// Drop empty groups, as Electron's `.filter((group) => group.items.length > 0)`
/// does — an empty section heading is worse than no section.
fn groups(sections: &[(&str, Vec<QuickItem>)]) -> Vec<QuickGroup> {
    sections
        .iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(label, items)| QuickGroup {
            label: SharedString::from(label.to_string()),
            items: items.clone(),
        })
        .collect()
}

/// Electron's `rankThreads`: active threads whose title contains the query,
/// newest first, capped.
fn rank_threads(threads: &[OrchestrationThreadShell], query: &str) -> Vec<QuickItem> {
    let needle = query.to_lowercase();
    let mut matching: Vec<&OrchestrationThreadShell> = threads
        .iter()
        // Triple-option: absent key / explicit null / a timestamp. Only the
        // last means archived.
        .filter(|thread| !matches!(&thread.archived_at, Some(Some(Some(_)))))
        .filter(|thread| needle.is_empty() || thread.title.0.to_lowercase().contains(&needle))
        .collect();
    matching.sort_by(|left, right| right.updated_at.0.cmp(&left.updated_at.0));
    matching
        .into_iter()
        .take(THREAD_RESULT_LIMIT)
        .map(|thread| QuickItem::Thread {
            id: thread.id.clone(),
            title: thread.title.0.clone(),
            updated_at: thread.updated_at.0.clone(),
        })
        .collect()
}
