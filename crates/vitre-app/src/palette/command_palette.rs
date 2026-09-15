//! The ⇧⌘P command palette, ported from `apps/web/src/components/CommandPalette.tsx`.
//!
//! Like [`super::quick_search`], the surface itself is gpui-component's
//! [`Command`] inside a [`Dialog`](gpui_component::dialog::Dialog); what is
//! ours is the item model, the group assembly, the Electron ranking rules in
//! [`super::rank`], and the add-project flow (Sources → in-palette filesystem
//! browsing / remote clone → `project.create`), whose stage machine lives in
//! [`super::add_project`]. Commands whose subsystems Vitre does not have yet —
//! terminal, knowledge graph, workspace roots, settings — are still absent.
//!
//! Known divergences from Electron, all deliberate:
//! - The "Setup Required" badge shows its readiness hint as a toast instead of
//!   opening the source-control settings page, which Vitre does not have yet.
//! - An empty browse listing shows the empty-state copy instead of Electron's
//!   bare "Directories" heading (its own port note flags that as a bug).
//! - The clone-destination "Repository" block sits above the search field
//!   (the [`Command::header`] slot) rather than between it and the list.
//! - "Open in Finder" cannot seed the picker's initial directory — gpui's
//!   `prompt_for_paths` takes no initial path at the pinned revision.

use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, IntoElement, KeyDownEvent, PathPromptOptions,
    SharedString, WeakEntity, Window, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    command::{Command, CommandGroup, CommandItem, CommandState},
    h_flex,
    kbd::Kbd,
    notification::Notification,
    v_flex,
};
use gpui_component::{IndexPath, WindowExt as _};
use vitre_client::EnvironmentClient;
use vitre_contracts::{
    ClientOrchestrationCommand, CommandId, FilesystemBrowseEntry, FilesystemBrowseInput,
    FilesystemBrowseResult, ModelSelection, OrchestrationProjectShell, OrchestrationThreadShell,
    ProjectCreateCommand, ProjectId, SourceControlCloneRepositoryInput,
    SourceControlDiscoveryResult, SourceControlRepositoryLookupInput, ThreadId,
    TrimmedNonEmptyString,
    methods::{
        FilesystemBrowse, ServerDiscoverSourceControl, SourceControlCloneRepository,
        SourceControlLookupRepository,
    },
};
use vitre_state::browse_path::{
    append_browse_path_segment, browse_entry_visible, can_navigate_up,
    ensure_browse_directory_path, get_browse_directory_path, get_browse_leaf_path_segment,
    get_browse_parent_path, has_trailing_path_separator, infer_project_title_from_path,
    is_explicit_relative_path, is_filesystem_browse_query, is_unsupported_windows_project_path,
    resolve_project_path_for_dispatch,
};
use vitre_state::project_grouping::normalize_project_path_for_comparison;

use crate::chat::{
    CommandPaletteToggle, NewThread, QuickSearchContent, QuickSearchOpen, RightPanelToggle,
    TerminalToggle, fresh_id, now_iso, tnes,
};

use super::add_project::{
    AddProjectStage, RemoteSource, SourceReadiness, ordered_provider_sources,
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
    /// The add-project flow dispatched `project.create` and it succeeded;
    /// Electron follows with a fresh thread in the new project.
    ProjectCreated(ProjectId),
    QuickSearchOpen,
    QuickSearchContent,
    ToggleFilesPanel,
    ToggleTerminal,
    WorkspaceSearch,
    KnowledgeGraph,
    BuildGraph,
    WorkspaceFolders,
    NewFile,
    NewFolder,
    /// Electron exposes this as a Settings → Editor switch; the palette keeps
    /// it as a shortcut to the same preference.
    ToggleVimMode,
    /// Electron's `action:settings` row, which navigates to `/settings`.
    OpenSettings,
}

pub enum CommandPaletteEvent {
    Run(PaletteAction),
    /// The dialog closed; the shell should drop its handle.
    Dismissed,
}

/// What confirming a row does. Electron models the same split with `run`
/// callbacks plus `keepOpen`; the palette-closing runs are [`ItemRun::Action`]
/// (forwarded to the shell), everything else stays inside the palette.
#[derive(Clone)]
enum ItemRun {
    /// A row that does nothing, such as a disabled provider source.
    Inert,
    Action(PaletteAction),
    /// A row that pushes a plain sub-view instead of running an action.
    Submenu(PaletteGroup),
    /// The root "Add project" row: enter the flow at the Sources view.
    StartAddProject,
    /// Sources → "Local folder": browse seeded from the base directory.
    SourceLocal,
    /// Sources → a remote source: enter the clone flow's repository step.
    SourceRemote(RemoteSource),
    /// The ".." browse row.
    BrowseUp,
    /// A directory browse row; descends by appending the entry name.
    BrowseTo(String),
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
    run: ItemRun,
    disabled: bool,
    /// A disabled provider row's "Setup Required" badge; the string is the
    /// readiness hint it surfaces when clicked.
    badge: Option<SharedString>,
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
            run: ItemRun::Inert,
            disabled: false,
            badge: None,
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

    fn run(mut self, run: ItemRun) -> Self {
        self.run = run;
        self
    }

    fn action(self, action: PaletteAction) -> Self {
        self.run(ItemRun::Action(action))
    }

    fn submenu(self, group: PaletteGroup) -> Self {
        self.run(ItemRun::Submenu(group))
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

    // --- add-project flow ---
    client: Option<Arc<EnvironmentClient>>,
    /// The **server's** platform: it decides whether Windows-shaped paths
    /// count as browse queries, exactly as Electron keys this off the browse
    /// environment's platform rather than the client's.
    windows_platform: bool,
    /// `settings.addProjectBaseDirectory`, seeding the initial browse query.
    base_directory: Option<String>,
    /// The active project's workspace root; relative paths resolve against it.
    active_project_cwd: Option<String>,
    /// The environment's default provider model, stamped onto `project.create`.
    default_model_selection: Option<ModelSelection>,
    /// Normalized workspace roots for the dedupe-by-path step.
    project_roots: Vec<(ProjectId, String)>,
    flow: Option<AddProjectStage>,
    /// `server.discoverSourceControl`, fetched when the flow starts; gates the
    /// provider rows exactly as Electron's readiness rules do.
    discovery: Option<SourceControlDiscoveryResult>,
    /// The directory portion the current `browse_result` answers (or the one
    /// in flight). Typing within a leaf never changes it, so no refetch runs
    /// until the user crosses a separator — Electron's caching effect.
    browse_dir: Option<String>,
    browse_result: Option<FilesystemBrowseResult>,
    /// Electron's `isBrowsePending`: no spinner, it only suppresses the
    /// "Create &" button label and the create empty-state.
    browse_pending: bool,
    browse_generation: u64,
    /// Clone repository step's lookup in flight ("Working").
    looking_up: bool,
    /// `sourceControl.cloneRepository` in flight ("Cloning").
    cloning: bool,
    /// The native folder picker is up; disables the footer button.
    picking: bool,
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
    pub client: Option<Arc<EnvironmentClient>>,
    pub windows_platform: bool,
    pub base_directory: Option<String>,
    pub active_project_cwd: Option<String>,
    pub default_model_selection: Option<ModelSelection>,
    /// Open straight into the add-project flow — the sidebar FolderPlus
    /// button's `openCommandPalette({open: "add-project"})` intent.
    pub open_add_project: bool,
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
        let project_roots = context
            .projects
            .iter()
            .map(|project| {
                (
                    project.id.clone(),
                    normalize_project_path_for_comparison(&project.workspace_root.0),
                )
            })
            .collect();
        let open_add_project = context.open_add_project;

        let palette = cx.new(|cx| Self {
            state: cx.new(|cx| CommandState::new(window, cx)),
            query: String::new(),
            stack: Vec::new(),
            displayed: Vec::new(),
            actions,
            threads,
            projects,
            client: context.client,
            windows_platform: context.windows_platform,
            base_directory: context.base_directory,
            active_project_cwd: context.active_project_cwd,
            default_model_selection: context.default_model_selection,
            project_roots,
            flow: None,
            discovery: None,
            browse_dir: None,
            browse_result: None,
            browse_pending: false,
            browse_generation: 0,
            looking_up: false,
            cloning: false,
            picking: false,
        });

        // Electron consumes the add-project intent in a pre-paint layout
        // effect so the root view never flashes; starting the flow before the
        // dialog's first frame gives the same result.
        if open_add_project {
            palette.update(cx, |palette, cx| {
                palette.start_add_project_flow(window, cx);
            });
        }

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

    // MARK: Modes

    /// Electron's `isBrowsing`: a path-shaped query flips any stage except the
    /// clone repository step into filesystem browsing — including the root.
    fn is_browsing(&self) -> bool {
        !matches!(self.flow, Some(AddProjectStage::CloneRepository { .. }))
            && is_filesystem_browse_query(&self.query, self.windows_platform)
    }

    /// Whether Enter (or the inline button) submits the typed path: browsing
    /// anywhere, or standing in the browse / clone-destination views.
    fn can_submit_browse_path(&self) -> bool {
        self.is_browsing()
            || matches!(
                self.flow,
                Some(AddProjectStage::Browse | AddProjectStage::CloneDestination { .. })
            )
    }

    /// An explicit `./`-style query with no active project to resolve against
    /// blanks the list and disables submission.
    fn relative_path_needs_active_project(&self) -> bool {
        self.is_browsing()
            && is_explicit_relative_path(self.query.trim())
            && self.active_project_cwd.is_none()
    }

    /// Whether the highlighted row is a browse row — the ⌘Enter semantics and
    /// the "Create &" labels key off exactly this. Takes the already-resolved
    /// selection because callers include the state's own render slots, where
    /// reading the [`CommandState`] entity again would be a reentrant borrow.
    fn highlighted_browse_row(&self, selected: Option<IndexPath>) -> bool {
        let Some(index) = selected else {
            return false;
        };
        let Some(item) = self
            .displayed
            .get(index.section)
            .and_then(|group| group.items.get(index.row))
        else {
            return false;
        };
        matches!(item.run, ItemRun::BrowseTo(_) | ItemRun::BrowseUp)
    }

    /// The fetched listing, but only when it answers the query's current
    /// directory portion — a stale result must not resolve submissions.
    fn current_browse_result(&self) -> Option<&FilesystemBrowseResult> {
        let directory = get_browse_directory_path(&self.query);
        if self.browse_dir.as_deref() == Some(directory) {
            self.browse_result.as_ref()
        } else {
            None
        }
    }

    /// Case-**sensitive** exact match of the typed leaf against the listing,
    /// as Electron's `exactBrowseEntry` is.
    fn exact_browse_entry(&self) -> Option<&FilesystemBrowseEntry> {
        if has_trailing_path_separator(&self.query) {
            return None;
        }
        let leaf = get_browse_leaf_path_segment(&self.query);
        if leaf.is_empty() {
            return None;
        }
        self.current_browse_result()?
            .entries
            .iter()
            .find(|entry| entry.name.0 == leaf)
    }

    /// Electron's `resolvedAddProjectPath`: a trailing separator submits the
    /// browsed directory itself (the server-resolved absolute `parentPath`,
    /// which is how `~/` submits as the real home dir); otherwise an exact
    /// entry wins over the raw typed text.
    fn resolved_add_project_path(&self) -> String {
        if has_trailing_path_separator(&self.query) {
            if let Some(result) = self.current_browse_result() {
                return result.parent_path.0.clone();
            }
            return self.query.trim().to_owned();
        }
        if let Some(entry) = self.exact_browse_entry() {
            return entry.full_path.0.clone();
        }
        self.query.trim().to_owned()
    }

    /// Whether submitting would `mkdir` a new folder — flips the button label
    /// to "Create & Add" / "Create & Clone" and the empty-state copy.
    fn will_create_project_path(&self, selected: Option<IndexPath>) -> bool {
        self.can_submit_browse_path()
            && !self.browse_pending
            && !self.query.trim().is_empty()
            && !self.highlighted_browse_row(selected)
            && if has_trailing_path_separator(&self.query) {
                self.current_browse_result().is_none()
            } else {
                self.exact_browse_entry().is_none()
            }
    }

    /// `addProjectBaseDirectory` with a guaranteed trailing separator, else
    /// `~/` — what the browse and clone-destination views open on.
    fn initial_browse_query(&self) -> String {
        match self.base_directory.as_deref().map(str::trim) {
            Some(directory) if !directory.is_empty() => ensure_browse_directory_path(directory),
            _ => "~/".to_owned(),
        }
    }

    // MARK: Group assembly

    fn visible_groups(&self) -> Vec<PaletteGroup> {
        if self.is_browsing() {
            return self.browse_groups();
        }
        match &self.flow {
            Some(AddProjectStage::Sources) => {
                filter_groups(vec![self.sources_group()], &self.query)
            }
            // The clone steps and the browse view with a non-path query render
            // no rows; the empty-state copy carries the instructions.
            Some(_) => Vec::new(),
            None => {
                let groups = match self.stack.last() {
                    Some(view) => vec![view.clone()],
                    None => root_groups(&self.actions, &self.projects, &self.threads, &self.query),
                };
                filter_groups(groups, &self.query)
            }
        }
    }

    /// The Sources view, rebuilt from the latest discovery data every render —
    /// Electron recomputes `activeGroups` the same way.
    fn sources_group(&self) -> PaletteGroup {
        let mut items = vec![
            PaletteItem::new(
                "Local folder",
                IconName::FolderPlus,
                &["local", "folder", "directory", "browse"],
            )
            .description("Browse a folder on disk")
            .run(ItemRun::SourceLocal),
            source_item(
                RemoteSource::Url,
                SourceReadiness {
                    ready: true,
                    hint: None,
                },
            ),
        ];
        for (source, readiness) in ordered_provider_sources(self.discovery.as_ref()) {
            items.push(source_item(source, readiness));
        }
        PaletteGroup::new("Sources", items)
    }

    /// Electron's `buildBrowseGroups`: an optional `..` row, then the fetched
    /// directories filtered by the typed leaf.
    fn browse_groups(&self) -> Vec<PaletteGroup> {
        if self.relative_path_needs_active_project() {
            return Vec::new();
        }
        let mut items = Vec::new();
        if can_navigate_up(get_browse_directory_path(&self.query)) {
            items.push(PaletteItem::new("..", IconName::CornerLeftUp, &[]).run(ItemRun::BrowseUp));
        }
        let leaf = if has_trailing_path_separator(&self.query) {
            ""
        } else {
            get_browse_leaf_path_segment(&self.query)
        };
        if let Some(result) = self.current_browse_result() {
            for entry in &result.entries {
                if browse_entry_visible(&entry.name.0, leaf) {
                    items.push(
                        PaletteItem::new(entry.name.0.clone(), IconName::Folder, &[])
                            .run(ItemRun::BrowseTo(entry.name.0.clone())),
                    );
                }
            }
        }
        if items.is_empty() {
            // Electron renders a bare "Directories" heading here; showing the
            // empty-state copy instead is a deliberate fix (module note).
            return Vec::new();
        }
        let label = if matches!(self.flow, Some(AddProjectStage::CloneDestination { .. })) {
            "Select where to clone"
        } else {
            "Directories"
        };
        vec![PaletteGroup::new(label, items)]
    }

    /// The empty-state copy, in Electron's precedence order.
    fn empty_copy(&self, selected: Option<IndexPath>) -> SharedString {
        if self.can_submit_browse_path() && self.relative_path_needs_active_project() {
            return "Relative paths require an active project.".into();
        }
        if let Some(AddProjectStage::CloneRepository { source }) = &self.flow {
            return source.repository_empty_copy().into();
        }
        if self.will_create_project_path(selected) {
            return "Press Enter to create this folder and add it as a project.".into();
        }
        if matches!(self.flow, Some(AddProjectStage::CloneDestination { .. })) {
            return "Choose a destination path and press Enter to clone.".into();
        }
        if self.is_browsing() || matches!(self.flow, Some(AddProjectStage::Browse)) {
            // Mid-fetch, or a leaf that filtered everything out while the
            // fetch is pending: show nothing rather than a wrong message.
            return "".into();
        }
        if self.query.starts_with('>') {
            "No matching actions.".into()
        } else {
            "No matching commands, projects, or threads.".into()
        }
    }

    // MARK: Row confirmation and navigation

    fn confirm(&mut self, index: IndexPath, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self
            .displayed
            .get(index.section)
            .and_then(|group| group.items.get(index.row))
            .cloned()
        else {
            return;
        };
        if item.disabled {
            return;
        }
        match item.run {
            ItemRun::Inert => {}
            ItemRun::Submenu(group) => self.push_view(group, window, cx),
            ItemRun::Action(action) => {
                cx.emit(CommandPaletteEvent::Run(action));
                self.dismiss(window, cx);
            }
            ItemRun::StartAddProject => self.start_add_project_flow(window, cx),
            ItemRun::SourceLocal => self.start_browse(window, cx),
            ItemRun::SourceRemote(source) => self.start_clone(source, window, cx),
            ItemRun::BrowseUp => self.browse_up(window, cx),
            ItemRun::BrowseTo(name) => self.browse_to(&name, window, cx),
        }
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
        self.set_stage_query("", window, cx);
    }

    /// Backspace at an empty query (or the back button) pops one level, as
    /// Electron's does — clearing the query like its `popView`.
    fn pop_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.stack.pop().is_some() {
            self.set_stage_query("", window, cx);
        }
    }

    /// Electron's `popView` inside the flow: every stage returns to Sources
    /// (popping also clears the clone flow there), and Sources returns to the
    /// root palette.
    fn pop_flow_stage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.flow {
            Some(AddProjectStage::Sources) | None => self.flow = None,
            Some(_) => self.flow = Some(AddProjectStage::Sources),
        }
        self.set_stage_query("", window, cx);
    }

    /// The input's back affordance: flow stages first, then plain sub-views.
    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.flow.is_some() {
            self.pop_flow_stage(window, cx);
        } else {
            self.pop_view(window, cx);
        }
    }

    // MARK: Flow transitions

    fn start_add_project_flow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.client.is_none() {
            // Electron's zero-environments branch.
            window.push_notification(
                Notification::error("No environment is available.")
                    .title("Unable to browse projects"),
                cx,
            );
            return;
        }
        self.flow = Some(AddProjectStage::Sources);
        self.set_stage_query("", window, cx);
        self.fetch_discovery(cx);
    }

    fn start_browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flow = Some(AddProjectStage::Browse);
        let initial = self.initial_browse_query();
        self.set_stage_query(&initial, window, cx);
    }

    fn start_clone(&mut self, source: RemoteSource, window: &mut Window, cx: &mut Context<Self>) {
        self.flow = Some(AddProjectStage::CloneRepository { source });
        self.set_stage_query("", window, cx);
    }

    fn browse_to(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let next = append_browse_path_segment(&self.query, name);
        self.set_stage_query(&next, window, cx);
    }

    fn browse_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(parent) = get_browse_parent_path(&self.query) {
            self.set_stage_query(&parent, window, cx);
        }
    }

    /// Replace the query as part of a stage change. `self.query` is written
    /// first so the deferred `on_query` echo of `set_query` is a no-op rather
    /// than a user-cleared pop.
    fn set_stage_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.query = query.to_owned();
        let query: SharedString = query.to_owned().into();
        self.state
            .update(cx, |state, cx| state.set_query(query, window, cx));
        self.sync_browse(cx);
        self.sync_highlight(window, cx);
        cx.notify();
    }

    /// Every query change, typed or programmatic.
    fn handle_query(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        let user_cleared = query.is_empty() && !self.query.is_empty();
        self.query = query;
        if user_cleared
            && matches!(
                self.flow,
                Some(AddProjectStage::Browse | AddProjectStage::CloneDestination { .. })
            )
        {
            // Deleting the whole path of a browse-seeded view returns to the
            // previous view (Electron's `handleQueryChange`).
            self.pop_flow_stage(window, cx);
            return;
        }
        self.sync_browse(cx);
        self.sync_highlight(window, cx);
        cx.notify();
    }

    /// Electron's `autoHighlight: false` while browsing or in the clone flow:
    /// no row is preselected, so plain Enter submits the typed path. The
    /// state auto-highlights the first row on every query change, so this
    /// clears it right back.
    fn sync_highlight(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_browsing()
            || matches!(
                self.flow,
                Some(
                    AddProjectStage::CloneRepository { .. }
                        | AddProjectStage::CloneDestination { .. }
                )
            )
        {
            self.state
                .update(cx, |state, cx| state.set_selected_index(None, window, cx));
        }
    }

    /// Refetch `filesystem.browse` when the query's directory portion changes.
    /// Typing within a leaf keeps the listing; errors read as an empty one,
    /// exactly as Electron leaves its query errors unrendered.
    fn sync_browse(&mut self, cx: &mut Context<Self>) {
        let directory = if self.client.is_some()
            && self.is_browsing()
            && !self.relative_path_needs_active_project()
        {
            get_browse_directory_path(&self.query).to_owned()
        } else {
            String::new()
        };
        if directory.is_empty() {
            self.browse_dir = None;
            self.browse_result = None;
            self.browse_pending = false;
            return;
        }
        if self.browse_dir.as_deref() == Some(directory.as_str()) {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.browse_dir = Some(directory.clone());
        self.browse_result = None;
        self.browse_pending = true;
        self.browse_generation += 1;
        let generation = self.browse_generation;
        let cwd = self.active_project_cwd.clone();
        cx.spawn(async move |this, cx| {
            let payload = FilesystemBrowseInput {
                cwd: cwd.map(|cwd| Some(TrimmedNonEmptyString(cwd))),
                partial_path: TrimmedNonEmptyString(directory),
            };
            let result = client.call::<FilesystemBrowse>(&payload).await;
            let _ = this.update(cx, |this, cx| {
                if this.browse_generation != generation {
                    return;
                }
                this.browse_pending = false;
                this.browse_result = result.ok();
                cx.notify();
            });
        })
        .detach();
    }

    fn fetch_discovery(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            if let Ok(discovery) = client
                .call::<ServerDiscoverSourceControl>(&serde_json::json!({}))
                .await
            {
                let _ = this.update(cx, |this, cx| {
                    this.discovery = Some(discovery);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    // MARK: Submission

    /// Enter (or the inline button) on a path: adds the project, or picks the
    /// clone destination when the flow is on that step.
    fn submit_browse_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.resolved_add_project_path();
        if matches!(self.flow, Some(AddProjectStage::CloneDestination { .. })) {
            self.submit_clone_destination(path, window, cx);
        } else {
            self.handle_add_project(path, window, cx);
        }
    }

    /// Electron's `handleAddProject` pipeline: guards → resolve → dedupe by
    /// normalized root → `project.create`. Success closes the palette with no
    /// toast; failure toasts and keeps it open.
    fn handle_add_project(&mut self, raw: String, window: &mut Window, cx: &mut Context<Self>) {
        if is_unsupported_windows_project_path(&raw, self.windows_platform) {
            error_toast(
                "Failed to add project",
                "Windows-style paths are only supported on Windows.",
                window,
                cx,
            );
            return;
        }
        if is_explicit_relative_path(raw.trim()) && self.active_project_cwd.is_none() {
            error_toast(
                "Failed to add project",
                "Relative paths require an active project.",
                window,
                cx,
            );
            return;
        }
        let cwd = resolve_project_path_for_dispatch(&raw, self.active_project_cwd.as_deref());
        if cwd.is_empty() {
            return;
        }
        let normalized = normalize_project_path_for_comparison(&cwd);
        if let Some((project_id, _)) = self
            .project_roots
            .iter()
            .find(|(_, root)| *root == normalized)
        {
            // Already a project: jump to it instead of creating a duplicate.
            cx.emit(CommandPaletteEvent::Run(PaletteAction::OpenProject(
                project_id.clone(),
            )));
            self.dismiss(window, cx);
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let project_id = ProjectId(fresh_id("vitre-project"));
        let command = ClientOrchestrationCommand::ProjectCreateCommand(ProjectCreateCommand {
            additional_roots: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            create_workspace_root_if_missing: Some(Some(true)),
            created_at: tnes(now_iso()),
            default_model_selection: Some(Some(self.default_model_selection.clone())),
            project_id: project_id.clone(),
            title: tnes(infer_project_title_from_path(&cwd)),
            r#type: Default::default(),
            workspace_root: tnes(cwd),
        });
        cx.spawn_in(window, async move |this, cx| {
            match client.dispatch(&command).await {
                Ok(_) => {
                    let _ = this.update_in(cx, |this, window, cx| {
                        cx.emit(CommandPaletteEvent::Run(PaletteAction::ProjectCreated(
                            project_id,
                        )));
                        this.dismiss(window, cx);
                    });
                }
                Err(error) => {
                    let _ = this.update_in(cx, |_, window, cx| {
                        error_toast("Failed to add project", format!("{error:?}"), window, cx);
                    });
                }
            }
        })
        .detach();
    }

    /// The clone repository step's Enter: a raw URL goes straight to the
    /// destination step; a provider input is looked up first.
    fn submit_clone_repository(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AddProjectStage::CloneRepository { source }) = self.flow.clone() else {
            return;
        };
        let input = self.query.trim().to_owned();
        if input.is_empty() || self.looking_up {
            return;
        }
        let Some(provider) = source.provider_kind() else {
            self.flow = Some(AddProjectStage::CloneDestination {
                source,
                repository_input: input.clone(),
                repository: None,
                remote_url: input,
            });
            let initial = self.initial_browse_query();
            self.set_stage_query(&initial, window, cx);
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        self.looking_up = true;
        cx.notify();
        let payload = SourceControlRepositoryLookupInput {
            cwd: None,
            provider,
            repository: TrimmedNonEmptyString(input.clone()),
        };
        cx.spawn_in(window, async move |this, cx| {
            let result = client.call::<SourceControlLookupRepository>(&payload).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.looking_up = false;
                match result {
                    Ok(info) => {
                        let remote_url = info.ssh_url.0.clone();
                        this.flow = Some(AddProjectStage::CloneDestination {
                            source,
                            repository_input: input,
                            repository: Some(info),
                            remote_url,
                        });
                        let initial = this.initial_browse_query();
                        this.set_stage_query(&initial, window, cx);
                    }
                    Err(error) => {
                        error_toast("Repository lookup failed", format!("{error:?}"), window, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The destination step's Enter: validate like add-local (with "Clone
    /// failed" toasts), clone, then feed the checked-out directory through
    /// [`Self::handle_add_project`].
    fn submit_clone_destination(
        &mut self,
        raw: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(AddProjectStage::CloneDestination { remote_url, .. }) = self.flow.clone() else {
            return;
        };
        if self.cloning || raw.trim().is_empty() {
            return;
        }
        if is_unsupported_windows_project_path(&raw, self.windows_platform) {
            error_toast(
                "Clone failed",
                "Windows-style paths are only supported on Windows.",
                window,
                cx,
            );
            return;
        }
        if is_explicit_relative_path(raw.trim()) && self.active_project_cwd.is_none() {
            error_toast(
                "Clone failed",
                "Relative paths require an active project.",
                window,
                cx,
            );
            return;
        }
        let destination =
            resolve_project_path_for_dispatch(&raw, self.active_project_cwd.as_deref());
        if destination.is_empty() {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.cloning = true;
        cx.notify();
        let payload = SourceControlCloneRepositoryInput {
            destination_path: TrimmedNonEmptyString(destination),
            protocol: None,
            provider: None,
            remote_url: Some(Some(TrimmedNonEmptyString(remote_url))),
            repository: None,
        };
        cx.spawn_in(window, async move |this, cx| {
            let result = client.call::<SourceControlCloneRepository>(&payload).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.cloning = false;
                match result {
                    Ok(cloned) => this.handle_add_project(cloned.cwd.0.clone(), window, cx),
                    Err(error) => {
                        error_toast("Clone failed", format!("{error:?}"), window, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The footer's "Open in Finder" button: the native directory picker in
    /// place of Electron's `dialogs.pickFolder`. A cancelled or failed pick is
    /// a no-op, exactly as there.
    fn open_folder_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picking {
            return;
        }
        self.picking = true;
        cx.notify();
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let picked = paths.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.picking = false;
                if let Ok(Ok(Some(paths))) = picked
                    && let Some(path) = paths.first()
                {
                    this.handle_add_project(path.to_string_lossy().into_owned(), window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // MARK: Render

    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let groups = self.visible_groups();
        self.displayed = groups.clone();

        let in_submenu = !self.stack.is_empty() || self.flow.is_some();
        let browsing = self.is_browsing();

        let placeholder: SharedString = match &self.flow {
            Some(AddProjectStage::CloneRepository { source }) => {
                source.repository_placeholder().into()
            }
            _ => match (in_submenu, browsing) {
                (false, false) => "Search commands, projects, and threads...".into(),
                (false, true) => "Enter project path (e.g. ~/projects/my-app)".into(),
                (true, false) => "Search...".into(),
                (true, true) => "Enter path (e.g. ~/projects/my-app)".into(),
            },
        };
        let empty_copy = self.empty_copy(self.state.read(cx).selected_index());

        // The destination step's "Repository" context block.
        let repository_header = match &self.flow {
            Some(AddProjectStage::CloneDestination {
                source,
                repository_input,
                repository,
                remote_url,
            }) => Some((
                source.icon(),
                SharedString::from(
                    repository
                        .as_ref()
                        .map(|info| info.name_with_owner.0.clone())
                        .unwrap_or_else(|| repository_input.clone()),
                ),
                SharedString::from(
                    repository
                        .as_ref()
                        .map(|info| info.url.0.clone())
                        .unwrap_or_else(|| remote_url.clone()),
                ),
            )),
            _ => None,
        };

        let query_owner = cx.weak_entity();
        let confirm_owner = cx.weak_entity();
        let back_owner = cx.weak_entity();
        let suffix_owner = cx.weak_entity();
        let footer_owner = cx.weak_entity();

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
                if empty_copy.is_empty() {
                    return div().into_any_element();
                }
                v_flex()
                    .w_full()
                    .items_center()
                    .py_10()
                    .px_2()
                    .text_size(px(12.))
                    .text_color(cx.theme().muted_foreground)
                    .child(empty_copy.clone())
                    .into_any_element()
            })
            .suffix(move |state, _, cx| suffix_element(&suffix_owner, state, cx))
            .footer(move |state, _, cx| footer_element(&footer_owner, state, cx))
            .on_query(move |query, window, cx| {
                let query = query.to_string();
                _ = query_owner.update(cx, |this, cx| this.handle_query(query, window, cx));
            })
            .on_confirm(move |index, window, cx| {
                _ = confirm_owner.update(cx, |this, cx| this.confirm(index, window, cx));
            });

        // Electron's start addon: a back button in any submenu, a static
        // FolderPlus while browsing from the root, the search glyph otherwise.
        command = if in_submenu {
            command.prefix(move |_, _, _| {
                Button::new("palette-back")
                    .icon(Icon::new(IconName::ArrowLeft).size_4())
                    .ghost()
                    .xsmall()
                    .on_click({
                        let owner = back_owner.clone();
                        move |_, window, cx| {
                            _ = owner.update(cx, |this, cx| this.back(window, cx));
                        }
                    })
            })
        } else if browsing {
            command.prefix(|_, _, cx| {
                Icon::new(IconName::FolderPlus).text_color(cx.theme().muted_foreground)
            })
        } else {
            command
        };

        if let Some((icon, title, subtitle)) = repository_header {
            command = command.header(move |_, _, cx| {
                let muted = cx.theme().muted_foreground;
                v_flex()
                    .px_4()
                    .pt_3()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(muted)
                            .child("Repository"),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Icon::new(icon.clone()).size_4().text_color(muted))
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .child(div().truncate().text_size(px(13.)).child(title.clone()))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(11.))
                                            .text_color(muted)
                                            .child(subtitle.clone()),
                                    ),
                            ),
                    )
            });
        }

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
            // Electron pops a sub-view on Backspace at an empty query and owns
            // Enter in the flow's submit modes. The capture phase is the only
            // place to see either: the query field and the list would
            // otherwise swallow the keys on their way down.
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                if key == "enter" {
                    if matches!(this.flow, Some(AddProjectStage::CloneRepository { .. })) {
                        this.submit_clone_repository(window, cx);
                        cx.stop_propagation();
                        return;
                    }
                    if this.can_submit_browse_path() {
                        let modifiers = event.keystroke.modifiers;
                        let primary = if cfg!(target_os = "macos") {
                            modifiers.platform && !modifiers.control
                        } else {
                            modifiers.control && !modifiers.platform
                        };
                        let selected = this.state.read(cx).selected_index();
                        if !this.highlighted_browse_row(selected) || primary {
                            this.submit_browse_path(window, cx);
                            cx.stop_propagation();
                        }
                        // A highlighted row and plain Enter fall through to
                        // the list's own confirm, which descends into it.
                    }
                    return;
                }
                if key == "backspace" && this.query.is_empty() {
                    if this.flow.is_some() {
                        this.pop_flow_stage(window, cx);
                        cx.stop_propagation();
                    } else if !this.stack.is_empty() {
                        this.pop_view(window, cx);
                        cx.stop_propagation();
                    }
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

/// One remote source row for the Sources view, gated by provider readiness.
fn source_item(source: RemoteSource, readiness: SourceReadiness) -> PaletteItem {
    let label = source.label();
    let (title, description) = match source {
        RemoteSource::Url => ("Git URL".to_owned(), "Clone from a remote URL".to_owned()),
        _ => (
            format!("{label} repository"),
            format!("Clone {label} {}", source.path_hint()),
        ),
    };
    let mut item = PaletteItem::new(
        title,
        source.icon(),
        &["clone", "remote", "repository", "repo", "git", label],
    )
    .description(description)
    .run(ItemRun::SourceRemote(source));
    if !readiness.ready {
        item.disabled = true;
        item.run = ItemRun::Inert;
        item.terms.push("setup required".into());
        item.badge = Some(SharedString::from(readiness.hint.unwrap_or_else(|| {
            "Open Settings -> Source Control to configure this provider.".to_owned()
        })));
    }
    item
}

/// Electron's row: icon, then title over an optional description, then the
/// badge, the timestamp, the shortcut chip or the submenu chevron.
fn command_item(item: &PaletteItem) -> CommandItem {
    let disabled = item.disabled;
    let item = item.clone();
    CommandItem::new().disabled(disabled).child(move |_, cx| {
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
            .children(item.badge.clone().map(|hint| {
                div().flex_shrink_0().child(
                    Button::new(SharedString::from(format!("setup-{}", item.title)))
                        .outline()
                        .xsmall()
                        .label("Setup Required")
                        .on_click(move |_, window, cx| {
                            // Electron opens the source-control settings page,
                            // which Vitre does not have yet; surface the
                            // readiness hint instead (module note).
                            window.push_notification(Notification::info(hint.clone()), cx);
                        }),
                )
            }))
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
            .when(matches!(item.run, ItemRun::Submenu(_)), |this| {
                this.child(
                    Icon::new(IconName::ChevronRight)
                        .size_4()
                        .flex_shrink_0()
                        .text_color(muted),
                )
            })
    })
}

fn error_toast(
    title: &'static str,
    message: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) {
    window.push_notification(Notification::error(message.into()).title(title), cx);
}

/// The client platform's file-manager name, as Electron derives it from
/// `navigator.platform`.
fn file_manager_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Finder"
    } else if cfg!(target_os = "windows") {
        "Explorer"
    } else {
        "Files"
    }
}

/// The submit-the-typed-path chord shown while a browse row is highlighted.
fn primary_enter_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘ Enter"
    } else {
        "Ctrl Enter"
    }
}

/// The inline submit button at the input's end — Electron's `submitActionLabel`
/// machinery: Continue/Lookup/Working on the repository step, Add/Create & Add
/// while browsing, Clone/Create & Clone/Cloning on the destination step.
///
/// Runs inside the [`CommandState`] render, so the state arrives by reference
/// — reading its entity here would be a reentrant borrow.
fn suffix_element(
    owner: &WeakEntity<CommandPalette>,
    state: &CommandState,
    cx: &mut App,
) -> AnyElement {
    let Some(palette) = owner.upgrade() else {
        return div().into_any_element();
    };
    let selected = state.selected_index();
    let palette = palette.read(cx);
    let muted = cx.theme().muted_foreground;

    if let Some(AddProjectStage::CloneRepository { source }) = &palette.flow {
        let label = if palette.looking_up {
            "Working"
        } else if source.provider_kind().is_some() {
            "Lookup"
        } else {
            "Continue"
        };
        let disabled = palette.query.trim().is_empty() || palette.looking_up;
        let owner = owner.clone();
        return h_flex()
            .gap_1p5()
            .items_center()
            .mr_1()
            .child(
                Button::new("palette-submit")
                    .outline()
                    .xsmall()
                    .label(label)
                    .disabled(disabled)
                    .on_click(move |_, window, cx| {
                        _ = owner.update(cx, |this, cx| this.submit_clone_repository(window, cx));
                    }),
            )
            .child(div().text_size(px(10.)).text_color(muted).child("Enter"))
            .into_any_element();
    }

    if !palette.can_submit_browse_path() {
        return div().into_any_element();
    }
    let destination = matches!(palette.flow, Some(AddProjectStage::CloneDestination { .. }));
    let will_create = palette.will_create_project_path(selected);
    let label = match (destination, palette.cloning, will_create) {
        (true, true, _) => "Cloning",
        (true, false, true) => "Create & Clone",
        (true, false, false) => "Clone",
        (false, _, true) => "Create & Add",
        (false, _, false) => "Add",
    };
    let chord = if palette.highlighted_browse_row(selected) {
        primary_enter_label()
    } else {
        "Enter"
    };
    let disabled = palette.relative_path_needs_active_project() || (destination && palette.cloning);
    let owner = owner.clone();
    h_flex()
        .gap_1p5()
        .items_center()
        .mr_1()
        .child(
            Button::new("palette-submit")
                .outline()
                .xsmall()
                .label(label)
                .disabled(disabled)
                .on_click(move |_, window, cx| {
                    _ = owner.update(cx, |this, cx| this.submit_browse_path(window, cx));
                }),
        )
        .child(div().text_size(px(10.)).text_color(muted).child(chord))
        .into_any_element()
}

/// Electron's `CommandFooter`: the key hints (stage-aware) and, while
/// browsing, the "Open in Finder" affordance on the right.
///
/// Runs inside the [`CommandState`] render — same reentrancy rule as
/// [`suffix_element`].
fn footer_element(
    owner: &WeakEntity<CommandPalette>,
    state: &CommandState,
    cx: &mut App,
) -> AnyElement {
    let Some(palette) = owner.upgrade() else {
        return div().into_any_element();
    };
    let selected = state.selected_index();
    let palette = palette.read(cx);
    let in_submenu = !palette.stack.is_empty() || palette.flow.is_some();
    let repo_label = match &palette.flow {
        Some(AddProjectStage::CloneRepository { source }) => {
            Some(if source.provider_kind().is_some() {
                "Lookup"
            } else {
                "Continue"
            })
        }
        _ => None,
    };
    // "Enter Select" only reads true when Enter would actually select a row.
    let enter_select = repo_label.is_none()
        && (!palette.can_submit_browse_path() || palette.highlighted_browse_row(selected));
    let show_finder = palette.is_browsing() && palette.client.is_some();
    let picking = palette.picking;
    let owner = owner.clone();

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
        .map(|this| match repo_label {
            Some(label) => this.child(hint("Enter", label, cx)),
            None => this.when(enter_select, |this| this.child(hint("Enter", "Select", cx))),
        })
        .when(in_submenu, |this| this.child(hint("Backspace", "Back", cx)))
        .child(hint("Esc", "Close", cx))
        .when(show_finder, |this| {
            this.child(
                div().ml_auto().child(
                    Button::new("open-in-file-manager")
                        .ghost()
                        .xsmall()
                        .label(format!("Open in {}", file_manager_name()))
                        .disabled(picking)
                        .on_click(move |_, window, cx| {
                            _ = owner.update(cx, |this, cx| this.open_folder_picker(window, cx));
                        }),
                ),
            )
        })
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
    items.push(
        PaletteItem::new(
            "Add project",
            IconName::FolderPlus,
            &[
                "add project",
                "folder",
                "directory",
                "browse",
                "clone",
                "remote",
                "repository",
                "repo",
                "git",
                "github",
                "gitlab",
                "bitbucket",
                "azure",
                "devops",
                "url",
                "environment",
            ],
        )
        .run(ItemRun::StartAddProject),
    );

    // Electron gates the workspace block on an active thread, because every
    // command in it addresses the open workspace.
    if context.active_thread.is_some() {
        for (title, icon, terms, action) in [
            (
                "Search and replace in files",
                IconName::Search,
                vec!["search", "replace", "find in files"],
                PaletteAction::WorkspaceSearch,
            ),
            (
                "Explore knowledge graph",
                IconName::Network,
                vec!["graph", "symbols", "relationships"],
                PaletteAction::KnowledgeGraph,
            ),
            (
                "Build knowledge graph",
                IconName::Network,
                vec!["build graph", "index code"],
                PaletteAction::BuildGraph,
            ),
            (
                "Attach workspace folders…",
                IconName::FolderPlus,
                vec!["workspace roots", "attach folder", "multi root"],
                PaletteAction::WorkspaceFolders,
            ),
        ] {
            items.push(PaletteItem::new(title, icon, &terms).action(action));
        }
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
            .shortcut(shortcut_for(&RightPanelToggle, window))
            .action(PaletteAction::ToggleFilesPanel),
        );
        items.push(
            PaletteItem::new(
                "Toggle terminal",
                IconName::SquareTerminal,
                &["terminal", "shell", "console", "toggle", "drawer"],
            )
            .shortcut(shortcut_for(&TerminalToggle, window))
            .action(PaletteAction::ToggleTerminal),
        );
        items.push(
            PaletteItem::new(
                "Toggle vim mode",
                IconName::SquareTerminal,
                &[
                    "vim",
                    "modal",
                    "normal mode",
                    "editor",
                    "toggle",
                    "keybindings",
                ],
            )
            .action(PaletteAction::ToggleVimMode),
        );
        items.push(
            PaletteItem::new(
                "Open settings",
                IconName::Settings,
                &["settings", "preferences", "configuration", "keybindings"],
            )
            .action(PaletteAction::OpenSettings),
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

    #[test]
    fn unready_sources_are_disabled_and_badged() {
        let ready = source_item(
            RemoteSource::Github,
            SourceReadiness {
                ready: true,
                hint: None,
            },
        );
        assert!(!ready.disabled);
        assert!(ready.badge.is_none());
        assert!(matches!(
            ready.run,
            ItemRun::SourceRemote(RemoteSource::Github)
        ));

        let unready = source_item(
            RemoteSource::Github,
            SourceReadiness {
                ready: false,
                hint: Some("Run gh auth login.".into()),
            },
        );
        assert!(unready.disabled);
        assert!(matches!(unready.run, ItemRun::Inert));
        assert_eq!(unready.badge.as_deref(), Some("Run gh auth login."));
        assert!(unready.terms.iter().any(|term| term == "setup required"));
    }
}
