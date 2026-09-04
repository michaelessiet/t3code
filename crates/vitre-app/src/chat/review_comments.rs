//! Review comments on the composer + timeline side.
//!
//! The pending list is Electron's `composerDraftStore` reviewComments slice —
//! Vitre keeps it in-memory on the open thread (composer drafts are not
//! persisted yet, matrix-noted). Chips above the textarea port
//! `ComposerPendingReviewComments`; sent messages embedding
//! `<review_comment>` blocks render `UserMessageReviewCommentCard`s instead of
//! the plain bubble text.

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};
use gpui_component::{ActiveTheme as _, Icon, IconName, StyledExt as _, h_flex, v_flex};
use vitre_state::diff_patch::{PatchFile, PatchLineKind, parse_unified_patch};
use vitre_state::review_comments::{
    ReviewCommentContext, ReviewCommentMessageSegment, build_review_comment_renderable_patch,
    parse_review_comment_message_segments,
};

use crate::assets::VitreIcon;

use super::ChatApp;

impl ChatApp {
    /// `composerDraftStore.addReviewComment`: same-id entries are replaced,
    /// new ones append.
    pub(super) fn add_review_comment(
        &mut self,
        comment: ReviewCommentContext,
        cx: &mut Context<Self>,
    ) {
        self.pending_review_comments
            .retain(|existing| existing.id != comment.id);
        self.pending_review_comments.push(comment);
        cx.notify();
    }

    /// `composerDraftStore.removeReviewComment`.
    pub(super) fn remove_review_comment(&mut self, id: &str, cx: &mut Context<Self>) {
        let before = self.pending_review_comments.len();
        self.pending_review_comments
            .retain(|existing| existing.id != id);
        if self.pending_review_comments.len() != before {
            cx.notify();
        }
    }

    /// `ComposerPendingReviewComments`: one chip per pending comment
    /// (`{filePath} {rangeLabel}`, hover shows the text, X removes).
    pub(super) fn render_pending_review_comments(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.pending_review_comments.is_empty() {
            return None;
        }
        let mut chips = h_flex().flex_wrap().gap_1p5().px_3().pt_2();
        for (index, comment) in self.pending_review_comments.iter().enumerate() {
            let label = format!("{} {}", comment.file_path, comment.range_label);
            let tooltip = SharedString::from(comment.text.clone());
            let remove_id = comment.id.clone();
            chips = chips.child(
                h_flex()
                    .id(("review-comment-chip", index))
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
                    .when(!tooltip.is_empty(), |chip| {
                        chip.tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                        })
                    })
                    .child(
                        Icon::new(VitreIcon::MessageCircle)
                            .size_3()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .max_w(px(220.))
                            .truncate()
                            .child(SharedString::from(label)),
                    )
                    .child(
                        div()
                            .id(("review-comment-remove", index))
                            .cursor_pointer()
                            .rounded(px(4.))
                            .p_0p5()
                            .text_color(cx.theme().muted_foreground)
                            .hover(|style| style.bg(cx.theme().accent))
                            .child(Icon::new(IconName::Close).size_3())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_review_comment(&remove_id, cx);
                            })),
                    ),
            );
        }
        Some(chips.into_any_element())
    }

    /// `UserMessageBody`'s review-comment path: `Some` when the sent message
    /// embeds `<review_comment>` blocks — trimmed text segments interleaved
    /// with comment cards; `None` falls back to the plain bubble text.
    pub(super) fn user_message_review_body(
        &self,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let segments = parse_review_comment_message_segments(text);
        if !segments
            .iter()
            .any(|segment| matches!(segment, ReviewCommentMessageSegment::ReviewComment { .. }))
        {
            return None;
        }
        let mut column = v_flex().gap_3();
        for segment in segments {
            match segment {
                ReviewCommentMessageSegment::Text { text, .. } => {
                    let trimmed = text.trim().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }
                    column = column.child(div().child(SharedString::from(trimmed)));
                }
                ReviewCommentMessageSegment::ReviewComment { comment } => {
                    column = column.child(review_comment_card(comment, cx));
                }
            }
        }
        Some(column.into_any_element())
    }
}

/// `UserMessageReviewCommentCard`: file path, `section · range`, the comment
/// text, then the captured context — a mini unified diff for `diff` fences
/// (Electron renders a full Pierre `FileDiff`; Vitre renders bare rows, the
/// file path already heads the card), a mono block otherwise.
fn review_comment_card(comment: ReviewCommentContext, cx: &mut Context<ChatApp>) -> AnyElement {
    let fence_language = comment.fence_language.as_deref().unwrap_or("diff");
    let mut card = v_flex()
        .gap_2()
        .p_3()
        .rounded(px(8.))
        .border_1()
        .border_color(cx.theme().border.opacity(0.7))
        .bg(cx.theme().background.opacity(0.7))
        .child(
            v_flex()
                .gap_0p5()
                .child(
                    div()
                        .text_xs()
                        .font_medium()
                        .text_color(cx.theme().foreground)
                        .child(SharedString::from(comment.file_path.clone())),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(format!(
                            "{} · {}",
                            comment.section_title, comment.range_label
                        ))),
                ),
        );
    if !comment.text.is_empty() {
        card = card.child(
            div()
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(SharedString::from(comment.text.clone())),
        );
    }
    if fence_language != "diff" && !comment.diff.trim().is_empty() {
        card = card.child(code_block(&comment.diff, cx));
    }
    let renderable = build_review_comment_renderable_patch(&comment);
    if !renderable.is_empty() {
        let files = parse_unified_patch(&renderable);
        if files.is_empty() {
            card = card.child(code_block(renderable.trim(), cx));
        } else {
            for file in &files {
                card = card.child(mini_unified_diff(file, cx));
            }
        }
    }
    card.into_any_element()
}

fn code_block(text: &str, cx: &mut Context<ChatApp>) -> AnyElement {
    div()
        .w_full()
        .p_2()
        .rounded(px(6.))
        .bg(cx.theme().muted.opacity(0.4))
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(px(11.))
        .text_color(cx.theme().foreground)
        .child(SharedString::from(text.to_string()))
        .into_any_element()
}

/// Static (non-virtualized) unified rows with the diff panel's color mixes —
/// review-comment hunks are a handful of lines by construction.
fn mini_unified_diff(file: &PatchFile, cx: &mut Context<ChatApp>) -> AnyElement {
    let mono = cx.theme().mono_font_family.clone();
    let foreground = cx.theme().foreground;
    let number_color = cx.theme().muted_foreground.opacity(0.8);
    let success = cx.theme().success;
    let danger = cx.theme().danger;
    let context_number_bg = cx.theme().foreground.opacity(0.02);

    let mut column = v_flex()
        .w_full()
        .rounded(px(6.))
        .border_1()
        .border_color(cx.theme().border.opacity(0.6))
        .overflow_hidden();
    for hunk in &file.hunks {
        for line in &hunk.lines {
            let (row_bg, number_bg, marker) = match line.kind {
                PatchLineKind::Add => (success.opacity(0.08), success.opacity(0.12), "+"),
                PatchLineKind::Del => (danger.opacity(0.08), danger.opacity(0.12), "-"),
                PatchLineKind::Context => (gpui::transparent_black(), context_number_bg, " "),
            };
            let number_cell = |value: Option<u32>| {
                div()
                    .w(px(34.))
                    .flex_shrink_0()
                    .pr_1()
                    .bg(number_bg)
                    .text_right()
                    .text_color(number_color)
                    .child(SharedString::from(
                        value.map(|number| number.to_string()).unwrap_or_default(),
                    ))
            };
            column = column.child(
                h_flex()
                    .w_full()
                    .items_stretch()
                    .bg(row_bg)
                    .font_family(mono.clone())
                    .text_size(px(11.))
                    .line_height(px(18.))
                    .child(number_cell(line.old_line))
                    .child(number_cell(line.new_line))
                    .child(
                        div()
                            .w_4()
                            .flex_shrink_0()
                            .text_center()
                            .text_color(foreground.opacity(0.7))
                            .child(SharedString::from(marker.trim().to_string())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pr_2()
                            .text_color(foreground)
                            .child(SharedString::from(line.content.clone())),
                    ),
            );
        }
    }
    column.into_any_element()
}
