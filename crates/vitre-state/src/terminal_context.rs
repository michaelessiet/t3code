//! Terminal-context serialization: term-for-term port of
//! `apps/web/src/lib/terminalContext.ts`, plus the timeline-side helpers from
//! `apps/web/src/components/chat/userMessageTerminalContexts.ts` and the
//! trailing `<element_context>` extraction from `lib/elementContext.ts` that
//! `deriveDisplayedUserMessageState` depends on.
//!
//! A terminal selection ("Add to chat") becomes a pending context on the
//! composer; on send the contexts serialize into a trailing
//! `<terminal_context>` block (`- {label}:` headers with `  {line} | {text}`
//! bodies), and inline U+FFFC placeholders in the prompt materialize into
//! `@{label}:{range}` labels. The timeline strips the trailing block back out
//! of sent messages and renders the parsed entries as chips.
//!
//! Divergence notes: the placeholder cursor helpers use *character* indices
//! where the TS operates on UTF-16 code units — identical for BMP text, and
//! Vitre never feeds them non-ASCII cursors (its composer has no inline-chip
//! placeholders; blocks are appended without inline labels).

/// The object-replacement character the Electron prompt editor uses to mark
/// inline terminal-context chips.
pub const INLINE_TERMINAL_CONTEXT_PLACEHOLDER: char = '\u{FFFC}';

/// `TerminalContextSelection` — what "Add to chat" captures from a terminal.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TerminalContextSelection {
    pub terminal_id: String,
    pub terminal_label: String,
    /// 1-based first buffer line of the selection.
    pub line_start: u32,
    pub line_end: u32,
    pub text: String,
}

/// One `- header:` entry parsed back out of a trailing context block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedContextEntry {
    pub header: String,
    pub body: String,
}

/// `ExtractedTerminalContexts` (contextCount = `contexts.len()`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedTerminalContexts {
    pub prompt_text: String,
    pub preview_title: Option<String>,
    pub contexts: Vec<ParsedContextEntry>,
}

/// `DisplayedUserMessageState` minus `copyText` (that is the input string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayedUserMessageState {
    pub visible_text: String,
    pub preview_title: Option<String>,
    pub contexts: Vec<ParsedContextEntry>,
    pub element_contexts: Vec<ParsedContextEntry>,
}

/// `normalizeTerminalContextText`: CRLF → LF, strip leading/trailing newlines
/// (only `\n`, matching the `/^\n+|\n+$/g` regex).
pub fn normalize_terminal_context_text(text: &str) -> String {
    let unified = text.replace("\r\n", "\n");
    unified
        .trim_start_matches('\n')
        .trim_end_matches('\n')
        .to_string()
}

pub fn has_terminal_context_text(text: &str) -> bool {
    !normalize_terminal_context_text(text).is_empty()
}

/// `isTerminalContextExpired`: a context whose snapshot text is empty.
pub fn is_terminal_context_expired(text: &str) -> bool {
    !has_terminal_context_text(text)
}

/// `previewTerminalContextText`: first three lines (plus a literal `...` line
/// when truncated), capped at 180 characters with a `...` suffix.
fn preview_terminal_context_text(text: &str) -> String {
    let normalized = normalize_terminal_context_text(text);
    if normalized.is_empty() {
        return String::new();
    }
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut visible: Vec<&str> = lines.iter().take(3).copied().collect();
    if lines.len() > 3 {
        visible.push("...");
    }
    let preview = visible.join("\n");
    if preview.chars().count() > 180 {
        let truncated: String = preview.chars().take(177).collect();
        format!("{truncated}...")
    } else {
        preview
    }
}

/// `normalizeTerminalContextSelection`: trims ids/labels, normalizes text,
/// clamps lines to ≥ 1 with `end ≥ start`; `None` when anything is empty.
pub fn normalize_terminal_context_selection(
    selection: &TerminalContextSelection,
) -> Option<TerminalContextSelection> {
    let text = normalize_terminal_context_text(&selection.text);
    let terminal_id = selection.terminal_id.trim().to_string();
    let terminal_label = selection.terminal_label.trim().to_string();
    if text.is_empty() || terminal_id.is_empty() || terminal_label.is_empty() {
        return None;
    }
    let line_start = selection.line_start.max(1);
    let line_end = selection.line_end.max(line_start);
    Some(TerminalContextSelection {
        terminal_id,
        terminal_label,
        line_start,
        line_end,
        text,
    })
}

/// `formatTerminalContextRange`: `line 9` / `lines 12-13`.
pub fn format_terminal_context_range(line_start: u32, line_end: u32) -> String {
    if line_start == line_end {
        format!("line {line_start}")
    } else {
        format!("lines {line_start}-{line_end}")
    }
}

/// `formatTerminalContextLabel`: `Terminal 1 lines 12-13`.
pub fn format_terminal_context_label(selection: &TerminalContextSelection) -> String {
    format!(
        "{} {}",
        selection.terminal_label,
        format_terminal_context_range(selection.line_start, selection.line_end)
    )
}

/// Collapse whitespace runs to `-` after lowercasing (the label half of
/// `formatInlineTerminalContextLabel`).
fn hyphenate_label(label: &str) -> String {
    let lowered = label.to_lowercase();
    let mut result = String::with_capacity(lowered.len());
    let mut in_whitespace = false;
    for ch in lowered.chars() {
        if ch.is_whitespace() {
            if !in_whitespace {
                result.push('-');
                in_whitespace = true;
            }
        } else {
            result.push(ch);
            in_whitespace = false;
        }
    }
    result
}

/// `formatInlineTerminalContextLabel` (the selection overload):
/// `@terminal-1:12-13`.
pub fn format_inline_terminal_context_selection_label(
    terminal_label: &str,
    line_start: u32,
    line_end: u32,
) -> String {
    let label = hyphenate_label(terminal_label.trim());
    let range = if line_start == line_end {
        format!("{line_start}")
    } else {
        format!("{line_start}-{line_end}")
    };
    format!("@{label}:{range}")
}

/// `buildTerminalContextPreviewTitle`.
pub fn build_terminal_context_preview_title(
    contexts: &[TerminalContextSelection],
) -> Option<String> {
    if contexts.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for context in contexts {
        let Some(normalized) = normalize_terminal_context_selection(context) else {
            continue;
        };
        let preview = preview_terminal_context_text(&normalized.text);
        let label = format_terminal_context_label(&normalized);
        parts.push(if preview.is_empty() {
            label
        } else {
            format!("{label}\n{preview}")
        });
    }
    let previews = parts.join("\n\n");
    if previews.is_empty() {
        None
    } else {
        Some(previews)
    }
}

fn build_terminal_context_body_lines(selection: &TerminalContextSelection) -> Vec<String> {
    normalize_terminal_context_text(&selection.text)
        .split('\n')
        .enumerate()
        .map(|(index, line)| format!("  {} | {}", selection.line_start + index as u32, line))
        .collect()
}

/// `buildTerminalContextBlock`.
pub fn build_terminal_context_block(contexts: &[TerminalContextSelection]) -> String {
    let normalized: Vec<TerminalContextSelection> = contexts
        .iter()
        .filter_map(normalize_terminal_context_selection)
        .collect();
    if normalized.is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = Vec::new();
    for (index, context) in normalized.iter().enumerate() {
        lines.push(format!("- {}:", format_terminal_context_label(context)));
        lines.extend(build_terminal_context_body_lines(context));
        if index < normalized.len() - 1 {
            lines.push(String::new());
        }
    }
    let mut block = String::from("<terminal_context>\n");
    block.push_str(&lines.join("\n"));
    block.push_str("\n</terminal_context>");
    block
}

/// `materializeInlineTerminalContextPrompt`: each U+FFFC becomes the next
/// context's inline label (dropped when there is no matching context).
pub fn materialize_inline_terminal_context_prompt(
    prompt: &str,
    contexts: &[TerminalContextSelection],
) -> String {
    let mut next_context_index = 0usize;
    let mut result = String::with_capacity(prompt.len());
    for ch in prompt.chars() {
        if ch != INLINE_TERMINAL_CONTEXT_PLACEHOLDER {
            result.push(ch);
            continue;
        }
        let context = contexts.get(next_context_index);
        next_context_index += 1;
        if let Some(context) = context {
            result.push_str(&format_inline_terminal_context_selection_label(
                &context.terminal_label,
                context.line_start,
                context.line_end,
            ));
        }
    }
    result
}

/// `appendTerminalContextsToPrompt`.
pub fn append_terminal_contexts_to_prompt(
    prompt: &str,
    contexts: &[TerminalContextSelection],
) -> String {
    let trimmed_prompt = materialize_inline_terminal_context_prompt(prompt, contexts)
        .trim()
        .to_string();
    let context_block = build_terminal_context_block(contexts);
    if context_block.is_empty() {
        return trimmed_prompt;
    }
    if trimmed_prompt.is_empty() {
        context_block
    } else {
        format!("{trimmed_prompt}\n\n{context_block}")
    }
}

/// Matches the trailing-block regex
/// `/\n*<{tag}>\n([\s\S]*?)\n<\/{tag}>\s*$/`: returns
/// `(prompt_text, block_body)` — leftmost opener whose nearest viable closer
/// leaves only whitespace to the end; `prompt_text` drops the newline run
/// before the opener.
fn extract_trailing_block(
    prompt: &str,
    open_tag: &str,
    close_tag: &str,
) -> Option<(String, String)> {
    let open_marker = format!("{open_tag}\n");
    let close_marker = format!("\n{close_tag}");
    let mut search_from = 0usize;
    while let Some(relative) = prompt[search_from..].find(&open_marker) {
        let open_at = search_from + relative;
        let body_start = open_at + open_marker.len();
        // Lazy body: the nearest closer whose remainder is all whitespace.
        let mut closer_from = body_start;
        while let Some(close_relative) = prompt[closer_from..].find(&close_marker) {
            let close_at = closer_from + close_relative;
            let after = close_at + close_marker.len();
            if prompt[after..].chars().all(char::is_whitespace) {
                // `match.index` swallows the `\n*` run before the opener and
                // the caller strips `/\n+$/` again — net effect: everything
                // before the opener minus trailing newlines.
                let prompt_text = prompt[..open_at].trim_end_matches('\n').to_string();
                let body = prompt[body_start..close_at].to_string();
                return Some((prompt_text, body));
            }
            closer_from = close_at + 1;
        }
        search_from = open_at + 1;
    }
    None
}

/// `parseTerminalContextEntries` / `parseElementContextEntries` (identical
/// line grammars): `- {header}:` starts an entry, two-space-indented lines are
/// body, blank lines inside an entry keep a blank body line.
fn parse_context_entries(block: &str) -> Vec<ParsedContextEntry> {
    struct Current {
        header: String,
        body_lines: Vec<String>,
    }
    let mut entries: Vec<ParsedContextEntry> = Vec::new();
    let mut current: Option<Current> = None;
    let commit = |current: &mut Option<Current>, entries: &mut Vec<ParsedContextEntry>| {
        if let Some(entry) = current.take() {
            entries.push(ParsedContextEntry {
                header: entry.header,
                body: entry.body_lines.join("\n").trim_end().to_string(),
            });
        }
    };
    for raw_line in block.split('\n') {
        // `/^- (.+):$/`
        if let Some(rest) = raw_line.strip_prefix("- ")
            && let Some(header) = rest.strip_suffix(':')
            && !header.is_empty()
        {
            commit(&mut current, &mut entries);
            current = Some(Current {
                header: header.to_string(),
                body_lines: Vec::new(),
            });
            continue;
        }
        let Some(entry) = current.as_mut() else {
            continue;
        };
        if let Some(body) = raw_line.strip_prefix("  ") {
            entry.body_lines.push(body.to_string());
        } else if raw_line.is_empty() {
            entry.body_lines.push(String::new());
        }
    }
    commit(&mut current, &mut entries);
    entries
}

fn preview_title_for_entries(entries: &[ParsedContextEntry]) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    Some(
        entries
            .iter()
            .map(|entry| {
                if entry.body.is_empty() {
                    entry.header.clone()
                } else {
                    format!("{}\n{}", entry.header, entry.body)
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}

/// `extractTrailingTerminalContexts`.
pub fn extract_trailing_terminal_contexts(prompt: &str) -> ExtractedTerminalContexts {
    let Some((prompt_text, body)) =
        extract_trailing_block(prompt, "<terminal_context>", "</terminal_context>")
    else {
        return ExtractedTerminalContexts {
            prompt_text: prompt.to_string(),
            preview_title: None,
            contexts: Vec::new(),
        };
    };
    let contexts = parse_context_entries(&body);
    ExtractedTerminalContexts {
        prompt_text,
        preview_title: preview_title_for_entries(&contexts),
        contexts,
    }
}

/// `extractTrailingElementContexts` (`lib/elementContext.ts`) — same block
/// grammar under the `<element_context>` tag.
pub fn extract_trailing_element_contexts(prompt: &str) -> (String, Vec<ParsedContextEntry>) {
    match extract_trailing_block(prompt, "<element_context>", "</element_context>") {
        Some((prompt_text, body)) => (prompt_text, parse_context_entries(&body)),
        None => (prompt.to_string(), Vec::new()),
    }
}

/// `deriveDisplayedUserMessageState`: element block stripped first (send-time
/// appends terminal first, element last), then the terminal block.
pub fn derive_displayed_user_message_state(prompt: &str) -> DisplayedUserMessageState {
    let (after_element, element_contexts) = extract_trailing_element_contexts(prompt);
    let extracted = extract_trailing_terminal_contexts(&after_element);
    DisplayedUserMessageState {
        visible_text: extracted.prompt_text,
        preview_title: extracted.preview_title,
        contexts: extracted.contexts,
        element_contexts,
    }
}

/// `countInlineTerminalContextPlaceholders`.
pub fn count_inline_terminal_context_placeholders(prompt: &str) -> usize {
    prompt
        .chars()
        .filter(|ch| *ch == INLINE_TERMINAL_CONTEXT_PLACEHOLDER)
        .count()
}

/// `ensureInlineTerminalContextPlaceholders`: prepend the missing count.
pub fn ensure_inline_terminal_context_placeholders(
    prompt: &str,
    terminal_context_count: usize,
) -> String {
    let missing =
        terminal_context_count.saturating_sub(count_inline_terminal_context_placeholders(prompt));
    if missing == 0 {
        return prompt.to_string();
    }
    let mut result = String::with_capacity(prompt.len() + missing * 3);
    for _ in 0..missing {
        result.push(INLINE_TERMINAL_CONTEXT_PLACEHOLDER);
    }
    result.push_str(prompt);
    result
}

fn is_inline_boundary_whitespace(ch: Option<char>) -> bool {
    matches!(ch, None | Some(' ') | Some('\n') | Some('\t') | Some('\r'))
}

/// `insertInlineTerminalContextPlaceholder` result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertedInlinePlaceholder {
    pub prompt: String,
    /// Character index after the inserted replacement.
    pub cursor: usize,
    /// How many placeholders precede the insertion point.
    pub context_index: usize,
}

/// `insertInlineTerminalContextPlaceholder` (character-index cursor).
pub fn insert_inline_terminal_context_placeholder(
    prompt: &str,
    cursor_input: usize,
) -> InsertedInlinePlaceholder {
    let chars: Vec<char> = prompt.chars().collect();
    let cursor = cursor_input.min(chars.len());
    let needs_leading_space =
        !is_inline_boundary_whitespace(cursor.checked_sub(1).and_then(|i| chars.get(i)).copied());
    let mut replacement = String::new();
    if needs_leading_space {
        replacement.push(' ');
    }
    replacement.push(INLINE_TERMINAL_CONTEXT_PLACEHOLDER);
    replacement.push(' ');
    let range_end = if chars.get(cursor) == Some(&' ') {
        cursor + 1
    } else {
        cursor
    };
    let prefix: String = chars[..cursor].iter().collect();
    let suffix: String = chars[range_end..].iter().collect();
    let context_index = prefix
        .chars()
        .filter(|ch| *ch == INLINE_TERMINAL_CONTEXT_PLACEHOLDER)
        .count();
    InsertedInlinePlaceholder {
        prompt: format!("{prefix}{replacement}{suffix}"),
        cursor: cursor + replacement.chars().count(),
        context_index,
    }
}

/// `stripInlineTerminalContextPlaceholders`.
pub fn strip_inline_terminal_context_placeholders(prompt: &str) -> String {
    prompt
        .chars()
        .filter(|ch| *ch != INLINE_TERMINAL_CONTEXT_PLACEHOLDER)
        .collect()
}

/// `removeInlineTerminalContextPlaceholder`: drop the nth placeholder,
/// returning the new prompt and the character index where it sat.
pub fn remove_inline_terminal_context_placeholder(
    prompt: &str,
    context_index: usize,
) -> (String, usize) {
    let chars: Vec<char> = prompt.chars().collect();
    let mut placeholder_index = 0usize;
    for (index, ch) in chars.iter().enumerate() {
        if *ch != INLINE_TERMINAL_CONTEXT_PLACEHOLDER {
            continue;
        }
        if placeholder_index == context_index {
            let mut next: String = chars[..index].iter().collect();
            next.extend(chars[index + 1..].iter());
            return (next, index);
        }
        placeholder_index += 1;
    }
    (prompt.to_string(), chars.len())
}

/// `userMessageTerminalContexts.formatInlineTerminalContextLabel` — derive an
/// inline label back from a parsed entry header.
/// Pattern: `/^(.*?)\s+line(?:s)?\s+(\d+)(?:-(\d+))?$/i` (lazy prefix =
/// earliest whitespace split whose suffix parses).
pub fn format_inline_terminal_context_label_from_header(header: &str) -> String {
    let trimmed = header.trim();
    if let Some((label, line_start, line_end)) = match_terminal_context_header(trimmed) {
        let label = if label.trim().is_empty() {
            "terminal"
        } else {
            label.trim()
        };
        return format_inline_terminal_context_selection_label(label, line_start, line_end);
    }
    format!("@{}", hyphenate_label(trimmed))
}

fn match_terminal_context_header(header: &str) -> Option<(&str, u32, u32)> {
    // Try every whitespace run as the `\s+` before "line"/"lines" — leftmost
    // (shortest prefix) first, like the lazy `(.*?)`.
    let bytes = header.char_indices().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < bytes.len() {
        let (byte_at, ch) = bytes[index];
        if !ch.is_whitespace() {
            index += 1;
            continue;
        }
        // Extend the whitespace run.
        let mut end = index;
        while end < bytes.len() && bytes[end].1.is_whitespace() {
            end += 1;
        }
        let suffix_start = if end < bytes.len() {
            bytes[end].0
        } else {
            header.len()
        };
        if let Some((start, finish)) = match_line_range_suffix(&header[suffix_start..]) {
            return Some((&header[..byte_at], start, finish));
        }
        index = end;
    }
    None
}

/// `line(?:s)?\s+(\d+)(?:-(\d+))?$`, case-insensitive on the keyword.
fn match_line_range_suffix(suffix: &str) -> Option<(u32, u32)> {
    let lowered = suffix.to_lowercase();
    let rest = lowered.strip_prefix("line")?;
    let rest = rest.strip_prefix('s').unwrap_or(rest);
    // `\s+`
    let trimmed = rest.trim_start();
    if trimmed.len() == rest.len() {
        return None;
    }
    let digits_end = trimmed
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(trimmed.len());
    if digits_end == 0 {
        return None;
    }
    let start: u32 = trimmed[..digits_end].parse().ok()?;
    let after = &trimmed[digits_end..];
    if after.is_empty() {
        return Some((start, start));
    }
    let after_dash = after.strip_prefix('-')?;
    if after_dash.is_empty() || !after_dash.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let finish: u32 = after_dash.parse().ok()?;
    Some((start, finish))
}

/// `buildInlineTerminalContextText`: fallback prefix labels joined by spaces.
pub fn build_inline_terminal_context_text(entries: &[ParsedContextEntry]) -> String {
    entries
        .iter()
        .filter_map(|entry| {
            let header = entry.header.trim();
            if header.is_empty() {
                None
            } else {
                Some(format_inline_terminal_context_label_from_header(header))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `textContainsInlineTerminalContextLabels`: every entry's label appears in
/// order.
pub fn text_contains_inline_terminal_context_labels(
    text: &str,
    entries: &[ParsedContextEntry],
) -> bool {
    let mut search_start = 0usize;
    for entry in entries {
        let label = format_inline_terminal_context_label_from_header(&entry.header);
        match text[search_start..].find(&label) {
            Some(relative) => {
                search_start = search_start + relative + label.len();
            }
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context() -> TerminalContextSelection {
        TerminalContextSelection {
            terminal_id: "default".into(),
            terminal_label: "Terminal 1".into(),
            line_start: 12,
            line_end: 13,
            text: "git status\nOn branch main".into(),
        }
    }

    #[test]
    fn formats_terminal_labels_with_line_ranges() {
        assert_eq!(
            format_terminal_context_label(&make_context()),
            "Terminal 1 lines 12-13"
        );
        let mut single = make_context();
        single.line_start = 9;
        single.line_end = 9;
        assert_eq!(format_terminal_context_label(&single), "Terminal 1 line 9");
    }

    #[test]
    fn builds_a_numbered_terminal_context_block() {
        assert_eq!(
            build_terminal_context_block(&[make_context()]),
            [
                "<terminal_context>",
                "- Terminal 1 lines 12-13:",
                "  12 | git status",
                "  13 | On branch main",
                "</terminal_context>",
            ]
            .join("\n")
        );
    }

    #[test]
    fn appends_terminal_context_blocks_after_prompt_text() {
        assert_eq!(
            append_terminal_contexts_to_prompt("Investigate this", &[make_context()]),
            [
                "Investigate this",
                "",
                "<terminal_context>",
                "- Terminal 1 lines 12-13:",
                "  12 | git status",
                "  13 | On branch main",
                "</terminal_context>",
            ]
            .join("\n")
        );
    }

    #[test]
    fn replaces_inline_placeholders_with_inline_labels_before_appending() {
        let prompt = format!("Investigate {INLINE_TERMINAL_CONTEXT_PLACEHOLDER} carefully");
        assert_eq!(
            append_terminal_contexts_to_prompt(&prompt, &[make_context()]),
            [
                "Investigate @terminal-1:12-13 carefully",
                "",
                "<terminal_context>",
                "- Terminal 1 lines 12-13:",
                "  12 | git status",
                "  13 | On branch main",
                "</terminal_context>",
            ]
            .join("\n")
        );
    }

    #[test]
    fn extracts_terminal_context_blocks_from_message_text() {
        let prompt = append_terminal_contexts_to_prompt("Investigate this", &[make_context()]);
        let extracted = extract_trailing_terminal_contexts(&prompt);
        assert_eq!(extracted.prompt_text, "Investigate this");
        assert_eq!(
            extracted.preview_title.as_deref(),
            Some("Terminal 1 lines 12-13\n12 | git status\n13 | On branch main")
        );
        assert_eq!(
            extracted.contexts,
            vec![ParsedContextEntry {
                header: "Terminal 1 lines 12-13".into(),
                body: "12 | git status\n13 | On branch main".into(),
            }]
        );
    }

    #[test]
    fn derives_displayed_user_message_state_from_terminal_context_prompts() {
        let prompt = append_terminal_contexts_to_prompt("Investigate this", &[make_context()]);
        let state = derive_displayed_user_message_state(&prompt);
        assert_eq!(state.visible_text, "Investigate this");
        assert_eq!(state.contexts.len(), 1);
        assert_eq!(state.element_contexts.len(), 0);
        assert_eq!(
            state.preview_title.as_deref(),
            Some("Terminal 1 lines 12-13\n12 | git status\n13 | On branch main")
        );
    }

    #[test]
    fn preserves_prompt_text_when_no_trailing_block_exists() {
        let extracted = extract_trailing_terminal_contexts("No attached context");
        assert_eq!(extracted.prompt_text, "No attached context");
        assert_eq!(extracted.preview_title, None);
        assert!(extracted.contexts.is_empty());
    }

    #[test]
    fn returns_none_preview_title_when_every_context_is_invalid() {
        let mut blank_id = make_context();
        blank_id.terminal_id = "   ".into();
        let mut blank_text = make_context();
        blank_text.text = "\n\n".into();
        assert_eq!(
            build_terminal_context_preview_title(&[blank_id, blank_text]),
            None
        );
    }

    #[test]
    fn tracks_inline_placeholders_in_prompt_text() {
        let p = INLINE_TERMINAL_CONTEXT_PLACEHOLDER;
        assert_eq!(
            count_inline_terminal_context_placeholders(&format!("a{p}b{p}")),
            2
        );
        assert_eq!(
            ensure_inline_terminal_context_placeholders("Investigate this", 2),
            format!("{p}{p}Investigate this")
        );
        assert_eq!(
            insert_inline_terminal_context_placeholder("abc", 1),
            InsertedInlinePlaceholder {
                prompt: format!("a {p} bc"),
                cursor: 4,
                context_index: 0,
            }
        );
        assert_eq!(
            remove_inline_terminal_context_placeholder(&format!("a{p}b{p}c"), 1),
            (format!("a{p}bc"), 3)
        );
        assert_eq!(
            strip_inline_terminal_context_placeholders(&format!("a{p}b")),
            "ab"
        );
    }

    #[test]
    fn inserts_placeholder_after_a_file_mention_at_the_expanded_cursor() {
        let p = INLINE_TERMINAL_CONTEXT_PLACEHOLDER;
        assert_eq!(
            insert_inline_terminal_context_placeholder("Inspect @package.json ", 22),
            InsertedInlinePlaceholder {
                prompt: format!("Inspect @package.json {p} "),
                cursor: 24,
                context_index: 0,
            }
        );
    }

    #[test]
    fn adds_trailing_space_and_consumes_existing_space_at_insertion_point() {
        let p = INLINE_TERMINAL_CONTEXT_PLACEHOLDER;
        assert_eq!(
            insert_inline_terminal_context_placeholder("yo whats", 3),
            InsertedInlinePlaceholder {
                prompt: format!("yo {p} whats"),
                cursor: 5,
                context_index: 0,
            }
        );
    }

    #[test]
    fn marks_contexts_without_snapshot_text_as_expired() {
        assert!(has_terminal_context_text("git status"));
        assert!(!is_terminal_context_expired("git status"));
        assert!(!has_terminal_context_text(""));
        assert!(is_terminal_context_expired(""));
    }

    #[test]
    fn formats_and_materializes_inline_labels_from_placeholder_positions() {
        let context = make_context();
        assert_eq!(
            format_inline_terminal_context_selection_label(
                &context.terminal_label,
                context.line_start,
                context.line_end
            ),
            "@terminal-1:12-13"
        );
        let prompt = format!("Investigate {INLINE_TERMINAL_CONTEXT_PLACEHOLDER} carefully");
        assert_eq!(
            materialize_inline_terminal_context_prompt(&prompt, &[context]),
            "Investigate @terminal-1:12-13 carefully"
        );
    }

    #[test]
    fn parses_inline_labels_back_from_entry_headers() {
        assert_eq!(
            format_inline_terminal_context_label_from_header("Terminal 1 lines 12-13"),
            "@terminal-1:12-13"
        );
        assert_eq!(
            format_inline_terminal_context_label_from_header("Terminal 1 line 9"),
            "@terminal-1:9"
        );
        // No parsable range → hyphenated fallback.
        assert_eq!(
            format_inline_terminal_context_label_from_header("Build output"),
            "@build-output"
        );
        // Lazy prefix: the FIRST viable "line(s) N" split wins.
        assert_eq!(
            format_inline_terminal_context_label_from_header("web line up line 5"),
            "@web-line-up:5"
        );
    }

    #[test]
    fn detects_embedded_inline_labels_in_order() {
        let entries = vec![
            ParsedContextEntry {
                header: "Terminal 1 lines 12-13".into(),
                body: String::new(),
            },
            ParsedContextEntry {
                header: "Terminal 2 line 4".into(),
                body: String::new(),
            },
        ];
        assert!(text_contains_inline_terminal_context_labels(
            "see @terminal-1:12-13 then @terminal-2:4 now",
            &entries
        ));
        assert!(!text_contains_inline_terminal_context_labels(
            "see @terminal-2:4 then @terminal-1:12-13 now",
            &entries
        ));
        assert_eq!(
            build_inline_terminal_context_text(&entries),
            "@terminal-1:12-13 @terminal-2:4"
        );
    }

    #[test]
    fn strips_element_context_blocks_before_terminal_blocks() {
        let prompt = [
            "Fix this",
            "",
            "<terminal_context>",
            "- Terminal 1 line 3:",
            "  3 | oops",
            "</terminal_context>",
            "",
            "<element_context>",
            "- Button (#submit):",
            "  cursor: pointer",
            "</element_context>",
        ]
        .join("\n");
        let state = derive_displayed_user_message_state(&prompt);
        assert_eq!(state.visible_text, "Fix this");
        assert_eq!(state.contexts.len(), 1);
        assert_eq!(state.contexts[0].header, "Terminal 1 line 3");
        assert_eq!(state.element_contexts.len(), 1);
        assert_eq!(state.element_contexts[0].header, "Button (#submit)");
        assert_eq!(state.element_contexts[0].body, "cursor: pointer");
    }
}
