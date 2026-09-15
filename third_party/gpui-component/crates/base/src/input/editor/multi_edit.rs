//! Simultaneous replacements and linked snippet placeholders. A multi-edit is
//! one atomic history entry, so undo never leaves half the cursors edited.
use crate::input::{InputBaseState, InputModeKind, RopeExt};
use gpui::{Context, Window};
use std::{collections::BTreeMap, ops::Range};

#[derive(Debug, PartialEq)]
struct Snippet {
    text: String,
    stops: Vec<Vec<Range<usize>>>,
}

/// The portable, non-executing subset of TextMate snippets: $n,
/// ${n:default}, ${n|first,second|}, mirrored stops, $0 and escaped literals.
/// Unsupported variables/transforms are rejected, never inserted as code.
fn parse_snippet(source: &str) -> Result<Snippet, String> {
    let mut text = String::new();
    let mut stops = BTreeMap::<u32, Vec<Range<usize>>>::new();
    let mut values = BTreeMap::<u32, String>::new();
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek() {
                Some('$' | '}' | '\\') => text.push(chars.next().unwrap()),
                _ => text.push(ch),
            }
            continue;
        }
        if ch != '$' {
            text.push(ch);
            continue;
        }
        let braced = chars.peek() == Some(&'{');
        if braced {
            chars.next();
        }
        let mut digits = String::new();
        while chars.peek().is_some_and(char::is_ascii_digit) {
            digits.push(chars.next().unwrap());
        }
        if digits.is_empty() {
            return Err("Snippet variables and transformations are not supported".into());
        }
        let index: u32 = digits.parse().map_err(|_| "Invalid snippet tab stop")?;
        let mut value = String::new();
        if braced {
            match chars.next() {
                Some('}') => {}
                Some(':') => {
                    let mut closed = false;
                    while let Some(ch) = chars.next() {
                        if ch == '}' {
                            closed = true;
                            break;
                        }
                        if ch == '$' {
                            return Err("Nested snippet placeholders are not supported".into());
                        }
                        if ch == '\\' {
                            value.push(chars.next().ok_or("Incomplete snippet escape")?);
                        } else {
                            value.push(ch);
                        }
                    }
                    if !closed {
                        return Err("Unclosed snippet placeholder".into());
                    }
                }
                Some('|') => {
                    let mut options = String::new();
                    let mut closed = false;
                    while let Some(ch) = chars.next() {
                        if ch == '|' && chars.peek() == Some(&'}') {
                            chars.next();
                            closed = true;
                            break;
                        }
                        options.push(ch);
                    }
                    if !closed {
                        return Err("Unclosed snippet choices".into());
                    }
                    value = options.split(',').next().unwrap_or_default().into();
                }
                _ => return Err("Unsupported snippet expression".into()),
            }
        }
        let value = values.entry(index).or_insert(value);
        let start = text.len();
        text.push_str(value);
        stops.entry(index).or_default().push(start..text.len());
    }
    let finish = stops
        .remove(&0)
        .unwrap_or_else(|| vec![text.len()..text.len()]);
    let mut stops = stops.into_values().collect::<Vec<_>>();
    stops.push(finish);
    Ok(Snippet { text, stops })
}

fn combined_edit(
    text: &str,
    ranges: &[Range<usize>],
    replacement: &str,
) -> (Range<usize>, String, Vec<Range<usize>>) {
    let start = ranges.first().unwrap().start;
    let end = ranges.last().unwrap().end;
    let mut value = String::new();
    let mut cursor = start;
    let mut carets = Vec::new();
    for range in ranges {
        value.push_str(&text[cursor..range.start]);
        value.push_str(replacement);
        let caret = start + value.len();
        carets.push(caret..caret);
        cursor = range.end;
    }
    (start..end, value, carets)
}

impl<M: InputModeKind> InputBaseState<M> {
    pub(super) fn clear_multi_edit(&mut self) {
        self.secondary_selections.clear();
        self.snippet_stops.clear();
    }

    pub fn selection_count(&self) -> usize {
        1 + self.secondary_selections.len()
    }

    pub fn selections(&self) -> Vec<Range<usize>> {
        let mut ranges = self.secondary_selections.clone();
        ranges.push(self.selected_range.into());
        ranges.sort_by_key(|r| (r.start, r.end));
        ranges
    }

    pub fn add_selection(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        if !self.is_code_editor()
            || range.start > range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return;
        }
        if self
            .selections()
            .iter()
            .any(|r| *r == range || (r.start < range.end && range.start < r.end))
        {
            return;
        }
        self.secondary_selections.push(self.selected_range.into());
        self.secondary_selections.sort_by_key(|r| (r.start, r.end));
        self.selected_range = range.into();
        self.scroll_to(self.cursor(), None, cx);
        cx.notify();
    }

    pub fn select_next_occurrence(&mut self, all: bool, cx: &mut Context<Self>) {
        if !self.is_code_editor() {
            return;
        }
        self.snippet_stops.clear();
        let text = self.text.to_string();
        if self.selected_range.is_empty() {
            let cursor = self.cursor();
            let word = |c: char| c == '_' || c.is_alphanumeric();
            let start = text[..cursor]
                .char_indices()
                .rev()
                .take_while(|(_, c)| word(*c))
                .last()
                .map_or(cursor, |(i, _)| i);
            let end = text[cursor..]
                .char_indices()
                .take_while(|(_, c)| word(*c))
                .last()
                .map_or(cursor, |(i, c)| cursor + i + c.len_utf8());
            self.selected_range = (start..end).into();
            if !all {
                cx.notify();
                return;
            }
        }
        let selection: Range<usize> = self.selected_range.into();
        if selection.is_empty() {
            return;
        }
        let needle = &text[selection.clone()];
        let mut matches = text
            .match_indices(needle)
            .map(|(i, _)| i..i + needle.len())
            .collect::<Vec<_>>();
        if all {
            self.secondary_selections = matches
                .into_iter()
                .filter(|range| range.end <= selection.start || range.start >= selection.end)
                .collect();
            cx.notify();
            return;
        }
        // Next occurrence wraps once, without duplicating existing cursors.
        matches.sort_by_key(|r| (r.start < selection.end, r.start));
        for range in matches {
            let old = self.selection_count();
            self.add_selection(range, cx);
            if !all && self.selection_count() != old {
                break;
            }
        }
    }

    pub fn add_cursor_vertical(&mut self, down: bool, cx: &mut Context<Self>) {
        let position = self.text.offset_to_position(self.cursor());
        let line = if down {
            position.line.saturating_add(1)
        } else {
            position.line.saturating_sub(1)
        };
        if line as usize >= self.text.lines_len() {
            return;
        }
        let offset = self
            .text
            .position_to_offset(&lsp_types::Position::new(line, position.character));
        self.snippet_stops.clear();
        self.add_selection(offset..offset, cx);
    }

    pub(super) fn delete_multi(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ranges = self
            .selections()
            .into_iter()
            .map(|range| {
                if !range.is_empty() {
                    range
                } else if forward {
                    range.start..self.next_boundary(range.end)
                } else {
                    self.previous_boundary(range.start)..range.end
                }
            })
            .collect::<Vec<_>>();
        // Adjacent caret deletions may overlap after grapheme expansion.
        let mut merged: Vec<Range<usize>> = Vec::new();
        for range in ranges {
            if let Some(previous) = merged.last_mut()
                && previous.end >= range.start
            {
                previous.end = previous.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        self.selected_range = merged.remove(0).into();
        self.secondary_selections = merged;
        self.replace_multi("", window, cx);
    }

    pub(super) fn replace_multi(
        &mut self,
        replacement: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_editable() {
            return;
        }
        let ranges = self.selections();
        let replacement = replacement.replace("\r\n", "\n");
        let mut stops = self.snippet_stops.clone();
        let index = self.snippet_index;
        // Map every tab stop through each independent edit, not through the
        // encompassing history range (which includes untouched text).
        for (group_index, group) in stops.iter_mut().enumerate() {
            for stop in group {
                let map = |offset: usize, right: bool| {
                    let mut delta = 0isize;
                    for range in &ranges {
                        if offset < range.start {
                            break;
                        }
                        // Adjacent placeholders share boundaries. An edit
                        // ending at the next stop moves that stop after the
                        // replacement; it must not select the previous text.
                        if !range.is_empty() && offset == range.end {
                            delta += replacement.len() as isize - range.len() as isize;
                            continue;
                        }
                        if offset <= range.end {
                            let right = if range.is_empty() && group_index != index {
                                group_index > index
                            } else {
                                right && (range.is_empty() || offset > range.start)
                            };
                            return (range.start as isize + delta) as usize
                                + if right { replacement.len() } else { 0 };
                        }
                        delta += replacement.len() as isize - range.len() as isize;
                    }
                    offset.saturating_add_signed(delta)
                };
                *stop = map(stop.start, false)..map(stop.end, true);
            }
        }
        let primary = ranges
            .iter()
            .position(|r| *r == Range::from(self.selected_range))
            .unwrap_or(0);
        let (range, value, mut carets) =
            combined_edit(&self.text.to_string(), &ranges, &replacement);
        self.clear_multi_edit();
        self.edit_range(range, &value, window, cx);
        self.selected_range = carets.remove(primary).into();
        self.secondary_selections = carets;
        self.snippet_stops = stops;
        self.snippet_index = index;
        self.scroll_to(self.cursor(), None, cx);
        cx.notify();
    }

    pub fn insert_snippet(
        &mut self,
        source: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.is_editable() {
            return Err("The editor is read-only".into());
        }
        let mut snippet = parse_snippet(source)?;
        let range: Range<usize> = self.selected_range.into();
        // Preserve the current line's indentation for subsequent lines.
        let prefix = self.text.slice(0..range.start).to_string();
        let indent: String = prefix
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if !indent.is_empty() {
            for group in &mut snippet.stops {
                for stop in group {
                    let start = snippet.text[..stop.start]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count()
                        * indent.len();
                    let end = snippet.text[..stop.end]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count()
                        * indent.len();
                    *stop = stop.start + start..stop.end + end;
                }
            }
            snippet.text = snippet.text.replace('\n', &format!("\n{indent}"));
        }
        self.edit_range(range.clone(), &snippet.text, window, cx);
        self.snippet_stops = snippet
            .stops
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .map(|r| range.start + r.start..range.start + r.end)
                    .collect()
            })
            .collect();
        self.snippet_index = 0;
        self.select_snippet_stop(cx);
        Ok(())
    }

    fn select_snippet_stop(&mut self, cx: &mut Context<Self>) {
        let mut stops = self.snippet_stops[self.snippet_index].clone();
        self.selected_range = stops.remove(0).into();
        self.secondary_selections = stops;
        if self.snippet_index + 1 == self.snippet_stops.len() {
            self.snippet_stops.clear();
        }
        self.scroll_to(self.cursor(), None, cx);
        cx.notify();
    }

    pub(super) fn snippet_tab(&mut self, backwards: bool, cx: &mut Context<Self>) -> bool {
        if self.snippet_stops.is_empty() {
            return false;
        }
        self.snippet_index = if backwards {
            self.snippet_index.saturating_sub(1)
        } else {
            (self.snippet_index + 1).min(self.snippet_stops.len() - 1)
        };
        self.select_snippet_stop(cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_ordered_linked_placeholders_and_final_stop() {
        let snippet = parse_snippet("${2:😀} ${1:name} = $1; $0").unwrap();
        assert_eq!(snippet.text, "😀 name = name; ");
        assert_eq!(snippet.stops[0], [5..9, 12..16]);
        assert_eq!(snippet.stops[1], [0..4]);
        assert_eq!(snippet.stops[2], [18..18]);
        assert!(parse_snippet("${1:missing").is_err());
        assert!(parse_snippet("${TM_FILENAME}").is_err());
    }
    #[test]
    fn simultaneous_edits_preserve_intervening_unicode() {
        assert_eq!(
            combined_edit("one 😀 one", &[0..3, 9..12], "x"),
            (0..12, "x 😀 x".into(), vec![1..1, 8..8])
        );
    }
}
