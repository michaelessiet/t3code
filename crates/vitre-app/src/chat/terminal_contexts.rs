//! Terminal contexts on the composer + timeline side.
//!
//! The pending list is Electron's `composerDraftStore` terminalContexts slice
//! — Vitre keeps it in-memory on the open thread (drafts are not persisted
//! yet, matrix-noted). Chips above the textarea port
//! `ComposerPendingTerminalContexts` with an added X remove button — Electron
//! removes a context by deleting its inline U+FFFC placeholder chip from the
//! prompt editor, which Vitre's plain textarea has no equivalent for
//! (matrix-noted deviation; the prompt therefore carries no placeholders and
//! `append_terminal_contexts_to_prompt` appends the block only).
//!
//! Sent messages run `UserTimelineRow`'s pipeline:
//! `deriveDisplayedUserMessageState` (element block stripped first, then
//! terminal), a second trailing element extraction, element chips, then
//! `UserMessageBody` — review-comment segments first, else terminal label
//! chips + text. Electron's embedded-inline-label rendering (labels typed
//! into the text splitting into inline chips) is a matrix-noted deviation:
//! Vitre always uses the chips-prefix fallback path.

use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex, v_flex,
};
use vitre_state::terminal_context::{
    ParsedContextEntry, TerminalContextSelection, derive_displayed_user_message_state,
    extract_trailing_element_contexts, format_terminal_context_label,
    normalize_terminal_context_selection,
};

use crate::assets::VitreIcon;

use super::{ChatApp, fresh_id};

/// Electron's `TerminalContextDraft`, minus the thread scoping — the pending
/// list lives on the open thread and clears with it.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct PendingTerminalContext {
    pub id: String,
    pub selection: TerminalContextSelection,
}

/// `composerDraftStore.terminalContextDedupKey`.
fn dedup_key(selection: &TerminalContextSelection) -> String {
    format!(
        "{}\0{}\0{}",
        selection.terminal_id, selection.line_start, selection.line_end
    )
}

impl ChatApp {
    /// `composerDraftStore.addTerminalContext`: normalize, then skip entries
    /// whose dedup key (`terminalId\0lineStart\0lineEnd`) already pends.
    pub(super) fn add_terminal_context(
        &mut self,
        selection: &TerminalContextSelection,
        cx: &mut Context<Self>,
    ) {
        let Some(normalized) = normalize_terminal_context_selection(selection) else {
            return;
        };
        let key = dedup_key(&normalized);
        if self
            .pending_terminal_contexts
            .iter()
            .any(|pending| dedup_key(&pending.selection) == key)
        {
            return;
        }
        self.pending_terminal_contexts.push(PendingTerminalContext {
            id: fresh_id("terminal-context"),
            selection: normalized,
        });
        self.schedule_draft_save(cx);
        cx.notify();
    }

    pub(super) fn remove_terminal_context(&mut self, id: &str, cx: &mut Context<Self>) {
        let before = self.pending_terminal_contexts.len();
        self.pending_terminal_contexts
            .retain(|pending| pending.id != id);
        if self.pending_terminal_contexts.len() != before {
            self.schedule_draft_save(cx);
            cx.notify();
        }
    }

    /// `ComposerPendingTerminalContexts`: one inline chip per pending context
    /// (`{terminalLabel} {range}`, hover shows the captured text).
    pub(super) fn render_pending_terminal_contexts(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.pending_terminal_contexts.is_empty() {
            return None;
        }
        let mut chips = h_flex().flex_wrap().gap_1p5().px_3().pt_2();
        for (index, pending) in self.pending_terminal_contexts.iter().enumerate() {
            let label = format_terminal_context_label(&pending.selection);
            let tooltip = SharedString::from(pending.selection.text.clone());
            let remove_id = pending.id.clone();
            chips = chips.child(
                terminal_context_chip(("terminal-context-chip", index), label, tooltip, cx).child(
                    div()
                        .id(("terminal-context-remove", index))
                        .cursor_pointer()
                        .rounded(px(4.))
                        .p_0p5()
                        .text_color(cx.theme().muted_foreground)
                        .hover(|style| style.bg(cx.theme().accent))
                        .child(Icon::new(IconName::Close).size_3())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_terminal_context(&remove_id, cx);
                        })),
                ),
            );
        }
        Some(chips.into_any_element())
    }

    /// `UserTimelineRow` + `UserMessageBody`: `Some` when the sent message
    /// embeds context or review blocks; `None` falls back to the plain bubble
    /// text. Preview annotations are absent in Vitre (M4 territory).
    pub(super) fn user_message_body(
        &self,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = derive_displayed_user_message_state(text);
        let (prompt_text, more_elements) = extract_trailing_element_contexts(&state.visible_text);
        let mut element_contexts = state.element_contexts;
        element_contexts.extend(more_elements);
        let terminal_contexts = state.contexts;
        if element_contexts.is_empty() && terminal_contexts.is_empty() {
            // No trailing context blocks stripped (also Electron's quirk path:
            // review blocks after a terminal block keep it embedded as text).
            return self.user_message_review_body(text, cx);
        }
        let review_body = self.user_message_review_body(&prompt_text, cx);
        let mut column = v_flex().gap_2();
        if !element_contexts.is_empty() {
            let mut row = h_flex().flex_wrap().gap_1p5();
            for (index, context) in element_contexts.iter().enumerate() {
                row = row.child(element_context_chip(index, context, cx));
            }
            column = column.child(row);
        }
        if let Some(body) = review_body {
            column = column.child(body);
        } else if !terminal_contexts.is_empty() {
            let mut row = h_flex().flex_wrap().gap_1();
            for (index, context) in terminal_contexts.iter().enumerate() {
                let tooltip = SharedString::from(if context.body.is_empty() {
                    context.header.clone()
                } else {
                    format!("{}\n{}", context.header, context.body)
                });
                row = row.child(terminal_context_chip(
                    ("user-terminal-context", index),
                    context.header.clone(),
                    tooltip,
                    cx,
                ));
            }
            column = column.child(row);
            if !prompt_text.is_empty() {
                column = column.child(div().child(SharedString::from(prompt_text)));
            }
        } else if !prompt_text.is_empty() {
            column = column.child(div().child(SharedString::from(prompt_text)));
        }
        Some(column.into_any_element())
    }
}

/// `TerminalContextInlineChip`: terminal icon + truncated label on the
/// `composerInlineChip` style (`rounded-md border-border/70 bg-accent/40
/// px-1.5 py-px font-medium text-[12px]`).
fn terminal_context_chip(
    id: (&'static str, usize),
    label: String,
    tooltip: SharedString,
    cx: &mut Context<ChatApp>,
) -> gpui::Stateful<gpui::Div> {
    h_flex()
        .id(id)
        .max_w_full()
        .items_center()
        .gap_1()
        .rounded(px(6.))
        .border_1()
        .border_color(cx.theme().border.opacity(0.7))
        .bg(cx.theme().accent.opacity(0.4))
        .px_1p5()
        .py(px(1.))
        .font_medium()
        .text_size(px(12.))
        .text_color(cx.theme().foreground)
        .when(!tooltip.is_empty(), |chip| {
            chip.tooltip(move |window: &mut Window, cx: &mut gpui::App| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
        })
        .child(
            Icon::new(VitreIcon::Terminal)
                .with_size(px(14.))
                .flex_shrink_0(),
        )
        .child(
            div()
                .max_w(px(220.))
                .truncate()
                .child(SharedString::from(label)),
        )
}

/// `UserMessageElementContextChip`: mouse-pointer icon + truncated header
/// (`rounded-md border-border/70 bg-background/70 px-1.5 py-0.5 text-xs`).
fn element_context_chip(
    index: usize,
    context: &ParsedContextEntry,
    cx: &mut Context<ChatApp>,
) -> AnyElement {
    let tooltip = SharedString::from(if context.body.is_empty() {
        context.header.clone()
    } else {
        format!("{}\n{}", context.header, context.body)
    });
    h_flex()
        .id(("user-element-context", index))
        .max_w_full()
        .items_center()
        .gap_1()
        .rounded(px(6.))
        .border_1()
        .border_color(cx.theme().border.opacity(0.7))
        .bg(cx.theme().background.opacity(0.7))
        .px_1p5()
        .py_0p5()
        .text_xs()
        .text_color(cx.theme().foreground.opacity(0.85))
        .when(!tooltip.is_empty(), |chip| {
            chip.tooltip(move |window: &mut Window, cx: &mut gpui::App| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
        })
        .child(
            Icon::new(VitreIcon::MousePointerClick)
                .size_3()
                .flex_shrink_0(),
        )
        .child(
            div()
                .truncate()
                .child(SharedString::from(context.header.clone())),
        )
        .into_any_element()
}
