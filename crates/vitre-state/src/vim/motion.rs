//! Cursor motions over a UTF-8 document, in byte offsets.
//!
//! Byte offsets are what the fork's `InputState` speaks (`cursor()`,
//! `selected_range()`, `set_selected_range()` all clip against char
//! boundaries of the underlying rope), so the engine never converts.
//!
//! Vim's normal-mode caret sits *on* a character and may not rest on the
//! line terminator, so most motions clamp with [`clamp_normal`]. Operator
//! motions deliberately do not — `d$` must be able to reach the newline.

use std::ops::Range;

/// Vim's three character classes. `w`/`b`/`e` step between runs of the same
/// class; `W`/`B`/`E` fold `Word` and `Punct` together (see [`class`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharClass {
    Blank,
    Word,
    Punct,
}

/// Classify `c`. With `big`, every non-blank is a `Word` char, which is what
/// makes `W`/`B`/`E` treat `foo.bar(baz)` as one word.
pub fn class(c: char, big: bool) -> CharClass {
    if c == '\n' || c.is_whitespace() {
        CharClass::Blank
    } else if big || c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punct
    }
}

/// Byte offset of the first character of the line containing `offset`.
pub fn line_start(text: &str, offset: usize) -> usize {
    let offset = clamp_offset(text, offset);
    text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Byte offset of the line terminator (or `text.len()` on the last line).
pub fn line_end(text: &str, offset: usize) -> usize {
    let offset = clamp_offset(text, offset);
    text[offset..]
        .find('\n')
        .map(|i| offset + i)
        .unwrap_or(text.len())
}

/// First non-blank character of the line, or the line end when it is blank.
pub fn first_non_blank(text: &str, offset: usize) -> usize {
    let start = line_start(text, offset);
    let end = line_end(text, offset);
    for (i, c) in text[start..end].char_indices() {
        if !c.is_whitespace() {
            return start + i;
        }
    }
    end
}

/// Zero-based line index of `offset`.
pub fn line_index(text: &str, offset: usize) -> usize {
    let offset = clamp_offset(text, offset);
    text[..offset].matches('\n').count()
}

/// Byte offset where zero-based line `line` starts. Lines past the end clamp
/// to the start of the last line.
pub fn offset_of_line(text: &str, line: usize) -> usize {
    let mut remaining = line;
    let mut last_start = 0;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            if remaining == 0 {
                return last_start;
            }
            remaining -= 1;
            last_start = i + 1;
        }
    }
    last_start
}

/// Total number of lines. A trailing newline does NOT open a new line, which
/// matches how vim counts (`G` on `"a\n"` lands on line 1).
pub fn line_count(text: &str) -> usize {
    if text.is_empty() {
        return 1;
    }
    let trailing = usize::from(text.ends_with('\n'));
    text.matches('\n').count() + 1 - trailing
}

/// Snap `offset` to a char boundary at or before it, inside `text`.
pub fn clamp_offset(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Pull the caret off the line terminator, as vim's normal mode requires.
/// An empty line has nowhere else to go, so it stays on the line start.
pub fn clamp_normal(text: &str, offset: usize) -> usize {
    let offset = clamp_offset(text, offset);
    let start = line_start(text, offset);
    let end = line_end(text, offset);
    if offset < end {
        return offset;
    }
    if end == start {
        return start;
    }
    prev_boundary(text, end)
}

/// Byte offset one character to the right, saturating at `text.len()`.
pub fn next_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_offset(text, offset);
    match text[offset..].chars().next() {
        Some(c) => offset + c.len_utf8(),
        None => offset,
    }
}

/// Byte offset one character to the left, saturating at 0.
pub fn prev_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_offset(text, offset);
    match text[..offset].chars().next_back() {
        Some(c) => offset - c.len_utf8(),
        None => offset,
    }
}

/// The character at `offset`, if the caret is not at the end of the document.
pub fn char_at(text: &str, offset: usize) -> Option<char> {
    text[clamp_offset(text, offset)..].chars().next()
}

/// `h`: left within the line — vim never wraps to the previous line.
pub fn left(text: &str, offset: usize, count: usize) -> usize {
    let start = line_start(text, offset);
    let mut offset = clamp_offset(text, offset);
    for _ in 0..count {
        if offset <= start {
            break;
        }
        offset = prev_boundary(text, offset);
    }
    offset
}

/// `l`: right within the line. `for_operator` allows the newline position so
/// `dl` on the last character of a line still deletes it.
pub fn right(text: &str, offset: usize, count: usize, for_operator: bool) -> usize {
    let end = line_end(text, offset);
    let limit = if for_operator {
        end
    } else {
        let start = line_start(text, offset);
        if end == start {
            start
        } else {
            prev_boundary(text, end)
        }
    };
    let mut offset = clamp_offset(text, offset);
    for _ in 0..count {
        if offset >= limit {
            break;
        }
        offset = next_boundary(text, offset);
    }
    offset.min(limit)
}

/// `j`/`k`: vertical movement holding a "goal column" measured in characters,
/// the way vim keeps a long line's column across short ones.
pub fn vertical(text: &str, offset: usize, count: isize, goal_column: usize) -> usize {
    let line = line_index(text, offset) as isize;
    let target = (line + count).clamp(0, line_count(text) as isize - 1) as usize;
    let start = offset_of_line(text, target);
    let end = line_end(text, start);
    let mut current = start;
    for _ in 0..goal_column {
        if current >= end {
            break;
        }
        current = next_boundary(text, current);
    }
    current
}

/// Character column of `offset` within its line — the goal column `j`/`k` keep.
pub fn column(text: &str, offset: usize) -> usize {
    let start = line_start(text, offset);
    text[start..clamp_offset(text, offset)].chars().count()
}

/// `w` / `W`: start of the next word.
///
/// Empty lines are words in vim, so the blank-skipping stops on one rather
/// than running to the next non-blank.
pub fn next_word_start(text: &str, offset: usize, big: bool) -> usize {
    let mut offset = clamp_offset(text, offset);
    let Some(first) = char_at(text, offset) else {
        return text.len();
    };
    let start_class = class(first, big);
    if start_class != CharClass::Blank {
        while let Some(c) = char_at(text, offset) {
            if class(c, big) != start_class {
                break;
            }
            offset = next_boundary(text, offset);
        }
    }
    skip_blanks_stopping_on_empty_line(text, offset)
}

/// `b` / `B`: start of the current word, or of the previous one when already
/// standing on a word's first character.
pub fn prev_word_start(text: &str, offset: usize, big: bool) -> usize {
    let mut offset = clamp_offset(text, offset);
    if offset == 0 {
        return 0;
    }
    offset = prev_boundary(text, offset);
    // Walk back over blanks, but an empty line is itself a word.
    while offset > 0 {
        let Some(c) = char_at(text, offset) else {
            break;
        };
        if class(c, big) != CharClass::Blank {
            break;
        }
        if is_empty_line_start(text, offset) {
            return offset;
        }
        offset = prev_boundary(text, offset);
    }
    let Some(c) = char_at(text, offset) else {
        return offset;
    };
    let target = class(c, big);
    if target == CharClass::Blank {
        return offset;
    }
    while offset > 0 {
        let previous = prev_boundary(text, offset);
        match char_at(text, previous) {
            Some(p) if class(p, big) == target => offset = previous,
            _ => break,
        }
    }
    offset
}

/// `e` / `E`: end of the current word, or of the next one when already on it.
pub fn next_word_end(text: &str, offset: usize, big: bool) -> usize {
    let mut offset = clamp_offset(text, offset);
    if offset >= text.len() {
        return offset;
    }
    offset = next_boundary(text, offset);
    while offset < text.len() {
        let Some(c) = char_at(text, offset) else {
            break;
        };
        if class(c, big) != CharClass::Blank {
            break;
        }
        offset = next_boundary(text, offset);
    }
    let Some(c) = char_at(text, offset) else {
        return prev_boundary(text, text.len());
    };
    let target = class(c, big);
    loop {
        let next = next_boundary(text, offset);
        match char_at(text, next) {
            Some(n) if class(n, big) == target => offset = next,
            _ => break,
        }
    }
    offset
}

/// `ge` / `gE`: end of the previous word.
pub fn prev_word_end(text: &str, offset: usize, big: bool) -> usize {
    let mut offset = clamp_offset(text, offset);
    if offset == 0 {
        return 0;
    }
    offset = prev_boundary(text, offset);
    // Step off the run the caret was standing in.
    if let Some(c) = char_at(text, offset)
        && class(c, big) != CharClass::Blank
    {
        let target = class(c, big);
        while offset > 0 {
            let previous = prev_boundary(text, offset);
            match char_at(text, previous) {
                Some(p) if class(p, big) == target => offset = previous,
                _ => break,
            }
        }
        if offset == 0 {
            return 0;
        }
        offset = prev_boundary(text, offset);
    }
    while offset > 0 {
        let Some(c) = char_at(text, offset) else {
            break;
        };
        if class(c, big) != CharClass::Blank {
            break;
        }
        offset = prev_boundary(text, offset);
    }
    offset
}

/// `{` / `}`: paragraph boundaries — the next line that is empty, scanning in
/// `forward` direction, or the edge of the document.
pub fn paragraph(text: &str, offset: usize, forward: bool) -> usize {
    let mut line = line_index(text, offset);
    let last = line_count(text) - 1;
    loop {
        if forward {
            if line >= last {
                return text.len();
            }
            line += 1;
        } else {
            if line == 0 {
                return 0;
            }
            line -= 1;
        }
        let start = offset_of_line(text, line);
        if line_end(text, start) == start {
            return start;
        }
    }
}

/// `f` / `F` / `t` / `T`: character search inside the current line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindKind {
    /// `f`: forward, landing on the match.
    Find,
    /// `F`: backward, landing on the match.
    FindBack,
    /// `t`: forward, landing just before the match.
    Till,
    /// `T`: backward, landing just after the match.
    TillBack,
}

impl FindKind {
    /// The direction `;` repeats in; `,` uses the reverse.
    pub fn is_forward(self) -> bool {
        matches!(self, FindKind::Find | FindKind::Till)
    }

    /// `,` searches the same target the other way round.
    pub fn reversed(self) -> Self {
        match self {
            FindKind::Find => FindKind::FindBack,
            FindKind::FindBack => FindKind::Find,
            FindKind::Till => FindKind::TillBack,
            FindKind::TillBack => FindKind::Till,
        }
    }
}

/// Run a character search `count` times. Returns `None` when any repetition
/// fails, matching vim's all-or-nothing behaviour.
///
/// `repeated` marks a `;`/`,` repeat: a `t` that would not move because the
/// caret already sits next to the match steps over it first, so `;` after
/// `tx` advances instead of standing still.
pub fn find_char(
    text: &str,
    offset: usize,
    kind: FindKind,
    target: char,
    count: usize,
    repeated: bool,
) -> Option<usize> {
    let start = line_start(text, offset);
    let end = line_end(text, offset);
    let mut current = clamp_offset(text, offset);
    for iteration in 0..count {
        let bump_over_adjacent = repeated && iteration == 0;
        current = match kind {
            FindKind::Find => scan_forward(text, current, end, target)?,
            FindKind::FindBack => scan_backward(text, current, start, target)?,
            FindKind::Till => {
                let from = if bump_over_adjacent {
                    next_boundary(text, current)
                } else {
                    current
                };
                prev_boundary(text, scan_forward(text, from, end, target)?)
            }
            FindKind::TillBack => {
                let from = if bump_over_adjacent {
                    prev_boundary(text, current)
                } else {
                    current
                };
                next_boundary(text, scan_backward(text, from, start, target)?)
            }
        };
    }
    Some(current)
}

fn scan_forward(text: &str, offset: usize, end: usize, target: char) -> Option<usize> {
    let mut current = next_boundary(text, offset);
    while current < end {
        if char_at(text, current) == Some(target) {
            return Some(current);
        }
        current = next_boundary(text, current);
    }
    None
}

fn scan_backward(text: &str, offset: usize, start: usize, target: char) -> Option<usize> {
    let mut current = offset;
    while current > start {
        current = prev_boundary(text, current);
        if char_at(text, current) == Some(target) {
            return Some(current);
        }
    }
    None
}

/// `%`: jump to the bracket matching the one at (or next on the line after)
/// the caret, honouring nesting.
pub fn matching_bracket(text: &str, offset: usize) -> Option<usize> {
    const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];
    let end = line_end(text, offset);
    let mut cursor = clamp_offset(text, offset);
    // vim scans forward on the line for the first bracket.
    let (open, close, forward, at) = loop {
        if cursor >= end {
            return None;
        }
        let c = char_at(text, cursor)?;
        if let Some(&(open, close)) = PAIRS.iter().find(|(open, _)| *open == c) {
            break (open, close, true, cursor);
        }
        if let Some(&(open, close)) = PAIRS.iter().find(|(_, close)| *close == c) {
            break (open, close, false, cursor);
        }
        cursor = next_boundary(text, cursor);
    };

    let mut depth = 0i32;
    let mut current = at;
    loop {
        let c = char_at(text, current)?;
        if c == open {
            depth += if forward { 1 } else { -1 };
        } else if c == close {
            depth += if forward { -1 } else { 1 };
        }
        if depth == 0 {
            return Some(current);
        }
        if forward {
            current = next_boundary(text, current);
            if current >= text.len() {
                return None;
            }
        } else {
            if current == 0 {
                return None;
            }
            current = prev_boundary(text, current);
        }
    }
}

/// The `count`-th match of `needle` from `offset`, wrapping around the
/// document the way vim's `/` and `?` do.
pub fn search(
    text: &str,
    offset: usize,
    needle: &str,
    forward: bool,
    count: usize,
) -> Option<usize> {
    if needle.is_empty() || text.is_empty() {
        return None;
    }
    let mut current = clamp_offset(text, offset);
    for _ in 0..count {
        current = if forward {
            search_forward(text, current, needle)?
        } else {
            search_backward(text, current, needle)?
        };
    }
    Some(current)
}

fn search_forward(text: &str, offset: usize, needle: &str) -> Option<usize> {
    let from = next_boundary(text, offset);
    if let Some(i) = text[from..].find(needle) {
        return Some(from + i);
    }
    text.find(needle)
}

fn search_backward(text: &str, offset: usize, needle: &str) -> Option<usize> {
    if let Some(i) = text[..offset].rfind(needle) {
        return Some(i);
    }
    text.rfind(needle)
}

/// The word under (or next on the line after) the caret — the `*`/`#` target.
pub fn word_under_cursor(text: &str, offset: usize) -> Option<Range<usize>> {
    let end_of_line = line_end(text, offset);
    let mut start = clamp_offset(text, offset);
    while start < end_of_line && class(char_at(text, start)?, false) != CharClass::Word {
        start = next_boundary(text, start);
    }
    if start >= end_of_line {
        return None;
    }
    while start > 0 {
        let previous = prev_boundary(text, start);
        match char_at(text, previous) {
            Some(c) if class(c, false) == CharClass::Word => start = previous,
            _ => break,
        }
    }
    let mut end = start;
    while let Some(c) = char_at(text, end) {
        if class(c, false) != CharClass::Word {
            break;
        }
        end = next_boundary(text, end);
    }
    Some(start..end)
}

fn skip_blanks_stopping_on_empty_line(text: &str, offset: usize) -> usize {
    let mut offset = clamp_offset(text, offset);
    while let Some(c) = char_at(text, offset) {
        if class(c, false) != CharClass::Blank {
            break;
        }
        offset = next_boundary(text, offset);
        if is_empty_line_start(text, offset) {
            break;
        }
    }
    offset
}

fn is_empty_line_start(text: &str, offset: usize) -> bool {
    line_start(text, offset) == offset && line_end(text, offset) == offset
}
