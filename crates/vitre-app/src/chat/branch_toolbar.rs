//! The branch toolbar strip above the composer: Electron's
//! `BranchToolbar.tsx` — the static workspace label, the PR pill, and the
//! branch combobox (`vcs.listRefs` pagination + search, badges, switch /
//! create with an optimistic trigger label).
//!
//! Vitre has no draft threads, so the pieces that only exist pre-send are
//! deliberately absent (matrix-noted): the env-mode *selector* (every Vitre
//! thread is locked, so the label renders Electron's locked static variant),
//! the environment selector (one primary local environment — Electron hides
//! the indicator in that case too), the worktree-base mode + "Start from
//! origin" switch, and "Previous worktree" restore. Also deferred: the
//! checkout-PR row (needs the PR-thread dialog) and the copy-branch-name
//! context menu.
//!
//! Refs enumerate ONLY while the picker is open (a 5s revalidate loop runs
//! while open and dies on close); the closed trigger label comes from the
//! `subscribeVcsStatus` fold in [`super::git_actions`].

use std::time::Duration;

use gpui::{
    AnyElement, App, Context, Entity, SharedString, Subscription, Task, WeakEntity, Window, div,
    prelude::*, px, rgb, rgba,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, IndexPath, Sizable as _, StyledExt as _,
    combobox::{Combobox, ComboboxEvent, ComboboxState},
    h_flex,
    searchable_list::{SearchableListDelegate, SearchableListItem},
};
use vitre_contracts::methods::{VcsCreateRef, VcsListRefs, VcsSwitchRef};
use vitre_contracts::{
    ClientOrchestrationCommand, CommandId, NonNegativeInt, VcsCreateRefInput, VcsListRefsInput,
    VcsListRefsResult, VcsRef, VcsStatusRemoteResultPrState, VcsSwitchRefInput,
};
use vitre_state::branch_toolbar::BranchSelectionTarget;
use vitre_state::branch_toolbar::{
    MergedRefs, REF_LIST_LIMIT, branch_trigger_label, create_item_label, create_item_query,
    locked_workspace_label, merge_ref_pages, pr_state_label, refs_status_text,
    resolve_branch_selection_target, resolve_branch_toolbar_value, resolve_ref_badge,
    should_include_branch_picker_item,
};
use vitre_state::source_control::{
    derive_local_branch_name_from_remote_ref, resolve_thread_pr, status_terminology,
};

use crate::assets::VitreIcon;

use super::{ChatApp, fresh_id, now_iso, tnes};

use gpui_component::WindowExt as _;
use gpui_component::notification::Notification;

/// `VCS_REFS_REVALIDATE_INTERVAL` — subscribed pages refetch on this cadence
/// while the picker is open.
const REVALIDATE_INTERVAL: Duration = Duration::from_secs(5);

/// The create row's value prefix (`CREATE_NEW_BRANCH_SELECT_VALUE`).
const CREATE_VALUE_PREFIX: &str = "__create_new_branch__:";

/// One row of the branch picker.
#[derive(Clone)]
pub(super) enum PickerRow {
    Ref {
        name: SharedString,
        badge: Option<&'static str>,
        value: String,
    },
    Create {
        label: SharedString,
        value: String,
    },
}

impl SearchableListItem for PickerRow {
    type Value = String;

    fn title(&self) -> SharedString {
        match self {
            Self::Ref { name, .. } => name.clone(),
            Self::Create { label, .. } => label.clone(),
        }
    }

    fn value(&self) -> &String {
        match self {
            Self::Ref { value, .. } | Self::Create { value, .. } => value,
        }
    }

    /// Filtering happens upstream (server `query` + the client filter in
    /// `rebuild_branch_picker_rows`), never in the list itself.
    fn matches(&self, _query: &str) -> bool {
        true
    }
}

/// Data source handed to the fork's `Combobox`. Rows are precomputed by
/// [`ChatApp::rebuild_branch_picker_rows`]; search and pagination bounce back
/// to the app (deferred a tick — the list entity is locked while these hooks
/// run, so the app must not touch it synchronously).
pub(super) struct BranchPickerDelegate {
    chat: WeakEntity<ChatApp>,
    rows: Vec<PickerRow>,
    has_more: bool,
}

impl BranchPickerDelegate {
    fn empty(chat: WeakEntity<ChatApp>) -> Self {
        Self {
            chat,
            rows: Vec::new(),
            has_more: false,
        }
    }
}

impl SearchableListDelegate for BranchPickerDelegate {
    type Item = PickerRow;

    fn items_count(&self, _section: usize) -> usize {
        self.rows.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&PickerRow> {
        self.rows.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        Self::Item: SearchableListItem<Value = V>,
        V: PartialEq,
    {
        self.rows
            .iter()
            .position(|row| row.value() == value)
            .map(IndexPath::new)
    }

    fn perform_search(&mut self, query: &str, _window: &mut Window, cx: &mut App) -> Task<()> {
        let query = query.to_string();
        let chat = self.chat.clone();
        cx.spawn(async move |cx| {
            let _ = chat.update(cx, |app, cx| app.branch_query_changed(query, cx));
        })
        .detach();
        Task::ready(())
    }

    fn render_item(
        &self,
        _ix: IndexPath,
        item: &PickerRow,
        _checked: bool,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        // Custom rows: Electron renders `ComboboxItem hideIndicator` (no
        // check icon), name + right-aligned badge.
        let row = match item {
            PickerRow::Ref { name, badge, .. } => h_flex()
                .w_full()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .child(name.clone()),
                )
                .when_some(*badge, |this, badge| {
                    this.child(
                        div()
                            .text_size(px(10.))
                            .text_color(cx.theme().muted_foreground.opacity(0.45))
                            .child(badge),
                    )
                }),
            PickerRow::Create { label, .. } => h_flex().w_full().child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .child(label.clone()),
            ),
        };
        Some(row.into_any_element())
    }

    fn has_more(&self, _cx: &App) -> bool {
        self.has_more
    }

    /// Electron loads the next page within 96px of the bottom (~4 rows).
    fn load_more_threshold(&self) -> usize {
        4
    }

    fn load_more(&mut self, _window: &mut Window, cx: &mut App) {
        let chat = self.chat.clone();
        cx.spawn(async move |cx| {
            let _ = chat.update(cx, |app, cx| app.branch_load_next(cx));
        })
        .detach();
    }
}

/// All branch-toolbar state on the chat view.
pub(super) struct BranchToolbarState {
    pub(super) picker: Entity<ComboboxState<BranchPickerDelegate>>,
    _subscriptions: Vec<Subscription>,
    /// Last observed popup state — the open/close transition detector.
    was_open: bool,
    /// The `branchCwd` the pages below belong to (worktree ?? project root).
    cwd: Option<String>,
    /// The live (non-deferred) search text.
    query: String,
    /// Requested page cursors, in order: `[None, Some(c1), ...]`.
    cursors: Vec<Option<i64>>,
    /// Results parallel to `cursors` (None until a page lands).
    pages: Vec<Option<VcsListRefsResult>>,
    /// The fold over `pages` — kept for display while a refetch is in flight
    /// (SWR: stale refs stay visible under a new query's deferred filter).
    merged: MergedRefs,
    /// Any page ever stored for this cwd.
    has_loaded: bool,
    /// First fetch in flight with nothing to show yet.
    initial_pending: bool,
    fetching_next: bool,
    /// Invalidates in-flight fetches when the cwd or query changes.
    fetch_seq: u64,
    _revalidate_task: Option<Task<()>>,
    /// Trigger-label overlay during checkout (`useOptimistic`), rolled back
    /// on failure and dropped once the status stream catches up.
    optimistic_branch: Option<String>,
    /// A switch/create is in flight (`isBranchActionPending`).
    action_pending: bool,
}

impl BranchToolbarState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<ChatApp>) -> Self {
        let chat = cx.entity().downgrade();
        let picker = cx.new(|cx| {
            ComboboxState::new(BranchPickerDelegate::empty(chat), vec![], window, cx)
                .searchable(true)
        });
        let subscriptions = vec![
            cx.subscribe_in(&picker, window, ChatApp::on_branch_picker_event),
            // Open/close transitions: `set_open` always notifies the picker,
            // so observing it sees every toggle path (click, Escape, blur).
            cx.observe(&picker, |app: &mut ChatApp, picker, cx| {
                let open = picker.read(cx).is_open();
                if open != app.branch.was_open {
                    app.branch.was_open = open;
                    if open {
                        app.branch_picker_opened(cx);
                    } else {
                        app.branch_picker_closed(cx);
                    }
                }
                cx.notify();
            }),
        ];
        Self {
            picker,
            _subscriptions: subscriptions,
            was_open: false,
            cwd: None,
            query: String::new(),
            cursors: Vec::new(),
            pages: Vec::new(),
            merged: MergedRefs::default(),
            has_loaded: false,
            initial_pending: false,
            fetching_next: false,
            fetch_seq: 0,
            _revalidate_task: None,
            optimistic_branch: None,
            action_pending: false,
        }
    }

    fn reset_data(&mut self) {
        self.fetch_seq += 1;
        self.query.clear();
        self.cursors.clear();
        self.pages.clear();
        self.merged = MergedRefs::default();
        self.has_loaded = false;
        self.initial_pending = false;
        self.fetching_next = false;
        self._revalidate_task = None;
        self.optimistic_branch = None;
    }
}

impl ChatApp {
    // ------------------------------------------------------------- targeting

    /// (Re)target the toolbar at the open thread's git cwd. Cheap when
    /// nothing changed — safe to call from render.
    pub(super) fn sync_branch_toolbar(&mut self, cx: &mut Context<Self>) {
        let desired = self
            .thread
            .as_ref()
            .is_some()
            .then(|| self.open_project_root())
            .flatten();
        if desired == self.branch.cwd {
            return;
        }
        self.branch.cwd = desired;
        self.branch.reset_data();
        // Clearing rows touches the picker's list entity, which may be
        // mid-render right now — defer a tick.
        let seq = self.branch.fetch_seq;
        cx.spawn(async move |this, cx| {
            let _ = this.update_in(cx, |app, window, cx| {
                if app.branch.fetch_seq == seq {
                    app.rebuild_branch_picker_rows(window, cx);
                }
            });
        })
        .detach();
    }

    /// Workspace root of the open thread's project (NOT the worktree) — the
    /// badge and checkout-target logic compare against this.
    fn thread_project_root(&self) -> Option<String> {
        let open = self.thread.as_ref()?;
        let project_id = self.shell_thread(&open.id)?.project_id.clone();
        self.project_root(&project_id)
    }

    // ---------------------------------------------------------- open / close

    fn branch_picker_opened(&mut self, cx: &mut Context<Self>) {
        self.branch_refresh_pages(cx);
        // The 5s revalidate loop, alive only while open: refetch every
        // subscribed cursor so ref moves/creations show up.
        let task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(REVALIDATE_INTERVAL).await;
                let live = this
                    .update(cx, |app, cx| {
                        if !app.branch.was_open {
                            return false;
                        }
                        for (index, cursor) in app.branch.cursors.clone().into_iter().enumerate() {
                            app.branch_fetch(index, cursor, cx);
                        }
                        true
                    })
                    .unwrap_or(false);
                if !live {
                    return;
                }
            }
        });
        self.branch._revalidate_task = Some(task);
    }

    fn branch_picker_closed(&mut self, cx: &mut Context<Self>) {
        self.branch._revalidate_task = None;
        // Closing clears the search query (Electron). `set_query` re-enters
        // `perform_search`, which early-outs on the unchanged value below.
        self.branch.query.clear();
        cx.spawn(async move |this, cx| {
            let _ = this.update_in(cx, |app, window, cx| {
                let picker = app.branch.picker.clone();
                picker.update(cx, |state, cx| state.set_query("", window, cx));
            });
        })
        .detach();
    }

    // ------------------------------------------------------------- fetching

    fn branch_query_changed(&mut self, query: String, cx: &mut Context<Self>) {
        if query == self.branch.query {
            return;
        }
        self.branch.query = query;
        // The stale merged refs stay visible, filtered by the new query
        // (Electron's deferred-query window), while fresh pages load.
        if self.branch.was_open {
            self.branch_refresh_pages(cx);
        }
        cx.spawn(async move |this, cx| {
            let _ = this.update_in(cx, |app, window, cx| {
                app.rebuild_branch_picker_rows(window, cx);
            });
        })
        .detach();
    }

    /// `refresh()`: reset to a single first-page cursor and refetch.
    fn branch_refresh_pages(&mut self, cx: &mut Context<Self>) {
        self.branch.fetch_seq += 1;
        self.branch.cursors = vec![None];
        self.branch.pages = vec![None];
        self.branch.fetching_next = false;
        self.branch.initial_pending = !self.branch.has_loaded;
        self.branch_fetch(0, None, cx);
    }

    /// `loadNext()`: append the next cursor if not already subscribed.
    fn branch_load_next(&mut self, cx: &mut Context<Self>) {
        if self.branch.fetching_next || self.branch.action_pending {
            return;
        }
        let Some(cursor) = self.branch.merged.next_cursor else {
            return;
        };
        if self.branch.cursors.contains(&Some(cursor)) {
            return;
        }
        self.branch.fetching_next = true;
        self.branch.cursors.push(Some(cursor));
        self.branch.pages.push(None);
        let index = self.branch.cursors.len() - 1;
        self.branch_fetch(index, Some(cursor), cx);
    }

    fn branch_fetch(&mut self, index: usize, cursor: Option<i64>, cx: &mut Context<Self>) {
        let (Some(client), Some(cwd)) = (self.client.clone(), self.branch.cwd.clone()) else {
            return;
        };
        let seq = self.branch.fetch_seq;
        let query = self.branch.query.trim().to_string();
        cx.spawn(async move |this, cx| {
            let payload = VcsListRefsInput {
                // Double-option: only pages ≥ 2 send a cursor at all.
                cursor: cursor.map(|cursor| Some(NonNegativeInt(cursor))),
                cwd: tnes(&cwd),
                include_matching_remote_refs: None,
                limit: Some(Some(REF_LIST_LIMIT)),
                query: (!query.is_empty()).then(|| Some(tnes(&query))),
                ref_kind: None,
            };
            let result = client.call::<VcsListRefs>(&payload).await;
            let _ = this.update_in(cx, |app, window, cx| {
                if app.branch.fetch_seq != seq {
                    return;
                }
                if index == 0 {
                    app.branch.initial_pending = false;
                } else {
                    app.branch.fetching_next = false;
                }
                if let Ok(page) = result {
                    if let Some(slot) = app.branch.pages.get_mut(index) {
                        *slot = Some(page);
                    }
                    app.branch.has_loaded = true;
                    let filled: Vec<VcsListRefsResult> =
                        app.branch.pages.iter().flatten().cloned().collect();
                    app.branch.merged = merge_ref_pages(&filled);
                }
                // Errors keep the stale merged view; the revalidate loop or
                // the next open retries.
                app.rebuild_branch_picker_rows(window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Recompute the picker's rows from the merged pages + the client-side
    /// query filter, and push them into the combobox delegate.
    fn rebuild_branch_picker_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let project_cwd = self.thread_project_root().unwrap_or_default();
        let query = self.branch.query.clone();
        let mut rows: Vec<PickerRow> = self
            .branch
            .merged
            .refs
            .iter()
            .filter(|git_ref| should_include_branch_picker_item(&query, &git_ref.name.0))
            .map(|git_ref| PickerRow::Ref {
                name: SharedString::from(git_ref.name.0.clone()),
                badge: resolve_ref_badge(git_ref, &project_cwd).map(|badge| badge.label()),
                value: git_ref.name.0.clone(),
            })
            .collect();
        if let Some(create) = create_item_query(false, &query, &self.branch.merged.refs) {
            rows.push(PickerRow::Create {
                label: SharedString::from(create_item_label(&create)),
                value: format!("{CREATE_VALUE_PREFIX}{create}"),
            });
        }
        let delegate = BranchPickerDelegate {
            chat: cx.entity().downgrade(),
            rows,
            has_more: self.branch.merged.next_cursor.is_some(),
        };
        let picker = self.branch.picker.clone();
        picker.update(cx, |state, cx| state.set_items(delegate, window, cx));
    }

    // ------------------------------------------------------------- selection

    fn on_branch_picker_event(
        &mut self,
        _picker: &Entity<ComboboxState<BranchPickerDelegate>>,
        event: &ComboboxEvent<BranchPickerDelegate>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let ComboboxEvent::Change(values) = event
            && let Some(value) = values.last().cloned()
        {
            self.branch_picker_select(value, window, cx);
        }
    }

    fn branch_picker_select(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(name) = value.strip_prefix(CREATE_VALUE_PREFIX) {
            self.branch_create_ref(name.to_string(), window, cx);
            return;
        }
        let Some(picked) = self
            .branch
            .merged
            .refs
            .iter()
            .find(|git_ref| git_ref.name.0 == value)
            .cloned()
        else {
            return;
        };
        self.branch_select_ref(picked, window, cx);
    }

    /// `selectBranch(ref)` minus the draft-only worktree-base path.
    fn branch_select_ref(&mut self, picked: VcsRef, _window: &mut Window, cx: &mut Context<Self>) {
        if self.branch.action_pending {
            return;
        }
        let (Some(client), Some(project_cwd)) = (self.client.clone(), self.thread_project_root())
        else {
            return;
        };
        let active_worktree = self
            .thread
            .as_ref()
            .and_then(|open| self.shell_thread(&open.id))
            .and_then(|thread| thread.worktree_path.as_ref().map(|path| path.0.clone()));
        let target =
            resolve_branch_selection_target(&picked, &project_cwd, active_worktree.as_deref());
        match target {
            BranchSelectionTarget::Reuse { next_worktree_path } => {
                // The ref already lives in a worktree: adopt it, no git call.
                self.set_thread_branch(picked.name.0.clone(), next_worktree_path, cx);
                self.git_refresh_status(cx);
            }
            BranchSelectionTarget::Checkout {
                checkout_cwd,
                next_worktree_path,
            } => {
                let is_remote = picked.is_remote.flatten().unwrap_or(false);
                let derived = if is_remote {
                    derive_local_branch_name_from_remote_ref(&picked.name.0)
                } else {
                    picked.name.0.clone()
                };
                let previous_optimistic = self.branch.optimistic_branch.clone();
                self.branch.optimistic_branch = Some(derived.clone());
                self.branch.action_pending = true;
                cx.notify();
                let ref_name = picked.name.0.clone();
                cx.spawn(async move |this, cx| {
                    let payload = VcsSwitchRefInput {
                        cwd: tnes(&checkout_cwd),
                        ref_name: tnes(&ref_name),
                    };
                    let result = client.call::<VcsSwitchRef>(&payload).await;
                    let _ = this.update_in(cx, |app, window, cx| {
                        app.branch.action_pending = false;
                        match result {
                            Ok(result) => {
                                // Remote refs adopt the server's actual name.
                                let final_name = result
                                    .ref_name
                                    .map(|name| name.0)
                                    .unwrap_or_else(|| derived.clone());
                                app.branch.optimistic_branch = Some(final_name.clone());
                                app.set_thread_branch(final_name, next_worktree_path, cx);
                            }
                            Err(error) => {
                                // Roll back to the pre-action overlay.
                                app.branch.optimistic_branch = previous_optimistic;
                                window.push_notification(
                                    Notification::error(SharedString::from(format!("{error:?}")))
                                        .title("Failed to switch ref."),
                                    cx,
                                );
                            }
                        }
                        // After the action, refresh both the ref list and the
                        // status query.
                        app.git_refresh_status(cx);
                        if app.branch.was_open {
                            app.branch_refresh_pages(cx);
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
        }
    }

    /// `createRef(name)`: `vcs.createRef {switchRef: true}`.
    fn branch_create_ref(&mut self, name: String, _window: &mut Window, cx: &mut Context<Self>) {
        if self.branch.action_pending {
            return;
        }
        let (Some(client), Some(cwd)) = (self.client.clone(), self.branch.cwd.clone()) else {
            return;
        };
        let active_worktree = self
            .thread
            .as_ref()
            .and_then(|open| self.shell_thread(&open.id))
            .and_then(|thread| thread.worktree_path.as_ref().map(|path| path.0.clone()));
        let previous_optimistic = self.branch.optimistic_branch.clone();
        self.branch.optimistic_branch = Some(name.clone());
        self.branch.action_pending = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let payload = VcsCreateRefInput {
                cwd: tnes(&cwd),
                ref_name: tnes(&name),
                switch_ref: Some(Some(true)),
            };
            let result = client.call::<VcsCreateRef>(&payload).await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.branch.action_pending = false;
                match result {
                    Ok(result) => {
                        // Always use the returned (sanitized) name; the
                        // worktree stays unchanged.
                        let final_name = result.ref_name.0;
                        app.branch.optimistic_branch = Some(final_name.clone());
                        app.set_thread_branch(final_name, active_worktree, cx);
                    }
                    Err(error) => {
                        app.branch.optimistic_branch = previous_optimistic;
                        window.push_notification(
                            Notification::error(SharedString::from(format!("{error:?}")))
                                .title("Failed to create and switch ref."),
                            cx,
                        );
                    }
                }
                app.git_refresh_status(cx);
                if app.branch.was_open {
                    app.branch_refresh_pages(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `setThreadBranch(branch, worktreePath)` for server threads: stop a
    /// live session first when the worktree changes, then update metadata.
    fn set_thread_branch(
        &mut self,
        branch: String,
        worktree_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(open) = self.thread.as_ref() else {
            return;
        };
        let thread_id = open.id.clone();
        let (current_worktree, has_session) = self
            .shell_thread(&thread_id)
            .map(|thread| {
                (
                    thread.worktree_path.as_ref().map(|path| path.0.clone()),
                    thread.session.is_some(),
                )
            })
            .unwrap_or((None, false));
        let stop_first = has_session && worktree_path != current_worktree;
        let stop_command = stop_first.then(|| ClientOrchestrationCommand::ThreadSessionStop {
            command_id: CommandId(fresh_id("vitre-cmd")),
            created_at: tnes(now_iso()),
            thread_id: thread_id.clone(),
            r#type: Default::default(),
        });
        let update = ClientOrchestrationCommand::ThreadMetaUpdate {
            additional_roots: None,
            branch: Some(Some(Some(tnes(&branch)))),
            command_id: CommandId(fresh_id("vitre-cmd")),
            expected_branch: None,
            model_selection: None,
            thread_id,
            title: None,
            r#type: Default::default(),
            worktree_path: Some(Some(worktree_path.as_deref().map(tnes))),
        };
        cx.spawn(async move |_, _| {
            if let Some(stop) = stop_command {
                // Fire-and-forget, before the metadata update (Electron).
                let _ = client.dispatch(&stop).await;
            }
            let _ = client.dispatch(&update).await;
        })
        .detach();
    }

    // -------------------------------------------------------------- rendering

    /// The strip above the composer. `None` when no thread is open.
    pub(super) fn render_branch_toolbar(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.sync_branch_toolbar(cx);
        self.branch.cwd.as_ref()?;
        let open = self.thread.as_ref()?;
        let shell_thread = self.shell_thread(&open.id);
        let thread_branch =
            shell_thread.and_then(|thread| thread.branch.as_ref().map(|branch| branch.0.clone()));
        let has_worktree = shell_thread.is_some_and(|thread| thread.worktree_path.is_some());

        // Closed-state label: the status stream, then any cached current ref.
        let status = self.git.status.as_ref();
        let current_git_branch = status
            .and_then(|status| status.ref_name.clone())
            .or_else(|| {
                self.branch
                    .merged
                    .refs
                    .iter()
                    .find(|git_ref| git_ref.current)
                    .map(|git_ref| git_ref.name.0.clone())
            });
        // Drop the optimistic overlay once git reports the checked-out ref.
        if self.branch.optimistic_branch.is_some()
            && self.branch.optimistic_branch == current_git_branch
        {
            self.branch.optimistic_branch = None;
        }
        let value = self.branch.optimistic_branch.clone().or_else(|| {
            resolve_branch_toolbar_value(
                false,
                thread_branch.as_deref(),
                current_git_branch.as_deref(),
            )
        });
        let label = SharedString::from(branch_trigger_label(false, value.as_deref()));
        let disabled = self.branch.action_pending || self.branch.initial_pending;

        // The static workspace label (Electron's locked variant).
        let workspace_icon = if has_worktree {
            Icon::new(VitreIcon::FolderGit)
        } else {
            Icon::new(IconName::Folder)
        };
        let workspace = h_flex()
            .gap_1()
            .px_2()
            .text_xs()
            .font_medium()
            .text_color(cx.theme().muted_foreground.opacity(0.7))
            .child(workspace_icon.with_size(px(12.)))
            .child(locked_workspace_label(has_worktree));

        // The PR pill, when the status's PR belongs to this branch.
        let pr_pill = resolve_thread_pr(value.as_deref(), status, has_worktree).map(|pr| {
            let is_dark = cx.theme().is_dark();
            let color: gpui::Hsla = match &pr.state {
                VcsStatusRemoteResultPrState::Open => {
                    // emerald-600 / dark emerald-300 at 90%.
                    if is_dark {
                        rgba(0x6ee7b7e6).into()
                    } else {
                        rgb(0x059669).into()
                    }
                }
                VcsStatusRemoteResultPrState::Merged => {
                    // violet-600 / dark violet-300 at 90%.
                    if is_dark {
                        rgba(0xc4b5fde6).into()
                    } else {
                        rgb(0x7c3aed).into()
                    }
                }
                // closed (and unknown states): red-600 / dark red-300 at 90%.
                _ => {
                    if is_dark {
                        rgba(0xfca5a5e6).into()
                    } else {
                        rgb(0xdc2626).into()
                    }
                }
            };
            let terminology = status_terminology(status);
            let tooltip = SharedString::from(format!(
                "Open {} #{} ({}) in browser",
                terminology.singular,
                pr.number,
                pr_state_label(&pr.state)
            ));
            let url = pr.url.clone();
            h_flex()
                .id("branch-pr-pill")
                .gap_0p5()
                .px_1()
                .py_0p5()
                .rounded(px(4.))
                .text_size(px(11.))
                .font_medium()
                .text_color(color)
                .hover(|this| this.bg(cx.theme().muted.opacity(0.6)))
                .child(Icon::new(VitreIcon::GitPullRequest).with_size(px(12.)))
                .child(format!("#{}", pr.number))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    cx.open_url(&url);
                }))
        });

        // Footer status text (loading / "Showing X of Y refs"), with the
        // shown count taken from the client-filtered ref rows.
        let shown = self
            .branch
            .merged
            .refs
            .iter()
            .filter(|git_ref| {
                should_include_branch_picker_item(&self.branch.query, &git_ref.name.0)
            })
            .count();
        let footer_text = refs_status_text(
            self.branch.initial_pending,
            self.branch.fetching_next,
            self.branch.merged.next_cursor.is_some(),
            shown,
            self.branch.merged.total_count,
        );

        let muted = cx.theme().muted_foreground;
        let trigger_label = label.clone();
        let combobox = Combobox::new(&self.branch.picker)
            .appearance(false)
            .xsmall()
            .menu_width(px(320.))
            .menu_max_h(px(260.))
            .search_placeholder("Search refs...")
            .disabled(disabled)
            .render_trigger(move |_, _: &mut Window, cx: &mut App| {
                let muted = cx.theme().muted_foreground;
                h_flex()
                    .gap_1()
                    .px_1()
                    .py_0p5()
                    .rounded(px(4.))
                    .text_xs()
                    .text_color(muted.opacity(0.7))
                    .hover(|this| this.text_color(cx.theme().foreground.opacity(0.8)))
                    .child(
                        Icon::new(IconName::GitBranch)
                            .with_size(px(12.))
                            .opacity(0.7),
                    )
                    .child(
                        div()
                            .max_w(px(240.))
                            .truncate()
                            .child(trigger_label.clone()),
                    )
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .with_size(px(12.))
                            .opacity(0.5),
                    )
                    .into_any_element()
            })
            .empty(move |_, cx: &App| {
                div()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No refs found.")
                    .into_any_element()
            });
        let combobox = if let Some(text) = footer_text {
            let text = SharedString::from(text);
            combobox.footer(move |_: &mut Window, cx: &mut App| {
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(text.clone())
                    .into_any_element()
            })
        } else {
            combobox
        };

        Some(
            div()
                .px_5()
                .child(
                    h_flex()
                        .w_full()
                        .max_w(px(724.))
                        .mx_auto()
                        .items_center()
                        .gap_2()
                        .px_1()
                        .pt_1()
                        .pb_1()
                        .child(workspace)
                        .child(
                            h_flex()
                                .ml_auto()
                                .min_w_0()
                                .items_center()
                                .gap_1()
                                .children(pr_pill)
                                .child(div().flex_none().child(combobox)),
                        )
                        .text_color(muted),
                )
                .into_any_element(),
        )
    }
}
