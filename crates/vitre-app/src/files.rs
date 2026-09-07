//! M2 files panel: workspace file tree + editor, the Electron
//! `FilePreviewPanel` layout (editor area left, explorer aside right; the
//! aside fills the panel while no file is open).
//!
//! Data flow per the Electron sources: the tree is a flat
//! `projects.listEntries` snapshot (manual refresh only — the watcher does
//! NOT refresh entries, matching `FileBrowserPanel`), VCS decorations refresh
//! on every workspace-watch event, the open file re-reads on watch events
//! that name it (clean buffer → reload, dirty buffer → conflict banner), and
//! saves are debounced writes guarded by `baseRevision` with the
//! `stale_revision` failure surfacing the same banner.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable as _, MouseButton, PromptLevel,
    ScrollHandle, SharedString, Subscription, WeakEntity, Window, actions, div, prelude::*, px,
    rgb,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{
        BlockCursor, BlockOverlay, DefinitionProvider as _, Editor, EditorState, Escape,
        HoverProvider as _, Input, InputEvent, InputState, LineHighlight, Redo, RopeExt as _, Undo,
    },
    menu::{ContextMenuExt as _, PopupMenuItem},
    v_flex,
};
use vitre_client::EnvironmentClient;
use vitre_contracts::methods::{
    LspFormat, LspSubscribeDiagnostics, ProjectsListEntries, ProjectsMutateEntry, ProjectsReadFile,
    ProjectsSubscribeWorkspaceChanges, ProjectsWriteFile,
    ProjectsWriteFileError as WriteFileErrorUnion, VcsGetFileBaseline, VcsGetFileStatuses,
};
use vitre_contracts::{
    LspDiagnostic, LspDiagnosticsStreamEvent, LspFormattingInput, LspSubscribeDiagnosticsInput,
    ProjectFileFailure, ProjectListEntriesInput, ProjectMutateEntryInput,
    ProjectMutateEntryInputCreateKind, ProjectReadFileInput, ProjectWatchInput,
    ProjectWatchStreamEvent, ProjectWriteFileInput, TrimmedNonEmptyString, VcsFileBaselineInput,
    VcsFileStatusEntry, VcsFileStatusesInput,
};
use vitre_rpc::{RpcError, TypedError, TypedStreamEvent};
use vitre_state::file_buffer::{BufferConflict, DiskChange, FileBuffer};
use vitre_state::file_tree::{FileTreeModel, FileTreeRow};
use vitre_state::vcs_tree_status::{TreeVcsDecorations, TreeVcsStatus, build_tree_vcs_decorations};
use vitre_state::vim::{VimDocument, VimEffect};

use crate::client_settings::ClientSettings;
use crate::git_gutter::{GitGutterState, RECOMPUTE_DEBOUNCE};
use crate::lsp::bridge::{self, LspBridge};
use crate::lsp::positions::{self as positions, WirePosition, wire_to_offset};
use crate::vim::{self, ToggleVimMode, VimKeystroke, VimSession};

/// A server-completed watch stream must not resubscribe in a hot loop
/// (vitre-client's `RESUBSCRIBE_AFTER_COMPLETION`).
const RESUBSCRIBE_AFTER_COMPLETION: Duration = Duration::from_secs(2);

/// codemirror-vim's `.cm-fat-cursor` default, which is what Electron paints:
/// T3 never overrides it (`apps/web/src/components/files/codemirror/theme.ts`
/// restyles `.cm-cursor` only). Deliberately not a theme token — a modal caret
/// that borrows the foreground or selection colour is exactly the caret that
/// disappears into a visual selection.
const BLOCK_CURSOR_COLOR: u32 = 0xff9696;

/// codemirror-vim's `hCoeff` for a half-typed command (`vim.status` non-empty).
const PENDING_BLOCK_CURSOR_HEIGHT: f32 = 0.5;

/// `MAX_REVEALED_LINES` (`revealLine.ts`): highlighting an enormous range
/// would only bury the start the user jumped to.
const MAX_REVEALED_LINES: usize = 500;

actions!(
    vitre,
    [
        /// Write the open file now, ahead of the autosave debounce. The
        /// Electron `file.save` command (`mod+s`), which likewise routes to
        /// the one mounted save coordinator.
        SaveFile,
        /// Format the open file through the language server. Electron binds
        /// this on the editor itself (`Shift-Alt-f` in `useLspBridge.ts`), not
        /// as a rebindable command.
        FormatDocument,
        /// Move the caret to the next git hunk, wrapping at the end of the
        /// file (Electron's `Alt-F5`).
        GotoNextHunk,
        /// Move the caret to the previous git hunk, wrapping at the start of
        /// the file (Electron's `Shift-Alt-F5`).
        GotoPreviousHunk,
        /// Tree roving focus: down one visible row (`ArrowDown`, vim `j`).
        TreeFocusNext,
        /// Tree roving focus: up one visible row (`ArrowUp`, vim `k`).
        TreeFocusPrevious,
        /// Tree roving focus: the first visible row (`Home`, vim `gg`).
        TreeFocusFirst,
        /// Tree roving focus: the last visible row (`End`, vim `G`).
        TreeFocusLast,
        /// Collapse an expanded directory, else move to the parent row
        /// (`ArrowLeft`, vim `h`).
        TreeCollapseOrParent,
        /// Expand a collapsed directory, else move down one row
        /// (`ArrowRight`, vim `l`).
        TreeExpandOrNext,
        /// Open the focused file, or fold/unfold the focused directory —
        /// what Electron gets for free from rows being `<button>`s.
        TreeActivate,
        /// Jump to the definition of the symbol under the caret. Electron
        /// binds `F12` on the editor itself (`lspBridge.ts`), next to
        /// mod-click and vim `gd`; all three run the same jump.
        GoToDefinition,
        /// Open the completion menu at the caret — Electron's
        /// `editor.showCompletions` command, `mod+i` when `editorFocus`.
        ShowCompletions,
    ]
);

fn tnes(text: impl Into<String>) -> TrimmedNonEmptyString {
    TrimmedNonEmptyString(text.into())
}

/// Highlighter language for a path: the gpui-component registry resolves
/// extensions and short names itself ("rs" → rust), so pass the extension and
/// fall back to plain text.
pub(crate) fn language_for_path(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => extension.to_ascii_lowercase(),
        _ => match name.to_ascii_lowercase().as_str() {
            "makefile" => "make".into(),
            _ => "text".into(),
        },
    }
}

/// Where a reveal should land: a position, plus the last line of the range
/// when the request named one.
///
/// Electron's `revealLine`/`revealEndLine` pair (`rightPanelStore.ts`), which
/// `revealEditorLine` turns into a selection over the range, a highlight on
/// every line of it, and a centred scroll to the first.
#[derive(Clone, Copy)]
pub struct RevealTarget {
    pub position: WirePosition,
    /// Zero-based last line of the range.
    pub end_line: Option<u32>,
}

impl RevealTarget {
    /// A reveal that names a single position.
    pub fn at(position: WirePosition) -> Self {
        Self {
            position,
            end_line: None,
        }
    }
}

/// A file the panel asked for and could not get.
///
/// Electron keeps the surface: the breadcrumb, the right-panel tab and the
/// tree aside all render off the requested path, and only the content area is
/// replaced by the server's message (`FilePreviewPanel.tsx`'s error branch).
/// Vitre used to leave the *previous* file on screen and put a banner above
/// it, so the error read as if it were about the file you were looking at.
struct OpenError {
    relative_path: String,
    message: SharedString,
}

struct OpenFile {
    relative_path: String,
    buffer: FileBuffer,
    /// Server truncated the read (>1MB): shown as a banner, editing disabled.
    truncated: bool,
    /// Debounce generation: each edit bumps it; only the latest timer saves.
    debounce: u64,
}

/// Where an in-tree inline edit commits to.
#[derive(Clone)]
enum TreeEditTarget {
    /// New entry under `parent` ("" = workspace root).
    Create {
        parent: String,
        kind: ProjectMutateEntryInputCreateKind,
    },
    Rename {
        path: String,
    },
}

/// Inline name editor rendered inside the tree (create placeholder row or a
/// row's name swapped for an input), the Electron `startRenaming` flow.
struct TreeEdit {
    target: TreeEditTarget,
    input: Entity<InputState>,
    _subscription: Subscription,
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("")
}

/// The `.cm-reveal-line` background (`theme.ts` `REVEAL_LINE_BACKGROUND`):
/// `--primary` at 10% in light, 16% in dark.
fn reveal_line_color(cx: &App) -> gpui::Hsla {
    let theme = cx.theme();
    theme
        .primary
        .opacity(if theme.is_dark() { 0.16 } else { 0.10 })
}

fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

pub struct FilesPanel {
    client: Arc<EnvironmentClient>,
    cwd: String,
    tree: Option<FileTreeModel>,
    tree_truncated: bool,
    /// Every path listEntries reported (files + dirs) — the "paths the tree
    /// knows" input to VCS untracked-directory expansion.
    tree_paths: Vec<String>,
    expanded: HashSet<String>,
    /// Keyboard focus for the tree, so arrows and the vim motions have
    /// somewhere to land (Electron: DOM focus on the row `<button>`).
    tree_focus: FocusHandle,
    /// The roving-focus row, by path. Electron's tree keeps this as its
    /// "focused item" independently of which file is open.
    tree_focused: Option<String>,
    /// Tracks the tree's scroll so a keyboard move can bring its row back
    /// into view.
    tree_scroll: ScrollHandle,
    vcs_entries: Vec<VcsFileStatusEntry>,
    vcs: TreeVcsDecorations,
    open: Option<OpenFile>,
    edit: Option<TreeEdit>,
    editor: Entity<EditorState>,
    lsp: Rc<LspBridge>,
    /// Modal editing state, present only while the `vimMode` preference is
    /// on. `None` is the default, matching Electron.
    vim: Option<VimSession>,
    /// Git diff gutter for the open file: baseline, hunks, open peek.
    git: GitGutterState,
    /// Latest diagnostics per relative path (latest event wins, empty
    /// clears — the Electron per-file replacement semantics).
    diagnostics: HashMap<String, Vec<LspDiagnostic>>,
    /// The last open that failed, kept until another open supersedes it.
    /// Mutually exclusive with [`Self::open`].
    open_error: Option<OpenError>,
    /// `wordWrap`, and the value the editor was last given — the setter needs
    /// a `Window`, which only the frame has.
    word_wrap: bool,
    applied_word_wrap: bool,
    /// Cross-file go-to-definition target, applied once that file loads.
    pending_reveal: Option<(String, RevealTarget)>,
    /// The reveal the tree has already followed, as (generation, path).
    /// Electron keys the same effect on `revealRequestId` so a background
    /// entry refresh never yanks the scroll back to the open file.
    tree_revealed: Option<(u64, String)>,
    /// Bumped by every open and every explicit reveal — the counter
    /// `tree_revealed` is keyed on.
    reveal_generation: u64,
    /// Whether the tree aside is showing. Electron reads
    /// `t3code.fileExplorerOpen` at mount and defaults it on.
    explorer_open: bool,
    /// Bumped on every open/close; async completions for an older file drop.
    open_generation: u64,
    /// Bumped per listing request; completions for an older request drop.
    list_generation: u64,
    /// True while a listing request is running (spares re-entrant
    /// revalidates from activation edges).
    listing_in_flight: bool,
    /// When the last successful listing landed — the SWR staleness input
    /// (Electron: the listEntries atom's `staleTimeMs: 30_000`).
    listed_at: Option<std::time::Instant>,
    status: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl FilesPanel {
    pub fn new(
        client: Arc<EnvironmentClient>,
        cwd: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| EditorState::new(window, cx).line_number(true));
        let subscriptions = vec![
            cx.subscribe_in(
                &editor,
                window,
                |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => this.editor_edited(window, cx),
                    InputEvent::DiffGutterClick { line } => {
                        this.toggle_hunk_peek(*line, cx);
                    }
                    _ => {}
                },
            ),
            cx.observe_global::<ClientSettings>(|this: &mut Self, cx| {
                this.sync_client_settings(cx)
            }),
        ];

        let lsp = LspBridge::new(client.clone(), cwd.clone(), editor.downgrade());
        let panel_for_show: WeakEntity<Self> = cx.weak_entity();
        let show_lsp = lsp.clone();
        let show_cwd = cwd.clone();
        editor.update(cx, |state, _| {
            let editor_lsp = state.lsp_mut();
            editor_lsp.completion_provider = Some(lsp.clone());
            editor_lsp.hover_provider = Some(lsp.clone());
            editor_lsp.definition_provider = Some(lsp.clone());
            // Cross-file go-to-definition: locations carry WIRE (UTF-16)
            // positions (see lsp::bridge module docs); converted against the
            // target file after it loads. Same-document targets fall through
            // to the editor's built-in jump; out-of-workspace targets are
            // swallowed (Electron drops them).
            editor_lsp.show_document = Some(Rc::new(move |params, window, cx| {
                let Some(target) = bridge::relative_path_from_uri(&show_cwd, &params.uri) else {
                    return true;
                };
                if show_lsp.current_document().as_deref() == Some(target.as_str()) {
                    return false;
                }
                let Some(panel) = panel_for_show.upgrade() else {
                    return true;
                };
                let reveal = params.selection.map(|range| {
                    RevealTarget::at(WirePosition {
                        line: range.start.line,
                        character: range.start.character,
                    })
                });
                panel.update(cx, |panel, cx| {
                    panel.pending_reveal =
                        reveal.map(|target_position| (target.clone(), target_position));
                    panel.open_file(target.clone(), window, cx);
                });
                true
            }));
        });
        lsp.refresh_server_status(cx);

        let mut panel = Self {
            client,
            cwd,
            tree: None,
            tree_truncated: false,
            tree_paths: Vec::new(),
            expanded: HashSet::new(),
            tree_focus: cx.focus_handle(),
            tree_focused: None,
            tree_scroll: ScrollHandle::new(),
            tree_revealed: None,
            reveal_generation: 0,
            explorer_open: ClientSettings::file_explorer_open(cx),
            git: GitGutterState::default(),
            vcs_entries: Vec::new(),
            vcs: TreeVcsDecorations::default(),
            open: None,
            edit: None,
            editor,
            lsp,
            vim: ClientSettings::vim_mode(cx).then(VimSession::new),
            diagnostics: HashMap::new(),
            open_error: None,
            word_wrap: ClientSettings::word_wrap(cx),
            applied_word_wrap: true,
            pending_reveal: None,
            open_generation: 0,
            list_generation: 0,
            listing_in_flight: false,
            listed_at: None,
            status: None,
            _subscriptions: subscriptions,
        };
        panel.refresh_tree(cx);
        panel.spawn_watch_loop(window, cx);
        panel.spawn_diagnostics_loop(cx);
        panel
    }

    /// Which workspace this panel browses (ChatApp recreates on change).
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Re-list entries; on completion also refresh VCS so untracked-directory
    /// expansion sees the tree paths.
    ///
    /// Electron gates the listing on the environment being connected (the
    /// query atom is `Effect.never` until the supervisor phase is
    /// "connected", and re-executes on a new connection generation). A fetch
    /// issued before the session exists therefore WAITS for it — without
    /// this, a Files surface restored at app boot failed instantly with
    /// `ConnectionClosed` and sat on "Loading files…" until a manual refresh.
    fn refresh_tree(&mut self, cx: &mut Context<Self>) {
        self.list_generation += 1;
        self.listing_in_flight = true;
        let generation = self.list_generation;
        let client = self.client.clone();
        let payload = ProjectListEntriesInput {
            cwd: tnes(&self.cwd),
        };
        cx.spawn(async move |this, cx| {
            let mut sessions = client.sessions();
            loop {
                if sessions
                    .wait_for(|session| session.is_some())
                    .await
                    .is_err()
                {
                    return;
                }
                let outcome = client.call::<ProjectsListEntries>(&payload).await;
                let stale = this
                    .read_with(cx, |panel, _| panel.list_generation != generation)
                    .unwrap_or(true);
                if stale {
                    return;
                }
                match outcome {
                    Ok(result) => {
                        let _ = this.update(cx, |panel, cx| {
                            panel.listing_in_flight = false;
                            panel.listed_at = Some(std::time::Instant::now());
                            if panel
                                .status
                                .as_deref()
                                .is_some_and(|status| status.starts_with("listEntries failed"))
                            {
                                panel.status = None;
                            }
                            panel.tree_paths = result
                                .entries
                                .iter()
                                .map(|entry| entry.path.0.clone())
                                .collect();
                            panel.tree = Some(FileTreeModel::build(&result.entries));
                            panel.tree_truncated = result.truncated;
                            panel.rebuild_decorations();
                            panel.refresh_vcs(cx);
                            // A reveal that arrived before the listing did
                            // gets its retry here, as Electron's effect does
                            // when `treePaths` changes.
                            panel.follow_open_file_in_tree(cx);
                            cx.notify();
                        });
                        return;
                    }
                    // The session died between the gate and the call — wait
                    // for the next connection and retry, like Electron's
                    // per-connection-generation query re-execution.
                    Err(TypedError::Rpc(
                        RpcError::ConnectionClosed | RpcError::Ws(_) | RpcError::Transport(_),
                    )) => {
                        if sessions.changed().await.is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = this.update(cx, |panel, cx| {
                            panel.listing_in_flight = false;
                            panel.status = Some(
                                format!("listEntries failed: {}", error.user_message()).into(),
                            );
                            cx.notify();
                        });
                        return;
                    }
                }
            }
        })
        .detach();
    }

    /// Refetch the listing when it is missing or older than 30 seconds,
    /// keeping the current tree rendered meanwhile — the Electron SWR
    /// semantics (`staleTime: 30_000`, `revalidateOnMount: true`) applied on
    /// surface activation instead of React remount.
    pub fn revalidate_if_stale(&mut self, cx: &mut Context<Self>) {
        const TREE_STALE_AFTER: Duration = Duration::from_secs(30);
        if self.listing_in_flight {
            return;
        }
        if self
            .listed_at
            .is_none_or(|at| at.elapsed() >= TREE_STALE_AFTER)
        {
            self.refresh_tree(cx);
        }
    }

    fn refresh_vcs(&mut self, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let payload = VcsFileStatusesInput {
            cwd: tnes(&self.cwd),
        };
        cx.spawn(async move |this, cx| {
            // No repository / transient failures leave decorations as-is
            // (Electron's status atom behaves the same on error).
            let Ok(result) = client.call::<VcsGetFileStatuses>(&payload).await else {
                return;
            };
            let _ = this.update(cx, |panel, cx| {
                panel.vcs_entries = result.entries;
                panel.rebuild_decorations();
                // Every status emission is a chance the open file's HEAD or
                // index blob moved: this is the git gutter's refresh trigger.
                panel.refresh_baseline(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn rebuild_decorations(&mut self) {
        self.vcs = build_tree_vcs_decorations(&self.vcs_entries, &self.tree_paths);
    }

    // ---- git diff gutter --------------------------------------------------

    /// Fetch the HEAD + index baselines for the open file.
    ///
    /// Electron refreshes these on every workspace-watch event naming the
    /// file *and* on every VCS status emission — commit, branch switch,
    /// discard, stage/unstage. Both funnel through `refresh_vcs` here. The
    /// status stream carries no HEAD sha, so the refresh is unconditional and
    /// the oid key inside [`GitGutterState`] is what keeps it cheap.
    fn refresh_baseline(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        let generation = self.open_generation;
        let client = self.client.clone();
        let payload = VcsFileBaselineInput {
            cwd: tnes(&self.cwd),
            relative_path: tnes(&open.relative_path),
        };
        cx.spawn(async move |this, cx| {
            // No repository, or a transient failure, leaves the gutter exactly
            // as it is — the way Electron's baseline atom does on error.
            let Ok(result) = client.call::<VcsGetFileBaseline>(&payload).await else {
                return;
            };
            let _ = this.update(cx, |panel, cx| {
                if panel.open_generation != generation {
                    return;
                }
                if !panel.git.set_baseline(Some(&result)) {
                    return;
                }
                panel.recompute_gutter(cx);
            });
        })
        .detach();
    }

    /// Re-diff the buffer against the baseline and push the result at the
    /// editor.
    fn recompute_gutter(&mut self, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text().to_string();
        self.git.recompute(&text);
        self.apply_gutter(cx);
    }

    /// Push the current markers and peek at the editor.
    ///
    /// The colours are Electron's: `--success` for added lines, `--warning`
    /// for modified, `--destructive` for the deletion wedges.
    fn apply_gutter(&mut self, cx: &mut Context<Self>) {
        let gutter = self
            .git
            .gutter(cx.theme().success, cx.theme().warning, cx.theme().danger);
        let overlay = self.hunk_peek_overlay(cx);
        self.editor.update(cx, |state, cx| {
            state.set_diff_gutter(gutter, cx);
            state.set_block_overlay(overlay, cx);
        });
    }

    /// Re-diff once the edit storm settles.
    ///
    /// Electron debounces the same 250ms and maps the old markers through the
    /// edits meanwhile; here they simply hold still for that quarter second,
    /// which is the one visible difference.
    fn schedule_gutter_recompute(&mut self, cx: &mut Context<Self>) {
        if !self.git.has_baseline() {
            return;
        }
        let generation = self.open_generation;
        let debounce = self.git.arm_debounce();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RECOMPUTE_DEBOUNCE).await;
            let _ = this.update(cx, |panel, cx| {
                if panel.open_generation != generation || !panel.git.debounce_is_current(debounce) {
                    return;
                }
                panel.recompute_gutter(cx);
            });
        })
        .detach();
    }

    /// Gutter marker click: open that hunk's peek, or close it if it is the
    /// one already open.
    fn toggle_hunk_peek(&mut self, line: usize, cx: &mut Context<Self>) {
        if let Some(index) = self.git.toggle_peek_at_line(line) {
            self.scroll_to_chunk(index, cx);
        }
        self.apply_gutter(cx);
    }

    /// Peek back/forward: move to the adjacent hunk, wrapping at both ends.
    fn step_hunk_peek(&mut self, step: isize, cx: &mut Context<Self>) {
        if let Some(index) = self.git.step_peek(step) {
            self.scroll_to_chunk(index, cx);
        }
        self.apply_gutter(cx);
    }

    fn close_hunk_peek(&mut self, cx: &mut Context<Self>) {
        if self.git.close_peek() {
            self.apply_gutter(cx);
        }
    }

    /// Bring a hunk into view without disturbing the selection — the caret
    /// belongs to the user, not to the peek.
    fn scroll_to_chunk(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(offset) = self.git.chunk_offset(index) else {
            return;
        };
        self.editor.update(cx, |state, cx| {
            state.scroll_to_center(offset, cx);
        });
    }

    /// Peek `Revert`: put the hunk back to its HEAD contents.
    fn revert_hunk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((range, insert)) = self.git.peek_revert_edit() else {
            return;
        };
        self.editor.update(cx, |state, cx| {
            let text = state.text().clone();
            let edit = bridge::editor_offset_edit(&text, range, insert);
            state.apply_lsp_edits(&vec![edit], window, cx);
        });
        self.git.close_peek();
        // `apply_lsp_edits` replaces text silently, so the editor emits no
        // change event: tell the buffer and the LSP document by hand, as the
        // formatting path does.
        self.editor_edited(window, cx);
        // The replacement moved the caret behind vim's back, so the block
        // cursor has to be re-seated on it like every other programmatic edit.
        self.vim_adopt_selection(cx);
        // A revert re-diffs at once rather than waiting out the debounce
        // (Electron schedules the recompute with a 0ms delay here).
        self.recompute_gutter(cx);
    }

    /// `Alt-F5` / `Shift-Alt-F5`: move the caret to the next/previous hunk.
    /// Unlike the peek this *does* move the selection, as Electron does.
    fn goto_hunk(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let caret = self.editor.read(cx).selected_range().start;
        let Some(offset) = self.git.goto_offset(caret, forward) else {
            return;
        };
        self.editor.update(cx, |state, cx| {
            let position = bridge::editor_position(state.text(), offset);
            state.set_cursor_position(position, window, cx);
        });
        self.vim_adopt_selection(cx);
    }

    /// The hunk peek card, anchored under the last line of its hunk.
    fn hunk_peek_overlay(&self, cx: &mut Context<Self>) -> Option<BlockOverlay> {
        let index = self.git.peek_index()?;
        let line = self.git.peek_anchor_line()?;
        let count = self.git.chunk_count();
        let original: SharedString = self.git.peek_original_text()?.into();
        let panel = cx.weak_entity();
        Some(BlockOverlay::new(line, move |_, cx| {
            render_hunk_peek(&panel, index, count, &original, cx)
        }))
    }

    /// Durable workspace watch: resubscribes on every new session, exactly
    /// like the vitre-client domain loops (select on the session channel so a
    /// dead subscription can't wedge the loop).
    fn spawn_watch_loop(&self, window: &mut Window, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let payload = ProjectWatchInput {
            cwd: tnes(&self.cwd),
        };
        cx.spawn_in(window, async move |this, cx| {
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
                    .subscribe_typed::<ProjectsSubscribeWorkspaceChanges>(&payload)
                else {
                    // Published session already dead; wait for a replacement.
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
                                    .update_in(cx, |panel, window, cx| {
                                        panel.workspace_changed(&events, window, cx);
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

    /// Watch events: VCS decorations always refresh; the open file re-reads
    /// when named (or on overflow, where any path may have changed). The tree
    /// itself does NOT refresh (Electron parity: manual refresh only).
    fn workspace_changed(
        &mut self,
        events: &[ProjectWatchStreamEvent],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut open_touched = false;
        for event in events {
            match event {
                ProjectWatchStreamEvent::Changes { paths } => {
                    if let Some(open) = &self.open
                        && paths.iter().any(|path| path.0 == open.relative_path)
                    {
                        open_touched = true;
                    }
                }
                ProjectWatchStreamEvent::Overflow {} => open_touched = self.open.is_some(),
                ProjectWatchStreamEvent::Unknown(_) => {}
            }
        }
        self.refresh_vcs(cx);
        if open_touched {
            self.check_open_file_disk(window, cx);
        }
    }

    /// Durable diagnostics subscription, same session-watch shape as the
    /// workspace loop. The stream is live-only (no snapshot replay), so it
    /// starts at panel creation — before any didOpen can produce events.
    fn spawn_diagnostics_loop(&self, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let payload = LspSubscribeDiagnosticsInput {
            cwd: tnes(&self.cwd),
        };
        cx.spawn(async move |this, cx| {
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
                    .subscribe_typed::<LspSubscribeDiagnostics>(&payload)
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
                                    .update(cx, |panel, cx| {
                                        panel.diagnostics_changed(&events, cx);
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

    /// Latest event per file wins; an empty list clears the file (the
    /// Electron `subscribeDiagnostics` consumer semantics).
    fn diagnostics_changed(
        &mut self,
        events: &[LspDiagnosticsStreamEvent],
        cx: &mut Context<Self>,
    ) {
        let mut open_touched = false;
        for event in events {
            let path = &event.relative_path.0;
            if self
                .open
                .as_ref()
                .is_some_and(|open| open.relative_path == *path)
            {
                open_touched = true;
            }
            if event.diagnostics.is_empty() {
                self.diagnostics.remove(path);
            } else {
                self.diagnostics
                    .insert(path.clone(), event.diagnostics.clone());
            }
        }
        if open_touched {
            self.apply_diagnostics_to_editor(cx);
        }
    }

    /// Full-replacement projection of the open file's stored diagnostics into
    /// the editor, ranges converted against the current text.
    fn apply_diagnostics_to_editor(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        let wire = self
            .diagnostics
            .get(&open.relative_path)
            .cloned()
            .unwrap_or_default();
        self.editor.update(cx, |state, cx| {
            let text = state.text().clone();
            let mapped = bridge::editor_diagnostics(&text, &wire);
            let Some(diagnostics) = state.diagnostics_mut() else {
                return;
            };
            diagnostics.reset(&text);
            diagnostics.extend(mapped);
            cx.notify();
        });
    }

    /// Open `path` and put the cursor on `position`, if given.
    ///
    /// The entry point for anything outside the panel that knows a location —
    /// QuickSearch results, the command palette, a go-to-definition hop. When
    /// the file is already open `open_file` is a no-op, so the reveal has to
    /// be applied here instead of through `pending_reveal`.
    pub fn reveal(
        &mut self,
        path: String,
        target: Option<RevealTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reveal_inner(path, target, true, window, cx);
    }

    /// [`Self::reveal`] without stealing keyboard focus — for render-driven
    /// restores (dock surface sync on relaunch or thread switch), where the
    /// user did not just ask for the file.
    pub fn reveal_unfocused(
        &mut self,
        path: String,
        target: Option<RevealTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reveal_inner(path, target, false, window, cx);
    }

    fn reveal_inner(
        &mut self,
        path: String,
        target: Option<RevealTarget>,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A reveal is an explicit request, so the tree follows it even when
        // the file was already open and nothing else changes.
        self.reveal_generation += 1;
        let already_open = self
            .open
            .as_ref()
            .is_some_and(|open| open.relative_path == path);
        if already_open {
            match target {
                Some(target) => self.apply_reveal(target, window, cx),
                // A reveal with no line clears the previous highlight, the
                // way `revealEditorLine(view, null)` does.
                None => self.clear_reveal_highlight(cx),
            }
            self.follow_open_file_in_tree(cx);
            if focus {
                self.focus_editor(window, cx);
            }
            return;
        }
        self.pending_reveal = target.map(|target| (path.clone(), target));
        self.open_file(path, window, cx);
        if focus {
            self.focus_editor(window, cx);
        }
    }

    /// Select the revealed range, highlight every line of it and centre its
    /// first line — CodeMirror's `revealEditorLine` (`revealLine.ts`).
    fn apply_reveal(&mut self, target: RevealTarget, window: &mut Window, cx: &mut Context<Self>) {
        let color = reveal_line_color(cx);
        let start = self.editor.update(cx, |state, cx| {
            let text = state.text().clone();
            let last_line = positions::last_line(&text);
            let start_line = (target.position.line as usize).min(last_line);
            let end_line = target
                .end_line
                .map_or(start_line, |line| (line as usize).min(last_line))
                .max(start_line);
            let start = wire_to_offset(&text, target.position);
            if end_line > start_line {
                // The selection spans the range but stays anchored at its
                // first line, so typing and vim motions continue from the
                // line the request named rather than from its end.
                state.set_selected_range(start..positions::line_end(&text, end_line), cx);
            } else {
                let position = bridge::editor_position(&text, start);
                state.set_cursor_position(position, window, cx);
            }
            let last_highlighted = end_line.min(start_line + MAX_REVEALED_LINES - 1);
            state.set_line_highlight(
                Some(LineHighlight::new(start_line..last_highlighted + 1, color)),
                cx,
            );
            start
        });
        // Vim hears about the jump before the scroll, not after: adopting the
        // selection re-seats the caret, and every seat scrolls it minimally
        // into view, which would replace the centring with the target sitting
        // against whichever edge it came from.
        self.vim_adopt_selection(cx);
        self.center_on_reveal(start, window, cx);
    }

    /// Centre the reveal target, then centre it again on the next frame.
    ///
    /// The second pass is what makes this reliable: a reveal usually arrives
    /// with the file, and until that frame is laid out the geometry still
    /// describes the buffer that just closed, so the first centre measures
    /// against the wrong wrapped-line count (and before the very first layout
    /// it cannot measure at all).
    fn center_on_reveal(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |state, cx| state.scroll_to_center(offset, cx));
        let panel = cx.weak_entity();
        window.on_next_frame(move |_, cx| {
            let _ = panel.update(cx, |panel, cx| {
                panel
                    .editor
                    .update(cx, |state, cx| state.scroll_to_center(offset, cx));
            });
        });
    }

    fn clear_reveal_highlight(&mut self, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |state, cx| state.set_line_highlight(None, cx));
    }

    /// Reveal the open file in the tree: expand its ancestors, park roving
    /// focus on its row and scroll it into view.
    ///
    /// Electron keys this on `revealRequestId` (so a background entry refresh
    /// never yanks the scroll back to the open file) and skips it outright
    /// when the row is already the tree's own — which is the case whenever
    /// the file was opened from the tree in the first place.
    fn follow_open_file_in_tree(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.open.as_ref().map(|open| open.relative_path.clone()) else {
            return;
        };
        let key = (self.reveal_generation, path.clone());
        if self.tree_revealed.as_ref() == Some(&key) || self.tree.is_none() {
            return;
        }
        if self.tree_focused.as_deref() == Some(path.as_str()) {
            self.tree_revealed = Some(key);
            return;
        }
        // Electron's `expand()` opens the entire ancestor chain, not just one
        // level.
        let mut parent = parent_dir(&path);
        while !parent.is_empty() {
            self.expanded.insert(parent.to_string());
            parent = parent_dir(parent);
        }
        let rows = self.visible_rows();
        // Absent (entries still loading, or a truncated listing): leave the
        // request unrecorded so the next listing retries it.
        let Some(index) = rows.iter().position(|row| row.path == path) else {
            return;
        };
        self.tree_revealed = Some(key);
        // Electron's `scrollToPath` moves the tree's *virtual* focus only —
        // keyboard focus stays wherever it was, usually the editor.
        self.focus_row(&rows, index, cx);
    }

    /// Move keyboard focus into the editor buffer.
    fn focus_editor(&self, window: &mut Window, cx: &mut App) {
        self.editor.read(cx).focus_handle(cx).focus(window, cx);
    }

    fn open_file(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .open
            .as_ref()
            .is_some_and(|open| open.relative_path == path)
        {
            return;
        }
        self.open_generation += 1;
        self.reveal_generation += 1;
        // Whatever failed before is not what we are loading now.
        self.open_error = None;
        let generation = self.open_generation;
        let client = self.client.clone();
        let payload = ProjectReadFileInput {
            cwd: tnes(&self.cwd),
            relative_path: tnes(&path),
        };
        cx.spawn_in(window, async move |this, cx| {
            match client.call::<ProjectsReadFile>(&payload).await {
                Ok(result) => {
                    let _ = this.update_in(cx, |panel, window, cx| {
                        if panel.open_generation != generation {
                            return;
                        }
                        let revision = result.revision.flatten().map(|revision| revision.0);
                        panel.editor.update(cx, |state, cx| {
                            state.set_highlighter(language_for_path(&path), cx);
                            state.set_value(result.contents.0.clone(), window, cx);
                        });
                        // Truncated reads never attach LSP: the server holds
                        // partial text and the buffer is read-only anyway.
                        if result.truncated {
                            panel.lsp.close_document(cx);
                        } else {
                            panel
                                .lsp
                                .open_document(&path, result.contents.0.clone(), cx);
                            panel.lsp.refresh_server_status(cx);
                        }
                        panel.open_error = None;
                        panel.open = Some(OpenFile {
                            relative_path: path.clone(),
                            buffer: FileBuffer::open(revision),
                            truncated: result.truncated,
                            debounce: 0,
                        });
                        panel.apply_diagnostics_to_editor(cx);
                        // The previous file's hunks say nothing about this
                        // one: drop them, hide the column, fetch again.
                        panel.git.clear();
                        panel.apply_gutter(cx);
                        panel.refresh_baseline(cx);
                        // A new buffer starts in normal mode with a fresh
                        // caret, as vim does when it opens a file.
                        if let Some(vim) = panel.vim.as_mut() {
                            vim.reset();
                        }
                        // Cross-file definition target: convert the WIRE
                        // position against the loaded text and move there.
                        // A plain open carries none, and clears the previous
                        // file's reveal highlight.
                        match panel.pending_reveal.take() {
                            Some((target, reveal)) if target == path => {
                                panel.apply_reveal(reveal, window, cx);
                            }
                            _ => {
                                panel.clear_reveal_highlight(cx);
                                // Fresh buffer, caret at the top: seat the
                                // block cursor on it. The reveal branch does
                                // its own adoption, and re-running it here
                                // would undo that centring.
                                panel.vim_adopt_selection(cx);
                            }
                        }
                        panel.follow_open_file_in_tree(cx);
                        panel.status = None;
                        cx.notify();
                    });
                }
                Err(error) => {
                    let _ = this.update_in(cx, |panel, window, cx| {
                        if panel.open_generation != generation {
                            return;
                        }
                        panel.fail_open(path, error.user_message().into(), window, cx);
                    });
                }
            }
        })
        .detach();
    }

    /// Show `message` in place of the buffer, attributed to `path`.
    ///
    /// The previous file goes with it — clicking a file in Electron swaps the
    /// surface's path immediately, so its content is gone whether the new read
    /// succeeds or not, and keeping it here is what let an error about one file
    /// paint over another.
    fn fail_open(
        &mut self,
        path: String,
        message: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open = None;
        self.pending_reveal = None;
        self.lsp.close_document(cx);
        self.git.clear();
        self.apply_gutter(cx);
        self.clear_reveal_highlight(cx);
        self.editor
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.open_error = Some(OpenError {
            relative_path: path,
            message,
        });
        cx.notify();
    }

    fn close_file(&mut self, cx: &mut Context<Self>) {
        self.open_generation += 1;
        self.open = None;
        self.open_error = None;
        self.lsp.close_document(cx);
        self.git.clear();
        self.apply_gutter(cx);
        cx.notify();
    }

    fn editor_edited(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(vim) = self.vim.as_mut() {
            vim.invalidate();
        }
        self.close_hunk_peek(cx);
        self.schedule_gutter_recompute(cx);
        self.lsp.document_edited(cx);
        let generation = self.open_generation;
        let Some(open) = &mut self.open else {
            return;
        };
        if open.truncated {
            return;
        }
        let rearm = open.buffer.edited();
        cx.notify();
        if !rearm {
            return;
        }
        // Autosave is a setting now (Settings ▸ Editor ▸ Auto save, and its
        // delay). Switched off, a dirty buffer waits for ⌘S or `:w` — which is
        // exactly what Electron's `autoSaveEnabled: false` does.
        let Some(autosave_delay) = ClientSettings::auto_save_delay(cx) else {
            return;
        };
        open.debounce += 1;
        let debounce = open.debounce;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(autosave_delay).await;
            let _ = this.update_in(cx, |panel, window, cx| {
                if panel.open_generation != generation {
                    return;
                }
                let fresh = panel
                    .open
                    .as_ref()
                    .is_some_and(|open| open.debounce == debounce);
                if fresh {
                    panel.flush(window, cx);
                }
            });
        })
        .detach();
    }

    /// Start a write if the buffer wants one (debounce expiry or post-save
    /// coalesce).
    fn flush(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let Some(base_revision) = open.buffer.begin_save() else {
            return;
        };
        self.spawn_write(base_revision, window, cx);
    }

    /// Start an inline create row at the workspace root, the way the panel's
    /// own header buttons do. Backs the command palette's "New file" / "New
    /// folder" rows, which Electron routes to the same file-tree affordance.
    pub fn create_at_root(&mut self, directory: bool, window: &mut Window, cx: &mut Context<Self>) {
        let kind = if directory {
            ProjectMutateEntryInputCreateKind::Directory
        } else {
            ProjectMutateEntryInputCreateKind::File
        };
        self.start_edit(
            TreeEditTarget::Create {
                parent: String::new(),
                kind,
            },
            window,
            cx,
        );
    }

    /// ⌘S: write now instead of waiting out the autosave debounce. A clean
    /// buffer (or one whose write is already in flight) is a no-op, as it is
    /// in Electron — `flush` asks the buffer, which owns that decision.
    pub fn save_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(window, cx);
    }

    /// Shift-Alt-F: `textDocument/formatting` through the sidecar, applied as
    /// one edit batch. Electron leaves the result unsaved — the edits mark the
    /// buffer dirty and autosave (or ⌘S) persists it — so this does the same.
    pub fn format_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(relative_path) = self.lsp.current_document() else {
            return;
        };
        let generation = self.open_generation;
        // Formatting reads the server's copy of the document, so the pending
        // didChange has to land first.
        let text = self.editor.read(cx).text().clone();
        let flush = self.lsp.flush_document(&text, cx);
        let payload = LspFormattingInput {
            cwd: tnes(&self.cwd),
            insert_spaces: None,
            relative_path: tnes(relative_path),
            tab_size: None,
        };
        let client = self.client.clone();
        cx.spawn_in(window, async move |this, cx| {
            flush.await;
            let Ok(result) = client.call::<LspFormat>(&payload).await else {
                return;
            };
            if result.edits.is_empty() {
                return;
            }
            let _ = this.update_in(cx, |panel, window, cx| {
                if panel.open_generation != generation {
                    return;
                }
                panel.editor.update(cx, |state, cx| {
                    let text = state.text().clone();
                    let edits = result
                        .edits
                        .iter()
                        .map(|edit| bridge::editor_text_edit(&text, edit))
                        .collect();
                    state.apply_lsp_edits(&edits, window, cx);
                });
                // `apply_lsp_edits` replaces text silently, so the editor
                // emits no change event: tell the buffer and the LSP document
                // about the edit by hand.
                panel.editor_edited(window, cx);
            });
        })
        .detach();
    }

    /// Issue the writeFile RPC for the open file. The buffer must already
    /// count the write as in flight (`begin_save` or `resolve_keep_mine`).
    fn spawn_write(
        &mut self,
        base_revision: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let generation = self.open_generation;
        let Some(open) = &self.open else {
            return;
        };
        let payload = ProjectWriteFileInput {
            base_revision: base_revision.map(|revision| Some(tnes(revision))),
            contents: tnes(self.editor.read(cx).value().to_string()),
            cwd: tnes(&self.cwd),
            relative_path: tnes(&open.relative_path),
        };
        let client = self.client.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = client.call::<ProjectsWriteFile>(&payload).await;
            let _ = this.update_in(cx, |panel, window, cx| {
                if panel.open_generation != generation {
                    return;
                }
                let Some(open) = &mut panel.open else {
                    return;
                };
                match result {
                    Ok(result) => {
                        let outcome = open
                            .buffer
                            .save_succeeded(result.revision.flatten().map(|revision| revision.0));
                        if outcome.resave {
                            panel.flush(window, cx);
                        } else if outcome.disk == DiskChange::Reload {
                            panel.check_open_file_disk(window, cx);
                        }
                        // The saved file's git status changed.
                        panel.refresh_vcs(cx);
                    }
                    Err(TypedError::Failed(WriteFileErrorUnion::ProjectWriteFileError(error)))
                        if error.failure == Some(Some(ProjectFileFailure::StaleRevision)) =>
                    {
                        open.buffer.save_failed_stale();
                    }
                    Err(error) => {
                        open.buffer.save_failed();
                        panel.status = Some(
                            format!(
                                "save failed ({}): {}",
                                open.relative_path,
                                error.user_message()
                            )
                            .into(),
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Re-read the open file and let the buffer judge the disk revision:
    /// self-written → ignore, clean+foreign → reload in place, dirty+foreign
    /// → conflict banner.
    fn check_open_file_disk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.open_generation;
        let Some(open) = &self.open else {
            return;
        };
        let client = self.client.clone();
        let payload = ProjectReadFileInput {
            cwd: tnes(&self.cwd),
            relative_path: tnes(&open.relative_path),
        };
        cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = client.call::<ProjectsReadFile>(&payload).await else {
                return;
            };
            let _ = this.update_in(cx, |panel, window, cx| {
                if panel.open_generation != generation {
                    return;
                }
                let Some(open) = &mut panel.open else {
                    return;
                };
                let revision = result.revision.flatten().map(|revision| revision.0);
                if open.buffer.disk_changed(revision.clone()) == DiskChange::Reload {
                    open.buffer.resolve_reload(revision);
                    open.truncated = result.truncated;
                    let editor = panel.editor.clone();
                    editor.update(cx, |state, cx| {
                        state.set_value(result.contents.0.clone(), window, cx);
                    });
                    // The buffer changed under the LSP doc; sync it and
                    // restore diagnostics that set_value cleared.
                    panel.lsp.document_edited(cx);
                    panel.apply_diagnostics_to_editor(cx);
                    // `set_value` replaces the text silently, so nothing else
                    // asks for the diff markers to be recomputed.
                    panel.recompute_gutter(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Conflict banner "Reload from disk".
    fn resolve_by_reloading(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.open_generation;
        let Some(open) = &self.open else {
            return;
        };
        let client = self.client.clone();
        let payload = ProjectReadFileInput {
            cwd: tnes(&self.cwd),
            relative_path: tnes(&open.relative_path),
        };
        cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = client.call::<ProjectsReadFile>(&payload).await else {
                return;
            };
            let _ = this.update_in(cx, |panel, window, cx| {
                if panel.open_generation != generation {
                    return;
                }
                let Some(open) = &mut panel.open else {
                    return;
                };
                open.buffer
                    .resolve_reload(result.revision.flatten().map(|revision| revision.0));
                open.truncated = result.truncated;
                let editor = panel.editor.clone();
                editor.update(cx, |state, cx| {
                    state.set_value(result.contents.0.clone(), window, cx);
                });
                panel.lsp.document_edited(cx);
                panel.apply_diagnostics_to_editor(cx);
                panel.recompute_gutter(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Conflict banner "Keep my version": unconditional write (no
    /// baseRevision guard). `resolve_keep_mine` already counts the write as
    /// in flight, so this must NOT route through `begin_save`.
    fn resolve_by_keeping_mine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        if open.buffer.resolve_keep_mine() {
            self.spawn_write(None, window, cx);
        }
        cx.notify();
    }

    // ---- tree keyboard navigation ----

    /// The rows the tree is currently showing — the list every keyboard move
    /// walks, and the same one `render_explorer` renders.
    fn visible_rows(&self) -> Vec<FileTreeRow> {
        self.tree
            .as_ref()
            .map(|tree| tree.visible_rows(&self.expanded))
            .unwrap_or_default()
    }

    fn focused_row_index(&self, rows: &[FileTreeRow]) -> Option<usize> {
        let path = self.tree_focused.as_deref()?;
        rows.iter().position(|row| row.path == path)
    }

    /// Park roving focus on `index` and scroll it back into view.
    ///
    /// Electron's tree scrolls the minimum amount to reveal the row, which is
    /// what `scroll_to_item`'s default strategy does.
    fn focus_row(&mut self, rows: &[FileTreeRow], index: usize, cx: &mut Context<Self>) {
        let Some(row) = rows.get(index) else {
            return;
        };
        self.tree_focused = Some(row.path.clone());
        self.tree_scroll.scroll_to_item(index);
        cx.notify();
    }

    /// Where a move starts from: the focused row, or the first one when the
    /// tree has not been navigated yet.
    fn move_focus(&mut self, delta: isize, cx: &mut Context<Self>) {
        let rows = self.visible_rows();
        if rows.is_empty() {
            return;
        }
        let index = match self.focused_row_index(&rows) {
            Some(index) => index.saturating_add_signed(delta).min(rows.len() - 1),
            None => 0,
        };
        self.focus_row(&rows, index, cx);
    }

    fn focus_edge(&mut self, last: bool, cx: &mut Context<Self>) {
        let rows = self.visible_rows();
        if rows.is_empty() {
            return;
        }
        let index = if last { rows.len() - 1 } else { 0 };
        self.focus_row(&rows, index, cx);
    }

    /// `ArrowRight` / `l`: unfold a folded directory, otherwise step down —
    /// an expanded directory's "right" is its first child.
    fn expand_or_next(&mut self, cx: &mut Context<Self>) {
        let rows = self.visible_rows();
        let Some(index) = self.focused_row_index(&rows) else {
            self.move_focus(0, cx);
            return;
        };
        let row = &rows[index];
        if row.is_dir && !row.expanded {
            let path = row.path.clone();
            self.toggle_dir(&path, cx);
            return;
        }
        self.move_focus(1, cx);
    }

    /// `ArrowLeft` / `h`: fold an unfolded directory, otherwise climb to the
    /// parent row.
    fn collapse_or_parent(&mut self, cx: &mut Context<Self>) {
        let rows = self.visible_rows();
        let Some(index) = self.focused_row_index(&rows) else {
            self.move_focus(0, cx);
            return;
        };
        let row = &rows[index];
        if row.is_dir && row.expanded {
            let path = row.path.clone();
            self.toggle_dir(&path, cx);
            return;
        }
        let parent = parent_dir(&row.path).to_string();
        if parent.is_empty() {
            return;
        }
        if let Some(parent_index) = rows.iter().position(|row| row.path == parent) {
            self.focus_row(&rows, parent_index, cx);
        }
    }

    /// `Enter` / `Space`: what a click on the focused row would do. Electron
    /// gets this from the browser, whose rows are `<button>`s.
    fn activate_focused_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.visible_rows();
        let Some(index) = self.focused_row_index(&rows) else {
            return;
        };
        let row = &rows[index];
        let path = row.path.clone();
        if row.is_dir {
            self.toggle_dir(&path, cx);
        } else {
            self.open_file(path, window, cx);
        }
    }

    fn toggle_dir(&mut self, path: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_string());
        }
        cx.notify();
    }

    /// Begin an inline create/rename edit in the tree (Electron's
    /// `startRenaming` flow: commit on Enter, cancel on blur).
    fn start_edit(&mut self, target: TreeEditTarget, window: &mut Window, cx: &mut Context<Self>) {
        let initial = match &target {
            TreeEditTarget::Rename { path } => path.rsplit('/').next().unwrap_or(path).to_string(),
            TreeEditTarget::Create { parent, .. } => {
                if !parent.is_empty() {
                    self.expanded.insert(parent.clone());
                }
                String::new()
            }
        };
        let input = cx.new(|cx| {
            let state = InputState::new(window, cx).placeholder("name…");
            if initial.is_empty() {
                state
            } else {
                state.default_value(initial)
            }
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.commit_edit(window, cx),
                InputEvent::Blur => this.cancel_edit(cx),
                _ => {}
            },
        );
        input.update(cx, |state, cx| state.focus(window, cx));
        self.edit = Some(TreeEdit {
            target,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        if self.edit.take().is_some() {
            cx.notify();
        }
    }

    fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.edit.take() else {
            return;
        };
        cx.notify();
        let name = edit.input.read(cx).value().trim().to_string();
        // Same guard the wire enforces: no empty names, no path separators.
        if name.is_empty() || name.contains('/') {
            return;
        }
        let input = match &edit.target {
            TreeEditTarget::Create { parent, kind } => ProjectMutateEntryInput::Create {
                cwd: tnes(&self.cwd),
                kind: kind.clone(),
                relative_path: tnes(join_path(parent, &name)),
            },
            TreeEditTarget::Rename { path } => {
                let to = join_path(parent_dir(path), &name);
                if to == *path {
                    return;
                }
                ProjectMutateEntryInput::Rename {
                    cwd: tnes(&self.cwd),
                    from_relative_path: tnes(path),
                    to_relative_path: tnes(to),
                }
            }
        };
        self.mutate(input, window, cx);
    }

    /// Native confirm then delete (Electron uses `window.confirm`).
    fn confirm_delete(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = window.prompt(
            PromptLevel::Warning,
            &format!("Delete \"{path}\"?"),
            Some("This cannot be undone."),
            &["Delete", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if receiver.await != Ok(0) {
                return;
            }
            let _ = this.update_in(cx, |panel, window, cx| {
                let input = ProjectMutateEntryInput::Delete {
                    cwd: tnes(&panel.cwd),
                    relative_path: tnes(&path),
                };
                panel.mutate(input, window, cx);
            });
        })
        .detach();
    }

    /// Dispatch a structural mutation; on success apply the follow-up (open
    /// created file / follow rename / close deleted) and re-list. Electron
    /// mutates optimistically and refreshes on failure; refreshing on success
    /// keeps the same end state with less machinery.
    fn mutate(
        &mut self,
        input: ProjectMutateEntryInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let client = self.client.clone();
        cx.spawn_in(window, async move |this, cx| {
            match client.call::<ProjectsMutateEntry>(&input).await {
                Ok(_) => {
                    let _ = this.update_in(cx, |panel, window, cx| {
                        match &input {
                            ProjectMutateEntryInput::Create {
                                kind,
                                relative_path,
                                ..
                            } => match kind {
                                ProjectMutateEntryInputCreateKind::File => {
                                    panel.open_file(relative_path.0.clone(), window, cx);
                                }
                                _ => {
                                    panel.expanded.insert(relative_path.0.clone());
                                }
                            },
                            ProjectMutateEntryInput::Rename {
                                from_relative_path,
                                to_relative_path,
                                ..
                            } => {
                                panel.follow_rename(&from_relative_path.0, &to_relative_path.0, cx);
                            }
                            ProjectMutateEntryInput::Delete { relative_path, .. } => {
                                panel.close_if_within(&relative_path.0, cx);
                            }
                            ProjectMutateEntryInput::Unknown(_) => {}
                        }
                        panel.refresh_tree(cx);
                        cx.notify();
                    });
                }
                Err(error) => {
                    let _ = this.update(cx, |panel, cx| {
                        panel.status =
                            Some(format!("operation failed: {}", error.user_message()).into());
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Keep the open buffer attached across a rename of itself or an
    /// ancestor directory.
    fn follow_rename(&mut self, from: &str, to: &str, cx: &mut Context<Self>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let new_path = if open.relative_path == from {
            Some(to.to_string())
        } else {
            open.relative_path
                .strip_prefix(&format!("{from}/"))
                .map(|rest| format!("{to}/{rest}"))
        };
        if let Some(new_path) = new_path {
            open.relative_path = new_path.clone();
            let truncated = open.truncated;
            self.editor.update(cx, |state, cx| {
                state.set_highlighter(language_for_path(&new_path), cx);
            });
            // Rebind the LSP doc under its new name (the Electron editor
            // remounts on rename: didClose old, didOpen new).
            if !truncated {
                let contents = self.editor.read(cx).text().to_string();
                self.lsp.open_document(&new_path, contents, cx);
                self.apply_diagnostics_to_editor(cx);
            }
        }
        // Expansion keys under the old name are stale; move them over.
        let moved: Vec<String> = self
            .expanded
            .iter()
            .filter(|dir| *dir == from || dir.starts_with(&format!("{from}/")))
            .cloned()
            .collect();
        for dir in moved {
            self.expanded.remove(&dir);
            let renamed = if dir == from {
                to.to_string()
            } else {
                format!("{to}{}", &dir[from.len()..])
            };
            self.expanded.insert(renamed);
        }
    }

    fn close_if_within(&mut self, path: &str, cx: &mut Context<Self>) {
        let is_within =
            |candidate: &str| candidate == path || candidate.starts_with(&format!("{path}/"));
        // A failed-open surface for something inside the deleted path is just
        // as stale as an open buffer would be.
        if self
            .open_error
            .as_ref()
            .is_some_and(|error| is_within(&error.relative_path))
        {
            self.open_error = None;
            cx.notify();
        }
        let within = self
            .open
            .as_ref()
            .is_some_and(|open| is_within(&open.relative_path));
        if within {
            self.close_file(cx);
        }
    }

    fn row_decoration(
        &self,
        row: &FileTreeRow,
        cx: &Context<Self>,
    ) -> (Option<gpui::Hsla>, Option<&'static str>) {
        let Some(status) = self.vcs.statuses.get(&row.path) else {
            return (None, None);
        };
        let theme = cx.theme();
        let (color, letter) = match status {
            TreeVcsStatus::Modified => (theme.warning, "M"),
            TreeVcsStatus::Renamed => (theme.info, "R"),
            TreeVcsStatus::Deleted => (theme.danger, "D"),
            TreeVcsStatus::Added => (theme.success, "A"),
            TreeVcsStatus::Untracked => (theme.success, "U"),
            TreeVcsStatus::Ignored => return (None, None),
        };
        let letter = if self.vcs.conflicted.contains(&row.path) {
            "!"
        } else {
            letter
        };
        // Folders tint the name only (Electron hides the folder status letter).
        (Some(color), (!row.is_dir).then_some(letter))
    }

    fn render_tree_row(
        &self,
        index: usize,
        row: &FileTreeRow,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (tint, letter) = self.row_decoration(row, cx);
        let is_open = self
            .open
            .as_ref()
            .is_some_and(|open| open.relative_path == row.path);
        let is_focused = self.tree_focused.as_deref() == Some(row.path.as_str());
        let name_color = tint.unwrap_or(cx.theme().foreground);
        let path = row.path.clone();
        let is_dir = row.is_dir;
        let panel: WeakEntity<Self> = cx.entity().downgrade();
        let menu_path = row.path.clone();
        // New entries land inside a directory row, next to a file row.
        let create_parent = if is_dir {
            row.path.clone()
        } else {
            parent_dir(&row.path).to_string()
        };
        h_flex()
            .id(("file-tree-row", index))
            .relative()
            .h(px(24.))
            // The rows are direct children of the scrolling column now, so
            // they have to refuse to be squeezed into its fixed height.
            .flex_shrink_0()
            .w_full()
            .pl(px(8. + row.depth as f32 * 12.))
            .pr_2()
            .gap_1()
            .items_center()
            .text_sm()
            .cursor_pointer()
            // Electron rounds its row buttons (`border-radius: 5px` in the
            // panel's tree CSS), so the fills and the focus ring below match.
            .rounded(px(5.))
            .when(is_open, |this| this.bg(cx.theme().accent))
            .hover(|this| this.bg(cx.theme().accent.opacity(0.6)))
            // The roving-focus ring: 1px, drawn just inside the row, exactly
            // the `outline: 1px solid; outline-offset: -1px` the tree paints
            // on `data-item-focused`.
            .when(is_focused, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .border_1()
                        .border_color(cx.theme().ring)
                        .rounded(px(5.)),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    // A click parks roving focus here too, the way clicking a
                    // row moves DOM focus onto its button in Electron.
                    this.tree_focused = Some(path.clone());
                    this.tree_focus.focus(window, cx);
                    if is_dir {
                        this.toggle_dir(&path, cx);
                    } else {
                        this.open_file(path.clone(), window, cx);
                    }
                }),
            )
            .child(
                div()
                    .w(px(14.))
                    .flex_shrink_0()
                    .children(row.is_dir.then(|| {
                        Icon::new(if row.expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size_3()
                        .text_color(cx.theme().muted_foreground)
                    })),
            )
            .child(
                Icon::new(match (row.is_dir, row.expanded) {
                    (true, true) => IconName::FolderOpen,
                    (true, false) => IconName::Folder,
                    (false, _) => IconName::File,
                })
                .size_3p5()
                .flex_shrink_0()
                .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(name_color)
                    .when(row.ignored, |this| this.opacity(0.5))
                    .child(row.display_name.clone()),
            )
            .children(letter.map(|letter| {
                let color = if letter == "!" {
                    cx.theme().danger
                } else {
                    name_color
                };
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(color)
                    .child(letter)
            }))
            .context_menu(move |menu, _, _| {
                let new_file = (panel.clone(), create_parent.clone());
                let new_folder = (panel.clone(), create_parent.clone());
                let rename = (panel.clone(), menu_path.clone());
                let delete = (panel.clone(), menu_path.clone());
                menu.item(
                    PopupMenuItem::new("New File…").on_click(move |_, window, cx| {
                        let (panel, parent) = &new_file;
                        let target = TreeEditTarget::Create {
                            parent: parent.clone(),
                            kind: ProjectMutateEntryInputCreateKind::File,
                        };
                        let _ = panel.update(cx, |panel, cx| panel.start_edit(target, window, cx));
                    }),
                )
                .item(
                    PopupMenuItem::new("New Folder…").on_click(move |_, window, cx| {
                        let (panel, parent) = &new_folder;
                        let target = TreeEditTarget::Create {
                            parent: parent.clone(),
                            kind: ProjectMutateEntryInputCreateKind::Directory,
                        };
                        let _ = panel.update(cx, |panel, cx| panel.start_edit(target, window, cx));
                    }),
                )
                .separator()
                .item(
                    PopupMenuItem::new("Rename…").on_click(move |_, window, cx| {
                        let (panel, path) = &rename;
                        let target = TreeEditTarget::Rename { path: path.clone() };
                        let _ = panel.update(cx, |panel, cx| panel.start_edit(target, window, cx));
                    }),
                )
                .item(PopupMenuItem::new("Delete").on_click(move |_, window, cx| {
                    let (panel, path) = &delete;
                    let path = path.clone();
                    let _ = panel.update(cx, |panel, cx| panel.confirm_delete(path, window, cx));
                }))
            })
            .into_any_element()
    }

    /// The inline create/rename row: same geometry as a tree row with the
    /// name swapped for a single-line input.
    fn render_edit_row(
        &self,
        depth: usize,
        is_dir: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(edit) = &self.edit else {
            return div().into_any_element();
        };
        h_flex()
            .h(px(26.))
            .flex_shrink_0()
            .w_full()
            .pl(px(8. + depth as f32 * 12.))
            .pr_2()
            .gap_1()
            .items_center()
            .child(div().w(px(14.)).flex_shrink_0())
            .child(
                Icon::new(if is_dir {
                    IconName::Folder
                } else {
                    IconName::File
                })
                .size_3p5()
                .flex_shrink_0()
                .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&edit.input).xsmall()),
            )
            .into_any_element()
    }

    fn render_explorer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<_> = self
            .tree
            .as_ref()
            .map(|tree| tree.visible_rows(&self.expanded))
            .unwrap_or_default();
        let empty = rows.is_empty();
        // Interleave the inline edit row: root-level creates render at the
        // top, in-directory creates directly under the parent row, renames
        // replace the row in place.
        let mut items: Vec<gpui::AnyElement> = Vec::new();
        if let Some(TreeEditTarget::Create { parent, kind }) =
            self.edit.as_ref().map(|edit| edit.target.clone())
            && parent.is_empty()
        {
            let is_dir = matches!(kind, ProjectMutateEntryInputCreateKind::Directory);
            items.push(self.render_edit_row(0, is_dir, cx));
        }
        for (index, row) in rows.iter().enumerate() {
            let target = self.edit.as_ref().map(|edit| edit.target.clone());
            if matches!(&target, Some(TreeEditTarget::Rename { path }) if *path == row.path) {
                items.push(self.render_edit_row(row.depth, row.is_dir, cx));
                continue;
            }
            items.push(self.render_tree_row(index, row, cx));
            if let Some(TreeEditTarget::Create { parent, kind }) = target
                && parent == row.path
                && row.is_dir
                && row.expanded
            {
                let is_dir = matches!(kind, ProjectMutateEntryInputCreateKind::Directory);
                items.push(self.render_edit_row(row.depth + 1, is_dir, cx));
            }
        }
        v_flex()
            .h_full()
            .min_h_0()
            .child(
                v_flex()
                    .id("file-tree-scroll")
                    .track_focus(&self.tree_focus)
                    // The tree's keys are only live while nothing in it is
                    // being typed into — Electron's `isTextEntryTarget`
                    // guard, expressed as the context simply not being there.
                    .when(self.edit.is_none(), |this| this.key_context("FileTree"))
                    .on_action(cx.listener(|panel, _: &TreeFocusNext, _, cx| {
                        panel.move_focus(1, cx);
                    }))
                    .on_action(cx.listener(|panel, _: &TreeFocusPrevious, _, cx| {
                        panel.move_focus(-1, cx);
                    }))
                    .on_action(cx.listener(|panel, _: &TreeFocusFirst, _, cx| {
                        panel.focus_edge(false, cx);
                    }))
                    .on_action(cx.listener(|panel, _: &TreeFocusLast, _, cx| {
                        panel.focus_edge(true, cx);
                    }))
                    .on_action(cx.listener(|panel, _: &TreeExpandOrNext, _, cx| {
                        panel.expand_or_next(cx);
                    }))
                    .on_action(cx.listener(|panel, _: &TreeCollapseOrParent, _, cx| {
                        panel.collapse_or_parent(cx);
                    }))
                    .on_action(cx.listener(|panel, _: &TreeActivate, window, cx| {
                        panel.activate_focused_row(window, cx);
                    }))
                    .flex_1()
                    .min_h_0()
                    .py_1()
                    // The rows have to be this element's own children for
                    // `scroll_to_item` to find their bounds.
                    .overflow_y_scroll()
                    .track_scroll(&self.tree_scroll)
                    .children(items)
                    .when(empty, |this| {
                        this.child(
                            div()
                                .p_3()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(if self.tree.is_none() {
                                    "Loading files…"
                                } else {
                                    "No files"
                                }),
                        )
                    }),
            )
            .children(self.tree_truncated.then(|| {
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child("Listing truncated — some files are not shown")
            }))
    }

    fn render_conflict_banner(
        &self,
        conflict: BufferConflict,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let message = match conflict {
            BufferConflict::StaleSave | BufferConflict::ExternalChange => {
                "This file changed on disk while you were editing. Your edits are not being saved."
            }
        };
        h_flex()
            .px_3()
            .py_2()
            .gap_2()
            .items_center()
            .bg(cx.theme().warning.opacity(0.15))
            .border_b_1()
            .border_color(cx.theme().warning.opacity(0.4))
            .child(
                Icon::new(IconName::TriangleAlert)
                    .size_4()
                    .text_color(cx.theme().warning),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child(message),
            )
            .child(
                Button::new("conflict-reload")
                    .small()
                    .outline()
                    .label("Reload from disk")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.resolve_by_reloading(window, cx);
                    })),
            )
            .child(
                Button::new("conflict-keep")
                    .small()
                    .outline()
                    .label("Keep my version")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.resolve_by_keeping_mine(window, cx);
                    })),
            )
    }

    // ---- vim mode ---------------------------------------------------------

    /// Follow the `vimMode` preference after it flips.
    /// Re-read the client settings this panel honours. Fired by
    /// `cx.observe_global::<ClientSettings>`, so flipping a switch in Settings
    /// reaches an already-open buffer without a reopen.
    fn sync_client_settings(&mut self, cx: &mut Context<Self>) {
        // Soft wrap needs a `Window` the observer does not have, so it is
        // applied on the next frame (see `Render`).
        self.word_wrap = ClientSettings::word_wrap(cx);
        cx.notify();
        let enabled = ClientSettings::vim_mode(cx);
        if enabled == self.vim.is_some() {
            return;
        }
        self.vim = enabled.then(VimSession::new);
        if let Some(vim) = self.vim.as_mut() {
            // Adopt whatever the editor's caret is right now, so vim starts in
            // normal mode over the character the user was looking at.
            let rope = self.editor.read(cx).text().clone();
            let caret = self.editor.read(cx).selected_range().start;
            let text = vim.text(&rope);
            vim.engine.sync_cursor(&text, caret);
        }
        self.sync_vim_view(cx);
        cx.notify();
    }

    /// Take the editor's selection as the engine's, for a jump the engine did
    /// not drive: go-to-definition, a revealed range, a file opened at a
    /// position. Without this [`Self::sync_vim_view`] would immediately
    /// paint the *stale* engine caret back over the jump target.
    ///
    /// A non-empty one enters visual mode over it, which is what
    /// codemirror-vim does from its own `cursorActivity` hook
    /// (`handleExternalSelection`) for any selection that did not come from a
    /// vim command — a mouse drag, or a reveal naming a line range.
    fn vim_adopt_selection(&mut self, cx: &mut Context<Self>) {
        let rope = self.editor.read(cx).text().clone();
        let selection = self.editor.read(cx).selected_range();
        if let Some(vim) = self.vim.as_mut() {
            vim.invalidate();
            let text = vim.text(&rope);
            vim.engine.sync_selection(&text, selection);
        }
        self.sync_vim_view(cx);
    }

    /// Push the engine's caret and mode into the editor: the selection it
    /// should paint, the block cursor sitting on the caret, and the key
    /// context its bindings are matched against.
    fn sync_vim_view(&mut self, cx: &mut Context<Self>) {
        let rope = self.editor.read(cx).text().clone();
        let Some(vim) = self.vim.as_mut() else {
            self.editor.update(cx, |state, cx| {
                state.set_extra_key_context(None, cx);
                state.set_block_cursor(None, cx);
            });
            return;
        };
        vim.invalidate();
        let text = vim.text(&rope);
        let selection = vim.engine.editor_selection(&text);
        let mode = vim.engine.mode();
        let context = vim::key_context(Some(mode));
        // vim squashes the block to half height while a multi-key command is
        // half-typed, so `d` waiting for its motion is visible in the caret
        // rather than only in the status line.
        let height = if vim.engine.has_pending_keys() {
            PENDING_BLOCK_CURSOR_HEIGHT
        } else {
            1.0
        };
        let block = vim
            .engine
            .caret_cell(&text)
            .map(|cell| BlockCursor::new(cell, rgb(BLOCK_CURSOR_COLOR)).with_height(height));
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(selection, cx);
            state.set_extra_key_context(context, cx);
            // The caret is a solid block on the character rather than a bar
            // between two, so it stays readable inside a visual selection —
            // codemirror-vim's `cm-fat-cursor`, which is what Electron paints.
            state.set_block_cursor(block, cx);
        });
    }

    /// Feed one key to the engine and apply what it asks for.
    fn vim_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = vim::engine_key(key) else {
            return;
        };
        if self.vim.is_none() {
            return;
        }
        let (rope, selection, visible_lines) = {
            let state = self.editor.read(cx);
            (
                state.text().clone(),
                state.selected_range(),
                // Before the first layout there is no viewport; `H`/`M`/`L`
                // and the half-page scrolls fall back to a screenful.
                state.visible_row_range().unwrap_or(0..40),
            )
        };

        let vim = self.vim.as_mut().expect("checked above");
        let text = vim.text(&rope);

        // A mouse click, a drag or an LSP jump moved the caret behind the
        // engine's back.
        if selection != vim.engine.editor_selection(&text) {
            vim.engine.sync_selection(&text, selection.clone());
        }

        // Insert mode is about to end: the editor owned text input while it
        // lasted (IME composition has to reach the platform untouched), so
        // recover what was typed for `.`.
        let was_inserting = vim.engine.mode().is_inserting();
        if was_inserting {
            let inserted = vim.take_inserted(&text, selection.end);
            vim.engine.record_inserted_text(&inserted);
        }

        let response = vim.engine.handle_key(
            &VimDocument {
                text: &text,
                visible_lines,
            },
            key,
        );
        if !response.handled {
            return;
        }
        let caret = vim.engine.cursor();
        let effects = response.effects;
        let entered_insert = !was_inserting && vim.engine.mode().is_inserting();

        let readonly = self.open.as_ref().is_some_and(|open| open.truncated);
        for effect in effects {
            match effect {
                VimEffect::Edit {
                    range,
                    text,
                    cursor,
                } => {
                    // A truncated read is a partial buffer the server would
                    // reject anyway; motions still work, edits do not.
                    if readonly {
                        continue;
                    }
                    self.editor.update(cx, |state, cx| {
                        state.set_selected_range(range, cx);
                        state.replace(text, window, cx);
                        state.set_selected_range(cursor..cursor, cx);
                    });
                }
                VimEffect::Undo | VimEffect::Redo => {
                    let undo = matches!(effect, VimEffect::Undo);
                    let handle = self.editor.read(cx).focus_handle(cx);
                    if undo {
                        handle.dispatch_action(&Undo, window, cx);
                    } else {
                        handle.dispatch_action(&Redo, window, cx);
                    }
                }
                // `:w` — Electron routes it to the same save coordinator the
                // `file.save` command uses.
                VimEffect::Save => self.save_now(window, cx),
                VimEffect::Quit => self.close_file(cx),
                VimEffect::ShowHover => self.vim_show_hover(caret, window, cx),
                VimEffect::GoToDefinition => self.go_to_definition(caret, window, cx),
                VimEffect::Format => self.format_document(window, cx),
                // The caret follows the selection we set below, and
                // `set_selected_range` already scrolls it into view.
                VimEffect::ScrollToCursor => {}
            }
        }

        self.sync_vim_view(cx);
        if entered_insert && let Some(vim) = self.vim.as_mut() {
            vim.begin_insert(vim.engine.cursor());
        }
        cx.notify();
    }

    /// `gh` — the LSP hover popover at the caret, Electron's `lspHover`
    /// action. The mouse path debounces; a deliberate keystroke does not.
    fn vim_show_hover(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        let rope = self.editor.read(cx).text().clone();
        let task = self.lsp.hover(&rope, offset, window, cx);
        let editor = self.editor.downgrade();
        cx.spawn(async move |_, cx| {
            let Ok(Some(hover)) = task.await else {
                return;
            };
            let _ = editor.update(cx, |state, cx| {
                let symbol_range = hover
                    .range
                    .map(|range| {
                        state.text().position_to_offset(&range.start)
                            ..state.text().position_to_offset(&range.end)
                    })
                    .or_else(|| state.text().word_range(offset))
                    .unwrap_or(offset..offset);
                state.present_hover(symbol_range, hover, cx);
            });
        })
        .detach();
    }

    /// Go to the definition at `offset` — Electron's `lspDefinition`, shared
    /// by `F12`, vim `gd`/`<C-]>` and mod-click. The fork's own
    /// `GoToDefinition` only fires for a location a modifier-hover already
    /// resolved, so the jump is driven from the provider directly.
    fn go_to_definition(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        let rope = self.editor.read(cx).text().clone();
        let task = self.lsp.definitions(&rope, offset, window, cx);
        cx.spawn_in(window, async move |this, cx| {
            let Ok(links) = task.await else {
                return;
            };
            let Some(link) = links.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |panel, window, cx| {
                let Some(target) = bridge::relative_path_from_uri(&panel.cwd, &link.target_uri)
                else {
                    return;
                };
                // Same-file targets already carry editor coordinates;
                // cross-file ones stay in WIRE units until their file loads
                // (see the LspBridge definition provider).
                if panel.lsp.current_document().as_deref() == Some(target.as_str()) {
                    panel.editor.update(cx, |state, cx| {
                        state.set_cursor_position(link.target_selection_range.start, window, cx);
                    });
                    panel.vim_adopt_selection(cx);
                    return;
                }
                panel.pending_reveal = Some((
                    target.clone(),
                    RevealTarget::at(WirePosition {
                        line: link.target_selection_range.start.line,
                        character: link.target_selection_range.start.character,
                    }),
                ));
                panel.open_file(target, window, cx);
            });
        })
        .detach();
    }

    /// `mod+i` — Electron's `editor.showCompletions`, which calls CodeMirror's
    /// `startCompletion` on the live view.
    ///
    /// Electron gates the command on `editorFocus`; here the action reaches
    /// the panel from anywhere inside it, so the same gate is the explicit
    /// focus check.
    fn show_completions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor.read(cx).focus_handle(cx).is_focused(window) {
            return;
        }
        self.editor
            .update(cx, |state, cx| state.show_completions(window, cx));
    }

    /// The vim message line: Electron mounts `@replit/codemirror-vim` without
    /// `status`, so there is no persistent mode banner — only the panel that
    /// appears while a `:` or `/` command is being typed, and for the message
    /// a command leaves behind.
    fn render_vim_status(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
        let status = self.vim.as_ref()?.engine.status();
        let line = status.command_line.or(status.message)?;
        Some(
            div()
                .px_2()
                .py_0p5()
                .flex_shrink_0()
                .border_t_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().secondary)
                .font_family(cx.theme().mono_font_family.clone())
                .text_xs()
                .text_color(cx.theme().foreground)
                .child(line),
        )
    }

    fn render_editor_area(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(failed) = self.open_error.as_ref() {
            // Electron: one centred line of destructive text where the editor
            // would be, carrying the server's own message and nothing else —
            // no icon, no title, no retry button.
            return v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .px_8()
                        .max_w(px(480.))
                        .text_sm()
                        .text_center()
                        .text_color(cx.theme().danger)
                        .debug_selector(|| "files-open-error".into())
                        .child(failed.message.clone()),
                )
                .into_any_element();
        }
        let open = self.open.as_ref();
        // `showFileConflictWarning`: Electron keeps detecting conflicts and
        // only silences the banner, which is what hiding it here does — the
        // buffer still refuses to overwrite a newer revision.
        let conflict = open
            .and_then(|open| open.buffer.conflict())
            .filter(|_| ClientSettings::show_file_conflict_warning(cx));
        let truncated = open.is_some_and(|open| open.truncated);
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .children(conflict.map(|conflict| self.render_conflict_banner(conflict, cx)))
            .children(truncated.then(|| {
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child("File exceeds 1MB — showing a truncated read-only view")
            }))
            .child(
                div().flex_1().min_h_0().child(
                    Editor::new(&self.editor)
                        .readonly(truncated)
                        .appearance(false)
                        .size_full(),
                ),
            )
            .children(self.render_vim_status(cx))
            .into_any_element()
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let open_path = self
            .open
            .as_ref()
            .map(|open| open.relative_path.as_str())
            .or(self
                .open_error
                .as_ref()
                .map(|failed| failed.relative_path.as_str()))
            .map(|path| path.replace('/', " › "));
        let dirty = self
            .open
            .as_ref()
            .is_some_and(|open| open.buffer.is_dirty());
        h_flex()
            .h(px(36.))
            .px_3()
            .gap_2()
            .items_center()
            .flex_shrink_0()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .text_color(if open_path.is_some() {
                        cx.theme().foreground
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(open_path.unwrap_or_else(|| "Files".into())),
            )
            .children(dirty.then(|| {
                div()
                    .size(px(7.))
                    .rounded_full()
                    .flex_shrink_0()
                    .bg(cx.theme().muted_foreground)
            }))
            .children(self.open.is_some().then(|| {
                Button::new("files-close-file")
                    .icon(IconName::Close)
                    .ghost()
                    .xsmall()
                    .tooltip("Close file")
                    .on_click(cx.listener(|this, _, _, cx| this.close_file(cx)))
            }))
            .child(
                Button::new("files-new-file")
                    .icon(IconName::Plus)
                    .ghost()
                    .xsmall()
                    .tooltip("New file")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let target = TreeEditTarget::Create {
                            parent: String::new(),
                            kind: ProjectMutateEntryInputCreateKind::File,
                        };
                        this.start_edit(target, window, cx);
                    })),
            )
            .child(
                Button::new("files-new-folder")
                    .icon(IconName::Folder)
                    .ghost()
                    .xsmall()
                    .tooltip("New folder")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let target = TreeEditTarget::Create {
                            parent: String::new(),
                            kind: ProjectMutateEntryInputCreateKind::Directory,
                        };
                        this.start_edit(target, window, cx);
                    })),
            )
            .child(
                Button::new("files-refresh")
                    .icon(IconName::Redo)
                    .ghost()
                    .xsmall()
                    .tooltip("Refresh file list")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.refresh_tree(cx);
                        cx.notify();
                    })),
            )
            // Electron's `FolderTree` toggle, last in the breadcrumb bar —
            // which only exists while a file is open, since the aside always
            // shows when none is.
            .children(self.open.is_some().then(|| {
                Button::new("files-toggle-explorer")
                    .icon(IconName::FolderTree)
                    .ghost()
                    .xsmall()
                    .selected(self.explorer_open)
                    .tooltip(if self.explorer_open {
                        "Hide file explorer"
                    } else {
                        "Show file explorer"
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_explorer(cx)))
            }))
    }

    /// Show or hide the tree aside, persisting the choice the way Electron
    /// persists `t3code.fileExplorerOpen`.
    fn toggle_explorer(&mut self, cx: &mut Context<Self>) {
        self.explorer_open = !self.explorer_open;
        ClientSettings::set_file_explorer_open(cx, self.explorer_open);
        cx.notify();
    }
}

/// The hunk peek card: Electron's `.cm-gitDiffPeek` widget, rendered as a
/// block overlay under the hunk's last line.
///
/// The one structural difference from CodeMirror is that this floats over the
/// following lines instead of pushing them down — see [`BlockOverlay`].
///
/// The card swallows mouse-down, which is CodeMirror's `ignoreEvent(): true` on
/// the widget: clicking anywhere in the card must not move the caret into the
/// text underneath. Because the editor's own mouse-down handler sits on the
/// Input's root container — an ancestor, so it bubbles later — stopping here is
/// enough, and it leaves the buttons' click handlers (which fire on mouse-up)
/// untouched.
fn render_hunk_peek(
    panel: &WeakEntity<FilesPanel>,
    index: usize,
    count: usize,
    original: &SharedString,
    cx: &mut App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let mut card = v_flex().flex_1().min_w_0().child(
        h_flex()
            .justify_between()
            .items_center()
            // A narrow editor pane wraps the actions to their own row rather
            // than pushing them past the card's edge.
            .flex_wrap()
            .gap_2()
            .px_2()
            .py_0p5()
            .border_b_1()
            .border_color(cx.theme().border)
            .text_size(px(11.))
            .text_color(muted)
            // The label yields first when the editor pane is narrow, so the
            // actions stay reachable instead of being clipped by the card.
            .child(div().min_w_0().truncate().child(SharedString::from(format!(
                "Hunk {} of {}",
                index + 1,
                count
            ))))
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_1()
                    .items_center()
                    .child(
                        Button::new("git-peek-prev")
                            .xsmall()
                            .outline()
                            .label("\u{2039}")
                            .tooltip("Previous change")
                            .on_click({
                                let panel = panel.clone();
                                move |_, _: &mut Window, cx: &mut App| {
                                    let _ = panel.update(cx, |panel, cx| {
                                        panel.step_hunk_peek(-1, cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new("git-peek-next")
                            .xsmall()
                            .outline()
                            .label("\u{203a}")
                            .tooltip("Next change")
                            .on_click({
                                let panel = panel.clone();
                                move |_, _: &mut Window, cx: &mut App| {
                                    let _ = panel.update(cx, |panel, cx| {
                                        panel.step_hunk_peek(1, cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new("git-peek-revert")
                            .xsmall()
                            .outline()
                            .label("Revert")
                            .tooltip("Revert this change")
                            .on_click({
                                let panel = panel.clone();
                                move |_, window: &mut Window, cx: &mut App| {
                                    let _ = panel.update(cx, |panel, cx| {
                                        panel.revert_hunk(window, cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new("git-peek-close")
                            .xsmall()
                            .outline()
                            .label("\u{2715}")
                            .tooltip("Close")
                            .on_click({
                                let panel = panel.clone();
                                move |_, _: &mut Window, cx: &mut App| {
                                    let _ = panel.update(cx, |panel, cx| {
                                        panel.close_hunk_peek(cx);
                                    });
                                }
                            }),
                    ),
            ),
    );
    card = if original.is_empty() {
        card.child(
            div()
                .px_3()
                .py_1p5()
                .text_size(px(11.))
                .text_color(muted)
                .child("Added lines \u{2014} no previous content"),
        )
    } else {
        let mono = cx.theme().mono_font_family.clone();
        let deleted = cx.theme().danger.opacity(0.08);
        card.child(
            v_flex()
                .id("git-peek-body")
                .py_1()
                .max_h(px(200.))
                .overflow_y_scroll()
                .font_family(mono)
                .text_size(px(12.))
                .text_color(cx.theme().foreground)
                .children(original.lines().map(|line| {
                    div()
                        .px_3()
                        .bg(deleted)
                        .whitespace_nowrap()
                        // An empty line still needs to occupy one row.
                        .child(SharedString::from(if line.is_empty() {
                            " ".to_string()
                        } else {
                            line.to_string()
                        }))
                })),
        )
    };
    h_flex()
        .on_mouse_down(MouseButton::Left, |_, _: &mut Window, cx: &mut App| {
            cx.stop_propagation();
        })
        .items_start()
        .my_1()
        .max_w(px(720.))
        .overflow_hidden()
        .rounded(px(4.))
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().popover)
        .text_color(cx.theme().popover_foreground)
        // Electron paints the card's left edge in `--destructive`; gpui has one
        // border color per element, so the accent is a strip instead.
        .child(
            div()
                .w(px(3.))
                .flex_shrink_0()
                .self_stretch()
                .bg(cx.theme().danger.opacity(0.6)),
        )
        .child(card)
        .into_any_element()
}

impl Render for FilesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Electron's `wordWrap` client setting, applied to the live buffer
        // (`EditorView.lineWrapping` in a compartment there, `set_soft_wrap`
        // here). The fork's editor defaults to wrapping, which is also the
        // contract default.
        if self.applied_word_wrap != self.word_wrap {
            self.applied_word_wrap = self.word_wrap;
            let wrap = self.word_wrap;
            self.editor
                .update(cx, |state, cx| state.set_soft_wrap(wrap, window, cx));
        }
        // A file that failed to open still holds the surface: breadcrumb,
        // toggle and aside behave as they do for a file that loaded.
        let file_open = self.open.is_some() || self.open_error.is_some();
        v_flex()
            .size_full()
            .on_action(cx.listener(|panel, _: &SaveFile, window, cx| {
                panel.save_now(window, cx);
            }))
            .on_action(cx.listener(|panel, _: &FormatDocument, window, cx| {
                panel.format_document(window, cx);
            }))
            .on_action(cx.listener(|panel, _: &GotoNextHunk, window, cx| {
                panel.goto_hunk(true, window, cx);
            }))
            .on_action(cx.listener(|panel, _: &GotoPreviousHunk, window, cx| {
                panel.goto_hunk(false, window, cx);
            }))
            .on_action(cx.listener(|panel, _: &GoToDefinition, window, cx| {
                let caret = panel.editor.read(cx).selected_range().start;
                panel.go_to_definition(caret, window, cx);
            }))
            .on_action(cx.listener(|panel, _: &ShowCompletions, window, cx| {
                panel.show_completions(window, cx);
            }))
            // Escape closes the peek, as it does in Electron's gutter keymap.
            // The input handles its own Escape first (completion, IME) and
            // propagates; with no peek open this is a no-op.
            .on_action(cx.listener(|panel, _: &Escape, _, cx| {
                panel.close_hunk_peek(cx);
            }))
            .on_action(cx.listener(|panel, action: &VimKeystroke, window, cx| {
                panel.vim_key(&action.key, window, cx);
            }))
            .on_action(cx.listener(|panel, _: &ToggleVimMode, _, cx| {
                ClientSettings::toggle_vim_mode(cx);
                panel.sync_client_settings(cx);
            }))
            .bg(cx.theme().background)
            .border_l_1()
            .border_color(cx.theme().border)
            .child(self.render_header(cx))
            .children(self.status.clone().map(|status| {
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(status)
            }))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .when(file_open, |this| this.child(self.render_editor_area(cx)))
                    // Hidden only while a file is open: with none open the
                    // aside is the whole panel, so Electron shows it whatever
                    // the toggle says.
                    .when(self.explorer_open || !file_open, |this| {
                        this.child(
                            // Electron: explorer aside is ~22rem with a left
                            // border while a file is open, and fills the panel
                            // when nothing is open.
                            div()
                                .h_full()
                                .min_h_0()
                                .map(|this| {
                                    if file_open {
                                        this.w(px(300.))
                                            .flex_shrink_0()
                                            .border_l_1()
                                            .border_color(cx.theme().border)
                                    } else {
                                        this.flex_1().min_w_0()
                                    }
                                })
                                .child(self.render_explorer(cx)),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use vitre_sidecar::SupervisorStatus;

    #[test]
    fn language_for_path_uses_extension_and_known_names() {
        assert_eq!(language_for_path("src/main.rs"), "rs");
        assert_eq!(language_for_path("a/b/Component.TSX"), "tsx");
        assert_eq!(language_for_path("Makefile"), "make");
        assert_eq!(language_for_path("LICENSE"), "text");
        assert_eq!(language_for_path(".gitignore"), "text");
    }

    /// A read that fails must not read as an error about whatever file happens
    /// to be on screen.
    ///
    /// This is the bug the `.mcp.json` report was: `open_file`'s failure arm
    /// wrote a panel-global status line and left `open` alone, so a failed open
    /// of a *restored* surface (a path recorded against another workspace root)
    /// painted "open failed: …" over an unrelated, perfectly healthy buffer,
    /// naming neither file. Electron swaps the surface to the path you asked
    /// for and replaces only the content area, which is what this asserts.
    #[gpui::test]
    fn a_failed_open_takes_over_the_surface_it_asked_for(cx: &mut TestAppContext) {
        // The panel fires a listing and a watch subscription on construction.
        // A supervisor that never reaches `Ready` parks both forever, which is
        // exactly what this test wants: the transition under test is driven by
        // hand, with no server in the loop.
        let runtime = tokio::runtime::Runtime::new().expect("a tokio runtime");
        let guard = runtime.enter();
        let (sidecar, status) = tokio::sync::watch::channel(SupervisorStatus::Idle);
        let client = Arc::new(EnvironmentClient::start(status));
        drop(guard);
        // The client is inert from here on: its background tasks are aborted,
        // so nothing ever wakes the panel's foreground tasks from a tokio
        // worker — which gpui's test scheduler rejects as non-deterministic.
        runtime.shutdown_background();

        cx.update(gpui_component::init);
        let (panel, cx) = cx.add_window_view(|window, cx| {
            FilesPanel::new(client, "/tmp/project".into(), window, cx)
        });

        // A file is open and showing its contents.
        cx.update(|window, cx| {
            panel.update(cx, |panel, cx| {
                panel.open = Some(OpenFile {
                    relative_path: "src/a.rs".into(),
                    buffer: FileBuffer::open(None),
                    truncated: false,
                    debounce: 0,
                });
                panel
                    .editor
                    .update(cx, |state, cx| state.set_value("fn a() {}", window, cx));
            });
        });

        // Another file's read fails.
        let message = "Failed to read workspace file 'src/b.rs' in '/tmp/project'.";
        cx.update(|window, cx| {
            panel.update(cx, |panel, cx| {
                panel.fail_open("src/b.rs".into(), message.into(), window, cx);
            });
        });

        panel.read_with(cx, |panel, cx| {
            assert!(
                panel.open.is_none(),
                "the previous file must not survive another file's failed open"
            );
            let failed = panel
                .open_error
                .as_ref()
                .expect("the failure is attributed to the path that failed");
            assert_eq!(failed.relative_path, "src/b.rs");
            assert_eq!(failed.message.as_ref(), message);
            assert!(
                panel.editor.read(cx).text().to_string().is_empty(),
                "the old buffer's text must go with it"
            );
            assert!(
                panel.status.is_none(),
                "a per-file failure is not a panel-wide status line"
            );
        });

        // And it is what the content area actually paints.
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            cx.debug_bounds("files-open-error").is_some(),
            "the server's message replaces the editor"
        );

        drop(sidecar);
    }
}
