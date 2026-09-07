//! M3 diff panel: the Electron `DiffPanel.tsx` / `DiffPanelShell.tsx` surface
//! as a dock panel — scope selection (working tree / branch range / turn
//! checkpoints), server-produced patch rendering, and the base-ref picker.
//!
//! Selection semantics live in [`vitre_state::diff_panel`] (a pure port of
//! `diffPanelStore.ts` + `baseRefChoices.ts`); patch parsing in
//! [`vitre_state::diff_patch`]. Like Electron, the panel does **no client-side
//! diffing**: git modes render `review.getDiffPreview` patch text, turn mode
//! renders `orchestration.getTurnDiff` / `getFullThreadDiff`.
//!
//! Deliberate Vitre deviations from the Electron panel, all matrix-noted:
//! working-tree/branch previews refetch on `subscribeVcsStatus` deltas instead
//! of a 5-second SWR staleTime; the base-ref picker is a flat menu (no search
//! input, no per-row remote Switch, no 5s ref repolling); diff rows are plain
//! mono text (no syntax highlighting); word-wrap off truncates long lines
//! instead of scrolling horizontally; no GitRootSwitcher (single-root only);
//! the external-editor open action is not yet ported. Review-comment
//! annotations (`AnnotatableCodeView`) are ported: drag over the line-number
//! gutter to select, comment drafts anchor under the selection's end row —
//! without Pierre's hover "+" gutter-utility button.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, ListAlignment, ListState, MouseButton,
    MouseMoveEvent, SharedString, Subscription, Window, div, list, prelude::*, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Selectable as _, Sizable as _,
    StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, Textarea, TextareaState},
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex,
};
use vitre_client::EnvironmentClient;
use vitre_contracts::methods::{
    OrchestrationGetFullThreadDiff, OrchestrationGetTurnDiff, ReviewGetDiffPreview,
    SubscribeVcsStatus, VcsListRefs,
};
use vitre_contracts::{
    NonNegativeInt, OrchestrationGetFullThreadDiffInput, OrchestrationTurnDiffRange,
    ReviewDiffPreviewInput, ReviewDiffPreviewResult, ReviewDiffPreviewSource,
    ReviewDiffPreviewSourceKind, ThreadId, TurnId, VcsListRefsInput, VcsListRefsInputRefKind,
    VcsRef, VcsStatusInput, VcsStatusLocalResult, VcsStatusStreamEvent,
};
use vitre_rpc::TypedStreamEvent;
use vitre_state::diff_panel::{
    DIFF_PANEL_PERSISTED_VERSION, DiffPanelMap, DiffPanelSelection, GitScope, TurnDiffSummary,
    build_base_ref_choices,
};
use vitre_state::diff_patch::{
    PatchFile, PatchFileKind, PatchLine, PatchLineKind, parse_unified_patch, unmodified_gap_before,
};
use vitre_state::review_comments::{
    DiffReviewCommentInput, ReviewCommentContext, SelectedLineRange, annotation_side_is_deletions,
    build_diff_review_comment, restore_diff_review_comment_range,
    selected_line_range_from_row_indices,
};

use crate::assets::VitreIcon;

use super::tnes;

pub(super) const FILE_NAME: &str = "diff-panel-state.json";

/// A server-completed status stream must not resubscribe in a hot loop
/// (mirrors vitre-client's `RESUBSCRIBE_AFTER_COMPLETION`).
const RESUBSCRIBE_AFTER_COMPLETION: Duration = Duration::from_secs(2);

/// Exact copy strings from the Electron shell (`DiffPanelShell.tsx` §2.9).
const NOTE_NO_THREAD: &str = "Select a thread to inspect turn diffs.";
const NOTE_NOT_A_REPO: &str =
    "Turn diffs are unavailable because this project is not a git repository.";
const NOTE_NO_TURNS: &str = "No completed turns yet.";
const NOTE_NO_NET_CHANGES: &str = "No net changes in this selection.";
const NOTE_NO_PATCH: &str = "No patch available for this selection.";
const NOTE_TRUNCATED: &str = "This diff was truncated because it exceeded the preview limit. The changes shown are incomplete.";
const RAW_REASON_UNSUPPORTED: &str = "Unsupported diff format. Showing raw patch.";

pub enum DiffPanelEvent {
    /// A file-header title was clicked: open it in the in-app editor
    /// (Electron's `openDiffFilePrimaryAction` with the default
    /// `openFilesInExternalEditor: false`).
    OpenFile { path: String },
    /// A draft annotation was submitted — Electron's
    /// `composerDraftStore.addReviewComment` (ChatApp owns the pending list).
    AddReviewComment(ReviewCommentContext),
    /// A persisted annotation's delete button — `removeReviewComment`.
    RemoveReviewComment { id: String },
}

/// One virtualized row of the diff body. Indices point into
/// [`DiffPanel::files`] / a file's hunks/lines, or [`DiffPanel::raw_lines`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffRow {
    File {
        file: usize,
    },
    Binary {
        file: usize,
    },
    Separator {
        gap: u32,
    },
    Line {
        file: usize,
        hunk: usize,
        line: usize,
    },
    SplitLine {
        file: usize,
        hunk: usize,
        left: Option<usize>,
        right: Option<usize>,
    },
    RawLine {
        line: usize,
    },
    /// Review-comment annotation group anchored under the diff row carrying
    /// this side-qualified line number (Pierre's `renderAnnotation` slot).
    Annotation {
        file: usize,
        deletions: bool,
        line: u32,
    },
}

/// One SWR-ish unary query slot: previous `data` stays renderable while a
/// refetch is loading or after it fails (Electron gotcha #6).
struct QuerySlot<K: PartialEq, T> {
    /// The input the current fetch (or settled result) corresponds to.
    key: Option<K>,
    data: Option<T>,
    error: Option<SharedString>,
    loading: bool,
}

impl<K: PartialEq, T> Default for QuerySlot<K, T> {
    fn default() -> Self {
        Self {
            key: None,
            data: None,
            error: None,
            loading: false,
        }
    }
}

pub struct DiffPanel {
    client: Arc<EnvironmentClient>,
    /// `${environmentId}:${threadId}` — the persistence key.
    thread_key: String,
    /// The git root queries run against: the thread's worktree, else its
    /// project's workspace root (ChatApp recreates the panel on change).
    cwd: String,
    /// `None` on the home pseudo-thread, where Electron shows no panel at all
    /// and Vitre shows the select-a-thread note.
    thread_id: Option<ThreadId>,
    map: DiffPanelMap,
    store_path: PathBuf,
    /// Ordered turn summaries (latest first), pushed in by ChatApp from the
    /// open thread's checkpoints.
    checkpoints: Vec<TurnDiffSummary>,
    // Header toggles: per-panel-instance, Electron's mount defaults.
    split: bool,
    word_wrap: bool,
    ignore_whitespace: bool,
    /// The `hasWorkingTreeChanges` value frozen when the first status
    /// snapshot arrived — Electron's `initialGitScope`, re-evaluated exactly
    /// once via the key-based remount when git status resolves (gotcha #1).
    frozen_has_changes: Option<bool>,
    /// Latest folded local git status; `is_repo` is optimistically true while
    /// the stream warms (gotcha #18).
    vcs_local: Option<VcsStatusLocalResult>,
    preview: QuerySlot<(Option<String>, bool), ReviewDiffPreviewResult>,
    preview_generation: u64,
    turn_diff: QuerySlot<(String, i64, bool), String>,
    turn_generation: u64,
    /// Base-ref combobox data, fetched once per preview-echoed cwd.
    refs_cwd: Option<String>,
    refs_generation: u64,
    refs_local: Vec<VcsRef>,
    refs_remote: Vec<VcsRef>,
    /// The patch text the rows were built from.
    patch_text: Option<String>,
    files: Vec<PatchFile>,
    /// Unparseable non-empty patch → raw fallback with this reason.
    raw_reason: Option<&'static str>,
    raw_lines: Vec<SharedString>,
    /// Collapse state is in-memory and scope-keyed: a scope switch expands
    /// everything (gotcha #14).
    collapse_scope: Option<String>,
    collapsed: HashSet<String>,
    rows: Vec<DiffRow>,
    list_state: ListState,
    /// Last applied reveal request id — scroll at most once per request, but
    /// keep retrying while the file has not appeared yet (gotcha #5).
    applied_reveal: Option<u64>,
    /// The composer's pending review comments, pushed in by ChatApp
    /// (Electron's `composerDraftStore` reviewComments slice).
    review_comments: Vec<ReviewCommentContext>,
    /// An in-flight gutter drag: anchor/head are flattened diff-review row
    /// indices within `files[file]`.
    gutter_drag: Option<GutterDrag>,
    /// The one open draft annotation (`AnnotatableCodeView`'s `draft`) —
    /// line selection is disabled while it exists.
    draft: Option<DraftAnnotation>,
    /// `nextFileCommentId` sequence half (paired with a timestamp).
    draft_seq: u64,
}

#[derive(Debug, Clone, Copy)]
struct GutterDrag {
    file: usize,
    anchor: usize,
    head: usize,
}

struct DraftAnnotation {
    /// `nextFileCommentId()` — becomes the comment id on submit.
    id: String,
    file_path: String,
    range: SelectedLineRange,
    range_label: String,
    /// Anchor row: `annotationSide(range)` + `range.end`.
    deletions: bool,
    line: u32,
    input: Entity<TextareaState>,
    _input_sub: Subscription,
}

impl EventEmitter<DiffPanelEvent> for DiffPanel {}

fn new_diff_list(count: usize) -> ListState {
    ListState::new(count, ListAlignment::Top, px(1000.))
}

impl DiffPanel {
    pub fn new(
        client: Arc<EnvironmentClient>,
        thread_key: String,
        cwd: String,
        thread_id: Option<ThreadId>,
        store_path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut panel = Self {
            client,
            thread_key,
            cwd,
            thread_id,
            map: load_map(&store_path),
            store_path,
            checkpoints: Vec::new(),
            split: false,
            word_wrap: true,
            ignore_whitespace: true,
            frozen_has_changes: None,
            vcs_local: None,
            preview: QuerySlot::default(),
            preview_generation: 0,
            turn_diff: QuerySlot::default(),
            turn_generation: 0,
            refs_cwd: None,
            refs_generation: 0,
            refs_local: Vec::new(),
            refs_remote: Vec::new(),
            patch_text: None,
            files: Vec::new(),
            raw_reason: None,
            raw_lines: Vec::new(),
            collapse_scope: None,
            collapsed: HashSet::new(),
            rows: Vec::new(),
            list_state: new_diff_list(0),
            applied_reveal: None,
            review_comments: Vec::new(),
            gutter_drag: None,
            draft: None,
            draft_seq: 0,
        };
        panel.spawn_vcs_status_loop(cx);
        panel.sync_queries(cx);
        panel.refresh_patch(cx);
        panel
    }

    pub fn thread_key(&self) -> &str {
        &self.thread_key
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// ChatApp pushes the thread's ordered turn summaries in whenever they
    /// change (per-frame while the surface is visible, so this must converge).
    pub fn set_checkpoints(&mut self, ordered: Vec<TurnDiffSummary>, cx: &mut Context<Self>) {
        if ordered == self.checkpoints {
            return;
        }
        self.checkpoints = ordered;
        // Store-level healing of a vanished turn (Electron reconciles in an
        // effect; the render-time fallback below covers the same frame).
        let available: Vec<TurnId> = self
            .checkpoints
            .iter()
            .map(|summary| summary.turn_id.clone())
            .collect();
        if self
            .map
            .reconcile_turn_selection(&self.thread_key, &available)
        {
            self.save_store();
        }
        self.sync_queries(cx);
        self.refresh_patch(cx);
    }

    /// ChatApp pushes the composer's pending review comments in whenever they
    /// change (per-frame while the surface is visible, so this must converge).
    pub fn set_review_comments(
        &mut self,
        comments: Vec<ReviewCommentContext>,
        cx: &mut Context<Self>,
    ) {
        if comments == self.review_comments {
            return;
        }
        self.review_comments = comments;
        self.rebuild_rows();
        cx.notify();
    }

    // ---- selection ----------------------------------------------------

    /// The stored selection with Electron's render-time healing: a turn that
    /// no longer exists renders as the latest available turn (gotcha #4).
    fn resolved_selection(&self) -> DiffPanelSelection {
        let selection = self
            .map
            .selection(&self.thread_key, self.frozen_has_changes.unwrap_or(false));
        if let DiffPanelSelection::Turn {
            turn_id,
            file_path,
            reveal_request_id,
        } = &selection
        {
            let known = self
                .checkpoints
                .iter()
                .any(|summary| summary.turn_id == *turn_id);
            if !known && let Some(latest) = self.checkpoints.first() {
                return DiffPanelSelection::Turn {
                    turn_id: latest.turn_id.clone(),
                    file_path: file_path.clone(),
                    reveal_request_id: *reveal_request_id,
                };
            }
        }
        selection
    }

    fn scope_label(&self, selection: &DiffPanelSelection) -> SharedString {
        match selection {
            DiffPanelSelection::Unstaged => "Working tree".into(),
            DiffPanelSelection::Branch { .. } => "Branch changes".into(),
            DiffPanelSelection::Turn { turn_id, .. } => {
                if self
                    .checkpoints
                    .first()
                    .is_some_and(|latest| latest.turn_id == *turn_id)
                {
                    return "Latest turn".into();
                }
                match self
                    .checkpoints
                    .iter()
                    .find(|summary| summary.turn_id == *turn_id)
                {
                    Some(summary) => format!("Turn {}", summary.turn_count).into(),
                    None => "Turn ?".into(),
                }
            }
        }
    }

    /// Electron's `reviewSectionTitle`: unlike `scope_label`, the latest turn
    /// is still titled `Turn {count}` (never "Latest turn").
    fn review_section_title(&self, selection: &DiffPanelSelection) -> String {
        match selection {
            DiffPanelSelection::Unstaged => "Working tree".into(),
            DiffPanelSelection::Branch { .. } => "Branch changes".into(),
            DiffPanelSelection::Turn { turn_id, .. } => {
                match self
                    .checkpoints
                    .iter()
                    .find(|summary| summary.turn_id == *turn_id)
                {
                    Some(summary) => format!("Turn {}", summary.turn_count),
                    None => "Turn ?".into(),
                }
            }
        }
    }

    /// Electron's `reviewSectionId`, which also keys the collapse scope.
    fn section_id(&self, selection: &DiffPanelSelection) -> String {
        match selection {
            DiffPanelSelection::Unstaged => "unstaged".into(),
            DiffPanelSelection::Branch { .. } => "branch".into(),
            DiffPanelSelection::Turn { turn_id, .. } => format!("turn:{}", turn_id.0),
        }
    }

    fn set_git_scope(&mut self, scope: GitScope, cx: &mut Context<Self>) {
        self.map.select_git_scope(&self.thread_key, scope);
        self.save_store();
        self.after_selection_change(cx);
    }

    fn set_turn(&mut self, turn_id: TurnId, cx: &mut Context<Self>) {
        self.map.select_turn(&self.thread_key, turn_id, None);
        self.save_store();
        self.after_selection_change(cx);
    }

    /// Turn selection arriving from the chat changed-files card. The bumped
    /// reveal request scrolls to `file_path` once the patch renders it.
    pub(super) fn open_turn(
        &mut self,
        turn_id: TurnId,
        file_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.map
            .select_turn(&self.thread_key, turn_id, file_path.as_deref());
        self.save_store();
        self.after_selection_change(cx);
    }

    fn set_base_ref(&mut self, base_ref: Option<String>, cx: &mut Context<Self>) {
        self.map
            .select_branch_base_ref(&self.thread_key, base_ref.as_deref());
        self.save_store();
        self.after_selection_change(cx);
    }

    fn after_selection_change(&mut self, cx: &mut Context<Self>) {
        self.sync_queries(cx);
        self.sync_refs(cx);
        self.refresh_patch(cx);
    }

    fn save_store(&self) {
        let mut value = self.map.to_persisted();
        value["version"] = DIFF_PANEL_PERSISTED_VERSION.into();
        let write = serde_json::to_string_pretty(&value)
            .map_err(std::io::Error::other)
            .and_then(|json| {
                if let Some(parent) = self.store_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&self.store_path, json)
            });
        if let Err(error) = write {
            eprintln!("[vitre] failed to persist {FILE_NAME}: {error}");
        }
    }

    // ---- queries ------------------------------------------------------

    /// (Re-)issue whichever unary query the current selection needs, keyed on
    /// its exact input so unchanged inputs never refetch.
    fn sync_queries(&mut self, cx: &mut Context<Self>) {
        match self.resolved_selection() {
            DiffPanelSelection::Turn { turn_id, .. } => self.fetch_turn_diff(&turn_id, cx),
            DiffPanelSelection::Branch { base_ref } => self.fetch_preview(base_ref, cx),
            DiffPanelSelection::Unstaged => self.fetch_preview(None, cx),
        }
    }

    fn fetch_preview(&mut self, base_ref: Option<String>, cx: &mut Context<Self>) {
        let key = (base_ref.clone(), self.ignore_whitespace);
        if self.preview.key.as_ref() == Some(&key) {
            return;
        }
        self.preview.key = Some(key);
        self.preview.loading = true;
        self.preview_generation += 1;
        let generation = self.preview_generation;
        let client = self.client.clone();
        let payload = ReviewDiffPreviewInput {
            // Automatic (None) omits the field entirely, like Electron.
            base_ref: base_ref.map(|base| Some(tnes(base))),
            cwd: tnes(&self.cwd),
            ignore_whitespace: Some(self.ignore_whitespace),
        };
        cx.spawn(async move |this, cx| {
            let mut result = client.call::<ReviewGetDiffPreview>(&payload).await;
            if let Err(error) = &result {
                // Electron's retry-at-server-cwd fallback is keyed on exactly
                // this error-message substring (gotcha #7).
                if format!("{error:?}").contains("configured workspace root") {
                    let server_cwd = client
                        .sessions()
                        .borrow()
                        .as_ref()
                        .map(|session| session.config.cwd.0.clone());
                    if let Some(server_cwd) = server_cwd.filter(|cwd| *cwd != payload.cwd.0) {
                        let retry = ReviewDiffPreviewInput {
                            cwd: tnes(server_cwd),
                            ..payload.clone()
                        };
                        result = client.call::<ReviewGetDiffPreview>(&retry).await;
                    }
                }
            }
            let result = result.map_err(|error| error.user_message());
            let _ = this.update(cx, |panel, cx| panel.apply_preview(generation, result, cx));
        })
        .detach();
    }

    fn apply_preview(
        &mut self,
        generation: u64,
        result: Result<ReviewDiffPreviewResult, String>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.preview_generation {
            return;
        }
        self.preview.loading = false;
        match result {
            Ok(data) => {
                self.preview.data = Some(data);
                self.preview.error = None;
                self.sync_refs(cx);
            }
            Err(message) => self.preview.error = Some(message.into()),
        }
        self.refresh_patch(cx);
    }

    fn fetch_turn_diff(&mut self, turn_id: &TurnId, cx: &mut Context<Self>) {
        let Some(thread_id) = self.thread_id.clone() else {
            return;
        };
        let Some(summary) = self
            .checkpoints
            .iter()
            .find(|summary| summary.turn_id == *turn_id)
        else {
            return;
        };
        let key = (
            summary.turn_id.0.clone(),
            summary.turn_count,
            self.ignore_whitespace,
        );
        if self.turn_diff.key.as_ref() == Some(&key) {
            return;
        }
        self.turn_diff.key = Some(key);
        self.turn_diff.loading = true;
        self.turn_generation += 1;
        let generation = self.turn_generation;
        let client = self.client.clone();
        let ignore_whitespace = self.ignore_whitespace;
        // Electron: `{ from: max(0, n - 1), to: n }`; turn 1 uses the
        // full-thread diff RPC because `fromTurnCount` 0 has no checkpoint.
        let to = summary.turn_count.max(0);
        let from = (to - 1).max(0);
        cx.spawn(async move |this, cx| {
            let result = if from == 0 {
                client
                    .call::<OrchestrationGetFullThreadDiff>(&OrchestrationGetFullThreadDiffInput {
                        ignore_whitespace: Some(ignore_whitespace),
                        thread_id,
                        to_turn_count: NonNegativeInt(to),
                    })
                    .await
                    .map(|result| result.diff.0)
                    .map_err(|error| error.user_message())
            } else {
                client
                    .call::<OrchestrationGetTurnDiff>(&OrchestrationTurnDiffRange {
                        from_turn_count: NonNegativeInt(from),
                        ignore_whitespace: Some(ignore_whitespace),
                        thread_id,
                        to_turn_count: NonNegativeInt(to),
                    })
                    .await
                    .map(|result| result.diff.0)
                    .map_err(|error| error.user_message())
            };
            let _ = this.update(cx, |panel, cx| {
                panel.apply_turn_diff(generation, result, cx)
            });
        })
        .detach();
    }

    fn apply_turn_diff(
        &mut self,
        generation: u64,
        result: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.turn_generation {
            return;
        }
        self.turn_diff.loading = false;
        match result {
            Ok(diff) => {
                self.turn_diff.data = Some(diff);
                self.turn_diff.error = None;
            }
            Err(message) => self.turn_diff.error = Some(message.into()),
        }
        self.refresh_patch(cx);
    }

    /// Fetch the base-ref lists once per preview-echoed cwd (Electron fires
    /// these only in branch mode, against `preview.data.cwd`, not the panel
    /// cwd — and repolls every 5s, which Vitre skips).
    fn sync_refs(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.resolved_selection(), DiffPanelSelection::Branch { .. }) {
            return;
        }
        let Some(cwd) = self.preview.data.as_ref().map(|data| data.cwd.0.clone()) else {
            return;
        };
        if self.refs_cwd.as_deref() == Some(cwd.as_str()) {
            return;
        }
        self.refs_cwd = Some(cwd.clone());
        self.refs_generation += 1;
        let generation = self.refs_generation;
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let input = |kind: VcsListRefsInputRefKind| VcsListRefsInput {
                cursor: None,
                cwd: tnes(&cwd),
                include_matching_remote_refs: Some(Some(true)),
                limit: Some(Some(100)),
                query: None,
                ref_kind: Some(Some(kind)),
            };
            let local = client
                .call::<VcsListRefs>(&input(VcsListRefsInputRefKind::Local))
                .await;
            let remote = client
                .call::<VcsListRefs>(&input(VcsListRefsInputRefKind::Remote))
                .await;
            let _ = this.update(cx, |panel, cx| {
                if generation != panel.refs_generation {
                    return;
                }
                // Failures are logged and swallowed, keeping the last lists
                // (Electron's caching-stream refresh semantics).
                match local {
                    Ok(result) => panel.refs_local = result.refs,
                    Err(error) => eprintln!("[vitre] vcs.listRefs local failed: {error}"),
                }
                match remote {
                    Ok(result) => panel.refs_remote = result.refs,
                    Err(error) => eprintln!("[vitre] vcs.listRefs remote failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ---- git status stream ---------------------------------------------

    /// Durable `subscribeVcsStatus` loop, the same session-watch shape as the
    /// files panel's workspace watcher.
    fn spawn_vcs_status_loop(&self, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let payload = VcsStatusInput {
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
                    .subscribe_typed::<SubscribeVcsStatus>(&payload)
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
                                    .update(cx, |panel, cx| panel.vcs_events(&events, cx))
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

    fn vcs_events(&mut self, events: &[VcsStatusStreamEvent], cx: &mut Context<Self>) {
        let mut local_changed = false;
        for event in events {
            match event {
                VcsStatusStreamEvent::Snapshot { local, .. }
                | VcsStatusStreamEvent::LocalUpdated { local } => {
                    self.vcs_local = Some(local.clone());
                    local_changed = true;
                }
                // The panel only consumes the local half.
                VcsStatusStreamEvent::RemoteUpdated { .. } | VcsStatusStreamEvent::Unknown(_) => {}
            }
        }
        if !local_changed {
            return;
        }
        // Freeze the default-scope decision on the first resolved status
        // (Electron re-mounts the panel exactly once for this).
        if self.frozen_has_changes.is_none() {
            self.frozen_has_changes = self
                .vcs_local
                .as_ref()
                .map(|local| local.has_working_tree_changes);
        }
        // Git-scope previews refetch on status deltas — Vitre's stand-in for
        // Electron's 5-second SWR staleTime on `review.getDiffPreview`.
        if !matches!(self.resolved_selection(), DiffPanelSelection::Turn { .. }) {
            self.preview.key = None;
            self.sync_queries(cx);
        }
        cx.notify();
    }

    fn is_repo(&self) -> bool {
        self.vcs_local
            .as_ref()
            .map(|local| local.is_repo)
            .unwrap_or(true)
    }

    // ---- patch → rows ---------------------------------------------------

    fn selected_source(&self) -> Option<&ReviewDiffPreviewSource> {
        let wanted = match self.resolved_selection() {
            DiffPanelSelection::Unstaged => ReviewDiffPreviewSourceKind::WorkingTree,
            DiffPanelSelection::Branch { .. } => ReviewDiffPreviewSourceKind::BranchRange,
            DiffPanelSelection::Turn { .. } => return None,
        };
        self.preview
            .data
            .as_ref()?
            .sources
            .iter()
            .find(|source| source.kind == wanted)
    }

    /// Re-derive the rendered patch from the current selection + slots. Safe
    /// to call after any state change; only reparses when the text changed.
    fn refresh_patch(&mut self, cx: &mut Context<Self>) {
        let selection = self.resolved_selection();
        let patch = match &selection {
            DiffPanelSelection::Turn { .. } => self.turn_diff.data.clone(),
            _ => self.selected_source().map(|source| source.diff.0.clone()),
        };
        let scope = self.section_id(&selection);
        let scope_changed = self.collapse_scope.as_deref() != Some(scope.as_str());
        if scope_changed {
            self.collapse_scope = Some(scope);
            self.collapsed.clear();
            // Electron remounts the CodeView per scope, resetting scroll.
            self.rows.clear();
            self.list_state = new_diff_list(0);
        }
        if scope_changed || patch != self.patch_text {
            self.patch_text = patch;
            self.files.clear();
            self.raw_reason = None;
            self.raw_lines.clear();
            if let Some(text) = self.patch_text.as_deref() {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let mut files = parse_unified_patch(trimmed);
                    if files.is_empty() {
                        // The Rust parser is total (it never throws), so
                        // Electron's second reason string — "Failed to parse
                        // patch." — is unreachable here.
                        self.raw_reason = Some(RAW_REASON_UNSUPPORTED);
                        self.raw_lines = trimmed
                            .lines()
                            .map(|line| line.to_string().into())
                            .collect();
                    } else {
                        // Electron sorts case-insensitively with numeric
                        // collation; Vitre skips the numeric part.
                        files.sort_by(|a, b| {
                            a.display_path()
                                .to_lowercase()
                                .cmp(&b.display_path().to_lowercase())
                        });
                        self.files = files;
                    }
                }
            }
            self.rebuild_rows();
        }
        self.apply_reveal(&selection);
        cx.notify();
    }

    fn rebuild_rows(&mut self) {
        let old = self.rows.len();
        let anchors = self.annotation_anchors();
        let mut rows = Vec::new();
        if self.raw_reason.is_some() {
            rows.extend((0..self.raw_lines.len()).map(|line| DiffRow::RawLine { line }));
        } else {
            for (file_ix, file) in self.files.iter().enumerate() {
                rows.push(DiffRow::File { file: file_ix });
                if self.collapsed.contains(file.display_path()) {
                    continue;
                }
                if file.binary {
                    rows.push(DiffRow::Binary { file: file_ix });
                    continue;
                }
                for (hunk_ix, hunk) in file.hunks.iter().enumerate() {
                    let gap = unmodified_gap_before(file, hunk_ix);
                    if gap > 0 {
                        rows.push(DiffRow::Separator { gap });
                    }
                    if self.split {
                        for row in split_hunk_rows(file_ix, hunk_ix, &hunk.lines) {
                            rows.push(row);
                            let DiffRow::SplitLine { left, right, .. } = row else {
                                continue;
                            };
                            if let Some(line) = left.and_then(|ix| hunk.lines.get(ix)) {
                                push_line_annotations(&mut rows, &anchors, file_ix, line);
                            }
                            if right != left
                                && let Some(line) = right.and_then(|ix| hunk.lines.get(ix))
                            {
                                push_line_annotations(&mut rows, &anchors, file_ix, line);
                            }
                        }
                    } else {
                        for (line_ix, line) in hunk.lines.iter().enumerate() {
                            rows.push(DiffRow::Line {
                                file: file_ix,
                                hunk: hunk_ix,
                                line: line_ix,
                            });
                            push_line_annotations(&mut rows, &anchors, file_ix, line);
                        }
                    }
                }
            }
        }
        self.rows = rows;
        self.list_state.splice(0..old, self.rows.len());
    }

    /// Anchor set for annotation rows — persisted comments restored against
    /// the current patch plus the open draft, keyed
    /// `(file index, deletions side, end line number)` like Pierre groups
    /// annotations per `(side, lineNumber)`.
    fn annotation_anchors(&self) -> HashSet<(usize, bool, u32)> {
        let mut anchors = HashSet::new();
        if self.review_comments.is_empty() && self.draft.is_none() {
            return anchors;
        }
        let section = self.section_id(&self.resolved_selection());
        for (file_ix, file) in self.files.iter().enumerate() {
            let path = file.display_path();
            for comment in &self.review_comments {
                if comment.section_id != section
                    || comment.file_path != path
                    || comment.fence_language.as_deref().unwrap_or("diff") != "diff"
                {
                    continue;
                }
                if let Some(range) = restore_diff_review_comment_range(file, comment) {
                    anchors.insert((file_ix, annotation_side_is_deletions(&range), range.end));
                }
            }
            if let Some(draft) = &self.draft
                && draft.file_path == path
            {
                anchors.insert((file_ix, draft.deletions, draft.line));
            }
        }
        anchors
    }

    // ---- review-comment annotations -------------------------------------

    /// `enableLineSelection: !hasOpenComment` — one draft at a time.
    fn gutter_selection_enabled(&self) -> bool {
        self.draft.is_none()
    }

    fn gutter_mouse_down(&mut self, file: usize, hunk: usize, line: usize, cx: &mut Context<Self>) {
        if !self.gutter_selection_enabled() {
            return;
        }
        let Some(flat) = self
            .files
            .get(file)
            .map(|file| flat_row_index(file, hunk, line))
        else {
            return;
        };
        self.gutter_drag = Some(GutterDrag {
            file,
            anchor: flat,
            head: flat,
        });
        cx.notify();
    }

    fn gutter_mouse_move(
        &mut self,
        file: usize,
        hunk: usize,
        line: usize,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if self.gutter_drag.is_none() {
            return;
        }
        // The release landed outside the panel: abandon the drag.
        if event.pressed_button != Some(MouseButton::Left) {
            self.gutter_drag = None;
            cx.notify();
            return;
        }
        let Some(flat) = self
            .files
            .get(file)
            .map(|file| flat_row_index(file, hunk, line))
        else {
            return;
        };
        if let Some(drag) = &mut self.gutter_drag
            && drag.file == file
            && drag.head != flat
        {
            drag.head = flat;
            cx.notify();
        }
    }

    /// Pierre's `onLineSelectionEnd` → `beginComment`.
    fn finish_gutter_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.gutter_drag.take() else {
            return;
        };
        let Some(range) = self
            .files
            .get(drag.file)
            .and_then(|file| selected_line_range_from_row_indices(file, drag.anchor, drag.head))
        else {
            cx.notify();
            return;
        };
        self.begin_comment(drag.file, range, window, cx);
    }

    /// `beginComment`: open a draft annotation under the selection's end row.
    fn begin_comment(
        &mut self,
        file_ix: usize,
        range: SelectedLineRange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(file) = self.files.get(file_ix) else {
            cx.notify();
            return;
        };
        let file_path = file.display_path().to_string();
        // `nextFileCommentId()`.
        self.draft_seq += 1;
        let id = format!(
            "file-comment-{}-{}",
            chrono::Utc::now().timestamp_millis(),
            self.draft_seq
        );
        let selection = self.resolved_selection();
        let section_id = self.section_id(&selection);
        let section_title = self.review_section_title(&selection);
        // Built with empty text only for the range label (Electron does the
        // same to validate the range resolves).
        let Some(seed) = build_diff_review_comment(&DiffReviewCommentInput {
            id: &id,
            section_id: &section_id,
            section_title: &section_title,
            file_path: &file_path,
            file_diff: file,
            range,
            text: "",
        }) else {
            cx.notify();
            return;
        };
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Request change")
                .auto_grow(2, 6)
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        // Cmd+Enter submits like Electron's textarea keydown; Change re-renders
        // the Comment button's disabled state.
        let input_sub = cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter {
                    secondary: true, ..
                } => this.submit_draft(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        );
        self.draft = Some(DraftAnnotation {
            id,
            file_path,
            range,
            range_label: seed.range_label,
            deletions: annotation_side_is_deletions(&range),
            line: range.end,
            input,
            _input_sub: input_sub,
        });
        self.rebuild_rows();
        cx.notify();
    }

    /// `submitEntry`: rebuild the comment with the final text and hand it to
    /// the composer.
    fn submit_draft(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = &self.draft else {
            return;
        };
        let text = draft.input.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        let id = draft.id.clone();
        let file_path = draft.file_path.clone();
        let range = draft.range;
        let selection = self.resolved_selection();
        let comment = self
            .files
            .iter()
            .find(|file| file.display_path() == file_path)
            .and_then(|file| {
                build_diff_review_comment(&DiffReviewCommentInput {
                    id: &id,
                    section_id: &self.section_id(&selection),
                    section_title: &self.review_section_title(&selection),
                    file_path: &file_path,
                    file_diff: file,
                    range,
                    text: &text,
                })
            });
        self.draft = None;
        if let Some(comment) = comment {
            cx.emit(DiffPanelEvent::AddReviewComment(comment));
        }
        self.rebuild_rows();
        cx.notify();
    }

    /// `removeEntry`: a draft cancels locally; a persisted comment removes
    /// from the composer store (ChatApp pushes the shrunken list back).
    fn remove_annotation_entry(&mut self, entry_id: &str, cx: &mut Context<Self>) {
        if self
            .draft
            .as_ref()
            .is_some_and(|draft| draft.id == entry_id)
        {
            self.draft = None;
            self.rebuild_rows();
            cx.notify();
            return;
        }
        cx.emit(DiffPanelEvent::RemoveReviewComment {
            id: entry_id.to_string(),
        });
    }

    /// Scroll to the deep-linked file at most once per reveal request; keep
    /// retrying while the file has not appeared in the parsed patch yet.
    fn apply_reveal(&mut self, selection: &DiffPanelSelection) {
        let DiffPanelSelection::Turn {
            file_path: Some(path),
            reveal_request_id,
            ..
        } = selection
        else {
            return;
        };
        if self.applied_reveal == Some(*reveal_request_id) {
            return;
        }
        let target = self.rows.iter().position(|row| {
            matches!(row, DiffRow::File { file }
                if self.files[*file].display_path() == path)
        });
        if let Some(index) = target {
            self.applied_reveal = Some(*reveal_request_id);
            self.list_state.scroll_to_reveal_item(index);
        }
    }

    fn toggle_collapsed(&mut self, path: &str, cx: &mut Context<Self>) {
        if !self.collapsed.remove(path) {
            self.collapsed.insert(path.to_string());
        }
        self.rebuild_rows();
        cx.notify();
    }

    /// Electron's `toggleAllDiffFiles`: everything collapsed → expand all,
    /// otherwise collapse all.
    fn toggle_all_collapsed(&mut self, cx: &mut Context<Self>) {
        if self.files.is_empty() {
            return;
        }
        if self.all_collapsed() {
            self.collapsed.clear();
        } else {
            self.collapsed = self
                .files
                .iter()
                .map(|file| file.display_path().to_string())
                .collect();
        }
        self.rebuild_rows();
        cx.notify();
    }

    fn all_collapsed(&self) -> bool {
        !self.files.is_empty()
            && self
                .files
                .iter()
                .all(|file| self.collapsed.contains(file.display_path()))
    }

    fn set_split(&mut self, split: bool, cx: &mut Context<Self>) {
        if self.split == split {
            return;
        }
        self.split = split;
        self.rebuild_rows();
        cx.notify();
    }

    fn toggle_word_wrap(&mut self, cx: &mut Context<Self>) {
        self.word_wrap = !self.word_wrap;
        // Row heights change; splicing the whole range re-measures them.
        let len = self.rows.len();
        self.list_state.splice(0..len, len);
        cx.notify();
    }

    fn toggle_ignore_whitespace(&mut self, cx: &mut Context<Self>) {
        self.ignore_whitespace = !self.ignore_whitespace;
        // Each toggle value is a distinct query key (gotcha #11): the slot
        // keys embed it, so this refetches rather than re-filtering.
        self.sync_queries(cx);
        cx.notify();
    }

    // ---- render: header -------------------------------------------------

    fn render_header(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let selection = self.resolved_selection();
        let mut left = h_flex().gap_3().min_w_0().flex_1().items_center();
        left = left.child(self.render_scope_dropdown(&selection, cx));
        if let DiffPanelSelection::Branch { .. } = &selection
            && let Some(base) = self
                .selected_source()
                .and_then(|source| source.base_ref.clone())
        {
            let head: SharedString = self
                .selected_source()
                .and_then(|source| source.head_ref.clone())
                .map(|head| head.0)
                .unwrap_or_else(|| "HEAD".into())
                .into();
            left = left.child(
                h_flex()
                    .gap_2()
                    .min_w_0()
                    .items_center()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(div().max_w(px(192.)).truncate().child(head))
                    .child(
                        Icon::new(IconName::ArrowRight)
                            .size_3()
                            .text_color(cx.theme().muted_foreground.opacity(0.7)),
                    )
                    .child(self.render_base_ref_dropdown(base.0, cx)),
            );
        }

        let mut right = h_flex().gap_1().flex_shrink_0().items_center();
        let has_files = !self.files.is_empty() && self.raw_reason.is_none();
        if has_files {
            let all_collapsed = self.all_collapsed();
            right = right.child(
                Button::new("diff-collapse-all")
                    .icon(if all_collapsed {
                        Icon::new(IconName::ChevronsUpDown).size_3()
                    } else {
                        Icon::new(VitreIcon::ChevronsDownUp).size_3()
                    })
                    .ghost()
                    .xsmall()
                    .tooltip(if all_collapsed {
                        "Expand all files"
                    } else {
                        "Collapse all files"
                    })
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.toggle_all_collapsed(cx);
                    })),
            );
        }
        right = right
            .child(
                Button::new("diff-view-stacked")
                    .icon(Icon::new(VitreIcon::Rows3).size_3())
                    .ghost()
                    .xsmall()
                    .selected(!self.split)
                    .tooltip("Stacked diff view")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.set_split(false, cx);
                    })),
            )
            .child(
                Button::new("diff-view-split")
                    .icon(Icon::new(VitreIcon::Columns2).size_3())
                    .ghost()
                    .xsmall()
                    .selected(self.split)
                    .tooltip("Split diff view")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.set_split(true, cx);
                    })),
            )
            .child(
                Button::new("diff-word-wrap")
                    .icon(Icon::new(VitreIcon::WrapText).size_3())
                    .ghost()
                    .xsmall()
                    .selected(self.word_wrap)
                    .tooltip(if self.word_wrap {
                        "Disable line wrapping"
                    } else {
                        "Enable line wrapping"
                    })
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.toggle_word_wrap(cx);
                    })),
            )
            .child(
                Button::new("diff-whitespace")
                    .icon(Icon::new(VitreIcon::Pilcrow).size_3())
                    .ghost()
                    .xsmall()
                    .selected(self.ignore_whitespace)
                    .tooltip(if self.ignore_whitespace {
                        "Show whitespace changes"
                    } else {
                        "Hide whitespace changes"
                    })
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.toggle_ignore_whitespace(cx);
                    })),
            );

        // Electron's embedded `surface-subheader`: h-10, bottom border.
        h_flex()
            .h(px(40.))
            .flex_shrink_0()
            .px_4()
            .gap_2()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.6))
            .child(left)
            .child(right)
            .into_any_element()
    }

    fn render_scope_dropdown(
        &self,
        selection: &DiffPanelSelection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.scope_label(selection);
        let panel = cx.entity().downgrade();
        let summaries = self.checkpoints.clone();
        let latest = summaries.first().map(|summary| summary.turn_id.clone());
        let selected_turn = match selection {
            DiffPanelSelection::Turn { turn_id, .. } => Some(turn_id.clone()),
            _ => None,
        };
        let is_unstaged = matches!(selection, DiffPanelSelection::Unstaged);
        let is_branch = matches!(selection, DiffPanelSelection::Branch { .. });
        Button::new("diff-scope")
            .label(label)
            .icon(Icon::new(IconName::ChevronDown).size_3())
            .ghost()
            .xsmall()
            .dropdown_menu(move |menu, window, cx| {
                let unstaged_panel = panel.clone();
                let branch_panel = panel.clone();
                let latest_panel = panel.clone();
                let latest_turn = latest.clone();
                let is_latest = selected_turn.is_some() && selected_turn == latest;
                let submenu_panel = panel.clone();
                let submenu_summaries = summaries.clone();
                let submenu_selected = selected_turn.clone();
                menu.item(
                    PopupMenuItem::new("Working tree")
                        .checked(is_unstaged)
                        .on_click(move |_, _, cx| {
                            let _ = unstaged_panel.update(cx, |panel, cx| {
                                panel.set_git_scope(GitScope::Unstaged, cx);
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new("Branch changes")
                        .checked(is_branch)
                        .on_click(move |_, _, cx| {
                            let _ = branch_panel.update(cx, |panel, cx| {
                                panel.set_git_scope(GitScope::Branch, cx);
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new("Latest turn")
                        .checked(is_latest)
                        .disabled(latest_turn.is_none())
                        .on_click(move |_, _, cx| {
                            let Some(turn_id) = latest_turn.clone() else {
                                return;
                            };
                            let _ = latest_panel.update(cx, |panel, cx| {
                                panel.set_turn(turn_id, cx);
                            });
                        }),
                )
                .submenu("Turn", window, cx, move |mut menu, _, _| {
                    for summary in &submenu_summaries {
                        let item_panel = submenu_panel.clone();
                        let turn_id = summary.turn_id.clone();
                        let checked = submenu_selected.as_ref() == Some(&summary.turn_id);
                        let label: SharedString = format!("Turn {}", summary.turn_count).into();
                        let timestamp: SharedString =
                            format_short_timestamp(&summary.completed_at).into();
                        menu = menu.item(
                            PopupMenuItem::element(move |_, cx| {
                                h_flex()
                                    .w_full()
                                    .gap_3()
                                    .items_center()
                                    .justify_between()
                                    .child(label.clone())
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(timestamp.clone()),
                                    )
                            })
                            .checked(checked)
                            .on_click(move |_, _, cx| {
                                let turn_id = turn_id.clone();
                                let _ = item_panel.update(cx, |panel, cx| {
                                    panel.set_turn(turn_id, cx);
                                });
                            }),
                        );
                    }
                    menu
                })
            })
            .into_any_element()
    }

    /// Vitre's flat take on Electron's searchable base-ref combobox: the
    /// trigger shows the server-resolved ref (gotcha #16); the menu offers
    /// Automatic plus the paired local/remote choices.
    fn render_base_ref_dropdown(&self, displayed: String, cx: &mut Context<Self>) -> AnyElement {
        let panel = cx.entity().downgrade();
        let head_ref = self
            .selected_source()
            .and_then(|source| source.head_ref.as_ref())
            .map(|head| head.0.clone());
        // Local refs exclude the current head — you can't diff a branch
        // against itself (gotcha #9).
        let locals: Vec<VcsRef> = self
            .refs_local
            .iter()
            .filter(|reference| Some(&reference.name.0) != head_ref.as_ref())
            .cloned()
            .collect();
        let choices = build_base_ref_choices(&locals, &self.refs_remote);
        let stored = match self
            .map
            .selection(&self.thread_key, self.frozen_has_changes.unwrap_or(false))
        {
            DiffPanelSelection::Branch { base_ref } => base_ref,
            _ => None,
        };
        Button::new("diff-base-ref")
            .label(SharedString::from(displayed))
            .icon(Icon::new(IconName::ChevronDown).size_3())
            .ghost()
            .xsmall()
            .dropdown_menu(move |mut menu, _, _| {
                menu = menu.scrollable(true).max_h(px(320.));
                let automatic_panel = panel.clone();
                menu = menu.item(
                    PopupMenuItem::new("Automatic")
                        .checked(stored.is_none())
                        .on_click(move |_, _, cx| {
                            let _ = automatic_panel.update(cx, |panel, cx| {
                                panel.set_base_ref(None, cx);
                            });
                        }),
                );
                for choice in &choices {
                    // Selecting picks the local name when one exists, else the
                    // full remote name — Electron's `valueForBaseRefChoice`
                    // collapsed to a flat list.
                    let value = choice
                        .local
                        .as_ref()
                        .map(|local| local.name.0.clone())
                        .or_else(|| choice.remote.as_ref().map(|remote| remote.name.0.clone()));
                    let Some(value) = value else {
                        continue;
                    };
                    let checked = stored.as_deref() == Some(value.as_str());
                    let item_panel = panel.clone();
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(choice.label.clone()))
                            .checked(checked)
                            .on_click(move |_, _, cx| {
                                let value = value.clone();
                                let _ = item_panel.update(cx, |panel, cx| {
                                    panel.set_base_ref(Some(value), cx);
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    // ---- render: body ---------------------------------------------------

    fn render_body(&mut self, cx: &mut Context<Self>) -> AnyElement {
        // Electron's ordered render decision (§2.9). Vitre extension: on the
        // home pseudo-thread (Electron has no panel there at all) the git
        // scopes still work against the project root; only turn mode needs an
        // open thread.
        let selection = self.resolved_selection();
        let turn_mode = matches!(selection, DiffPanelSelection::Turn { .. });
        if turn_mode && self.thread_id.is_none() {
            return centered_note(NOTE_NO_THREAD, cx);
        }
        if !self.is_repo() {
            return centered_note(NOTE_NOT_A_REPO, cx);
        }
        if turn_mode && self.checkpoints.is_empty() {
            return centered_note(NOTE_NO_TURNS, cx);
        }

        let (loading, error) = if turn_mode {
            (self.turn_diff.loading, self.turn_diff.error.clone())
        } else {
            (self.preview.loading, self.preview.error.clone())
        };
        let has_render = !self.rows.is_empty();

        let mut column = v_flex().size_full().min_h_0().min_w_0();
        if !turn_mode
            && self
                .selected_source()
                .is_some_and(|source| source.truncated)
        {
            column = column.child(
                div()
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.7))
                    .bg(cx.theme().muted.opacity(0.4))
                    .px_3()
                    .py_1p5()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(NOTE_TRUNCATED),
            );
        }
        if has_render {
            if let Some(reason) = self.raw_reason {
                column = column.child(
                    div()
                        .px_3()
                        .py_1p5()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground.opacity(0.75))
                        .child(reason),
                );
            }
            return column
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        // A release outside the number gutter still ends the
                        // drag (bubble phase — gutter cells handle their own).
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.finish_gutter_selection(window, cx);
                            }),
                        )
                        .child(
                            list(
                                self.list_state.clone(),
                                cx.processor(|this, index, _window, cx| this.render_row(index, cx)),
                            )
                            .size_full(),
                        ),
                )
                .into_any_element();
        }

        // No renderable patch: the RPC error may show (gotcha #6), then a
        // skeleton or the empty-state copy.
        if let Some(error) = error {
            column = column.child(
                div()
                    .px_3()
                    .pt_2()
                    .text_xs()
                    .text_color(cx.theme().danger.opacity(0.8))
                    .child(error),
            );
        }
        if loading {
            let label = match &selection {
                DiffPanelSelection::Turn { .. } => "Loading checkpoint diff...",
                DiffPanelSelection::Unstaged => "Loading working tree diff...",
                DiffPanelSelection::Branch { .. } => "Loading branch diff...",
            };
            return column.child(loading_skeleton(label, cx)).into_any_element();
        }
        let message = if self
            .patch_text
            .as_deref()
            .is_some_and(|patch| patch.trim().is_empty())
        {
            NOTE_NO_NET_CHANGES
        } else {
            NOTE_NO_PATCH
        };
        column
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.7))
                    .child(message),
            )
            .into_any_element()
    }

    fn render_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.rows.get(index).copied() else {
            return div().into_any_element();
        };
        match row {
            DiffRow::File { file } => self.render_file_row(file, cx),
            DiffRow::Binary { .. } => div()
                .w_full()
                .px_3()
                .py_2()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Binary file not shown.")
                .into_any_element(),
            DiffRow::Separator { gap } => {
                let label = if gap == 1 {
                    "1 unmodified line".to_string()
                } else {
                    format!("{gap} unmodified lines")
                };
                div()
                    .w_full()
                    .h(px(24.))
                    .px_3()
                    .flex()
                    .items_center()
                    .bg(cx.theme().foreground.opacity(0.05))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label)
                    .into_any_element()
            }
            DiffRow::Line { file, hunk, line } => self.render_unified_line(file, hunk, line, cx),
            DiffRow::Annotation {
                file,
                deletions,
                line,
            } => self.render_annotation_row(file, deletions, line, cx),
            DiffRow::SplitLine {
                file,
                hunk,
                left,
                right,
            } => self.render_split_line(file, hunk, left, right, cx),
            DiffRow::RawLine { line } => div()
                .w_full()
                .px_3()
                .min_h(px(16.))
                .font_family(cx.theme().mono_font_family.clone())
                .text_size(px(11.))
                .line_height(px(16.))
                .text_color(cx.theme().muted_foreground.opacity(0.9))
                .when(!self.word_wrap, |this| this.truncate())
                .child(
                    self.raw_lines
                        .get(line)
                        .cloned()
                        .unwrap_or_else(|| "".into()),
                )
                .into_any_element(),
        }
    }

    fn render_file_row(&mut self, file_ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(file) = self.files.get(file_ix) else {
            return div().into_any_element();
        };
        let path = file.display_path().to_string();
        let collapsed = self.collapsed.contains(&path);
        let (accent, change_icon) = match file.kind {
            PatchFileKind::Added => (cx.theme().success, IconName::Plus),
            PatchFileKind::Deleted => (cx.theme().danger, IconName::Minus),
            // Electron's modified/renamed base color is blue.
            PatchFileKind::Modified => (cx.theme().info, IconName::SquarePen),
            PatchFileKind::Renamed => (cx.theme().info, IconName::ArrowRight),
        };
        let (deletions_label, additions_label) = count_labels(file.additions, file.deletions);
        let old_path = (file.kind == PatchFileKind::Renamed)
            .then(|| file.old_path.clone())
            .flatten();

        let toggle_path = path.clone();
        let open_path = path.clone();
        let mut left = h_flex()
            .gap_1p5()
            .min_w_0()
            .flex_1()
            .items_center()
            .child(
                Button::new(SharedString::from(format!("diff-collapse-{file_ix}")))
                    .icon(
                        if collapsed {
                            Icon::new(IconName::ChevronRight).size_4()
                        } else {
                            Icon::new(IconName::ChevronDown).size_4()
                        }
                        .text_color(accent),
                    )
                    .ghost()
                    .xsmall()
                    .tooltip(if collapsed {
                        "Expand diff"
                    } else {
                        "Collapse diff"
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.toggle_collapsed(&toggle_path, cx);
                    })),
            )
            .child(Icon::new(change_icon).size_3p5().text_color(accent));
        if let Some(old) = old_path {
            left = left
                .child(
                    div()
                        .max_w(px(160.))
                        .truncate()
                        .text_color(cx.theme().muted_foreground)
                        .child(old),
                )
                .child(
                    Icon::new(IconName::ArrowRight)
                        .size_3()
                        .text_color(cx.theme().muted_foreground),
                );
        }
        left = left.child(
            div()
                .id(SharedString::from(format!("diff-file-path-{file_ix}")))
                .min_w_0()
                .truncate()
                .cursor_pointer()
                .text_color(cx.theme().foreground)
                .hover(|style| style.text_color(cx.theme().info))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.emit(DiffPanelEvent::OpenFile {
                        path: open_path.clone(),
                    });
                    let _ = this;
                }))
                .child(SharedString::from(path)),
        );

        let mut counts = h_flex()
            .gap_2()
            .flex_shrink_0()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(px(11.));
        if let Some(deletions) = deletions_label {
            counts = counts.child(
                div()
                    .text_color(cx.theme().danger)
                    .child(SharedString::from(deletions)),
            );
        }
        if let Some(additions) = additions_label {
            counts = counts.child(
                div()
                    .text_color(cx.theme().success)
                    .child(SharedString::from(additions)),
            );
        }

        h_flex()
            .w_full()
            .min_h(px(32.))
            .px_2()
            .gap_2()
            .items_center()
            .justify_between()
            // Electron's header strip: card mixed 94% toward foreground; the
            // Vitre theme's closest surface token is `muted`.
            .bg(cx.theme().muted)
            .border_b_1()
            .border_color(cx.theme().border)
            .text_size(px(12.))
            .child(left)
            .child(counts)
            .into_any_element()
    }

    /// Whether an active gutter drag covers this row (Pierre's selected-line
    /// highlight).
    fn row_in_drag_selection(&self, file_ix: usize, hunk_ix: usize, line_ix: usize) -> bool {
        self.gutter_drag.as_ref().is_some_and(|drag| {
            drag.file == file_ix
                && self.files.get(file_ix).is_some_and(|file| {
                    let flat = flat_row_index(file, hunk_ix, line_ix);
                    (drag.anchor.min(drag.head)..=drag.anchor.max(drag.head)).contains(&flat)
                })
        })
    }

    /// Number cells with the selection mouse handlers attached — Pierre's
    /// selectable gutter (both cells in unified mode, one per half in split).
    fn gutter_cells(
        &self,
        file_ix: usize,
        hunk_ix: usize,
        line_ix: usize,
        numbers: &[Option<u32>],
        number_bg: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut gutter = h_flex()
            .items_stretch()
            .flex_shrink_0()
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                this.gutter_mouse_move(file_ix, hunk_ix, line_ix, event, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.finish_gutter_selection(window, cx)),
            );
        if self.gutter_selection_enabled() {
            gutter = gutter.cursor_pointer().on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.gutter_mouse_down(file_ix, hunk_ix, line_ix, cx);
                }),
            );
        }
        for number in numbers {
            gutter = gutter.child(line_number_cell(*number, number_bg, cx));
        }
        gutter.into_any_element()
    }

    fn render_unified_line(
        &self,
        file_ix: usize,
        hunk_ix: usize,
        line_ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(line) = self
            .files
            .get(file_ix)
            .and_then(|file| file.hunks.get(hunk_ix))
            .and_then(|hunk| hunk.lines.get(line_ix))
        else {
            return div().into_any_element();
        };
        let (mut row_bg, number_bg, bar) = line_colors(line.kind, cx);
        if self.row_in_drag_selection(file_ix, hunk_ix, line_ix) {
            row_bg = cx.theme().info.opacity(0.12);
        }
        let gutter = self.gutter_cells(
            file_ix,
            hunk_ix,
            line_ix,
            &[line.old_line, line.new_line],
            number_bg,
            cx,
        );
        h_flex()
            .w_full()
            .items_stretch()
            .min_h(px(20.))
            .bg(row_bg)
            .child(gutter)
            .child(div().w(px(3.)).flex_shrink_0().bg(bar))
            .child(self.line_content(line, cx))
            .into_any_element()
    }

    fn render_split_line(
        &self,
        file_ix: usize,
        hunk_ix: usize,
        left: Option<usize>,
        right: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(hunk) = self
            .files
            .get(file_ix)
            .and_then(|file| file.hunks.get(hunk_ix))
        else {
            return div().into_any_element();
        };
        let side = |line_ix: Option<usize>, old_side: bool, cx: &mut Context<Self>| {
            let Some((line_ix, line)) = line_ix.and_then(|ix| hunk.lines.get(ix).map(|l| (ix, l)))
            else {
                // Pierre's empty "buffer" half opposite an unpaired change.
                return div()
                    .flex_1()
                    .min_w_0()
                    .bg(cx.theme().foreground.opacity(0.03))
                    .into_any_element();
            };
            let (mut row_bg, number_bg, bar) = line_colors(line.kind, cx);
            if self.row_in_drag_selection(file_ix, hunk_ix, line_ix) {
                row_bg = cx.theme().info.opacity(0.12);
            }
            let number = if old_side {
                line.old_line
            } else {
                line.new_line
            };
            let gutter = self.gutter_cells(file_ix, hunk_ix, line_ix, &[number], number_bg, cx);
            h_flex()
                .flex_1()
                .min_w_0()
                .items_stretch()
                .bg(row_bg)
                .child(gutter)
                .child(div().w(px(3.)).flex_shrink_0().bg(bar))
                .child(self.line_content(line, cx))
                .into_any_element()
        };
        let left_side = side(left, true, cx);
        let right_side = side(right, false, cx);
        h_flex()
            .w_full()
            .items_stretch()
            .min_h(px(20.))
            .child(left_side)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_stretch()
                    .border_l_1()
                    .border_color(cx.theme().border.opacity(0.6))
                    .child(right_side),
            )
            .into_any_element()
    }

    /// The annotation group under one diff row — every persisted comment
    /// whose restored range ends here, then the open draft
    /// (`renderAnnotation` in AnnotatableCodeView).
    fn render_annotation_row(
        &mut self,
        file_ix: usize,
        deletions: bool,
        line: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(file) = self.files.get(file_ix) else {
            return div().into_any_element();
        };
        let path = file.display_path().to_string();
        let section = self.section_id(&self.resolved_selection());
        let comments: Vec<ReviewCommentContext> = self
            .review_comments
            .iter()
            .filter(|comment| {
                comment.section_id == section
                    && comment.file_path == path
                    && comment.fence_language.as_deref().unwrap_or("diff") == "diff"
                    && restore_diff_review_comment_range(file, comment).is_some_and(|range| {
                        annotation_side_is_deletions(&range) == deletions && range.end == line
                    })
            })
            .cloned()
            .collect();
        let draft_here = self.draft.as_ref().is_some_and(|draft| {
            draft.file_path == path && draft.deletions == deletions && draft.line == line
        });
        let mut column = v_flex().w_full().py_1();
        for comment in comments {
            column = column.child(self.render_comment_card(comment, cx));
        }
        if draft_here {
            column = column.child(self.render_draft_card(cx));
        }
        column.into_any_element()
    }

    /// `LocalCommentAnnotation` kind="comment": the saved-comment card.
    fn render_comment_card(
        &self,
        comment: ReviewCommentContext,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entry_id = comment.id.clone();
        v_flex()
            .mx_3()
            .my_2()
            .p_3()
            .rounded(px(12.))
            .border_1()
            .border_color(cx.theme().border.opacity(0.7))
            .bg(cx.theme().background)
            .shadow_sm()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Icon::new(VitreIcon::MessageCircle)
                            .size_4()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(div().text_xs().font_medium().child("Local comment"))
                    .child(
                        div()
                            .ml_auto()
                            .text_size(px(11.))
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(comment.range_label.clone())),
                    )
                    .child(
                        Button::new(SharedString::from(format!("annotation-delete-{entry_id}")))
                            .icon(Icon::new(VitreIcon::Trash2).size_3p5())
                            .ghost()
                            .xsmall()
                            .tooltip("Delete comment")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.remove_annotation_entry(&entry_id, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .mt_2()
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child(SharedString::from(comment.text)),
            )
            .into_any_element()
    }

    /// `LocalCommentAnnotation` kind="draft": textarea + Cancel/Comment.
    fn render_draft_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(draft) = &self.draft else {
            return div().into_any_element();
        };
        let can_comment = !draft.input.read(cx).value().trim().is_empty();
        let cancel_id = draft.id.clone();
        let escape_id = draft.id.clone();
        v_flex()
            .mx_3()
            .my_2()
            .p_3()
            .rounded(px(12.))
            .border_1()
            .border_color(cx.theme().border.opacity(0.7))
            .bg(cx.theme().background)
            .shadow_lg()
            // Escape cancels (bubbles up from the textarea when unhandled).
            .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.remove_annotation_entry(&escape_id, cx);
                }
            }))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Icon::new(VitreIcon::MessageCircle)
                            .size_4()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(div().text_sm().font_medium().child("Local comment")),
            )
            .child(
                div()
                    .mt_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(format!(
                        "Comment on lines {}",
                        draft.range_label
                    ))),
            )
            .child(div().mt_3().child(Textarea::new(&draft.input)))
            .child(
                h_flex()
                    .mt_3()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("annotation-cancel")
                            .label("Cancel")
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.remove_annotation_entry(&cancel_id, cx);
                            })),
                    )
                    .child(
                        Button::new("annotation-comment")
                            .label("Comment")
                            .primary()
                            .small()
                            .disabled(!can_comment)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.submit_draft(window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    fn line_content(&self, line: &PatchLine, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .px_2()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(px(13.))
            .line_height(px(20.))
            .text_color(cx.theme().foreground)
            // Word-wrap off truncates instead of scrolling horizontally — a
            // Vitre simplification (gpui's `list` has no horizontal overflow).
            .when(!self.word_wrap, |this| this.truncate())
            .child(SharedString::from(line.content.clone()))
            .into_any_element()
    }
}

impl Render for DiffPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(cx.theme().background)
            .child(self.render_header(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(self.render_body(cx)),
            )
    }
}

// ---- pure helpers -------------------------------------------------------

fn load_map(path: &Path) -> DiffPanelMap {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return DiffPanelMap::default();
    };
    match serde_json::from_str::<serde_json::Value>(&contents) {
        Ok(value) => DiffPanelMap::from_persisted(&value),
        Err(error) => {
            eprintln!("[vitre] ignoring unreadable {FILE_NAME}: {error}");
            DiffPanelMap::default()
        }
    }
}

/// Flattened diff-review row index of a `(hunk, line)` pair — the coordinate
/// space `selected_line_range_from_row_indices` consumes.
fn flat_row_index(file: &PatchFile, hunk_ix: usize, line_ix: usize) -> usize {
    file.hunks[..hunk_ix.min(file.hunks.len())]
        .iter()
        .map(|hunk| hunk.lines.len())
        .sum::<usize>()
        + line_ix
}

/// Push the annotation rows anchored under `line` (deletions match Del rows'
/// old numbers, additions match any row's new number — the two sides
/// `getDiffReviewSelectionPoint` can produce).
fn push_line_annotations(
    rows: &mut Vec<DiffRow>,
    anchors: &HashSet<(usize, bool, u32)>,
    file: usize,
    line: &PatchLine,
) {
    if line.kind == PatchLineKind::Del
        && let Some(old) = line.old_line
        && anchors.contains(&(file, true, old))
    {
        rows.push(DiffRow::Annotation {
            file,
            deletions: true,
            line: old,
        });
    }
    if let Some(new) = line.new_line
        && anchors.contains(&(file, false, new))
    {
        rows.push(DiffRow::Annotation {
            file,
            deletions: false,
            line: new,
        });
    }
}

/// Pair a hunk's deletion/addition runs side-by-side, Pierre's split layout:
/// context rows mirror themselves; each change segment zips its deletions
/// against its additions by index.
fn split_hunk_rows(file: usize, hunk: usize, lines: &[PatchLine]) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].kind == PatchLineKind::Context {
            rows.push(DiffRow::SplitLine {
                file,
                hunk,
                left: Some(index),
                right: Some(index),
            });
            index += 1;
            continue;
        }
        let mut deletions = Vec::new();
        let mut additions = Vec::new();
        while index < lines.len() && lines[index].kind == PatchLineKind::Del {
            deletions.push(index);
            index += 1;
        }
        while index < lines.len() && lines[index].kind == PatchLineKind::Add {
            additions.push(index);
            index += 1;
        }
        for pair in 0..deletions.len().max(additions.len()) {
            rows.push(DiffRow::SplitLine {
                file,
                hunk,
                left: deletions.get(pair).copied(),
                right: additions.get(pair).copied(),
            });
        }
    }
    rows
}

/// Electron's header-count edge rules (gotcha #19): `-D` renders when
/// `D > 0 || A == 0`, `+A` when `A > 0 || D == 0` — pure adds show only `+A`,
/// pure deletes only `-D`, a no-op file shows both zeros.
fn count_labels(additions: usize, deletions: usize) -> (Option<String>, Option<String>) {
    let deletions_label = (deletions > 0 || additions == 0).then(|| format!("-{deletions}"));
    let additions_label = (additions > 0 || deletions == 0).then(|| format!("+{additions}"));
    (deletions_label, additions_label)
}

/// Row colors approximating Electron's `color-mix` formulas (§3.7): additions
/// mix ~8% success into the background, deletions ~8% destructive, with the
/// number gutter mixed slightly further.
fn line_colors(
    kind: PatchLineKind,
    cx: &mut Context<DiffPanel>,
) -> (gpui::Hsla, gpui::Hsla, gpui::Hsla) {
    match kind {
        PatchLineKind::Add => (
            cx.theme().success.opacity(0.08),
            cx.theme().success.opacity(0.12),
            cx.theme().success.opacity(0.55),
        ),
        PatchLineKind::Del => (
            cx.theme().danger.opacity(0.08),
            cx.theme().danger.opacity(0.12),
            cx.theme().danger.opacity(0.55),
        ),
        PatchLineKind::Context => (
            gpui::transparent_black(),
            cx.theme().foreground.opacity(0.02),
            gpui::transparent_black(),
        ),
    }
}

fn line_number_cell(
    number: Option<u32>,
    bg: gpui::Hsla,
    cx: &mut Context<DiffPanel>,
) -> AnyElement {
    div()
        .w(px(44.))
        .flex_shrink_0()
        .pr_1p5()
        .bg(bg)
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(px(11.))
        .line_height(px(20.))
        .text_color(cx.theme().muted_foreground.opacity(0.8))
        .text_right()
        .child(SharedString::from(
            number.map(|n| n.to_string()).unwrap_or_default(),
        ))
        .into_any_element()
}

fn centered_note(message: &'static str, cx: &mut Context<DiffPanel>) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .px_5()
        .text_center()
        .text_xs()
        .text_color(cx.theme().muted_foreground.opacity(0.7))
        .child(message)
        .into_any_element()
}

/// Electron's `DiffPanelLoadingState`: a skeleton card with a fake header row
/// and five placeholder lines (the label is screen-reader-only there, so it
/// stays invisible here too — `label` is kept for parity documentation).
fn loading_skeleton(label: &'static str, cx: &mut Context<DiffPanel>) -> AnyElement {
    let _ = label;
    let bar = |width: f32, cx: &mut Context<DiffPanel>| {
        div()
            .h(px(12.))
            .w(gpui::relative(width))
            .rounded_full()
            .bg(cx.theme().muted_foreground.opacity(0.15))
    };
    v_flex()
        .flex_1()
        .min_h_0()
        .p_2()
        .child(
            v_flex()
                .rounded(px(6.))
                .border_1()
                .border_color(cx.theme().border.opacity(0.6))
                .bg(cx.theme().muted.opacity(0.25))
                .child(
                    h_flex()
                        .justify_between()
                        .border_b_1()
                        .border_color(cx.theme().border.opacity(0.5))
                        .px_3()
                        .py_2()
                        .child(
                            div()
                                .h(px(16.))
                                .w(px(128.))
                                .rounded_full()
                                .bg(cx.theme().muted_foreground.opacity(0.15)),
                        )
                        .child(
                            div()
                                .h(px(16.))
                                .w(px(80.))
                                .rounded_full()
                                .bg(cx.theme().muted_foreground.opacity(0.15)),
                        ),
                )
                .child(
                    v_flex()
                        .gap_4()
                        .px_3()
                        .py_4()
                        .child(bar(1., cx))
                        .child(bar(1., cx))
                        .child(bar(10. / 12., cx))
                        .child(bar(11. / 12., cx))
                        .child(bar(9. / 12., cx)),
                ),
        )
        .into_any_element()
}

/// Compact timestamp for the Turn submenu: time-of-day for today, otherwise
/// month + day + time (an approximation of Electron's `formatShortTimestamp`,
/// which also honours the user's 12/24h setting).
fn format_short_timestamp(iso: &str) -> String {
    let Ok(when) = chrono::DateTime::parse_from_rfc3339(iso) else {
        return iso.to_string();
    };
    let local = when.with_timezone(&chrono::Local);
    if local.date_naive() == chrono::Local::now().date_naive() {
        local.format("%H:%M").to_string()
    } else {
        local.format("%b %-d, %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: PatchLineKind) -> PatchLine {
        PatchLine {
            kind,
            content: String::new(),
            old_line: None,
            new_line: None,
        }
    }

    #[test]
    fn split_rows_zip_change_runs_and_mirror_context() {
        use PatchLineKind::*;
        let lines = vec![
            line(Context),
            line(Del),
            line(Del),
            line(Add),
            line(Context),
            line(Add),
        ];
        let rows = split_hunk_rows(0, 0, &lines);
        assert_eq!(
            rows,
            vec![
                DiffRow::SplitLine {
                    file: 0,
                    hunk: 0,
                    left: Some(0),
                    right: Some(0)
                },
                DiffRow::SplitLine {
                    file: 0,
                    hunk: 0,
                    left: Some(1),
                    right: Some(3)
                },
                DiffRow::SplitLine {
                    file: 0,
                    hunk: 0,
                    left: Some(2),
                    right: None
                },
                DiffRow::SplitLine {
                    file: 0,
                    hunk: 0,
                    left: Some(4),
                    right: Some(4)
                },
                DiffRow::SplitLine {
                    file: 0,
                    hunk: 0,
                    left: None,
                    right: Some(5)
                },
            ]
        );
    }

    #[test]
    fn count_labels_follow_electron_edge_rules() {
        assert_eq!(count_labels(3, 2), (Some("-2".into()), Some("+3".into())));
        assert_eq!(count_labels(3, 0), (None, Some("+3".into())));
        assert_eq!(count_labels(0, 2), (Some("-2".into()), None));
        assert_eq!(count_labels(0, 0), (Some("-0".into()), Some("+0".into())));
    }
}
