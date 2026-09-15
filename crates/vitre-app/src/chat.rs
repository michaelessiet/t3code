//! M1 chat core UI: thread list sidebar + chat view + composer, rendered
//! from the `vitre-client` watch channels.
//!
//! Detail views merge SHELL data with the detail projection: `subscribeThread`
//! only carries the six detail event kinds, so title/meta always come from
//! the thread shell (the TS client's `mergeEnvironmentThread` split).

mod branch_toolbar;
mod changed_files;
mod diff_panel;
mod drafts;
mod git_actions;
#[cfg(debug_assertions)]
mod menu_verification;
#[cfg(debug_assertions)]
mod phase5_verification;
mod plan_save;
mod polish;
#[cfg(debug_assertions)]
mod polish_verification;
mod preview;
mod preview_automation;
mod preview_capture;
mod preview_recording;
#[cfg(debug_assertions)]
mod preview_verification;
mod project_actions;
mod project_icons;
mod project_scripts;
mod review_comments;
mod right_panel;
mod sidebar;
mod sidebar_beta;
mod terminal_contexts;
mod terminal_drawer;
mod terminal_links;
mod terminal_view;
mod thread_actions;
#[cfg(test)]
mod thread_actions_tests;
#[cfg(debug_assertions)]
pub(crate) mod ui_verification;
mod workspace;

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash as _, Hasher as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use gpui::{
    AnyElement, ClipboardEntry, ClipboardItem, Context, Entity, ExternalPaths, FocusHandle,
    Focusable as _, FollowMode, ListAlignment, ListOffset, ListState, ObjectFit, PathPromptOptions,
    SharedString, StyledImage as _, Subscription, Task, WeakEntity, Window, actions, div, img,
    list, prelude::*, px, relative,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, ResizableState, Root, Sizable as _,
    StyledExt as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex, h_resizable,
    input::{InputEvent, Textarea, TextareaState},
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    progress::ProgressCircle,
    resizable_panel,
    scroll::ScrollableElement,
    text::{TableData, TextView},
    v_flex,
};
use gpui_tokio::Tokio;
use tokio::sync::watch;
use vitre_client::{EnvironmentClient, ShellState, SyncPhase, ThreadHandle, ThreadState};
use vitre_contracts::ClientOrchestrationCommandThreadTurnStartMessageAttachments as TurnAttachment;
use vitre_contracts::methods::{AssetsCreateUrl, ProjectsSearchEntries, SubscribeServerConfig};
use vitre_contracts::{
    ApprovalRequestId, AssetCreateUrlInput, AssetResource, ChatAttachment, ChatFileAttachment,
    ChatImageAttachment, ClientOrchestrationCommand, CommandId, ExecutionEnvironmentPlatformOs,
    MessageId, ModelSelection, NonNegativeInt, OrchestrationLatestTurnState, OrchestrationMessage,
    OrchestrationMessageRole, OrchestrationSessionStatus, OrchestrationThread,
    OrchestrationThreadActivityTone, OrchestrationThreadProposedPlans, OrchestrationThreadShell,
    ProjectEntry, ProjectEntryKind, ProjectId, ProjectSearchEntriesInput, ProviderApprovalDecision,
    ProviderInteractionMode, ProviderOptionDescriptor, RuntimeMode, ServerConfigStreamEvent,
    ThreadId, TrimmedNonEmptyString,
};
use vitre_rpc::TypedStreamEvent;
use vitre_sidecar::SupervisorStatus;
use vitre_state::diff_panel::ordered_turn_diff_summaries;
use vitre_state::local_dispatch::{LocalDispatchSnapshot, merge_optimistic_messages};
use vitre_state::review_comments::{ReviewCommentContext, append_review_comments_to_prompt};
use vitre_state::session_logic::{
    ActivePlanState, ApprovalRequestKind, PendingApproval, PendingUserInput, PlanStepStatus,
    derive_active_plan_state, derive_latest_context_window_snapshot, derive_pending_approvals,
    derive_pending_user_inputs,
};
use vitre_state::terminal_context::{TerminalContextSelection, append_terminal_contexts_to_prompt};
use vitre_state::work_log::{self, DerivedTimelineRow, TimelineDeriveInput, WorkLogEntry};

use crate::assets::VitreIcon;
use crate::client_settings::ClientSettings;
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
        SearchToggle,
        GraphToggle,
        GraphBuild,
        WorkspaceRootsManage,
        /// Show or hide the project/thread sidebar (`mod+b`).
        SidebarToggle,
        /// Electron's `quickSearch.content` (`mod+shift+f`): search chat and
        /// file contents.
        QuickSearchContent,
        /// Electron's `commandPalette.toggle` (`mod+shift+p`).
        CommandPaletteToggle,
        ModelPickerToggle,
        ModelPickerNext,
        ModelPickerPrevious,
        ModelPickerConfirm,
        EditorOpenFavorite,
        ModelPickerJump1,
        ModelPickerJump2,
        ModelPickerJump3,
        ModelPickerJump4,
        ModelPickerJump5,
        ModelPickerJump6,
        ModelPickerJump7,
        ModelPickerJump8,
        ModelPickerJump9,
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
        /// Electron's `preview.toggle` (`mod+shift+j`).
        PreviewToggle,
        /// Browser-local controls (`mod+r`, `mod+l`, zoom chords).
        PreviewRefresh,
        PreviewFocusUrl,
        PreviewZoomIn,
        PreviewZoomOut,
        PreviewResetZoom,
        /// Open or hide the diff surface (`mod+d` outside terminals).
        DiffToggle,
        /// Thread traversal and direct sidebar slots.
        ThreadPrevious,
        ThreadNext,
        ThreadJump1,
        ThreadJump2,
        ThreadJump3,
        ThreadJump4,
        ThreadJump5,
        ThreadJump6,
        ThreadJump7,
        ThreadJump8,
        ThreadJump9,
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
    thread_rename: Option<thread_actions::ThreadRename>,
    draft_project: Option<ProjectId>,
    draft_model: Option<ModelSelection>,
    draft_mode: Option<ProviderInteractionMode>,
    draft_creating: bool,
    draft_send_after_create: Option<ThreadId>,
    graph_panels: HashMap<String, Entity<crate::graph::GraphPanel>>,
    active_roots: HashMap<String, String>,
    workspace_updates: HashSet<ThreadId>,
    drafts: drafts::DraftStore,
    draft_restore: Option<drafts::ComposerDraft>,
    draft_save_task: Option<Task<()>>,
    search_panels: HashMap<String, Entity<crate::search::SearchPanel>>,
    project_icons: project_icons::ProjectIcons,
    welcome_orb: Entity<vitre_bezel_orbs::orbs::Orb>,
    activity_orb: Entity<vitre_bezel_orbs::orbs::Orb>,
    pending_terminal_link: Option<terminal_links::TerminalLink>,
    dock_presentation_key: Option<String>,
    dock_reveal_epoch: usize,
    last_slow_notice: Option<SharedString>,
    client: Option<Arc<EnvironmentClient>>,
    sidecar_status: SharedString,
    request_watchdog: Option<Task<()>>,
    auto_settle_task: Option<Task<()>>,
    slow_requests: Option<SharedString>,
    runtime_notice: Option<SharedString>,
    provider_notices_seen: std::collections::HashSet<String>,
    /// Latest provider-status projection from `subscribeServerConfig`.
    ///
    /// The environment-session bootstrap is only an initial snapshot: CLI
    /// probes (and settings changes) arrive later as provider-status events.
    /// Keeping those events here prevents every chat consumer from remaining
    /// stuck on the bootstrap list while Settings shows the live providers.
    provider_snapshots: Vec<vitre_contracts::ServerProvider>,
    shell: ShellState,
    thread: Option<OpenThread>,
    composer: Entity<TextareaState>,
    last_error: Option<SharedString>,
    /// Immediate mode feedback while the shell-scoped event round-trips.
    pending_interaction_mode: Option<(ThreadId, ProviderInteractionMode)>,
    /// The server state immediately before the current send. Until a newer
    /// projection acknowledges it, the composer stays busy and the timeline
    /// includes the local user bubble below.
    local_dispatch: Option<LocalDispatchSnapshot>,
    /// User messages rendered immediately on send, removed by id as their
    /// server-authored equivalents reach the projection.
    optimistic_user_messages: Vec<OrchestrationMessage>,
    /// Server messages merged with the still-unacknowledged local messages.
    /// Timeline rows index this stable vector rather than the raw projection.
    display_messages: Vec<OrchestrationMessage>,
    /// Visually paced prefixes for assistant messages that are still
    /// streaming. Protocol state remains authoritative in `display_messages`.
    smoothed_assistant_text: HashMap<MessageId, String>,
    smoothing_tick: Option<Task<()>>,
    /// The cleared composer state retained until the dispatch is acknowledged,
    /// so a transport failure can restore an untouched composer for retry.
    pending_send: Option<PendingSend>,
    /// Request ids with an approval / user-input response in flight.
    responding: HashSet<String>,
    /// Local draft state for the active pending user-input request.
    input_draft: Option<InputDraft>,
    /// Files staged to send with the next turn (data-URL attachments).
    pending_attachments: Vec<PendingAttachment>,
    /// Signed server URLs (and optimistic data URLs) keyed by attachment id.
    attachment_urls: HashMap<String, SharedString>,
    attachment_urls_loading: HashSet<String>,
    /// Review comments staged to send with the next turn (Electron's
    /// `composerDraftStore` reviewComments slice; appended to the prompt as
    /// `<review_comment>` blocks on send).
    pending_review_comments: Vec<ReviewCommentContext>,
    /// Terminal selections staged to send with the next turn (the
    /// `composerDraftStore` terminalContexts slice; appended as a trailing
    /// `<terminal_context>` block on send).
    pending_terminal_contexts: Vec<terminal_contexts::PendingTerminalContext>,
    /// Active `@`-mention autocomplete in the composer, if any.
    mention: Option<MentionState>,
    /// Synchronous `/`, `#`, and `$` suggestions. Kept separate from path
    /// search because only `@` needs an RPC round-trip.
    composer_menu: Option<ComposerMenuState>,
    /// Monotonic mention-search counter; stale results are dropped.
    mention_generation: u64,
    /// Two-step revert confirm: the user message armed for revert.
    pending_revert: Option<MessageId>,
    /// A `ThreadCheckpointRevert` is in flight.
    reverting: bool,
    /// Work rows expanded to show their payload detail (keyed by the derived
    /// work-log entry id).
    expanded_work_entries: HashSet<String>,
    /// Derived work-log entries for the open thread (Electron's
    /// `deriveWorkLogEntries`: started/context-window rows dropped, tool
    /// lifecycle merged) — rebuilt with the timeline.
    work_entries: Vec<WorkLogEntry>,
    /// Side tables for the non-`Copy` timeline row kinds.
    work_toggles: Vec<WorkToggleData>,
    turn_folds: Vec<TurnFoldData>,
    /// Settled turns the user re-expanded (Electron `expandedTurnIds`) and
    /// overflow groups shown in full (`expandedWorkGroupIds`). Session-local,
    /// like Electron's component state.
    expanded_turn_ids: HashSet<String>,
    expanded_work_groups: HashSet<String>,
    expanded_plans: HashSet<String>,
    /// Latest turn (id, state) last seen, driving the auto-fold lifecycle:
    /// an interrupt expands its turn, a new turn folds the previous one.
    prev_latest_turn: Option<(String, OrchestrationLatestTurnState)>,
    /// 1s repaint task while a turn runs — the "Working for Ns" timer.
    working_tick: Option<Task<()>>,
    /// Changed-files card state: persisted expansion (`~/.vitre/ui-state.json`)
    /// plus per-turn local UI (auto-expand decision, folder toggles).
    changed_files: changed_files::ChangedFilesState,
    /// Persisted per-provider model favorites used by the model picker.
    model_favorites: ModelFavorites,
    /// Project scripts ("actions") control: dialog + t3.json import offers.
    scripts: project_scripts::ScriptsState,
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
    /// A freshly sent user row is pinned to the top of the viewport while the
    /// answer grows below it (Electron's anchored-send scroll mode).
    pending_timeline_anchor: Option<MessageId>,
    /// Row descriptors backing `timeline_list`, rebuilt when the thread view
    /// changes rather than on every frame.
    timeline: Vec<TimelineRow>,
    /// Height fingerprint per timeline row, parallel to `timeline`.
    timeline_hashes: Vec<u64>,
    /// Long user prompts are collapsed until explicitly expanded.
    expanded_user_messages: HashSet<MessageId>,
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
    sidebar_visible: bool,
    /// Right-hand files panel (M2), hosting the dock's `files`/`file`
    /// surfaces. Kept alive while hidden so tree expansion and the open
    /// buffer survive; on project switch the panel is stashed in
    /// `files_cache` and revived when the user returns (Electron's
    /// listEntries atom cache serves a previously viewed root instantly).
    files: Option<Entity<FilesPanel>>,
    files_review_subscription: Option<Subscription>,
    files_event_subscription: Option<Subscription>,
    /// Stashed panels of previously viewed workspace roots, keyed by cwd,
    /// most recently used last. Bounded — Electron's atom cache evicts after
    /// five idle minutes; Vitre keeps the last few roots instead.
    files_cache: Vec<(String, Entity<FilesPanel>)>,
    /// `${threadKey}|${surfaceId}` of the dock surface last seen active —
    /// the activation edge on which the files listing revalidates (Electron
    /// revalidates on the panel's remount when the surface is selected).
    last_dock_active_surface: Option<String>,
    /// Header git controls (M3): the `subscribeVcsStatus` fold, quick-action/
    /// menu state, and the stacked-action progress pipeline.
    git: git_actions::GitState,
    /// Branch toolbar (M3): the strip above the composer — workspace label,
    /// PR pill, and the paginated branch combobox.
    branch: branch_toolbar::BranchToolbarState,
    /// Diff dock surface (M3), recreated when the dock's thread key or the
    /// active git root changes. `None` until first shown.
    diff: Option<Entity<diff_panel::DiffPanel>>,
    /// Open-file subscription on the current diff panel; replaced on recreate.
    diff_subscription: Option<Subscription>,
    /// Where the diff panel persists its per-thread selections.
    diff_store: PathBuf,
    /// Native browser previews, keyed by `${threadKey}|${surfaceId}`. Keeping
    /// them alive while hidden preserves page state across dock tab changes.
    preview_panels: HashMap<String, Entity<preview::PreviewPanel>>,
    preview_subscriptions: HashMap<gpui::EntityId, Subscription>,
    preview_automation_task: Option<gpui::Task<()>>,
    preview_mini: Option<String>,
    preview_mini_bounds: Option<gpui::Bounds<gpui::Pixels>>,
    preview_mini_drag: Option<(gpui::Point<gpui::Pixels>, gpui::Bounds<gpui::Pixels>, bool)>,
    /// Right-panel dock state (per-thread surfaces) + persisted panel width.
    right_panel: right_panel::RightPanelPrefs,
    dock_tabs_scroll: gpui::ScrollHandle,
    dock_tabs_active: Option<(String, String)>,
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
    /// Whether the right panel temporarily owns the full workspace width.
    right_panel_maximized: bool,
    /// File-surface reveal requests already applied, keyed
    /// `${threadKey}|${surfaceId}` → `reveal_request_id`.
    applied_reveals: HashMap<String, u64>,
    /// The thread key the dock last synced for, to catch thread switches.
    last_dock_sync_key: Option<String>,
    /// The open QuickSearch overlay. Held here because the dialog's content
    /// closure only keeps a weak reference to it.
    quick_search: Option<Entity<QuickSearch>>,
    model_picker: Option<Entity<ModelPickerDialog>>,
    /// The open command palette, held for the same reason.
    command_palette: Option<Entity<CommandPalette>>,
    /// The settings surface, which *replaces* the workspace while open —
    /// Electron makes settings a route, so the chat it covers keeps running
    /// (streams, terminals and the open buffer all survive the visit).
    settings: Option<Entity<crate::settings::SettingsPanel>>,
    /// Live while `settings` is: closes the surface when it asks to leave.
    settings_subscription: Option<Subscription>,
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
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct PendingAttachment {
    name: String,
    mime_type: String,
    size_bytes: i64,
    data_url: String,
    is_image: bool,
}

struct PendingSend {
    message_id: MessageId,
    raw_text: String,
    attachments: Vec<PendingAttachment>,
    review_comments: Vec<ReviewCommentContext>,
    terminal_contexts: Vec<terminal_contexts::PendingTerminalContext>,
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
    selected: usize,
    loading: bool,
}

#[derive(Clone)]
enum ComposerMenuItem {
    Model,
    InteractionMode(ProviderInteractionMode),
    ProviderCommand {
        name: String,
        description: String,
    },
    Thread {
        id: ThreadId,
        title: String,
        detail: String,
    },
    Skill {
        name: String,
        description: String,
    },
}

struct ComposerMenuState {
    token_start: usize,
    token_end: usize,
    selected: usize,
    items: Vec<ComposerMenuItem>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ComposerTriggerKind {
    Path,
    Slash,
    Thread,
    Skill,
}

struct OpenThread {
    id: ThreadId,
    /// Keeps the sync task alive; dropped (aborted) on reselection.
    _handle: ThreadHandle,
    state: ThreadState,
}

struct ModelPickerDialog {
    query: Entity<TextareaState>,
    providers: Vec<vitre_contracts::ServerProvider>,
    selected_provider: Option<String>,
    favorites: HashSet<String>,
    current_selection: Option<(String, String)>,
    thread_started: bool,
    chat: WeakEntity<ChatApp>,
    choices: Vec<(ModelSelection, usize)>,
    active_choice: usize,
    scroll: gpui::ScrollHandle,
    _query_subscription: Subscription,
}

const MODEL_FAVORITES_FILE: &str = "model-favorites.json";
const FAVORITES_PROVIDER_ID: &str = ":favorites";

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StoredModelFavorites {
    favorites: HashSet<String>,
}

struct ModelFavorites {
    values: HashSet<String>,
    path: PathBuf,
}

impl ModelFavorites {
    fn load(home: &Path) -> Self {
        let path = home.join(MODEL_FAVORITES_FILE);
        let values = std::fs::read_to_string(&path)
            .ok()
            .and_then(|contents| serde_json::from_str::<StoredModelFavorites>(&contents).ok())
            .map(|stored| stored.favorites)
            .unwrap_or_default();
        Self { values, path }
    }

    fn toggle(&mut self, key: &str) {
        if !self.values.insert(key.to_string()) {
            self.values.remove(key);
        }
        let stored = StoredModelFavorites {
            favorites: self.values.clone(),
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
            eprintln!("[vitre] failed to persist {MODEL_FAVORITES_FILE}: {error}");
        }
    }
}

fn model_favorite_key(instance_id: &str, model: &str) -> String {
    format!("{instance_id}\0{model}")
}

fn provider_requires_new_thread(provider: &vitre_contracts::ServerProvider) -> bool {
    matches!(
        provider.requires_new_thread_for_model_change,
        Some(Some(true))
    )
}

/// T3 Code keeps every configured, enabled instance in the picker rail, even
/// when the CLI is currently unavailable. Disabled instances remain visible
/// there with an explanation instead of silently disappearing.
fn provider_picker_visible(provider: &vitre_contracts::ServerProvider) -> bool {
    provider.enabled
}

fn provider_picker_selectable(provider: &vitre_contracts::ServerProvider) -> bool {
    provider.enabled
        && provider.availability
            != Some(Some(
                vitre_contracts::ServerProviderAvailability::Unavailable,
            ))
}

/// Only ready and available instances may contribute selectable model rows.
/// `installed` is deliberately not consulted: the web client uses the probe
/// status/availability pair as the source of truth, including remote/custom
/// provider implementations for which a local install flag is not meaningful.
fn provider_picker_ready(provider: &vitre_contracts::ServerProvider) -> bool {
    provider_picker_selectable(provider)
        && provider.status == vitre_contracts::ServerProviderState::Ready
}

fn provider_display_name(provider: &vitre_contracts::ServerProvider) -> String {
    provider
        .display_name
        .clone()
        .flatten()
        .map(|name| name.0)
        .unwrap_or_else(|| provider.instance_id.0.clone())
}

fn provider_picker_tooltip(provider: &vitre_contracts::ServerProvider) -> String {
    let label = provider_display_name(provider);
    if provider_picker_ready(provider) {
        return label;
    }
    if !provider.enabled || provider.status == vitre_contracts::ServerProviderState::Disabled {
        return format!("{label} — Disabled in settings.");
    }
    let kind = match provider.status {
        vitre_contracts::ServerProviderState::Error => "Unavailable",
        vitre_contracts::ServerProviderState::Warning => "Limited",
        _ => "Not ready",
    };
    let message = provider
        .message
        .clone()
        .flatten()
        .or_else(|| provider.unavailable_reason.clone().flatten())
        .map(|message| message.0);
    match message {
        Some(message) => format!("{label} — {kind}. {message}"),
        None => format!("{label} — {kind}."),
    }
}

fn model_selection_identity(selection: &ModelSelection) -> Option<(String, String)> {
    let instance = selection
        .instance_id
        .as_ref()
        .and_then(|value| value.as_ref())
        .and_then(serde_json::Value::as_str)?;
    let model = selection.model.as_str()?;
    Some((instance.to_string(), model.to_string()))
}

impl ModelPickerDialog {
    fn navigate(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.choices.is_empty() {
            return;
        }
        self.active_choice =
            (self.active_choice as isize + delta).rem_euclid(self.choices.len() as isize) as usize;
        self.scroll
            .scroll_to_item(self.choices[self.active_choice].1);
        cx.notify();
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((selection, _)) = self.choices.get(self.active_choice).cloned() else {
            return;
        };
        let _ = self.chat.update(cx, |app, cx| {
            app.set_model(selection, cx);
            app.model_picker = None;
            app.composer.focus_handle(cx).focus(window, cx);
            cx.notify();
        });
    }

    fn jump_provider(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let ids = (!self.favorites.is_empty())
            .then(|| FAVORITES_PROVIDER_ID.to_string())
            .into_iter()
            .chain(
                self.providers
                    .iter()
                    .filter(|provider| provider_picker_ready(provider))
                    .map(|provider| provider.instance_id.0.clone()),
            )
            .collect::<Vec<_>>();
        if let Some(id) = ids.get(index) {
            self.active_choice = 0;
            self.selected_provider = Some(id.clone());
            self.query.update(cx, |q, cx| q.set_value("", window, cx));
            cx.notify();
        }
    }
    fn new(
        providers: Vec<vitre_contracts::ServerProvider>,
        favorites: HashSet<String>,
        current_selection: Option<(String, String)>,
        thread_started: bool,
        chat: WeakEntity<ChatApp>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder("Search models and providers…")
                .auto_grow(1, 1);
            let mut context = gpui::KeyContext::default();
            context.add("ModelPickerInput");
            state.set_extra_key_context(Some(context), cx);
            state
        });
        let query_subscription =
            cx.subscribe(&query, |this: &mut Self, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.active_choice = 0;
                    cx.notify();
                }
            });
        query.read(cx).focus_handle(cx).focus(window, cx);
        let selected_provider = (!favorites.is_empty())
            .then(|| FAVORITES_PROVIDER_ID.to_string())
            .or_else(|| {
                current_selection.as_ref().and_then(|(instance, _)| {
                    providers
                        .iter()
                        .find(|provider| {
                            provider.instance_id.0 == *instance
                                && provider_picker_selectable(provider)
                        })
                        .map(|provider| provider.instance_id.0.clone())
                })
            })
            .or_else(|| {
                providers
                    .iter()
                    .find(|provider| provider_picker_ready(provider))
                    .map(|provider| provider.instance_id.0.clone())
            });
        Self {
            query,
            selected_provider,
            providers,
            favorites,
            current_selection,
            thread_started,
            chat,
            choices: Vec::new(),
            active_choice: 0,
            scroll: gpui::ScrollHandle::new(),
            _query_subscription: query_subscription,
        }
    }
}

impl Render for ModelPickerDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.choices.clear();
        let query = self.query.read(cx).value().trim().to_ascii_lowercase();
        let mut provider_rail = v_flex()
            .w(px(144.))
            .h_full()
            .min_h_0()
            .justify_start()
            .flex_none()
            .overflow_y_scrollbar()
            .gap_1()
            .p_1()
            .border_r_1()
            .border_color(cx.theme().border);
        if !self.favorites.is_empty() {
            let selected = self.selected_provider.as_deref() == Some(FAVORITES_PROVIDER_ID);
            provider_rail = provider_rail.child(
                Button::new("provider-rail-favorites")
                    .label("Favorites")
                    .icon(IconName::Star)
                    .small()
                    .w_full()
                    .justify_start()
                    .when(selected, |button| button.primary())
                    .when(!selected, |button| button.ghost())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.active_choice = 0;
                        this.selected_provider = Some(FAVORITES_PROVIDER_ID.to_string());
                        cx.notify();
                    })),
            );
        }
        for provider in &self.providers {
            let instance_id = provider.instance_id.0.clone();
            let selected = self.selected_provider.as_ref() == Some(&instance_id);
            let ready = provider_picker_ready(provider);
            let label = provider_display_name(provider);
            let tooltip = provider_picker_tooltip(provider);
            provider_rail = provider_rail.child(
                Button::new(SharedString::from(format!("provider-rail-{instance_id}")))
                    .label(label)
                    .icon(crate::icons::provider_icon(&provider.driver.0, cx))
                    .small()
                    .w_full()
                    .justify_start()
                    .when(selected, |button| button.primary())
                    .when(!selected, |button| button.ghost())
                    .disabled(!ready)
                    .tooltip(tooltip)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active_choice = 0;
                        this.selected_provider = Some(instance_id.clone());
                        cx.notify();
                    })),
            );
        }
        let mut rows = v_flex()
            .id("model-rows")
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .gap_1()
            .p_2();
        let mut count = 0usize;
        let mut headers = 0usize;
        for provider in &self.providers {
            // Unavailable enabled providers stay discoverable in the rail,
            // but stale cached models must never become selectable.
            if !provider_picker_ready(provider) {
                continue;
            }
            let favorites_only = query.is_empty()
                && self.selected_provider.as_deref() == Some(FAVORITES_PROVIDER_ID);
            if query.is_empty()
                && !favorites_only
                && self.selected_provider.as_ref() != Some(&provider.instance_id.0)
            {
                continue;
            }
            let provider_name = provider_display_name(provider);
            let mut models: Vec<_> = provider
                .models
                .iter()
                .filter(|model| {
                    let favorite = self
                        .favorites
                        .contains(&model_favorite_key(&provider.instance_id.0, &model.slug.0));
                    (!favorites_only || favorite)
                        && (query.is_empty()
                            || model.name.0.to_ascii_lowercase().contains(&query)
                            || model.slug.0.to_ascii_lowercase().contains(&query)
                            || provider_name.to_ascii_lowercase().contains(&query))
                })
                .collect();
            if !query.is_empty() {
                models.sort_by_key(|model| {
                    std::cmp::Reverse(crate::palette::rank::rank_item_match(
                        &[
                            model.name.0.clone(),
                            model.slug.0.clone(),
                            provider_name.clone(),
                        ],
                        &query,
                    ))
                });
            }
            if models.is_empty() {
                continue;
            }
            rows = rows.child(
                div()
                    .px_2()
                    .pt_2()
                    .pb_1()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(provider_name)),
            );
            headers += 1;
            for model in models {
                count += 1;
                let favorite_key = model_favorite_key(&provider.instance_id.0, &model.slug.0);
                let favorite = self.favorites.contains(&favorite_key);
                let current = self
                    .current_selection
                    .as_ref()
                    .is_some_and(|(instance, slug)| {
                        instance == &provider.instance_id.0 && slug == &model.slug.0
                    });
                let current_provider_requires = self
                    .current_selection
                    .as_ref()
                    .and_then(|(instance, _)| {
                        self.providers
                            .iter()
                            .find(|candidate| candidate.instance_id.0 == *instance)
                    })
                    .is_some_and(provider_requires_new_thread);
                let locked = self.thread_started
                    && !current
                    && (current_provider_requires || provider_requires_new_thread(provider));
                let selection = ModelSelection {
                    instance_id: Some(Some(serde_json::Value::String(
                        provider.instance_id.0.clone(),
                    ))),
                    model: serde_json::Value::String(model.slug.0.clone()),
                    options: None,
                    provider: None,
                };
                let owner = self.chat.clone();
                let highlighted = !locked && self.choices.len() == self.active_choice;
                if !locked {
                    self.choices.push((selection.clone(), count + headers - 1));
                }
                let favorite_owner = self.chat.clone();
                let favorite_key_for_click = favorite_key.clone();
                rows = rows.child(
                    h_flex()
                        .id(SharedString::from(format!(
                            "model-dialog-{}-{}",
                            provider.instance_id.0, model.slug.0
                        )))
                        .px_2()
                        .py_2()
                        .rounded(px(8.))
                        .when(!locked, |row| {
                            row.cursor_pointer()
                                .hover(|style| style.bg(cx.theme().accent))
                        })
                        .when(locked, |row| row.cursor_not_allowed().opacity(0.55))
                        .when(current, |row| row.bg(cx.theme().accent.opacity(0.5)))
                        .when(highlighted, |row| row.bg(cx.theme().accent))
                        .gap_2()
                        .child(crate::icons::provider_icon(&provider.driver.0, cx))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .truncate()
                                        .child(gpui::StyledText::new(model.name.0.clone()).with_highlights(
                                            (!query.is_empty()).then(|| model.name.0.to_ascii_lowercase().find(&query)).flatten()
                                                .map(|start| (start..start + query.len(), gpui::HighlightStyle {
                                                    color: Some(cx.theme().primary), font_weight: Some(gpui::FontWeight::SEMIBOLD), ..Default::default()
                                                })),
                                        )),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .truncate()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(SharedString::from(model.slug.0.clone())),
                                ),
                        )
                        .when(current, |row| row.child(Icon::new(IconName::Check).size_3p5()))
                        .child(
                            Button::new(SharedString::from(format!(
                                "favorite-model-{favorite_key}"
                            )))
                            .ghost()
                            .xsmall()
                            .icon(Icon::new(if favorite {
                                IconName::StarFill
                            } else {
                                IconName::Star
                            }))
                            .tooltip(if favorite {
                                "Remove from favorites"
                            } else {
                                "Add to favorites"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if !this.favorites.insert(favorite_key_for_click.clone()) {
                                    this.favorites.remove(&favorite_key_for_click);
                                }
                                let key = favorite_key_for_click.clone();
                                let _ = favorite_owner.update(cx, |app, cx| {
                                    app.model_favorites.toggle(&key);
                                    cx.notify();
                                });
                                cx.notify();
                            })),
                        )
                        .when(locked, |row| {
                            row.tooltip(|window, cx| {
                                gpui_component::tooltip::Tooltip::new(
                                    "Start a new chat to change models. This provider does not allow switching models after a conversation has started.",
                                )
                                .build(window, cx)
                            })
                        })
                        .when(!locked, |row| {
                            row.on_click(move |_, window, cx| {
                                let selection = selection.clone();
                                let _ = owner.update(cx, |this, cx| {
                                    this.set_model(selection, cx);
                                    this.model_picker = None;
                                    this.composer.focus_handle(cx).focus(window, cx);
                                    cx.notify();
                                });
                            })
                        }),
                );
            }
        }
        if count == 0 {
            rows = rows.child(
                div()
                    .px_3()
                    .py_8()
                    .text_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No matching models"),
            );
        }

        v_flex()
            .key_context("ModelPicker")
            .debug_selector(|| "model-menu".into())
            .on_action(cx.listener(|p, _: &ModelPickerNext, _, cx| p.navigate(1, cx)))
            .on_action(cx.listener(|p, _: &ModelPickerPrevious, _, cx| p.navigate(-1, cx)))
            .on_action(cx.listener(|p, _: &ModelPickerConfirm, w, cx| p.confirm(w, cx)))
            .on_action(cx.listener(|p, _: &ModelPickerJump1, w, cx| p.jump_provider(0, w, cx)))
            .on_action(cx.listener(|p, _: &ModelPickerJump2, w, cx| p.jump_provider(1, w, cx)))
            .on_action(cx.listener(|p, _: &ModelPickerJump3, w, cx| p.jump_provider(2, w, cx)))
            .on_action(cx.listener(|p, _: &ModelPickerJump4, w, cx| p.jump_provider(3, w, cx)))
            .on_action(cx.listener(|p, _: &ModelPickerJump5, w, cx| p.jump_provider(4, w, cx)))
            .on_action(cx.listener(|p, _: &ModelPickerJump6, w, cx| p.jump_provider(5, w, cx)))
            .on_action(cx.listener(|p, _: &ModelPickerJump7, w, cx| p.jump_provider(6, w, cx)))
            .on_action(cx.listener(|p, _: &ModelPickerJump8, w, cx| p.jump_provider(7, w, cx)))
            .on_action(cx.listener(|p, _: &ModelPickerJump9, w, cx| p.jump_provider(8, w, cx)))
            .child(
                div()
                    .p_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(Textarea::new(&self.query)),
            )
            .child(
                h_flex()
                    .h(px(340.))
                    .min_h_0()
                    .child(provider_rail)
                    .child(v_flex().flex_1().min_w_0().h_full().child(rows)),
            )
    }
}

struct ImageLightbox {
    images: Vec<(SharedString, SharedString)>,
    index: usize,
    focus: FocusHandle,
}

impl ImageLightbox {
    fn new(
        images: Vec<(SharedString, SharedString)>,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            images,
            index,
            focus: cx.focus_handle(),
        }
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.images.len() > 1 {
            self.index =
                (self.index as isize + delta).rem_euclid(self.images.len() as isize) as usize;
            cx.notify();
        }
    }
}

impl gpui::Focusable for ImageLightbox {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ImageLightbox {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.focus.focus(window, cx);
        let (url, name) = self.images[self.index].clone();
        let count = self.images.len();
        v_flex()
            .track_focus(&self.focus)
            .size_full()
            .gap_2()
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "left" => this.step(-1, cx),
                    "right" => this.step(1, cx),
                    "escape" => window.close_dialog(cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(
                h_flex()
                    .justify_between()
                    .text_sm()
                    .child(div().truncate().child(name))
                    .child(format!("{} / {count}", self.index + 1)),
            )
            .child(
                h_flex()
                    .min_h(px(560.))
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("lightbox-previous")
                            .ghost()
                            .icon(IconName::ChevronLeft)
                            .disabled(count < 2)
                            .on_click(cx.listener(|this, _, _, cx| this.step(-1, cx))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(img(url).size_full().object_fit(ObjectFit::Contain)),
                    )
                    .child(
                        Button::new("lightbox-next")
                            .ghost()
                            .icon(IconName::ChevronRight)
                            .disabled(count < 2)
                            .on_click(cx.listener(|this, _, _, cx| this.step(1, cx))),
                    ),
            )
    }
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

/// Default model for a new thread: the first picker-ready provider's default
/// model (else its first model). Used when the project carries no
/// `default_model_selection` of its own.
fn default_model_selection(
    providers: &[vitre_contracts::ServerProvider],
    cx: &gpui::App,
) -> Option<ModelSelection> {
    if let Some(selection) = ClientSettings::get(cx).default_model {
        let instance = selection
            .instance_id
            .as_ref()
            .and_then(|v| v.as_ref())
            .and_then(|v| v.as_str());
        if providers.iter().any(|p| {
            provider_picker_selectable(p)
                && Some(p.instance_id.0.as_str()) == instance
                && p.models
                    .iter()
                    .any(|m| Some(m.slug.0.as_str()) == selection.model.as_str())
        }) {
            return Some(selection);
        }
    }
    let provider = providers
        .iter()
        .find(|provider| provider_picker_ready(provider))
        .or_else(|| {
            providers.iter().find(|provider| {
                provider_picker_selectable(provider)
                    && provider.status != vitre_contracts::ServerProviderState::Error
            })
        })?;
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
fn message_timestamp(iso: &str, cx: &gpui::App) -> String {
    let mode = ClientSettings::get(cx).timestamp_format;
    if mode == "locale" {
        return relative_time(iso).unwrap_or_else(|| iso.into());
    }
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format(if mode == "12-hour" {
                    "%b %e, %I:%M %p"
                } else {
                    "%b %e, %H:%M"
                })
                .to_string()
        })
        .unwrap_or_else(|_| iso.into())
}

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

/// RFC-4180-enough CSV for the table copy action: quote cells containing a
/// comma, quote, or line break and double embedded quotes.
fn table_to_csv(table: &TableData) -> String {
    fn cell(value: &str) -> String {
        if value
            .chars()
            .any(|ch| matches!(ch, ',' | '"' | '\n' | '\r'))
        {
            format!("\"{}\"", value.replace('"', "\"\""))
        } else {
            value.to_string()
        }
    }

    std::iter::once(&table.headers)
        .chain(table.rows.iter())
        .map(|row| {
            row.iter()
                .map(|value| cell(value))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn proposed_plan_title(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        let trimmed = line.trim_start();
        let title = trimmed.trim_start_matches('#');
        (title.len() < trimmed.len())
            .then(|| title.trim().to_string())
            .filter(|title| !title.is_empty())
    })
}

fn collapsed_plan_markdown(markdown: &str) -> String {
    let mut lines = Vec::new();
    let mut visible = 0usize;
    for line in markdown.lines() {
        if !line.trim().is_empty() {
            if visible >= 8 {
                lines.push("…");
                break;
            }
            visible += 1;
        }
        lines.push(line);
    }
    lines.join("\n")
}

fn resolve_interaction_mode(
    pending: Option<&ProviderInteractionMode>,
    shell: Option<&ProviderInteractionMode>,
    detail: Option<&ProviderInteractionMode>,
) -> ProviderInteractionMode {
    pending
        .or(shell)
        .or(detail)
        .cloned()
        .unwrap_or(ProviderInteractionMode::Default)
}

fn should_collapse_user_message(text: &str) -> bool {
    !text.trim().is_empty() && (text.len() > 600 || text.lines().count() > 8)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum UserInlineSegment {
    Text(String),
    Thread { label: String, id: ThreadId },
    Skill(String),
}

fn parse_user_inline_segments(text: &str, skills: &HashSet<String>) -> Vec<UserInlineSegment> {
    let mut segments = Vec::new();
    let mut plain_start = 0usize;
    let mut cursor = 0usize;
    while cursor < text.len() {
        let tail = &text[cursor..];
        let thread = tail.strip_prefix("[#").and_then(|after| {
            let label_end = after.find("](")?;
            let href = &after[label_end + 2..];
            let href_end = href.find(')')?;
            let id = href[..href_end].strip_prefix("t3code://thread/")?;
            (!id.is_empty()).then(|| (label_end + 2 + href_end + 1, &after[..label_end], id))
        });
        if let Some((consumed_after_prefix, label, id)) = thread {
            if cursor > plain_start {
                segments.push(UserInlineSegment::Text(
                    text[plain_start..cursor].to_string(),
                ));
            }
            segments.push(UserInlineSegment::Thread {
                label: label.to_string(),
                id: ThreadId(id.to_string()),
            });
            cursor += 2 + consumed_after_prefix;
            plain_start = cursor;
            continue;
        }
        if tail.starts_with('$')
            && (cursor == 0
                || text[..cursor]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            let end = tail[1..]
                .char_indices()
                .take_while(|(_, ch)| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '_' | '-'))
                .last()
                .map_or(1, |(ix, ch)| 1 + ix + ch.len_utf8());
            let name = &tail[1..end];
            let boundary = tail[end..].chars().next().is_none_or(char::is_whitespace);
            if !name.is_empty() && boundary && skills.contains(name) {
                if cursor > plain_start {
                    segments.push(UserInlineSegment::Text(
                        text[plain_start..cursor].to_string(),
                    ));
                }
                segments.push(UserInlineSegment::Skill(name.to_string()));
                cursor += end;
                plain_start = cursor;
                continue;
            }
        }
        cursor += tail.chars().next().map_or(1, char::len_utf8);
    }
    if plain_start < text.len() {
        segments.push(UserInlineSegment::Text(text[plain_start..].to_string()));
    }
    if segments.is_empty() {
        segments.push(UserInlineSegment::Text(text.to_string()));
    }
    segments
}

fn plan_filename(markdown: &str) -> String {
    let stem = proposed_plan_title(markdown)
        .unwrap_or_else(|| "plan".into())
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    let stem = stem.trim_matches('-').replace("--", "-");
    format!("{}.md", if stem.is_empty() { "plan" } else { &stem })
}

/// Add the desktop chat affordances gpui-component deliberately leaves to
/// callers: language + copy for fenced code, and copy/expand for tables.
fn assistant_markdown(id: SharedString, source: SharedString) -> impl IntoElement {
    let copy_source = source.clone();
    div()
        .id(id.clone())
        .debug_selector({
            let id = id.clone();
            move || format!("markdown-context-{id}")
        })
        .w_full()
        .child(
            TextView::markdown(id, source)
                .selectable(true)
                .code_block_actions(|block, _, cx| {
                    let code = block.code().to_string();
                    let language = block.lang().unwrap_or_else(|| "text".into());
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(
                            div()
                                .px_1()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(language),
                        )
                        .child(
                            Button::new("copy-code")
                                .ghost()
                                .xsmall()
                                .icon(Icon::new(IconName::Copy))
                                .tooltip("Copy code")
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                                }),
                        )
                })
                .table_actions(|table, _, _| {
                    let markdown = table.markdown.clone();
                    let csv = table_to_csv(table);
                    let expanded = table.clone();
                    h_flex()
                        .w_full()
                        .justify_end()
                        .gap_1()
                        .child(
                            Button::new("copy-table-markdown")
                                .ghost()
                                .xsmall()
                                .label("Copy MD")
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        markdown.clone(),
                                    ));
                                }),
                        )
                        .child(
                            Button::new("copy-table-csv")
                                .ghost()
                                .xsmall()
                                .label("Copy CSV")
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(csv.clone()));
                                }),
                        )
                        .child(
                            Button::new("expand-table")
                                .ghost()
                                .xsmall()
                                .icon(Icon::new(IconName::Maximize))
                                .tooltip("Expand table")
                                .on_click(move |_, window, cx| {
                                    let source: SharedString = expanded.markdown.clone().into();
                                    window.open_dialog(cx, move |dialog, _, _| {
                                        dialog.title("Table").w(px(900.)).content({
                                            let source = source.clone();
                                            move |content, _, _| {
                                                content.child(
                                                    TextView::markdown(
                                                        "expanded-table",
                                                        source.clone(),
                                                    )
                                                    .selectable(true),
                                                )
                                            }
                                        })
                                    });
                                }),
                        )
                }),
        )
        .context_menu(move |menu, window, cx| {
            let selection = gpui_base::TextSelection::selected_text(window, cx);
            let source = copy_source.to_string();
            menu.item(
                PopupMenuItem::new("Copy selection")
                    .disabled(selection.is_empty())
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(selection.clone()));
                    }),
            )
            .item(
                PopupMenuItem::new("Copy Markdown").on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(source.clone()));
                }),
            )
        })
}

/// The inline token the cursor sits at the end of. This is the byte-indexed
/// native counterpart of `packages/shared/src/composerTrigger.ts`.
fn active_composer_token(
    text: &str,
    cursor: usize,
) -> Option<(ComposerTriggerKind, usize, String)> {
    let head = text.get(..cursor)?;
    let start = head
        .rfind(char::is_whitespace)
        .map(|index| index + head[index..].chars().next().map_or(1, char::len_utf8))
        .unwrap_or(0);
    let token = &head[start..];
    let (kind, query) = match token.as_bytes().first().copied()? {
        b'@' => (ComposerTriggerKind::Path, &token[1..]),
        b'/' => (ComposerTriggerKind::Slash, &token[1..]),
        b'$' => (ComposerTriggerKind::Skill, &token[1..]),
        b'#' => {
            let line_start = head[..start].rfind('\n').map_or(0, |ix| ix + 1);
            if start == line_start && token.bytes().all(|byte| byte == b'#') {
                return None;
            }
            (ComposerTriggerKind::Thread, &token[1..])
        }
        _ => return None,
    };
    if query
        .chars()
        .any(|ch| matches!(ch, '"' | '@' | '/' | '$' | '#'))
    {
        return None;
    }
    Some((kind, start, query.to_string()))
}

fn active_mention_token(text: &str, cursor: usize) -> Option<(usize, String)> {
    let (kind, start, query) = active_composer_token(text, cursor)?;
    (kind == ComposerTriggerKind::Path).then_some((start, query))
}

/// Attachment limits, from `packages/contracts/src/orchestration.ts`.
const MAX_ATTACHMENTS: usize = 8;
const MAX_ATTACHMENT_BYTES: u64 = 10 * 1024 * 1024;
const IMAGE_ONLY_BOOTSTRAP_PROMPT: &str = "[User attached one or more files without additional text. Respond using the conversation context and the attached file(s).]";

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

/// The lucide icon a work-log entry renders with (Electron
/// `workEntryIconName`, mapped from name strings to the vendored assets).
fn work_entry_icon(entry: &WorkLogEntry) -> Icon {
    match work_log::work_entry_icon_name(entry) {
        "message-circle" => Icon::new(VitreIcon::MessageCircle),
        "terminal" => Icon::new(VitreIcon::Terminal),
        "eye" => Icon::new(IconName::Eye),
        "square-pen" => Icon::new(IconName::SquarePen),
        "globe" => Icon::new(IconName::Globe),
        "wrench" => Icon::new(VitreIcon::Wrench),
        "hammer" => Icon::new(VitreIcon::Hammer),
        "circle-alert" => Icon::new(VitreIcon::CircleAlert),
        "bot" => Icon::new(IconName::Bot),
        "check" => Icon::new(IconName::Check),
        "x" => Icon::new(IconName::Close),
        _ => Icon::new(VitreIcon::Zap),
    }
}

/// One row of the virtualized timeline: a position in the thread view's
/// `messages`, in the derived work log or its grouping side tables, or the
/// trailing typing indicator.
///
/// Rows hold indices rather than borrows so the order can be cached on the
/// entity across frames; they are rebuilt whenever the view changes, which is
/// the only time the underlying vectors can move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TimelineRow {
    Message(usize),
    /// Index into the thread projection's `proposed_plans` vector.
    ProposedPlan(usize),
    /// Index into [`ChatApp::work_entries`].
    Work(usize),
    /// Index into [`ChatApp::work_toggles`].
    WorkToggle(usize),
    /// Index into [`ChatApp::turn_folds`].
    TurnFold(usize),
    Running,
}

/// The "+N previous tool calls" overflow row of one consecutive-work group
/// (Electron `WorkGroupToggleTimelineRow`).
#[derive(Clone, PartialEq, Eq, Debug)]
struct WorkToggleData {
    group_id: String,
    hidden_count: usize,
    expanded: bool,
    only_tool_entries: bool,
}

/// The "Worked for …" row folding one settled turn (Electron
/// `TurnFoldTimelineRow`).
#[derive(Clone, PartialEq, Eq, Debug)]
struct TurnFoldData {
    turn_id: String,
    label: SharedString,
    expanded: bool,
}

/// Overdraw roughly a viewport of rows so scrolling doesn't pop.
fn new_timeline_list() -> ListState {
    let state = ListState::new(0, ListAlignment::Bottom, px(800.));
    state.set_follow_mode(FollowMode::Tail);
    state
}

/// Fingerprint of everything in a row that changes its rendered height.
///
/// `ListState` caches the height it measured for each row, so a row whose
/// content grew — a streaming message, a tool result arriving, a detail block
/// expanding — keeps its stale height until it is explicitly remeasured.
/// Comparing fingerprints is how [`ChatApp::rebuild_timeline`] finds those
/// rows without remeasuring (and so re-laying-out) the whole thread.
#[allow(clippy::too_many_arguments)]
fn row_content_hash(
    view: &OrchestrationThread,
    messages: &[OrchestrationMessage],
    row: TimelineRow,
    revert_turn_counts: &HashMap<MessageId, i64>,
    expanded_work_entries: &HashSet<String>,
    thread_key: Option<&str>,
    changed_files: &changed_files::ChangedFilesState,
    work_entries: &[WorkLogEntry],
    work_toggles: &[WorkToggleData],
    turn_folds: &[TurnFoldData],
    expanded_plans: &HashSet<String>,
    workspace_root: Option<&str>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        TimelineRow::Message(index) => {
            let Some(message) = messages.get(index) else {
                return 0;
            };
            0u8.hash(&mut hasher);
            message.id.0.hash(&mut hasher);
            message.role.hash(&mut hasher);
            message.text.0.hash(&mut hasher);
            message.streaming.hash(&mut hasher);
            if let Some(attachments) = message
                .attachments
                .as_ref()
                .and_then(|attachments| attachments.as_ref())
            {
                for attachment in attachments {
                    match attachment {
                        ChatAttachment::ChatImageAttachment(image) => {
                            0u8.hash(&mut hasher);
                            image.id.0.hash(&mut hasher);
                            image.name.0.hash(&mut hasher);
                        }
                        ChatAttachment::ChatFileAttachment(file) => {
                            1u8.hash(&mut hasher);
                            file.id.0.hash(&mut hasher);
                            file.name.0.hash(&mut hasher);
                        }
                        ChatAttachment::Unknown(value) => value.to_string().hash(&mut hasher),
                    }
                }
            }
            revert_turn_counts
                .contains_key(&message.id)
                .hash(&mut hasher);
            if message.role == OrchestrationMessageRole::Assistant {
                changed_files.hash_card(&mut hasher, thread_key, view, &message.id);
            }
        }
        TimelineRow::ProposedPlan(index) => {
            5u8.hash(&mut hasher);
            if let Some(plan) = view
                .proposed_plans
                .as_ref()
                .and_then(|plans| plans.as_ref())
                .and_then(|plans| plans.get(index))
            {
                plan.id.hash(&mut hasher);
                plan.plan_markdown.hash(&mut hasher);
                expanded_plans.contains(&plan.id).hash(&mut hasher);
            }
        }
        TimelineRow::Work(index) => {
            let Some(entry) = work_entries.get(index) else {
                return 0;
            };
            1u8.hash(&mut hasher);
            entry.id.hash(&mut hasher);
            work_log::work_entry_heading(entry).hash(&mut hasher);
            work_log::work_entry_preview(entry).hash(&mut hasher);
            let expanded = expanded_work_entries.contains(&entry.id);
            expanded.hash(&mut hasher);
            let body = work_log::build_tool_call_expanded_body(entry, workspace_root);
            if expanded {
                body.hash(&mut hasher);
            } else {
                body.is_some().hash(&mut hasher);
            }
        }
        TimelineRow::WorkToggle(index) => {
            let Some(toggle) = work_toggles.get(index) else {
                return 0;
            };
            3u8.hash(&mut hasher);
            toggle.group_id.hash(&mut hasher);
            toggle.hidden_count.hash(&mut hasher);
            toggle.expanded.hash(&mut hasher);
            toggle.only_tool_entries.hash(&mut hasher);
        }
        TimelineRow::TurnFold(index) => {
            let Some(fold) = turn_folds.get(index) else {
                return 0;
            };
            4u8.hash(&mut hasher);
            fold.turn_id.hash(&mut hasher);
            fold.label.hash(&mut hasher);
            fold.expanded.hash(&mut hasher);
        }
        // Constant: the ticking "Working for Ns" text never changes the
        // row's height, so it must not trigger remeasures.
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
                    // While a composer popover is open, Enter accepts its
                    // highlighted row instead of sending.
                    let accepted = this.apply_active_composer_suggestion(window, cx);
                    if !accepted {
                        this.send(window, cx);
                    }
                }
                InputEvent::Change => {
                    this.sync_mention(cx);
                    this.sync_composer_menu(cx);
                    this.schedule_draft_save(cx);
                }
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
                    app.client = Some(client.clone());
                    // Settings opened in the sub-second window before the
                    // client existed would otherwise sit on "not connected"
                    // for as long as it stayed open.
                    if let Some(settings) = app.settings.clone() {
                        settings.update(cx, |panel, cx| panel.attach_client(client, cx));
                    }
                    app.spawn_terminal_metadata_loop(cx);
                    app.spawn_server_keymap_loop(cx);
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
                        if let Some((thread_id, pending)) = &app.pending_interaction_mode
                            && app.shell_thread(thread_id).is_some_and(|thread| {
                                thread
                                    .interaction_mode
                                    .as_ref()
                                    .and_then(|mode| mode.as_ref())
                                    == Some(pending)
                            })
                        {
                            app.pending_interaction_mode = None;
                        }
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
        subscriptions.push(cx.observe_global::<ClientSettings>(|_: &mut Self, cx| cx.notify()));
        cx.on_app_quit(move |app, cx| {
            app.save_composer_draft(cx);
            app.drafts.flush();
            async {}
        })
        .detach();

        Self {
            welcome_orb: polish::welcome_orb(cx),
            thread_rename: None,
            draft_project: None,
            draft_model: None,
            draft_mode: None,
            draft_creating: false,
            draft_send_after_create: None,
            graph_panels: HashMap::new(),
            active_roots: right_panel.active_roots.clone(),
            workspace_updates: HashSet::new(),
            drafts: drafts::DraftStore::new(home),
            draft_restore: None,
            draft_save_task: None,
            search_panels: HashMap::new(),
            project_icons: project_icons::ProjectIcons::default(),
            activity_orb: polish::activity_orb(cx),
            pending_terminal_link: None,
            dock_presentation_key: None,
            dock_reveal_epoch: 0,
            last_slow_notice: None,
            client: None,
            sidecar_status: "starting…".into(),
            request_watchdog: None,
            auto_settle_task: None,
            slow_requests: None,
            runtime_notice: None,
            provider_notices_seen: Default::default(),
            provider_snapshots: Vec::new(),
            shell: ShellState::default(),
            thread: None,
            composer,
            last_error: None,
            pending_interaction_mode: None,
            local_dispatch: None,
            optimistic_user_messages: Vec::new(),
            display_messages: Vec::new(),
            smoothed_assistant_text: HashMap::new(),
            smoothing_tick: None,
            pending_send: None,
            responding: HashSet::new(),
            input_draft: None,
            pending_attachments: Vec::new(),
            attachment_urls: HashMap::new(),
            attachment_urls_loading: HashSet::new(),
            pending_review_comments: Vec::new(),
            pending_terminal_contexts: Vec::new(),
            mention: None,
            composer_menu: None,
            mention_generation: 0,
            pending_revert: None,
            changed_files: changed_files::ChangedFilesState::load(home),
            model_favorites: ModelFavorites::load(home),
            scripts: project_scripts::ScriptsState::default(),
            debug_open_thread: std::env::var("VITRE_OPEN_THREAD")
                .ok()
                .filter(|value| !value.is_empty()),
            reverting: false,
            expanded_work_entries: HashSet::new(),
            work_entries: Vec::new(),
            work_toggles: Vec::new(),
            turn_folds: Vec::new(),
            expanded_turn_ids: HashSet::new(),
            expanded_work_groups: HashSet::new(),
            expanded_plans: HashSet::new(),
            prev_latest_turn: None,
            working_tick: None,
            timeline_list: new_timeline_list(),
            pending_timeline_anchor: None,
            timeline: Vec::new(),
            timeline_hashes: Vec::new(),
            expanded_user_messages: HashSet::new(),
            revert_turn_counts: HashMap::new(),
            sidebar,
            sidebar_rows: Vec::new(),
            sidebar_project_order: Vec::new(),
            project_rename: None,
            project_grouping: None,
            sidebar_resize,
            sidebar_visible: true,
            files: None,
            files_review_subscription: None,
            files_event_subscription: None,
            files_cache: Vec::new(),
            last_dock_active_surface: None,
            git: git_actions::GitState::default(),
            branch: branch_toolbar::BranchToolbarState::new(window, cx),
            diff: None,
            diff_subscription: None,
            diff_store: home.join(diff_panel::FILE_NAME),
            preview_panels: HashMap::new(),
            preview_subscriptions: HashMap::new(),
            preview_automation_task: None,
            preview_mini: None,
            preview_mini_bounds: None,
            preview_mini_drag: None,
            right_panel,
            dock_tabs_scroll: gpui::ScrollHandle::new(),
            dock_tabs_active: None,
            terminal_ui: terminal_drawer::TerminalPrefs::load(home),
            terminal_views: HashMap::new(),
            terminal_view_subs: HashMap::new(),
            terminal_metadata: Vec::new(),
            terminal_drag: None,
            viewport_height: 720.,
            right_panel_maximized: false,
            applied_reveals: HashMap::new(),
            last_dock_sync_key: None,
            quick_search: None,
            model_picker: None,
            command_palette: None,
            settings: None,
            settings_subscription: None,
            focus_handle,
            _subscriptions: subscriptions,
        }
    }

    /// Install the resolved server keymap and follow edits made by this or
    /// another client. Reconnects begin from the fresh session snapshot, so a
    /// sidecar restart cannot leave stale chords active.
    fn announce_provider_updates(
        &mut self,
        providers: &[vitre_contracts::ServerProvider],
        cx: &mut Context<Self>,
    ) {
        let mut updates = Vec::new();
        for provider in providers {
            if !provider.enabled {
                continue;
            }
            if let Some(advisory) = &provider.version_advisory
                && advisory.status
                    == vitre_contracts::ServerProviderVersionAdvisoryStatus::BehindLatest
            {
                let version = advisory
                    .latest_version
                    .as_ref()
                    .map(|v| v.0.as_str())
                    .unwrap_or("latest");
                if self
                    .provider_notices_seen
                    .insert(format!("{}:{version}", provider.instance_id.0))
                {
                    updates.push(format!("{} ({version})", provider.instance_id.0));
                }
            }
        }
        if !updates.is_empty() {
            self.runtime_notice = Some(
                format!(
                    "Provider updates available: {}. Open Settings → Providers to update.",
                    updates.join(", ")
                )
                .into(),
            );
            cx.notify();
        }
    }

    fn set_provider_snapshots(
        &mut self,
        providers: Vec<vitre_contracts::ServerProvider>,
        cx: &mut Context<Self>,
    ) {
        self.announce_provider_updates(&providers, cx);
        self.provider_snapshots = providers;

        // A picker may be open while a CLI probe or Settings mutation lands.
        // Reconcile it in place so provider rows do not require closing and
        // reopening the popover to become accurate.
        if let Some(picker) = self.model_picker.clone() {
            let visible: Vec<_> = self
                .provider_snapshots
                .iter()
                .filter(|provider| provider_picker_visible(provider))
                .cloned()
                .collect();
            picker.update(cx, |picker, cx| {
                let selected_is_usable =
                    picker.selected_provider.as_deref().is_some_and(|selected| {
                        selected == FAVORITES_PROVIDER_ID
                            || visible.iter().any(|provider| {
                                provider.instance_id.0 == selected
                                    && provider_picker_selectable(provider)
                            })
                    });
                if !selected_is_usable {
                    picker.selected_provider = visible
                        .iter()
                        .find(|provider| provider_picker_ready(provider))
                        .map(|provider| provider.instance_id.0.clone());
                    picker.active_choice = 0;
                }
                picker.providers = visible;
                cx.notify();
            });
        }
        cx.notify();
    }

    fn spawn_server_keymap_loop(&self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
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
                let initial = handle.config.keybindings.clone();
                let initial_providers = handle.config.providers.clone();
                if this
                    .update(cx, |app, cx| {
                        crate::keymap::install(&initial, cx);
                        app.set_provider_snapshots(initial_providers, cx);
                    })
                    .is_err()
                {
                    return;
                }
                let Ok(mut subscription) = handle
                    .session
                    .subscribe_typed::<SubscribeServerConfig>(&serde_json::json!({}))
                else {
                    if sessions.changed().await.is_err() {
                        return;
                    }
                    continue;
                };
                loop {
                    tokio::select! {
                        event = subscription.next() => match event {
                            Some(TypedStreamEvent::Values(events)) => {
                                for event in events {
                                    let bindings = match event {
                                        ServerConfigStreamEvent::ServerConfigStreamSnapshotEvent(event) => {
                                            let providers = event.config.providers.clone();
                                            let _ = this.update(cx, |app, cx| app.set_provider_snapshots(providers, cx));
                                            event.config.keybindings
                                        },
                                        ServerConfigStreamEvent::ServerConfigStreamKeybindingsUpdatedEvent(event) => {
                                            let _=this.update(cx,|app,cx|{app.runtime_notice=Some("Keyboard shortcuts updated".into());cx.notify();});event.payload.keybindings
                                        },
                                        ServerConfigStreamEvent::ServerConfigStreamProviderStatusesEvent(event) => {
                                            let _ = this.update(cx, |app, cx| {
                                                app.set_provider_snapshots(event.payload.providers, cx)
                                            });
                                            continue;
                                        },
                                        _ => continue,
                                    };
                                    if this.update(cx, |_, cx| crate::keymap::install(&bindings, cx)).is_err() {
                                        return;
                                    }
                                }
                                if subscription.ack().is_err() { break; }
                            }
                            Some(TypedStreamEvent::Completed(_)) => break,
                            None => break,
                        },
                        changed = sessions.changed() => {
                            if changed.is_err() { return; }
                            break;
                        }
                    }
                }
            }
        })
        .detach();
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

    /// Show the settings surface, or leave it if it is already up (Electron's
    /// menu item is a no-op once you are on `/settings`).
    pub fn toggle_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_some() {
            self.close_settings(window, cx);
            return;
        }
        let panel = cx.new(|cx| {
            crate::settings::SettingsPanel::new(self.client.clone(), self.open_project_root(), cx)
        });
        self.settings_subscription = Some(cx.subscribe_in(
            &panel,
            window,
            |this, _, _: &crate::settings::SettingsClosed, window, cx| {
                this.close_settings(window, cx)
            },
        ));
        // Escape is bound in the panel's own key context, so the surface has
        // to actually hold focus for it to answer.
        window.focus(&gpui::Focusable::focus_handle(&panel, cx), cx);
        self.settings = Some(panel);
        cx.notify();
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings = None;
        self.settings_subscription = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
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
            default_model_selection: default_model_selection(&self.provider_snapshots, cx),
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
            PaletteAction::WorkspaceSearch => self.open_workspace_surface(
                vitre_state::right_panel::SurfaceKind::Search,
                window,
                cx,
            ),
            PaletteAction::KnowledgeGraph => self.open_workspace_surface(
                vitre_state::right_panel::SurfaceKind::Graph,
                window,
                cx,
            ),
            PaletteAction::WorkspaceFolders => self.attach_workspace_folder(window, cx),
            PaletteAction::BuildGraph => {
                self.open_workspace_surface(
                    vitre_state::right_panel::SurfaceKind::Graph,
                    window,
                    cx,
                );
                if let Some(panel) = self
                    .search_root()
                    .and_then(|r| self.graph_panels.get(&r).cloned())
                {
                    panel.update(cx, |panel, cx| panel.build(cx));
                }
            }
            // The files panel follows the preference through its global
            // observer, so flipping it here is the whole handler.
            PaletteAction::ToggleVimMode => {
                crate::client_settings::ClientSettings::toggle_vim_mode(cx);
            }
            PaletteAction::OpenSettings => self.toggle_settings(window, cx),
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

    /// Create (or swap in, when the open thread's project changed) the files
    /// panel for the active workspace root. Panels of previously viewed
    /// roots are stashed rather than dropped, so switching back shows the
    /// old tree instantly and revalidates in the background — Electron's
    /// listEntries atom cache behaves the same way.
    fn ensure_files_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        /// How many background roots to keep warm (each holds a watch loop).
        const FILES_CACHE_ROOTS: usize = 4;
        let (Some(client), Some(cwd)) = (self.client.clone(), self.search_root()) else {
            return;
        };
        if self
            .files
            .as_ref()
            .is_some_and(|panel| panel.read(cx).cwd() == cwd)
        {
            return;
        }
        if let Some(previous) = self.files.take() {
            let previous_cwd = previous.read(cx).cwd().to_string();
            self.files_cache.retain(|(key, _)| key != &previous_cwd);
            self.files_cache.push((previous_cwd, previous));
            if self.files_cache.len() > FILES_CACHE_ROOTS {
                // Unsaved buffers remain owned even when several roots are open.
                if let Some(index) = self.files_cache.iter().position(|(_, panel)| {
                    panel
                        .read(cx)
                        .active_file_status()
                        .is_none_or(|(_, dirty)| !dirty)
                }) {
                    self.files_cache.remove(index);
                }
            }
        }
        let revived = self
            .files_cache
            .iter()
            .position(|(key, _)| key == &cwd)
            .map(|index| self.files_cache.remove(index).1);
        let panel = match revived {
            Some(panel) => {
                panel.update(cx, |panel, cx| panel.revalidate_if_stale(cx));
                panel
            }
            None => cx.new(|cx| FilesPanel::new(client, cwd, window, cx)),
        };
        self.files_review_subscription = Some(cx.subscribe_in(
            &panel,
            window,
            |app, _, comment: &ReviewCommentContext, window, cx| {
                app.open_editor_comment(comment.clone(), window, cx)
            },
        ));
        self.files_event_subscription = Some(cx.subscribe_in(&panel, window, |app, source, event: &crate::files::FilesEvent, window, cx| {
            match event {
                crate::files::FilesEvent::Opened(path) => {
                    if let Some(key) = app.dock_thread_key() {
                        let cwd = source.read(cx).cwd().to_string();
                        if app.search_root().as_ref() != Some(&cwd) { return; }
                        let root = Some(cwd).filter(|r| Some(r) != app.open_project_root().as_ref());
                        if app.right_panel.map.active_surface(&key).is_some_and(|s| matches!(s, vitre_state::right_panel::RightPanelSurface::File { relative_path, root_path, .. } if relative_path == path && root_path == &root)) { return; }
                        app.right_panel.map.open_file(&key, path, None, None, root.as_deref());
                        app.right_panel.save(); cx.notify();
                    }
                }
                crate::files::FilesEvent::AddToChat(context) => app.insert_file_context(context, window, cx),
                crate::files::FilesEvent::CopyToThread(context) => app.copy_to_thread_picker(context.clone(),window,cx),
            }
        }));
        self.files = Some(panel);
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
                    diff_panel::DiffPanelEvent::AddReviewComment(comment) => {
                        this.add_review_comment(comment.clone(), cx);
                    }
                    diff_panel::DiffPanelEvent::RemoveReviewComment { id } => {
                        this.remove_review_comment(id, cx);
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
            panel.update(cx, |panel, cx| {
                panel.set_checkpoints(ordered, cx);
                panel.set_review_comments(self.pending_review_comments.clone(), cx);
            });
        }
    }

    /// Start a thread in `project_id`, or — when it is `None` — in the project
    /// the view is already pointed at, falling back to the first known one
    /// (Electron's `startNewThreadFromContext`).
    fn create_draft_thread(
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
        if client.sessions().borrow().is_none() {
            return;
        }
        let Some(model_selection) = self
            .draft_model
            .clone()
            .or_else(|| {
                project
                    .as_ref()
                    .and_then(|project| project.default_model_selection.clone())
            })
            .or_else(|| default_model_selection(&self.provider_snapshots, cx))
        else {
            window.push_notification(Notification::error("No provider configured."), cx);
            return;
        };
        let thread_id = ThreadId(fresh_id("vitre-thread"));
        if self.draft_creating {
            return;
        }
        self.draft_creating = true;
        let draft = self.capture_composer_draft(cx);
        let draft_key = format!("project:{}", project_id.0);
        let command = ClientOrchestrationCommand::ThreadCreate {
            additional_roots: None,
            branch: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            interaction_mode: self.draft_mode.clone().map(Some),
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
                    let _ = this.update(cx, |app, cx| {
                        app.draft_creating = false;
                        if let Err(error) = app.drafts.save(&thread_id.0, &draft) {
                            app.runtime_notice =
                                Some(format!("Could not save draft: {error}").into());
                        }
                        // A thread selection made during creation must not be
                        // stolen by this async completion or start a surprise turn.
                        if app.thread.is_some() || app.draft_project.as_ref() != Some(&project_id) {
                            cx.notify();
                            return;
                        }
                        app.select_thread(thread_id.clone(), cx);
                        app.draft_send_after_create = Some(thread_id);
                        let _ = app
                            .drafts
                            .save(&draft_key, &drafts::ComposerDraft::default());
                    });
                }
                Err(error) => {
                    let _ = this.update_in(cx, |app, window, cx| {
                        app.draft_creating = false;
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

    fn implement_plan_in_new_thread(
        &mut self,
        plan: OrchestrationThreadProposedPlans,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(client), Some(open)) = (self.client.clone(), self.thread.as_ref()) else {
            return;
        };
        let Some(shell) = self.shell_thread(&open.id).cloned() else {
            return;
        };
        let source_thread_id = open.id.clone();
        let operation = format!("implement-plan:{}:{}", source_thread_id.0, plan.id);
        if !self.responding.insert(operation.clone()) {
            return;
        }
        cx.notify();
        let runtime_mode = open
            .state
            .view
            .as_ref()
            .map(|view| view.runtime_mode.clone())
            .unwrap_or(RuntimeMode::FullAccess);
        let thread_id = ThreadId(fresh_id("vitre-thread"));
        let title = proposed_plan_title(&plan.plan_markdown)
            .map(|title| format!("Implement: {title}"))
            .unwrap_or_else(|| "Implement plan".to_string());
        let create = ClientOrchestrationCommand::ThreadCreate {
            additional_roots: None,
            branch: shell.branch.clone(),
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            interaction_mode: Some(Some(ProviderInteractionMode::Default)),
            model_selection: shell.model_selection.clone(),
            project_id: shell.project_id.clone(),
            runtime_mode: runtime_mode.clone(),
            thread_id: thread_id.clone(),
            title: tnes(title),
            r#type: Default::default(),
            worktree_path: shell.worktree_path.clone(),
        };
        let turn = ClientOrchestrationCommand::ThreadTurnStart {
            bootstrap: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            interaction_mode: ProviderInteractionMode::Default,
            message: vitre_contracts::ClientOrchestrationCommandThreadTurnStartMessage {
                attachments: Vec::new(),
                message_id: MessageId(fresh_id("vitre-msg")),
                role: Default::default(),
                text: tnes(format!(
                    "PLEASE IMPLEMENT THIS PLAN:\n{}",
                    plan.plan_markdown.trim()
                )),
            },
            model_selection: None,
            runtime_mode,
            source_proposed_plan: Some(Some(
                vitre_contracts::ClientOrchestrationCommandThreadTurnStartSourceProposedPlan {
                    plan_id: tnes(plan.id),
                    thread_id: source_thread_id,
                },
            )),
            thread_id: thread_id.clone(),
            title_seed: None,
            r#type: Default::default(),
        };
        cx.spawn_in(window, async move |this, cx| {
            let mut created = false;
            let result = async {
                client.dispatch(&create).await?;
                created = true;
                client.dispatch(&turn).await
            }
            .await;
            let _ = this.update(cx, |app, cx| { app.responding.remove(&operation); cx.notify(); });
            match result {
                Ok(_) => {
                    let _ = this.update(cx, |app, cx| app.select_thread(thread_id, cx));
                }
                Err(error) => {
                    let _ = this.update_in(cx, |app, window, cx| {
                        let owner = cx.entity().downgrade();
                        let target = thread_id.clone();
                        window.push_notification(
                            Notification::error(format!(
                                "{}: {}",
                                if created { "The new thread was created, but starting the plan was not confirmed. Click to inspect it before retrying" } else { "Could not create a thread for this plan" }, error.user_message()
                            )).on_click(move |_, _, cx| {
                                if created { let _ = owner.update(cx, |app, cx| app.select_thread(target.clone(), cx)); }
                            }),
                            cx,
                        );
                        app.responding.remove(&operation);
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

    pub(crate) fn open_deep_link(&mut self, id: String, cx: &mut Context<Self>) {
        self.debug_open_thread = Some(id);
        self.maybe_open_debug_thread(cx);
        cx.activate(true);
        cx.notify();
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
        self.save_composer_draft(cx);
        self.draft_project = None;
        self.draft_restore = Some(self.drafts.load(&id.0));
        self.input_draft = None;
        self.mention = None;
        self.composer_menu = None;
        self.pending_attachments.clear();
        self.pending_review_comments.clear();
        self.pending_terminal_contexts.clear();
        self.local_dispatch = None;
        self.optimistic_user_messages.clear();
        self.display_messages.clear();
        self.smoothed_assistant_text.clear();
        self.smoothing_tick = None;
        self.pending_send = None;
        self.pending_revert = None;
        self.pending_interaction_mode = None;
        self.pending_timeline_anchor = None;
        self.expanded_user_messages.clear();
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
        // Retarget the vcs status stream at the new thread's git root.
        self.sync_git_status(cx);
        // Retarget the branch toolbar (drops stale pages + optimistic label).
        self.sync_branch_toolbar(cx);
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
                        app.reconcile_local_dispatch();
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

    /// Drop the open thread and return to the home view — Electron's
    /// navigate-to-`/` after deleting the last thread of a project. The same
    /// per-thread teardown as `select_thread`, with nothing opened after.
    fn close_open_thread(&mut self, cx: &mut Context<Self>) {
        if self.thread.is_none() {
            return;
        }
        self.save_composer_draft(cx);
        self.draft_restore = Some(drafts::ComposerDraft::default());
        self.input_draft = None;
        self.mention = None;
        self.composer_menu = None;
        self.pending_attachments.clear();
        self.pending_review_comments.clear();
        self.pending_terminal_contexts.clear();
        self.local_dispatch = None;
        self.optimistic_user_messages.clear();
        self.display_messages.clear();
        self.smoothed_assistant_text.clear();
        self.smoothing_tick = None;
        self.pending_send = None;
        self.pending_revert = None;
        self.pending_timeline_anchor = None;
        self.expanded_user_messages.clear();
        self.changed_files.clear_local();
        self.timeline = Vec::new();
        self.timeline_hashes = Vec::new();
        self.revert_turn_counts = HashMap::new();
        self.timeline_list = new_timeline_list();
        self.terminal_views.clear();
        self.terminal_view_subs.clear();
        self.terminal_drag = None;
        self.thread = None;
        self.rebuild_sidebar();
        self.sync_git_status(cx);
        self.sync_branch_toolbar(cx);
        cx.notify();
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.thread.is_none() {
            if !self.composer.read(cx).value().trim().is_empty()
                || !self.pending_attachments.is_empty()
            {
                self.create_draft_thread(self.draft_project.clone(), window, cx);
            }
            return;
        }
        // Electron disables the primary action while a local dispatch awaits
        // projection acknowledgment. This also prevents two optimistic rows
        // from competing for the single retry snapshot.
        if self.local_dispatch.is_some() {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(open) = &self.thread else {
            return;
        };
        let thread_id = open.id.clone();
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
        // Electron's `deriveComposerSendState`: pending terminal contexts and
        // review comments each make an empty prompt sendable — the message
        // body becomes just the context blocks.
        let raw_text = self.composer.read(cx).value().trim().to_string();
        let actionable_plan = self
            .shell_thread(&thread_id)
            .is_some_and(|thread| thread.has_actionable_proposed_plan)
            .then(|| {
                view.proposed_plans
                    .as_ref()
                    .and_then(|plans| plans.as_ref())
                    .and_then(|plans| {
                        plans
                            .iter()
                            .rev()
                            .find(|plan| !matches!(plan.implemented_at, Some(Some(Some(_)))))
                    })
                    .cloned()
            })
            .flatten();
        if self.pending_attachments.is_empty()
            && self.pending_terminal_contexts.is_empty()
            && self.pending_review_comments.is_empty()
        {
            let standalone_mode = match raw_text.to_ascii_lowercase().as_str() {
                "/plan" => Some(ProviderInteractionMode::Plan),
                "/default" => Some(ProviderInteractionMode::Default),
                _ => None,
            };
            if let Some(mode) = standalone_mode {
                self.composer
                    .update(cx, |input, cx| input.clean(window, cx));
                self.set_interaction_mode(mode, cx);
                return;
            }
        }
        if raw_text.is_empty()
            && self.pending_attachments.is_empty()
            && self.pending_terminal_contexts.is_empty()
            && self.pending_review_comments.is_empty()
            && actionable_plan.is_none()
        {
            return;
        }
        let selected_interaction_mode = self.current_interaction_mode(&thread_id);
        let pending_terminal_contexts = std::mem::take(&mut self.pending_terminal_contexts);
        let terminal_contexts: Vec<TerminalContextSelection> = pending_terminal_contexts
            .iter()
            .map(|pending| pending.selection.clone())
            .collect();
        let review_comments = std::mem::take(&mut self.pending_review_comments);
        // Send order (`ChatView` handleSend): terminal block first, review
        // comments outermost (element/preview blocks are M4 territory).
        let (prompt_text, interaction_mode, source_proposed_plan) = if let Some(plan) =
            actionable_plan
        {
            if raw_text.is_empty() {
                (
                    format!("PLEASE IMPLEMENT THIS PLAN:\n{}", plan.plan_markdown.trim()),
                    ProviderInteractionMode::Default,
                    Some(Some(
                        vitre_contracts::ClientOrchestrationCommandThreadTurnStartSourceProposedPlan {
                            plan_id: tnes(plan.id),
                            thread_id: thread_id.clone(),
                        },
                    )),
                )
            } else {
                (raw_text.clone(), ProviderInteractionMode::Plan, None)
            }
        } else if raw_text.is_empty() && !self.pending_attachments.is_empty() {
            (
                IMAGE_ONLY_BOOTSTRAP_PROMPT.to_string(),
                selected_interaction_mode,
                None,
            )
        } else {
            (raw_text.clone(), selected_interaction_mode, None)
        };
        let text = append_review_comments_to_prompt(
            &append_terminal_contexts_to_prompt(&prompt_text, &terminal_contexts),
            &review_comments,
        );
        let pending_attachments = std::mem::take(&mut self.pending_attachments);
        let message_id = MessageId(fresh_id("vitre-msg"));
        let created_at = tnes(now_iso());
        let optimistic_attachments: Vec<ChatAttachment> = pending_attachments
            .iter()
            .map(|attachment| {
                let id = tnes(fresh_id("vitre-attachment"));
                self.attachment_urls.insert(
                    id.0.clone(),
                    SharedString::from(attachment.data_url.clone()),
                );
                if attachment.is_image {
                    ChatAttachment::ChatImageAttachment(ChatImageAttachment {
                        id,
                        mime_type: tnes(attachment.mime_type.clone()),
                        name: tnes(attachment.name.clone()),
                        size_bytes: attachment.size_bytes,
                        r#type: Default::default(),
                    })
                } else {
                    ChatAttachment::ChatFileAttachment(ChatFileAttachment {
                        id,
                        mime_type: tnes(attachment.mime_type.clone()),
                        name: tnes(attachment.name.clone()),
                        size_bytes: attachment.size_bytes,
                        r#type: Default::default(),
                    })
                }
            })
            .collect();
        let attachments = pending_attachments
            .iter()
            .map(|attachment| {
                let data_url = tnes(attachment.data_url.clone());
                let mime_type = tnes(attachment.mime_type.clone());
                let name = tnes(attachment.name.clone());
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

        self.local_dispatch = Some(LocalDispatchSnapshot::capture(&view, created_at.clone()));
        self.optimistic_user_messages.push(OrchestrationMessage {
            attachments: (!optimistic_attachments.is_empty())
                .then_some(Some(optimistic_attachments)),
            created_at: created_at.clone(),
            id: message_id.clone(),
            role: OrchestrationMessageRole::User,
            streaming: false,
            text: tnes(text.clone()),
            turn_id: None,
            updated_at: created_at.clone(),
        });
        self.pending_send = Some(PendingSend {
            message_id: message_id.clone(),
            raw_text,
            attachments: pending_attachments,
            review_comments,
            terminal_contexts: pending_terminal_contexts,
        });
        self.composer
            .update(cx, |input, cx| input.clean(window, cx));
        self.mention = None;
        self.composer_menu = None;
        self.last_error = None;
        self.pending_timeline_anchor = Some(message_id.clone());
        self.rebuild_timeline(cx);

        let command = ClientOrchestrationCommand::ThreadTurnStart {
            bootstrap: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at,
            interaction_mode: interaction_mode.clone(),
            message: vitre_contracts::ClientOrchestrationCommandThreadTurnStartMessage {
                attachments,
                message_id: message_id.clone(),
                role: Default::default(),
                text: tnes(text),
            },
            model_selection: None,
            runtime_mode: view.runtime_mode.clone(),
            source_proposed_plan,
            thread_id,
            title_seed: None,
            r#type: Default::default(),
        };
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = client.dispatch(&command).await {
                let _ = this.update_in(cx, |app, window, cx| {
                    app.rollback_send(&message_id, window, cx);
                    app.last_error = Some(format!("send failed: {}", error.user_message()).into());
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    /// Remove server-acknowledged optimistic rows and release the local busy
    /// state. Called on every detail-projection update before timeline derive.
    fn reconcile_local_dispatch(&mut self) {
        let Some(view) = self
            .thread
            .as_ref()
            .and_then(|open| open.state.view.as_ref())
            .cloned()
        else {
            return;
        };
        let server_ids: HashSet<MessageId> = view
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect();
        self.optimistic_user_messages
            .retain(|message| !server_ids.contains(&message.id));

        let acknowledged = self.local_dispatch.as_ref().is_some_and(|snapshot| {
            snapshot.is_acknowledged(
                &view,
                !derive_pending_approvals(&view.activities).is_empty(),
                !derive_pending_user_inputs(&view.activities).is_empty(),
                view.session
                    .as_ref()
                    .and_then(|session| session.last_error.as_ref())
                    .is_some(),
            )
        });
        if acknowledged {
            self.local_dispatch = None;
            self.pending_send = None;
        }
    }

    fn rollback_send(
        &mut self,
        message_id: &MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.local_dispatch = None;
        self.optimistic_user_messages
            .retain(|message| &message.id != message_id);
        let Some(pending) = self
            .pending_send
            .take()
            .filter(|pending| &pending.message_id == message_id)
        else {
            self.rebuild_timeline(cx);
            return;
        };
        // Match Electron's retry guard: never overwrite anything the user
        // already typed or attached while the request was in flight.
        if self.composer.read(cx).value().is_empty()
            && self.pending_attachments.is_empty()
            && self.pending_review_comments.is_empty()
            && self.pending_terminal_contexts.is_empty()
        {
            self.composer.update(cx, |input, cx| {
                input.set_value(pending.raw_text, window, cx)
            });
            self.pending_attachments = pending.attachments;
            self.pending_review_comments = pending.review_comments;
            self.pending_terminal_contexts = pending.terminal_contexts;
        }
        self.rebuild_timeline(cx);
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
        if self.thread.is_none() {
            self.draft_model = Some(selection);
            self.schedule_draft_save(cx);
            cx.notify();
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(open) = &self.thread else {
            return;
        };
        let current = self
            .shell_thread(&open.id)
            .map(|thread| &thread.model_selection)
            .or_else(|| open.state.view.as_ref().map(|view| &view.model_selection));
        let started = open
            .state
            .view
            .as_ref()
            .is_some_and(|view| view.session.is_some() || !view.messages.is_empty());
        if started
            && current.is_some_and(|current| {
                let current_id = model_selection_identity(current);
                let next_id = model_selection_identity(&selection);
                if current_id == next_id {
                    return false;
                }
                let providers = &self.provider_snapshots;
                let current_requires = current_id.as_ref().is_some_and(|(instance, _)| {
                    providers.iter().any(|provider| {
                        provider.instance_id.0 == *instance
                            && provider_requires_new_thread(provider)
                    })
                });
                let next_requires = next_id.as_ref().is_some_and(|(instance, _)| {
                    providers.iter().any(|provider| {
                        provider.instance_id.0 == *instance
                            && provider_requires_new_thread(provider)
                    })
                });
                current_requires || next_requires
            })
        {
            self.last_error = Some(
                "Start a new chat to change models. This provider locks the model after a conversation has started."
                    .into(),
            );
            cx.notify();
            return;
        }
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

    fn set_model_option(
        &mut self,
        option_id: String,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = &self.thread else {
            return;
        };
        let mut selection = self
            .shell_thread(&open.id)
            .map(|thread| thread.model_selection.clone())
            .or_else(|| {
                open.state
                    .view
                    .as_ref()
                    .map(|view| view.model_selection.clone())
            })
            .unwrap_or(ModelSelection {
                instance_id: None,
                model: serde_json::Value::Null,
                options: None,
                provider: None,
            });
        let mut options = selection
            .options
            .clone()
            .flatten()
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        options.retain(|option| {
            option.get("id").and_then(serde_json::Value::as_str) != Some(&option_id)
        });
        options.push(serde_json::json!({ "id": option_id, "value": value }));
        selection.options = Some(Some(serde_json::Value::Array(options)));
        self.set_model(selection, cx);
    }

    fn set_prompt_effort(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            let current = input.value();
            let stripped = current
                .strip_prefix("Ultrathink:\n")
                .or_else(|| current.strip_prefix("Ultrathink: "))
                .unwrap_or(&current)
                .to_string();
            let next = if value.eq_ignore_ascii_case("ultrathink") {
                if stripped.trim().is_empty() {
                    "Ultrathink:\n".to_string()
                } else {
                    format!("Ultrathink:\n{stripped}")
                }
            } else {
                stripped
            };
            input.set_value(next, window, cx);
        });
    }

    fn open_model_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model_picker.take().is_some() {
            self.composer.focus_handle(cx).focus(window, cx);
            cx.notify();
            return;
        }
        let providers: Vec<_> = self
            .provider_snapshots
            .iter()
            .filter(|provider| provider_picker_visible(provider))
            .cloned()
            .collect();
        let current_selection = self
            .thread
            .as_ref()
            .and_then(|open| {
                self.shell_thread(&open.id)
                    .map(|thread| &thread.model_selection)
                    .or_else(|| open.state.view.as_ref().map(|view| &view.model_selection))
                    .and_then(model_selection_identity)
            })
            .or_else(|| self.draft_model.as_ref().and_then(model_selection_identity))
            .or_else(|| {
                self.draft_project
                    .as_ref()
                    .and_then(|id| {
                        self.shell
                            .snapshot
                            .as_ref()?
                            .projects
                            .iter()
                            .find(|p| &p.id == id)?
                            .default_model_selection
                            .as_ref()
                    })
                    .and_then(model_selection_identity)
            })
            .or_else(|| {
                default_model_selection(&self.provider_snapshots, cx)
                    .as_ref()
                    .and_then(model_selection_identity)
            });
        let thread_started = self.thread.as_ref().is_some_and(|open| {
            open.state
                .view
                .as_ref()
                .is_some_and(|view| view.session.is_some() || !view.messages.is_empty())
        });
        let owner = cx.entity().downgrade();
        let favorites = self.model_favorites.values.clone();
        let picker = cx.new(|cx| {
            ModelPickerDialog::new(
                providers,
                favorites,
                current_selection,
                thread_started,
                owner,
                window,
                cx,
            )
        });
        self.model_picker = Some(picker);
        cx.notify();
    }

    /// The shell owns mode updates; the detail stream only carries message,
    /// activity, session and turn events. Both the label and turn dispatch
    /// must use this same selection, including an unacknowledged click.
    fn current_interaction_mode(&self, thread_id: &ThreadId) -> ProviderInteractionMode {
        resolve_interaction_mode(
            self.pending_interaction_mode
                .as_ref()
                .filter(|(id, _)| id == thread_id)
                .map(|(_, mode)| mode),
            self.shell_thread(thread_id)
                .and_then(|thread| thread.interaction_mode.as_ref())
                .and_then(|mode| mode.as_ref()),
            self.thread
                .as_ref()
                .filter(|open| &open.id == thread_id)
                .and_then(|open| open.state.view.as_ref())
                .and_then(|view| view.interaction_mode.as_ref())
                .and_then(|mode| mode.as_ref()),
        )
    }

    fn set_interaction_mode(
        &mut self,
        interaction_mode: ProviderInteractionMode,
        cx: &mut Context<Self>,
    ) {
        if self.thread.is_none() {
            self.draft_mode = Some(interaction_mode);
            self.schedule_draft_save(cx);
            cx.notify();
            return;
        }
        let (Some(client), Some(open)) = (self.client.clone(), self.thread.as_ref()) else {
            return;
        };
        // Serialize mode writes until the shell acknowledges the last click.
        if self.pending_interaction_mode.is_some()
            || self.current_interaction_mode(&open.id) == interaction_mode
        {
            return;
        }
        let pending = (open.id.clone(), interaction_mode.clone());
        let command = ClientOrchestrationCommand::ThreadInteractionModeSet {
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            interaction_mode: interaction_mode.clone(),
            thread_id: open.id.clone(),
            r#type: Default::default(),
        };
        self.pending_interaction_mode = Some((open.id.clone(), interaction_mode.clone()));
        self.last_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            if let Err(error) = client.dispatch(&command).await {
                let _ = this.update(cx, |app, cx| {
                    // A failed request from a previously selected thread must
                    // not roll back the current thread's pending selection.
                    if app.pending_interaction_mode.as_ref() != Some(&pending) {
                        return;
                    }
                    app.pending_interaction_mode = None;
                    app.last_error = Some(
                        format!("interaction mode change failed: {}", error.user_message()).into(),
                    );
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn set_runtime_mode(&mut self, runtime_mode: RuntimeMode, cx: &mut Context<Self>) {
        let (Some(client), Some(open)) = (self.client.clone(), self.thread.as_ref()) else {
            return;
        };
        let command = ClientOrchestrationCommand::ThreadRuntimeModeSet {
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            runtime_mode,
            thread_id: open.id.clone(),
            r#type: Default::default(),
        };
        self.last_error = None;
        cx.spawn(async move |this, cx| {
            if let Err(error) = client.dispatch(&command).await {
                let _ = this.update(cx, |app, cx| {
                    app.last_error = Some(
                        format!("runtime mode change failed: {}", error.user_message()).into(),
                    );
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
        self.selected_workspace_root()
            .or_else(|| self.open_project_root())
            .or_else(|| {
                self.draft_project
                    .as_ref()
                    .and_then(|id| self.project_root(id))
            })
            .or_else(|| {
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
            selected: 0,
            loading: !query.is_empty(),
        });
        cx.notify();
        if query.is_empty() {
            return;
        }
        let (Some(client), Some(cwd)) = (self.client.clone(), self.search_root()) else {
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
            let result = client.call::<ProjectsSearchEntries>(&payload).await.ok();
            let _ = this.update(cx, |app, cx| {
                if let Some(mention) = &mut app.mention
                    && mention.generation == generation
                {
                    mention.results = result.map(|result| result.entries).unwrap_or_default();
                    mention.selected = mention
                        .selected
                        .min(mention.results.len().saturating_sub(1));
                    mention.loading = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn sync_composer_menu(&mut self, cx: &mut Context<Self>) {
        let (value, cursor) = {
            let state = self.composer.read(cx);
            (state.value(), state.cursor())
        };
        let Some((kind, token_start, query)) = active_composer_token(&value, cursor) else {
            if self.composer_menu.take().is_some() {
                cx.notify();
            }
            return;
        };
        if kind == ComposerTriggerKind::Path {
            if self.composer_menu.take().is_some() {
                cx.notify();
            }
            return;
        }

        let query_lower = query.trim().to_ascii_lowercase();
        let mut items = Vec::new();
        match kind {
            ComposerTriggerKind::Slash => {
                let builtins = [
                    ComposerMenuItem::Model,
                    ComposerMenuItem::InteractionMode(ProviderInteractionMode::Plan),
                    ComposerMenuItem::InteractionMode(ProviderInteractionMode::Default),
                ];
                items.extend(builtins.into_iter().filter(|item| {
                    let label = match item {
                        ComposerMenuItem::Model => "model",
                        ComposerMenuItem::InteractionMode(ProviderInteractionMode::Plan) => "plan",
                        ComposerMenuItem::InteractionMode(_) => "default",
                        _ => unreachable!(),
                    };
                    query_lower.is_empty() || label.contains(&query_lower)
                }));

                if let Some(provider) = self.selected_provider() {
                    for command in provider
                        .slash_commands
                        .clone()
                        .flatten()
                        .unwrap_or_default()
                    {
                        let description = command
                            .description
                            .clone()
                            .flatten()
                            .or_else(|| command.input.clone().flatten().map(|input| input.hint))
                            .unwrap_or_else(|| "Run provider command".into());
                        if query_lower.is_empty()
                            || command.name.to_ascii_lowercase().contains(&query_lower)
                            || description.to_ascii_lowercase().contains(&query_lower)
                        {
                            items.push(ComposerMenuItem::ProviderCommand {
                                name: command.name,
                                description,
                            });
                        }
                    }
                }
            }
            ComposerTriggerKind::Skill => {
                if let Some(provider) = self.selected_provider() {
                    for skill in provider.skills.clone().flatten().unwrap_or_default() {
                        if !skill.enabled {
                            continue;
                        }
                        let description = skill
                            .short_description
                            .clone()
                            .flatten()
                            .or_else(|| skill.description.clone().flatten())
                            .unwrap_or_else(|| "Run provider skill".into());
                        if query_lower.is_empty()
                            || skill.name.to_ascii_lowercase().contains(&query_lower)
                            || description.to_ascii_lowercase().contains(&query_lower)
                        {
                            items.push(ComposerMenuItem::Skill {
                                name: skill.name,
                                description,
                            });
                        }
                    }
                }
            }
            ComposerTriggerKind::Thread => {
                let current_project = self
                    .thread
                    .as_ref()
                    .and_then(|open| self.shell_thread(&open.id))
                    .map(|thread| thread.project_id.clone());
                let mut threads = self.shell_threads();
                threads.retain(|thread| {
                    query_lower.is_empty()
                        || thread.title.0.to_ascii_lowercase().contains(&query_lower)
                });
                threads.sort_by(|a, b| {
                    let a_prefix = a.title.0.to_ascii_lowercase().starts_with(&query_lower);
                    let b_prefix = b.title.0.to_ascii_lowercase().starts_with(&query_lower);
                    let a_same = current_project.as_ref() == Some(&a.project_id);
                    let b_same = current_project.as_ref() == Some(&b.project_id);
                    b_prefix
                        .cmp(&a_prefix)
                        .then_with(|| b_same.cmp(&a_same))
                        .then_with(|| b.updated_at.0.cmp(&a.updated_at.0))
                });
                for thread in threads.into_iter().take(20) {
                    let project = self
                        .shell
                        .snapshot
                        .as_ref()
                        .and_then(|snapshot| {
                            snapshot
                                .projects
                                .iter()
                                .find(|project| project.id == thread.project_id)
                        })
                        .map(|project| project.title.0.clone());
                    let time = relative_time(&thread.updated_at.0);
                    let detail = [project, time]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · ");
                    items.push(ComposerMenuItem::Thread {
                        id: thread.id,
                        title: thread.title.0,
                        detail,
                    });
                }
            }
            ComposerTriggerKind::Path => unreachable!(),
        }

        self.composer_menu = Some(ComposerMenuState {
            token_start,
            token_end: cursor,
            selected: self
                .composer_menu
                .as_ref()
                .map_or(0, |menu| menu.selected.min(items.len().saturating_sub(1))),
            items,
        });
        cx.notify();
    }

    fn selected_provider(&self) -> Option<vitre_contracts::ServerProvider> {
        let open = self.thread.as_ref()?;
        let selection = self
            .shell_thread(&open.id)
            .map(|thread| thread.model_selection.clone())
            .or_else(|| {
                open.state
                    .view
                    .as_ref()
                    .map(|view| view.model_selection.clone())
            })?;
        let instance = selection
            .instance_id
            .flatten()
            .and_then(|value| value.as_str().map(str::to_string));
        let providers = &self.provider_snapshots;
        instance
            .as_ref()
            .and_then(|instance| {
                providers
                    .iter()
                    .find(|provider| &provider.instance_id.0 == instance)
            })
            .or_else(|| {
                providers
                    .iter()
                    .find(|provider| provider_picker_ready(provider))
            })
            .cloned()
    }

    fn replace_composer_token(
        &mut self,
        start: usize,
        end: usize,
        replacement: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |state, cx| {
            let value = state.value();
            if start > end || end > value.len() {
                return;
            }
            let text = format!("{}{}{}", &value[..start], replacement, &value[end..]);
            let caret = start + replacement.len();
            state.set_value(text, window, cx);
            state.set_selected_range(caret..caret, cx);
        });
    }

    fn apply_active_composer_suggestion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(menu) = &self.composer_menu {
            let Some(item) = menu.items.get(menu.selected).cloned() else {
                return false;
            };
            let (start, end) = (menu.token_start, menu.token_end);
            self.composer_menu = None;
            match item {
                ComposerMenuItem::Model => {
                    self.replace_composer_token(start, end, "", window, cx);
                    self.open_model_picker(window, cx);
                }
                ComposerMenuItem::InteractionMode(mode) => {
                    self.replace_composer_token(start, end, "", window, cx);
                    self.set_interaction_mode(mode, cx);
                }
                ComposerMenuItem::ProviderCommand { name, .. } => {
                    self.replace_composer_token(start, end, &format!("/{name} "), window, cx);
                }
                ComposerMenuItem::Thread { id, title, .. } => {
                    let title = title.replace(']', "\\]");
                    self.replace_composer_token(
                        start,
                        end,
                        &format!("[#{title}](t3code://thread/{}) ", id.0),
                        window,
                        cx,
                    );
                }
                ComposerMenuItem::Skill { name, .. } => {
                    self.replace_composer_token(start, end, &format!("${name} "), window, cx);
                }
            }
            cx.notify();
            return true;
        }

        let selected = self.mention.as_ref().map_or(0, |mention| mention.selected);
        self.apply_mention(selected, window, cx)
    }

    fn move_composer_suggestion(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        fn moved(current: usize, len: usize, delta: isize) -> usize {
            if len == 0 {
                return 0;
            }
            (current as isize + delta).rem_euclid(len as isize) as usize
        }
        if let Some(menu) = &mut self.composer_menu
            && !menu.items.is_empty()
        {
            menu.selected = moved(menu.selected, menu.items.len(), delta);
            cx.notify();
            return true;
        }
        if let Some(mention) = &mut self.mention
            && !mention.results.is_empty()
        {
            mention.selected = moved(mention.selected, mention.results.len(), delta);
            cx.notify();
            return true;
        }
        false
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
        if self.thread.is_none() && self.draft_project.is_none() {
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
                app.stage_attachment_paths(paths.iter(), window, cx);
            });
        })
        .detach();
    }

    fn stage_attachment_paths<'a>(
        &mut self,
        paths: impl IntoIterator<Item = &'a PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut error: Option<String> = None;
        for path in paths {
            if !path.is_file() {
                continue;
            }
            if self.pending_attachments.len() >= MAX_ATTACHMENTS {
                error = Some(format!(
                    "You can attach up to {MAX_ATTACHMENTS} files per message."
                ));
                break;
            }
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".to_string());
            let bytes = match std::fs::read(path) {
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
            self.pending_attachments.push(PendingAttachment {
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
        self.schedule_draft_save(cx);
        cx.notify();
    }

    /// Consume non-text clipboard entries as attachments. GPUI exposes native
    /// bitmap paste separately from text, so screenshots can be sent without
    /// first saving them to disk. If text is the leading flavor, return false
    /// and let the textarea perform its normal paste.
    fn stage_clipboard_item(
        &mut self,
        clipboard: &ClipboardItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if matches!(
            clipboard.entries().first(),
            Some(ClipboardEntry::String(_)) | None
        ) {
            return false;
        }
        let paths: Vec<PathBuf> = clipboard
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::ExternalPaths(paths) => Some(paths.paths().iter().cloned()),
                _ => None,
            })
            .flatten()
            .collect();
        if !paths.is_empty() {
            self.stage_attachment_paths(paths.iter(), window, cx);
        }
        let mut error = None;
        for image in clipboard.entries().iter().filter_map(|entry| match entry {
            ClipboardEntry::Image(image) => Some(image),
            _ => None,
        }) {
            if self.pending_attachments.len() >= MAX_ATTACHMENTS {
                error = Some(format!(
                    "You can attach up to {MAX_ATTACHMENTS} files per message."
                ));
                break;
            }
            if image.bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
                error = Some("Pasted image exceeds the 10MB attachment limit.".to_string());
                continue;
            }
            let mime_type = image.format.mime_type();
            let name = format!(
                "pasted-image-{}.{}",
                self.pending_attachments.len() + 1,
                image.format.extension()
            );
            self.pending_attachments.push(PendingAttachment {
                name,
                mime_type: mime_type.to_string(),
                size_bytes: image.bytes.len() as i64,
                data_url: format!(
                    "data:{mime_type};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(&image.bytes)
                ),
                is_image: true,
            });
        }
        if let Some(error) = error {
            window.push_notification(Notification::error(SharedString::from(error)), cx);
        }
        self.schedule_draft_save(cx);
        cx.notify();
        true
    }

    fn export_plan(&self, markdown: String, window: &mut Window, cx: &mut Context<Self>) {
        let directory = self
            .open_project_root()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let filename = plan_filename(&markdown);
        let receiver = cx.prompt_for_new_path(&directory, Some(&filename));
        cx.spawn_in(window, async move |_, window| {
            let Some(path) = receiver.await.ok().into_iter().flatten().flatten().next() else {
                return;
            };
            match std::fs::write(&path, format!("{}\n", markdown.trim_end())) {
                Ok(()) => window
                    .update(|window, cx| {
                        window.push_notification(Notification::success("Plan exported"), cx)
                    })
                    .ok(),
                Err(error) => window
                    .update(|window, cx| {
                        window.push_notification(
                            Notification::error(SharedString::from(format!(
                                "Plan export failed: {error}"
                            ))),
                            cx,
                        )
                    })
                    .ok(),
            };
        })
        .detach();
    }

    fn sync_attachment_urls(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(session) = client.sessions().borrow().clone() else {
            return;
        };
        let base_url = session.target.base_url;
        let ids: Vec<String> = self
            .display_messages
            .iter()
            .flat_map(|message| {
                message
                    .attachments
                    .as_ref()
                    .and_then(|attachments| attachments.as_ref())
                    .into_iter()
                    .flatten()
            })
            .filter_map(|attachment| match attachment {
                ChatAttachment::ChatImageAttachment(image) => Some(image.id.0.clone()),
                ChatAttachment::ChatFileAttachment(file) => Some(file.id.0.clone()),
                ChatAttachment::Unknown(_) => None,
            })
            .filter(|id| {
                !self.attachment_urls.contains_key(id) && !self.attachment_urls_loading.contains(id)
            })
            .collect();
        for id in ids {
            self.attachment_urls_loading.insert(id.clone());
            let client = client.clone();
            let base_url = base_url.clone();
            cx.spawn(async move |this, cx| {
                let payload = AssetCreateUrlInput {
                    resource: AssetResource::Attachment {
                        attachment_id: tnes(id.clone()),
                    },
                };
                let result = client.call::<AssetsCreateUrl>(&payload).await;
                let _ = this.update(cx, |app, cx| {
                    app.attachment_urls_loading.remove(&id);
                    if let Ok(result) = result {
                        let relative = result.relative_url.0;
                        let url = if relative.starts_with('/') {
                            format!("{}{}", base_url.trim_end_matches('/'), relative)
                        } else {
                            format!("{}/{}", base_url.trim_end_matches('/'), relative)
                        };
                        app.attachment_urls.insert(id, url.into());
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    fn sync_stream_smoothing(&mut self, cx: &mut Context<Self>) {
        let live_ids: HashSet<MessageId> = self
            .display_messages
            .iter()
            .filter(|message| message.role == OrchestrationMessageRole::Assistant)
            .map(|message| message.id.clone())
            .collect();
        self.smoothed_assistant_text
            .retain(|id, _| live_ids.contains(id));

        let mut lagging = false;
        for message in &self.display_messages {
            if message.role != OrchestrationMessageRole::Assistant {
                continue;
            }
            if !message.streaming {
                self.smoothed_assistant_text
                    .insert(message.id.clone(), message.text.0.clone());
                continue;
            }
            let displayed = self
                .smoothed_assistant_text
                .entry(message.id.clone())
                .or_insert_with(|| message.text.0.clone());
            if !message.text.0.starts_with(displayed.as_str()) {
                *displayed = message.text.0.clone();
            }
            lagging |= displayed.len() < message.text.0.len();
        }
        if !lagging || self.smoothing_tick.is_some() {
            return;
        }
        self.smoothing_tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(20))
                    .await;
                let keep = this
                    .update(cx, |app, cx| {
                        let targets: Vec<(MessageId, String)> = app
                            .display_messages
                            .iter()
                            .filter(|message| {
                                message.role == OrchestrationMessageRole::Assistant
                                    && message.streaming
                            })
                            .map(|message| (message.id.clone(), message.text.0.clone()))
                            .collect();
                        let mut changed_ids = HashSet::new();
                        let mut keep = false;
                        for (id, target) in targets {
                            let displayed =
                                app.smoothed_assistant_text.entry(id.clone()).or_default();
                            if !target.starts_with(displayed.as_str()) {
                                *displayed = target.clone();
                                changed_ids.insert(id);
                                continue;
                            }
                            if displayed.len() < target.len() {
                                let suffix = &target[displayed.len()..];
                                displayed.extend(suffix.chars().take(18));
                                changed_ids.insert(id);
                            }
                            keep |= displayed.len() < target.len();
                        }
                        for (index, row) in app.timeline.iter().enumerate() {
                            if let TimelineRow::Message(message_index) = row
                                && app
                                    .display_messages
                                    .get(*message_index)
                                    .is_some_and(|message| changed_ids.contains(&message.id))
                            {
                                app.timeline_list.remeasure_items(index..index + 1);
                            }
                        }
                        cx.notify();
                        keep
                    })
                    .unwrap_or(false);
                if !keep {
                    let _ = this.update(cx, |app, _| app.smoothing_tick = None);
                    break;
                }
            }
        }));
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
            .bg(crate::glass::elevated(cx))
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
            .cloned()
        else {
            self.display_messages.clear();
            self.smoothed_assistant_text.clear();
            self.smoothing_tick = None;
            self.timeline = Vec::new();
            self.timeline_hashes = Vec::new();
            self.pending_timeline_anchor = None;
            self.revert_turn_counts = HashMap::new();
            self.work_entries = Vec::new();
            self.work_toggles = Vec::new();
            self.turn_folds = Vec::new();
            self.working_tick = None;
            self.timeline_list.reset(0);
            cx.notify();
            return;
        };

        self.display_messages =
            merge_optimistic_messages(&view.messages, &self.optimistic_user_messages);
        self.sync_attachment_urls(cx);
        self.sync_stream_smoothing(cx);
        let view = view.as_ref();
        let running = view
            .session
            .as_ref()
            .is_some_and(|session| session.status == OrchestrationSessionStatus::Running);
        let is_working = running || self.local_dispatch.is_some();

        // Auto-fold lifecycle (Electron MessagesTimeline's expandedTurnIds
        // effect): an interrupt leaves its turn expanded; a NEW latest turn
        // folds the previous one.
        if let Some(latest) = view.latest_turn.as_ref() {
            let latest_id = latest.turn_id.0.clone();
            match &self.prev_latest_turn {
                Some((previous_id, _)) if *previous_id != latest_id => {
                    let previous_id = previous_id.clone();
                    self.expanded_turn_ids.remove(&previous_id);
                }
                Some((_, previous_state))
                    if *previous_state == OrchestrationLatestTurnState::Running
                        && latest.state == OrchestrationLatestTurnState::Interrupted =>
                {
                    self.expanded_turn_ids.insert(latest_id.clone());
                }
                _ => {}
            }
            self.prev_latest_turn = Some((latest_id, latest.state.clone()));
        }

        // The Electron derive pipeline: activities → filtered/merged work-log
        // entries → rows with turn folds and "+N previous" overflow groups.
        self.work_entries = work_log::derive_work_log_entries(&view.activities);
        let running_turn_id: Option<String> = view
            .session
            .as_ref()
            .filter(|session| session.status == OrchestrationSessionStatus::Running)
            .and_then(|session| session.active_turn_id.as_ref())
            .map(|turn| turn.0.clone());
        let derived = work_log::derive_timeline_rows(&TimelineDeriveInput {
            messages: &self.display_messages,
            work_entries: &self.work_entries,
            latest_turn: view.latest_turn.as_ref(),
            running_turn_id: running_turn_id.as_deref(),
            expanded_turn_ids: &self.expanded_turn_ids,
            expanded_work_group_ids: &self.expanded_work_groups,
            is_working,
        });
        let mut rows = Vec::with_capacity(derived.len());
        let mut work_toggles = Vec::new();
        let mut turn_folds = Vec::new();
        for row in derived {
            match row {
                DerivedTimelineRow::Message { message_index } => {
                    rows.push(TimelineRow::Message(message_index));
                }
                DerivedTimelineRow::Work { entry_index } => {
                    rows.push(TimelineRow::Work(entry_index));
                }
                DerivedTimelineRow::WorkToggle {
                    group_id,
                    hidden_count,
                    expanded,
                    only_tool_entries,
                } => {
                    rows.push(TimelineRow::WorkToggle(work_toggles.len()));
                    work_toggles.push(WorkToggleData {
                        group_id,
                        hidden_count,
                        expanded,
                        only_tool_entries,
                    });
                }
                DerivedTimelineRow::TurnFold {
                    turn_id,
                    label,
                    expanded,
                } => {
                    rows.push(TimelineRow::TurnFold(turn_folds.len()));
                    turn_folds.push(TurnFoldData {
                        turn_id,
                        label: label.into(),
                        expanded,
                    });
                }
                DerivedTimelineRow::Working => rows.push(TimelineRow::Running),
            }
        }
        if let Some(plans) = view
            .proposed_plans
            .as_ref()
            .and_then(|plans| plans.as_ref())
        {
            let insert_at = rows
                .iter()
                .position(|row| *row == TimelineRow::Running)
                .unwrap_or(rows.len());
            rows.splice(
                insert_at..insert_at,
                (0..plans.len()).map(TimelineRow::ProposedPlan),
            );
        }
        self.work_toggles = work_toggles;
        self.turn_folds = turn_folds;

        // The "Working for Ns" timer repaints once a second while a turn
        // runs; the row's height never changes, so a bare notify suffices
        // (list heights are cached by content hash, and Running hashes
        // constant).
        if is_working {
            if self.working_tick.is_none() {
                self.working_tick = Some(cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor()
                            .timer(std::time::Duration::from_secs(1))
                            .await;
                        let keep = this
                            .update(cx, |this, cx| {
                                let running = this.local_dispatch.is_some()
                                    || this
                                        .thread
                                        .as_ref()
                                        .and_then(|open| open.state.view.as_ref())
                                        .and_then(|view| view.session.as_ref())
                                        .is_some_and(|session| {
                                            session.status == OrchestrationSessionStatus::Running
                                        });
                                if running {
                                    cx.notify();
                                }
                                running
                            })
                            .unwrap_or(false);
                        if !keep {
                            break;
                        }
                    }
                }));
            }
        } else {
            self.working_tick = None;
        }

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
        let mut messages: Vec<&OrchestrationMessage> = self.display_messages.iter().collect();
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
        let workspace_root = self.search_root();
        let hashes = rows
            .iter()
            .map(|row| {
                row_content_hash(
                    view,
                    &self.display_messages,
                    *row,
                    &self.revert_turn_counts,
                    &self.expanded_work_entries,
                    thread_key.as_deref(),
                    &self.changed_files,
                    &self.work_entries,
                    &self.work_toggles,
                    &self.turn_folds,
                    &self.expanded_plans,
                    workspace_root.as_deref(),
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
        if let Some(message_id) = self.pending_timeline_anchor.take()
            && let Some(item_ix) = self.timeline.iter().position(|row| {
                matches!(
                    row,
                    TimelineRow::Message(message_index)
                        if self
                            .display_messages
                            .get(*message_index)
                            .is_some_and(|message| message.id == message_id)
                )
            })
        {
            self.timeline_list.scroll_to(ListOffset {
                item_ix,
                offset_in_item: px(0.),
            });
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
        let workspace_root = self.search_root();
        let hash = self
            .thread
            .as_ref()
            .and_then(|open| open.state.view.as_ref())
            .map(|view| {
                row_content_hash(
                    view,
                    &self.display_messages,
                    row,
                    &self.revert_turn_counts,
                    &self.expanded_work_entries,
                    thread_key.as_deref(),
                    &self.changed_files,
                    &self.work_entries,
                    &self.work_toggles,
                    &self.turn_folds,
                    &self.expanded_plans,
                    workspace_root.as_deref(),
                )
            });
        if let Some(hash) = hash {
            self.timeline_hashes[index] = hash;
        }
        self.timeline_list.remeasure_items(index..index + 1);
        cx.notify();
    }

    fn render_message_attachments(
        &self,
        message: &OrchestrationMessage,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let attachments = message
            .attachments
            .as_ref()
            .and_then(|attachments| attachments.as_ref())?;
        if attachments.is_empty() {
            return None;
        }
        let mut images = h_flex().flex_wrap().gap_2();
        let mut files = h_flex().flex_wrap().gap_1p5();
        let lightbox_images: Vec<(SharedString, SharedString)> = attachments
            .iter()
            .filter_map(|attachment| match attachment {
                ChatAttachment::ChatImageAttachment(image) => self
                    .attachment_urls
                    .get(&image.id.0)
                    .cloned()
                    .map(|url| (url, image.name.0.clone().into())),
                _ => None,
            })
            .collect();
        let mut image_count = 0usize;
        let mut file_count = 0usize;
        for (index, attachment) in attachments.iter().enumerate() {
            match attachment {
                ChatAttachment::ChatImageAttachment(image_attachment) => {
                    image_count += 1;
                    let url = self.attachment_urls.get(&image_attachment.id.0).cloned();
                    let name: SharedString = image_attachment.name.0.clone().into();
                    let tile: AnyElement = if let Some(url) = url {
                        let lightbox_index = lightbox_images
                            .iter()
                            .position(|(candidate, _)| candidate == &url)
                            .unwrap_or(0);
                        let all_images = lightbox_images.clone();
                        div()
                            .id(("message-image", index))
                            .w(px(190.))
                            .h(px(130.))
                            .rounded(px(9.))
                            .overflow_hidden()
                            .border_1()
                            .border_color(cx.theme().border.opacity(0.8))
                            .cursor_pointer()
                            .child(img(url).size_full().object_fit(ObjectFit::Cover))
                            .on_click(move |_, window, cx| {
                                let lightbox = cx.new(|cx| {
                                    ImageLightbox::new(all_images.clone(), lightbox_index, cx)
                                });
                                window.open_dialog(cx, move |dialog, _, _| {
                                    let lightbox = lightbox.clone();
                                    dialog.title("Image preview").w(px(980.)).content(
                                        move |content, _, _| content.child(lightbox.clone()),
                                    )
                                });
                            })
                            .into_any_element()
                    } else {
                        div()
                            .w(px(190.))
                            .h(px(130.))
                            .rounded(px(9.))
                            .border_1()
                            .border_color(cx.theme().border.opacity(0.8))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(name)
                            .into_any_element()
                    };
                    images = images.child(tile);
                }
                ChatAttachment::ChatFileAttachment(file) => {
                    file_count += 1;
                    let url = self.attachment_urls.get(&file.id.0).cloned();
                    let name: SharedString = file.name.0.clone().into();
                    let size = format_attachment_size(file.size_bytes.max(0) as u64);
                    files = files.child(
                        h_flex()
                            .id(("message-file", index))
                            .gap_1p5()
                            .px_2()
                            .py_1()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(cx.theme().border.opacity(0.8))
                            .bg(cx.theme().background.opacity(0.5))
                            .when(url.is_some(), |chip| chip.cursor_pointer())
                            .child(crate::icons::file_icon(&file.name.0, cx))
                            .child(div().max_w(px(220.)).truncate().child(name))
                            .child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(SharedString::from(size)),
                            )
                            .when_some(url, |chip, url| {
                                chip.on_click(move |_, _, cx| cx.open_url(&url))
                            }),
                    );
                }
                ChatAttachment::Unknown(_) => {}
            }
        }
        Some(
            v_flex()
                .gap_2()
                .when(image_count > 0, |column| column.child(images))
                .when(file_count > 0, |column| column.child(files))
                .into_any_element(),
        )
    }

    fn enabled_skill_names(&self) -> HashSet<String> {
        self.provider_snapshots
            .iter()
            .flat_map(|provider| {
                provider
                    .skills
                    .as_ref()
                    .and_then(|skills| skills.as_ref())
                    .into_iter()
                    .flatten()
            })
            .filter(|skill| skill.enabled)
            .map(|skill| skill.name.clone())
            .collect()
    }

    fn render_user_inline_text(&self, text: &str, cx: &mut Context<Self>) -> AnyElement {
        let segments = parse_user_inline_segments(text, &self.enabled_skill_names());
        h_flex()
            .w_full()
            .flex_wrap()
            .gap_1()
            .whitespace_normal()
            .children(segments.into_iter().enumerate().map(|(index, segment)| {
                match segment {
                    UserInlineSegment::Text(text) => {
                        div().child(SharedString::from(text)).into_any_element()
                    }
                    UserInlineSegment::Skill(name) => h_flex()
                        .id(("inline-skill", index))
                        .gap_1()
                        .px_1p5()
                        .py_0p5()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(cx.theme().magenta.opacity(0.3))
                        .bg(cx.theme().magenta.opacity(0.1))
                        .text_color(cx.theme().magenta)
                        .child(Icon::new(VitreIcon::Zap).size_3())
                        .child(format!("${name}"))
                        .into_any_element(),
                    UserInlineSegment::Thread { label, id } => h_flex()
                        .id(("inline-thread", index))
                        .gap_1()
                        .px_1p5()
                        .py_0p5()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(cx.theme().info.opacity(0.3))
                        .bg(cx.theme().info.opacity(0.1))
                        .text_color(cx.theme().info)
                        .cursor_pointer()
                        .child(Icon::new(VitreIcon::MessageCircle).size_3())
                        .child(format!("#{label}"))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.select_thread(id.clone(), cx)),
                        )
                        .into_any_element(),
                }
            }))
            .into_any_element()
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
                    let Some(message) = self.display_messages.get(message_index) else {
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
                        let attachments = self.render_message_attachments(message, cx);
                        let copy_text = message.text.0.clone();
                        let can_collapse = should_collapse_user_message(&message.text.0);
                        let expanded = self.expanded_user_messages.contains(&message.id);
                        let message_id = message.id.clone();
                        let timestamp: SharedString =
                            message_timestamp(&message.created_at.0, cx).into();
                        let timestamp_tooltip: SharedString = message.created_at.0.clone().into();
                        let bubble = v_flex()
                            .max_w(relative(0.8))
                            .gap_2()
                            .rounded(px(16.))
                            .bg(cx.theme().accent)
                            .p_3()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .children(attachments)
                            .child(
                                v_flex()
                                    .w_full()
                                    // Messages carrying trailing context blocks
                                    // (`<terminal_context>` / `<element_context>`)
                                    // or `<review_comment>` blocks render chips and
                                    // cards instead of the raw markup (Electron's
                                    // `UserTimelineRow` + `UserMessageBody`).
                                    .child(
                                        div()
                                            .w_full()
                                            .when(can_collapse && !expanded, |body| {
                                                body.max_h(px(176.)).overflow_hidden()
                                            })
                                            .map(|bubble| {
                                                match self.user_message_body(&message.text.0, cx) {
                                                    Some(body) => bubble.child(body),
                                                    None => {
                                                        bubble.child(self.render_user_inline_text(
                                                            &message.text.0,
                                                            cx,
                                                        ))
                                                    }
                                                }
                                            }),
                                    )
                                    .when(can_collapse, |body| {
                                        body.child(
                                            Button::new(("expand-user-message", message_index))
                                                .ghost()
                                                .xsmall()
                                                .label(if expanded {
                                                    "Show less"
                                                } else {
                                                    "Show full message"
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    if !this
                                                        .expanded_user_messages
                                                        .insert(message_id.clone())
                                                    {
                                                        this.expanded_user_messages
                                                            .remove(&message_id);
                                                    }
                                                    this.remeasure_row(index, cx);
                                                })),
                                        )
                                    }),
                            );
                        v_flex()
                            .w_full()
                            .items_end()
                            .gap_1()
                            .child(line.child(bubble))
                            .child(
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .text_size(px(10.))
                                    .text_color(cx.theme().muted_foreground.opacity(0.65))
                                    .child(
                                        Button::new(("copy-user-message", message_index))
                                            .ghost()
                                            .xsmall()
                                            .icon(Icon::new(IconName::Copy).size_3())
                                            .tooltip("Copy message")
                                            .on_click(move |_, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    copy_text.clone(),
                                                ));
                                            }),
                                    )
                                    .child(
                                        Button::new(("user-message-time", message_index))
                                            .when(
                                                !ClientSettings::get(cx).show_message_timestamps,
                                                |button| button.invisible(),
                                            )
                                            .label(timestamp)
                                            .ghost()
                                            .xsmall()
                                            .tooltip(timestamp_tooltip),
                                    ),
                            )
                            .into_any_element()
                    } else {
                        let streaming = message.streaming;
                        let text = if streaming {
                            self.smoothed_assistant_text
                                .get(&message.id)
                                .cloned()
                                .unwrap_or_else(|| message.text.0.clone())
                        } else {
                            message.text.0.clone()
                        };
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
                                        this.child(assistant_markdown(
                                            SharedString::from(format!("md-{}", message.id.0)),
                                            SharedString::from(text),
                                        ))
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
                        if !streaming {
                            let copy_text = message.text.0.clone();
                            let timestamp: SharedString =
                                message_timestamp(&message.created_at.0, cx).into();
                            let tooltip: SharedString = message.created_at.0.clone().into();
                            column = column.child(
                                h_flex()
                                    .mt_1()
                                    .gap_1()
                                    .items_center()
                                    .text_size(px(10.))
                                    .text_color(cx.theme().muted_foreground.opacity(0.65))
                                    .child(
                                        Button::new(("copy-assistant-message", message_index))
                                            .ghost()
                                            .xsmall()
                                            .icon(Icon::new(IconName::Copy).size_3())
                                            .tooltip("Copy response")
                                            .on_click(move |_, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    copy_text.clone(),
                                                ));
                                            }),
                                    )
                                    .child(
                                        Button::new(("assistant-message-time", message_index))
                                            .when(
                                                !ClientSettings::get(cx).show_message_timestamps,
                                                |button| button.invisible(),
                                            )
                                            .label(timestamp)
                                            .ghost()
                                            .xsmall()
                                            .tooltip(tooltip),
                                    ),
                            );
                        }
                        column.into_any_element()
                    }
                }
                TimelineRow::ProposedPlan(plan_index) => {
                    let Some(plan) = view
                        .proposed_plans
                        .as_ref()
                        .and_then(|plans| plans.as_ref())
                        .and_then(|plans| plans.get(plan_index))
                    else {
                        return div().into_any_element();
                    };
                    let expanded = self.expanded_plans.contains(&plan.id);
                    let collapsible = plan.plan_markdown.chars().count() > 900
                        || plan
                            .plan_markdown
                            .lines()
                            .filter(|line| !line.trim().is_empty())
                            .count()
                            > 8;
                    let displayed = if collapsible && !expanded {
                        collapsed_plan_markdown(&plan.plan_markdown)
                    } else {
                        plan.plan_markdown.clone()
                    };
                    let copy = plan.plan_markdown.clone();
                    let export = plan.plan_markdown.clone();
                    let save = plan.plan_markdown.clone();
                    let plan_id = plan.id.clone();
                    v_flex()
                        .w_full()
                        .rounded(px(14.))
                        .border_1()
                        .border_color(cx.theme().info.opacity(0.35))
                        .bg(cx.theme().info.opacity(0.06))
                        .overflow_hidden()
                        .child(
                            h_flex()
                                .px_3()
                                .py_2()
                                .gap_2()
                                .border_b_1()
                                .border_color(cx.theme().info.opacity(0.25))
                                .child(
                                    div()
                                        .px_1p5()
                                        .rounded(px(5.))
                                        .bg(cx.theme().info.opacity(0.16))
                                        .text_size(px(10.))
                                        .font_semibold()
                                        .text_color(cx.theme().info)
                                        .child("PLAN"),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_sm()
                                        .font_medium()
                                        .child(SharedString::from(
                                            proposed_plan_title(&plan.plan_markdown)
                                                .unwrap_or_else(|| "Proposed plan".into()),
                                        )),
                                )
                                .child(
                                    Button::new(("copy-plan", plan_index))
                                        .ghost()
                                        .xsmall()
                                        .icon(Icon::new(IconName::Copy))
                                        .tooltip("Copy plan")
                                        .on_click(move |_, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                copy.clone(),
                                            ));
                                        }),
                                )
                                .child(
                                    Button::new(("save-plan-workspace", plan_index))
                                        .ghost()
                                        .xsmall()
                                        .icon(IconName::Folder)
                                        .tooltip("Save plan to workspace")
                                        .on_click(cx.listener(move |app, _, window, cx| {
                                            app.save_plan_in_workspace(save.clone(), window, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("export-plan", plan_index))
                                        .ghost()
                                        .xsmall()
                                        .icon(Icon::new(IconName::File))
                                        .tooltip("Export as Markdown")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.export_plan(export.clone(), window, cx)
                                        })),
                                ),
                        )
                        .child(div().p_3().child(assistant_markdown(
                            SharedString::from(format!("plan-{}", plan.id)),
                            SharedString::from(displayed),
                        )))
                        .when(collapsible, |card| {
                            card.child(
                                Button::new(("expand-plan", plan_index))
                                    .ghost()
                                    .small()
                                    .label(if expanded {
                                        "Show less"
                                    } else {
                                        "Show full plan"
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if !this.expanded_plans.remove(&plan_id) {
                                            this.expanded_plans.insert(plan_id.clone());
                                        }
                                        this.remeasure_row(index, cx);
                                    })),
                            )
                        })
                        .into_any_element()
                }
                // Electron `SimpleWorkEntryRow`: icon, heading+preview split,
                // trailing chevron + status glyph, click-to-expand mono body.
                TimelineRow::Work(entry_index) => {
                    let Some(entry) = self.work_entries.get(entry_index) else {
                        return div().into_any_element();
                    };
                    let heading = work_log::work_entry_heading(entry);
                    let preview = work_log::work_entry_preview(entry)
                        .filter(|preview| !preview.eq_ignore_ascii_case(&heading));
                    let failed = work_log::work_entry_indicates_tool_failure(entry);
                    let success = work_log::work_entry_indicates_tool_success(entry);
                    let neutral = work_log::work_entry_indicates_tool_neutral_status(entry);
                    // Electron: activeTurnInProgress = isWorking || the latest
                    // turn is not settled yet.
                    let turn_settled = !(running
                        || view.latest_turn.as_ref().is_some_and(|latest| {
                            latest.completed_at.is_none()
                                || latest.state == OrchestrationLatestTurnState::Running
                        }));
                    let warning = entry.kind == "runtime.warning";
                    let destructive = entry.kind == "runtime.error";
                    let heading_color = if warning {
                        cx.theme().warning
                    } else if destructive {
                        cx.theme().danger
                    } else {
                        cx.theme().foreground.opacity(0.82)
                    };
                    let icon_color = if warning || destructive {
                        cx.theme().danger
                    } else if entry.tone == OrchestrationThreadActivityTone::Tool || failed {
                        cx.theme().muted_foreground.opacity(0.65)
                    } else if entry.tone == OrchestrationThreadActivityTone::Info
                        || entry.tone == OrchestrationThreadActivityTone::Approval
                    {
                        cx.theme().muted_foreground
                    } else {
                        cx.theme().foreground.opacity(0.92)
                    };
                    let workspace_root = self.search_root();
                    let body =
                        work_log::build_tool_call_expanded_body(entry, workspace_root.as_deref());
                    let expanded = self.expanded_work_entries.contains(&entry.id);
                    let entry_id = entry.id.clone();
                    let mut line = h_flex()
                        .id(SharedString::from(format!("work-{}", entry.id)))
                        .px_0p5()
                        .py_0p5()
                        .gap_1p5()
                        .items_center()
                        .rounded(cx.theme().radius)
                        .when(body.is_some(), |this| {
                            this.cursor_pointer()
                                .hover(|style| style.bg(cx.theme().accent.opacity(0.2)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.expanded_work_entries.remove(&entry_id) {
                                        this.expanded_work_entries.insert(entry_id.clone());
                                    }
                                    // Local UI state, so `rebuild_timeline`
                                    // never runs — remeasure by hand or the
                                    // body renders into the collapsed row's
                                    // cached height.
                                    this.remeasure_row(index, cx);
                                }))
                        })
                        .child(
                            h_flex().size_5().flex_shrink_0().justify_center().child(
                                work_entry_icon(entry)
                                    .size_3p5()
                                    .text_color(icon_color)
                                    .opacity(0.8),
                            ),
                        )
                        .child(
                            h_flex()
                                .min_w_0()
                                .flex_1()
                                .gap_1p5()
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(12.))
                                        .font_medium()
                                        .text_color(heading_color)
                                        .child(SharedString::from(heading)),
                                )
                                .children(preview.map(|preview| {
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .truncate()
                                        .text_size(px(12.))
                                        .text_color(cx.theme().muted_foreground.opacity(0.55))
                                        .child(SharedString::from(preview))
                                })),
                        );
                    if body.is_some() {
                        line = line.child(
                            h_flex().size_4().flex_shrink_0().justify_center().child(
                                Icon::new(if expanded {
                                    IconName::ChevronUp
                                } else {
                                    IconName::ChevronDown
                                })
                                .size_3()
                                .text_color(cx.theme().foreground.opacity(0.7)),
                            ),
                        );
                    }
                    // Status glyph (Electron's trailing cluster): X = failed,
                    // check = succeeded (or settled neutral), minus = still
                    // empty while the turn is active.
                    let status: Option<(Icon, &str)> = if failed {
                        Some((
                            Icon::new(IconName::Close).text_color(cx.theme().danger),
                            "Failed",
                        ))
                    } else if success || (turn_settled && neutral) {
                        Some((Icon::new(IconName::Check), "Completed"))
                    } else if neutral {
                        Some((
                            Icon::new(IconName::Minus)
                                .text_color(cx.theme().foreground.opacity(0.7)),
                            "Empty",
                        ))
                    } else {
                        None
                    };
                    if let Some((icon, tooltip)) = status {
                        line = line.child(
                            div()
                                .id(SharedString::from(format!("work-status-{}", entry.id)))
                                .flex_shrink_0()
                                .tooltip(move |window, cx| {
                                    gpui_component::tooltip::Tooltip::new(tooltip).build(window, cx)
                                })
                                .child(icon.size_3()),
                        );
                    }
                    let mut block = v_flex().child(line);
                    if expanded && let Some(body) = body {
                        // Electron: `ms-7 border-s ps-3` guide line, 11px mono
                        // payload capped at max-h-64.
                        block = block.child(
                            div()
                                .ml_7()
                                .mt_1()
                                .pl_3()
                                .pt_0p5()
                                .border_l_1()
                                .border_color(cx.theme().border.opacity(0.45))
                                .child(
                                    div()
                                        .id(SharedString::from(format!("work-body-{}", entry.id)))
                                        .max_h(px(256.))
                                        .overflow_y_scroll()
                                        .font_family(cx.theme().mono_font_family.clone())
                                        .text_size(px(11.))
                                        .text_color(cx.theme().muted_foreground)
                                        .whitespace_normal()
                                        .child(SharedString::from(body)),
                                ),
                        );
                    }
                    block.into_any_element()
                }
                // Electron `WorkGroupToggleTimelineRow`: "+N previous tool
                // calls" / "Show fewer tool calls".
                TimelineRow::WorkToggle(toggle_index) => {
                    let Some(toggle) = self.work_toggles.get(toggle_index) else {
                        return div().into_any_element();
                    };
                    let label = if toggle.expanded {
                        format!(
                            "Show fewer {}",
                            if toggle.only_tool_entries {
                                "tool calls"
                            } else {
                                "log entries"
                            }
                        )
                    } else {
                        let noun = match (toggle.only_tool_entries, toggle.hidden_count == 1) {
                            (true, true) => "tool call",
                            (true, false) => "tool calls",
                            (false, true) => "log entry",
                            (false, false) => "log entries",
                        };
                        format!("+{} previous {}", toggle.hidden_count, noun)
                    };
                    let group_id = toggle.group_id.clone();
                    let expanded = toggle.expanded;
                    h_flex()
                        .id(SharedString::from(format!(
                            "work-toggle-{}",
                            toggle.group_id
                        )))
                        .w_full()
                        .gap_1p5()
                        .px_0p5()
                        .py_0p5()
                        .rounded(cx.theme().radius)
                        .cursor_pointer()
                        .hover(|style| style.bg(cx.theme().accent.opacity(0.2)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.expanded_work_groups.remove(&group_id) {
                                this.expanded_work_groups.insert(group_id.clone());
                            }
                            this.rebuild_timeline(cx);
                        }))
                        .child(
                            h_flex().size_5().flex_shrink_0().justify_center().child(
                                Icon::new(if expanded {
                                    IconName::ChevronUp
                                } else {
                                    IconName::ChevronDown
                                })
                                .size_3p5()
                                .text_color(cx.theme().foreground.opacity(0.7)),
                            ),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_medium()
                                .text_color(cx.theme().foreground.opacity(0.82))
                                .child(SharedString::from(label)),
                        )
                        .into_any_element()
                }
                // Electron `TurnFoldTimelineRow`: "Worked for …" with a
                // chevron, bottom-ruled.
                TimelineRow::TurnFold(fold_index) => {
                    let Some(fold) = self.turn_folds.get(fold_index) else {
                        return div().into_any_element();
                    };
                    let turn_id = fold.turn_id.clone();
                    let expanded = fold.expanded;
                    div()
                        .w_full()
                        .border_b_1()
                        .border_color(cx.theme().border.opacity(0.6))
                        .pb_2()
                        .pt_1()
                        .child(
                            h_flex()
                                .id(SharedString::from(format!("turn-fold-{}", fold.turn_id)))
                                .gap_1()
                                .px_1()
                                .rounded(cx.theme().radius)
                                .text_size(px(12.))
                                .text_color(cx.theme().muted_foreground)
                                .cursor_pointer()
                                .hover(|style| style.text_color(cx.theme().foreground))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.expanded_turn_ids.remove(&turn_id) {
                                        this.expanded_turn_ids.insert(turn_id.clone());
                                    }
                                    this.rebuild_timeline(cx);
                                }))
                                .child(div().child(fold.label.clone()))
                                .child(
                                    Icon::new(if expanded {
                                        IconName::ChevronDown
                                    } else {
                                        IconName::ChevronRight
                                    })
                                    .size_3p5(),
                                ),
                        )
                        .into_any_element()
                }
                TimelineRow::Running => {
                    // Electron `WorkingTimelineRow`: pulsing dots + a ticking
                    // "Working for Ns" (`working_tick` repaints each second).
                    let start = self
                        .local_dispatch
                        .as_ref()
                        .map(|dispatch| &dispatch.started_at)
                        .or_else(|| {
                            view.latest_turn.as_ref().map(|latest| {
                                latest.started_at.as_ref().unwrap_or(&latest.requested_at)
                            })
                        });
                    let label = start
                        .and_then(|start| chrono::DateTime::parse_from_rfc3339(&start.0).ok())
                        .map(|start| {
                            let elapsed = chrono::Utc::now()
                                .signed_duration_since(start)
                                .num_milliseconds()
                                .max(0);
                            format!("Working for {}", work_log::format_duration(elapsed))
                        })
                        .unwrap_or_else(|| "Working…".into());
                    (h_flex()
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
                                .child(SharedString::from(label)),
                        ))
                    .into_any_element()
                }
            }
        })();
        // Per-row bottom padding (Electron `TimelineRowContent`: pb-2 for
        // work/work-toggle rows, pb-4 otherwise); `max_w` keeps the web
        // timeline's centred measure. `list()` lays each row out as its own
        // layout root, where auto margins resolve to zero — centre with a
        // flex parent instead of `mx_auto`.
        let bottom_padding = match row {
            TimelineRow::Work(_) | TimelineRow::WorkToggle(_) => px(8.),
            TimelineRow::ProposedPlan(_) => px(12.),
            _ => px(16.),
        };
        div()
            .w_full()
            .flex()
            .justify_center()
            .child(
                div()
                    .w_full()
                    .max_w(px(768.))
                    .px_5()
                    .pb(bottom_padding)
                    .child(body),
            )
            .into_any_element()
    }

    /// Chat column mirroring the Electron `ChatView`: header, error banner,
    /// centered max-w-3xl timeline (user bubbles right, assistant plain,
    /// low-weight activity rows), rounded-22 composer with circular send.
    fn render_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(open) = &self.thread else {
            return self.render_draft_landing(window, cx);
        };
        let title = self.thread_title(&open.id);
        let project = self
            .shell_thread(&open.id)
            .and_then(|thread| {
                self.shell
                    .snapshot
                    .as_ref()?
                    .projects
                    .iter()
                    .find(|project| project.id == thread.project_id)
            })
            .map(|project| (project.title.0.clone(), project.workspace_root.0.clone()));
        let view = open.state.view.as_ref();
        let running = view.is_some_and(|view| {
            view.session
                .as_ref()
                .is_some_and(|session| session.status == OrchestrationSessionStatus::Running)
        });
        let dispatching = self.local_dispatch.is_some();
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
        let providers = &self.provider_snapshots;
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
        let interaction_mode = self.current_interaction_mode(&open.id);
        let runtime_mode = self
            .shell_thread(&open.id)
            .map(|thread| thread.runtime_mode.clone())
            .or_else(|| view.map(|view| view.runtime_mode.clone()))
            .unwrap_or(RuntimeMode::FullAccess);
        let selected_provider = providers.iter().find(|provider| {
            current_instance
                .as_ref()
                .is_some_and(|instance| instance == &provider.instance_id.0)
        });
        let show_interaction_mode = selected_provider
            .and_then(|provider| provider.show_interaction_mode_toggle)
            .flatten()
            .unwrap_or(true);
        let option_descriptors: Vec<ProviderOptionDescriptor> = selected_provider
            .and_then(|provider| {
                provider
                    .models
                    .iter()
                    .find(|model| Some(&model.slug.0) == current_slug.as_ref())
            })
            .and_then(|model| model.capabilities.as_ref())
            .and_then(|capabilities| capabilities.option_descriptors.clone().flatten())
            .unwrap_or_default();
        let selected_model_options: HashMap<String, serde_json::Value> = current_selection
            .as_ref()
            .and_then(|selection| selection.options.clone().flatten())
            .and_then(|options| options.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|option| {
                Some((
                    option.get("id")?.as_str()?.to_string(),
                    option.get("value")?.clone(),
                ))
            })
            .collect();

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
        let can_send = active_approval.is_none()
            && active_input.is_none()
            && (!self.composer.read(cx).value().trim().is_empty()
                || !self.pending_attachments.is_empty()
                || !self.pending_terminal_contexts.is_empty()
                || !self.pending_review_comments.is_empty()
                || self
                    .shell_thread(&open.id)
                    .is_some_and(|thread| thread.has_actionable_proposed_plan));
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
        } else if dispatching {
            div()
                .id("send-pending")
                .size_8()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(cx.theme().primary.opacity(0.5))
                .child(
                    ProgressCircle::new("send-spinner")
                        .loading(true)
                        .small()
                        .color(cx.theme().primary_foreground.opacity(0.7)),
                )
                .into_any_element()
        } else if !can_send {
            div()
                .id("send-disabled")
                .size_8()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(cx.theme().primary.opacity(0.32))
                .child(
                    Icon::new(IconName::ArrowUp)
                        .size_4()
                        .text_color(cx.theme().primary_foreground.opacity(0.45)),
                )
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
        // The picker rail shows every enabled provider; only probe-ready
        // providers contribute models. Picking one dispatches
        // `ThreadMetaUpdate` (see `set_model`).
        let model_button = Button::new("model-picker")
            .label(model_label)
            .icon(
                selected_provider
                    .map(|provider| crate::icons::provider_icon(&provider.driver.0, cx))
                    .unwrap_or_else(|| Icon::new(IconName::Bot)),
            )
            .ghost()
            .small()
            .tooltip("Choose model");
        let picker = self.model_picker.clone();
        let picker_focus = picker
            .as_ref()
            .map(|picker| picker.read(cx).query.focus_handle(cx));
        let owner = cx.weak_entity();
        let model_picker = gpui_component::popover::Popover::new("composer-model-menu")
            .anchor(gpui::Anchor::BottomLeft)
            .open(picker.is_some())
            .w(px(520.).min(window.viewport_size().width - px(32.)))
            .p(px(5.))
            .trigger(model_button)
            .when_some(picker_focus, |popover, focus| popover.track_focus(&focus))
            .on_open_change(move |open, window, cx| {
                let _ = owner.update(cx, |app, cx| {
                    if *open != app.model_picker.is_some() {
                        app.open_model_picker(window, cx);
                    }
                });
            })
            .content(move |_, _, _| div().children(picker.clone()))
            .into_any_element();
        let runtime_picker: gpui::AnyElement = {
            let current = runtime_mode.clone();
            let (label, icon) = match current {
                RuntimeMode::ApprovalRequired => ("Supervised", IconName::Settings),
                RuntimeMode::AutoAcceptEdits => ("Auto-accept edits", IconName::SquarePen),
                RuntimeMode::Auto => ("Auto", IconName::CircleCheck),
                RuntimeMode::FullAccess => ("Full access", IconName::Globe),
                RuntimeMode::Unknown(_) => ("Runtime", IconName::Settings),
            };
            let chat = cx.entity().downgrade();
            Button::new("runtime-mode")
                .label(label)
                .icon(Icon::new(icon))
                .ghost()
                .small()
                .dropdown_menu(move |mut menu, _, _| {
                    let entries = [
                        (RuntimeMode::ApprovalRequired, "Supervised"),
                        (RuntimeMode::AutoAcceptEdits, "Auto-accept edits"),
                        (RuntimeMode::Auto, "Auto"),
                        (RuntimeMode::FullAccess, "Full access"),
                    ];
                    for (mode, label) in entries {
                        let selected = mode == current;
                        let chat = chat.clone();
                        menu = menu.item(PopupMenuItem::new(label).checked(selected).on_click(
                            move |_, _, cx| {
                                let mode = mode.clone();
                                let _ = chat.update(cx, |this, cx| this.set_runtime_mode(mode, cx));
                            },
                        ));
                    }
                    menu
                })
                .into_any_element()
        };
        let interaction_toggle = show_interaction_mode.then(|| {
            self.render_mode_switch(
                interaction_mode == ProviderInteractionMode::Plan,
                window,
                cx,
            )
        });
        let traits_picker: Option<AnyElement> = (!option_descriptors.is_empty()).then(|| {
            let descriptors = option_descriptors.clone();
            let current_options = selected_model_options.clone();
            let chat = cx.entity().downgrade();
            Button::new("model-traits")
                .label("Reasoning")
                .icon(Icon::new(VitreIcon::Zap))
                .ghost()
                .small()
                .dropdown_menu(move |mut menu, _, _| {
                    for descriptor in &descriptors {
                        match descriptor {
                            ProviderOptionDescriptor::SelectProviderOptionDescriptor(select) => {
                                menu = menu.item(PopupMenuItem::label(select.label.0.clone()));
                                let current = current_options
                                    .get(&select.id.0)
                                    .and_then(serde_json::Value::as_str)
                                    .map(str::to_string)
                                    .or_else(|| {
                                        select.current_value.clone().flatten().map(|value| value.0)
                                    });
                                for option in &select.options {
                                    let id = select.id.0.clone();
                                    let value = option.id.0.clone();
                                    let prompt_injected = select
                                        .prompt_injected_values
                                        .as_ref()
                                        .and_then(|values| values.as_ref())
                                        .is_some_and(|values| {
                                            values.iter().any(|candidate| candidate.0 == value)
                                        });
                                    let owner = chat.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(option.label.0.clone())
                                            .checked(current.as_ref() == Some(&value))
                                            .on_click(move |_, window, cx| {
                                                let id = id.clone();
                                                let value = value.clone();
                                                let _ = owner.update(cx, |this, cx| {
                                                    if prompt_injected {
                                                        this.set_prompt_effort(&value, window, cx);
                                                    } else {
                                                        this.set_model_option(
                                                            id,
                                                            serde_json::Value::String(value),
                                                            cx,
                                                        )
                                                    }
                                                });
                                            }),
                                    );
                                }
                            }
                            ProviderOptionDescriptor::BooleanProviderOptionDescriptor(boolean) => {
                                let current = current_options
                                    .get(&boolean.id.0)
                                    .and_then(serde_json::Value::as_bool)
                                    .or_else(|| boolean.current_value.flatten())
                                    .unwrap_or(false);
                                let id = boolean.id.0.clone();
                                let owner = chat.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(boolean.label.0.clone())
                                        .checked(current)
                                        .on_click(move |_, _, cx| {
                                            let id = id.clone();
                                            let _ = owner.update(cx, |this, cx| {
                                                this.set_model_option(
                                                    id,
                                                    serde_json::Value::Bool(!current),
                                                    cx,
                                                )
                                            });
                                        }),
                                );
                            }
                            ProviderOptionDescriptor::Unknown(_) => {}
                        }
                    }
                    menu
                })
                .into_any_element()
        });
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
                        .child(model_picker)
                        .children(traits_picker)
                        .child(runtime_picker)
                        .children(interaction_toggle),
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
        let (mention_list, command_list) = self.render_composer_suggestions(cx);
        // Staged attachment chips (name + size + remove), Electron's chip row
        // above the textarea.
        let attachment_chips: Option<gpui::AnyElement> = (!self.pending_attachments.is_empty())
            .then(|| {
                let mut chips = h_flex().flex_wrap().gap_1p5().px_3().pt_2();
                for (index, attachment) in self.pending_attachments.iter().enumerate() {
                    let icon = crate::icons::file_icon(&attachment.name, cx);
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
                            .child(icon)
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
                                            this.schedule_draft_save(cx);
                                            cx.notify();
                                        }
                                    })),
                            ),
                    );
                }
                chips.into_any_element()
            });
        // Pending terminal contexts + review comments, above the textarea like
        // Electron's `ComposerPendingTerminalContexts` /
        // `ComposerPendingReviewComments`.
        let terminal_chips = self.render_pending_terminal_contexts(cx);
        let review_chips = self.render_pending_review_comments(cx);
        let actionable_plan = self
            .shell_thread(&open.id)
            .filter(|thread| thread.has_actionable_proposed_plan)
            .and(view)
            .and_then(|view| view.proposed_plans.as_ref())
            .and_then(|plans| plans.as_ref())
            .and_then(|plans| {
                plans
                    .iter()
                    .rev()
                    .find(|plan| !matches!(plan.implemented_at, Some(Some(Some(_)))))
            });
        let plan_followup_banner: Option<AnyElement> = actionable_plan.map(|plan| {
            let has_draft = !self.composer.read(cx).value().trim().is_empty();
            let plan_for_new_thread = plan.clone();
            h_flex()
                .px_3()
                .py_2()
                .gap_2()
                .border_b_1()
                .border_color(cx.theme().info.opacity(0.25))
                .bg(cx.theme().info.opacity(0.06))
                .child(
                    div()
                        .px_1p5()
                        .rounded(px(5.))
                        .bg(cx.theme().info.opacity(0.15))
                        .text_size(px(10.))
                        .font_semibold()
                        .text_color(cx.theme().info)
                        .child("PLAN READY"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .child(SharedString::from(
                            proposed_plan_title(&plan.plan_markdown)
                                .unwrap_or_else(|| "Review the proposed plan".into()),
                        )),
                )
                .child(
                    Button::new("plan-follow-up")
                        .small()
                        .label(if has_draft { "Refine" } else { "Implement" })
                        .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
                )
                .when(!has_draft, |banner| {
                    banner.child(
                        Button::new("plan-follow-up-new-thread")
                            .small()
                            .ghost()
                            .label("New thread")
                            .tooltip("Implement this plan in a new thread")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.implement_plan_in_new_thread(
                                    plan_for_new_thread.clone(),
                                    window,
                                    cx,
                                );
                            })),
                    )
                })
                .into_any_element()
        });
        let provider_status = selected_provider.and_then(|provider| {
            let unhealthy = provider.auth.status
                == vitre_contracts::ServerProviderAuthStatus::Unauthenticated
                || provider.status != vitre_contracts::ServerProviderState::Ready
                || provider.availability
                    == Some(Some(
                        vitre_contracts::ServerProviderAvailability::Unavailable,
                    ));
            unhealthy.then(|| {
                provider
                    .message
                    .clone()
                    .flatten()
                    .or_else(|| provider.unavailable_reason.clone().flatten())
                    .map(|message| message.0)
                    .unwrap_or_else(|| {
                        if provider.auth.status
                            == vitre_contracts::ServerProviderAuthStatus::Unauthenticated
                        {
                            "Sign in via the CLI to authenticate again.".into()
                        } else {
                            "The selected provider is currently unavailable.".into()
                        }
                    })
            })
        });
        let status_copy = if self.shell.phase == SyncPhase::Disconnected {
            Some("Environment unavailable — reconnecting…".to_string())
        } else {
            provider_status
        };
        let status_banner: Option<AnyElement> = status_copy.map(|copy| {
            h_flex()
                .w_full()
                .max_w(px(768.))
                .mx_auto()
                .px_3()
                .py_2()
                .gap_2()
                .rounded(px(10.))
                .border_1()
                .border_color(cx.theme().warning.opacity(0.35))
                .bg(cx.theme().warning.opacity(0.08))
                .text_xs()
                .text_color(cx.theme().warning)
                .child(Icon::new(IconName::TriangleAlert).size_3p5())
                .child(div().flex_1().min_w_0().child(SharedString::from(copy)))
                .into_any_element()
        });
        let composer = v_flex()
            .px_5()
            .pb_4()
            .gap_2()
            .children(status_banner)
            .children(self.lifecycle_banner(cx))
            .when(running, |column| {
                column.child(
                    h_flex()
                        .mx_auto()
                        .w_full()
                        .max_w(px(768.))
                        .px_3()
                        .gap_2()
                        .h(px(28.))
                        .child(self.activity_orb.clone())
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    if !pending_approvals.is_empty() || !pending_inputs.is_empty() {
                                        "Waiting for your input"
                                    } else {
                                        "Working on your request"
                                    },
                                ),
                        ),
                )
            })
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(768.))
                    .mx_auto()
                    .drag_over::<ExternalPaths>(|style, _, _, cx| {
                        style
                            .border_color(cx.theme().info)
                            .bg(cx.theme().info.opacity(0.06))
                    })
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                        this.stage_attachment_paths(paths.paths().iter(), window, cx);
                    }))
                    .drag_over::<crate::files::FileContext>(|style, _, _, cx| {
                        style.bg(cx.theme().drop_target)
                    })
                    .on_drop(cx.listener(
                        |this, context: &crate::files::FileContext, window, cx| {
                            this.insert_file_context(context, window, cx)
                        },
                    ))
                    .capture_key_down(cx.listener(
                        |this, event: &gpui::KeyDownEvent, window, cx| {
                            let modifiers = event.keystroke.modifiers;
                            let paste = event.keystroke.key == "v"
                                && if cfg!(target_os = "macos") {
                                    modifiers.platform
                                } else {
                                    modifiers.control
                                };
                            if paste
                                && let Some(clipboard) = cx.read_from_clipboard()
                                && this.stage_clipboard_item(&clipboard, window, cx)
                            {
                                cx.stop_propagation();
                                return;
                            }
                            let handled = match event.keystroke.key.as_str() {
                                "up" => this.move_composer_suggestion(-1, cx),
                                "down" => this.move_composer_suggestion(1, cx),
                                "escape"
                                    if this.composer_menu.is_some() || this.mention.is_some() =>
                                {
                                    this.composer_menu = None;
                                    this.mention = None;
                                    cx.notify();
                                    true
                                }
                                _ => false,
                            };
                            if handled {
                                cx.stop_propagation();
                            }
                        },
                    ))
                    .rounded(px(22.))
                    .border_1()
                    .border_color(gpui_base::motion::transition(
                        "composer-focus-border",
                        if self.composer.focus_handle(cx).is_focused(window) {
                            cx.theme().primary.opacity(0.55)
                        } else {
                            cx.theme().border
                        },
                        gpui_base::motion::Transition::new(std::time::Duration::from_millis(160)),
                        window,
                        cx,
                    ))
                    .bg(crate::glass::control(cx))
                    .overflow_hidden()
                    .children(panel)
                    .children(plan_followup_banner)
                    .children(mention_list)
                    .children(command_list)
                    .children(attachment_chips)
                    .children(terminal_chips)
                    .children(review_chips)
                    .child(
                        div()
                            .px_3()
                            .pt_3()
                            .child(Textarea::new(&self.composer).appearance(false)),
                    )
                    .child(footer),
            );

        // A compact map of user turns. Keep a representative sample for very
        // long threads so the rail stays useful instead of becoming a solid
        // stripe; the first and last turns are always represented.
        let all_markers: Vec<(usize, SharedString)> = self
            .timeline
            .iter()
            .enumerate()
            .filter_map(|(item_ix, row)| {
                let TimelineRow::Message(message_ix) = row else {
                    return None;
                };
                let message = self.display_messages.get(*message_ix)?;
                (message.role == OrchestrationMessageRole::User).then(|| {
                    let prompt = message
                        .text
                        .0
                        .split_whitespace()
                        .take(12)
                        .collect::<Vec<_>>()
                        .join(" ");
                    let reply = self.timeline[item_ix + 1..].iter().find_map(|next| {
                        let TimelineRow::Message(next_ix) = next else {
                            return None;
                        };
                        let next = self.display_messages.get(*next_ix)?;
                        (next.role == OrchestrationMessageRole::Assistant).then(|| {
                            next.text
                                .0
                                .split_whitespace()
                                .take(10)
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                    });
                    let preview = reply
                        .filter(|reply| !reply.is_empty())
                        .map_or(prompt.clone(), |reply| {
                            format!("{prompt}\n\nAssistant: {reply}")
                        });
                    (item_ix, SharedString::from(preview))
                })
            })
            .collect();
        let marker_count = all_markers.len();
        let marker_step = marker_count.div_ceil(40).max(1);
        let minimap_markers: Vec<_> = all_markers
            .into_iter()
            .enumerate()
            .filter_map(|(index, marker)| {
                (index % marker_step == 0 || index + 1 == marker_count).then_some(marker)
            })
            .collect();
        let visible_item = self.timeline_list.logical_scroll_top().item_ix;
        let active_marker = minimap_markers
            .iter()
            .rposition(|(item_ix, _)| *item_ix <= visible_item)
            .unwrap_or(0);
        let minimap: Option<AnyElement> =
            (minimap_markers.len() > 1).then(|| {
                v_flex()
                    .absolute()
                    .top_0()
                    .left(px(8.))
                    .bottom_0()
                    .w(px(24.))
                    .items_center()
                    .justify_center()
                    .child(v_flex().gap(px(3.)).items_center().children(
                        minimap_markers.into_iter().enumerate().map(
                            |(index, (item_ix, preview))| {
                                let active = index == active_marker;
                                let width = gpui_base::motion::spring(
                                    (item_ix, "timeline-marker"),
                                    if active { 18f32 } else { 8. },
                                    gpui_base::motion::Spring::new(Duration::from_millis(240)),
                                    window,
                                    cx,
                                );
                                div()
                                    .id(("timeline-marker", item_ix))
                                    .w(px(24.))
                                    .h(px(9.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .child(div().w(px(width)).h(px(3.)).rounded_full().bg(
                                        if active {
                                            cx.theme().primary
                                        } else {
                                            cx.theme().foreground.opacity(0.4)
                                        },
                                    ))
                                    .rounded(px(3.))
                                    .hover(|style| style.bg(cx.theme().foreground.opacity(0.08)))
                                    .tooltip(move |window, cx| {
                                        gpui_component::tooltip::Tooltip::new(preview.clone())
                                            .build(window, cx)
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.timeline_list.scroll_to(ListOffset {
                                            item_ix,
                                            offset_in_item: px(0.),
                                        });
                                        cx.notify();
                                    }))
                            },
                        ),
                    ))
                    .into_any_element()
            });

        // Header controls (Electron: `ProjectScriptsControl` then
        // `GitActionsControl` at the far right of the chat header, before the
        // panel toggle).
        let script_controls = self.render_project_scripts(cx);
        let git_controls = self.render_git_actions(cx);
        let workspace_controls = self.render_workspace_roots(cx);
        let mut main = v_flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .bg(crate::glass::background(cx))
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let modifiers = event.keystroke.modifiers;
                let printable = event
                    .keystroke
                    .key_char
                    .as_ref()
                    .filter(|text| text.chars().count() == 1 && !text.chars().all(char::is_control))
                    .cloned();
                if this.focus_handle.is_focused(window)
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.platform
                    && let Some(text) = printable
                {
                    this.composer.update(cx, |input, cx| {
                        input.focus_handle(cx).focus(window, cx);
                        input.insert(text, window, cx);
                    });
                    cx.stop_propagation();
                }
            }))
            .child(
                h_flex()
                    .h(px(crate::ui::CHROME_HEIGHT))
                    .px_5()
                    .items_center()
                    .gap_2()
                    .flex_shrink_0()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .children(project.map(|(name, cwd)| {
                                h_flex()
                                    .min_w_0()
                                    .max_w(px(200.))
                                    .gap_2()
                                    .items_center()
                                    .text_sm()
                                    .font_medium()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(self.project_icon(&name, &cwd, cx))
                                    .child(div().min_w_0().truncate().child(name))
                                    .child(
                                        div()
                                            .text_color(cx.theme().muted_foreground.opacity(0.4))
                                            .child("/"),
                                    )
                            }))
                            .child(
                                div()
                                    .id("chat-thread-title")
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .font_medium()
                                    .truncate()
                                    .tooltip({
                                        let title = title.clone();
                                        move |window, cx| {
                                            gpui_component::tooltip::Tooltip::new(title.clone())
                                                .build(window, cx)
                                        }
                                    })
                                    .child(title),
                            ),
                    )
                    .children(script_controls)
                    .child(workspace_controls)
                    .children(git_controls)
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
        let dismissible_error = self.last_error.is_some();
        let banner: Option<SharedString> = self.last_error.clone().or(session_error);
        if let Some(error) = banner {
            main = main.child(
                div().px_5().pt_1().child(
                    h_flex()
                        .mx_auto()
                        .w_auto()
                        .max_w(px(768.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(cx.theme().danger.opacity(0.32))
                        .bg(cx.theme().danger.opacity(0.04))
                        .px_3p5()
                        .py_3()
                        .gap_2()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .max_h(px(96.))
                                .overflow_hidden()
                                .child(error),
                        )
                        .when(dismissible_error, |banner| {
                            banner.child(
                                Button::new("dismiss-thread-error")
                                    .ghost()
                                    .xsmall()
                                    .icon(Icon::new(IconName::Close))
                                    .tooltip("Dismiss")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.last_error = None;
                                        cx.notify();
                                    })),
                            )
                        }),
                ),
            );
        }
        main.child(
            // The timeline is virtualized: only rows near the viewport are
            // built, so a long thread costs the same per frame as a short one.
            // `py_4` sits outside the scroll area (the old column's padding was
            // inside it) — the difference is invisible at the bottom anchor.
            div().relative().flex_1().py_4().map(|this| {
                if self.timeline.is_empty() && !running {
                    // Electron's `MessagesTimeline` empty state: an
                    // opened thread with nothing in it says so rather
                    // than rendering a blank list.
                    return this.child(polish::reveal("welcome-enter", self.render_welcome(cx)));
                }
                this.child(
                    list(
                        self.timeline_list.clone(),
                        cx.processor(|this, index, _window, cx| {
                            this.render_timeline_row(index, cx)
                        }),
                    )
                    .size_full(),
                )
                .children(minimap)
            }),
        )
        .children(self.render_branch_toolbar(cx))
        .child(composer)
        .children(self.render_terminal_drawer(window, cx))
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
        self.ensure_draft_project(cx);
        self.restore_composer_draft(window, cx);
        if let Some(error) = self.drafts.take_error() {
            self.runtime_notice = Some(format!("Could not save your draft: {error}").into());
        }
        if self.draft_send_after_create.as_ref().is_some_and(|id| {
            self.thread
                .as_ref()
                .is_some_and(|t| &t.id == id && t.state.view.is_some())
        }) {
            self.draft_send_after_create = None;
            self.send(window, cx);
        }
        self.sync_activity_orb(cx);
        self.sync_project_icons(cx);
        #[cfg(debug_assertions)]
        self.verify_polish_if_requested(window, cx);
        #[cfg(debug_assertions)]
        self.verify_icons_if_requested(window, cx);
        if let Some(message) = self.runtime_notice.take() {
            window.push_notification(Notification::info(message), cx);
        }
        if self.slow_requests != self.last_slow_notice {
            self.last_slow_notice = self.slow_requests.clone();
            if let Some(message) = self.slow_requests.clone() {
                window.push_notification(
                    Notification::info(message).id1::<ChatApp>("slow-rpc-notice"),
                    cx,
                );
            }
        }
        if let Some(link) = self.pending_terminal_link.take() {
            match link {
                terminal_links::TerminalLink::Url(url) => cx.open_url(&url),
                terminal_links::TerminalLink::File { path, line, column } => {
                    if let Some(relative) = self
                        .search_root()
                        .and_then(|root| path.strip_prefix(root).ok().map(Path::to_path_buf))
                    {
                        if let Some(key) = self.dock_thread_key() {
                            self.right_panel.map.open_file(
                                &key,
                                &relative.to_string_lossy(),
                                line,
                                column,
                                None,
                            );
                            self.after_dock_change(true, window, cx);
                        }
                    } else {
                        window.push_notification(
                            Notification::warning(
                                "This terminal link is outside the current workspace.",
                            ),
                            cx,
                        );
                    }
                }
            }
        }
        self.sync_preview_automation(window, cx);
        self.sync_auto_settle(cx);
        if self.request_watchdog.is_none()
            && let Some(client) = self.client.clone()
        {
            self.request_watchdog = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(1))
                        .await;
                    let requests = client.slow_requests(std::time::Duration::from_secs(5));
                    let message = (!requests.is_empty()).then(|| {
                        SharedString::from(format!(
                            "Still waiting for {}. The request is continuing.",
                            requests.join(", ")
                        ))
                    });
                    if this
                        .update(cx, |app, cx| {
                            if app.slow_requests != message {
                                app.slow_requests = message;
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }));
        }
        #[cfg(debug_assertions)]
        self.verify_preview_if_requested(window, cx);
        // Keep the shared files panel in line with the dock's active surface:
        // recreate it on project switch, apply un-applied file reveals. This
        // runs per frame, so it must not steal focus — user-initiated opens
        // focus through their own `after_dock_change(true)` path.
        let dock_reveal = gpui_base::motion::spring(
            "workspace-dock-reveal",
            if self.dock_open() { 1f32 } else { 0. },
            gpui_base::motion::Spring::new(Duration::from_millis(300)),
            window,
            cx,
        )
        .clamp(0., 1.);
        let sidebar_reveal = gpui_base::motion::spring(
            "workspace-sidebar-reveal",
            if self.sidebar_visible { 1f32 } else { 0. },
            gpui_base::motion::Spring::new(Duration::from_millis(300)),
            window,
            cx,
        )
        .clamp(0., 1.);
        let dock_key = (self.dock_open() || dock_reveal > 0.001)
            .then(|| self.dock_thread_key())
            .flatten();
        if dock_key.is_some() {
            self.sync_active_file_surface(false, window, cx);
        } else {
            // Closing the dock resets the activation edge, so reopening it
            // revalidates the files listing (Electron: the panel unmounts
            // with the dock and revalidates on remount).
            self.last_dock_active_surface = None;
        }
        self.sync_active_preview_surface(window, cx);
        // Keep resize limits aligned with the live viewport.
        self.viewport_height = f32::from(window.viewport_size().height);
        let dock_sheet_mode = self.right_panel_maximized
            || window.viewport_size().width < px(if self.sidebar_visible { 1200. } else { 960. });
        let mut dock_panel = dock_key
            .as_deref()
            .map(|key| self.render_right_panel(key, cx));
        // Tab activation changes the dock's contents, not its presentation.
        // Retain the entrance animation while switching or closing tabs.
        let presentation = dock_key
            .as_ref()
            .map(|key| format!("{key}:{dock_sheet_mode}"));
        if presentation != self.dock_presentation_key {
            self.dock_presentation_key = presentation;
            self.dock_reveal_epoch = self.dock_reveal_epoch.wrapping_add(1);
            // Reparenting a cached editor between inline and overlay layouts
            // must invalidate its painted text/layout cache as well.
            if let Some(files) = &self.files {
                files.update(cx, |_, cx| cx.notify());
            }
        }
        dock_panel =
            dock_panel.map(|panel| polish::reveal(("dock-enter", self.dock_reveal_epoch), panel));
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
            .when(cfg!(target_os = "macos"), |this| {
                this.key_context("MacPlatform")
            })
            .bg(crate::glass::root(cx))
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &QuickSearchOpen, window, cx| {
                this.toggle_quick_search(QuickSearchMode::Open, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SearchToggle, w, cx| {
                this.open_workspace_surface(vitre_state::right_panel::SurfaceKind::Search, w, cx)
            }))
            .on_action(cx.listener(|this, _: &GraphToggle, w, cx| {
                this.open_workspace_surface(vitre_state::right_panel::SurfaceKind::Graph, w, cx)
            }))
            .on_action(cx.listener(|this, _: &GraphBuild, w, cx| {
                this.open_workspace_surface(vitre_state::right_panel::SurfaceKind::Graph, w, cx);
                if let Some(panel) = this
                    .search_root()
                    .and_then(|r| this.graph_panels.get(&r).cloned())
                {
                    panel.update(cx, |p, cx| p.build(cx));
                }
            }))
            .on_action(cx.listener(|this, _: &WorkspaceRootsManage, w, cx| {
                this.attach_workspace_folder(w, cx)
            }))
            .on_action(cx.listener(|this, _: &SidebarToggle, _, cx| {
                this.sidebar_visible = !this.sidebar_visible;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditorOpenFavorite, window, cx| {
                let (Some(client), Some(cwd)) = (this.client.clone(), this.search_root()) else {
                    return;
                };
                let editor = ClientSettings::get(cx).favorite_editor.or_else(|| {
                    client
                        .sessions()
                        .borrow()
                        .as_ref()
                        .and_then(|s| s.config.available_editors.first().cloned())
                });
                let Some(editor) = editor else {
                    window.push_notification(
                        Notification::error(
                            "No external editor detected. Configure one in Settings.",
                        ),
                        cx,
                    );
                    return;
                };
                cx.spawn_in(window, async move |_, cx| {
                    let result = client
                        .call::<vitre_contracts::methods::ShellOpenInEditor>(
                            &vitre_contracts::LaunchEditorInput {
                                cwd: TrimmedNonEmptyString(cwd),
                                editor,
                            },
                        )
                        .await;
                    if let Err(e) = result {
                        let _ = cx.update(|window, cx| {
                            window.push_notification(Notification::error(e.user_message()), cx)
                        });
                    }
                })
                .detach();
            }))
            .on_action(
                cx.listener(|this, _: &crate::files::TreeToggleFocus, window, cx| {
                    if let Some(files) = this.files.clone() {
                        files.update(cx, |files, cx| files.toggle_tree_focus(window, cx));
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &QuickSearchContent, window, cx| {
                this.toggle_quick_search(QuickSearchMode::Content, window, cx);
            }))
            .on_action(cx.listener(|this, _: &CommandPaletteToggle, window, cx| {
                this.toggle_command_palette(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ModelPickerToggle, window, cx| {
                this.open_model_picker(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &crate::settings::SettingsOpen, window, cx| {
                    this.toggle_settings(window, cx);
                }),
            )
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
            .on_action(cx.listener(|this, _: &PreviewToggle, window, cx| {
                this.preview_toggle(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviewRefresh, _, cx| {
                this.preview_refresh(cx);
            }))
            .on_action(cx.listener(|this, _: &PreviewFocusUrl, window, cx| {
                this.preview_focus_url(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviewZoomIn, _, cx| {
                this.preview_zoom(1, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviewZoomOut, _, cx| {
                this.preview_zoom(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviewResetZoom, _, cx| {
                this.preview_reset_zoom(cx);
            }))
            .on_action(cx.listener(|this, _: &DiffToggle, window, cx| {
                this.diff_toggle(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ThreadPrevious, _, cx| {
                this.cycle_thread(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &ThreadNext, _, cx| {
                this.cycle_thread(1, cx);
            }))
            .on_action(cx.listener(|this, _: &ThreadJump1, _, cx| this.jump_thread(0, cx)))
            .on_action(cx.listener(|this, _: &ThreadJump2, _, cx| this.jump_thread(1, cx)))
            .on_action(cx.listener(|this, _: &ThreadJump3, _, cx| this.jump_thread(2, cx)))
            .on_action(cx.listener(|this, _: &ThreadJump4, _, cx| this.jump_thread(3, cx)))
            .on_action(cx.listener(|this, _: &ThreadJump5, _, cx| this.jump_thread(4, cx)))
            .on_action(cx.listener(|this, _: &ThreadJump6, _, cx| this.jump_thread(5, cx)))
            .on_action(cx.listener(|this, _: &ThreadJump7, _, cx| this.jump_thread(6, cx)))
            .on_action(cx.listener(|this, _: &ThreadJump8, _, cx| this.jump_thread(7, cx)))
            .on_action(cx.listener(|this, _: &ThreadJump9, _, cx| this.jump_thread(8, cx)))
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
            .when(self.preview_mini_drag.is_some(), |root| {
                root.on_mouse_move(
                    cx.listener(|app, event: &gpui::MouseMoveEvent, window, cx| {
                        app.move_preview_mini(event, window, cx)
                    }),
                )
                .on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|app, _, _, cx| {
                        app.preview_mini_drag = None;
                        cx.notify();
                    }),
                )
            })
            .child(match self.settings.as_ref() {
                // Electron's settings is a route: the workspace unmounts while
                // you are in there and comes back untouched, because none of
                // its state lived in the DOM. Same here — the panels are held
                // by this view, not by the element tree.
                Some(panel) => panel.clone().into_any_element(),
                None => {
                    let sheet_mode = dock_panel.is_some() && dock_sheet_mode;
                    let inline_dock = if sheet_mode { None } else { dock_panel.take() };
                    let workspace = h_resizable("workspace")
                        .with_state(&self.sidebar_resize)
                        .child(
                            resizable_panel()
                                .size(px(256.))
                                .size_range(px(208.)..px(480.))
                                .flex_none()
                                .visible(sidebar_reveal > 0.001)
                                .reveal(sidebar_reveal)
                                .child(self.render_sidebar(cx).into_any_element()),
                        )
                        .child(resizable_panel().child(self.render_chat(window, cx)))
                        .child(
                            // Hidden (not unmounted) when the dock is
                            // closed: the state slot keeps the width,
                            // so re-opening restores the drag size.
                            resizable_panel()
                                .size(px(self.right_panel.width))
                                .size_range(px(right_panel::MIN_PANEL_WIDTH)..dock_max_width)
                                .flex_none()
                                .visible(inline_dock.is_some())
                                .reveal(dock_reveal)
                                .children(inline_dock),
                        );
                    h_flex()
                        .size_full()
                        .relative()
                        .overflow_hidden()
                        .child(div().flex_1().min_w_0().h_full().child(workspace))
                        .children(sheet_mode.then(|| {
                            div()
                                .id("dock-sheet")
                                .occlude()
                                .absolute()
                                .top_0()
                                .right(
                                    -window.viewport_size().width
                                        * (if self.right_panel_maximized { 1. } else { 0.82 })
                                        * (1. - dock_reveal),
                                )
                                .bottom_0()
                                .w(if self.right_panel_maximized {
                                    relative(1.)
                                } else {
                                    relative(0.82)
                                })
                                .min_w(px(right_panel::MIN_PANEL_WIDTH))
                                // Behind-window blur cannot blur sibling UI.
                                // An overlay must cover the chat's text and hit targets.
                                .bg(cx.theme().background)
                                .shadow_2xl()
                                .children(dock_panel)
                        }))
                        .children(
                            active_plan
                                .as_ref()
                                .map(|plan| self.render_plan_sidebar(plan, cx)),
                        )
                        .into_any_element()
                }
            })
            .children(self.render_preview_mini(window, cx))
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ComposerTriggerKind, UserInlineSegment, active_composer_token, active_mention_token,
        collapsed_plan_markdown, format_mention, parse_user_inline_segments, plan_filename,
        proposed_plan_title, provider_picker_ready, provider_picker_visible,
        should_collapse_user_message, table_to_csv,
    };
    use gpui_component::text::TableData;
    use std::collections::HashSet;
    use vitre_contracts::ThreadId;

    #[test]
    fn interaction_mode_round_trip_does_not_reuse_stale_detail_state() {
        use super::resolve_interaction_mode;
        use vitre_contracts::ProviderInteractionMode::{Default as Build, Plan};

        // Initial snapshot is Build. Clicking Plan must affect both the
        // toolbar and the next send before the shell event arrives.
        assert_eq!(
            resolve_interaction_mode(Some(&Plan), Some(&Build), Some(&Build)),
            Plan
        );
        // The shell acknowledges Plan, but no new detail snapshot is sent.
        assert_eq!(
            resolve_interaction_mode(None, Some(&Plan), Some(&Build)),
            Plan
        );
        // The reverse transition must also outrank a stale Plan snapshot.
        assert_eq!(
            resolve_interaction_mode(Some(&Build), Some(&Plan), Some(&Plan)),
            Build
        );
        assert_eq!(
            resolve_interaction_mode(None, Some(&Build), Some(&Plan)),
            Build
        );
        // Failed optimistic writes fall back to the canonical shell.
        assert_eq!(
            resolve_interaction_mode(None, Some(&Build), Some(&Build)),
            Build
        );
        assert_eq!(resolve_interaction_mode(None, None, Some(&Plan)), Plan);
        assert_eq!(resolve_interaction_mode(None, None, None), Build);
    }

    #[test]
    fn parses_known_skill_and_thread_chips_without_eating_surrounding_text() {
        let skills = HashSet::from(["review".to_string()]);
        assert_eq!(
            parse_user_inline_segments(
                "Ask $review about [#Rust port](t3code://thread/thread-2) next",
                &skills,
            ),
            vec![
                UserInlineSegment::Text("Ask ".into()),
                UserInlineSegment::Skill("review".into()),
                UserInlineSegment::Text(" about ".into()),
                UserInlineSegment::Thread {
                    label: "Rust port".into(),
                    id: ThreadId("thread-2".into()),
                },
                UserInlineSegment::Text(" next".into()),
            ]
        );
    }

    #[test]
    fn leaves_unknown_skills_as_plain_text() {
        assert_eq!(
            parse_user_inline_segments("use $missing", &HashSet::new()),
            vec![UserInlineSegment::Text("use $missing".into())]
        );
    }

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

    #[test]
    fn composer_triggers_cover_commands_threads_and_skills() {
        assert_eq!(
            active_composer_token("try /mod", 8),
            Some((ComposerTriggerKind::Slash, 4, "mod".into()))
        );
        assert_eq!(
            active_composer_token("see #rust", 9),
            Some((ComposerTriggerKind::Thread, 4, "rust".into()))
        );
        assert_eq!(
            active_composer_token("use $review", 11),
            Some((ComposerTriggerKind::Skill, 4, "review".into()))
        );
        assert_eq!(active_composer_token("#", 1), None);
        assert_eq!(active_composer_token("## heading", 2), None);
    }

    #[test]
    fn table_csv_quotes_special_cells() {
        let table = TableData {
            headers: vec!["name".into(), "note".into()],
            rows: vec![vec!["Vitre".into(), "glass, fast \"native\"".into()]],
            markdown: String::new(),
            span: None,
        };
        assert_eq!(
            table_to_csv(&table),
            "name,note\nVitre,\"glass, fast \"\"native\"\"\""
        );
    }

    #[test]
    fn proposed_plan_helpers_are_stable() {
        let plan = "# Ship the glass UI\n\n1. Build\n2. Verify\n3. Package\n4. Test\n5. Review\n6. Sign\n7. Release\n8. Celebrate\n9. Follow up";
        assert_eq!(
            proposed_plan_title(plan).as_deref(),
            Some("Ship the glass UI")
        );
        assert_eq!(plan_filename(plan), "ship-the-glass-ui.md");
        assert!(collapsed_plan_markdown(plan).ends_with('…'));
    }

    #[test]
    fn long_user_message_threshold_matches_electron() {
        assert!(!should_collapse_user_message("short prompt"));
        assert!(should_collapse_user_message(&"x".repeat(601)));
        assert!(should_collapse_user_message(&["x"; 9].join("\n")));
    }

    fn provider_fixture(
        id: &str,
        enabled: bool,
        installed: bool,
        status: &str,
        availability: Option<&str>,
    ) -> vitre_contracts::ServerProvider {
        let mut value = serde_json::json!({
            "auth": { "status": "authenticated" },
            "checkedAt": "2026-09-15T00:00:00.000Z",
            "driver": id,
            "enabled": enabled,
            "installed": installed,
            "instanceId": id,
            "models": [],
            "status": status,
            "version": null
        });
        if let Some(availability) = availability {
            value["availability"] = serde_json::Value::String(availability.to_string());
        }
        serde_json::from_value(value).expect("provider fixture should match the wire schema")
    }

    #[test]
    fn model_picker_shows_enabled_providers_but_only_uses_ready_models() {
        let claude = provider_fixture("claudeAgent", true, true, "ready", None);
        let remote = provider_fixture("remoteCustom", true, false, "ready", Some("available"));
        let grok = provider_fixture("grok", true, false, "error", Some("unavailable"));
        let cursor = provider_fixture("cursor", false, false, "disabled", None);

        assert!(provider_picker_visible(&claude));
        assert!(provider_picker_ready(&claude));
        // Matches T3 Code: probe readiness, not a local-CLI install bit, is
        // authoritative for remote/custom providers.
        assert!(provider_picker_ready(&remote));
        // Enabled-but-unavailable instances remain discoverable in the rail.
        assert!(provider_picker_visible(&grok));
        assert!(!provider_picker_ready(&grok));
        // Explicitly disabled instances are omitted entirely.
        assert!(!provider_picker_visible(&cursor));
        assert!(!provider_picker_ready(&cursor));
    }
}
