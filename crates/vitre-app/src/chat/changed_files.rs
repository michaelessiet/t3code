//! The changed-files card under assistant messages (M3 slice 2.4).
//!
//! Ports `apps/web/src/components/chat/ChangedFilesTree.tsx`
//! (`ChangedFilesCard` + `ChangedFilesTree`) and its call site in
//! `MessagesTimeline.tsx` (`AssistantChangedFilesSection`). Pure derivations
//! live in `vitre_state::turn_diff_tree`; persisted expansion in
//! `vitre_state::chat_ui` (`~/.vitre/ui-state.json`, Electron's
//! `t3code:ui-state:v1` slice).
//!
//! Deviations noted in the parity matrix:
//! - No sticky card header while expanded (the card lives inside one
//!   virtualized timeline row; gpui has no in-row sticky positioning).
//! - Generic file/folder icons instead of Pierre's per-filetype colored
//!   sprite — same deviation as the files panel.
//! - Persist writes are immediate rather than debounced 500 ms.

use std::collections::{BTreeMap, HashMap};
use std::hash::Hash as _;
use std::path::{Path, PathBuf};

use gpui::{AnyElement, ClickEvent, Context, SharedString, div, prelude::*, px};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, button::Button, h_flex, v_flex,
};
use vitre_contracts::{OrchestrationCheckpointSummary, OrchestrationThread, TurnId};
use vitre_state::chat_ui::ChatUiState;
use vitre_state::turn_diff_tree::{
    TurnDiffNode, TurnDiffStat, build_turn_diff_tree, changed_file_name, expansion_state_key,
    format_compact_count, select_changed_file_preview, should_auto_expand_changed_files,
    summarize_changed_file_scopes, summarize_turn_diff_stats,
};

use super::ChatApp;
use crate::assets::VitreIcon;

/// Persisted UI-state file under `~/.vitre`.
pub(super) const FILE_NAME: &str = "ui-state.json";

/// Per-turn component-local state, mirroring the web card's `useState`s.
#[derive(Debug, Clone)]
struct CardLocal {
    /// `shouldAutoExpandChangedFiles`, evaluated once at "mount" (the
    /// checkpoint's first appearance) and deliberately never recomputed.
    auto_expanded: bool,
    /// The header's expand/collapse-all-folders toggle. Never persisted.
    all_dirs_expanded: bool,
    /// Manual per-directory overrides, valid only while the expansion-state
    /// key they were stamped with still matches (toggle-all or a changed
    /// directory set discards them). BTreeMap for deterministic hashing.
    overrides_key: String,
    dir_overrides: BTreeMap<String, bool>,
}

/// All changed-files card state hanging off [`ChatApp`].
pub(super) struct ChangedFilesState {
    store: ChatUiState,
    store_path: PathBuf,
    local: HashMap<String, CardLocal>,
}

impl ChangedFilesState {
    pub(super) fn load(home: &Path) -> Self {
        let store_path = home.join(FILE_NAME);
        let store = std::fs::read_to_string(&store_path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .map(|value| ChatUiState::from_persisted(&value))
            .unwrap_or_default();
        Self {
            store,
            store_path,
            local: HashMap::new(),
        }
    }

    fn save(&self) {
        let write = serde_json::to_string_pretty(&self.store.to_persisted())
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

    /// Record the once-only auto-expand decision for a newly seen checkpoint.
    /// Called from `rebuild_timeline`, the Vitre analog of the card mounting.
    pub(super) fn ensure_local(
        &mut self,
        summary: &OrchestrationCheckpointSummary,
        is_latest_turn: bool,
    ) {
        self.local
            .entry(summary.turn_id.0.clone())
            .or_insert_with(|| {
                let auto = should_auto_expand_changed_files(&summary.files, is_latest_turn);
                CardLocal {
                    auto_expanded: auto,
                    all_dirs_expanded: auto,
                    overrides_key: String::new(),
                    dir_overrides: BTreeMap::new(),
                }
            });
    }

    /// Thread switch ≙ every card unmounting.
    pub(super) fn clear_local(&mut self) {
        self.local.clear();
    }

    /// `expanded = persisted ?? (isLatestTurn && autoExpanded)`.
    fn effective_expanded(&self, thread_key: &str, turn_id: &str, is_latest_turn: bool) -> bool {
        self.store
            .changed_files_expanded(thread_key, turn_id)
            .unwrap_or_else(|| {
                is_latest_turn
                    && self
                        .local
                        .get(turn_id)
                        .map(|local| local.auto_expanded)
                        .unwrap_or(false)
            })
    }

    /// Everything that can change the card's rendered height, folded into the
    /// timeline row fingerprint. Over-hashing only costs a remeasure;
    /// under-hashing leaves a stale cached row height.
    pub(super) fn hash_card(
        &self,
        hasher: &mut std::collections::hash_map::DefaultHasher,
        thread_key: Option<&str>,
        view: &OrchestrationThread,
        message_id: &vitre_contracts::MessageId,
    ) {
        let Some(summary) = summary_for_message(view, message_id) else {
            false.hash(hasher);
            return;
        };
        true.hash(hasher);
        summary.turn_id.0.hash(hasher);
        for file in &summary.files {
            file.path.0.hash(hasher);
            file.additions.0.hash(hasher);
            file.deletions.0.hash(hasher);
        }
        let is_latest = is_latest_turn(view, summary);
        is_latest.hash(hasher);
        if let Some(key) = thread_key {
            self.effective_expanded(key, &summary.turn_id.0, is_latest)
                .hash(hasher);
        }
        if let Some(local) = self.local.get(&summary.turn_id.0) {
            local.all_dirs_expanded.hash(hasher);
            local.overrides_key.hash(hasher);
            for (path, expanded) in &local.dir_overrides {
                path.hash(hasher);
                expanded.hash(hasher);
            }
        }
    }
}

/// Last checkpoint bound to this assistant message — Electron builds a map
/// where later duplicates overwrite earlier ones, hence the reverse scan.
pub(super) fn summary_for_message<'a>(
    view: &'a OrchestrationThread,
    message_id: &vitre_contracts::MessageId,
) -> Option<&'a OrchestrationCheckpointSummary> {
    view.checkpoints
        .iter()
        .rev()
        .find(|checkpoint| checkpoint.assistant_message_id.as_ref() == Some(message_id))
}

pub(super) fn is_latest_turn(
    view: &OrchestrationThread,
    summary: &OrchestrationCheckpointSummary,
) -> bool {
    view.latest_turn
        .as_ref()
        .is_some_and(|latest| latest.turn_id == summary.turn_id)
}

impl ChatApp {
    /// `AssistantChangedFilesSection`: nothing without a bound checkpoint or
    /// with an empty file list — there is deliberately no loading state.
    pub(super) fn changed_files_card(
        &self,
        row_index: usize,
        summary: &OrchestrationCheckpointSummary,
        is_latest: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if summary.files.is_empty() {
            return None;
        }
        let thread_key = self.dock_thread_key()?;
        let turn_id = summary.turn_id.0.clone();
        let expanded = self
            .changed_files
            .effective_expanded(&thread_key, &turn_id, is_latest);
        let totals = summarize_turn_diff_stats(&summary.files);

        // Header ---------------------------------------------------------
        let file_count = summary.files.len();
        let count_label = if file_count == 1 {
            "1 changed file".to_string()
        } else {
            format!("{file_count} changed files")
        };
        let hint = if expanded { "Hide files" } else { "Show files" };
        let toggle_key = thread_key.clone();
        let toggle_turn = summary.turn_id.clone();
        let mut header_toggle = h_flex()
            .id(SharedString::from(format!("cfc-toggle-{turn_id}")))
            .flex_1()
            .min_w_0()
            .items_center()
            .gap_1p5()
            .rounded(px(8.))
            .px_1()
            .py_1p5()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().accent.opacity(0.6)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_changed_files_expanded(
                    &toggle_key,
                    &toggle_turn,
                    !expanded,
                    row_index,
                    cx,
                );
            }))
            .child(
                Icon::new(if expanded {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .size_3p5()
                .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .font_medium()
                    .text_color(cx.theme().foreground)
                    .whitespace_nowrap()
                    .child(SharedString::from(count_label)),
            );
        if !totals.is_zero() {
            header_toggle = header_toggle.child(diff_stat_label(totals, false, cx));
        }
        header_toggle = header_toggle.child(
            div()
                .ml_1()
                .truncate()
                .text_size(px(11.))
                .text_color(cx.theme().muted_foreground)
                .child(hint),
        );

        let mut controls = h_flex().gap_1p5().flex_shrink_0().items_center();
        if expanded {
            let local = self.changed_files.local.get(&turn_id);
            let all_dirs = local.map(|local| local.all_dirs_expanded).unwrap_or(false);
            let dirs_turn = summary.turn_id.clone();
            controls = controls.child(
                Button::new(SharedString::from(format!("cfc-dirs-{turn_id}")))
                    .icon(if all_dirs {
                        Icon::new(VitreIcon::ChevronsDownUp).size_3()
                    } else {
                        Icon::new(IconName::ChevronsUpDown).size_3()
                    })
                    .outline()
                    .xsmall()
                    .tooltip(if all_dirs {
                        "Collapse all folders"
                    } else {
                        "Expand all folders"
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.toggle_changed_files_dirs_all(&dirs_turn, row_index, cx);
                    })),
            );
        }
        let open_turn = summary.turn_id.clone();
        let open_path = summary.files.first().map(|file| file.path.0.clone());
        controls = controls.child(
            Button::new(SharedString::from(format!("cfc-open-{turn_id}")))
                .icon(Icon::new(VitreIcon::FileDiff).size_3())
                .label("Open diff")
                .outline()
                .xsmall()
                .tooltip("Open the full diff")
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.dock_open_turn_diff(open_turn.clone(), open_path.clone(), window, cx);
                })),
        );

        let header = h_flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_1()
            .child(header_toggle)
            .child(controls);

        // Card container (rounded-2xl; light: border+secondary, dark:
        // borderless input/32 — Electron's dark-mode branch).
        let mut card = v_flex().mt_4().rounded(px(16.)).p_2();
        if cx.theme().is_dark() {
            card = card.bg(cx.theme().input.opacity(0.32));
        } else {
            card = card
                .border_1()
                .border_color(cx.theme().border.opacity(0.7))
                .bg(cx.theme().secondary);
        }
        card = card.child(header);

        if expanded {
            let (nodes, _) = build_turn_diff_tree(&summary.files);
            let local = self.changed_files.local.get(&turn_id);
            let all_dirs = local.map(|local| local.all_dirs_expanded).unwrap_or(false);
            let state_key = expansion_state_key(all_dirs, &nodes);
            let overrides: BTreeMap<String, bool> = local
                .filter(|local| local.overrides_key == state_key)
                .map(|local| local.dir_overrides.clone())
                .unwrap_or_default();
            let has_dirs = nodes
                .iter()
                .any(|node| matches!(node, TurnDiffNode::Dir(_)));
            let mut rows: Vec<AnyElement> = Vec::new();
            self.render_turn_diff_nodes(
                &mut rows,
                &nodes,
                0,
                &TreeContext {
                    turn_id: summary.turn_id.clone(),
                    row_index,
                    all_dirs_expanded: all_dirs,
                    state_key,
                    overrides,
                    has_dirs,
                },
                cx,
            );
            card = card.child(v_flex().gap_0p5().mt_2().children(rows));
        } else if is_latest {
            card = card.child(self.changed_files_preview(summary, row_index, cx));
        }
        Some(card.into_any_element())
    }

    /// The compact preview under the header — latest turn only, collapsed
    /// only: scope roll-up line + up to three file chips + "Show all".
    fn changed_files_preview(
        &self,
        summary: &OrchestrationCheckpointSummary,
        row_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let turn_id = summary.turn_id.0.clone();
        let scopes = summarize_changed_file_scopes(&summary.files);
        let mut scope_line = h_flex()
            .flex_wrap()
            .items_center()
            .gap_x_1p5()
            .gap_y_0p5()
            .text_size(px(11.))
            .text_color(cx.theme().muted_foreground);
        for (index, scope) in scopes.iter().enumerate() {
            if index > 0 {
                scope_line = scope_line.child(div().child("·"));
            }
            let count = if scope.file_count == 1 {
                "1 file".to_string()
            } else {
                format!("{} files", scope.file_count)
            };
            scope_line = scope_line.child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().foreground.opacity(0.75))
                            .child(SharedString::from(scope.label.clone())),
                    )
                    .child(SharedString::from(count)),
            );
        }

        let mut chips = h_flex().flex_wrap().items_center().gap_1p5().mt_2();
        for index in select_changed_file_preview(&summary.files) {
            let file = &summary.files[index];
            // Chips pass the RAW path through to the diff panel; only the
            // displayed name is normalized (gotcha 14).
            let raw_path = file.path.0.clone();
            let name = changed_file_name(&raw_path);
            let chip_turn = summary.turn_id.clone();
            chips = chips.child(
                h_flex()
                    .id(SharedString::from(format!("cfc-chip-{turn_id}-{index}")))
                    .max_w(px(192.))
                    .items_center()
                    .gap_1()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(cx.theme().border.opacity(0.7))
                    .bg(cx.theme().background.opacity(0.45))
                    .px_1p5()
                    .py_1()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(px(10.))
                    .text_color(cx.theme().muted_foreground)
                    .cursor_pointer()
                    .hover(|style| {
                        style
                            .bg(cx.theme().accent.opacity(0.6))
                            .text_color(cx.theme().foreground)
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.dock_open_turn_diff(
                            chip_turn.clone(),
                            Some(raw_path.clone()),
                            window,
                            cx,
                        );
                    }))
                    .child(
                        Icon::new(IconName::File)
                            .size_3()
                            .text_color(cx.theme().muted_foreground.opacity(0.7)),
                    )
                    .child(div().truncate().child(SharedString::from(name))),
            );
        }
        let show_all_key = self.dock_thread_key();
        let show_all_turn = summary.turn_id.clone();
        chips = chips.child(
            div()
                .id(SharedString::from(format!("cfc-show-all-{turn_id}")))
                .rounded(px(6.))
                .px_1p5()
                .py_1()
                .text_size(px(11.))
                .font_medium()
                .text_color(cx.theme().muted_foreground)
                .cursor_pointer()
                .hover(|style| {
                    style
                        .bg(cx.theme().accent.opacity(0.6))
                        .text_color(cx.theme().foreground)
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(key) = show_all_key.clone() {
                        this.set_changed_files_expanded(&key, &show_all_turn, true, row_index, cx);
                    }
                }))
                .child(SharedString::from(format!(
                    "Show all {} files",
                    summary.files.len()
                ))),
        );

        v_flex()
            .px_2()
            .pb_1p5()
            .pt_1()
            .child(scope_line)
            .child(chips)
            .into_any_element()
    }

    fn render_turn_diff_nodes(
        &self,
        out: &mut Vec<AnyElement>,
        nodes: &[TurnDiffNode],
        depth: usize,
        tree: &TreeContext,
        cx: &mut Context<Self>,
    ) {
        let indent = px(8. + depth as f32 * 14.);
        for node in nodes {
            match node {
                TurnDiffNode::Dir(dir) => {
                    let expanded = tree
                        .overrides
                        .get(&dir.path)
                        .copied()
                        .unwrap_or(tree.all_dirs_expanded);
                    let toggle_turn = tree.turn_id.clone();
                    let toggle_path = dir.path.clone();
                    let toggle_key = tree.state_key.clone();
                    let row_index = tree.row_index;
                    let mut row = h_flex()
                        .id(SharedString::from(format!(
                            "cfc-dir-{}-{}",
                            tree.turn_id.0, dir.path
                        )))
                        .w_full()
                        .items_center()
                        .gap_1p5()
                        .rounded(px(12.))
                        .py_1()
                        .pr_3()
                        .pl(indent)
                        .cursor_pointer()
                        .hover(|style| style.bg(cx.theme().accent.opacity(0.6)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_changed_files_dir(
                                &toggle_turn,
                                toggle_path.clone(),
                                toggle_key.clone(),
                                row_index,
                                cx,
                            );
                        }))
                        .child(
                            Icon::new(if expanded {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .size_3p5()
                            .text_color(cx.theme().muted_foreground.opacity(0.7)),
                        )
                        .child(
                            Icon::new(if expanded {
                                IconName::FolderOpen
                            } else {
                                IconName::Folder
                            })
                            .size_3p5()
                            .text_color(cx.theme().muted_foreground.opacity(0.75)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_size(px(11.))
                                .text_color(cx.theme().muted_foreground.opacity(0.9))
                                .child(SharedString::from(dir.name.clone())),
                        );
                    // Directory rows suppress all-zero stats.
                    if !dir.stat.is_zero() {
                        row = row.child(
                            div()
                                .ml_auto()
                                .flex_shrink_0()
                                .child(diff_stat_label(dir.stat, true, cx)),
                        );
                    }
                    out.push(row.into_any_element());
                    if expanded {
                        self.render_turn_diff_nodes(out, &dir.children, depth + 1, tree, cx);
                    }
                }
                TurnDiffNode::File(file) => {
                    let open_turn = tree.turn_id.clone();
                    // Tree rows pass the NORMALIZED path (gotcha 14).
                    let open_path = file.path.clone();
                    let mut row = h_flex()
                        .id(SharedString::from(format!(
                            "cfc-file-{}-{}",
                            tree.turn_id.0, file.path
                        )))
                        .w_full()
                        .items_center()
                        .gap_1p5()
                        .rounded(px(12.))
                        .py_1()
                        .pr_3()
                        .pl(indent)
                        .cursor_pointer()
                        .hover(|style| style.bg(cx.theme().accent.opacity(0.6)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.dock_open_turn_diff(
                                open_turn.clone(),
                                Some(open_path.clone()),
                                window,
                                cx,
                            );
                        }));
                    // Alignment spacer where directory rows have a chevron;
                    // flat all-root-file trees skip it.
                    if tree.has_dirs || depth > 0 {
                        row = row.child(div().size_3p5().flex_shrink_0());
                    }
                    row = row
                        .child(
                            Icon::new(IconName::File)
                                .size_3p5()
                                .text_color(cx.theme().muted_foreground.opacity(0.7)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_size(px(11.))
                                .text_color(cx.theme().muted_foreground.opacity(0.8))
                                .child(SharedString::from(file.name.clone())),
                        )
                        // File rows always show their stat, `+0 −0` included.
                        .child(
                            div()
                                .ml_auto()
                                .flex_shrink_0()
                                .child(diff_stat_label(file.stat, true, cx)),
                        );
                    out.push(row.into_any_element());
                }
            }
        }
    }

    // ---- actions --------------------------------------------------------

    fn set_changed_files_expanded(
        &mut self,
        thread_key: &str,
        turn_id: &TurnId,
        expanded: bool,
        row_index: usize,
        cx: &mut Context<Self>,
    ) {
        self.changed_files
            .store
            .set_changed_files_expanded(thread_key, &turn_id.0, expanded);
        self.changed_files.save();
        self.remeasure_row(row_index, cx);
    }

    fn toggle_changed_files_dirs_all(
        &mut self,
        turn_id: &TurnId,
        row_index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(local) = self.changed_files.local.get_mut(&turn_id.0) {
            // Flipping the toggle changes the expansion-state key, which
            // implicitly discards every manual per-directory override.
            local.all_dirs_expanded = !local.all_dirs_expanded;
            self.remeasure_row(row_index, cx);
        }
    }

    fn toggle_changed_files_dir(
        &mut self,
        turn_id: &TurnId,
        dir_path: String,
        state_key: String,
        row_index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(local) = self.changed_files.local.get_mut(&turn_id.0) {
            if local.overrides_key != state_key {
                local.dir_overrides.clear();
                local.overrides_key = state_key;
            }
            let effective = local
                .dir_overrides
                .get(&dir_path)
                .copied()
                .unwrap_or(local.all_dirs_expanded);
            local.dir_overrides.insert(dir_path, !effective);
            self.remeasure_row(row_index, cx);
        }
    }
}

/// Per-card constants threaded through the tree recursion.
struct TreeContext {
    turn_id: TurnId,
    row_index: usize,
    all_dirs_expanded: bool,
    state_key: String,
    overrides: BTreeMap<String, bool>,
    has_dirs: bool,
}

/// `DiffStatLabel`: `+adds` in success, `−dels` in danger, mono,
/// compact-formatted. `aligned` mimics the fixed 4ch grid columns so stats
/// line up down the tree; the header uses the inline layout.
fn diff_stat_label(stat: TurnDiffStat, aligned: bool, cx: &mut Context<ChatApp>) -> AnyElement {
    let additions = div()
        .text_color(cx.theme().success)
        .child(SharedString::from(format!(
            "+{}",
            format_compact_count(stat.additions)
        )));
    let deletions = div()
        .text_color(cx.theme().danger)
        .child(SharedString::from(format!(
            "-{}",
            format_compact_count(stat.deletions)
        )));
    if aligned {
        h_flex()
            .gap_2()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(px(10.))
            .child(div().w(px(28.)).text_right().child(additions))
            .child(div().w(px(28.)).text_right().child(deletions))
            .into_any_element()
    } else {
        h_flex()
            .gap_1()
            .items_center()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(px(12.))
            .child(additions)
            .child(deletions)
            .into_any_element()
    }
}
