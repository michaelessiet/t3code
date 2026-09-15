//! A native selection-to-review action, using the same wire format as diff comments.
use super::*;
use vitre_state::review_comments::ReviewCommentContext;

pub fn selection_lines(text: &str, range: std::ops::Range<usize>) -> (usize, usize, String) {
    let start = range.start.min(text.len());
    let end = range.end.min(text.len()).max(start);
    let first = text.as_bytes()[..start]
        .iter()
        .filter(|&&b| b == b'\n')
        .count();
    // A selection ending at the next line's first column excludes that line.
    let inclusive_end = if end > start { end - 1 } else { end };
    let last = text.as_bytes()[..inclusive_end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count();
    let snippet = text
        .split('\n')
        .skip(first)
        .take(last - first + 1)
        .collect::<Vec<_>>()
        .join("\n");
    (first, last, snippet)
}

impl gpui::EventEmitter<ReviewCommentContext> for FilesPanel {}

impl FilesPanel {
    #[cfg(debug_assertions)]
    pub(crate) fn verify_annotation(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.set_selected_range(0..4, cx));
        self.comment_on_selection(cx);
    }
    pub(super) fn comment_on_selection(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        let editor = self.editor.read(cx);
        let (start, end, snippet) =
            selection_lines(&editor.text().to_string(), editor.selected_range());
        let comment = ReviewCommentContext {
            id: format!(
                "editor-{}",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
            ),
            section_id: format!("editor:{}", open.relative_path),
            section_title: "Editor selection".into(),
            file_path: open.relative_path.clone(),
            start_index: start,
            end_index: end,
            range_label: if start == end {
                format!("L{}", start + 1)
            } else {
                format!("L{}–{}", start + 1, end + 1)
            },
            text: String::new(),
            diff: snippet,
            fence_language: Some("text".into()),
        };
        cx.emit(comment);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_at_line_boundary_and_unicode() {
        assert_eq!(
            selection_lines("één\nsecond\nlast", 0..5),
            (0, 0, "één".into())
        );
        assert_eq!(selection_lines("a\nb\nc", 2..4), (1, 1, "b".into()));
        assert_eq!(selection_lines("a\nb", 2..2), (1, 1, "b".into()));
        assert_eq!(selection_lines("", 0..0), (0, 0, "".into()));
    }
}
