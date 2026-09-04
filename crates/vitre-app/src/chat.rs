//! M1 chat core UI: thread list sidebar + chat view + composer, rendered
//! from the `vitre-client` watch channels.
//!
//! Detail views merge SHELL data with the detail projection: `subscribeThread`
//! only carries the six detail event kinds, so title/meta always come from
//! the thread shell (the TS client's `mergeEnvironmentThread` split).

mod changed_files;
mod diff_panel;
mod project_actions;
mod right_panel;
mod sidebar;
mod terminal_drawer;
mod terminal_view;

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash as _, Hasher as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use gpui::{
    AnyElement, Context, Entity, FocusHandle, FollowMode, ListAlignment, ListState,
    PathPromptOptions, SharedString, Subscription, Window, actions, div, list, prelude::*, px,
    relative,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, ResizableState, Root, Sizable as _,
    StyledExt as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex, h_resizable,
    input::{InputEvent, Textarea, TextareaState},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    progress::ProgressCircle,
    resizable_panel,
    text::TextView,
    v_flex,
};
use gpui_tokio::Tokio;
use tokio::sync::watch;
use vitre_client::{EnvironmentClient, ShellState, SyncPhase, ThreadHandle, ThreadState};
use vitre_contracts::ClientOrchestrationCommandThreadTurnStartMessageAttachments as TurnAttachment;
use vitre_contracts::methods::ProjectsSearchEntries;
use vitre_contracts::{
    ApprovalRequestId, ClientOrchestrationCommand, CommandId, ExecutionEnvironmentPlatformOs,
    MessageId, ModelSelection, NonNegativeInt, OrchestrationMessage, OrchestrationMessageRole,
    OrchestrationSessionStatus, OrchestrationThread, OrchestrationThreadActivity,
    OrchestrationThreadActivityTone, OrchestrationThreadShell, ProjectEntry, ProjectEntryKind,
    ProjectId, ProjectSearchEntriesInput, ProviderApprovalDecision, ProviderInteractionMode,
    RuntimeMode, ServerConfig, ThreadId, TrimmedNonEmptyString,
};
use vitre_sidecar::SupervisorStatus;
use vitre_state::diff_panel::ordered_turn_diff_summaries;
use vitre_state::session_logic::{
    ActivePlanState, ApprovalRequestKind, PendingApproval, PendingUserInput, PlanStepStatus,
    derive_active_plan_state, derive_latest_context_window_snapshot, derive_pending_approvals,
    derive_pending_user_inputs,
};

use crate::files::FilesPanel;
use crate::palette::command_palette::{
    CommandPalette, CommandPaletteEvent, PaletteAction, PaletteContext,
};
pub use crate::palette::quick_search::QuickSearchMode;
use crate::palette::quick_search::{QuickSearch, QuickSearchEvent};
use crate::sidebar_prefs::SidebarPrefs;

actions!(
    vitre,
    [
        /// Electron's `quickSearch.open` (`mod+p`): jump to a chat or file.
        QuickSearchOpen,
        /// Electron's `quickSearch.content` (`mod+shift+f`): search chat and
        /// file contents.
        QuickSearchContent,
        /// Electron's `commandPalette.toggle` (`mod+shift+p`).
        CommandPaletteToggle,
        /// Electron's `chat.new` (`mod+shift+o`): start a thread in the
        /// contextual project.
        NewThread,
        /// Electron's `rightPanel.toggle` (`mod+j` / `mod+alt+b`).
        RightPanelToggle,
        /// Electron's `rightPanel.closeSurface` (`mod+w`).
        RightPanelCloseSurface,
        /// Electron's `rightPanel.nextSurface` (`mod+shift+]`).
        RightPanelNextSurface,
        /// Electron's `rightPanel.previousSurface` (`mod+shift+[`).
        RightPanelPreviousSurface,
        /// Electron's `terminal.toggle` (`` ctrl+` `` / `mod+r`): the bottom
        /// terminal drawer for the open thread.
        TerminalToggle,
        /// Terminal-context `mod+d`: split the active group to the right.
        TerminalSplit,
        /// Terminal-context `mod+shift+d`: split the active group downward.
        TerminalSplitVertical,
        /// Terminal-context `mod+n` / `mod+t`: new terminal tab.
        TerminalNew,
        /// Terminal-context `mod+w`: close the active terminal.
        TerminalCloseActive,
    ]
);

pub struct ChatApp {
    client: Option<Arc<EnvironmentClient>>,
    sidecar_status: SharedString,
    shell: ShellState,
    thread: Option<OpenThread>,
    composer: Entity<TextareaState>,
    last_error: Option<SharedString>,
    /// Request ids with an approval / user-input response in flight.
    responding: HashSet<String>,
    /// Local draft state for the active pending user-input request.
    input_draft: Option<InputDraft>,
    /// Files staged to send with the next turn (data-URL attachments).
    pending_attachments: Vec<PendingAttachment>,
    /// Active `@`-mention autocomplete in the composer, if any.
    mention: Option<MentionState>,
    /// Monotonic mention-search counter; stale results are dropped.
    mention_generation: u64,
    /// Two-step revert confirm: the user message armed for revert.
    pending_revert: Option<MessageId>,
    /// A `ThreadCheckpointRevert` is in flight.
    reverting: bool,
    /// Activity (tool) rows expanded to show their payload detail.
    expanded_activities: HashSet<String>,
    /// Changed-files card state: persisted expansion (`~/.vitre/ui-state.json`)
    /// plus per-turn local UI (auto-expand decision, folder toggles).
    changed_files: changed_files::ChangedFilesState,
    /// `VITRE_OPEN_THREAD=<thread-id>`: select this thread as soon as the
    /// shell carries it, then forget it. Verification hook for sandboxed runs
    /// where synthetic clicks are dropped (docs/vitre-parity.md QA recipe).
    debug_open_thread: Option<String>,
    /// The virtualized timeline. Rows vary wildly in height (markdown bodies,
    /// expandable tool detail), so this is gpui's `list`, which measures rows
    /// lazily as they scroll in — not `uniform_list` or gpui-component's
    /// `VirtualList`, both of which need every row's height up front.
    /// `FollowMode::Tail` supplies the stick-to-bottom-while-streaming
    /// behaviour Electron hand-rolls in `timelineScrollAnchoring.ts`.
    timeline_list: ListState,
    /// Row descriptors backing `timeline_list`, rebuilt when the thread view
    /// changes rather than on every frame.
    timeline: Vec<TimelineRow>,
    /// Height fingerprint per timeline row, parallel to `timeline`.
    timeline_hashes: Vec<u64>,
    /// Revert target per user message, rebuilt alongside `timeline`.
    revert_turn_counts: HashMap<MessageId, i64>,
    /// Persisted sidebar preferences: grouping mode + per-project overrides,
    /// sort orders, preview count, expansion, manual order, visit stamps.
    /// Electron splits these across `ClientSettings` and a browser-local UI
    /// store; neither is server state, so Vitre keeps its own file.
    sidebar: SidebarPrefs,
    /// Derived sidebar rows, rebuilt on shell/selection/preference changes
    /// rather than per frame — grouping walks every project and thread.
    sidebar_rows: Vec<sidebar::SidebarProjectRow>,
    /// Physical project keys in on-screen order, pre-grouping: what a manual
    /// reorder drag rewrites.
    sidebar_project_order: Vec<String>,
    /// The open "Rename project" dialog, if any. Held here because the
    /// dialog's content closure only keeps a weak reference to this view.
    project_rename: Option<project_actions::ProjectRenameDialog>,
    /// The open "Project grouping" dialog, held for the same reason.
    project_grouping: Option<project_actions::ProjectGroupingDialog>,
    /// Sidebar ⟷ chat ⟷ right-panel split state (drag-resizable).
    sidebar_resize: Entity<ResizableState>,
    /// Right-hand files panel (M2), hosting the dock's `files`/`file`
    /// surfaces. Kept alive while hidden so tree expansion and the open
    /// buffer survive, recreated on project switch.
    files: Option<Entity<FilesPanel>>,
    /// Diff dock surface (M3), recreated when the dock's thread key or the
    /// active git root changes. `None` until first shown.
    diff: Option<Entity<diff_panel::DiffPanel>>,
    /// Open-file subscription on the current diff panel; replaced on recreate.
    diff_subscription: Option<Subscription>,
    /// Where the diff panel persists its per-thread selections.
    diff_store: PathBuf,
    /// Right-panel dock state (per-thread surfaces) + persisted panel width.
    right_panel: right_panel::RightPanelPrefs,
    /// Bottom terminal drawer: persisted per-thread UI state (tabs, groups,
    /// height) plus the session-only suppression map.
    terminal_ui: terminal_drawer::TerminalPrefs,
    /// Live drawer terminal panes, keyed `(threadKey, terminalId)`. Cleared on
    /// thread switch — Electron unmounts the drawer's xterms the same way
    /// (sessions live on server-side).
    terminal_views: HashMap<(String, String), Entity<terminal_view::TerminalView>>,
    /// Session-exited subscriptions, parallel to `terminal_views`.
    terminal_view_subs: HashMap<(String, String), Subscription>,
    /// `subscribeTerminalMetadata` fold: every session the environment knows,
    /// MRU-ordered (never used for tab order — see the drawer's reconcile).
    terminal_metadata: Vec<vitre_contracts::TerminalSummary>,
    /// An in-flight drag on the drawer's resize handle.
    terminal_drag: Option<terminal_drawer::TerminalDrag>,
    /// Window height as of the last frame, for the drawer's height clamp
    /// (`render_chat` has no `Window` access).
    viewport_height: f32,
    /// File-surface reveal requests already applied, keyed
    /// `${threadKey}|${surfaceId}` → `reveal_request_id`.
    applied_reveals: HashMap<String, u64>,
    /// The thread key the dock last synced for, to catch thread switches.
    last_dock_sync_key: Option<String>,
    /// The open QuickSearch overlay. Held here because the dialog's content
    /// closure only keeps a weak reference to it.
    quick_search: Option<Entity<QuickSearch>>,
    /// The open command palette, held for the same reason.
    command_palette: Option<Entity<CommandPalette>>,
    /// The shell's own focus. Actions dispatch along the focus path, and with
    /// nothing focused gpui falls back to the window's root node — which is
    /// above this view, so the palette shortcuts would reach no handler.
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

/// Draft answers for the front pending user-input request, keyed by its
/// request id so a new request starts clean (Electron keys drafts the same
/// way, per request id).
struct InputDraft {
    request_id: String,
    question_index: usize,
    selections: HashMap<String, Vec<String>>,
}

/// A file staged in the composer, already encoded as the wire's data-URL
/// attachment shape (attachments are inlined into `ThreadTurnStart`).
struct PendingAttachment {
    name: String,
    mime_type: String,
    size_bytes: i64,
    data_url: String,
    is_image: bool,
}

/// The composer's active `@token`. Mentions are plain text on the wire
/// (`@path` / `@"path with spaces"`, parsed server-side from the message
/// text), so autocomplete only has to insert text at the token.
struct MentionState {
    /// Byte offset of the `@` in the composer text.
    token_start: usize,
    /// Text typed after the `@`.
    query: String,
    /// Generation of the search whose results are shown / awaited.
    generation: u64,
    results: Vec<ProjectEntry>,
}

struct OpenThread {
    id: ThreadId,
    /// Keeps the sync task alive; dropped (aborted) on reselection.
    _handle: ThreadHandle,
    state: ThreadState,
}

pub(crate) fn tnes(text: impl Into<String>) -> TrimmedNonEmptyString {
    TrimmedNonEmptyString(text.into())
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn fresh_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    )
}

/// Default model for a new thread: the first enabled+installed provider's
/// default model (else its first model). Used when the project carries no
/// `default_model_selection` of its own.
fn default_model_selection(config: &ServerConfig) -> Option<ModelSelection> {
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.enabled && provider.installed)
        .or_else(|| config.providers.first())?;
    let model = provider
        .models
        .iter()
        .find(|model| matches!(model.is_default, Some(Some(true))))
        .or_else(|| provider.models.first())?;
    Some(ModelSelection {
        instance_id: Some(Some(serde_json::Value::String(
            provider.instance_id.0.clone(),
        ))),
        model: serde_json::Value::String(model.slug.0.clone()),
        options: None,
        provider: None,
    })
}

/// Compact relative timestamp for sidebar rows ("now", "5m", "2h", "3d").
fn relative_time(iso: &str) -> Option<String> {
    let then = chrono::DateTime::parse_from_rfc3339(iso).ok()?;
    let delta = chrono::Utc::now().signed_duration_since(then.with_timezone(&chrono::Utc));
    Some(if delta.num_minutes() < 1 {
        "now".into()
    } else if delta.num_hours() < 1 {
        format!("{}m", delta.num_minutes())
    } else if delta.num_days() < 1 {
        format!("{}h", delta.num_hours())
    } else {
        format!("{}d", delta.num_days())
    })
}

/// The `@token` the cursor sits at the end of, if any: byte offset of the
/// `@` plus the query typed after it. Mirrors the Electron composer trigger:
/// `@` at start-of-text or after whitespace, no spaces/quotes/`@` inside the
/// typed query (`packages/shared/src/composerInlineTokens.ts`).
fn active_mention_token(text: &str, cursor: usize) -> Option<(usize, String)> {
    let head = text.get(..cursor)?;
    let start = head
        .rfind(char::is_whitespace)
        .map(|index| index + head[index..].chars().next().map_or(1, char::len_utf8))
        .unwrap_or(0);
    let query = head[start..].strip_prefix('@')?;
    if query.contains('"') || query.contains('@') {
        return None;
    }
    Some((start, query.to_string()))
}

/// Attachment limits, from `packages/contracts/src/orchestration.ts`.
const MAX_ATTACHMENTS: usize = 8;
const MAX_ATTACHMENT_BYTES: u64 = 10 * 1024 * 1024;

/// Extension-keyed MIME inference (`inferAttachmentMimeType` port — with no
/// browser-reported type in a native app, the extension map is the whole
/// story; unknown extensions fall back to octet-stream).
fn infer_attachment_mime_type(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map(|(_, ext)| ext.to_lowercase());
    match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        Some("c") | Some("h") => "text/x-c",
        Some("cjs") | Some("js") | Some("mjs") => "text/javascript",
        Some("cpp") | Some("hpp") => "text/x-c++",
        Some("cs") => "text/x-csharp",
        Some("css") => "text/css",
        Some("csv") => "text/csv",
        Some("go") => "text/x-go",
        Some("html") => "text/html",
        Some("java") => "text/x-java",
        Some("json") => "application/json",
        Some("jsx") => "text/jsx",
        Some("kt") => "text/x-kotlin",
        Some("log") | Some("txt") | Some("svelte") | Some("vue") => "text/plain",
        Some("md") | Some("markdown") => "text/markdown",
        Some("pdf") => "application/pdf",
        Some("php") => "text/x-php",
        Some("py") => "text/x-python",
        Some("rb") => "text/x-ruby",
        Some("rs") => "text/x-rust",
        Some("sh") => "text/x-shellscript",
        Some("sql") => "application/sql",
        Some("swift") => "text/x-swift",
        Some("toml") => "application/toml",
        Some("ts") | Some("tsx") => "application/typescript",
        Some("xml") => "application/xml",
        Some("yaml") | Some("yml") => "application/yaml",
        _ => "application/octet-stream",
    }
}

/// Compact "512 B" / "84 KB" / "9.6 MB" for attachment chips.
fn format_attachment_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Compact token counts for the context meter — "842", "8.4k", "84k", "1.2m"
/// (`formatContextWindowTokens` port).
fn format_context_tokens(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    let one_decimal = |scaled: f64, suffix: &str| {
        let text = format!("{scaled:.1}");
        format!("{}{suffix}", text.strip_suffix(".0").unwrap_or(&text))
    };
    if value < 1_000.0 {
        format!("{}", value.round() as i64)
    } else if value < 10_000.0 {
        one_decimal(value / 1_000.0, "k")
    } else if value < 1_000_000.0 {
        format!("{}k", (value / 1_000.0).round() as i64)
    } else {
        one_decimal(value / 1_000_000.0, "m")
    }
}

/// "7.5%" below ten percent, "42%" above (`formatPercentage` port).
fn format_context_percentage(value: f64) -> String {
    if value < 10.0 {
        let text = format!("{value:.1}");
        format!("{}%", text.strip_suffix(".0").unwrap_or(&text))
    } else {
        format!("{}%", value.round() as i64)
    }
}

/// Render a path as a composer mention token (quoted when it has spaces).
fn format_mention(path: &str) -> String {
    if path.chars().any(char::is_whitespace) {
        format!("@\"{path}\" ")
    } else {
        format!("@{path} ")
    }
}

fn activity_icon(kind: &str) -> IconName {
    if kind.contains("terminal") || kind.contains("bash") || kind.contains("shell") {
        IconName::SquareTerminal
    } else if kind.contains("web") || kind.contains("fetch") || kind.contains("url") {
        IconName::Globe
    } else if kind.contains("read") || kind.contains("view") || kind.contains("search") {
        IconName::Eye
    } else if kind.contains("edit") || kind.contains("write") || kind.contains("file") {
        IconName::File
    } else {
        IconName::Settings2
    }
}

/// Expanded tool-row detail: a well-known string payload field when present
/// (command/detail/preview/text/path), else the pretty-printed payload.
fn activity_detail(activity: &OrchestrationThreadActivity) -> Option<String> {
    const MAX_LEN: usize = 4000;
    let payload = activity.payload.as_object()?;
    if payload.is_empty() {
        return None;
    }
    let text = ["command", "detail", "preview", "text", "path"]
        .iter()
        .find_map(|key| payload.get(*key).and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .or_else(|| serde_json::to_string_pretty(&activity.payload).ok())?;
    if text.trim().is_empty() {
        return None;
    }
    let mut text = text;
    if text.len() > MAX_LEN {
        let mut end = MAX_LEN;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
    Some(text)
}

/// One row of the virtualized timeline: a position in the thread view's
/// `messages` or `activities`, or the trailing typing indicator.
///
/// Rows hold indices rather than borrows so the order can be cached on the
/// entity across frames; they are rebuilt whenever the view changes, which is
/// the only time the underlying vectors can move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TimelineRow {
    Message(usize),
    Activity(usize),
    Running,
}

/// Overdraw roughly a viewport of rows so scrolling doesn't pop.
fn new_timeline_list() -> ListState {
    let state = ListState::new(0, ListAlignment::Bottom, px(800.));
    state.set_follow_mode(FollowMode::Tail);
    state
}

/// A turn's timeline interleaves messages and activity (tool) rows in
/// creation order, exactly like the Electron `MessagesTimeline`, with the
/// typing indicator as a trailing row so it participates in virtualization.
fn timeline_rows(view: &OrchestrationThread, running: bool) -> Vec<TimelineRow> {
    let mut entries: Vec<(&str, TimelineRow)> = view
        .messages
        .iter()
        .enumerate()
        .map(|(index, message)| (message.created_at.0.as_str(), TimelineRow::Message(index)))
        .chain(view.activities.iter().enumerate().map(|(index, activity)| {
            (activity.created_at.0.as_str(), TimelineRow::Activity(index))
        }))
        .collect();
    // RFC3339 timestamps with fixed millisecond precision sort lexically. The
    // sort is stable, so messages keep their lead over same-instant activities.
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut rows: Vec<TimelineRow> = entries.into_iter().map(|(_, row)| row).collect();
    if running {
        rows.push(TimelineRow::Running);
    }
    rows
}

/// Fingerprint of everything in a row that changes its rendered height.
///
/// `ListState` caches the height it measured for each row, so a row whose
/// content grew — a streaming message, a tool result arriving, a detail block
/// expanding — keeps its stale height until it is explicitly remeasured.
/// Comparing fingerprints is how [`ChatApp::rebuild_timeline`] finds those
/// rows without remeasuring (and so re-laying-out) the whole thread.
fn row_content_hash(
    view: &OrchestrationThread,
    row: TimelineRow,
    revert_turn_counts: &HashMap<MessageId, i64>,
    expanded_activities: &HashSet<String>,
    thread_key: Option<&str>,
    changed_files: &changed_files::ChangedFilesState,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        TimelineRow::Message(index) => {
            let Some(message) = view.messages.get(index) else {
                return 0;
            };
            0u8.hash(&mut hasher);
            message.id.0.hash(&mut hasher);
            message.role.hash(&mut hasher);
            message.text.0.hash(&mut hasher);
            message.streaming.hash(&mut hasher);
            revert_turn_counts
                .contains_key(&message.id)
                .hash(&mut hasher);
            if message.role == OrchestrationMessageRole::Assistant {
                changed_files.hash_card(&mut hasher, thread_key, view, &message.id);
            }
        }
        TimelineRow::Activity(index) => {
            let Some(activity) = view.activities.get(index) else {
                return 0;
            };
            1u8.hash(&mut hasher);
            activity.id.0.hash(&mut hasher);
            activity.summary.0.hash(&mut hasher);
            let expanded = expanded_activities.contains(&activity.id.0);
            expanded.hash(&mut hasher);
            if expanded {
                activity_detail(activity).hash(&mut hasher);
            } else {
                activity_detail(activity).is_some().hash(&mut hasher);
            }
        }
        TimelineRow::Running => 2u8.hash(&mut hasher),
    }
    hasher.finish()
}

impl ChatApp {
    pub fn new(
        home: &Path,
        status_rx: watch::Receiver<SupervisorStatus>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let sidebar = SidebarPrefs::load(home);
        let right_panel = right_panel::RightPanelPrefs::load(home);
        // Panel width persists on drag end only (`Resized` fires once per
        // drag), matching Electron's localStorage write in `onLayout` commit.
        let sidebar_resize = cx.new(|_| ResizableState::default());
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Ask anything, @tag files/folders, $use skills, or / for commands")
                .auto_grow(1, 8)
        });
        let mut subscriptions = vec![cx.subscribe_in(
            &composer,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => {
                    // While the mention popover is open, Enter accepts the
                    // top result instead of sending.
                    let accepted_mention = this.apply_mention(0, window, cx);
                    if !accepted_mention {
                        this.send(window, cx);
                    }
                }
                InputEvent::Change => this.sync_mention(cx),
                _ => {}
            },
        )];
        subscriptions.push(cx.subscribe(
            &sidebar_resize,
            |this: &mut Self, state, _: &gpui_component::resizable::ResizablePanelEvent, cx| {
                if !this.dock_open() {
                    return;
                }
                // Layout order: sidebar | chat | dock — the dock is index 2.
                let Some(width) = state.read(cx).sizes().get(2).copied() else {
                    return;
                };
                let width = f32::from(width).max(right_panel::MIN_PANEL_WIDTH);
                if (width - this.right_panel.width).abs() > f32::EPSILON {
                    this.right_panel.width = width;
                    this.right_panel.save();
                }
            },
        ));

        // Sidecar status line for the footer.
        cx.spawn({
            let mut status_rx = status_rx.clone();
            async move |this, cx| {
                loop {
                    let line: SharedString = describe_status(&status_rx.borrow_and_update()).into();
                    if this
                        .update(cx, |app, cx| {
                            app.sidecar_status = line;
                            cx.notify();
                        })
                        .is_err()
                    {
                        return;
                    }
                    if status_rx.changed().await.is_err() {
                        return;
                    }
                }
            }
        })
        .detach();

        // Start the environment client on the tokio runtime, then follow the
        // shell channel for the lifetime of the window.
        cx.spawn(async move |this, cx| {
            let client = cx
                .update(|cx| {
                    Tokio::spawn_result(cx, async move {
                        Ok(Arc::new(EnvironmentClient::start(status_rx)))
                    })
                })
                .await;
            let client = match client {
                Ok(client) => client,
                Err(error) => {
                    let _ = this.update(cx, |app, cx| {
                        app.last_error = Some(format!("client start failed: {error:#}").into());
                        cx.notify();
                    });
                    return;
                }
            };
            let mut shell_rx = client.shell();
            if this
                .update(cx, |app, cx| {
                    app.client = Some(client);
                    app.spawn_terminal_metadata_loop(cx);
                    cx.notify();
                })
                .is_err()
            {
                return;
            }
            loop {
                let state = shell_rx.borrow_and_update().clone();
                if this
                    .update(cx, |app, cx| {
                        app.shell = state;
                        // The open thread's row keeps its "seen" stamp current
                        // so a turn finishing under your eyes doesn't light up
                        // as unread.
                        app.sync_thread_visit();
                        app.rebuild_sidebar();
                        app.maybe_open_debug_thread(cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
                if shell_rx.changed().await.is_err() {
                    return;
                }
            }
        })
        .detach();

        // Seed the focus path so the palette shortcuts land before the user
        // has clicked anything.
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        // ...and take it back whenever it empties. gpui dispatches a key from
        // the focused node upwards, and with nothing focused it starts at the
        // window's root node — which is `Root`, above this view — so every
        // chord silently reaches no handler. Plenty of ordinary actions empty
        // the path: committing a tree rename drops the `InputState` that owned
        // focus, and hiding the files panel unmounts the focused editor, which
        // dispatch cannot tell apart from nothing being focused. Zed's
        // `Workspace` guards itself the same way.
        subscriptions.push(cx.on_focus_lost(window, |this, window, cx| {
            let handle = window
                .focus_lost_restore_target(cx)
                .unwrap_or_else(|| this.focus_handle.clone());
            window.focus(&handle, cx);
        }));

        Self {
            client: None,
            sidecar_status: "starting…".into(),
            shell: ShellState::default(),
            thread: None,
            composer,
            last_error: None,
            responding: HashSet::new(),
            input_draft: None,
            pending_attachments: Vec::new(),
            mention: None,
            mention_generation: 0,
            pending_revert: None,
            changed_files: changed_files::ChangedFilesState::load(home),
            debug_open_thread: std::env::var("VITRE_OPEN_THREAD")
                .ok()
                .filter(|value| !value.is_empty()),
            reverting: false,
            expanded_activities: HashSet::new(),
            timeline_list: new_timeline_list(),
            timeline: Vec::new(),
            timeline_hashes: Vec::new(),
            revert_turn_counts: HashMap::new(),
            sidebar,
            sidebar_rows: Vec::new(),
            sidebar_project_order: Vec::new(),
            project_rename: None,
            project_grouping: None,
            sidebar_resize,
            files: None,
            diff: None,
            diff_subscription: None,
            diff_store: home.join(diff_panel::FILE_NAME),
            right_panel,
            terminal_ui: terminal_drawer::TerminalPrefs::load(home),
            terminal_views: HashMap::new(),
            terminal_view_subs: HashMap::new(),
            terminal_metadata: Vec::new(),
            terminal_drag: None,
            viewport_height: 720.,
            applied_reveals: HashMap::new(),
            last_dock_sync_key: None,
            quick_search: None,
            command_palette: None,
            focus_handle,
            _subscriptions: subscriptions,
        }
    }

    /// Open (or re-target, or dismiss) the QuickSearch overlay.
    ///
    /// Electron's trigger is a toggle: the shortcut for the mode already
    /// showing closes the dialog, the other mode switches corpus in place and
    /// keeps the typed query.
    pub fn toggle_quick_search(
        &mut self,
        mode: QuickSearchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = self.quick_search.clone() {
            if search.read(cx).mode() == mode {
                // Closing from out here skips the overlay's own `dismiss`, so
                // drop the handle in the same breath — otherwise the next
                // press finds a `Some` whose dialog is already gone and closes
                // an empty stack forever.
                window.close_dialog(cx);
                self.quick_search = None;
                cx.notify();
            } else {
                search.update(cx, |search, cx| search.set_mode(mode, window, cx));
            }
            return;
        }
        let search = QuickSearch::open(
            mode,
            self.client.clone(),
            self.search_root(),
            self.shell_threads(),
            self.shell
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.projects.clone())
                .unwrap_or_default(),
            window,
            cx,
        );
        self._subscriptions
            .push(cx.subscribe_in(&search, window, Self::on_quick_search_event));
        self.quick_search = Some(search);
        cx.notify();
    }

    fn on_quick_search_event(
        &mut self,
        _search: &Entity<QuickSearch>,
        event: &QuickSearchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            QuickSearchEvent::OpenThread(id) => self.select_thread(id.clone(), cx),
            QuickSearchEvent::OpenFile { path, line } => {
                // Electron opens files as right-panel file surfaces, revealing
                // the panel if collapsed. Content-search lines are one-based,
                // which is what `openFile` stores too.
                self.dock_open_file(path.clone(), *line, window, cx);
            }
            QuickSearchEvent::Dismissed => {
                self.quick_search = None;
                cx.notify();
            }
        }
    }

    /// Open (or dismiss) the command palette. Electron binds the same chord to
    /// both directions.
    pub fn toggle_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_palette.is_some() {
            // Same as above: this path bypasses the palette's own `dismiss`.
            window.close_dialog(cx);
            self.command_palette = None;
            cx.notify();
            return;
        }
        self.open_command_palette(false, window, cx);
    }

    /// The sidebar's FolderPlus button: the palette opened straight into the
    /// add-project flow (Electron's `openCommandPalette({open:"add-project"})`).
    pub fn open_add_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_palette.is_some() {
            window.close_dialog(cx);
            self.command_palette = None;
        }
        self.open_command_palette(true, window, cx);
    }

    fn open_command_palette(
        &mut self,
        add_project: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = self
            .client
            .as_ref()
            .and_then(|client| client.sessions().borrow().clone());
        let projects = self
            .shell
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.projects.clone())
            .unwrap_or_default();
        let active_project = self
            .thread
            .as_ref()
            .and_then(|open| self.shell_thread(&open.id))
            .map(|thread| thread.project_id.clone());
        let active_project_cwd = active_project
            .as_ref()
            .and_then(|id| projects.iter().find(|project| project.id == *id))
            .map(|project| project.workspace_root.0.clone());
        let context = PaletteContext {
            projects,
            threads: self.shell_threads(),
            active_thread: self.thread.as_ref().map(|open| open.id.clone()),
            active_project,
            client: self.client.clone(),
            windows_platform: session.as_ref().is_some_and(|session| {
                session.config.environment.platform.os == ExecutionEnvironmentPlatformOs::Windows
            }),
            base_directory: session.as_ref().and_then(|session| {
                session
                    .config
                    .settings
                    .add_project_base_directory
                    .clone()
                    .flatten()
            }),
            active_project_cwd,
            default_model_selection: session
                .as_ref()
                .and_then(|session| default_model_selection(&session.config)),
            open_add_project: add_project,
        };
        let palette = CommandPalette::open(context, window, cx);
        self._subscriptions
            .push(cx.subscribe_in(&palette, window, Self::on_command_palette_event));
        self.command_palette = Some(palette);
        cx.notify();
    }

    fn on_command_palette_event(
        &mut self,
        _palette: &Entity<CommandPalette>,
        event: &CommandPaletteEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = match event {
            CommandPaletteEvent::Dismissed => {
                self.command_palette = None;
                cx.notify();
                return;
            }
            CommandPaletteEvent::Run(action) => action.clone(),
        };
        match action {
            PaletteAction::NewThread { project_id } => self.new_thread(project_id, window, cx),
            // Electron follows a successful `project.create` with a fresh
            // draft thread in the new project.
            PaletteAction::ProjectCreated(project_id) => {
                self.new_thread(Some(project_id), window, cx)
            }
            PaletteAction::OpenThread(id) => self.select_thread(id, cx),
            PaletteAction::OpenProject(project_id) => {
                // Electron's `openProjectFromSearch`: the project's most recent
                // thread, or a new one when it has none.
                match self
                    .shell_threads()
                    .into_iter()
                    .find(|thread| thread.project_id == project_id)
                {
                    Some(thread) => self.select_thread(thread.id, cx),
                    None => self.new_thread(Some(project_id), window, cx),
                }
            }
            PaletteAction::QuickSearchOpen => {
                self.toggle_quick_search(QuickSearchMode::Open, window, cx);
            }
            PaletteAction::QuickSearchContent => {
                self.toggle_quick_search(QuickSearchMode::Content, window, cx);
            }
            PaletteAction::ToggleFilesPanel => self.toggle_right_panel(window, cx),
            PaletteAction::ToggleTerminal => self.terminal_toggle(cx),
            PaletteAction::NewFile | PaletteAction::NewFolder => {
                self.dock_open_files_surface(window, cx);
                let Some(files) = self.files.clone() else {
                    return;
                };
                let directory = action == PaletteAction::NewFolder;
                files.update(cx, |files, cx| files.create_at_root(directory, window, cx));
                cx.notify();
            }
        }
    }

    /// Create (or recreate, when the open thread's project changed) the files
    /// panel for the active workspace root.
    fn ensure_files_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(client), Some(cwd)) = (self.client.clone(), self.search_root()) else {
            return;
        };
        let stale = self
            .files
            .as_ref()
            .is_none_or(|panel| panel.read(cx).cwd() != cwd);
        if stale {
            self.files = Some(cx.new(|cx| FilesPanel::new(client, cwd, window, cx)));
        }
    }

    /// Create (or recreate, when the dock's thread key or git root changed)
    /// the diff panel, and feed it the open thread's ordered turn summaries.
    /// Runs per frame while the diff surface is active, so both steps are
    /// change-guarded.
    fn ensure_diff_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(client), Some(key)) = (self.client.clone(), self.dock_thread_key()) else {
            return;
        };
        let Some(cwd) = self.search_root() else {
            return;
        };
        let stale = self.diff.as_ref().is_none_or(|panel| {
            let panel = panel.read(cx);
            panel.thread_key() != key || panel.cwd() != cwd
        });
        if stale {
            let thread_id = self.thread.as_ref().map(|open| open.id.clone());
            let panel = cx.new(|cx| {
                diff_panel::DiffPanel::new(client, key, cwd, thread_id, self.diff_store.clone(), cx)
            });
            self.diff_subscription = Some(cx.subscribe_in(
                &panel,
                window,
                |this, _, event: &diff_panel::DiffPanelEvent, window, cx| match event {
                    diff_panel::DiffPanelEvent::OpenFile { path } => {
                        this.dock_open_file(path.clone(), None, window, cx);
                    }
                },
            ));
            self.diff = Some(panel);
        }
        if let Some(panel) = self.diff.clone() {
            let ordered = self
                .thread
                .as_ref()
                .and_then(|open| open.state.view.as_ref())
                .map(|view| ordered_turn_diff_summaries(&view.checkpoints))
                .unwrap_or_default();
            panel.update(cx, |panel, cx| panel.set_checkpoints(ordered, cx));
        }
    }

    /// Start a thread in `project_id`, or — when it is `None` — in the project
    /// the view is already pointed at, falling back to the first known one
    /// (Electron's `startNewThreadFromContext`).
    fn new_thread(
        &mut self,
        project_id: Option<ProjectId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let project_id = project_id.or_else(|| {
            self.thread
                .as_ref()
                .and_then(|open| self.shell_thread(&open.id))
                .map(|thread| thread.project_id.clone())
        });
        let project = self.shell.snapshot.as_ref().and_then(|snapshot| {
            match &project_id {
                Some(id) => snapshot.projects.iter().find(|project| project.id == *id),
                None => snapshot.projects.first(),
            }
            .cloned()
        });
        // An explicit id may not have reached the shell snapshot yet — a
        // freshly created project's `project.create` resolves before the
        // subscription delivers it — so only the no-id case needs a project.
        let Some(project_id) = project_id.or_else(|| project.as_ref().map(|p| p.id.clone())) else {
            // Toast, not banner: the banner only renders with an open thread.
            window.push_notification(Notification::error("No project yet — open a folder."), cx);
            return;
        };
        let session = client.sessions().borrow().clone();
        let Some(session) = session else {
            return;
        };
        let Some(model_selection) = project
            .as_ref()
            .and_then(|project| project.default_model_selection.clone())
            .or_else(|| default_model_selection(&session.config))
        else {
            window.push_notification(Notification::error("No provider configured."), cx);
            return;
        };
        let thread_id = ThreadId(fresh_id("vitre-thread"));
        let command = ClientOrchestrationCommand::ThreadCreate {
            additional_roots: None,
            branch: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            interaction_mode: None,
            model_selection,
            project_id: project_id.clone(),
            runtime_mode: RuntimeMode::FullAccess,
            thread_id: thread_id.clone(),
            title: tnes("New thread"),
            r#type: Default::default(),
            worktree_path: None,
        };
        self.last_error = None;
        cx.spawn_in(window, async move |this, cx| {
            match client.dispatch(&command).await {
                Ok(_) => {
                    let _ = this.update(cx, |app, cx| app.select_thread(thread_id, cx));
                }
                Err(error) => {
                    let _ = this.update_in(cx, |_, window, cx| {
                        window.push_notification(
                            Notification::error(SharedString::from(format!(
                                "New thread failed: {error:?}"
                            ))),
                            cx,
                        );
                    });
                }
            }
        })
        .detach();
    }

    /// Consume the `VITRE_OPEN_THREAD` hook once its thread shows up in the
    /// shell (thread selection has no persisted state to seed instead).
    fn maybe_open_debug_thread(&mut self, cx: &mut Context<Self>) {
        let Some(wanted) = self.debug_open_thread.clone() else {
            return;
        };
        if self
            .shell_threads()
            .iter()
            .any(|thread| thread.id.0 == wanted)
        {
            self.debug_open_thread = None;
            self.select_thread(ThreadId(wanted), cx);
        }
    }

    fn select_thread(&mut self, id: ThreadId, cx: &mut Context<Self>) {
        if self.thread.as_ref().is_some_and(|open| open.id == id) {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let handle = client.open_thread(id.clone());
        let mut state_rx = handle.state();
        self.input_draft = None;
        self.mention = None;
        self.pending_attachments.clear();
        self.pending_revert = None;
        // Thread switch ≙ every changed-files card unmounting: the next
        // thread's cards re-run their auto-expand decision on first sight.
        self.changed_files.clear_local();
        // A different thread is a different list: drop every measured row and
        // re-arm tail following so the new thread opens at its newest message.
        self.timeline = Vec::new();
        self.timeline_hashes = Vec::new();
        self.revert_turn_counts = HashMap::new();
        self.timeline_list = new_timeline_list();
        // The drawer unmounts with the thread: panes are per-thread views
        // (server sessions persist; reselecting re-attaches).
        self.terminal_views.clear();
        self.terminal_view_subs.clear();
        self.terminal_drag = None;
        self.thread = Some(OpenThread {
            id: id.clone(),
            _handle: handle,
            state: ThreadState::default(),
        });
        // Opening a thread clears its unread completion and re-pins it in its
        // project's preview window.
        self.sync_thread_visit();
        self.rebuild_sidebar();
        // Terminals the server already has for this thread appear as tabs.
        self.reconcile_drawer_terminals(cx);
        cx.spawn(async move |this, cx| {
            loop {
                let state = state_rx.borrow_and_update().clone();
                let stop = this
                    .update(cx, |app, cx| {
                        if app.thread.as_ref().is_none_or(|open| open.id != id) {
                            return true;
                        }
                        if let Some(open) = &mut app.thread {
                            open.state = state;
                        }
                        // The list follows the tail on its own
                        // (`FollowMode::Tail`) and re-engages when the user
                        // scrolls back down, so this only has to resync rows.
                        app.rebuild_timeline(cx);
                        false
                    })
                    .unwrap_or(true);
                if stop || state_rx.changed().await.is_err() {
                    return;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(open) = &self.thread else {
            return;
        };
        let Some(view) = open.state.view.clone() else {
            return;
        };
        // While an approval or user-input request is pending, the composer's
        // primary action belongs to that request — don't start a new turn.
        // (Custom free-text answers are a post-parity iteration.)
        if !derive_pending_approvals(&view.activities).is_empty()
            || !derive_pending_user_inputs(&view.activities).is_empty()
        {
            return;
        }
        let text = self.composer.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        self.composer
            .update(cx, |input, cx| input.clean(window, cx));
        self.mention = None;
        self.last_error = None;
        let attachments = std::mem::take(&mut self.pending_attachments)
            .into_iter()
            .map(|attachment| {
                let data_url = tnes(attachment.data_url);
                let mime_type = tnes(attachment.mime_type);
                let name = tnes(attachment.name);
                if attachment.is_image {
                    TurnAttachment::Image {
                        data_url,
                        mime_type,
                        name,
                        size_bytes: attachment.size_bytes,
                    }
                } else {
                    TurnAttachment::File {
                        data_url,
                        mime_type,
                        name,
                        size_bytes: attachment.size_bytes,
                    }
                }
            })
            .collect();

        let command = ClientOrchestrationCommand::ThreadTurnStart {
            bootstrap: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            interaction_mode: view
                .interaction_mode
                .clone()
                .flatten()
                .unwrap_or(ProviderInteractionMode::Default),
            message: vitre_contracts::ClientOrchestrationCommandThreadTurnStartMessage {
                attachments,
                message_id: MessageId(fresh_id("vitre-msg")),
                role: Default::default(),
                text: tnes(text),
            },
            model_selection: None,
            runtime_mode: view.runtime_mode.clone(),
            source_proposed_plan: None,
            thread_id: open.id.clone(),
            title_seed: None,
            r#type: Default::default(),
        };
        cx.spawn(async move |this, cx| {
            if let Err(error) = client.dispatch(&command).await {
                let _ = this.update(cx, |app, cx| {
                    app.last_error = Some(format!("send failed: {error:?}").into());
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn stop_turn(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(open) = &self.thread else {
            return;
        };
        let command = ClientOrchestrationCommand::ThreadTurnInterrupt {
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            thread_id: open.id.clone(),
            turn_id: None,
            r#type: Default::default(),
        };
        cx.spawn(async move |this, cx| {
            if let Err(error) = client.dispatch(&command).await {
                let _ = this.update(cx, |app, cx| {
                    app.last_error = Some(format!("stop failed: {error:?}").into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Dispatch an approval / user-input response, tracking the request id so
    /// its buttons disable while the command is in flight.
    fn dispatch_response(
        &mut self,
        request_id: String,
        command: ClientOrchestrationCommand,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        if !self.responding.insert(request_id.clone()) {
            return;
        }
        self.last_error = None;
        cx.spawn(async move |this, cx| {
            let result = client.dispatch(&command).await;
            let _ = this.update(cx, |app, cx| {
                app.responding.remove(&request_id);
                if let Err(error) = result {
                    app.last_error = Some(format!("response failed: {error:?}").into());
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn respond_approval(
        &mut self,
        request_id: String,
        decision: ProviderApprovalDecision,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = &self.thread else {
            return;
        };
        let command = ClientOrchestrationCommand::ThreadApprovalRespond {
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            decision,
            request_id: ApprovalRequestId(request_id.clone()),
            thread_id: open.id.clone(),
            r#type: Default::default(),
        };
        self.dispatch_response(request_id, command, cx);
    }

    /// The front pending user-input request (the one the panel shows).
    fn active_pending_user_input(&self) -> Option<PendingUserInput> {
        let view = self.thread.as_ref()?.state.view.as_ref()?;
        derive_pending_user_inputs(&view.activities)
            .into_iter()
            .next()
    }

    /// Draft for `request_id`, resetting whenever the front request changes.
    fn input_draft_mut(&mut self, request_id: &str) -> &mut InputDraft {
        if self
            .input_draft
            .as_ref()
            .is_none_or(|draft| draft.request_id != request_id)
        {
            self.input_draft = Some(InputDraft {
                request_id: request_id.to_string(),
                question_index: 0,
                selections: HashMap::new(),
            });
        }
        self.input_draft.as_mut().expect("draft just ensured")
    }

    fn toggle_user_input_option(&mut self, option_label: String, cx: &mut Context<Self>) {
        let Some(pending) = self.active_pending_user_input() else {
            return;
        };
        if self.responding.contains(&pending.request_id) {
            return;
        }
        let draft = self.input_draft_mut(&pending.request_id);
        let index = draft.question_index.min(pending.questions.len() - 1);
        let question = &pending.questions[index];
        let selections = draft.selections.entry(question.id.clone()).or_default();
        if question.multi_select {
            if let Some(position) = selections.iter().position(|label| *label == option_label) {
                selections.remove(position);
            } else {
                selections.push(option_label);
            }
            cx.notify();
        } else {
            // Single-select answers advance immediately (Electron auto-advances
            // 200ms after the click).
            *selections = vec![option_label];
            self.advance_user_input(cx);
        }
    }

    /// Move to the next question, or submit `ThreadUserInputRespond` from the
    /// last one. Answers mirror `resolvePendingUserInputAnswer`: label array
    /// for multi-select questions, single label string otherwise. (Custom
    /// free-text answers are a post-parity iteration.)
    fn advance_user_input(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.active_pending_user_input() else {
            return;
        };
        if self.responding.contains(&pending.request_id) {
            return;
        }
        let Some(open) = &self.thread else {
            return;
        };
        let thread_id = open.id.clone();
        let draft = self.input_draft_mut(&pending.request_id);
        let index = draft.question_index.min(pending.questions.len() - 1);
        if index + 1 < pending.questions.len() {
            draft.question_index = index + 1;
            cx.notify();
            return;
        }
        let mut answers = serde_json::Map::new();
        for question in &pending.questions {
            let labels = draft
                .selections
                .get(&question.id)
                .cloned()
                .unwrap_or_default();
            let value = if question.multi_select {
                serde_json::Value::Array(
                    labels.into_iter().map(serde_json::Value::String).collect(),
                )
            } else if let Some(label) = labels.into_iter().next() {
                serde_json::Value::String(label)
            } else {
                serde_json::Value::Null
            };
            answers.insert(question.id.clone(), value);
        }
        let command = ClientOrchestrationCommand::ThreadUserInputRespond {
            answers,
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            request_id: ApprovalRequestId(pending.request_id.clone()),
            thread_id,
            r#type: Default::default(),
        };
        self.dispatch_response(pending.request_id, command, cx);
    }

    /// Two-step checkpoint revert: the first click arms the message's button
    /// ("Revert?"), the second dispatches `ThreadCheckpointRevert` — standing
    /// in for Electron's native confirm dialog.
    fn revert_user_message(
        &mut self,
        message_id: MessageId,
        turn_count: i64,
        cx: &mut Context<Self>,
    ) {
        if self.reverting {
            return;
        }
        if self.pending_revert.as_ref() != Some(&message_id) {
            self.pending_revert = Some(message_id);
            cx.notify();
            return;
        }
        self.pending_revert = None;
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(open) = &self.thread else {
            return;
        };
        let running = open
            .state
            .view
            .as_ref()
            .and_then(|view| view.session.as_ref())
            .is_some_and(|session| session.status == OrchestrationSessionStatus::Running);
        if running {
            self.last_error =
                Some("Interrupt the current turn before reverting checkpoints.".into());
            cx.notify();
            return;
        }
        let command = ClientOrchestrationCommand::ThreadCheckpointRevert {
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            thread_id: open.id.clone(),
            turn_count: NonNegativeInt(turn_count),
            r#type: Default::default(),
        };
        self.reverting = true;
        self.last_error = None;
        cx.spawn(async move |this, cx| {
            let result = client.dispatch(&command).await;
            let _ = this.update(cx, |app, cx| {
                app.reverting = false;
                if let Err(error) = result {
                    app.last_error = Some(format!("revert failed: {error:?}").into());
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn shell_threads(&self) -> Vec<OrchestrationThreadShell> {
        let Some(snapshot) = &self.shell.snapshot else {
            return Vec::new();
        };
        let mut threads: Vec<_> = snapshot
            .threads
            .iter()
            .filter(|thread| {
                // archived_at: triple Option — only Some(Some(Some(_))) is set.
                !matches!(&thread.archived_at, Some(Some(Some(_))))
            })
            .cloned()
            .collect();
        threads.sort_by(|a, b| b.created_at.0.cmp(&a.created_at.0));
        threads
    }

    /// The SHELL's copy of a thread — canonical for title/meta (meta events
    /// are shell-scoped, so the detail projection never sees them).
    fn shell_thread(&self, id: &ThreadId) -> Option<&OrchestrationThreadShell> {
        self.shell
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.threads.iter().find(|thread| thread.id == *id))
    }

    /// Canonical title comes from the SHELL (meta events are shell-scoped).
    fn thread_title(&self, id: &ThreadId) -> SharedString {
        self.shell_thread(id)
            .map(|thread| thread.title.0.clone().into())
            .unwrap_or_else(|| "(untitled)".into())
    }

    /// Change the open thread's model (`ThreadMetaUpdate`). Electron persists
    /// lazily on next turn start; persisting immediately is equivalent for the
    /// thread's stored selection and keeps the picker stateless.
    fn set_model(&mut self, selection: ModelSelection, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(open) = &self.thread else {
            return;
        };
        let command = ClientOrchestrationCommand::ThreadMetaUpdate {
            additional_roots: None,
            branch: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            expected_branch: None,
            model_selection: Some(Some(selection)),
            thread_id: open.id.clone(),
            title: None,
            r#type: Default::default(),
            worktree_path: None,
        };
        self.last_error = None;
        cx.spawn(async move |this, cx| {
            if let Err(error) = client.dispatch(&command).await {
                let _ = this.update(cx, |app, cx| {
                    app.last_error = Some(format!("model change failed: {error:?}").into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Root of the open thread — its worktree when it has one, otherwise its
    /// project's workspace root. This is the thread-scoped cwd: the composer's
    /// `@`-mention search runs here, as Electron's does.
    fn open_project_root(&self) -> Option<String> {
        let open = self.thread.as_ref()?;
        let thread = self.shell_thread(&open.id)?;
        // A worktree thread searches its checkout, not the project's main
        // tree (Electron: `activeThread.worktreePath ?? project.workspaceRoot`).
        if let Some(worktree) = thread.worktree_path.as_ref() {
            return Some(worktree.0.clone());
        }
        let project_id = thread.project_id.clone();
        self.project_root(&project_id)
    }

    /// Workspace root of `project_id`, if the shell knows it.
    fn project_root(&self, project_id: &ProjectId) -> Option<String> {
        self.shell
            .snapshot
            .as_ref()?
            .projects
            .iter()
            .find(|project| project.id == *project_id)
            .map(|project| project.workspace_root.0.clone())
    }

    /// The root QuickSearch and the files panel look at.
    ///
    /// Electron resolves the overlay's scope down a chain — active thread,
    /// then the draft thread, then the first project — so ⌘P still searches
    /// files from the home view, where no thread is selected. Without the
    /// last step the file corpus is silently empty there.
    fn search_root(&self) -> Option<String> {
        self.open_project_root().or_else(|| {
            self.shell
                .snapshot
                .as_ref()?
                .projects
                .first()
                .map(|project| project.workspace_root.0.clone())
        })
    }

    /// Recompute the composer's active `@token` and (re-)issue the entry
    /// search. Previous results stay visible while typing; a generation
    /// counter drops out-of-order responses.
    fn sync_mention(&mut self, cx: &mut Context<Self>) {
        let (value, cursor) = {
            let state = self.composer.read(cx);
            (state.value(), state.cursor())
        };
        let Some((token_start, query)) = active_mention_token(&value, cursor) else {
            if self.mention.take().is_some() {
                cx.notify();
            }
            return;
        };
        let unchanged = self
            .mention
            .as_ref()
            .is_some_and(|mention| mention.token_start == token_start && mention.query == query);
        if unchanged {
            return;
        }
        self.mention_generation += 1;
        let generation = self.mention_generation;
        let results = self
            .mention
            .take()
            .map(|mention| mention.results)
            .unwrap_or_default();
        self.mention = Some(MentionState {
            token_start,
            query: query.clone(),
            generation,
            results: if query.is_empty() { vec![] } else { results },
        });
        cx.notify();
        if query.is_empty() {
            return;
        }
        let (Some(client), Some(cwd)) = (self.client.clone(), self.open_project_root()) else {
            return;
        };
        let payload = ProjectSearchEntriesInput {
            cwd: tnes(cwd),
            limit: 8,
            query: tnes(query),
        };
        cx.spawn(async move |this, cx| {
            // Search failures just leave the popover as-is (Electron shows
            // nothing on error too).
            let Ok(result) = client.call::<ProjectsSearchEntries>(&payload).await else {
                return;
            };
            let _ = this.update(cx, |app, cx| {
                if let Some(mention) = &mut app.mention
                    && mention.generation == generation
                {
                    mention.results = result.entries;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Replace the composer's `@token` with the picked entry's mention text.
    /// Returns false when no popover result is available at `index`.
    fn apply_mention(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(mention) = &self.mention else {
            return false;
        };
        let Some(entry) = mention.results.get(index) else {
            return false;
        };
        let token_start = mention.token_start;
        let formatted = format_mention(&entry.path.0);
        self.mention = None;
        self.composer.update(cx, |state, cx| {
            let value = state.value();
            let cursor = state.cursor();
            if token_start > cursor || cursor > value.len() {
                return;
            }
            let text = format!("{}{}{}", &value[..token_start], formatted, &value[cursor..]);
            let caret = token_start + formatted.len();
            state.set_value(text, window, cx);
            state.set_selected_range(caret..caret, cx);
        });
        cx.notify();
        true
    }

    /// Stage files via the native open dialog. Files are read immediately and
    /// held as data URLs (the wire inlines attachments into `ThreadTurnStart`),
    /// with Electron's caps: 8 per message, 10MB each.
    fn attach_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.thread.is_none() {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let _ = this.update_in(cx, |app, window, cx| {
                let mut error: Option<String> = None;
                for path in paths {
                    if app.pending_attachments.len() >= MAX_ATTACHMENTS {
                        error = Some(format!(
                            "You can attach up to {MAX_ATTACHMENTS} files per message."
                        ));
                        break;
                    }
                    let name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "file".to_string());
                    let bytes = match std::fs::read(&path) {
                        Ok(bytes) => bytes,
                        Err(read_error) => {
                            error = Some(format!("Could not read '{name}': {read_error}"));
                            continue;
                        }
                    };
                    if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
                        error = Some(format!("'{name}' exceeds the 10MB attachment limit."));
                        continue;
                    }
                    let mime_type = infer_attachment_mime_type(&name);
                    app.pending_attachments.push(PendingAttachment {
                        data_url: format!(
                            "data:{mime_type};base64,{}",
                            base64::engine::general_purpose::STANDARD.encode(&bytes)
                        ),
                        name,
                        mime_type: mime_type.to_string(),
                        size_bytes: bytes.len() as i64,
                        is_image: mime_type.starts_with("image/"),
                    });
                }
                if let Some(error) = error {
                    window.push_notification(Notification::error(SharedString::from(error)), cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Composer-top approval panel (`ComposerPendingApprovalPanel`): PENDING
    /// APPROVAL eyebrow + kind summary + count, and a mono detail box.
    fn render_approval_panel(
        &self,
        approval: &PendingApproval,
        pending_count: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (summary, detail_label) = match approval.request_kind {
            ApprovalRequestKind::Command => ("Command approval requested", "Command"),
            ApprovalRequestKind::FileRead => ("File-read approval requested", "File to read"),
            ApprovalRequestKind::FileChange => ("File-change approval requested", "File change"),
        };
        let mut panel = v_flex().px_4().py_3p5().child(
            h_flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child("PENDING APPROVAL"),
                )
                .child(div().text_sm().font_medium().child(summary))
                .when(pending_count > 1, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(format!("1/{pending_count}"))),
                    )
                }),
        );
        if let Some(detail) = &approval.detail {
            panel = panel.child(
                v_flex()
                    .mt_3()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background.opacity(0.7))
                    .p_3()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().muted_foreground)
                            .child(detail_label),
                    )
                    .child(
                        div()
                            .id("approval-detail")
                            .mt_2()
                            .max_h(px(160.))
                            .overflow_y_scroll()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_xs()
                            .text_color(cx.theme().foreground)
                            .whitespace_normal()
                            .child(SharedString::from(detail.clone())),
                    ),
            );
        }
        panel.into_any_element()
    }

    /// Composer footer replacement while an approval is pending
    /// (`ComposerPendingApprovalActions`): Cancel turn / Decline / Always
    /// allow this session / Approve once.
    fn render_approval_actions(
        &self,
        approval: &PendingApproval,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let responding = self.responding.contains(&approval.request_id);
        let respond = |decision: ProviderApprovalDecision| {
            let request_id = approval.request_id.clone();
            cx.listener(move |this: &mut Self, _, _, cx| {
                this.respond_approval(request_id.clone(), decision.clone(), cx);
            })
        };
        h_flex()
            .px_3()
            .pb_3()
            .gap_2()
            .items_center()
            .justify_end()
            .child(
                Button::new("approval-cancel")
                    .label("Cancel turn")
                    .ghost()
                    .small()
                    .disabled(responding)
                    .on_click(respond(ProviderApprovalDecision::Cancel)),
            )
            .child(
                Button::new("approval-decline")
                    .label("Decline")
                    .danger()
                    .outline()
                    .small()
                    .disabled(responding)
                    .on_click(respond(ProviderApprovalDecision::Decline)),
            )
            .child(
                Button::new("approval-accept-session")
                    .label("Always allow this session")
                    .outline()
                    .small()
                    .disabled(responding)
                    .on_click(respond(ProviderApprovalDecision::AcceptForSession)),
            )
            .child(
                Button::new("approval-accept")
                    .label("Approve once")
                    .primary()
                    .small()
                    .disabled(responding)
                    .on_click(respond(ProviderApprovalDecision::Accept)),
            )
            .into_any_element()
    }

    /// Composer-top user-input panel (`ComposerPendingUserInputPanel`): one
    /// question at a time — header eyebrow + n/N chip, question text, option
    /// rows with selection state and number-key chips.
    fn render_user_input_panel(
        &self,
        pending: &PendingUserInput,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let responding = self.responding.contains(&pending.request_id);
        let (question_index, selected): (usize, Vec<String>) = match &self.input_draft {
            Some(draft) if draft.request_id == pending.request_id => {
                let index = draft.question_index.min(pending.questions.len() - 1);
                (
                    index,
                    draft
                        .selections
                        .get(&pending.questions[index].id)
                        .cloned()
                        .unwrap_or_default(),
                )
            }
            _ => (0, Vec::new()),
        };
        let question = &pending.questions[question_index];

        let mut options = v_flex().mt_3().gap_1p5();
        for (index, option) in question.options.iter().enumerate() {
            let is_selected = selected.contains(&option.label);
            let label = option.label.clone();
            let mut labels = v_flex().min_w_0().flex_1().gap_0p5().child(
                div()
                    .text_sm()
                    .font_medium()
                    .child(SharedString::from(option.label.clone())),
            );
            if !option.description.is_empty() && option.description != option.label {
                labels = labels.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(option.description.clone())),
                );
            }
            let trailing: gpui::AnyElement = if is_selected {
                Icon::new(IconName::Check)
                    .size_3p5()
                    .text_color(cx.theme().primary)
                    .into_any_element()
            } else {
                div()
                    .size_5()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .text_size(px(11.))
                    .text_color(cx.theme().muted_foreground.opacity(0.7))
                    .child(SharedString::from(format!("{}", index + 1)))
                    .into_any_element()
            };
            let mut row = h_flex()
                .id(("ui-option", index))
                .w_full()
                .items_center()
                .gap_3()
                .rounded(cx.theme().radius)
                .border_1()
                .px_3()
                .py_2()
                .child(labels)
                .child(trailing);
            row = if is_selected {
                row.border_color(cx.theme().primary.opacity(0.3))
                    .bg(cx.theme().primary.opacity(0.08))
            } else {
                row.border_color(gpui::transparent_black())
                    .bg(cx.theme().secondary)
                    .hover(|style| style.bg(cx.theme().accent))
            };
            row = if responding {
                row.opacity(0.5)
            } else {
                row.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_user_input_option(label.clone(), cx);
                    }))
            };
            options = options.child(row);
        }

        v_flex()
            .px_4()
            .py_3()
            .child(
                h_flex()
                    .mb_2()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground.opacity(0.55))
                            .child(SharedString::from(question.header.to_uppercase())),
                    )
                    .when(pending.questions.len() > 1, |this| {
                        this.child(
                            div()
                                .h_5()
                                .px_1p5()
                                .flex()
                                .items_center()
                                .rounded(px(6.))
                                .bg(cx.theme().muted.opacity(0.6))
                                .text_size(px(10.))
                                .font_medium()
                                .text_color(cx.theme().muted_foreground.opacity(0.6))
                                .child(SharedString::from(format!(
                                    "{}/{}",
                                    question_index + 1,
                                    pending.questions.len()
                                ))),
                        )
                    }),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().foreground.opacity(0.9))
                    .child(SharedString::from(question.question.clone())),
            )
            .when(question.multi_select, |this| {
                this.child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground.opacity(0.65))
                        .child("Select one or more options."),
                )
            })
            .child(options)
            .when(question.multi_select, |this| {
                let is_last = question_index + 1 >= pending.questions.len();
                this.child(
                    h_flex().mt_3().justify_end().child(
                        Button::new("ui-advance")
                            .label(if is_last { "Submit" } else { "Continue" })
                            .primary()
                            .small()
                            .disabled(responding || selected.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.advance_user_input(cx);
                            })),
                    ),
                )
            })
            .into_any_element()
    }

    /// Right-hand plan panel (`PlanSidebar`): TASKS badge header + the active
    /// TodoWrite plan's steps with status glyphs.
    fn render_plan_sidebar(
        &self,
        plan: &ActivePlanState,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let mut steps = v_flex().gap_1();
        for (index, step) in plan.steps.iter().enumerate() {
            let glyph: gpui::AnyElement = match step.status {
                PlanStepStatus::Pending => div()
                    .size_3p5()
                    .flex_shrink_0()
                    .rounded_full()
                    .border_1()
                    .border_color(cx.theme().muted_foreground.opacity(0.4))
                    .into_any_element(),
                PlanStepStatus::InProgress => Icon::new(IconName::LoaderCircle)
                    .size_3p5()
                    .text_color(cx.theme().info)
                    .into_any_element(),
                PlanStepStatus::Completed => Icon::new(IconName::CircleCheck)
                    .size_3p5()
                    .text_color(cx.theme().success)
                    .into_any_element(),
            };
            let text = div()
                .text_size(px(13.))
                .map(|this| match step.status {
                    PlanStepStatus::Completed => this
                        .text_color(cx.theme().muted_foreground.opacity(0.5))
                        .line_through(),
                    PlanStepStatus::InProgress => {
                        this.text_color(cx.theme().foreground.opacity(0.9))
                    }
                    PlanStepStatus::Pending => {
                        this.text_color(cx.theme().muted_foreground.opacity(0.7))
                    }
                })
                .child(SharedString::from(step.step.clone()));
            let mut row = h_flex()
                .id(("plan-step", index))
                .items_center()
                .gap_2p5()
                .rounded(cx.theme().radius)
                .px_2p5()
                .py_2()
                .child(glyph)
                .child(text);
            row = match step.status {
                PlanStepStatus::InProgress => row.bg(cx.theme().info.opacity(0.05)),
                PlanStepStatus::Completed => row.bg(cx.theme().success.opacity(0.05)),
                PlanStepStatus::Pending => row,
            };
            steps = steps.child(row);
        }

        let mut content = v_flex().p_3().gap_4();
        if let Some(explanation) = &plan.explanation {
            content = content.child(
                div()
                    .text_size(px(13.))
                    .text_color(cx.theme().muted_foreground.opacity(0.8))
                    .child(SharedString::from(explanation.clone())),
            );
        }
        content = content.child(
            v_flex()
                .child(
                    div()
                        .mb_2()
                        .text_size(px(10.))
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground.opacity(0.4))
                        .child("STEPS"),
                )
                .child(steps),
        );

        v_flex()
            .w(px(340.))
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .h(px(48.))
                    .px_3()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .px_1p5()
                            .rounded(px(6.))
                            .bg(cx.theme().info.opacity(0.15))
                            .text_size(px(10.))
                            .font_semibold()
                            .text_color(cx.theme().info)
                            .child("TASKS"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(cx.theme().muted_foreground.opacity(0.6))
                            .children(relative_time(&plan.created_at)),
                    ),
            )
            .child(
                div()
                    .id("plan-steps")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(content),
            )
            .into_any_element()
    }

    /// Recompute the cached timeline order, revert targets and list length.
    ///
    /// Called when the thread view changes — never from `render`, which is the
    /// point: this walks every message and activity, and doing that per frame
    /// is what made scrolling a long thread stutter.
    fn rebuild_timeline(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self
            .thread
            .as_ref()
            .and_then(|open| open.state.view.as_ref())
        else {
            self.timeline = Vec::new();
            self.timeline_hashes = Vec::new();
            self.revert_turn_counts = HashMap::new();
            self.timeline_list.reset(0);
            cx.notify();
            return;
        };

        let running = view
            .session
            .as_ref()
            .is_some_and(|session| session.status == OrchestrationSessionStatus::Running);
        let rows = timeline_rows(view, running);

        // Revert targets: a user message reverts to the checkpoint BEFORE the
        // next assistant turn's checkpoint (`checkpointTurnCount - 1`) —
        // ported from ChatView's `revertTurnCountByUserMessageId`.
        let by_assistant: HashMap<&MessageId, i64> = view
            .checkpoints
            .iter()
            .filter_map(|checkpoint| {
                checkpoint
                    .assistant_message_id
                    .as_ref()
                    .map(|id| (id, checkpoint.checkpoint_turn_count.0))
            })
            .collect();
        let mut messages: Vec<&OrchestrationMessage> = view.messages.iter().collect();
        messages.sort_by(|a, b| a.created_at.0.cmp(&b.created_at.0));
        let mut counts = HashMap::new();
        for (index, message) in messages.iter().enumerate() {
            if message.role != OrchestrationMessageRole::User {
                continue;
            }
            for next in &messages[index + 1..] {
                if next.role == OrchestrationMessageRole::User {
                    break;
                }
                if let Some(count) = by_assistant.get(&next.id) {
                    counts.insert(message.id.clone(), (count - 1).max(0));
                    break;
                }
            }
        }
        self.revert_turn_counts = counts;

        // The changed-files card's auto-expand decision is made once per turn,
        // at the web component's mount ≙ the checkpoint's first appearance.
        let latest_turn_id = view.latest_turn.as_ref().map(|latest| &latest.turn_id);
        for checkpoint in &view.checkpoints {
            let is_latest = latest_turn_id == Some(&checkpoint.turn_id);
            self.changed_files.ensure_local(checkpoint, is_latest);
        }

        let thread_key = self.dock_thread_key();
        let hashes = rows
            .iter()
            .map(|row| {
                row_content_hash(
                    view,
                    *row,
                    &self.revert_turn_counts,
                    &self.expanded_activities,
                    thread_key.as_deref(),
                    &self.changed_files,
                )
            })
            .collect();
        let previous = std::mem::replace(&mut self.timeline, rows);
        let previous_hashes: Vec<u64> = std::mem::replace(&mut self.timeline_hashes, hashes);

        // The common shapes are "rows appended" and "same rows, tail grew"
        // (a streaming message). Splice the identity difference rather than
        // resetting, which would throw away the user's place in the history.
        let shared = previous
            .iter()
            .zip(self.timeline.iter())
            .take_while(|(before, after)| before == after)
            .count();
        if shared != previous.len() || shared != self.timeline.len() {
            self.timeline_list
                .splice(shared..previous.len(), self.timeline.len() - shared);
        }
        // Rows that survived the splice keep their cached height, so remeasure
        // the ones whose content moved. In a streaming turn that is one row.
        let mut index = 0;
        while index < shared {
            if previous_hashes.get(index) == self.timeline_hashes.get(index) {
                index += 1;
                continue;
            }
            let start = index;
            while index < shared && previous_hashes.get(index) != self.timeline_hashes.get(index) {
                index += 1;
            }
            self.timeline_list.remeasure_items(start..index);
        }
        cx.notify();
    }

    /// Invalidate one row's cached height after a purely local change.
    fn remeasure_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.timeline.get(index).copied() else {
            return;
        };
        // Keep the fingerprint in step, or the next rebuild remeasures again.
        let thread_key = self.dock_thread_key();
        let hash = self
            .thread
            .as_ref()
            .and_then(|open| open.state.view.as_ref())
            .map(|view| {
                row_content_hash(
                    view,
                    row,
                    &self.revert_turn_counts,
                    &self.expanded_activities,
                    thread_key.as_deref(),
                    &self.changed_files,
                )
            });
        if let Some(hash) = hash {
            self.timeline_hashes[index] = hash;
        }
        self.timeline_list.remeasure_items(index..index + 1);
        cx.notify();
    }

    /// Render one virtualized timeline row.
    ///
    /// Only rows near the viewport are built, so this runs a handful of times
    /// per frame instead of once per message in the thread.
    fn render_timeline_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.timeline.get(index).copied() else {
            return div().into_any_element();
        };
        let Some(view) = self
            .thread
            .as_ref()
            .and_then(|open| open.state.view.as_ref())
        else {
            return div().into_any_element();
        };
        let running = view
            .session
            .as_ref()
            .is_some_and(|session| session.status == OrchestrationSessionStatus::Running);
        let revert_turn_counts = &self.revert_turn_counts;
        let body: AnyElement = (|| -> AnyElement {
            match row {
                TimelineRow::Message(message_index) => {
                    let Some(message) = view.messages.get(message_index) else {
                        return div().into_any_element();
                    };
                    let is_user = message.role == OrchestrationMessageRole::User;
                    if is_user {
                        let mut line = h_flex().w_full().justify_end().items_center().gap_2();
                        if let Some(turn_count) = revert_turn_counts.get(&message.id).copied() {
                            let armed = self.pending_revert.as_ref() == Some(&message.id);
                            let message_id = message.id.clone();
                            let button: gpui::AnyElement = if armed {
                                Button::new(SharedString::from(format!("revert-{}", message.id.0)))
                                    .label("Revert?")
                                    .danger()
                                    .outline()
                                    .xsmall()
                                    .disabled(self.reverting || running)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.revert_user_message(
                                            message_id.clone(),
                                            turn_count,
                                            cx,
                                        );
                                    }))
                                    .into_any_element()
                            } else {
                                div()
                                    .id(SharedString::from(format!("revert-{}", message.id.0)))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size_6()
                                    .rounded(cx.theme().radius)
                                    .cursor_pointer()
                                    .text_color(cx.theme().muted_foreground.opacity(0.5))
                                    .hover(|style| style.bg(cx.theme().secondary))
                                    .child(Icon::new(IconName::Undo2).size_3())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.revert_user_message(
                                            message_id.clone(),
                                            turn_count,
                                            cx,
                                        );
                                    }))
                                    .into_any_element()
                            };
                            line = line.child(button);
                        }
                        (line.child(
                            div()
                                .max_w(relative(0.8))
                                .rounded(px(16.))
                                .bg(cx.theme().accent)
                                .p_3()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(SharedString::from(message.text.0.clone())),
                        ))
                        .into_any_element()
                    } else {
                        let text = message.text.0.clone();
                        let streaming = message.streaming;
                        let mut column = v_flex().child(
                            div()
                                .px_1()
                                .py_0p5()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .map(|this| {
                                    if text.is_empty() && streaming {
                                        this.text_color(cx.theme().muted_foreground)
                                            .child("(empty response)")
                                    } else {
                                        // Keyed per message: the free
                                        // `markdown()` helper keys by call
                                        // site and would collide in this
                                        // loop.
                                        this.child(
                                            TextView::markdown(
                                                SharedString::from(format!("md-{}", message.id.0)),
                                                SharedString::from(text),
                                            )
                                            .selectable(true),
                                        )
                                    }
                                }),
                        );
                        // Changed-files card, right after the markdown body
                        // (Electron's AssistantChangedFilesSection). Can
                        // appear mid-stream: placeholder checkpoints land
                        // while the turn is still running.
                        if let Some(summary) = changed_files::summary_for_message(view, &message.id)
                        {
                            let is_latest = changed_files::is_latest_turn(view, summary);
                            if let Some(card) =
                                self.changed_files_card(index, summary, is_latest, cx)
                            {
                                column = column.child(card);
                            }
                        }
                        column.into_any_element()
                    }
                }
                TimelineRow::Activity(activity_index) => {
                    let Some(activity) = view.activities.get(activity_index) else {
                        return div().into_any_element();
                    };
                    let tone_color = match activity.tone {
                        OrchestrationThreadActivityTone::Error => cx.theme().danger,
                        OrchestrationThreadActivityTone::Approval => cx.theme().warning,
                        _ => cx.theme().foreground.opacity(0.82),
                    };
                    let expanded = self.expanded_activities.contains(&activity.id.0);
                    let detail = activity_detail(activity);
                    let activity_id = activity.id.0.clone();
                    let mut block = v_flex().child(
                        h_flex()
                            .id(SharedString::from(format!("act-{}", activity.id.0)))
                            .px_0p5()
                            .py_0p5()
                            .gap_1p5()
                            .items_center()
                            .rounded(cx.theme().radius)
                            .when(detail.is_some(), |this| {
                                this.cursor_pointer()
                                    .hover(|style| style.bg(cx.theme().secondary))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if !this.expanded_activities.remove(&activity_id) {
                                            this.expanded_activities.insert(activity_id.clone());
                                        }
                                        // Local UI state, so `rebuild_timeline`
                                        // never runs — remeasure by hand or the
                                        // detail block renders into the collapsed
                                        // row's cached height.
                                        this.remeasure_row(index, cx);
                                    }))
                            })
                            .child(
                                Icon::new(activity_icon(&activity.kind.0))
                                    .size_3p5()
                                    .text_color(cx.theme().muted_foreground.opacity(0.8)),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_medium()
                                    .text_color(tone_color)
                                    .truncate()
                                    .child(SharedString::from(activity.summary.0.clone())),
                            ),
                    );
                    if expanded && let Some(detail) = detail {
                        // Electron parity: expanded tool detail is 11px
                        // mono under the row.
                        block = block.child(
                            div()
                                .id(SharedString::from(format!("act-detail-{}", activity.id.0)))
                                .ml_5()
                                .mt_0p5()
                                .max_h(px(240.))
                                .overflow_y_scroll()
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().secondary)
                                .px_2p5()
                                .py_2()
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_size(px(11.))
                                .text_color(cx.theme().muted_foreground)
                                .whitespace_normal()
                                .child(SharedString::from(detail)),
                        );
                    }
                    block.into_any_element()
                }
                TimelineRow::Running => (h_flex()
                    .gap_1p5()
                    .items_center()
                    .px_0p5()
                    .py_1()
                    .child(h_flex().gap_1().children((0..3).map(|_| {
                        div()
                            .size(px(4.))
                            .rounded_full()
                            .bg(cx.theme().muted_foreground.opacity(0.3))
                    })))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(cx.theme().muted_foreground.opacity(0.7))
                            .child("Working…"),
                    ))
                .into_any_element(),
            }
        })();
        // The old column's `gap_2` becomes per-row padding now that rows are
        // independent elements; `max_w` keeps the web timeline's centred measure.
        div()
            .w_full()
            .max_w(px(768.))
            .mx_auto()
            .px_5()
            .py_1()
            .child(body)
            .into_any_element()
    }

    /// Chat column mirroring the Electron `ChatView`: header, error banner,
    /// centered max-w-3xl timeline (user bubbles right, assistant plain,
    /// low-weight activity rows), rounded-22 composer with circular send.
    fn render_chat(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(open) = &self.thread else {
            return v_flex()
                .flex_1()
                .h_full()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Select a thread to start chatting")
                .into_any_element();
        };
        let title = self.thread_title(&open.id);
        let view = open.state.view.as_ref();
        let running = view.is_some_and(|view| {
            view.session
                .as_ref()
                .is_some_and(|session| session.status == OrchestrationSessionStatus::Running)
        });
        let session_error: Option<SharedString> = view.and_then(|view| {
            view.session
                .as_ref()
                .and_then(|session| session.last_error.as_ref())
                .map(|error| error.0.clone().into())
        });
        // Model picker: current selection comes from the SHELL copy (meta
        // events are shell-scoped); options come from the server config.
        let current_selection: Option<ModelSelection> = self
            .shell_thread(&open.id)
            .map(|thread| thread.model_selection.clone())
            .or_else(|| view.map(|view| view.model_selection.clone()));
        let current_slug: Option<String> = current_selection
            .as_ref()
            .and_then(|selection| selection.model.as_str().map(str::to_string));
        let current_instance: Option<String> = current_selection
            .as_ref()
            .and_then(|selection| selection.instance_id.clone().flatten())
            .and_then(|value| value.as_str().map(str::to_string));
        let providers = self
            .client
            .as_ref()
            .and_then(|client| client.sessions().borrow().clone())
            .map(|session| session.config.providers.clone())
            .unwrap_or_default();
        let model_label: SharedString = providers
            .iter()
            .flat_map(|provider| provider.models.iter().map(move |model| (provider, model)))
            .find(|(provider, model)| {
                Some(&model.slug.0) == current_slug.as_ref()
                    && (current_instance.is_none()
                        || Some(&provider.instance_id.0) == current_instance.as_ref())
            })
            .map(|(_, model)| SharedString::from(model.name.0.clone()))
            .or_else(|| current_slug.clone().map(SharedString::from))
            .unwrap_or_else(|| "Model".into());

        // Pending approvals / user-input requests are derived from the
        // activity log (the reducer deliberately skips their events); the
        // approval panel takes precedence, like the Electron composer.
        let pending_approvals = view
            .map(|view| derive_pending_approvals(&view.activities))
            .unwrap_or_default();
        let pending_inputs = view
            .map(|view| derive_pending_user_inputs(&view.activities))
            .unwrap_or_default();
        let active_approval = pending_approvals.first().cloned();
        let active_input = if active_approval.is_none() {
            pending_inputs.first().cloned()
        } else {
            None
        };
        let panel: Option<gpui::AnyElement> = {
            let inner = if let Some(approval) = &active_approval {
                Some(self.render_approval_panel(approval, pending_approvals.len(), cx))
            } else {
                active_input
                    .as_ref()
                    .map(|input| self.render_user_input_panel(input, cx))
            };
            inner.map(|inner| {
                div()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().secondary)
                    .child(inner)
                    .into_any_element()
            })
        };

        // Composer: rounded-22 shell, textarea, footer (model label + send).
        let send_button: gpui::AnyElement = if running {
            div()
                .id("stop")
                .size_8()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(cx.theme().danger.opacity(0.9))
                .hover(|style| style.bg(cx.theme().danger))
                .child(
                    div()
                        .size(px(10.))
                        .rounded(px(2.))
                        .bg(cx.theme().danger_foreground),
                )
                .on_click(cx.listener(|this, _, _, cx| this.stop_turn(cx)))
                .into_any_element()
        } else {
            div()
                .id("send")
                .size_8()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(cx.theme().primary.opacity(0.9))
                .hover(|style| style.bg(cx.theme().primary))
                .child(
                    Icon::new(IconName::ArrowUp)
                        .size_4()
                        .text_color(cx.theme().primary_foreground),
                )
                .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))
                .into_any_element()
        };
        // The picker lists every enabled+installed provider's models; picking
        // one dispatches `ThreadMetaUpdate` (see `set_model`).
        let model_picker: gpui::AnyElement = {
            let chat = cx.entity().downgrade();
            let menu_providers: Vec<_> = providers
                .iter()
                .filter(|provider| provider.enabled && provider.installed)
                .cloned()
                .collect();
            let multiple_providers = menu_providers.len() > 1;
            let current_slug = current_slug.clone();
            let current_instance = current_instance.clone();
            Button::new("model-picker")
                .label(model_label)
                .ghost()
                .small()
                .dropdown_menu(move |mut menu, _window, _cx| {
                    for provider in &menu_providers {
                        if multiple_providers {
                            let name = provider
                                .display_name
                                .clone()
                                .flatten()
                                .map(|name| name.0)
                                .unwrap_or_else(|| provider.instance_id.0.clone());
                            menu = menu.item(PopupMenuItem::label(SharedString::from(name)));
                        }
                        for model in &provider.models {
                            let checked = Some(&model.slug.0) == current_slug.as_ref()
                                && (current_instance.is_none()
                                    || Some(&provider.instance_id.0) == current_instance.as_ref());
                            let selection = ModelSelection {
                                instance_id: Some(Some(serde_json::Value::String(
                                    provider.instance_id.0.clone(),
                                ))),
                                model: serde_json::Value::String(model.slug.0.clone()),
                                options: None,
                                provider: None,
                            };
                            let chat = chat.clone();
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(model.name.0.clone()))
                                    .checked(checked)
                                    .on_click(move |_, _, cx| {
                                        let selection = selection.clone();
                                        let _ = chat.update(cx, |this, cx| {
                                            this.set_model(selection, cx);
                                        });
                                    }),
                            );
                        }
                    }
                    menu
                })
                .into_any_element()
        };
        // Context meter (ContextWindowMeter port): usage ring beside the send
        // button, latest `context-window.updated` payload behind it. Electron
        // shows the detail in a hover popover; a tooltip carries it here.
        let context_meter: Option<gpui::AnyElement> = view
            .and_then(|view| derive_latest_context_window_snapshot(&view.activities))
            .map(|usage| {
                let percentage = usage.used_percentage.unwrap_or(0.0).clamp(0.0, 100.0);
                let overloaded = percentage > 90.0;
                let color = if overloaded {
                    cx.theme().danger
                } else {
                    cx.theme().muted_foreground.opacity(0.72)
                };
                let mut tip = match (usage.used_percentage, usage.max_tokens) {
                    (Some(pct), Some(max)) => format!(
                        "Context window {} · {}/{}",
                        format_context_percentage(pct),
                        format_context_tokens(usage.used_tokens),
                        format_context_tokens(max),
                    ),
                    _ => format!(
                        "Context window · {} tokens used",
                        format_context_tokens(usage.used_tokens)
                    ),
                };
                if let Some(total) = usage.total_processed_tokens.filter(|total| *total > 0.0) {
                    tip.push_str(&format!(
                        " · {} total processed",
                        format_context_tokens(total)
                    ));
                }
                if usage.compacts_automatically {
                    tip.push_str(" · compacts automatically");
                }
                Button::new("context-meter")
                    .ghost()
                    .small()
                    .icon(
                        ProgressCircle::new("context-ring")
                            .value(percentage as f32)
                            .color(color)
                            .large(),
                    )
                    .tooltip(SharedString::from(tip))
                    .into_any_element()
            });
        let footer: gpui::AnyElement = if let Some(approval) = &active_approval {
            // The bottom toolbar is replaced by the approval actions while an
            // approval is pending (Electron's ChatComposer does the same).
            self.render_approval_actions(approval, cx)
        } else {
            h_flex()
                .px_3()
                .pb_3()
                .pt_1()
                .justify_between()
                .items_center()
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(
                            Button::new("attach")
                                .ghost()
                                .small()
                                .icon(Icon::new(IconName::Plus))
                                .tooltip("Attach files")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.attach_files(window, cx);
                                })),
                        )
                        .child(model_picker),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .children(context_meter)
                        .child(send_button),
                )
                .into_any_element()
        };
        // @-mention autocomplete rows (top of the composer shell). Rows are
        // plain-text inserts: `@path ` (quoted when the path has spaces).
        let mention_list: Option<gpui::AnyElement> = self
            .mention
            .as_ref()
            .filter(|mention| !mention.results.is_empty())
            .map(|mention| {
                let mut list = v_flex()
                    .w_full()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().secondary);
                for (index, entry) in mention.results.iter().enumerate() {
                    let icon = if entry.kind == ProjectEntryKind::Directory {
                        IconName::Folder
                    } else {
                        IconName::File
                    };
                    list = list.child(
                        h_flex()
                            .id(("mention", index))
                            .px_3()
                            .py_1()
                            .gap_2()
                            .items_center()
                            .cursor_pointer()
                            .text_sm()
                            .when(index == 0, |row| row.bg(cx.theme().accent))
                            .hover(|style| style.bg(cx.theme().accent))
                            .child(
                                Icon::new(icon)
                                    .size_3p5()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(SharedString::from(entry.path.0.clone())),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.apply_mention(index, window, cx);
                            })),
                    );
                }
                list.into_any_element()
            });
        // Staged attachment chips (name + size + remove), Electron's chip row
        // above the textarea.
        let attachment_chips: Option<gpui::AnyElement> = (!self.pending_attachments.is_empty())
            .then(|| {
                let mut chips = h_flex().flex_wrap().gap_1p5().px_3().pt_2();
                for (index, attachment) in self.pending_attachments.iter().enumerate() {
                    let icon = if attachment.is_image {
                        IconName::Eye
                    } else {
                        IconName::File
                    };
                    chips = chips.child(
                        h_flex()
                            .id(("attachment", index))
                            .gap_1p5()
                            .items_center()
                            .pl_2()
                            .pr_1()
                            .py_1()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().secondary)
                            .text_xs()
                            .child(
                                Icon::new(icon)
                                    .size_3()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(
                                div()
                                    .max_w(px(180.))
                                    .truncate()
                                    .child(SharedString::from(attachment.name.clone())),
                            )
                            .child(div().text_color(cx.theme().muted_foreground).child(
                                SharedString::from(format_attachment_size(
                                    attachment.size_bytes.max(0) as u64,
                                )),
                            ))
                            .child(
                                div()
                                    .id(("attachment-remove", index))
                                    .cursor_pointer()
                                    .rounded(px(4.))
                                    .p_0p5()
                                    .text_color(cx.theme().muted_foreground)
                                    .hover(|style| style.bg(cx.theme().accent))
                                    .child(Icon::new(IconName::Close).size_3())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if index < this.pending_attachments.len() {
                                            this.pending_attachments.remove(index);
                                            cx.notify();
                                        }
                                    })),
                            ),
                    );
                }
                chips.into_any_element()
            });
        let composer = div().px_5().pb_4().child(
            v_flex()
                .w_full()
                .max_w(px(768.))
                .mx_auto()
                .rounded(px(22.))
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().muted)
                .overflow_hidden()
                .children(panel)
                .children(mention_list)
                .children(attachment_chips)
                .child(
                    div()
                        .px_2()
                        .pt_2()
                        .child(Textarea::new(&self.composer).appearance(false)),
                )
                .child(footer),
        );

        let mut main = v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .h(px(40.))
                    .px_5()
                    .items_center()
                    .gap_2()
                    .flex_shrink_0()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .font_medium()
                            .truncate()
                            .child(title),
                    )
                    .child(
                        Button::new("toggle-right-panel")
                            .icon(if self.dock_open() {
                                IconName::PanelRightClose
                            } else {
                                IconName::PanelRightOpen
                            })
                            .ghost()
                            .xsmall()
                            .tooltip("Toggle right panel")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_right_panel(window, cx);
                            })),
                    ),
            );
        let banner: Option<SharedString> = self.last_error.clone().or(session_error);
        if let Some(error) = banner {
            main = main.child(
                div().px_5().pt_1().child(
                    div()
                        .mx_auto()
                        .w_auto()
                        .max_w(px(768.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(cx.theme().danger.opacity(0.32))
                        .bg(cx.theme().danger.opacity(0.04))
                        .px_3p5()
                        .py_3()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error),
                ),
            );
        }
        main.child(
            // The timeline is virtualized: only rows near the viewport are
            // built, so a long thread costs the same per frame as a short one.
            // `py_4` sits outside the scroll area (the old column's padding was
            // inside it) — the difference is invisible at the bottom anchor.
            div().flex_1().py_4().child(
                list(
                    self.timeline_list.clone(),
                    cx.processor(|this, index, _window, cx| this.render_timeline_row(index, cx)),
                )
                .size_full(),
            ),
        )
        .child(composer)
        .children(self.render_terminal_drawer(cx))
        .into_any_element()
    }
}
fn describe_status(status: &SupervisorStatus) -> String {
    match status {
        SupervisorStatus::Idle => "sidecar: idle".into(),
        SupervisorStatus::Starting { attempt } => format!("sidecar: starting (attempt {attempt})"),
        SupervisorStatus::Ready { info } => format!("sidecar: 127.0.0.1:{}", info.port),
        SupervisorStatus::Backoff { delay_ms, .. } => {
            format!("sidecar: restarting in {delay_ms}ms")
        }
        SupervisorStatus::Stopped { reason } => format!("sidecar: stopped ({reason})"),
    }
}

impl Render for ChatApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Keep the shared files panel in line with the dock's active surface:
        // recreate it on project switch, apply un-applied file reveals. This
        // runs per frame, so it must not steal focus — user-initiated opens
        // focus through their own `after_dock_change(true)` path.
        let dock_key = self.dock_open().then(|| self.dock_thread_key()).flatten();
        if dock_key.is_some() {
            self.sync_active_file_surface(false, window, cx);
        }
        // The drawer clamps its height against the live viewport; render_chat
        // has no Window, so the frame's height is captured here.
        self.viewport_height = f32::from(window.viewport_size().height);
        let dock_panel = dock_key
            .as_deref()
            .map(|key| self.render_right_panel(key, cx));
        // Electron caps the panel at 70% of the window (`maxWidthPct`).
        let dock_max_width =
            (window.viewport_size().width * 0.7).max(px(right_panel::MIN_PANEL_WIDTH));
        let active_plan = self
            .thread
            .as_ref()
            .and_then(|open| open.state.view.as_ref())
            .and_then(|view| {
                derive_active_plan_state(
                    &view.activities,
                    view.latest_turn.as_ref().map(|turn| &turn.turn_id),
                )
            });
        // `Root` owns the dialog/sheet/notification stacks but does not paint
        // them — the window's content view has to mount the layers itself
        // (gpui-component's own examples do this in their root view). Without
        // this, `open_dialog`/`push_notification` are silently invisible.
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        div()
            .size_full()
            .relative()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &QuickSearchOpen, window, cx| {
                this.toggle_quick_search(QuickSearchMode::Open, window, cx);
            }))
            .on_action(cx.listener(|this, _: &QuickSearchContent, window, cx| {
                this.toggle_quick_search(QuickSearchMode::Content, window, cx);
            }))
            .on_action(cx.listener(|this, _: &CommandPaletteToggle, window, cx| {
                this.toggle_command_palette(window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewThread, window, cx| {
                this.new_thread(None, window, cx);
            }))
            .on_action(cx.listener(|this, _: &RightPanelToggle, window, cx| {
                this.toggle_right_panel(window, cx);
            }))
            .on_action(cx.listener(|this, _: &RightPanelCloseSurface, window, cx| {
                this.dock_close_active_surface(window, cx);
            }))
            .on_action(cx.listener(|this, _: &RightPanelNextSurface, window, cx| {
                this.dock_cycle_surface(1, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &RightPanelPreviousSurface, window, cx| {
                    this.dock_cycle_surface(-1, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &TerminalToggle, _, cx| {
                this.terminal_toggle(cx);
            }))
            .on_action(cx.listener(|this, _: &TerminalSplit, _, cx| {
                this.terminal_split(false, cx);
            }))
            .on_action(cx.listener(|this, _: &TerminalSplitVertical, _, cx| {
                this.terminal_split(true, cx);
            }))
            .on_action(cx.listener(|this, _: &TerminalNew, _, cx| {
                this.terminal_new(cx);
            }))
            .on_action(cx.listener(|this, _: &TerminalCloseActive, _, cx| {
                this.terminal_close_active(cx);
            }))
            // Drawer drag-resize: pointer moves/releases land anywhere in the
            // window, so the root tracks them while a drag is live (Electron
            // attaches the same listeners to `window` on pointerdown).
            .when(self.terminal_drag.is_some(), |this| {
                this.on_mouse_move(cx.listener(|this, event, window, cx| {
                    this.terminal_drag_move(event, window, cx);
                }))
                .on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.terminal_drag_end(cx);
                    }),
                )
            })
            .child(
                h_flex()
                    .size_full()
                    .child(
                        div().flex_1().min_w_0().h_full().child(
                            h_resizable("workspace")
                                .with_state(&self.sidebar_resize)
                                .child(
                                    resizable_panel()
                                        .size(px(256.))
                                        .size_range(px(208.)..px(480.))
                                        .child(self.render_sidebar(cx).into_any_element()),
                                )
                                .child(resizable_panel().child(self.render_chat(cx)))
                                .child(
                                    // Hidden (not unmounted) when the dock is
                                    // closed: the state slot keeps the width,
                                    // so re-opening restores the drag size.
                                    resizable_panel()
                                        .size(px(self.right_panel.width))
                                        .size_range(
                                            px(right_panel::MIN_PANEL_WIDTH)..dock_max_width,
                                        )
                                        .flex_none()
                                        .visible(dock_panel.is_some())
                                        .children(dock_panel),
                                ),
                        ),
                    )
                    .children(
                        active_plan
                            .as_ref()
                            .map(|plan| self.render_plan_sidebar(plan, cx)),
                    ),
            )
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}

#[cfg(test)]
mod tests {
    use super::{active_mention_token, format_mention};

    #[test]
    fn mention_token_at_start_and_after_whitespace() {
        assert_eq!(
            active_mention_token("@src", 4),
            Some((0, "src".to_string()))
        );
        assert_eq!(
            active_mention_token("fix @cra", 8),
            Some((4, "cra".to_string()))
        );
        assert_eq!(active_mention_token("fix @cra", 4), None);
        // Cursor mid-token completes the typed prefix only.
        assert_eq!(active_mention_token("@src tail", 2), Some((0, "s".into())));
    }

    #[test]
    fn mention_token_rejects_non_tokens() {
        assert_eq!(active_mention_token("plain text", 5), None);
        assert_eq!(active_mention_token("user@host", 9), None);
        assert_eq!(active_mention_token("@\"quoted", 8), None);
        assert_eq!(active_mention_token("", 0), None);
        // Whitespace right before the cursor ends the token.
        assert_eq!(active_mention_token("@src ", 5), None);
    }

    #[test]
    fn mention_token_survives_multibyte_boundaries() {
        // "日本 @é" — token after a multibyte space-separated word.
        let text = "日本 @é";
        assert_eq!(
            active_mention_token(text, text.len()),
            Some((7, "é".to_string()))
        );
    }

    #[test]
    fn mention_formatting_quotes_spaces() {
        assert_eq!(format_mention("src/app.ts"), "@src/app.ts ");
        assert_eq!(format_mention("My Docs/a.md"), "@\"My Docs/a.md\" ");
    }
}
