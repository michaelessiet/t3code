//! Pure matching/ranking logic behind the palette surfaces, ported verbatim
//! from the Electron app so both clients order results identically.
//!
//! Sources: `apps/web/src/components/CommandPalette.logic.ts` (normalization
//! and the two rank functions) and `apps/web/src/components/SearchPanel.logic.ts`
//! (`splitSearchResultPath`, `matchLineSegments`).

use std::cmp::Reverse;

/// `value.trim().toLowerCase().replace(/\s+/g, " ")` — the normalization both
/// the haystack and the needle pass through before any comparison.
pub fn normalize_search_text(value: &str) -> String {
    let lowered = value.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    for word in lowered.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// Exact match beats prefix beats substring; `None` = this field doesn't match
/// at all. Mirrors `rankSearchFieldMatch`, whose `-Infinity` is our `None`.
fn rank_search_field_match(field: &str, normalized_query: &str) -> Option<i32> {
    let normalized_field = normalize_search_text(field);
    if normalized_field.is_empty() || !normalized_field.contains(normalized_query) {
        return None;
    }
    Some(if normalized_field == normalized_query {
        3
    } else if normalized_field.starts_with(normalized_query) {
        2
    } else {
        1
    })
}

/// Score an item by its search terms. The *first* matching term wins and its
/// position dominates the score (`1000 - index * 100`), so an item matched on
/// its title always outranks one matched on a later term like a branch name.
/// 0 means "no term matched" — but note an item with no usable terms also
/// scores 0, exactly as the TypeScript does.
pub fn rank_item_match(search_terms: &[String], normalized_query: &str) -> i32 {
    let terms: Vec<&String> = search_terms
        .iter()
        .filter(|term| !term.is_empty())
        .collect();
    if terms.is_empty() {
        return 0;
    }
    for (index, field) in terms.iter().enumerate() {
        if let Some(field_rank) = rank_search_field_match(field, normalized_query) {
            return 1_000 - (index as i32) * 100 + field_rank;
        }
    }
    0
}

/// The cheap pre-filter `filterCommandPaletteGroups` applies before ranking:
/// the query must appear in the concatenation of every term.
pub fn item_matches(search_terms: &[String], normalized_query: &str) -> bool {
    normalize_search_text(&search_terms.join(" ")).contains(normalized_query)
}

/// Filter a list to the items whose terms contain `normalized_query`, ordered
/// best-match first with ties broken by original position — the body of
/// `filterCommandPaletteGroups`'s per-group `flatMap`. Returns indices so
/// callers keep their own item types.
///
/// An empty query keeps every item in its original order.
pub fn rank_indices(term_sets: &[Vec<String>], normalized_query: &str) -> Vec<usize> {
    if normalized_query.is_empty() {
        return (0..term_sets.len()).collect();
    }
    let mut scored: Vec<(usize, i32)> = term_sets
        .iter()
        .enumerate()
        .filter(|(_, terms)| item_matches(terms, normalized_query))
        .map(|(index, terms)| (index, rank_item_match(terms, normalized_query)))
        .collect();
    // Stable sort keeps equal ranks in source order, which is what the
    // TypeScript's `|| left.index - right.index` tie-break spells out.
    scored.sort_by_key(|(_, rank)| Reverse(*rank));
    scored.into_iter().map(|(index, _)| index).collect()
}

/// `"src/lib/utils.ts"` → `("utils.ts", "src/lib")`.
pub fn split_search_result_path(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(index) => (&path[index + 1..], &path[..index]),
        None => (path, ""),
    }
}

/// Byte offset of UTF-16 code-unit offset `target` within `text`, clamped to
/// the string's length.
///
/// The server reports `matchStart`/`matchEnd` as UTF-16 offsets — ripgrep
/// emits byte offsets and `WorkspaceContentSearch.ts` converts them for JS
/// clients (`byteOffsetToCharOffset`, "clients live in UTF-16"). Rust slices
/// by bytes, so every wire offset has to come back through here or a line
/// containing any non-ASCII character slices at the wrong column.
pub fn utf16_offset_to_byte(text: &str, target: usize) -> usize {
    if target == 0 {
        return 0;
    }
    let mut units = 0usize;
    for (byte_index, ch) in text.char_indices() {
        if units >= target {
            return byte_index;
        }
        units += ch.len_utf16();
    }
    text.len()
}

/// How many leading context characters a match line keeps before it is
/// clipped with an ellipsis (`MATCH_CONTEXT_PREFIX_MAX`).
const MATCH_CONTEXT_PREFIX_MAX: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchLineSegments {
    pub before: String,
    pub matched: String,
    pub after: String,
    /// Leading context was truncated, so the row renders a leading `…`.
    pub before_clipped: bool,
}

/// Split a content-search line into before/matched/after for highlighting.
/// `match_start`/`match_end` are UTF-16 offsets straight off the wire.
pub fn match_line_segments(
    line_text: &str,
    match_start: usize,
    match_end: usize,
) -> MatchLineSegments {
    let start = utf16_offset_to_byte(line_text, match_start);
    let end = utf16_offset_to_byte(line_text, match_end.max(match_start));

    let raw_before = line_text[..start].trim_start();
    let mut before_clipped = false;
    // The TypeScript counts UTF-16 units here; counting characters differs
    // only for astral-plane context, where either choice is a cosmetic
    // truncation point rather than a correctness boundary.
    let before = if raw_before.chars().count() > MATCH_CONTEXT_PREFIX_MAX {
        before_clipped = true;
        let skip = raw_before.chars().count() - MATCH_CONTEXT_PREFIX_MAX;
        raw_before.chars().skip(skip).collect()
    } else {
        raw_before.to_string()
    };

    MatchLineSegments {
        before,
        matched: line_text[start..end].to_string(),
        after: line_text[end..].to_string(),
        before_clipped,
    }
}

/// Case-insensitive first-occurrence split used to highlight the typed query
/// inside a name. Returns `(before, matched, after)`; `matched` is empty when
/// the query does not occur.
pub fn name_segments<'a>(text: &'a str, query: &str) -> (&'a str, &'a str, &'a str) {
    if query.is_empty() {
        return (text, "", "");
    }
    let Some(index) = text.to_lowercase().find(&query.to_lowercase()) else {
        return (text, "", "");
    };
    // `to_lowercase` can change byte lengths (e.g. `İ`), which would make the
    // index meaningless against the original string. Fall back to no
    // highlight rather than slicing at a bogus boundary.
    if !text.is_char_boundary(index) {
        return (text, "", "");
    }
    let end = index + query.len();
    if end > text.len() || !text.is_char_boundary(end) {
        return (text, "", "");
    }
    (&text[..index], &text[index..end], &text[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_whitespace_and_case() {
        assert_eq!(normalize_search_text("  Foo   BAR "), "foo bar");
        assert_eq!(normalize_search_text("\t\n"), "");
    }

    #[test]
    fn field_rank_orders_exact_prefix_substring() {
        assert_eq!(rank_search_field_match("open file", "open file"), Some(3));
        assert_eq!(rank_search_field_match("open file", "open"), Some(2));
        assert_eq!(rank_search_field_match("open file", "file"), Some(1));
        assert_eq!(rank_search_field_match("open file", "zzz"), None);
        assert_eq!(rank_search_field_match("   ", "a"), None);
    }

    #[test]
    fn earlier_terms_dominate_later_ones() {
        let title_match = rank_item_match(&["deploy".into(), "other".into()], "deploy");
        // A weaker (substring) hit on the *first* term still beats an exact
        // hit on the second — the 100-per-index penalty outweighs rank 3.
        let branch_match = rank_item_match(&["unrelated".into(), "deploy".into()], "deploy");
        assert!(
            title_match > branch_match,
            "{title_match} !> {branch_match}"
        );
        assert_eq!(title_match, 1_000 + 3);
        assert_eq!(branch_match, 900 + 3);
    }

    #[test]
    fn empty_terms_are_skipped_before_indexing() {
        // The blank term is filtered out entirely, so "deploy" is index 0.
        assert_eq!(
            rank_item_match(&["".into(), "deploy".into()], "deploy"),
            1_003
        );
    }

    #[test]
    fn no_match_scores_zero() {
        assert_eq!(rank_item_match(&["deploy".into()], "zzz"), 0);
        assert_eq!(rank_item_match(&[], "zzz"), 0);
    }

    #[test]
    fn rank_indices_orders_by_rank_then_position() {
        let terms = vec![
            vec!["settings".to_string()],  // substring hit, rank 1
            vec!["open file".to_string()], // prefix hit, rank 2
            vec!["unrelated".to_string()], // no hit, dropped
            vec!["open".to_string()],      // exact hit, rank 3
        ];
        assert_eq!(rank_indices(&terms, "open"), vec![3, 1]);
    }

    #[test]
    fn rank_indices_keeps_source_order_for_an_empty_query() {
        let terms = vec![vec!["b".to_string()], vec!["a".to_string()]];
        assert_eq!(rank_indices(&terms, ""), vec![0, 1]);
    }

    #[test]
    fn rank_indices_breaks_rank_ties_by_position() {
        let terms = vec![vec!["zzz open".to_string()], vec!["aaa open".to_string()]];
        // Both are substring hits on the first term (rank 1_001), so the
        // earlier item wins rather than anything alphabetical.
        assert_eq!(rank_indices(&terms, "open"), vec![0, 1]);
    }

    #[test]
    fn splits_path_into_name_and_directory() {
        assert_eq!(
            split_search_result_path("src/lib/utils.ts"),
            ("utils.ts", "src/lib")
        );
        assert_eq!(split_search_result_path("README.md"), ("README.md", ""));
    }

    #[test]
    fn utf16_offsets_map_past_astral_characters() {
        // "🎉" is 4 bytes / 2 UTF-16 units; "é" is 2 bytes / 1 unit.
        let text = "🎉é ok";
        assert_eq!(utf16_offset_to_byte(text, 0), 0);
        assert_eq!(utf16_offset_to_byte(text, 2), 4);
        assert_eq!(utf16_offset_to_byte(text, 3), 6);
        assert_eq!(utf16_offset_to_byte(text, 999), text.len());
    }

    #[test]
    fn match_segments_slice_on_utf16_offsets() {
        // The server would report the match on "ok" as UTF-16 [4, 6).
        let segments = match_line_segments("🎉é ok", 4, 6);
        assert_eq!(segments.matched, "ok");
        // `trimStart` only strips *leading* whitespace, so the separating
        // space before the match survives.
        assert_eq!(segments.before, "🎉é ");
        assert_eq!(segments.after, "");
        assert!(!segments.before_clipped);
    }

    #[test]
    fn match_segments_clip_long_leading_context() {
        let line = format!("{}NEEDLE tail", "x".repeat(50));
        let segments = match_line_segments(&line, 50, 56);
        assert_eq!(segments.matched, "NEEDLE");
        assert_eq!(segments.before.chars().count(), MATCH_CONTEXT_PREFIX_MAX);
        assert!(segments.before_clipped);
        assert_eq!(segments.after, " tail");
    }

    #[test]
    fn match_segments_trim_leading_indentation() {
        // "    let x = 1;" — the `1` sits at UTF-16 offset 12.
        let segments = match_line_segments("    let x = 1;", 12, 13);
        assert_eq!(segments.before, "let x = ");
        assert_eq!(segments.matched, "1");
        assert_eq!(segments.after, ";");
        assert!(!segments.before_clipped);
    }

    #[test]
    fn name_segments_highlight_first_case_insensitive_hit() {
        assert_eq!(
            name_segments("ChatView.tsx", "chat"),
            ("", "Chat", "View.tsx")
        );
        assert_eq!(
            name_segments("ChatView.tsx", "zzz"),
            ("ChatView.tsx", "", "")
        );
        assert_eq!(name_segments("ChatView.tsx", ""), ("ChatView.tsx", "", ""));
    }

    #[test]
    fn name_segments_survive_multibyte_text() {
        let (before, matched, after) = name_segments("héllo world", "world");
        assert_eq!((before, matched, after), ("héllo ", "world", ""));
    }
}
