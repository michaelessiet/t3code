//! The command surface, driven the way a user drives it: feed keys, apply the
//! effects to a `String`, assert on the resulting buffer and caret.

use super::*;

/// A document the tests can type into, mirroring what the glue layer does
/// with the real editor.
struct Buffer {
    text: String,
    engine: VimEngine,
    visible_lines: std::ops::Range<usize>,
}

impl Buffer {
    fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            engine: VimEngine::default(),
            visible_lines: 0..20,
        }
    }

    /// Start with the caret on the character at `offset`.
    fn at(mut self, offset: usize) -> Self {
        let text = self.text.clone();
        self.engine.sync_cursor(&text, offset);
        self
    }

    fn with_shift_width(mut self, width: usize) -> Self {
        self.engine.set_config(VimConfig {
            shift_width: width,
            use_tabs: false,
        });
        self
    }

    /// Feed a key sequence. `<esc>` and `<cr>` name the non-printing keys;
    /// everything else is one character per key, including in insert mode
    /// where the engine passes the key through and the test types it in.
    fn keys(&mut self, sequence: &str) -> &mut Self {
        for key in parse_keys(sequence) {
            self.key(key);
        }
        self
    }

    fn key(&mut self, key: VimKey) {
        let response = {
            let document = VimDocument::new(&self.text, self.visible_lines.clone());
            self.engine.handle_key(&document, key.clone())
        };
        for effect in &response.effects {
            if let VimEffect::Edit {
                range,
                text,
                cursor: _,
            } = effect
            {
                self.text.replace_range(range.clone(), text);
            }
        }
        if !response.handled {
            // Insert mode: the editor would have inserted the character, so
            // the test does it and tells the engine where the caret landed.
            let inserted = match key {
                VimKey::Char(c) => Some(c.to_string()),
                VimKey::Enter => Some("\n".to_string()),
                VimKey::Tab => Some("\t".to_string()),
                _ => None,
            };
            if let Some(inserted) = inserted {
                let at = self.engine.cursor();
                self.text.insert_str(at, &inserted);
                let text = self.text.clone();
                self.engine.sync_cursor(&text, at + inserted.len());
            }
        }
    }

    fn cursor(&self) -> usize {
        self.engine.cursor()
    }

    fn mode(&self) -> VimMode {
        self.engine.mode()
    }

    fn selection(&self) -> std::ops::Range<usize> {
        self.engine.selection(&self.text)
    }
}

fn parse_keys(sequence: &str) -> Vec<VimKey> {
    let mut keys = Vec::new();
    let mut rest = sequence;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix("<esc>") {
            keys.push(VimKey::Escape);
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("<cr>") {
            keys.push(VimKey::Enter);
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("<bs>") {
            keys.push(VimKey::Backspace);
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("<c-") {
            let (name, tail) = tail.split_once('>').expect("unterminated <c-…>");
            keys.push(VimKey::Ctrl(name.chars().next().expect("empty <c-…>")));
            rest = tail;
        } else {
            let c = rest.chars().next().expect("non-empty");
            keys.push(VimKey::Char(c));
            rest = &rest[c.len_utf8()..];
        }
    }
    keys
}

// ---- motions --------------------------------------------------------------

#[test]
fn hjkl_move_within_bounds() {
    let mut buffer = Buffer::new("abc\ndefgh\nij").at(0);
    buffer.keys("ll");
    assert_eq!(buffer.cursor(), 2);
    // `l` will not step onto the newline.
    buffer.keys("l");
    assert_eq!(buffer.cursor(), 2);
    buffer.keys("j");
    assert_eq!(buffer.cursor(), 6);
    buffer.keys("hh");
    assert_eq!(buffer.cursor(), 4);
    // `h` will not wrap to the previous line.
    buffer.keys("h");
    assert_eq!(buffer.cursor(), 4);
}

#[test]
fn vertical_motion_keeps_the_goal_column() {
    let mut buffer = Buffer::new("abcdef\nxy\nghijkl").at(0);
    buffer.keys("$");
    assert_eq!(buffer.cursor(), 5);
    buffer.keys("j");
    // Short line: clamp to its last character.
    assert_eq!(buffer.cursor(), 8);
    buffer.keys("j");
    // Long line again: the goal column comes back.
    assert_eq!(buffer.cursor(), 15);
}

#[test]
fn counts_multiply_motions() {
    let mut buffer = Buffer::new("one two three four").at(0);
    buffer.keys("3w");
    assert_eq!(buffer.cursor(), 14);
    buffer.keys("2b");
    assert_eq!(buffer.cursor(), 4);
}

#[test]
fn word_motions_respect_character_classes() {
    let mut buffer = Buffer::new("foo.bar baz").at(0);
    buffer.keys("w");
    assert_eq!(buffer.cursor(), 3, "punctuation is its own word");
    buffer.keys("w");
    assert_eq!(buffer.cursor(), 4);
    buffer.keys("w");
    assert_eq!(buffer.cursor(), 8);

    let mut big = Buffer::new("foo.bar baz").at(0);
    big.keys("W");
    assert_eq!(big.cursor(), 8, "WORD spans the punctuation");
}

#[test]
fn word_end_and_back_land_on_the_right_characters() {
    let mut buffer = Buffer::new("alpha beta").at(0);
    buffer.keys("e");
    assert_eq!(buffer.cursor(), 4);
    buffer.keys("e");
    assert_eq!(buffer.cursor(), 9);
    buffer.keys("b");
    assert_eq!(buffer.cursor(), 6);
    buffer.keys("b");
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn word_motion_stops_on_empty_lines() {
    let mut buffer = Buffer::new("foo\n\nbar").at(0);
    buffer.keys("w");
    assert_eq!(buffer.cursor(), 4, "the empty line is a word");
    buffer.keys("w");
    assert_eq!(buffer.cursor(), 5);
}

#[test]
fn line_anchors() {
    let mut buffer = Buffer::new("  indented text").at(9);
    buffer.keys("0");
    assert_eq!(buffer.cursor(), 0);
    buffer.keys("^");
    assert_eq!(buffer.cursor(), 2);
    buffer.keys("$");
    assert_eq!(buffer.cursor(), 14);
}

#[test]
fn gg_and_g_jump_to_lines() {
    let mut buffer = Buffer::new("one\ntwo\nthree\nfour").at(0);
    buffer.keys("G");
    assert_eq!(buffer.cursor(), 14);
    buffer.keys("gg");
    assert_eq!(buffer.cursor(), 0);
    buffer.keys("3G");
    assert_eq!(buffer.cursor(), 8);
    buffer.keys("2gg");
    assert_eq!(buffer.cursor(), 4);
}

#[test]
fn find_char_and_repeat() {
    let mut buffer = Buffer::new("a.b.c.d").at(0);
    buffer.keys("f.");
    assert_eq!(buffer.cursor(), 1);
    buffer.keys(";");
    assert_eq!(buffer.cursor(), 3);
    buffer.keys(",");
    assert_eq!(buffer.cursor(), 1);
    buffer.keys("t.");
    assert_eq!(buffer.cursor(), 2, "t stops before the match");
    buffer.keys(";");
    assert_eq!(buffer.cursor(), 4, "repeat steps past the adjacent match");
}

#[test]
fn percent_matches_brackets() {
    let mut buffer = Buffer::new("fn f(a, (b)) {}").at(4);
    buffer.keys("%");
    assert_eq!(buffer.cursor(), 11, "nesting is honoured");
    buffer.keys("%");
    assert_eq!(buffer.cursor(), 4);
}

#[test]
fn paragraph_motions() {
    let mut buffer = Buffer::new("a\nb\n\nc\nd\n\ne").at(0);
    buffer.keys("}");
    assert_eq!(buffer.cursor(), 4);
    buffer.keys("}");
    assert_eq!(buffer.cursor(), 9);
    buffer.keys("{");
    assert_eq!(buffer.cursor(), 4);
}

// ---- operators ------------------------------------------------------------

#[test]
fn dw_deletes_to_the_next_word() {
    let mut buffer = Buffer::new("one two three").at(0);
    buffer.keys("dw");
    assert_eq!(buffer.text, "two three");
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn de_keeps_the_trailing_space() {
    let mut buffer = Buffer::new("one two").at(0);
    buffer.keys("de");
    assert_eq!(buffer.text, " two");
}

#[test]
fn dd_removes_the_whole_line_including_its_newline() {
    let mut buffer = Buffer::new("one\ntwo\nthree").at(4);
    buffer.keys("dd");
    assert_eq!(buffer.text, "one\nthree");
    assert_eq!(buffer.cursor(), 4);
}

#[test]
fn dd_on_the_last_line_takes_the_preceding_newline() {
    let mut buffer = Buffer::new("one\ntwo").at(4);
    buffer.keys("dd");
    assert_eq!(buffer.text, "one");
}

#[test]
fn counted_dd_removes_several_lines() {
    let mut buffer = Buffer::new("a\nb\nc\nd").at(0);
    buffer.keys("3dd");
    assert_eq!(buffer.text, "d");
}

#[test]
fn operator_counts_multiply() {
    let mut buffer = Buffer::new("a b c d e f g").at(0);
    buffer.keys("2d3w");
    assert_eq!(buffer.text, "g");
}

#[test]
fn cw_behaves_like_ce() {
    let mut buffer = Buffer::new("one two").at(0);
    buffer.keys("cw");
    assert_eq!(buffer.text, " two", "the space survives, unlike dw");
    assert_eq!(buffer.mode(), VimMode::Insert);
}

#[test]
fn change_enters_insert_and_typing_lands() {
    let mut buffer = Buffer::new("one two").at(0);
    buffer.keys("cwtwo<esc>");
    assert_eq!(buffer.text, "two two");
    assert_eq!(buffer.mode(), VimMode::Normal);
}

#[test]
fn cc_keeps_the_indentation() {
    let mut buffer = Buffer::new("fn f() {\n    body();\n}").at(13);
    buffer.keys("cc");
    assert_eq!(buffer.text, "fn f() {\n    \n}");
    assert_eq!(buffer.mode(), VimMode::Insert);
    assert_eq!(buffer.cursor(), 13);
}

#[test]
fn d_dollar_and_capital_d_delete_to_end_of_line() {
    let mut buffer = Buffer::new("hello world\nnext").at(5);
    buffer.keys("d$");
    assert_eq!(buffer.text, "hello\nnext");

    let mut shorthand = Buffer::new("hello world\nnext").at(5);
    shorthand.keys("D");
    assert_eq!(shorthand.text, "hello\nnext");
}

#[test]
fn yank_and_paste_characterwise() {
    let mut buffer = Buffer::new("abc").at(0);
    buffer.keys("yl");
    assert_eq!(buffer.cursor(), 0, "yank leaves the caret at the start");
    buffer.keys("p");
    assert_eq!(buffer.text, "aabc");
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn yank_and_paste_linewise() {
    let mut buffer = Buffer::new("one\ntwo").at(0);
    buffer.keys("yyp");
    assert_eq!(buffer.text, "one\none\ntwo");
    assert_eq!(buffer.cursor(), 4);
}

#[test]
fn linewise_paste_before() {
    let mut buffer = Buffer::new("one\ntwo").at(4);
    buffer.keys("yyP");
    assert_eq!(buffer.text, "one\ntwo\ntwo");
}

#[test]
fn linewise_paste_on_the_last_line_opens_a_new_one() {
    let mut buffer = Buffer::new("one\ntwo").at(0);
    buffer.keys("yy");
    buffer.keys("j");
    buffer.keys("p");
    assert_eq!(buffer.text, "one\ntwo\none");
}

#[test]
fn delete_fills_the_unnamed_register() {
    let mut buffer = Buffer::new("abc def").at(0);
    buffer.keys("dw");
    buffer.keys("$p");
    assert_eq!(buffer.text, "defabc ");
}

#[test]
fn named_registers_round_trip() {
    let mut buffer = Buffer::new("alpha\nbeta").at(0);
    buffer.keys("\"ayy");
    buffer.keys("j");
    buffer.keys("\"ap");
    assert_eq!(buffer.text, "alpha\nbeta\nalpha");
}

#[test]
fn indent_and_outdent() {
    let mut buffer = Buffer::new("a\nb\nc").with_shift_width(2).at(0);
    buffer.keys("2>>");
    assert_eq!(buffer.text, "  a\n  b\nc");
    buffer.keys("<<");
    assert_eq!(buffer.text, "a\n  b\nc");
}

#[test]
fn case_operators() {
    let mut buffer = Buffer::new("hello world").at(0);
    buffer.keys("gUw");
    assert_eq!(buffer.text, "HELLO world");
    buffer.keys("guw");
    assert_eq!(buffer.text, "hello world");
}

// ---- text objects ---------------------------------------------------------

#[test]
fn inner_word_object() {
    let mut buffer = Buffer::new("foo bar baz").at(5);
    buffer.keys("diw");
    assert_eq!(buffer.text, "foo  baz");
}

#[test]
fn around_word_object_takes_the_trailing_space() {
    let mut buffer = Buffer::new("foo bar baz").at(4);
    buffer.keys("daw");
    assert_eq!(buffer.text, "foo baz");
}

#[test]
fn bracket_objects() {
    let mut buffer = Buffer::new("call(a, b)").at(6);
    buffer.keys("di(");
    assert_eq!(buffer.text, "call()");

    let mut around = Buffer::new("call(a, b)").at(6);
    around.keys("da(");
    assert_eq!(around.text, "call");
}

#[test]
fn nested_bracket_objects_take_the_innermost_pair() {
    let mut buffer = Buffer::new("f(g(x), y)").at(4);
    buffer.keys("di(");
    assert_eq!(buffer.text, "f(g(), y)");
}

#[test]
fn quote_objects() {
    let mut buffer = Buffer::new("let s = \"hello\";").at(10);
    buffer.keys("ci\"bye<esc>");
    assert_eq!(buffer.text, "let s = \"bye\";");
}

#[test]
fn quote_object_works_from_before_the_string() {
    let mut buffer = Buffer::new("let s = \"hello\";").at(0);
    buffer.keys("di\"");
    assert_eq!(buffer.text, "let s = \"\";");
}

#[test]
fn paragraph_object() {
    let mut buffer = Buffer::new("a\nb\n\nc").at(0);
    buffer.keys("dip");
    // The two-line paragraph goes with its terminator, leaving the blank
    // separator and the next paragraph.
    assert_eq!(buffer.text, "\nc");
}

// ---- direct edits ---------------------------------------------------------

#[test]
fn x_deletes_under_the_caret_and_clamps() {
    let mut buffer = Buffer::new("abc").at(2);
    buffer.keys("x");
    assert_eq!(buffer.text, "ab");
    assert_eq!(buffer.cursor(), 1, "the caret cannot sit past the line end");
}

#[test]
fn counted_x_stops_at_the_line_end() {
    let mut buffer = Buffer::new("abc\ndef").at(1);
    buffer.keys("9x");
    assert_eq!(buffer.text, "a\ndef");
}

#[test]
fn r_replaces_one_character() {
    let mut buffer = Buffer::new("cat").at(0);
    buffer.keys("rb");
    assert_eq!(buffer.text, "bat");
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn counted_r_replaces_a_run() {
    let mut buffer = Buffer::new("abcd").at(0);
    buffer.keys("3rx");
    assert_eq!(buffer.text, "xxxd");
}

#[test]
fn tilde_toggles_case_and_advances() {
    let mut buffer = Buffer::new("abc").at(0);
    buffer.keys("~");
    assert_eq!(buffer.text, "Abc");
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn o_and_capital_o_open_lines_with_the_current_indentation() {
    let mut buffer = Buffer::new("    body").at(4);
    buffer.keys("onext<esc>");
    assert_eq!(buffer.text, "    body\n    next");

    let mut above = Buffer::new("    body").at(4);
    above.keys("Oprev<esc>");
    assert_eq!(above.text, "    prev\n    body");
}

#[test]
fn a_appends_after_the_caret() {
    let mut buffer = Buffer::new("ac").at(0);
    buffer.keys("ab<esc>");
    assert_eq!(buffer.text, "abc");
}

#[test]
fn capital_a_and_capital_i_go_to_the_line_ends() {
    let mut buffer = Buffer::new("  mid").at(3);
    buffer.keys("A!<esc>");
    assert_eq!(buffer.text, "  mid!");
    buffer.keys("I><esc>");
    assert_eq!(buffer.text, "  >mid!");
}

#[test]
fn s_substitutes_the_character_under_the_caret() {
    let mut buffer = Buffer::new("cat").at(0);
    buffer.keys("sb<esc>");
    assert_eq!(buffer.text, "bat");
}

#[test]
fn join_collapses_lines_with_a_single_space() {
    let mut buffer = Buffer::new("one\n    two\nthree").at(0);
    buffer.keys("J");
    assert_eq!(buffer.text, "one two\nthree");
}

#[test]
fn counted_join_takes_several_lines() {
    let mut buffer = Buffer::new("a\nb\nc\nd").at(0);
    buffer.keys("3J");
    assert_eq!(buffer.text, "a b c\nd");
}

#[test]
fn escape_from_insert_steps_the_caret_left() {
    let mut buffer = Buffer::new("ab").at(0);
    buffer.keys("ix");
    assert_eq!(buffer.cursor(), 1);
    buffer.keys("<esc>");
    assert_eq!(buffer.text, "xab");
    assert_eq!(buffer.cursor(), 0);
}

// ---- visual mode ----------------------------------------------------------

#[test]
fn visual_selection_is_inclusive() {
    let mut buffer = Buffer::new("abcdef").at(1);
    buffer.keys("vll");
    assert_eq!(buffer.mode(), VimMode::Visual);
    assert_eq!(buffer.selection(), 1..4);
    buffer.keys("d");
    assert_eq!(buffer.text, "aef");
    assert_eq!(buffer.mode(), VimMode::Normal);
}

#[test]
fn visual_line_mode_takes_whole_lines() {
    let mut buffer = Buffer::new("one\ntwo\nthree").at(5);
    buffer.keys("Vd");
    assert_eq!(buffer.text, "one\nthree");
}

#[test]
fn visual_o_swaps_the_ends() {
    let mut buffer = Buffer::new("abcdef").at(1);
    buffer.keys("vll");
    buffer.keys("o");
    assert_eq!(buffer.cursor(), 1);
    buffer.keys("h");
    assert_eq!(buffer.selection(), 0..4);
}

#[test]
fn visual_yank_then_paste() {
    let mut buffer = Buffer::new("abcd").at(0);
    buffer.keys("vly");
    assert_eq!(buffer.mode(), VimMode::Normal);
    buffer.keys("$p");
    assert_eq!(buffer.text, "abcdab");
}

#[test]
fn visual_text_object_extends_the_selection() {
    let mut buffer = Buffer::new("foo bar baz").at(5);
    buffer.keys("viw");
    assert_eq!(buffer.selection(), 4..7);
}

#[test]
fn escape_leaves_visual_mode() {
    let mut buffer = Buffer::new("abc").at(0);
    buffer.keys("vl<esc>");
    assert_eq!(buffer.mode(), VimMode::Normal);
    assert_eq!(buffer.selection(), 1..2);
}

// ---- search and ex --------------------------------------------------------

#[test]
fn search_forward_and_repeat() {
    let mut buffer = Buffer::new("foo bar foo baz foo").at(0);
    buffer.keys("/foo<cr>");
    assert_eq!(buffer.cursor(), 8);
    buffer.keys("n");
    assert_eq!(buffer.cursor(), 16);
    buffer.keys("n");
    assert_eq!(buffer.cursor(), 0, "search wraps");
    buffer.keys("N");
    assert_eq!(buffer.cursor(), 16);
}

#[test]
fn search_backward() {
    let mut buffer = Buffer::new("aXbXc").at(4);
    buffer.keys("?X<cr>");
    assert_eq!(buffer.cursor(), 3);
    buffer.keys("n");
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn star_searches_the_word_under_the_caret() {
    let mut buffer = Buffer::new("value = value + other").at(0);
    buffer.keys("*");
    assert_eq!(buffer.cursor(), 8);
}

#[test]
fn missing_pattern_reports_an_error() {
    let mut buffer = Buffer::new("abc").at(0);
    buffer.keys("/zzz<cr>");
    assert_eq!(buffer.cursor(), 0);
    assert_eq!(
        buffer.engine.status().message.as_deref(),
        Some("E486: Pattern not found: zzz")
    );
}

#[test]
fn ex_line_jump() {
    let mut buffer = Buffer::new("a\nb\nc\nd").at(0);
    buffer.keys(":3<cr>");
    assert_eq!(buffer.cursor(), 4);
    buffer.keys(":$<cr>");
    assert_eq!(buffer.cursor(), 6);
}

#[test]
fn ex_write_and_quit_emit_effects() {
    let mut buffer = Buffer::new("a").at(0);
    let document = VimDocument::new(&buffer.text, 0..1);
    for key in parse_keys(":wq") {
        buffer.engine.handle_key(&document, key);
    }
    let response = buffer.engine.handle_key(&document, VimKey::Enter);
    assert_eq!(
        response.effects,
        vec![VimEffect::Save, VimEffect::Quit],
        "`:wq` saves before it closes"
    );
}

#[test]
fn ex_substitute_on_the_current_line() {
    let mut buffer = Buffer::new("foo foo\nfoo").at(0);
    buffer.keys(":s/foo/bar/<cr>");
    assert_eq!(buffer.text, "bar foo\nfoo");
}

#[test]
fn ex_substitute_global_over_the_file() {
    let mut buffer = Buffer::new("foo foo\nfoo").at(0);
    buffer.keys(":%s/foo/bar/g<cr>");
    assert_eq!(buffer.text, "bar bar\nbar");
}

#[test]
fn ex_substitute_falls_back_to_a_literal_match() {
    let mut buffer = Buffer::new("a(b) c").at(0);
    buffer.keys(":s/(b/[b/<cr>");
    assert_eq!(buffer.text, "a[b) c", "an uncompilable pattern is literal");
}

#[test]
fn unknown_ex_command_reports_an_error() {
    let mut buffer = Buffer::new("a").at(0);
    buffer.keys(":nope<cr>");
    assert_eq!(
        buffer.engine.status().message.as_deref(),
        Some("E492: Not an editor command: nope")
    );
}

#[test]
fn escape_cancels_the_command_line() {
    let mut buffer = Buffer::new("a\nb\nc").at(0);
    buffer.keys(":3<esc>");
    assert_eq!(buffer.cursor(), 0);
    assert_eq!(buffer.engine.status().command_line, None);
}

#[test]
fn backspacing_an_empty_command_line_closes_it() {
    let mut buffer = Buffer::new("a").at(0);
    buffer.keys(":<bs>");
    assert_eq!(buffer.engine.status().command_line, None);
}

// ---- marks, undo, hover ---------------------------------------------------

#[test]
fn marks_round_trip() {
    let mut buffer = Buffer::new("one\ntwo\nthree").at(4);
    buffer.keys("ma");
    buffer.keys("G");
    assert_eq!(buffer.cursor(), 8);
    buffer.keys("`a");
    assert_eq!(buffer.cursor(), 4);
}

#[test]
fn jumping_to_an_unset_mark_reports_an_error() {
    let mut buffer = Buffer::new("a").at(0);
    buffer.keys("`z");
    assert_eq!(
        buffer.engine.status().message.as_deref(),
        Some("E20: Mark not set: z")
    );
}

#[test]
fn undo_and_redo_emit_effects() {
    let mut buffer = Buffer::new("abc").at(0);
    let document = VimDocument::new(&buffer.text, 0..1);
    let undo = buffer.engine.handle_key(&document, VimKey::Char('u'));
    assert!(undo.effects.contains(&VimEffect::Undo));
    let redo = buffer.engine.handle_key(&document, VimKey::Ctrl('r'));
    assert!(redo.effects.contains(&VimEffect::Redo));
}

#[test]
fn gh_and_gd_reach_the_language_server() {
    let mut buffer = Buffer::new("symbol").at(0);
    let document = VimDocument::new(&buffer.text, 0..1);
    buffer.engine.handle_key(&document, VimKey::Char('g'));
    let hover = buffer.engine.handle_key(&document, VimKey::Char('h'));
    assert!(hover.effects.contains(&VimEffect::ShowHover));

    buffer.engine.handle_key(&document, VimKey::Char('g'));
    let definition = buffer.engine.handle_key(&document, VimKey::Char('d'));
    assert!(definition.effects.contains(&VimEffect::GoToDefinition));

    let tag_jump = buffer.engine.handle_key(&document, VimKey::Ctrl(']'));
    assert!(tag_jump.effects.contains(&VimEffect::GoToDefinition));
}

// ---- repeat ---------------------------------------------------------------

#[test]
fn dot_repeats_a_delete() {
    let mut buffer = Buffer::new("one two three four").at(0);
    buffer.keys("dw");
    assert_eq!(buffer.text, "two three four");
    buffer.keys(".");
    assert_eq!(buffer.text, "three four");
}

#[test]
fn dot_repeats_a_change_including_the_typed_text() {
    let mut buffer = Buffer::new("aaa bbb").at(0);
    buffer.keys("ciwxxx<esc>");
    assert_eq!(buffer.text, "xxx bbb");
    buffer.keys("w");
    buffer.keys(".");
    assert_eq!(buffer.text, "xxx xxx");
}

#[test]
fn dot_repeats_x() {
    let mut buffer = Buffer::new("abcdef").at(0);
    buffer.keys("x");
    buffer.keys("..");
    assert_eq!(buffer.text, "def");
}

// ---- caret presentation ---------------------------------------------------

#[test]
fn normal_mode_paints_a_block_over_the_character() {
    let buffer = Buffer::new("abc").at(1);
    assert_eq!(buffer.selection(), 1..2);
}

#[test]
fn the_block_collapses_at_a_line_end() {
    let buffer = Buffer::new("ab\ncd").at(2);
    // The caret cannot rest on the newline, so it clamps back onto 'b'.
    assert_eq!(buffer.selection(), 1..2);
}

#[test]
fn an_empty_line_has_a_collapsed_caret() {
    let buffer = Buffer::new("a\n\nb").at(2);
    assert_eq!(buffer.selection(), 2..2);
}

#[test]
fn insert_mode_paints_a_bare_caret() {
    let mut buffer = Buffer::new("abc").at(1);
    buffer.keys("i");
    assert_eq!(buffer.selection(), 1..1);
}

// ---- unicode --------------------------------------------------------------

#[test]
fn motions_step_whole_characters() {
    let mut buffer = Buffer::new("héllo wörld").at(0);
    buffer.keys("l");
    assert_eq!(buffer.cursor(), 1);
    buffer.keys("l");
    assert_eq!(buffer.cursor(), 3, "é is two bytes");
    buffer.keys("x");
    assert_eq!(buffer.text, "hélo wörld");
}

#[test]
fn deleting_a_multibyte_character_keeps_the_buffer_valid() {
    let mut buffer = Buffer::new("aéb").at(1);
    buffer.keys("x");
    assert_eq!(buffer.text, "ab");
}

// ---- pending state --------------------------------------------------------

#[test]
fn pending_keys_are_reported_for_the_status_line() {
    let mut buffer = Buffer::new("abc").at(0);
    buffer.keys("2d");
    assert_eq!(buffer.engine.status().pending, "2d");
    buffer.keys("<esc>");
    assert_eq!(buffer.engine.status().pending, "");
}

#[test]
fn a_second_operator_cancels_the_pending_one() {
    let mut buffer = Buffer::new("one two").at(0);
    buffer.keys("dy");
    assert_eq!(buffer.text, "one two");
    assert_eq!(buffer.engine.status().pending, "");
}

#[test]
fn syncing_the_cursor_cancels_a_pending_command() {
    let mut buffer = Buffer::new("one two").at(0);
    buffer.keys("2d");
    let text = buffer.text.clone();
    buffer.engine.sync_cursor(&text, 4);
    assert_eq!(buffer.engine.status().pending, "");
    buffer.keys("w");
    assert_eq!(buffer.text, "one two", "the operator is gone; w just moves");
    assert_eq!(buffer.cursor(), 6);
}
