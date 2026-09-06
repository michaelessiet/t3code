//! Text objects: the `iw`/`aw`, `i(`/`a(`, `i"`/`a"` … ranges an operator
//! can take instead of a motion.
//!
//! All ranges are half-open byte ranges, so `d` and `c` can hand them
//! straight to the editor as a replacement span.

use std::ops::Range;

use super::motion::{
    CharClass, char_at, clamp_offset, class, line_end, line_start, next_boundary, prev_boundary,
};

/// Which text object a `i`/`a` prefix selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextObject {
    /// `w` — a word; `W` when `big`.
    Word { big: bool },
    /// A bracket pair, named by its opening character.
    Bracket(char),
    /// A quote pair.
    Quote(char),
    /// `p` — a paragraph (a run of non-blank lines).
    Paragraph,
}

impl TextObject {
    /// Map the character after `i`/`a` to an object. Closing brackets and the
    /// `b`/`B` aliases resolve to the same pair as their opener, as in vim.
    pub fn from_key(key: char) -> Option<Self> {
        match key {
            'w' => Some(TextObject::Word { big: false }),
            'W' => Some(TextObject::Word { big: true }),
            '(' | ')' | 'b' => Some(TextObject::Bracket('(')),
            '[' | ']' => Some(TextObject::Bracket('[')),
            '{' | '}' | 'B' => Some(TextObject::Bracket('{')),
            '<' | '>' => Some(TextObject::Bracket('<')),
            '"' => Some(TextObject::Quote('"')),
            '\'' => Some(TextObject::Quote('\'')),
            '`' => Some(TextObject::Quote('`')),
            'p' => Some(TextObject::Paragraph),
            _ => None,
        }
    }
}

/// Resolve a text object at `offset`. `around` is the `a` flavour (include
/// the delimiters / trailing whitespace); `false` is the `i` flavour.
pub fn resolve(
    text: &str,
    offset: usize,
    object: TextObject,
    around: bool,
) -> Option<Range<usize>> {
    match object {
        TextObject::Word { big } => word(text, offset, big, around),
        TextObject::Bracket(open) => bracket(text, offset, open, around),
        TextObject::Quote(quote) => quoted(text, offset, quote, around),
        TextObject::Paragraph => paragraph(text, offset, around),
    }
}

/// `iw` is the run of same-class characters under the caret (whitespace runs
/// count as an object of their own). `aw` additionally swallows the following
/// whitespace, or the preceding whitespace when there is none after.
fn word(text: &str, offset: usize, big: bool, around: bool) -> Option<Range<usize>> {
    let offset = clamp_offset(text, offset);
    let here = char_at(text, offset)?;
    let target = class(here, big);
    let mut start = offset;
    while start > 0 {
        let previous = prev_boundary(text, start);
        match char_at(text, previous) {
            Some(c) if class(c, big) == target && c != '\n' => start = previous,
            _ => break,
        }
    }
    let mut end = offset;
    while let Some(c) = char_at(text, end) {
        if class(c, big) != target || c == '\n' {
            break;
        }
        end = next_boundary(text, end);
    }
    if !around {
        return Some(start..end);
    }
    let mut trailing = end;
    while let Some(c) = char_at(text, trailing) {
        if class(c, big) != CharClass::Blank || c == '\n' {
            break;
        }
        trailing = next_boundary(text, trailing);
    }
    if trailing > end {
        return Some(start..trailing);
    }
    let mut leading = start;
    while leading > 0 {
        let previous = prev_boundary(text, leading);
        match char_at(text, previous) {
            Some(c) if class(c, big) == CharClass::Blank && c != '\n' => leading = previous,
            _ => break,
        }
    }
    Some(leading..end)
}

/// The innermost pair enclosing (or starting at) the caret.
fn bracket(text: &str, offset: usize, open: char, around: bool) -> Option<Range<usize>> {
    let close = match open {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '<' => '>',
        _ => return None,
    };
    let offset = clamp_offset(text, offset);
    // Standing on the opener counts as being inside the pair.
    let start = if char_at(text, offset) == Some(open) {
        offset
    } else {
        scan_open(text, offset, open, close)?
    };
    let end = scan_close(text, start, open, close)?;
    if around {
        Some(start..next_boundary(text, end))
    } else {
        Some(next_boundary(text, start)..end)
    }
}

fn scan_open(text: &str, offset: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0i32;
    let mut current = offset;
    loop {
        let c = char_at(text, current)?;
        if c == close && current != offset {
            depth += 1;
        } else if c == open {
            if depth == 0 {
                return Some(current);
            }
            depth -= 1;
        }
        if current == 0 {
            return None;
        }
        current = prev_boundary(text, current);
    }
}

fn scan_close(text: &str, open_at: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0i32;
    let mut current = open_at;
    while current < text.len() {
        let c = char_at(text, current)?;
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(current);
            }
        }
        current = next_boundary(text, current);
    }
    None
}

/// Quote objects are line-scoped in vim: the pair must open and close on the
/// caret's line, and the scan starts from the line start so `ci"` works with
/// the caret anywhere inside (or before) the string.
fn quoted(text: &str, offset: usize, quote: char, around: bool) -> Option<Range<usize>> {
    let offset = clamp_offset(text, offset);
    let start_of_line = line_start(text, offset);
    let end_of_line = line_end(text, offset);

    let mut open: Option<usize> = None;
    let mut current = start_of_line;
    while current < end_of_line {
        let c = char_at(text, current)?;
        if c == '\\' {
            current = next_boundary(text, next_boundary(text, current));
            continue;
        }
        if c == quote {
            match open {
                None => open = Some(current),
                Some(start) => {
                    if offset <= current {
                        return Some(if around {
                            start..next_boundary(text, current)
                        } else {
                            next_boundary(text, start)..current
                        });
                    }
                    open = None;
                }
            }
        }
        current = next_boundary(text, current);
    }
    None
}

/// `ip` is the run of non-blank lines around the caret (or the run of blank
/// lines when it is on one); `ap` also takes the blank lines that follow.
fn paragraph(text: &str, offset: usize, around: bool) -> Option<Range<usize>> {
    let offset = clamp_offset(text, offset);
    let blank = |at: usize| line_end(text, at) == line_start(text, at);
    let on_blank = blank(offset);

    let mut start = line_start(text, offset);
    while start > 0 {
        let previous = line_start(text, prev_boundary(text, start));
        if blank(previous) != on_blank {
            break;
        }
        start = previous;
    }
    let mut end = line_end(text, offset);
    while end < text.len() {
        let next = next_boundary(text, end);
        if blank(next) != on_blank {
            break;
        }
        end = line_end(text, next);
    }
    if !around {
        return Some(start..end);
    }
    let mut extended = end;
    while extended < text.len() {
        let next = next_boundary(text, extended);
        if blank(next) == on_blank {
            break;
        }
        extended = line_end(text, next);
    }
    Some(start..extended)
}
