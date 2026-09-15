use super::*;
use gpui::EntityInputHandler as _;

struct Probe(Entity<EditorState>);
impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .on_action(cx.listener(|p, _: &SelectNextOccurrence, _, cx| {
                p.0.update(cx, |s, cx| s.select_next_occurrence(false, cx))
            }))
            .on_action(|_: &crate::chat::DiffToggle, _, _| {})
            .child(Editor::new(&self.0).size_full())
    }
}

#[gpui::test]
fn stock_diff_binding_cannot_override_editor_multi_cursor(cx: &mut gpui::TestAppContext) {
    let config: vitre_contracts::ResolvedKeybindingsConfig = serde_json::from_value(serde_json::json!([{
        "command":"diff.toggle", "shortcut":{"key":"d","modKey":true,"metaKey":false,"ctrlKey":false,"shiftKey":false,"altKey":false},
        "whenAst":{"type":"not","node":{"type":"identifier","name":"terminalFocus"}}
    }])).unwrap();
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.bind_keys([gpui::KeyBinding::new(
            "secondary-d",
            SelectNextOccurrence,
            Some("Editor && !vim_command"),
        )]);
        crate::keymap::install(&config, cx);
    });
    let (root, cx) = cx.add_window_view(|w, cx| {
        Probe(cx.new(|cx| {
            let mut state = EditorState::new(w, cx);
            let mut context = gpui::KeyContext::default();
            context.add("Editor");
            state.set_extra_key_context(Some(context), cx);
            state.set_value("one one", w, cx);
            state.set_selected_range(0..3, cx);
            state.focus(w, cx);
            state
        }))
    });
    cx.simulate_keystrokes("secondary-d");
    assert_eq!(
        root.read_with(cx, |p, cx| p.0.read(cx).selection_count()),
        2
    );
    // Reinstalling a server keymap must not unbind the editor action.
    cx.update(|_, cx| crate::keymap::install(&config, cx));
    cx.simulate_keystrokes("escape secondary-d");
    assert_eq!(
        root.read_with(cx, |p, cx| p.0.read(cx).selection_count()),
        2
    );
}

#[gpui::test]
fn multiple_cursors_type_delete_paste_and_undo_atomically(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_component::init);
    let (root, cx) = cx.add_window_view(|w, cx| {
        Probe(cx.new(|cx| {
            let mut state = EditorState::new(w, cx);
            state.set_value("one 😀 one", w, cx);
            state.focus(w, cx);
            state
        }))
    });
    let editor = root.read_with(cx, |p, _| p.0.clone());
    cx.update(|w, cx| {
        editor.update(cx, |s, cx| {
            s.set_selected_range(0..3, cx);
            s.select_next_occurrence(true, cx);
            assert_eq!(s.selection_count(), 2);
            s.replace_text_in_range(None, "x", w, cx);
            assert_eq!(s.value(), "x 😀 x");
            assert_eq!(s.selection_count(), 2);
            s.replace_text_in_range(None, "y", w, cx);
            assert_eq!(s.value(), "xy 😀 xy");
        })
    });
    cx.simulate_keystrokes("backspace");
    assert_eq!(editor.read_with(cx, |s, _| s.value()), "x 😀 x");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(editor.read_with(cx, |s, _| s.value()), "xy 😀 xy");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(editor.read_with(cx, |s, _| s.value()), "x 😀 x");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(editor.read_with(cx, |s, _| s.value()), "one 😀 one");
}

#[gpui::test]
fn snippet_mirrors_and_tabstops_track_unicode_edits(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_component::init);
    let (root, cx) = cx.add_window_view(|w, cx| {
        Probe(cx.new(|cx| {
            let state = EditorState::new(w, cx);
            state.focus(w, cx);
            state
        }))
    });
    let editor = root.read_with(cx, |p, _| p.0.clone());
    cx.update(|w, cx| {
        editor.update(cx, |s, cx| {
            s.insert_snippet("<${1:div}>${2:content}</$1>$0", w, cx)
                .unwrap();
            assert_eq!(s.value(), "<div>content</div>");
            assert_eq!(s.selection_count(), 2);
            s.replace_text_in_range(None, "main", w, cx);
            assert_eq!(s.value(), "<main>content</main>");
        })
    });
    cx.simulate_keystrokes("tab");
    cx.update(|w, cx| {
        editor.update(cx, |s, cx| {
            assert_eq!(s.text().slice(s.selected_range()).to_string(), "content");
            s.replace_text_in_range(None, "😀", w, cx);
            assert_eq!(s.value(), "<main>😀</main>");
        })
    });
    cx.simulate_keystrokes("tab");
    cx.update(|_, cx| {
        editor.update(cx, |s, _| {
            assert_eq!(s.cursor(), s.text().len());
            assert_eq!(s.selection_count(), 1);
        })
    });
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(
        editor.read_with(cx, |s, _| s.value()),
        "<main>content</main>"
    );
}

#[gpui::test]
fn overlapping_matches_and_adjacent_snippet_stops_stay_valid(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_component::init);
    let (root, cx) = cx.add_window_view(|w, cx| {
        Probe(cx.new(|cx| {
            let state = EditorState::new(w, cx);
            state.focus(w, cx);
            state
        }))
    });
    let editor = root.read_with(cx, |p, _| p.0.clone());
    cx.update(|w, cx| {
        editor.update(cx, |s, cx| {
            s.set_value("aaaaa", w, cx);
            s.set_selected_range(1..3, cx);
            s.select_next_occurrence(true, cx);
            assert_eq!(
                s.selection_count(),
                1,
                "overlapping matches must not become selections"
            );
            s.replace_text_in_range(None, "x", w, cx);
            assert_eq!(s.value(), "axaa");
            s.set_value("", w, cx);
            s.insert_snippet("${1:a}${2:b}$0", w, cx).unwrap();
            s.replace_text_in_range(None, "hello", w, cx);
            assert_eq!(s.value(), "hellob");
        })
    });
    cx.simulate_keystrokes("tab");
    cx.update(|w, cx| {
        editor.update(cx, |s, cx| {
            assert_eq!(s.text().slice(s.selected_range()).to_string(), "b");
            s.replace_text_in_range(None, "😀", w, cx);
            assert_eq!(s.value(), "hello😀");
        })
    });
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(
        editor.read_with(cx, |s, _| s.text().slice(s.selected_range()).to_string()),
        "hello"
    );
    cx.update(|w, cx| {
        editor.update(cx, |s, cx| {
            s.set_value("", w, cx);
            s.insert_snippet("$1$2$0", w, cx).unwrap();
            s.replace_text_in_range(None, "one", w, cx);
        })
    });
    cx.simulate_keystrokes("tab");
    cx.update(|w, cx| {
        editor.update(cx, |s, cx| {
            assert_eq!(s.selected_range(), 3..3);
            s.replace_text_in_range(None, "two", w, cx);
            assert_eq!(s.value(), "onetwo");
        })
    });
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(
        editor.read_with(cx, |s, _| s.text().slice(s.selected_range()).to_string()),
        "one"
    );
}
