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
use gpui_component::input::EditorState;
use gpui_component::{
    ActiveTheme as _, Icon, IconName, StyledExt as _,
    command::{Command, CommandGroup, CommandItem, CommandState},
    h_flex, v_flex,
};
use gpui_component::{IndexPath, WindowExt as _};
use vitre_client::EnvironmentClient;
use vitre_contracts::methods::{
    OrchestrationSearchMessages, ProjectsReadFile, ProjectsSearchContent, ProjectsSearchEntries,
};
use vitre_contracts::{
    OrchestrationMessageSearchMatch, OrchestrationProjectShell, OrchestrationSearchMessagesInput,
    OrchestrationThreadShell, ProjectEntryKind, ProjectReadFileInput, ProjectSearchContentInput,
    ProjectSearchContentMatch, ProjectSearchEntriesInput, ThreadId, TrimmedNonEmptyString,
};

use crate::chat::{QuickSearchContent, QuickSearchOpen};

use super::preview::{self, PreviewFile, PreviewKind};
use super::rank::{match_line_segments, name_segments, split_search_result_path};
use super::relative_time;

/// Electron's `QUERY_DEBOUNCE_MS`.
const QUERY_DEBOUNCE: Duration = Duration::from_millis(200);
/// Electron's `THREAD_RESULT_LIMIT`.
const THREAD_RESULT_LIMIT: usize = 8;
/// Electron's `COMPOSER_PATH_SEARCH_LIMIT` — asked for. The server truncates
/// files and directories *together* at this limit, so asking for the display
/// count and then dropping directories would return far fewer files than it
/// should; Electron over-fetches for the same reason.
const FILE_NAME_SEARCH_LIMIT: i64 = 80;
/// Electron's `FILE_NAME_RESULT_LIMIT` — shown, after directories are dropped.
const FILE_NAME_RESULT_LIMIT: usize = 15;
/// Electron's `FILE_CONTENT_RESULT_LIMIT` — asked for.
const FILE_CONTENT_RESULT_LIMIT: i64 = 100;
/// Electron's `FILE_CONTENT_DISPLAY_LIMIT` — shown.
const FILE_CONTENT_DISPLAY_LIMIT: usize = 30;
/// Electron's `MESSAGE_RESULT_LIMIT`.
const MESSAGE_RESULT_LIMIT: i64 = 15;
/// Content search does not fire below this length (Electron: `>= 2`).
const CONTENT_MIN_QUERY: usize = 2;
/// Height of the dialog body — Electron's `h-[26rem]` less the query row.
/// Both the list and the preview pane are measured against it.
const BODY_HEIGHT: gpui::Pixels = px(360.);

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
    /// Carries the whole shell: the row shows title and time, the preview card
    /// also wants the project, branch and last-message stamp.
    Thread(Box<OrchestrationThreadShell>),
    File {
        path: String,
    },
    FileMatch(ProjectSearchContentMatch),
    /// Likewise carried whole — the preview card adds role and timestamp to
    /// the row's title and snippet.
    Message(Box<OrchestrationMessageSearchMatch>),
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
    /// Projects, only so a thread's preview card can name its project.
    projects: Vec<OrchestrationProjectShell>,
    /// The preview's editor, reused across files rather than rebuilt, so
    /// arrowing through matches in one file does not re-lay it out.
    preview_editor: Entity<EditorState>,
    /// The file the editor currently holds, and how its read went.
    preview_path: Option<String>,
    preview_state: PreviewFile,
    /// Bumped per read so a slow answer for a row the user has already left
    /// cannot overwrite a newer one.
    preview_generation: usize,
    _preview: Option<Task<()>>,
    /// A reveal waiting for the editor to lay out — `line_height` is unknown
    /// until then, so centring has to wait a frame.
    pending_centre: Option<u32>,
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
        projects: Vec<OrchestrationProjectShell>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let search = cx.new(|cx| {
            let state = cx.new(|cx| CommandState::new(window, cx));
            // Line numbers are not just chrome here: the editor paints its
            // active-line background only when the gutter is on, and that
            // background is what marks the revealed line.
            let preview_editor = cx.new(|cx| {
                EditorState::new(window, cx)
                    .line_number(true)
                    .soft_wrap(false)
            });
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
                projects,
                preview_editor,
                preview_path: None,
                preview_state: PreviewFile::Loading,
                preview_generation: 0,
                _preview: None,
                pending_centre: None,
            };
            // An empty query is not an empty result set in Open mode: it lists
            // recent threads, exactly as Electron's `rankThreads` does.
            this.run_query(String::new(), false, window, cx);
            this
        });

        let dialog_owner = search.downgrade();
        let close_owner = search.downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let content_owner = dialog_owner.clone();
            let close_owner = close_owner.clone();
            // Electron: `pt-[12vh]`, `h-[26rem]`, and a max width that grows
            // with the preview — `max-w-xl` bare, `max-w-2xl` for a card,
            // `max-w-3xl` for a file. The builder re-runs every frame, so the
            // width tracks the highlighted row.
            let width = content_owner
                .upgrade()
                .map(|search| search.read(cx).dialog_width(cx))
                .unwrap_or(px(576.));
            dialog
                .close_button(false)
                .p_0()
                .margin_top(px(96.))
                .w(width)
                .max_w(width)
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
    pub fn set_mode(&mut self, mode: QuickSearchMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        let query = self.state.read(cx).query(cx).to_string();
        self.run_query(query, false, window, cx);
        cx.notify();
    }

    pub fn mode(&self) -> QuickSearchMode {
        self.mode
    }

    /// Debounce, then fetch. `debounce` is false for the seeding query and for
    /// a mode switch, where the delay would only add lag.
    fn run_query(
        &mut self,
        query: String,
        debounce: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        self._search = Some(cx.spawn_in(window, async move |this, cx| {
            if debounce {
                cx.background_executor().timer(QUERY_DEBOUNCE).await;
            }
            let (groups, error) = fetch(mode, &query, client, cwd, &threads).await;
            _ = this.update_in(cx, |this, window, cx| {
                this.settled_query = query;
                this.groups = groups;
                this.error = error;
                this._search = None;
                this.set_loading(false, cx);
                // A new corpus under an unchanged highlight is a new *row*,
                // which the command's own selection callback cannot see — it
                // compares index paths, and (0, 0) before is (0, 0) after.
                // Re-point the preview here; the callback still covers the
                // case where the old path no longer exists and the highlight
                // is reset.
                this.sync_preview(this.pending_index(cx), window, cx);
                cx.notify();
            });
        }));
    }

    fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.loading = loading;
        cx.notify();
    }

    /// The row `index` points at, resolved against the un-filtered corpus.
    ///
    /// The index is a parameter rather than something this reads off
    /// [`Self::state`] because the preview slot renders *inside*
    /// `CommandState`, which is leased for the duration; reading it there
    /// panics. The slot is handed the state it is rendering for, so it passes
    /// the index down instead.
    fn item_at(&self, index: Option<IndexPath>) -> Option<&QuickItem> {
        let index = index?;
        self.groups
            .get(index.section)
            .and_then(|group| group.items.get(index.row))
    }

    /// The row the command will highlight once it has the corpus that was
    /// just installed.
    ///
    /// `CommandState` only learns about new items when it renders, so its own
    /// selection is a frame behind a finished query. It keeps the old index
    /// path when the new corpus still has that row and falls back to the first
    /// one otherwise; resolving the same rule here is what lets the preview
    /// move with the results instead of a frame later — or, when the path
    /// happens to be unchanged, at all.
    fn pending_index(&self, cx: &App) -> Option<IndexPath> {
        let current = self.state.read(cx).selected_index();
        if current.is_some_and(|index| self.item_at(Some(index)).is_some()) {
            return current;
        }
        (!self.groups.is_empty()).then(IndexPath::default)
    }

    /// Which shape the pane takes for the highlighted row (Electron's
    /// `previewKind`). File rows fall back to a card carrying the
    /// "open a chat" copy when there is no root to read them from.
    fn preview_kind(&self, index: Option<IndexPath>) -> PreviewKind {
        match self.item_at(index) {
            None => PreviewKind::None,
            Some(QuickItem::Thread(_) | QuickItem::Message(_)) => PreviewKind::Card,
            Some(QuickItem::File { .. } | QuickItem::FileMatch(_)) => {
                if self.client.is_some() && self.cwd.is_some() {
                    PreviewKind::File
                } else {
                    PreviewKind::Card
                }
            }
        }
    }

    /// Point the preview at the highlighted row.
    ///
    /// Only file rows do any work: the cards render straight from the item.
    /// Re-selecting a row in the file already loaded just re-reveals, which is
    /// what makes arrowing through several matches in one file cheap.
    fn sync_preview(
        &mut self,
        index: Option<IndexPath>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((path, line)) = self.item_at(index).and_then(|item| match item {
            QuickItem::File { path } => Some((path.clone(), None)),
            QuickItem::FileMatch(hit) => Some((hit.path.0.clone(), Some(hit.line.0 as u32))),
            _ => None,
        }) else {
            return;
        };
        let (Some(client), Some(cwd)) = (self.client.clone(), self.cwd.clone()) else {
            return;
        };

        if self.preview_path.as_deref() == Some(path.as_str())
            && matches!(self.preview_state, PreviewFile::Ready)
        {
            self.reveal(line, cx);
            return;
        }

        self.preview_generation += 1;
        let generation = self.preview_generation;
        self.preview_path = Some(path.clone());
        self.preview_state = PreviewFile::Loading;
        cx.notify();

        let payload = ProjectReadFileInput {
            cwd: tnes(cwd),
            relative_path: tnes(path.clone()),
        };
        self._preview = Some(cx.spawn_in(window, async move |this, cx| {
            let result = client.call::<ProjectsReadFile>(&payload).await;
            _ = this.update_in(cx, |this, window, cx| {
                // A newer row won the race; its own read owns the editor now.
                if this.preview_generation != generation {
                    return;
                }
                match result {
                    Ok(file) => {
                        let language = crate::files::language_for_path(&path);
                        this.preview_editor.update(cx, |state, cx| {
                            state.set_highlighter(language, cx);
                        });
                        this.preview_editor.update(cx, |state, cx| {
                            state.replace_all(file.contents.0.to_string(), window, cx);
                        });
                        this.preview_state = PreviewFile::Ready;
                        this.reveal(line, cx);
                    }
                    Err(error) => {
                        this.preview_state = PreviewFile::Error(error.user_message().into());
                    }
                }
                cx.notify();
            });
        }));
    }

    /// Put the cursor on a one-based line so the editor scrolls it in and
    /// paints its active-line background, then ask for the centring pass.
    ///
    /// A row with no line (a name match) still has to clear the previous
    /// row's highlight, which is what moving to the top does.
    fn reveal(&mut self, line: Option<u32>, cx: &mut Context<Self>) {
        let row = line.map(|line| line.saturating_sub(1)).unwrap_or(0);
        self.preview_editor.update(cx, |state, cx| {
            state.move_cursor_to_position(lsp_types::Position::new(row, 0), cx);
        });
        self.pending_centre = line;
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
            QuickItem::Thread(thread) => QuickSearchEvent::OpenThread(thread.id.clone()),
            QuickItem::Message(hit) => QuickSearchEvent::OpenThread(hit.thread_id.clone()),
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
        self.dismiss(window, cx);
    }

    /// Close the overlay and tell the shell we are gone.
    ///
    /// `close_dialog` only pops the dialog off `Root`; it never runs the
    /// dialog's own `on_close`, which is invoked solely by the Dialog
    /// element's Escape/confirm/backdrop handlers. So every close path that
    /// goes through `close_dialog` has to emit `Dismissed` itself, or the
    /// shell keeps holding this entity and refuses to open a fresh one.
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.close_dialog(cx);
        cx.emit(QuickSearchEvent::Dismissed);
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

        // Centring needs a laid-out editor to know its line height, so the
        // reveal asks for it and the next render that can measure delivers it.
        if let Some(line) = self.pending_centre {
            let measured = self.preview_editor.read(cx).line_height().is_some();
            if measured {
                preview::centre_on_line(&self.preview_editor, line, BODY_HEIGHT, cx);
                self.pending_centre = None;
            }
        }

        let query_owner = cx.weak_entity();
        let confirm_owner = cx.weak_entity();
        let open_owner = cx.weak_entity();
        let content_owner = cx.weak_entity();
        let select_owner = cx.weak_entity();
        let side_owner = cx.weak_entity();

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
            .min_h(BODY_HEIGHT)
            .max_h(BODY_HEIGHT)
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
            .on_query(move |query, window, cx| {
                let query = query.to_string();
                _ = query_owner.update(cx, |this, cx| this.run_query(query, true, window, cx));
            })
            .on_confirm(move |index, window, cx| {
                _ = confirm_owner.update(cx, |this, cx| this.confirm(index, window, cx));
            })
            .on_select(move |index, window, cx| {
                _ = select_owner.update(cx, |this, cx| this.sync_preview(Some(index), window, cx));
            })
            .side(move |state, _, cx| {
                let index = state.selected_index();
                let Some(search) = side_owner.upgrade() else {
                    return preview::pane(PreviewKind::None, None, cx);
                };
                let (kind, body) = search.update(cx, |search, cx| {
                    (search.preview_kind(index), search.render_preview(index, cx))
                });
                preview::pane(kind, body, cx)
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

        // A deferred draw keeps its parent dispatch node, so these sit *below*
        // the shell's handlers for the same actions and shadow them on the way
        // up. That is what gives Electron's toggle semantics: the showing
        // mode's shortcut closes, the other one switches corpus.
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

    /// The pane's contents for the highlighted row.
    fn render_preview(
        &mut self,
        index: Option<IndexPath>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let query = self.settled_query.clone();
        match self.item_at(index)? {
            QuickItem::Thread(thread) => {
                let title = self
                    .projects
                    .iter()
                    .find(|project| project.id == thread.project_id)
                    .map(|project| project.title.0.clone());
                Some(preview::thread_card(thread, title.as_deref(), cx))
            }
            QuickItem::Message(hit) => Some(preview::message_card(hit, &query, cx)),
            QuickItem::File { .. } | QuickItem::FileMatch(_) => {
                if self.client.is_none() || self.cwd.is_none() {
                    return Some(preview::unavailable(cx));
                }
                Some(preview::file_view(
                    &self.preview_editor,
                    &self.preview_state,
                    cx,
                ))
            }
        }
    }

    /// The dialog's width, which Electron widens for a preview and widens
    /// again for a file preview.
    pub fn dialog_width(&self, cx: &App) -> gpui::Pixels {
        self.preview_kind(self.state.read(cx).selected_index())
            .dialog_width()
    }

    /// The shortcut pressed while the overlay is already up.
    fn toggle(&mut self, mode: QuickSearchMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == mode {
            self.dismiss(window, cx);
        } else {
            self.set_mode(mode, window, cx);
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
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            _ = owner.update(cx, |search, cx| search.set_mode(mode, window, cx));
        })
}

/// Build the palette row for one result.
///
/// The label is what a screen reader and the (disabled) local filter see; the
/// body is the custom child, since every row kind lays out differently.
fn command_item(item: &QuickItem, query: &str) -> CommandItem {
    let label: SharedString = match item {
        QuickItem::Thread(thread) => thread.title.0.clone().into(),
        QuickItem::File { path } => path.clone().into(),
        QuickItem::FileMatch(hit) => hit.path.0.clone().into(),
        QuickItem::Message(hit) => hit.thread_title.0.clone().into(),
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
        QuickItem::Thread(_) | QuickItem::Message(_) => IconName::MessageSquare,
        QuickItem::File { .. } | QuickItem::FileMatch(_) => IconName::File,
    };
    let body: AnyElement = match item {
        QuickItem::Thread(thread) => h_flex()
            .min_w_0()
            .flex_1()
            .gap_2()
            .items_center()
            .child(highlighted_name(&thread.title.0, query, cx))
            .child(
                div()
                    .ml_auto()
                    .flex_shrink_0()
                    .text_size(px(10.))
                    .text_color(muted)
                    .child(relative_time(&thread.updated_at.0).unwrap_or_default()),
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
        QuickItem::Message(hit) => v_flex()
            .min_w_0()
            .flex_1()
            .child(
                div()
                    .truncate()
                    .font_medium()
                    .child(hit.thread_title.0.clone()),
            )
            .child(
                div()
                    .truncate()
                    .text_color(muted)
                    .child(hit.snippet.0.clone()),
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
                        limit: FILE_NAME_SEARCH_LIMIT,
                        query: tnes(query),
                    };
                    match client.call::<ProjectsSearchEntries>(&payload).await {
                        Ok(result) => (
                            result
                                .entries
                                .into_iter()
                                .filter(|entry| entry.kind == ProjectEntryKind::File)
                                .take(FILE_NAME_RESULT_LIMIT)
                                .map(|entry| QuickItem::File { path: entry.path.0 })
                                .collect(),
                            None,
                        ),
                        Err(error) => (Vec::new(), Some(error.user_message().into())),
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
                Ok(result) => (
                    result
                        .matches
                        .into_iter()
                        .map(|hit| QuickItem::Message(Box::new(hit)))
                        .collect(),
                    None,
                ),
                Err(error) => (Vec::new(), Some(SharedString::from(error.user_message()))),
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
                        Err(error) => (Vec::new(), Some(SharedString::from(error.user_message()))),
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
        .map(|thread| QuickItem::Thread(Box::new(thread.clone())))
        .collect()
}
