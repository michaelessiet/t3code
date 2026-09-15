//! Selection-preserving, language-aware line commands for the native editor.
use super::*;
use gpui::{Action, KeyBinding};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
#[action(namespace = vitre, no_json)]
pub(crate) enum EditLines {
    Comment,
    DuplicateUp,
    DuplicateDown,
    MoveUp,
    MoveDown,
    Delete,
    Select,
    InsertAbove,
    InsertBelow,
}

pub(crate) fn init(cx: &mut App) {
    let context = Some("Editor && !vim_command");
    cx.bind_keys([
        KeyBinding::new("secondary-/", EditLines::Comment, context),
        KeyBinding::new("alt-shift-up", EditLines::DuplicateUp, context),
        KeyBinding::new("alt-shift-down", EditLines::DuplicateDown, context),
        KeyBinding::new("alt-up", EditLines::MoveUp, context),
        KeyBinding::new("alt-down", EditLines::MoveDown, context),
        KeyBinding::new("secondary-shift-k", EditLines::Delete, context),
        KeyBinding::new("secondary-l", EditLines::Select, context),
        KeyBinding::new("secondary-shift-enter", EditLines::InsertAbove, context),
        KeyBinding::new("secondary-enter", EditLines::InsertBelow, context),
        KeyBinding::new("secondary-[", gpui_component::input::Outdent, context),
        KeyBinding::new("secondary-]", gpui_component::input::Indent, context),
        KeyBinding::new("secondary-alt-f", gpui_component::input::Replace, context),
    ]);
}

impl FilesPanel {
    pub(super) fn edit_lines(
        &mut self,
        command: &EditLines,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let state = self.editor.read(cx);
        if !state.focus_handle(cx).is_focused(window)
            || (!state.is_editable() && *command != EditLines::Select)
        {
            return;
        }
        let Some(edit) = transform(
            &state.text().to_string(),
            state.selected_range(),
            &open.relative_path,
            *command,
        ) else {
            return;
        };
        self.editor.update(cx, |state, cx| {
            if let Some(text) = edit.text {
                state.edit_range(edit.range, &text, window, cx);
            }
            state.set_selected_range(edit.selection, cx);
        });
    }
}

#[derive(Debug)]
struct Edit {
    range: Range<usize>,
    text: Option<String>,
    selection: Range<usize>,
}

fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |i| i + 1)
}

fn transform(text: &str, selection: Range<usize>, path: &str, command: EditLines) -> Option<Edit> {
    if selection.start > selection.end
        || !text.is_char_boundary(selection.start)
        || !text.is_char_boundary(selection.end)
    {
        return None;
    }
    let start = line_start(text, selection.start);
    // A selection ending exactly at the next line's start excludes that line.
    let last = if !selection.is_empty()
        && text.as_bytes().get(selection.end.wrapping_sub(1)) == Some(&b'\n')
    {
        selection.end - 1
    } else {
        selection.end
    };
    let end = text[last..].find('\n').map_or(text.len(), |i| last + i + 1);
    let range = start..end;
    let block = &text[range.clone()];
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let result = |range, replacement: String, selection| {
        Some(Edit {
            range,
            text: Some(replacement),
            selection,
        })
    };
    match command {
        EditLines::Select => Some(Edit {
            range: range.clone(),
            text: None,
            selection: range,
        }),
        EditLines::Delete => {
            let start = if end == text.len() && !block.ends_with('\n') && start > 0 {
                start
                    - if text[..start].ends_with("\r\n") {
                        2
                    } else {
                        1
                    }
            } else {
                start
            };
            result(start..end, String::new(), start..start)
        }
        EditLines::DuplicateUp | EditLines::DuplicateDown => {
            let separator = if block.ends_with('\n') { "" } else { eol };
            let delta = block.len() + separator.len();
            let offset = if command == EditLines::DuplicateDown {
                delta
            } else {
                0
            };
            result(
                range,
                format!("{block}{separator}{block}"),
                selection.start + offset..selection.end + offset,
            )
        }
        EditLines::MoveUp | EditLines::MoveDown => {
            if block.is_empty() && command == EditLines::MoveUp && start > 0 {
                let previous = line_start(text, start - 1);
                let previous_line = &text[previous..start];
                let ending = if previous_line.ends_with("\r\n") {
                    "\r\n"
                } else {
                    "\n"
                };
                return result(
                    previous..end,
                    format!("{ending}{}", previous_line.strip_suffix(ending).unwrap()),
                    previous..previous,
                );
            }
            let (range, before, after, offset) = if command == EditLines::MoveUp {
                if start == 0 {
                    return None;
                }
                let previous = line_start(text, start - 1);
                (
                    previous..end,
                    block,
                    &text[previous..start],
                    previous as isize - start as isize,
                )
            } else {
                if end == text.len() {
                    return None;
                }
                let next = text[end..].find('\n').map_or(text.len(), |i| end + i + 1);
                (start..next, &text[end..next], block, (next - end) as isize)
            };
            let separator = if after.ends_with("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            let replacement = if before.ends_with('\n') {
                format!("{before}{after}")
            } else {
                format!(
                    "{before}{separator}{}",
                    after.strip_suffix(separator).unwrap_or(after)
                )
            };
            // Moving past a final line without EOL needs a separator before the moved line.
            let offset = offset
                + if command == EditLines::MoveDown && !before.ends_with('\n') {
                    separator.len() as isize
                } else {
                    0
                };
            let a = selection.start.saturating_add_signed(offset);
            let b = selection
                .end
                .saturating_add_signed(offset)
                .min(range.start + replacement.len());
            result(range, replacement, a..b)
        }
        EditLines::InsertAbove | EditLines::InsertBelow => {
            let indent: String = text[start..]
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
            let above = command == EditLines::InsertAbove;
            let pos = if above { start } else { end };
            let missing_eol = !above && !block.ends_with('\n');
            let replacement = if missing_eol {
                format!("{eol}{indent}")
            } else {
                format!("{indent}{eol}")
            };
            let caret = pos + indent.len() + if missing_eol { eol.len() } else { 0 };
            result(pos..pos, replacement, caret..caret)
        }
        EditLines::Comment => {
            if matches!(
                language_for_path(path).as_str(),
                "tsx" | "jsx" | "js" | "mjs" | "cjs"
            ) && let Some(edit) = jsx_comment(text, selection.clone(), range.clone(), path)
            {
                return Some(edit);
            }
            let (prefix, suffix) = match language_for_path(path).as_str() {
                "py" | "pyi" | "rb" | "ruby" | "sh" | "bash" | "zsh" | "fish" | "yaml" | "yml"
                | "toml" | "make" | "cmake" | "r" | "pl" => ("#", ""),
                "sql" | "lua" | "hs" => ("--", ""),
                "html" | "xml" | "svg" | "md" => ("<!--", " -->"),
                "css" | "scss" | "sass" => ("/*", " */"),
                "rs" | "ts" | "mts" | "cts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "go" | "c"
                | "h" | "cpp" | "cc" | "cxx" | "hh" | "hxx" | "hpp" | "java" | "kt" | "swift"
                | "cs" | "dart" | "jsonc" => ("//", ""),
                // Don't introduce invalid comments into JSON or unknown formats.
                _ => return None,
            };
            let uncomment = block.lines().filter(|l| !l.trim().is_empty()).all(|l| {
                l.trim_start().starts_with(prefix) && l.trim_end().ends_with(suffix.trim_start())
            });
            let mut replacement = String::new();
            let mut edits = Vec::new();
            let mut offset = start;
            for line in block.split_inclusive('\n') {
                let content = line.trim_end_matches(['\r', '\n']);
                let indent = content.len() - content.trim_start_matches([' ', '\t']).len();
                let mut body = content[indent..].to_string();
                if !body.is_empty() {
                    if uncomment {
                        let remove =
                            prefix.len() + usize::from(body[prefix.len()..].starts_with(' '));
                        edits.push((offset + indent, remove, 0));
                        body = body[remove..].to_string();
                        if !suffix.is_empty() {
                            let end = body.trim_end().len();
                            let without = end - suffix.trim_start().len();
                            let without = without - usize::from(body[..without].ends_with(' '));
                            edits.push((
                                offset + indent + remove + without,
                                body.len() - without,
                                0,
                            ));
                            body.truncate(without);
                        }
                    } else {
                        edits.push((offset + indent, 0, prefix.len() + 1));
                        if !suffix.is_empty() {
                            edits.push((offset + content.len(), 0, suffix.len()));
                        }
                        body = format!("{prefix} {body}{suffix}");
                    }
                }
                replacement.push_str(&content[..indent]);
                replacement.push_str(&body);
                replacement.push_str(&line[content.len()..]);
                offset += line.len();
            }
            let map = |pos: usize| {
                let mut delta = 0isize;
                for &(at, removed, added) in &edits {
                    if pos < at {
                        break;
                    }
                    if pos < at + removed {
                        return at.saturating_add_signed(delta) + added;
                    }
                    delta += added as isize - removed as isize;
                }
                pos.saturating_add_signed(delta)
            };
            result(range, replacement, map(selection.start)..map(selection.end))
        }
    }
}

/// JSX children need one expression comment around the selected block, not
/// JavaScript line comments (which would render as literal text in React).
fn jsx_comment(
    text: &str,
    selection: Range<usize>,
    range: Range<usize>,
    path: &str,
) -> Option<Edit> {
    use gpui_component::highlighter::SyntaxHighlighter;
    let block = &text[range.clone()];
    let content = block.trim_end_matches(['\r', '\n']);
    let indent = content.len() - content.trim_start_matches([' ', '\t']).len();
    let body = &content[indent..];
    let uncomment = body.starts_with("{/*") && body.ends_with("*/}");
    let unchanged = || Edit {
        range: range.clone(),
        text: None,
        selection: selection.clone(),
    };
    if !uncomment {
        let mut parser = SyntaxHighlighter::new(&language_for_path(path));
        parser.update(None, &ropey::Rope::from(text), None);
        let tree = parser.tree()?;
        let start = range.start + indent;
        let end = range.start + content.trim_end().len();
        let mut node = tree.root_node().descendant_for_byte_range(start, end)?;
        loop {
            if node.kind() == "jsx_expression" {
                return None;
            }
            if matches!(
                node.kind(),
                "jsx_opening_element" | "jsx_closing_element" | "jsx_self_closing_element"
            ) && (start > node.start_byte() || end < node.end_byte())
            {
                return Some(unchanged());
            }
            if node.kind() == "jsx_element"
                && let (Some(open), Some(close)) = (
                    node.child_by_field_name("open_tag"),
                    node.child_by_field_name("close_tag"),
                )
                && start >= open.end_byte()
                && end <= close.start_byte()
            {
                break;
            }
            node = node.parent()?;
        }
        // A nested block comment cannot be safely wrapped.
        if body.contains("*/") {
            return Some(unchanged());
        }
    }
    let removed_prefix = 3 + usize::from(body.starts_with("{/* "));
    let body = if uncomment {
        let end = body.len() - 3 - usize::from(body.ends_with(" */}"));
        body[removed_prefix..end].to_string()
    } else {
        format!("{{/* {body} */}}")
    };
    let replacement = format!("{}{body}{}", &content[..indent], &block[content.len()..]);
    let map = |pos: usize| {
        let pos = if pos < range.start + indent {
            pos
        } else if uncomment {
            pos.saturating_sub(removed_prefix).max(range.start + indent)
        } else {
            pos + 4
        };
        pos.min(range.start + replacement.len())
    };
    let mapped = map(selection.start)..map(selection.end);
    Some(Edit {
        range,
        text: Some(replacement),
        selection: mapped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[gpui::test]
    fn shortcuts_dispatch_undo_and_respect_readonly(cx: &mut gpui::TestAppContext) {
        struct Probe {
            editor: Entity<EditorState>,
            readonly: bool,
        }
        impl Render for Probe {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                div()
                    .size_full()
                    .on_action(cx.listener(|p, command: &EditLines, w, cx| {
                        p.editor.update(cx, |s, cx| {
                            if !s.is_editable() {
                                return;
                            }
                            if let Some(edit) = transform(
                                &s.text().to_string(),
                                s.selected_range(),
                                "main.rs",
                                *command,
                            ) {
                                if let Some(text) = edit.text {
                                    s.edit_range(edit.range, &text, w, cx);
                                }
                                s.set_selected_range(edit.selection, cx);
                            }
                        });
                    }))
                    .child(
                        Editor::new(&self.editor)
                            .readonly(self.readonly)
                            .size_full(),
                    )
            }
        }
        cx.update(|cx| {
            gpui_component::init(cx);
            init(cx);
        });
        let (root, cx) = cx.add_window_view(|w, cx| Probe {
            readonly: false,
            editor: cx.new(|cx| {
                let mut state = EditorState::new(w, cx);
                let mut context = gpui::KeyContext::default();
                context.add("Editor");
                state.set_extra_key_context(Some(context), cx);
                state.set_value("let café = 1;\nnext", w, cx);
                state.focus(w, cx);
                state
            }),
        });
        let editor = root.read_with(cx, |p, _| p.editor.clone());
        cx.simulate_keystrokes("secondary-/");
        assert_eq!(
            editor.read_with(cx, |s, _| s.value()),
            "// let café = 1;\nnext"
        );
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(
            editor.read_with(cx, |s, _| s.value()),
            "let café = 1;\nnext"
        );
        cx.simulate_keystrokes("alt-shift-down");
        assert_eq!(
            editor.read_with(cx, |s, _| s.value()),
            "let café = 1;\nlet café = 1;\nnext"
        );
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(
            editor.read_with(cx, |s, _| s.value()),
            "let café = 1;\nnext"
        );
        cx.update(|w, cx| {
            editor.update(cx, |s, cx| {
                s.set_readonly(true, cx);
                s.edit_range(0..0, "must not change", w, cx);
            })
        });
        root.update(cx, |p, cx| {
            p.readonly = true;
            cx.notify();
        });
        cx.simulate_keystrokes("secondary-/ secondary-shift-k");
        assert_eq!(
            editor.read_with(cx, |s, _| s.value()),
            "let café = 1;\nnext"
        );
    }
    fn apply(
        text: &str,
        selection: Range<usize>,
        path: &str,
        command: EditLines,
    ) -> (String, Range<usize>) {
        let edit = transform(text, selection, path, command).unwrap();
        let mut text = text.to_string();
        if let Some(replacement) = edit.text {
            text.replace_range(edit.range, &replacement);
        }
        assert!(text.is_char_boundary(edit.selection.start));
        assert!(text.is_char_boundary(edit.selection.end));
        (text, edit.selection)
    }
    #[test]
    fn language_support_jsx_comments_wrap_children_once_and_round_trip() {
        for path in ["App.tsx", "App.jsx"] {
            let text = "function App() {\n  return <main>\n    <h1>Héllo</h1>\n    <button>Go</button>\n  </main>;\n}\n";
            let start = text.find("    <h1>").unwrap();
            let end = text.find("  </main>").unwrap();
            let (commented, selection) = apply(text, start..end, path, EditLines::Comment);
            assert!(
                commented.contains("{/* <h1>Héllo</h1>\n    <button>Go</button> */}"),
                "{commented}"
            );
            let mut parser =
                gpui_component::highlighter::SyntaxHighlighter::new(&language_for_path(path));
            parser.update(None, &ropey::Rope::from(commented.as_str()), None);
            assert!(!parser.tree().unwrap().root_node().has_error());
            assert_eq!(
                apply(&commented, selection, path, EditLines::Comment).0,
                text
            );
        }
        let source = "const café = 1;\nconst next = 2;\n";
        assert_eq!(
            apply(source, 0..0, "App.tsx", EditLines::Comment).0,
            "// const café = 1;\nconst next = 2;\n"
        );
    }

    #[test]
    fn comments_preserve_unicode_selection_and_line_endings() {
        let text = "  let café = 1;\r\n  let 世界 = 2;\r\nnext";
        let selection = 2..text.find("next").unwrap();
        let (commented, selected) = apply(text, selection.clone(), "main.rs", EditLines::Comment);
        assert_eq!(
            commented,
            "  // let café = 1;\r\n  // let 世界 = 2;\r\nnext"
        );
        assert_eq!(
            apply(&commented, selected, "main.rs", EditLines::Comment),
            (text.into(), selection)
        );
    }
    #[test]
    fn comments_use_language_syntax_and_skip_unknown_formats() {
        for (path, expected) in [
            ("a.py", "# value"),
            ("a.sql", "-- value"),
            ("a.css", "/* value */"),
            ("a.html", "<!-- value -->"),
        ] {
            let (text, range) = apply("value", 0..5, path, EditLines::Comment);
            assert_eq!(text, expected);
            assert_eq!(apply(&text, range, path, EditLines::Comment).0, "value");
        }
        assert!(transform("{}", 0..0, "a.json", EditLines::Comment).is_none());
    }
    #[test]
    fn moves_and_duplicates_final_lines_without_losing_newlines() {
        assert_eq!(
            apply("one\n世界", 4..10, "a.rs", EditLines::MoveUp),
            ("世界\none".into(), 0..6)
        );
        assert_eq!(
            apply("one\n世界", 0..3, "a.rs", EditLines::MoveDown),
            ("世界\none".into(), 7..10)
        );
        assert_eq!(
            apply("one\ntwo", 4..7, "a.rs", EditLines::DuplicateDown),
            ("one\ntwo\ntwo".into(), 8..11)
        );
        assert_eq!(
            apply("one\ntwo\nthree", 0..8, "a.rs", EditLines::MoveDown).0,
            "three\none\ntwo"
        );
    }
    #[test]
    fn insert_delete_and_select_lines() {
        assert_eq!(
            apply("a\r\nb\nc", 5..5, "a.rs", EditLines::Delete).0,
            "a\r\nb"
        );
        assert_eq!(
            apply("a\r\nb\nc", 3..4, "a.rs", EditLines::MoveUp).0,
            "b\na\r\nc"
        );
        assert_eq!(
            apply("a\nb\n", 4..4, "a.rs", EditLines::MoveUp),
            ("a\n\nb".into(), 2..2)
        );
        assert_eq!(
            apply("  one\ntwo", 3..3, "a.rs", EditLines::InsertBelow),
            ("  one\n  \ntwo".into(), 8..8)
        );
        assert_eq!(
            apply("  one", 3..3, "a.rs", EditLines::InsertAbove),
            ("  \n  one".into(), 2..2)
        );
        assert_eq!(
            apply("one\ntwo", 4..4, "a.rs", EditLines::Delete),
            ("one".into(), 3..3)
        );
        assert_eq!(apply("one\ntwo", 0..4, "a.rs", EditLines::Select).1, 0..4);
    }
}
