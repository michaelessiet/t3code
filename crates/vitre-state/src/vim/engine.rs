//! The vim state machine: mode, pending command, registers, marks.

use std::collections::HashMap;
use std::ops::Range;

use super::motion::{
    self, FindKind, char_at, clamp_normal, clamp_offset, first_non_blank, line_count, line_end,
    line_index, line_start, next_boundary, offset_of_line, prev_boundary,
};
use super::object::{self, TextObject};

/// Which mode the editor is in. `Replace` is `R`, the overwrite variant of
/// insert mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
}

impl VimMode {
    /// The `-- INSERT --`-style banner vim shows for this mode.
    pub fn label(self) -> &'static str {
        match self {
            VimMode::Normal => "NORMAL",
            VimMode::Insert => "INSERT",
            VimMode::Replace => "REPLACE",
            VimMode::Visual => "VISUAL",
            VimMode::VisualLine => "V-LINE",
        }
    }

    /// True while keystrokes should reach the editor as text.
    pub fn is_inserting(self) -> bool {
        matches!(self, VimMode::Insert | VimMode::Replace)
    }

    /// True in either visual flavour.
    pub fn is_visual(self) -> bool {
        matches!(self, VimMode::Visual | VimMode::VisualLine)
    }
}

/// A key as the engine sees it. Chords with modifiers other than `ctrl` are
/// not vim commands, so the glue layer does not forward them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VimKey {
    Char(char),
    Ctrl(char),
    Escape,
    Enter,
    Backspace,
    Tab,
    Left,
    Right,
    Up,
    Down,
}

impl VimKey {
    fn as_char(&self) -> Option<char> {
        match self {
            VimKey::Char(c) => Some(*c),
            _ => None,
        }
    }
}

/// The document as of this keystroke, plus the viewport that `H`/`M`/`L` and
/// the half-page scrolls are defined against.
pub struct VimDocument<'a> {
    pub text: &'a str,
    /// Zero-based, half-open range of painted lines.
    pub visible_lines: Range<usize>,
}

impl<'a> VimDocument<'a> {
    pub fn new(text: &'a str, visible_lines: Range<usize>) -> Self {
        Self {
            text,
            visible_lines,
        }
    }
}

/// What the caller must do to the editor. Applied in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VimEffect {
    /// Replace `range` with `text`, then put the caret at `cursor`.
    Edit {
        range: Range<usize>,
        text: String,
        cursor: usize,
    },
    /// Undo one edit (`u`).
    Undo,
    /// Redo one edit (`ctrl-r`).
    Redo,
    /// `:w` — flush the pending save.
    Save,
    /// `:q` — close the open file.
    Quit,
    /// `gh` — show the LSP hover popover at the caret.
    ShowHover,
    /// `gd` / `ctrl-]` — jump to the LSP definition at the caret.
    GoToDefinition,
    /// `=` — format through the language server.
    Format,
    /// Bring the caret's line into view.
    ScrollToCursor,
}

/// The message line under the editor: either the command being typed or the
/// result of the last one.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct VimStatus {
    /// `:wq`, `/needle` — echoed verbatim while being typed.
    pub command_line: Option<String>,
    /// Keys accepted so far for an incomplete command, e.g. `2d`.
    pub pending: String,
    /// Result or error text (`E486: Pattern not found: foo`).
    pub message: Option<String>,
}

/// The outcome of one keystroke.
#[derive(Clone, Debug, Default)]
pub struct VimResponse {
    /// True when the key was a vim command and must not reach the editor as
    /// text. Insert-mode typing returns false.
    pub handled: bool,
    pub effects: Vec<VimEffect>,
}

/// Editor settings the engine needs in order to produce text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VimConfig {
    /// Columns one `>>` shifts by.
    pub shift_width: usize,
    /// Indent with a tab character instead of `shift_width` spaces.
    pub use_tabs: bool,
}

impl Default for VimConfig {
    fn default() -> Self {
        Self {
            shift_width: 4,
            use_tabs: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operator {
    Delete,
    Change,
    Yank,
    Indent,
    Outdent,
    Lowercase,
    Uppercase,
    ToggleCase,
    Format,
}

impl Operator {
    fn key(self) -> &'static str {
        match self {
            Operator::Delete => "d",
            Operator::Change => "c",
            Operator::Yank => "y",
            Operator::Indent => ">",
            Operator::Outdent => "<",
            Operator::Lowercase => "gu",
            Operator::Uppercase => "gU",
            Operator::ToggleCase => "g~",
            Operator::Format => "=",
        }
    }

    /// Operators that only ever work on whole lines.
    fn is_line_only(self) -> bool {
        matches!(
            self,
            Operator::Indent | Operator::Outdent | Operator::Format
        )
    }
}

/// What the engine is waiting for before it can run the pending command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Awaiting {
    /// After `"` — the register name.
    Register,
    /// After `f`/`F`/`t`/`T` — the target character.
    FindChar(FindKind),
    /// After `r` — the replacement character.
    ReplaceChar,
    /// After `g`.
    GPrefix,
    /// After `z`.
    ZPrefix,
    /// After `m` — the mark name.
    SetMark,
    /// After `` ` `` or `'` — the mark to jump to.
    JumpMark { line_wise: bool },
    /// After `i`/`a` with an operator pending, or in visual mode.
    TextObject { around: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandLineKind {
    Ex,
    SearchForward,
    SearchBackward,
}

impl CommandLineKind {
    fn prompt(self) -> char {
        match self {
            CommandLineKind::Ex => ':',
            CommandLineKind::SearchForward => '/',
            CommandLineKind::SearchBackward => '?',
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RegisterContent {
    text: String,
    line_wise: bool,
}

/// How a motion's target combines with the caret to form an operator range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Span {
    /// `w`, `0`, `{` — the character under the target is not included.
    Exclusive(usize),
    /// `e`, `f`, `$` — the target character is included.
    Inclusive(usize),
    /// `j`, `gg`, `G` — whole lines from the caret's line to the target's.
    Line(usize),
}

impl Span {
    fn target(self) -> usize {
        match self {
            Span::Exclusive(offset) | Span::Inclusive(offset) | Span::Line(offset) => offset,
        }
    }
}

/// The keys and inserted text of the last change, replayed by `.`.
#[derive(Clone, Debug, Default)]
struct Repeat {
    keys: Vec<VimKey>,
    inserted: String,
}

/// A vim session for one editor.
pub struct VimEngine {
    config: VimConfig,
    mode: VimMode,
    /// Authoritative caret. The editor's caret follows this, not the other
    /// way round — [`VimEngine::sync_cursor`] exists for mouse clicks and
    /// jumps that originate outside the engine.
    cursor: usize,
    /// The fixed end of a visual selection.
    visual_anchor: usize,
    /// Column `j`/`k` try to return to.
    goal_column: usize,
    count: Option<usize>,
    operator: Option<Operator>,
    operator_count: Option<usize>,
    register: Option<char>,
    awaiting: Option<Awaiting>,
    pending_keys: String,
    command_line: Option<(CommandLineKind, String)>,
    registers: HashMap<char, RegisterContent>,
    marks: HashMap<char, usize>,
    last_find: Option<(FindKind, char)>,
    last_search: Option<(String, bool)>,
    message: Option<String>,
    /// Keys of the change in progress, promoted to `repeat` when it lands.
    recording: Option<Repeat>,
    repeat: Option<Repeat>,
    /// Guards `.` against re-entering itself.
    replaying: bool,
}

impl Default for VimEngine {
    fn default() -> Self {
        Self::new(VimConfig::default())
    }
}

impl VimEngine {
    pub fn new(config: VimConfig) -> Self {
        Self {
            config,
            mode: VimMode::Normal,
            cursor: 0,
            visual_anchor: 0,
            goal_column: 0,
            count: None,
            operator: None,
            operator_count: None,
            register: None,
            awaiting: None,
            pending_keys: String::new(),
            command_line: None,
            registers: HashMap::new(),
            marks: HashMap::new(),
            last_find: None,
            last_search: None,
            message: None,
            recording: None,
            repeat: None,
            replaying: false,
        }
    }

    pub fn mode(&self) -> VimMode {
        self.mode
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_config(&mut self, config: VimConfig) {
        self.config = config;
    }

    pub fn status(&self) -> VimStatus {
        VimStatus {
            command_line: self
                .command_line
                .as_ref()
                .map(|(kind, buffer)| format!("{}{}", kind.prompt(), buffer)),
            pending: self.pending_keys.clone(),
            message: self.message.clone(),
        }
    }

    /// The range the host editor should paint as *selected*.
    ///
    /// Normal mode's one-character "selection" is the block cursor, which the
    /// editor paints separately from [`Self::caret_cell`]; painting it as a
    /// selection as well would only wash the block out in the muted selection
    /// colour, which is what made the caret hard to find inside a visual
    /// selection in the first place.
    pub fn editor_selection(&self, text: &str) -> Range<usize> {
        match self.mode {
            VimMode::Normal => self.cursor..self.cursor,
            _ => self.selection(text),
        }
    }

    /// The one-character cell the caret sits *on*, for a block cursor.
    ///
    /// `None` while inserting, where vim shows the ordinary thin caret between
    /// two characters. An empty range means there is no character to cover —
    /// the caret is on a newline or at the end of the buffer — and the painter
    /// widens the cell to one character itself.
    pub fn caret_cell(&self, text: &str) -> Option<Range<usize>> {
        if self.mode.is_inserting() {
            return None;
        }
        Some(block_at(text, self.cursor))
    }

    /// Whether a multi-key command is half-typed. vim squashes the block
    /// cursor to half height to show it.
    pub fn has_pending_keys(&self) -> bool {
        !self.pending_keys.is_empty()
    }

    /// The selection an operator applies to: a one-character block in normal
    /// mode, the visual range in visual mode, a bare caret while inserting.
    pub fn selection(&self, text: &str) -> Range<usize> {
        match self.mode {
            VimMode::Insert | VimMode::Replace => self.cursor..self.cursor,
            VimMode::Normal => block_at(text, self.cursor),
            VimMode::Visual => {
                let (start, end) = ordered(self.visual_anchor, self.cursor);
                start..block_at(text, end).end.max(end)
            }
            VimMode::VisualLine => {
                let (start, end) = ordered(self.visual_anchor, self.cursor);
                line_start(text, start)..line_end(text, end)
            }
        }
    }

    /// Adopt a caret the editor moved on its own (mouse click, LSP jump, a
    /// freshly opened file). Cancels any pending command; keeps the mode.
    pub fn sync_cursor(&mut self, text: &str, offset: usize) {
        self.cursor = if self.mode.is_inserting() {
            clamp_offset(text, offset)
        } else {
            clamp_normal(text, offset)
        };
        self.goal_column = motion::column(text, self.cursor);
        if !self.mode.is_visual() {
            self.visual_anchor = self.cursor;
        }
        self.reset_pending();
    }

    /// Adopt a selection the editor made on its own — a mouse drag — the way
    /// vim does: dragging in normal mode leaves you in visual mode over the
    /// dragged range. An empty range is just a caret move.
    pub fn sync_selection(&mut self, text: &str, range: Range<usize>) {
        if range.start >= range.end {
            self.sync_cursor(text, range.start);
            return;
        }
        if self.mode.is_inserting() {
            self.cursor = clamp_offset(text, range.end);
            return;
        }
        if !self.mode.is_visual() {
            self.mode = VimMode::Visual;
        }
        self.visual_anchor = clamp_normal(text, range.start);
        self.cursor = clamp_normal(text, prev_boundary(text, range.end));
        self.goal_column = motion::column(text, self.cursor);
        self.reset_pending();
    }

    /// Hand the engine the text an insert session produced, so `.` can replay
    /// it. Insert-mode keystrokes go to the editor rather than the engine —
    /// IME composition has to reach the platform untouched — so the caller
    /// recovers the text from the buffer and reports it here before the key
    /// that ends the session.
    pub fn record_inserted_text(&mut self, text: &str) {
        if let Some(recording) = self.recording.as_mut() {
            recording.inserted = text.to_string();
        }
    }

    /// Drop back to normal mode without running a command — used when the
    /// editor loses focus or the open file changes.
    pub fn reset(&mut self, text: &str) {
        self.mode = VimMode::Normal;
        self.command_line = None;
        self.recording = None;
        self.reset_pending();
        self.cursor = clamp_normal(text, self.cursor);
        self.visual_anchor = self.cursor;
    }

    /// Feed one key. Returns whether the key was consumed.
    pub fn handle_key(&mut self, doc: &VimDocument<'_>, key: VimKey) -> VimResponse {
        self.message = None;

        if self.command_line.is_some() {
            return self.handle_command_line(doc, key);
        }

        if self.mode.is_inserting() {
            return self.handle_insert(doc, key);
        }

        if let Some(recording) = self.recording.as_mut() {
            recording.keys.push(key.clone());
        }
        let mut response = self.handle_normal(doc, key);
        if !response.effects.is_empty() || self.mode.is_inserting() {
            response.effects.push(VimEffect::ScrollToCursor);
        }
        response
    }

    // ---- insert mode ------------------------------------------------------

    fn handle_insert(&mut self, doc: &VimDocument<'_>, key: VimKey) -> VimResponse {
        match key {
            VimKey::Escape => {
                self.mode = VimMode::Normal;
                self.cursor = clamp_normal(doc.text, prev_boundary(doc.text, self.cursor));
                self.goal_column = motion::column(doc.text, self.cursor);
                self.visual_anchor = self.cursor;
                if let Some(recording) = self.recording.take() {
                    self.repeat = Some(recording);
                }
                handled(vec![VimEffect::ScrollToCursor])
            }
            // Everything else is ordinary typing: the editor applies it, and
            // the engine only remembers what was typed so `.` can replay it.
            other => {
                if let Some(recording) = self.recording.as_mut() {
                    match &other {
                        VimKey::Char(c) => recording.inserted.push(*c),
                        VimKey::Enter => recording.inserted.push('\n'),
                        VimKey::Tab => recording.inserted.push('\t'),
                        // A backspace or an arrow key makes the recorded text
                        // an unreliable script; give up on `.` rather than
                        // replay something wrong.
                        _ => self.recording = None,
                    }
                }
                VimResponse::default()
            }
        }
    }

    // ---- command line -----------------------------------------------------

    fn handle_command_line(&mut self, doc: &VimDocument<'_>, key: VimKey) -> VimResponse {
        let Some((kind, buffer)) = self.command_line.as_mut() else {
            return VimResponse::default();
        };
        match key {
            VimKey::Escape => {
                self.command_line = None;
                handled(Vec::new())
            }
            VimKey::Backspace => {
                if buffer.pop().is_none() {
                    self.command_line = None;
                }
                handled(Vec::new())
            }
            VimKey::Enter => {
                let kind = *kind;
                let buffer = std::mem::take(buffer);
                self.command_line = None;
                let effects = match kind {
                    CommandLineKind::Ex => self.run_ex(doc, &buffer),
                    CommandLineKind::SearchForward => self.run_search(doc, buffer, true),
                    CommandLineKind::SearchBackward => self.run_search(doc, buffer, false),
                };
                handled(effects)
            }
            VimKey::Char(c) => {
                buffer.push(c);
                handled(Vec::new())
            }
            _ => handled(Vec::new()),
        }
    }

    fn run_search(
        &mut self,
        doc: &VimDocument<'_>,
        pattern: String,
        forward: bool,
    ) -> Vec<VimEffect> {
        let pattern = if pattern.is_empty() {
            let Some((previous, _)) = self.last_search.clone() else {
                self.message = Some("E35: No previous regular expression".into());
                return Vec::new();
            };
            previous
        } else {
            pattern
        };
        self.last_search = Some((pattern.clone(), forward));
        self.jump_to_search(doc, &pattern, forward, 1)
    }

    fn jump_to_search(
        &mut self,
        doc: &VimDocument<'_>,
        pattern: &str,
        forward: bool,
        count: usize,
    ) -> Vec<VimEffect> {
        match search_offset(doc.text, self.cursor, pattern, forward, count) {
            Some(offset) => self.move_caret(doc.text, offset),
            None => self.message = Some(format!("E486: Pattern not found: {pattern}")),
        }
        Vec::new()
    }

    fn run_ex(&mut self, doc: &VimDocument<'_>, command: &str) -> Vec<VimEffect> {
        let command = command.trim();
        if command.is_empty() {
            return Vec::new();
        }
        // `:42` and `:$` jump to a line.
        if command == "$" {
            let offset = offset_of_line(doc.text, line_count(doc.text) - 1);
            self.move_caret(doc.text, first_non_blank(doc.text, offset));
            return Vec::new();
        }
        if let Ok(line) = command.parse::<usize>() {
            let target = line.saturating_sub(1).min(line_count(doc.text) - 1);
            let offset = offset_of_line(doc.text, target);
            self.move_caret(doc.text, first_non_blank(doc.text, offset));
            return Vec::new();
        }
        if let Some(effects) = self.run_substitute(doc, command) {
            return effects;
        }
        match command {
            "w" | "write" => vec![VimEffect::Save],
            "q" | "quit" | "q!" | "quit!" => vec![VimEffect::Quit],
            "wq" | "x" | "wq!" | "xit" => vec![VimEffect::Save, VimEffect::Quit],
            "noh" | "nohl" | "nohlsearch" => Vec::new(),
            other => {
                self.message = Some(format!("E492: Not an editor command: {other}"));
                Vec::new()
            }
        }
    }

    /// `:s/pat/rep/[g]` on the caret's line, `:%s/…` over the whole buffer.
    fn run_substitute(&mut self, doc: &VimDocument<'_>, command: &str) -> Option<Vec<VimEffect>> {
        let (whole_file, rest) = match command.strip_prefix('%') {
            Some(rest) => (true, rest),
            None => (false, command),
        };
        let rest = rest.strip_prefix('s')?;
        let separator = rest.chars().next()?;
        if separator.is_alphanumeric() {
            return None;
        }
        let mut parts = rest[separator.len_utf8()..].split(separator);
        let pattern = parts.next()?.to_string();
        let replacement = parts.next().unwrap_or("").to_string();
        let global = parts.next().unwrap_or("").contains('g');

        let range = if whole_file {
            0..doc.text.len()
        } else {
            line_start(doc.text, self.cursor)..line_end(doc.text, self.cursor)
        };
        let source = &doc.text[range.clone()];
        let replaced = match (compile(&pattern), global) {
            (Some(regex), true) => regex.replace_all(source, replacement.as_str()).into_owned(),
            (Some(regex), false) => regex.replace(source, replacement.as_str()).into_owned(),
            (None, true) => source.replace(&pattern, &replacement),
            (None, false) => source.replacen(&pattern, &replacement, 1),
        };
        if replaced == source {
            self.message = Some(format!("E486: Pattern not found: {pattern}"));
            return Some(Vec::new());
        }
        let cursor = range.start;
        self.cursor = cursor;
        Some(vec![VimEffect::Edit {
            range,
            text: replaced,
            cursor,
        }])
    }

    // ---- normal and visual mode -------------------------------------------

    fn handle_normal(&mut self, doc: &VimDocument<'_>, key: VimKey) -> VimResponse {
        let text = doc.text;

        if key == VimKey::Escape {
            let was_visual = self.mode.is_visual();
            self.mode = VimMode::Normal;
            self.reset_pending();
            self.recording = None;
            if was_visual {
                self.cursor = clamp_normal(text, self.cursor);
                self.visual_anchor = self.cursor;
            }
            return handled(Vec::new());
        }

        if let Some(awaiting) = self.awaiting.take() {
            return self.resolve_awaiting(doc, awaiting, key);
        }

        let Some(c) = key.as_char() else {
            return self.handle_special(doc, key);
        };

        // Counts. A leading `0` is the line-start motion, not a digit.
        if c.is_ascii_digit() && !(c == '0' && self.active_count().is_none()) {
            let digit = c as usize - '0' as usize;
            let slot = if self.operator.is_some() {
                &mut self.operator_count
            } else {
                &mut self.count
            };
            *slot = Some(slot.unwrap_or(0) * 10 + digit);
            self.pending_keys.push(c);
            return handled(Vec::new());
        }

        match c {
            '"' => {
                self.awaiting = Some(Awaiting::Register);
                self.pending_keys.push('"');
                handled(Vec::new())
            }
            'i' | 'a' if self.operator.is_some() || self.mode.is_visual() => {
                self.awaiting = Some(Awaiting::TextObject { around: c == 'a' });
                self.pending_keys.push(c);
                handled(Vec::new())
            }
            'g' => {
                self.awaiting = Some(Awaiting::GPrefix);
                self.pending_keys.push('g');
                handled(Vec::new())
            }
            'z' => {
                self.awaiting = Some(Awaiting::ZPrefix);
                self.pending_keys.push('z');
                handled(Vec::new())
            }
            'm' => {
                self.awaiting = Some(Awaiting::SetMark);
                self.pending_keys.push('m');
                handled(Vec::new())
            }
            '`' | '\'' => {
                self.awaiting = Some(Awaiting::JumpMark {
                    line_wise: c == '\'',
                });
                self.pending_keys.push(c);
                handled(Vec::new())
            }
            'f' | 'F' | 't' | 'T' => {
                let kind = match c {
                    'f' => FindKind::Find,
                    'F' => FindKind::FindBack,
                    't' => FindKind::Till,
                    _ => FindKind::TillBack,
                };
                self.awaiting = Some(Awaiting::FindChar(kind));
                self.pending_keys.push(c);
                handled(Vec::new())
            }
            'r' if !self.mode.is_visual() => {
                self.awaiting = Some(Awaiting::ReplaceChar);
                self.pending_keys.push('r');
                handled(Vec::new())
            }
            ':' => {
                self.command_line = Some((CommandLineKind::Ex, String::new()));
                handled(Vec::new())
            }
            '/' => {
                self.command_line = Some((CommandLineKind::SearchForward, String::new()));
                handled(Vec::new())
            }
            '?' => {
                self.command_line = Some((CommandLineKind::SearchBackward, String::new()));
                handled(Vec::new())
            }
            _ => self.dispatch(doc, c),
        }
    }

    fn resolve_awaiting(
        &mut self,
        doc: &VimDocument<'_>,
        awaiting: Awaiting,
        key: VimKey,
    ) -> VimResponse {
        let text = doc.text;
        match awaiting {
            Awaiting::Register => {
                match key.as_char() {
                    Some(c) => {
                        self.register = Some(c);
                        self.pending_keys.push(c);
                    }
                    None => self.reset_pending(),
                }
                handled(Vec::new())
            }
            Awaiting::SetMark => {
                if let Some(c) = key.as_char() {
                    self.marks.insert(c, self.cursor);
                }
                self.reset_pending();
                handled(Vec::new())
            }
            Awaiting::JumpMark { line_wise } => {
                let Some(c) = key.as_char() else {
                    self.reset_pending();
                    return handled(Vec::new());
                };
                let Some(&offset) = self.marks.get(&c) else {
                    self.message = Some(format!("E20: Mark not set: {c}"));
                    self.reset_pending();
                    return handled(Vec::new());
                };
                let span = if line_wise {
                    Span::Line(first_non_blank(text, offset))
                } else {
                    Span::Exclusive(clamp_offset(text, offset))
                };
                self.apply_motion(doc, span)
            }
            Awaiting::FindChar(kind) => {
                let Some(target) = key.as_char() else {
                    self.reset_pending();
                    return handled(Vec::new());
                };
                self.last_find = Some((kind, target));
                let count = self.peek_count();
                match motion::find_char(text, self.cursor, kind, target, count, false) {
                    Some(offset) => {
                        let span = if kind.is_forward() {
                            Span::Inclusive(offset)
                        } else {
                            Span::Exclusive(offset)
                        };
                        self.apply_motion(doc, span)
                    }
                    None => {
                        self.reset_pending();
                        handled(Vec::new())
                    }
                }
            }
            Awaiting::ReplaceChar => {
                let Some(replacement) = key.as_char() else {
                    self.reset_pending();
                    return handled(Vec::new());
                };
                let count = self.take_count();
                let end = motion::right(text, self.cursor, count, true);
                if end.saturating_sub(self.cursor) < count {
                    // `3r` needs three characters left on the line.
                    self.reset_pending();
                    return handled(Vec::new());
                }
                let range = self.cursor..end;
                let cursor = prev_boundary(text, end);
                self.record_simple(count, &[VimKey::Char('r'), key]);
                self.reset_pending();
                self.cursor = cursor;
                handled(vec![VimEffect::Edit {
                    range,
                    text: replacement.to_string().repeat(count),
                    cursor,
                }])
            }
            Awaiting::TextObject { around } => {
                let Some(object) = key.as_char().and_then(TextObject::from_key) else {
                    self.reset_pending();
                    return handled(Vec::new());
                };
                let Some(range) = object::resolve(text, self.cursor, object, around) else {
                    self.reset_pending();
                    return handled(Vec::new());
                };
                if self.mode.is_visual() {
                    self.visual_anchor = range.start;
                    self.cursor = clamp_normal(text, prev_boundary(text, range.end));
                    self.reset_pending();
                    return handled(Vec::new());
                }
                let Some(operator) = self.operator else {
                    self.reset_pending();
                    return handled(Vec::new());
                };
                let line_wise = matches!(object, TextObject::Paragraph);
                let recording = self.finish_change();
                let effects = self.run_operator(doc, operator, range, line_wise);
                self.restore_recording(recording);
                handled(effects)
            }
            Awaiting::GPrefix => self.resolve_g_prefix(doc, key),
            Awaiting::ZPrefix => {
                self.reset_pending();
                // `zz`/`zt`/`zb` all collapse to "get the caret on screen":
                // the fork scrolls the caret into view but exposes no way to
                // pin it to a particular row.
                handled(vec![VimEffect::ScrollToCursor])
            }
        }
    }

    fn resolve_g_prefix(&mut self, doc: &VimDocument<'_>, key: VimKey) -> VimResponse {
        let text = doc.text;
        let Some(c) = key.as_char() else {
            self.reset_pending();
            return handled(Vec::new());
        };
        match c {
            'g' => {
                let line = self
                    .peek_count_raw()
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .min(line_count(text) - 1);
                let offset = first_non_blank(text, offset_of_line(text, line));
                self.apply_motion(doc, Span::Line(offset))
            }
            'e' | 'E' => {
                let count = self.peek_count();
                let mut offset = self.cursor;
                for _ in 0..count {
                    offset = motion::prev_word_end(text, offset, c == 'E');
                }
                self.apply_motion(doc, Span::Inclusive(offset))
            }
            'h' => {
                self.reset_pending();
                handled(vec![VimEffect::ShowHover])
            }
            'd' | 'D' => {
                self.reset_pending();
                handled(vec![VimEffect::GoToDefinition])
            }
            'u' | 'U' | '~' => {
                let operator = match c {
                    'u' => Operator::Lowercase,
                    'U' => Operator::Uppercase,
                    _ => Operator::ToggleCase,
                };
                if self.mode.is_visual() {
                    let (range, line_wise) = self.visual_range(text);
                    self.mode = VimMode::Normal;
                    let recording = self.finish_change();
                    let effects = self.run_operator(doc, operator, range, line_wise);
                    self.restore_recording(recording);
                    return handled(effects);
                }
                self.operator = Some(operator);
                self.pending_keys.push(c);
                self.start_recording(&[VimKey::Char('g'), key]);
                handled(Vec::new())
            }
            '_' => {
                let count = self.peek_count();
                let line = (line_index(text, self.cursor) + count - 1).min(line_count(text) - 1);
                let target = last_non_blank(text, offset_of_line(text, line));
                self.apply_motion(doc, Span::Inclusive(target))
            }
            _ => {
                self.reset_pending();
                handled(Vec::new())
            }
        }
    }

    fn handle_special(&mut self, doc: &VimDocument<'_>, key: VimKey) -> VimResponse {
        let text = doc.text;
        match key {
            VimKey::Left | VimKey::Backspace => self.dispatch(doc, 'h'),
            VimKey::Right => self.dispatch(doc, 'l'),
            VimKey::Up => self.dispatch(doc, 'k'),
            VimKey::Down => self.dispatch(doc, 'j'),
            VimKey::Enter => {
                let count = self.peek_count();
                let line = (line_index(text, self.cursor) + count).min(line_count(text) - 1);
                let offset = first_non_blank(text, offset_of_line(text, line));
                self.apply_motion(doc, Span::Line(offset))
            }
            VimKey::Ctrl(c) => self.handle_ctrl(doc, c),
            _ => {
                self.reset_pending();
                handled(Vec::new())
            }
        }
    }

    fn handle_ctrl(&mut self, doc: &VimDocument<'_>, c: char) -> VimResponse {
        let text = doc.text;
        let page = doc.visible_lines.len().max(2);
        match c {
            'r' => {
                self.reset_pending();
                handled(vec![VimEffect::Redo])
            }
            ']' => {
                self.reset_pending();
                handled(vec![VimEffect::GoToDefinition])
            }
            'd' | 'u' | 'f' | 'b' => {
                let lines = match c {
                    'd' | 'u' => (page / 2) as isize,
                    _ => page as isize,
                };
                let delta = if c == 'd' || c == 'f' { lines } else { -lines };
                let offset = motion::vertical(text, self.cursor, delta, self.goal_column);
                self.apply_motion(doc, Span::Line(offset))
            }
            'e' | 'y' => {
                self.reset_pending();
                handled(vec![VimEffect::ScrollToCursor])
            }
            // `<C-v>` (block visual) is not implemented; fall back to
            // characterwise so the key is not silently swallowed as text.
            'v' => self.dispatch(doc, 'v'),
            _ => {
                self.reset_pending();
                handled(Vec::new())
            }
        }
    }

    /// Everything that is not a prefix: motions, operators and the direct
    /// editing commands.
    fn dispatch(&mut self, doc: &VimDocument<'_>, c: char) -> VimResponse {
        let text = doc.text;
        let count = self.peek_count();

        if let Some(span) = self.eval_motion(doc, c, count) {
            return self.apply_motion(doc, span);
        }

        if let Some(operator) = operator_for(c) {
            return self.push_operator(doc, operator, c);
        }

        match c {
            'v' | 'V' => {
                let target = if c == 'v' {
                    VimMode::Visual
                } else {
                    VimMode::VisualLine
                };
                if self.mode == target {
                    self.mode = VimMode::Normal;
                } else {
                    if !self.mode.is_visual() {
                        self.visual_anchor = self.cursor;
                    }
                    self.mode = target;
                }
                self.reset_pending();
                handled(Vec::new())
            }
            'o' if self.mode.is_visual() => {
                std::mem::swap(&mut self.visual_anchor, &mut self.cursor);
                self.reset_pending();
                handled(Vec::new())
            }
            'i' | 'a' | 'I' | 'A' | 'o' | 'O' => self.enter_insert(doc, c),
            'x' | 's' | 'X' => self.delete_chars(doc, c),
            'D' | 'C' | 'Y' | 'S' => self.line_tail_command(doc, c),
            'p' | 'P' => self.paste(doc, c == 'p'),
            'u' => {
                self.reset_pending();
                handled(vec![VimEffect::Undo])
            }
            'J' => self.join_lines(doc),
            '~' => self.toggle_case_char(doc),
            'n' | 'N' => {
                let count = self.take_count();
                let Some((pattern, forward)) = self.last_search.clone() else {
                    self.message = Some("E35: No previous regular expression".into());
                    return handled(Vec::new());
                };
                let forward = if c == 'n' { forward } else { !forward };
                handled(self.jump_to_search(doc, &pattern, forward, count))
            }
            '*' | '#' => {
                self.reset_pending();
                let Some(range) = motion::word_under_cursor(text, self.cursor) else {
                    return handled(Vec::new());
                };
                let pattern = format!("\\b{}\\b", regex::escape(&text[range]));
                let forward = c == '*';
                self.last_search = Some((pattern.clone(), forward));
                handled(self.jump_to_search(doc, &pattern, forward, 1))
            }
            '.' => self.repeat_last_change(doc),
            _ => {
                self.reset_pending();
                handled(Vec::new())
            }
        }
    }

    fn push_operator(
        &mut self,
        doc: &VimDocument<'_>,
        operator: Operator,
        key: char,
    ) -> VimResponse {
        let text = doc.text;
        if self.mode.is_visual() {
            let (range, line_wise) = self.visual_range(text);
            self.mode = VimMode::Normal;
            let recording = self.finish_change();
            let effects = self.run_operator(doc, operator, range, line_wise);
            self.restore_recording(recording);
            return handled(effects);
        }
        if self.operator == Some(operator) {
            // Doubled operator (`dd`, `yy`, `>>`): whole lines.
            let count = self.take_count();
            let first = line_index(text, self.cursor);
            let last = (first + count - 1).min(line_count(text) - 1);
            let range = offset_of_line(text, first)..line_end(text, offset_of_line(text, last));
            let recording = self.finish_change();
            let effects = self.run_operator(doc, operator, range, true);
            self.restore_recording(recording);
            return handled(effects);
        }
        if self.operator.is_some() {
            // A different operator cancels the pending one, as in vim.
            self.reset_pending();
            return handled(Vec::new());
        }
        self.operator = Some(operator);
        self.pending_keys.push_str(operator.key());
        self.start_recording(&[VimKey::Char(key)]);
        handled(Vec::new())
    }

    // ---- motions ----------------------------------------------------------

    fn eval_motion(&mut self, doc: &VimDocument<'_>, c: char, count: usize) -> Option<Span> {
        let text = doc.text;
        let cursor = self.cursor;
        let for_operator = self.operator.is_some();
        Some(match c {
            'h' => Span::Exclusive(motion::left(text, cursor, count)),
            'l' | ' ' => Span::Exclusive(motion::right(text, cursor, count, for_operator)),
            'j' => Span::Line(motion::vertical(
                text,
                cursor,
                count as isize,
                self.goal_column,
            )),
            'k' => Span::Line(motion::vertical(
                text,
                cursor,
                -(count as isize),
                self.goal_column,
            )),
            '+' => {
                let line = (line_index(text, cursor) + count).min(line_count(text) - 1);
                Span::Line(first_non_blank(text, offset_of_line(text, line)))
            }
            '-' => {
                let line = line_index(text, cursor).saturating_sub(count);
                Span::Line(first_non_blank(text, offset_of_line(text, line)))
            }
            'w' | 'W' => {
                let big = c == 'W';
                // `cw` behaves like `ce` — vim's one documented special case.
                let like_end = self.operator == Some(Operator::Change)
                    && char_at(text, cursor)
                        .is_some_and(|c| motion::class(c, big) != motion::CharClass::Blank);
                let mut offset = cursor;
                for _ in 0..count {
                    offset = if like_end {
                        motion::next_word_end(text, offset, big)
                    } else {
                        motion::next_word_start(text, offset, big)
                    };
                }
                if like_end {
                    Span::Inclusive(offset)
                } else {
                    Span::Exclusive(offset)
                }
            }
            'b' | 'B' => {
                let mut offset = cursor;
                for _ in 0..count {
                    offset = motion::prev_word_start(text, offset, c == 'B');
                }
                Span::Exclusive(offset)
            }
            'e' | 'E' => {
                let mut offset = cursor;
                for _ in 0..count {
                    offset = motion::next_word_end(text, offset, c == 'E');
                }
                Span::Inclusive(offset)
            }
            '0' => Span::Exclusive(line_start(text, cursor)),
            '^' => Span::Exclusive(first_non_blank(text, cursor)),
            '$' => {
                let line = (line_index(text, cursor) + count - 1).min(line_count(text) - 1);
                let offset = offset_of_line(text, line);
                let end = line_end(text, offset);
                if for_operator {
                    Span::Exclusive(end)
                } else {
                    Span::Inclusive(prev_boundary(text, end).max(line_start(text, offset)))
                }
            }
            '|' => {
                let start = line_start(text, cursor);
                let end = line_end(text, cursor);
                let mut offset = start;
                for _ in 1..count {
                    if offset >= end {
                        break;
                    }
                    offset = next_boundary(text, offset);
                }
                Span::Exclusive(offset)
            }
            'G' => {
                let line = self
                    .peek_count_raw()
                    .map(|count| count.saturating_sub(1))
                    .unwrap_or(line_count(text) - 1)
                    .min(line_count(text) - 1);
                Span::Line(first_non_blank(text, offset_of_line(text, line)))
            }
            'H' | 'M' | 'L' => {
                let visible = &doc.visible_lines;
                let last = line_count(text) - 1;
                let line = match c {
                    'H' => (visible.start + count - 1).min(last),
                    'M' => ((visible.start + visible.end.max(visible.start + 1)) / 2).min(last),
                    _ => visible.end.max(1).saturating_sub(count).min(last),
                };
                Span::Line(first_non_blank(text, offset_of_line(text, line)))
            }
            '{' | '}' => {
                let mut offset = cursor;
                for _ in 0..count {
                    offset = motion::paragraph(text, offset, c == '}');
                }
                Span::Exclusive(offset)
            }
            '%' => Span::Inclusive(motion::matching_bracket(text, cursor)?),
            ';' | ',' => {
                let (kind, target) = self.last_find?;
                let kind = if c == ';' { kind } else { kind.reversed() };
                let offset = motion::find_char(text, cursor, kind, target, count, true)?;
                if kind.is_forward() {
                    Span::Inclusive(offset)
                } else {
                    Span::Exclusive(offset)
                }
            }
            _ => return None,
        })
    }

    /// Either move the caret (no operator pending) or run the pending
    /// operator over the span the motion described.
    fn apply_motion(&mut self, doc: &VimDocument<'_>, span: Span) -> VimResponse {
        let text = doc.text;
        let Some(operator) = self.operator else {
            self.take_count();
            self.awaiting = None;
            self.pending_keys.clear();
            self.cursor = clamp_normal(text, span.target());
            // `j`/`k`/`G` keep the column they were aiming for; every other
            // motion sets a new one.
            if !matches!(span, Span::Line(_)) {
                self.goal_column = motion::column(text, self.cursor);
            }
            return handled(Vec::new());
        };
        self.take_count();
        let (range, line_wise) = match span {
            Span::Exclusive(target) => {
                let (start, end) = ordered(self.cursor, target);
                (start..end, false)
            }
            Span::Inclusive(target) => {
                let (start, end) = ordered(self.cursor, target);
                (start..next_boundary(text, end), false)
            }
            Span::Line(target) => {
                let (start, end) = ordered(self.cursor, target);
                (line_start(text, start)..line_end(text, end), true)
            }
        };
        let recording = self.finish_change();
        let effects = self.run_operator(doc, operator, range, line_wise);
        self.restore_recording(recording);
        handled(effects)
    }

    // ---- operators --------------------------------------------------------

    fn run_operator(
        &mut self,
        doc: &VimDocument<'_>,
        operator: Operator,
        range: Range<usize>,
        line_wise: bool,
    ) -> Vec<VimEffect> {
        let text = doc.text;
        let line_wise = line_wise || operator.is_line_only();
        let range = if line_wise {
            line_start(text, range.start)..line_end(text, range.end.max(range.start))
        } else {
            range
        };
        // `reset_pending` clears the register, so capture it first.
        let register = self.register;
        self.reset_pending();

        match operator {
            Operator::Yank => {
                let yanked = register_text(text, &range, line_wise);
                self.store_register(register, yanked, line_wise, true);
                // Vim leaves the caret at the start of the yanked text.
                self.cursor = clamp_normal(text, range.start);
                self.goal_column = motion::column(text, self.cursor);
                Vec::new()
            }
            Operator::Delete | Operator::Change => {
                let change = operator == Operator::Change;
                let yanked = register_text(text, &range, line_wise);
                self.store_register(register, yanked, line_wise, false);
                if line_wise && change {
                    // `cc` keeps the (now empty) line and its indentation.
                    let indent = leading_indent(text, range.start);
                    let cursor = range.start + indent.len();
                    self.mode = VimMode::Insert;
                    self.cursor = cursor;
                    return vec![VimEffect::Edit {
                        range,
                        text: indent,
                        cursor,
                    }];
                }
                let range = if line_wise {
                    with_line_terminator(text, range)
                } else {
                    range
                };
                let cursor = range.start;
                if change {
                    self.mode = VimMode::Insert;
                    self.cursor = cursor;
                } else {
                    self.cursor = clamp_after_delete(text, &range);
                }
                self.goal_column = 0;
                vec![VimEffect::Edit {
                    range,
                    text: String::new(),
                    cursor,
                }]
            }
            Operator::Indent | Operator::Outdent => {
                let unit = self.indent_unit();
                let source = &text[range.clone()];
                let shifted = source
                    .split('\n')
                    .map(|line| {
                        if operator == Operator::Indent {
                            if line.is_empty() {
                                line.to_string()
                            } else {
                                format!("{unit}{line}")
                            }
                        } else {
                            outdent_line(line, &unit)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if shifted == source {
                    return Vec::new();
                }
                let cursor = range.start;
                self.cursor = cursor;
                vec![VimEffect::Edit {
                    range,
                    text: shifted,
                    cursor,
                }]
            }
            Operator::Format => vec![VimEffect::Format],
            Operator::Lowercase | Operator::Uppercase | Operator::ToggleCase => {
                let mapped: String = text[range.clone()]
                    .chars()
                    .map(|c| match operator {
                        Operator::Lowercase => c.to_lowercase().next().unwrap_or(c),
                        Operator::Uppercase => c.to_uppercase().next().unwrap_or(c),
                        _ => toggle_case(c),
                    })
                    .collect();
                let cursor = clamp_normal(text, range.start);
                self.cursor = cursor;
                vec![VimEffect::Edit {
                    range,
                    text: mapped,
                    cursor,
                }]
            }
        }
    }

    // ---- direct editing commands ------------------------------------------

    fn enter_insert(&mut self, doc: &VimDocument<'_>, c: char) -> VimResponse {
        let text = doc.text;
        let was_visual = self.mode.is_visual();
        let selection = self.selection(text);
        self.take_count();
        self.reset_pending();
        self.start_recording(&[VimKey::Char(c)]);
        self.mode = VimMode::Insert;
        match c {
            'i' => {
                if was_visual {
                    self.cursor = selection.start;
                }
                handled(Vec::new())
            }
            'a' => {
                if was_visual {
                    self.cursor = selection.end;
                } else if char_at(text, self.cursor).is_some_and(|c| c != '\n') {
                    self.cursor = next_boundary(text, self.cursor);
                }
                handled(Vec::new())
            }
            'I' => {
                self.cursor = first_non_blank(text, selection.start);
                handled(Vec::new())
            }
            'A' => {
                self.cursor = line_end(
                    text,
                    if was_visual {
                        selection.end
                    } else {
                        self.cursor
                    },
                );
                handled(Vec::new())
            }
            'o' | 'O' => {
                let indent = leading_indent(text, self.cursor);
                let (at, inserted, offset) = if c == 'o' {
                    let at = line_end(text, self.cursor);
                    (at, format!("\n{indent}"), 1 + indent.len())
                } else {
                    let at = line_start(text, self.cursor);
                    (at, format!("{indent}\n"), indent.len())
                };
                let cursor = at + offset;
                self.cursor = cursor;
                handled(vec![VimEffect::Edit {
                    range: at..at,
                    text: inserted,
                    cursor,
                }])
            }
            _ => handled(Vec::new()),
        }
    }

    fn delete_chars(&mut self, doc: &VimDocument<'_>, c: char) -> VimResponse {
        let text = doc.text;
        if self.mode.is_visual() {
            let operator = if c == 's' {
                Operator::Change
            } else {
                Operator::Delete
            };
            let (range, line_wise) = self.visual_range(text);
            self.mode = VimMode::Normal;
            let recording = self.finish_change();
            let effects = self.run_operator(doc, operator, range, line_wise);
            self.restore_recording(recording);
            return handled(effects);
        }
        let count = self.take_count();
        let register = self.register;
        let range = if c == 'X' {
            motion::left(text, self.cursor, count)..self.cursor
        } else {
            self.cursor..motion::right(text, self.cursor, count, true)
        };
        if range.is_empty() {
            self.reset_pending();
            return handled(Vec::new());
        }
        self.store_register(register, text[range.clone()].to_string(), false, false);
        self.record_simple(count, &[VimKey::Char(c)]);
        self.reset_pending();
        let cursor = range.start;
        if c == 's' {
            self.mode = VimMode::Insert;
            self.cursor = cursor;
        } else {
            self.cursor = clamp_after_delete(text, &range);
        }
        handled(vec![VimEffect::Edit {
            range,
            text: String::new(),
            cursor,
        }])
    }

    fn line_tail_command(&mut self, doc: &VimDocument<'_>, c: char) -> VimResponse {
        let text = doc.text;
        let operator = match c {
            'C' | 'S' => Operator::Change,
            'Y' => Operator::Yank,
            _ => Operator::Delete,
        };
        if self.mode.is_visual() {
            let (start, end) = ordered(self.visual_anchor, self.cursor);
            let range = line_start(text, start)..line_end(text, end);
            self.mode = VimMode::Normal;
            let recording = self.finish_change();
            let effects = self.run_operator(doc, operator, range, true);
            self.restore_recording(recording);
            return handled(effects);
        }
        let count = self.take_count();
        let first = line_index(text, self.cursor);
        let last = (first + count - 1).min(line_count(text) - 1);
        let (range, line_wise) = match c {
            // `D`/`C` run from the caret to the end of the last line.
            'D' | 'C' => (
                self.cursor..line_end(text, offset_of_line(text, last)),
                false,
            ),
            _ => (
                offset_of_line(text, first)..line_end(text, offset_of_line(text, last)),
                true,
            ),
        };
        self.record_simple(count, &[VimKey::Char(c)]);
        let recording = self.finish_change();
        let effects = self.run_operator(doc, operator, range, line_wise);
        self.restore_recording(recording);
        handled(effects)
    }

    fn paste(&mut self, doc: &VimDocument<'_>, after: bool) -> VimResponse {
        let text = doc.text;
        let count = self.take_count();
        let name = self.register.unwrap_or('"');
        let Some(content) = self.registers.get(&name).cloned() else {
            self.reset_pending();
            return handled(Vec::new());
        };
        if content.text.is_empty() {
            self.reset_pending();
            return handled(Vec::new());
        }
        self.record_simple(count, &[VimKey::Char(if after { 'p' } else { 'P' })]);
        self.reset_pending();

        if self.mode.is_visual() {
            let (range, _) = self.visual_range(text);
            self.mode = VimMode::Normal;
            let cursor = range.start;
            self.cursor = cursor;
            return handled(vec![VimEffect::Edit {
                range,
                text: content.text.repeat(count),
                cursor,
            }]);
        }

        if content.line_wise {
            let body = content.text.repeat(count);
            let (at, inserted, cursor_offset) = if after {
                let end = line_end(text, self.cursor);
                if end >= text.len() {
                    // Last line, no trailing newline: open one first.
                    let body = body.trim_end_matches('\n').to_string();
                    let offset = 1 + leading_indent_len(&body);
                    (end, format!("\n{body}"), offset)
                } else {
                    let offset = leading_indent_len(&body);
                    (end + 1, body, offset)
                }
            } else {
                let offset = leading_indent_len(&body);
                (line_start(text, self.cursor), body, offset)
            };
            let cursor = at + cursor_offset;
            self.cursor = cursor;
            return handled(vec![VimEffect::Edit {
                range: at..at,
                text: inserted,
                cursor,
            }]);
        }

        let at = if after && char_at(text, self.cursor).is_some_and(|c| c != '\n') {
            next_boundary(text, self.cursor)
        } else {
            self.cursor
        };
        let inserted = content.text.repeat(count);
        // Vim leaves the caret on the last pasted character.
        let cursor = at + prev_boundary(&inserted, inserted.len());
        self.cursor = cursor;
        handled(vec![VimEffect::Edit {
            range: at..at,
            text: inserted,
            cursor,
        }])
    }

    fn join_lines(&mut self, doc: &VimDocument<'_>) -> VimResponse {
        let text = doc.text;
        let joins = if self.mode.is_visual() {
            let (start, end) = ordered(self.visual_anchor, self.cursor);
            self.mode = VimMode::Normal;
            self.cursor = start;
            (line_index(text, end) - line_index(text, start)).max(1)
        } else {
            self.take_count().max(2) - 1
        };
        self.reset_pending();

        let first = line_index(text, self.cursor);
        let last = (first + joins).min(line_count(text) - 1);
        if last == first {
            return handled(Vec::new());
        }
        let start = line_end(text, offset_of_line(text, first));
        let end = line_end(text, offset_of_line(text, last));
        let mut joined = String::new();
        for line in (first + 1)..=last {
            let line_at = offset_of_line(text, line);
            let segment = text[line_at..line_end(text, line_at)].trim_start_matches([' ', '\t']);
            if segment.is_empty() {
                continue;
            }
            joined.push(' ');
            joined.push_str(segment);
        }
        self.record_simple(joins + 1, &[VimKey::Char('J')]);
        self.cursor = start;
        handled(vec![VimEffect::Edit {
            range: start..end,
            text: joined,
            cursor: start,
        }])
    }

    fn toggle_case_char(&mut self, doc: &VimDocument<'_>) -> VimResponse {
        let text = doc.text;
        if self.mode.is_visual() {
            let (range, line_wise) = self.visual_range(text);
            self.mode = VimMode::Normal;
            let recording = self.finish_change();
            let effects = self.run_operator(doc, Operator::ToggleCase, range, line_wise);
            self.restore_recording(recording);
            return handled(effects);
        }
        let count = self.take_count();
        let end = motion::right(text, self.cursor, count, true);
        if end <= self.cursor {
            self.reset_pending();
            return handled(Vec::new());
        }
        let range = self.cursor..end;
        let mapped: String = text[range.clone()].chars().map(toggle_case).collect();
        self.record_simple(count, &[VimKey::Char('~')]);
        self.reset_pending();
        let cursor = clamp_normal(text, end);
        self.cursor = cursor;
        handled(vec![VimEffect::Edit {
            range,
            text: mapped,
            cursor,
        }])
    }

    fn repeat_last_change(&mut self, doc: &VimDocument<'_>) -> VimResponse {
        self.reset_pending();
        if self.replaying {
            return handled(Vec::new());
        }
        let Some(repeat) = self.repeat.clone() else {
            return handled(Vec::new());
        };
        self.replaying = true;
        let mut effects = Vec::new();
        for key in repeat.keys.iter().cloned() {
            let mut response = self.handle_key(doc, key);
            effects.append(&mut response.effects);
            if self.mode.is_inserting() {
                break;
            }
        }
        if self.mode.is_inserting() {
            if !repeat.inserted.is_empty() {
                let at = self.cursor;
                let cursor = at + repeat.inserted.len();
                effects.push(VimEffect::Edit {
                    range: at..at,
                    text: repeat.inserted.clone(),
                    cursor,
                });
                self.cursor = cursor;
            }
            self.mode = VimMode::Normal;
            self.cursor = prev_boundary(doc.text, self.cursor);
        }
        self.replaying = false;
        self.repeat = Some(repeat);
        effects.push(VimEffect::ScrollToCursor);
        handled(effects)
    }

    // ---- helpers ----------------------------------------------------------

    /// The visual selection as an operator range.
    fn visual_range(&self, text: &str) -> (Range<usize>, bool) {
        let (start, end) = ordered(self.visual_anchor, self.cursor);
        match self.mode {
            VimMode::VisualLine => (line_start(text, start)..line_end(text, end), true),
            _ => (start..next_boundary(text, end), false),
        }
    }

    fn move_caret(&mut self, text: &str, offset: usize) {
        self.cursor = clamp_normal(text, offset);
        self.goal_column = motion::column(text, self.cursor);
    }

    fn indent_unit(&self) -> String {
        if self.config.use_tabs {
            "\t".to_string()
        } else {
            " ".repeat(self.config.shift_width)
        }
    }

    /// The effective count: `2d3w` deletes six words, so the counts multiply.
    fn peek_count(&self) -> usize {
        self.peek_count_raw().unwrap_or(1)
    }

    fn peek_count_raw(&self) -> Option<usize> {
        match (self.count, self.operator_count) {
            (Some(a), Some(b)) => Some(a * b),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn active_count(&self) -> Option<usize> {
        if self.operator.is_some() {
            self.operator_count
        } else {
            self.count
        }
    }

    fn take_count(&mut self) -> usize {
        let count = self.peek_count();
        self.count = None;
        self.operator_count = None;
        count
    }

    fn reset_pending(&mut self) {
        self.count = None;
        self.operator = None;
        self.operator_count = None;
        self.register = None;
        self.awaiting = None;
        self.pending_keys.clear();
    }

    /// Yanks fill the unnamed register and register `0`; deletes fill the
    /// unnamed register and `1`. An uppercase name appends; `_` discards.
    fn store_register(&mut self, name: Option<char>, text: String, line_wise: bool, yank: bool) {
        let content = RegisterContent { text, line_wise };
        match name {
            Some('_') => return,
            Some(name) if name.is_ascii_uppercase() => {
                let entry = self.registers.entry(name.to_ascii_lowercase()).or_default();
                entry.text.push_str(&content.text);
                entry.line_wise |= line_wise;
            }
            Some(name) => {
                self.registers.insert(name, content.clone());
            }
            None => {}
        }
        self.registers.insert('"', content.clone());
        self.registers.insert(if yank { '0' } else { '1' }, content);
    }

    /// Begin buffering the keys of a change so `.` can replay it. Subsequent
    /// keys are appended by [`VimEngine::handle_key`].
    fn start_recording(&mut self, keys: &[VimKey]) {
        if self.replaying {
            return;
        }
        self.recording = Some(Repeat {
            keys: keys.to_vec(),
            inserted: String::new(),
        });
    }

    /// Remember a self-contained change (`x`, `r`, `p`, `J`, `~`) whose keys
    /// were never accumulated, count included so `.` repeats `3x` as `3x`.
    fn record_simple(&mut self, count: usize, keys: &[VimKey]) {
        if self.replaying {
            return;
        }
        let mut recorded = count_keys(count);
        recorded.extend_from_slice(keys);
        self.recording = None;
        self.repeat = Some(Repeat {
            keys: recorded,
            inserted: String::new(),
        });
    }

    /// Close out an operator change whose keys `handle_key` accumulated.
    /// Returns the buffer so a command that opened insert mode can keep
    /// recording the text still to be typed (see
    /// [`VimEngine::restore_recording`]).
    fn finish_change(&mut self) -> Option<Repeat> {
        if self.replaying {
            return None;
        }
        let recording = self.recording.take()?;
        self.repeat = Some(recording.clone());
        Some(recording)
    }

    fn restore_recording(&mut self, recording: Option<Repeat>) {
        if self.mode.is_inserting() {
            self.recording = recording;
        }
    }
}

fn handled(effects: Vec<VimEffect>) -> VimResponse {
    VimResponse {
        handled: true,
        effects,
    }
}

fn ordered(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}

/// The one-character block the caret paints as in normal mode.
fn block_at(text: &str, cursor: usize) -> Range<usize> {
    let cursor = clamp_offset(text, cursor);
    match char_at(text, cursor) {
        Some('\n') | None => cursor..cursor,
        Some(_) => cursor..next_boundary(text, cursor),
    }
}

fn operator_for(c: char) -> Option<Operator> {
    match c {
        'd' => Some(Operator::Delete),
        'c' => Some(Operator::Change),
        'y' => Some(Operator::Yank),
        '>' => Some(Operator::Indent),
        '<' => Some(Operator::Outdent),
        '=' => Some(Operator::Format),
        _ => None,
    }
}

fn count_keys(count: usize) -> Vec<VimKey> {
    if count <= 1 {
        return Vec::new();
    }
    count.to_string().chars().map(VimKey::Char).collect()
}

/// A line-wise register always ends in a newline, even when the last line of
/// the document does not.
fn register_text(text: &str, range: &Range<usize>, line_wise: bool) -> String {
    if line_wise {
        format!("{}\n", &text[range.clone()])
    } else {
        text[range.clone()].to_string()
    }
}

fn toggle_case(c: char) -> char {
    if c.is_lowercase() {
        c.to_uppercase().next().unwrap_or(c)
    } else if c.is_uppercase() {
        c.to_lowercase().next().unwrap_or(c)
    } else {
        c
    }
}

/// Whitespace prefix of the line containing `offset`.
fn leading_indent(text: &str, offset: usize) -> String {
    let start = line_start(text, offset);
    let end = line_end(text, offset);
    text[start..end]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

fn leading_indent_len(text: &str) -> usize {
    text.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(char::len_utf8)
        .sum()
}

fn last_non_blank(text: &str, offset: usize) -> usize {
    let start = line_start(text, offset);
    let end = line_end(text, offset);
    let trimmed = text[start..end].trim_end();
    if trimmed.is_empty() {
        return start;
    }
    prev_boundary(text, start + trimmed.len())
}

/// Extend a line-wise range over its terminator so `dd` removes the line
/// rather than leaving a blank one. On the last line there is no trailing
/// newline to take, so the preceding one goes instead.
fn with_line_terminator(text: &str, range: Range<usize>) -> Range<usize> {
    if range.end < text.len() {
        return range.start..next_boundary(text, range.end);
    }
    if range.start > 0 {
        return prev_boundary(text, range.start)..range.end;
    }
    range
}

fn outdent_line(line: &str, unit: &str) -> String {
    if let Some(rest) = line.strip_prefix(unit) {
        return rest.to_string();
    }
    if let Some(rest) = line.strip_prefix('\t') {
        return rest.to_string();
    }
    let removable = line
        .chars()
        .take(unit.chars().count())
        .take_while(|c| *c == ' ')
        .count();
    line[removable..].to_string()
}

/// After deleting `range`, vim keeps the caret on the same character index
/// unless that would leave it past the end of the now-shorter line.
fn clamp_after_delete(text: &str, range: &Range<usize>) -> usize {
    let start = range.start;
    let start_of_line = line_start(text, start);
    let end_of_line = line_end(text, start);
    let removed = range.end.min(end_of_line).saturating_sub(start);
    let new_end = end_of_line - removed;
    if start < new_end {
        start
    } else if new_end > start_of_line {
        prev_boundary(text, new_end)
    } else {
        start_of_line
    }
}

/// Translate the vim-isms that differ from Rust regex, then compile. Returns
/// `None` for a pattern that will not compile so the caller can fall back to
/// a literal match rather than erroring.
fn compile(pattern: &str) -> Option<regex::Regex> {
    let translated = pattern.replace("\\<", "\\b").replace("\\>", "\\b");
    regex::Regex::new(&translated).ok()
}

fn search_offset(
    text: &str,
    cursor: usize,
    pattern: &str,
    forward: bool,
    count: usize,
) -> Option<usize> {
    let Some(regex) = compile(pattern) else {
        return motion::search(text, cursor, pattern, forward, count);
    };
    let mut current = cursor;
    for _ in 0..count {
        current = if forward {
            let from = next_boundary(text, current);
            regex
                .find_at(text, from)
                .map(|m| m.start())
                .or_else(|| regex.find(text).map(|m| m.start()))?
        } else {
            regex
                .find_iter(text)
                .map(|m| m.start())
                .filter(|start| *start < current)
                .last()
                .or_else(|| regex.find_iter(text).map(|m| m.start()).last())?
        };
    }
    Some(current)
}
