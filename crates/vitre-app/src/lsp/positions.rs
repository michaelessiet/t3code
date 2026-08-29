//! LSP wire-position math: the sidecar speaks LSP positions (0-based line,
//! 0-based UTF-16 code-unit column — `packages/contracts/src/lsp.ts`
//! `LspPosition`), while the editor addresses its `ropey::Rope` by byte
//! offset. Conversions port the Electron clamping semantics from
//! `apps/web/src/components/files/codemirror/lspPositions.ts`:
//!
//! - position → offset: a line past EOF clamps to the LAST line; a character
//!   past the line end clamps to the end of that line (never spilling into
//!   the next); a character landing inside a surrogate pair floors to the
//!   char boundary at or before it (ropey's `utf16_to_byte_idx` mid-char
//!   behavior) — never a panic, never a split char.
//! - offset → position: the offset clamps to `[0, len]` and floors to a char
//!   boundary.
//! - range → offsets: `end = max(start, end)`, so ranges never invert.
//!
//! "End of line" means before the trailing line break, matching CodeMirror's
//! `line.to`: the `\n` is excluded, and so is a `\r` directly preceding it
//! (CRLF). A bare `\r` not followed by `\n` is ordinary content, consistent
//! with `LineType::LF` where a lone `\r` is not a line break. (This
//! deliberately differs from the vendored `RopeExt::slice_line`, which keeps
//! the `\r` of a CRLF pair in the line slice; the CM6 reference never faces
//! the question because CodeMirror normalizes line breaks on load, while our
//! rope holds raw file bytes.)

use std::ops::Range;

use ropey::{LineType, Rope};

/// 0-based line + UTF-16 code-unit column, the sidecar's wire encoding
/// (`packages/contracts/src/lsp.ts` `LspPosition`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WirePosition {
    pub line: u32,
    pub character: u32,
}

/// Wire position → byte offset, clamped (`lspPositionToOffset`): line past
/// EOF → last line, character past line end → line end (before the trailing
/// `\n`/`\r\n`), character inside a surrogate pair → start of that char.
pub fn wire_to_offset(text: &Rope, position: WirePosition) -> usize {
    let last_line = text.len_lines(LineType::LF) - 1;
    let line = (position.line as usize).min(last_line);
    let line_start = text.line_to_byte_idx(line, LineType::LF);
    let line_end = line_content_end(text, line, line_start);

    let start_utf16 = text.byte_to_utf16_idx(line_start);
    let end_utf16 = text.byte_to_utf16_idx(line_end);
    let target_utf16 = start_utf16
        .saturating_add(position.character as usize)
        .min(end_utf16);
    // A target between a surrogate pair's two units floors to the char start.
    text.utf16_to_byte_idx(target_utf16)
}

/// Byte offset → wire position (`offsetToLspPosition`). Offsets past the end
/// clamp to `len`; offsets inside a multi-byte char floor to its start.
pub fn offset_to_wire(text: &Rope, offset: usize) -> WirePosition {
    let offset = text.floor_char_boundary(offset.min(text.len()));
    let line = text.byte_to_line_idx(offset, LineType::LF);
    let line_start = text.line_to_byte_idx(line, LineType::LF);
    let character = text.byte_to_utf16_idx(offset) - text.byte_to_utf16_idx(line_start);
    WirePosition {
        line: saturate_u32(line),
        character: saturate_u32(character),
    }
}

/// Wire range → byte range, clamped and ordered (`lspRangeToOffsets`): the
/// end is pulled up to the start, so the result is never inverted.
pub fn wire_range_to_offsets(text: &Rope, start: WirePosition, end: WirePosition) -> Range<usize> {
    let start_offset = wire_to_offset(text, start);
    let end_offset = wire_to_offset(text, end).max(start_offset);
    start_offset..end_offset
}

/// One past the last content byte of `line`: before the trailing `\n` and any
/// `\r` directly preceding it — CodeMirror's `line.to` (see module docs).
fn line_content_end(text: &Rope, line: usize, line_start: usize) -> usize {
    // `line + 1` may be one-past-the-end, which yields `text.len()`.
    let mut end = text.line_to_byte_idx(line + 1, LineType::LF);
    // `\n`/`\r` are ASCII, so a raw byte match cannot hit the interior of a
    // multi-byte char; the `line_start` guards keep the previous line's break
    // (an empty final line has `end == line_start`) out of reach.
    if end > line_start && text.byte(end - 1) == b'\n' {
        end -= 1;
        if end > line_start && text.byte(end - 1) == b'\r' {
            end -= 1;
        }
    }
    end
}

fn saturate_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rope(text: &str) -> Rope {
        Rope::from(text)
    }

    fn pos(line: u32, character: u32) -> WirePosition {
        WirePosition { line, character }
    }

    #[test]
    fn ascii_round_trip() {
        let text = rope("hello\nworld");
        assert_eq!(wire_to_offset(&text, pos(0, 0)), 0);
        assert_eq!(wire_to_offset(&text, pos(0, 5)), 5);
        assert_eq!(wire_to_offset(&text, pos(1, 2)), 8);
        assert_eq!(offset_to_wire(&text, 0), pos(0, 0));
        assert_eq!(offset_to_wire(&text, 5), pos(0, 5));
        assert_eq!(offset_to_wire(&text, 8), pos(1, 2));
        assert_eq!(offset_to_wire(&text, text.len()), pos(1, 5));
    }

    #[test]
    fn cjk_columns_are_utf16_units() {
        // 中 is 1 UTF-16 unit but 3 UTF-8 bytes.
        let text = rope("a中b\nx");
        assert_eq!(wire_to_offset(&text, pos(0, 1)), 1); // before 中
        assert_eq!(wire_to_offset(&text, pos(0, 2)), 4); // after 中
        assert_eq!(offset_to_wire(&text, 1), pos(0, 1));
        assert_eq!(offset_to_wire(&text, 4), pos(0, 2));
    }

    #[test]
    fn emoji_columns_are_utf16_units() {
        // 🚀 is 2 UTF-16 units and 4 UTF-8 bytes (offsets 1..5 here).
        let text = rope("a🚀b");
        assert_eq!(wire_to_offset(&text, pos(0, 1)), 1); // before 🚀
        assert_eq!(wire_to_offset(&text, pos(0, 3)), 5); // after 🚀
        assert_eq!(wire_to_offset(&text, pos(0, 4)), 6); // after b
        assert_eq!(offset_to_wire(&text, 1), pos(0, 1));
        assert_eq!(offset_to_wire(&text, 5), pos(0, 3));
        assert_eq!(offset_to_wire(&text, 6), pos(0, 4));
    }

    #[test]
    fn character_inside_surrogate_pair_floors_to_char_start() {
        let text = rope("a🚀b");
        // Column 2 lands between 🚀's two UTF-16 code units: floor to the
        // char boundary before it, never split the char.
        assert_eq!(wire_to_offset(&text, pos(0, 2)), 1);
    }

    #[test]
    fn offset_inside_multibyte_char_floors_to_char_start() {
        let text = rope("a🚀b");
        assert_eq!(offset_to_wire(&text, 3), pos(0, 1));
        let text = rope("中");
        assert_eq!(offset_to_wire(&text, 1), pos(0, 0));
    }

    #[test]
    fn character_past_line_end_clamps_to_line_end_not_next_line() {
        let text = rope("ab\ncd");
        // lspPositions.ts: min(line.from + character, line.to).
        assert_eq!(wire_to_offset(&text, pos(0, 99)), 2);
        assert_eq!(offset_to_wire(&text, 2), pos(0, 2));
    }

    #[test]
    fn line_past_eof_clamps_to_last_line() {
        let text = rope("ab\ncd");
        // lspPositions.ts: min(position.line + 1, doc.lines).
        assert_eq!(wire_to_offset(&text, pos(9, 1)), 4);
        assert_eq!(wire_to_offset(&text, pos(9, 99)), 5);
        // With a trailing newline the clamp target is the empty final line.
        let text = rope("ab\n");
        assert_eq!(wire_to_offset(&text, pos(9, 99)), 3);
    }

    #[test]
    fn offset_past_rope_end_clamps() {
        let text = rope("ab\ncd");
        assert_eq!(offset_to_wire(&text, 999), pos(1, 2));
        let text = rope("ab\n");
        assert_eq!(offset_to_wire(&text, 999), pos(1, 0));
    }

    #[test]
    fn final_line_without_trailing_newline() {
        let text = rope("ab\ncd");
        assert_eq!(wire_to_offset(&text, pos(1, 2)), 5);
        assert_eq!(wire_to_offset(&text, pos(1, 99)), 5);
        assert_eq!(offset_to_wire(&text, 5), pos(1, 2));
    }

    #[test]
    fn crlf_line_end_excludes_carriage_return() {
        let text = rope("ab\r\ncd\r\n");
        // Clamping stops before the "\r\n", not between "\r" and "\n".
        assert_eq!(wire_to_offset(&text, pos(0, 99)), 2);
        assert_eq!(wire_to_offset(&text, pos(1, 99)), 6);
        // In-bounds columns are unaffected.
        assert_eq!(wire_to_offset(&text, pos(1, 1)), 5);
        assert_eq!(offset_to_wire(&text, 5), pos(1, 1));
        // A bare "\r" with no "\n" is content, not a break (LineType::LF).
        let text = rope("ab\r");
        assert_eq!(wire_to_offset(&text, pos(0, 99)), 3);
    }

    #[test]
    fn offset_inside_crlf_maps_back_before_the_pair() {
        let text = rope("ab\r\ncd");
        // The boundary between "\r" and "\n" is a valid byte offset but sits
        // inside the line break: its wire position reads character 3, and
        // converting back clamps to the content end before the "\r".
        assert_eq!(offset_to_wire(&text, 3), pos(0, 3));
        assert_eq!(wire_to_offset(&text, pos(0, 3)), 2);
    }

    #[test]
    fn inverted_range_normalizes_to_empty_at_start() {
        let text = rope("hello\nworld");
        // lspPositions.ts: to = max(from, to).
        assert_eq!(wire_range_to_offsets(&text, pos(1, 3), pos(0, 1)), 9..9);
        assert_eq!(wire_range_to_offsets(&text, pos(0, 1), pos(1, 3)), 1..9);
    }

    #[test]
    fn empty_rope() {
        let text = rope("");
        assert_eq!(wire_to_offset(&text, pos(0, 0)), 0);
        assert_eq!(wire_to_offset(&text, pos(5, 7)), 0);
        assert_eq!(offset_to_wire(&text, 0), pos(0, 0));
        assert_eq!(offset_to_wire(&text, 42), pos(0, 0));
        assert_eq!(wire_range_to_offsets(&text, pos(3, 1), pos(0, 9)), 0..0);
    }

    #[test]
    fn mixed_content_round_trips_at_every_char_boundary() {
        // LF-only mixed content: ASCII, CJK, emoji, an empty line, and a
        // final line without a trailing newline. (CRLF boundaries are checked
        // separately: the offset between "\r" and "\n" is mid-break and is
        // deliberately not identity — see offset_inside_crlf_maps_back_*.)
        let source = "let a = 1;\n中文 comment 🚀🎉\n\nfin 中";
        let text = rope(source);
        for (offset, _) in source.char_indices() {
            let wire = offset_to_wire(&text, offset);
            assert_eq!(
                wire_to_offset(&text, wire),
                offset,
                "offset {offset} → {wire:?}"
            );
        }
        let wire = offset_to_wire(&text, source.len());
        assert_eq!(wire_to_offset(&text, wire), source.len());
    }
}
