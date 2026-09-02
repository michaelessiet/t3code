//! QuickSearch's preview pane — the right-hand column of Electron's overlay.
//!
//! Electron derives the pane (and the dialog's own width) from whatever row is
//! highlighted: a thread or message row previews as a small card, a file or
//! content-match row previews the file itself, and an empty result list
//! previews nothing and collapses the pane to zero. See
//! `apps/web/src/components/QuickSearch.tsx`.
//!
//! The file view is gpui-component's read-only [`Editor`]. Two of its existing
//! behaviours do the work Electron needs CodeMirror extensions for: moving the
//! cursor scrolls its row into view, and with line numbers enabled the editor
//! paints an active-line background on the cursor's row — which is exactly the
//! `.cm-reveal-line` highlight, and is painted whether or not the editor has
//! focus.

use gpui::{
    AnyElement, App, Entity, HighlightStyle, IntoElement, SharedString, StyledText, div,
    prelude::*, px,
};
use gpui_component::input::{Editor, EditorState};
use gpui_component::{ActiveTheme as _, Icon, IconName, StyledExt as _, h_flex, v_flex};
use vitre_contracts::{OrchestrationMessageSearchMatch, OrchestrationThreadShell};

use super::rank::name_segments;
use super::relative_time;

/// Which shape the pane takes. Electron sizes both the pane and the dialog
/// from this.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PreviewKind {
    /// Nothing highlighted: the pane collapses and the dialog is list-width.
    None,
    /// A thread or message row: a fixed-width summary card.
    Card,
    /// A file or content-match row: the file itself, at its wider measure.
    File,
}

impl PreviewKind {
    /// Electron's `max-w-xl` / `max-w-2xl` / `max-w-3xl` on the popup.
    pub(super) fn dialog_width(self) -> gpui::Pixels {
        match self {
            PreviewKind::None => px(576.),
            PreviewKind::Card => px(672.),
            PreviewKind::File => px(768.),
        }
    }

    /// Electron's `w-0` / `w-72` / `w-[55%]` on the pane itself. The file
    /// measure is a fraction of the dialog, so it is resolved against the
    /// width above rather than stored.
    pub(super) fn pane_width(self) -> gpui::Pixels {
        match self {
            PreviewKind::None => px(0.),
            PreviewKind::Card => px(288.),
            PreviewKind::File => self.dialog_width() * 0.55,
        }
    }
}

/// How far along the file read for the highlighted row is.
pub(super) enum PreviewFile {
    /// The read is in flight. Electron shows bare "Loading…" text here, with
    /// no skeleton.
    Loading,
    /// Contents are in the editor.
    Ready,
    /// The read failed. Electron renders the server's message verbatim, which
    /// is also how a binary file surfaces — there is no dedicated branch for
    /// one.
    Error(SharedString),
}

/// The pane's outer column. Always mounted, zero-width when there is nothing
/// to show, so the width transition has something to animate.
pub(super) fn pane(kind: PreviewKind, body: Option<AnyElement>, cx: &App) -> AnyElement {
    let column = v_flex()
        .flex_none()
        .min_h_0()
        .w(kind.pane_width())
        .overflow_hidden()
        .bg(cx.theme().muted.opacity(0.3));
    match body {
        Some(body) if kind != PreviewKind::None => column
            .border_l_1()
            .border_color(cx.theme().border)
            .child(body)
            .into_any_element(),
        _ => column.into_any_element(),
    }
}

/// Electron's `ThreadPreview`: title, then a muted block of project, branch
/// and timestamps.
pub(super) fn thread_card(
    thread: &OrchestrationThreadShell,
    project_title: Option<&str>,
    cx: &App,
) -> AnyElement {
    let branch = thread.branch.as_ref().map(|branch| branch.0.clone());
    let updated = relative_time(&thread.updated_at.0);
    let last_message = thread
        .latest_user_message_at
        .as_ref()
        .and_then(|at| relative_time(&at.0));

    v_flex()
        .gap_3()
        .p_4()
        .text_size(px(12.))
        .child(
            div()
                .text_size(px(14.))
                .font_medium()
                .child(thread.title.0.clone()),
        )
        .child(
            v_flex()
                .gap_1p5()
                .text_color(cx.theme().muted_foreground)
                .children(project_title.map(|title| div().child(format!("Project: {title}"))))
                .children(branch.map(|branch| {
                    h_flex()
                        .gap_1()
                        .child(Icon::new(IconName::GitBranch).size_3())
                        .child(branch)
                }))
                .children(updated.map(|updated| div().child(format!("Updated {updated}"))))
                .children(last_message.map(|at| div().child(format!("Last message {at}")))),
        )
        .into_any_element()
}

/// Electron's `MessagePreview`: thread title, a small uppercase role/time
/// line, then the snippet with the query highlighted.
pub(super) fn message_card(
    hit: &OrchestrationMessageSearchMatch,
    query: &str,
    cx: &App,
) -> AnyElement {
    let role = format!("{:?}", hit.role).to_lowercase();
    let when = relative_time(&hit.updated_at.0).unwrap_or_default();
    let snippet = hit.snippet.0.clone();
    // The snippet wraps, so the match cannot be a sibling element the way it
    // is in the single-line result rows — a `div` child of a flex column is a
    // line of its own. A highlight run over one text element keeps it inline.
    let matched = {
        let (before, matched, _) = name_segments(&snippet, query);
        (!matched.is_empty()).then(|| before.len()..before.len() + matched.len())
    };

    v_flex()
        .gap_3()
        .p_4()
        .text_size(px(12.))
        .child(
            div()
                .text_size(px(14.))
                .font_medium()
                .child(hit.thread_title.0.clone()),
        )
        .child(
            div()
                .text_size(px(10.))
                .text_color(cx.theme().muted_foreground)
                .child(format!("{} · {when}", role.to_uppercase())),
        )
        .child(div().text_color(cx.theme().muted_foreground).child(
            StyledText::new(snippet).with_highlights(matched.map(|range| {
                (
                    range,
                    HighlightStyle {
                        background_color: Some(cx.theme().warning.opacity(0.25)),
                        color: Some(cx.theme().foreground),
                        ..Default::default()
                    },
                )
            })),
        ))
        .into_any_element()
}

/// The file view: "Loading…", the server's error, or the editor itself.
pub(super) fn file_view(editor: &Entity<EditorState>, state: &PreviewFile, cx: &App) -> AnyElement {
    match state {
        PreviewFile::Loading => div()
            .p_4()
            .text_size(px(12.))
            .text_color(cx.theme().muted_foreground)
            .child("Loading…")
            .into_any_element(),
        PreviewFile::Error(message) => div()
            .p_4()
            .text_size(px(12.))
            .text_color(cx.theme().danger)
            .child(message.clone())
            .into_any_element(),
        // Electron pins the preview to 11px regardless of the user's editor
        // zoom; the editor inherits the ambient text style, so setting it on
        // the wrapper is enough.
        PreviewFile::Ready => div()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .text_size(px(11.))
            .child(
                Editor::new(editor)
                    .readonly(true)
                    .appearance(false)
                    .h_full(),
            )
            .into_any_element(),
    }
}

/// Electron's fallback when a file row is highlighted but the overlay has no
/// environment or workspace root to read it from.
pub(super) fn unavailable(cx: &App) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .p_3()
        .text_center()
        .text_size(px(12.))
        .text_color(cx.theme().muted_foreground)
        .child("Open a chat to preview project files.")
        .into_any_element()
}

/// Centre `line` (one-based) in the pane, the way CodeMirror's
/// `scrollIntoView({ y: "center" })` does.
///
/// Moving the cursor already scrolled the row to whichever edge was nearest,
/// so this only has to re-aim; it is a no-op until the editor has laid out
/// once and can report its line height.
pub(super) fn centre_on_line(
    editor: &Entity<EditorState>,
    line: u32,
    viewport_height: gpui::Pixels,
    cx: &mut App,
) {
    editor.update(cx, |state, cx| {
        let Some(line_height) = state.line_height() else {
            return;
        };
        let row = line.saturating_sub(1) as f32;
        let target = (row * line_height - viewport_height / 2. + line_height / 2.).max(px(0.));
        // The editor scrolls with a negative y and clamps to its own content.
        state.set_scroll_offset(gpui::point(px(0.), -target), cx);
    });
}
