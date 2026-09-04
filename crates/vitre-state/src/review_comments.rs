//! Review-comment context blocks for the composer and timeline.
//!
//! Ports `apps/web/src/reviewCommentContext.ts` term-for-term: pending review
//! comments serialize into `<review_comment …>` blocks appended to the outgoing
//! prompt, and sent user messages parse back into text / review-comment
//! segments for timeline rendering. The Electron module drives its parsers
//! with three regexes; this port hand-rolls scanners with the same
//! leftmost-first + backtracking semantics (the fence pattern needs a
//! backreference, which the `regex` crate does not support).

use serde::{Deserialize, Serialize};

use crate::diff_patch::{PatchFile, PatchLineKind};

/// `ReviewCommentContext` — one pending or persisted review comment. Field
/// names keep Electron's persisted camelCase wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCommentContext {
    pub id: String,
    pub section_id: String,
    pub section_title: String,
    pub file_path: String,
    /// Index into the flattened diff-review rows (or 0-based file line).
    pub start_index: usize,
    pub end_index: usize,
    pub range_label: String,
    pub text: String,
    pub diff: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fence_language: Option<String>,
}

/// `ReviewCommentMessageSegment` — a sent user message split around its
/// review-comment blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewCommentMessageSegment {
    Text { id: String, text: String },
    ReviewComment { comment: ReviewCommentContext },
}

// ---------------------------------------------------------------------------
// Scanners — hand-rolled ports of the three Electron regexes.
// ---------------------------------------------------------------------------

const REVIEW_COMMENT_OPEN_TAG: &str = "<review_comment";
const REVIEW_COMMENT_CLOSE_TAG: &str = "</review_comment>";

struct ReviewCommentBlockMatch<'a> {
    start: usize,
    end: usize,
    attributes: &'a str,
    body: &'a str,
}

/// `/<review_comment\b([^>]*)>\s*([\s\S]*?)<\/review_comment>/g`
fn review_comment_block_matches(value: &str) -> Vec<ReviewCommentBlockMatch<'_>> {
    let mut matches = Vec::new();
    let mut cursor = 0;
    while let Some(found) = value[cursor..].find(REVIEW_COMMENT_OPEN_TAG) {
        let start = cursor + found;
        let after_tag = start + REVIEW_COMMENT_OPEN_TAG.len();
        // `\b`: the next character must not extend the tag-name word.
        if value[after_tag..]
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            cursor = start + 1;
            continue;
        }
        // `[^>]*>` — attributes run to the first `>`; none anywhere means no
        // later start can complete either.
        let Some(gt) = value[after_tag..].find('>') else {
            break;
        };
        let attributes_end = after_tag + gt;
        // `\s*` is greedy; the close tag contains non-whitespace, so it can
        // never hide inside the skipped run.
        let after_whitespace = value[attributes_end + 1..].trim_start();
        let body_start = value.len() - after_whitespace.len();
        // Lazy body: up to the first close tag. None left means done.
        let Some(close) = value[body_start..].find(REVIEW_COMMENT_CLOSE_TAG) else {
            break;
        };
        let body_end = body_start + close;
        let end = body_end + REVIEW_COMMENT_CLOSE_TAG.len();
        matches.push(ReviewCommentBlockMatch {
            start,
            end,
            attributes: &value[after_tag..attributes_end],
            body: &value[body_start..body_end],
        });
        cursor = end;
    }
    matches
}

/// `/([a-zA-Z][a-zA-Z0-9_-]*)="([^"]*)"/g` — later duplicates win, like the
/// record assignment in Electron's `readReviewCommentAttributes`.
fn read_review_comment_attributes(raw_attributes: &str) -> Vec<(String, String)> {
    let bytes = raw_attributes.as_bytes();
    let mut attributes = Vec::new();
    let mut position = 0;
    while position < bytes.len() {
        if !bytes[position].is_ascii_alphabetic() {
            position += 1;
            continue;
        }
        let mut name_end = position + 1;
        while name_end < bytes.len()
            && (bytes[name_end].is_ascii_alphanumeric()
                || bytes[name_end] == b'_'
                || bytes[name_end] == b'-')
        {
            name_end += 1;
        }
        if !raw_attributes[name_end..].starts_with("=\"") {
            position += 1;
            continue;
        }
        let value_start = name_end + 2;
        let Some(quote) = raw_attributes[value_start..].find('"') else {
            position += 1;
            continue;
        };
        let value_end = value_start + quote;
        attributes.push((
            raw_attributes[position..name_end].to_owned(),
            unescape_review_comment_attribute(&raw_attributes[value_start..value_end]),
        ));
        position = value_end + 1;
    }
    attributes
}

fn attribute<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .rev()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

struct FenceMatch<'a> {
    start: usize,
    language: &'a str,
    contents: &'a str,
}

/// ``/(`{3,})([^\s`]*)[^\n]*\n([\s\S]*?)\n\1/g`` — the opening run is greedy
/// with backtracking (a shorter opening is retried until a same-length closing
/// run exists), and matches never overlap.
fn fence_matches(body: &str) -> Vec<FenceMatch<'_>> {
    let bytes = body.as_bytes();
    let mut matches = Vec::new();
    let mut position = 0;
    while position < bytes.len() {
        if bytes[position] != b'`' {
            position += 1;
            continue;
        }
        let mut run_end = position;
        while run_end < bytes.len() && bytes[run_end] == b'`' {
            run_end += 1;
        }
        if run_end - position < 3 {
            position = run_end;
            continue;
        }
        let mut matched = false;
        for fence_len in (3..=run_end - position).rev() {
            let after_open = position + fence_len;
            // `([^\s`]*)` — greedy; the rest-of-line class accepts a superset,
            // so the maximal run is final.
            let language_len = body[after_open..]
                .char_indices()
                .find(|(_, ch)| ch.is_whitespace() || *ch == '`')
                .map(|(offset, _)| offset)
                .unwrap_or(body.len() - after_open);
            let language_end = after_open + language_len;
            // `[^\n]*\n` — the header line must end in a newline.
            let Some(newline) = body[language_end..].find('\n') else {
                continue;
            };
            let contents_start = language_end + newline + 1;
            // Lazy contents: the earliest `\n` followed by `fence_len`
            // backticks closes the fence.
            let mut search = contents_start;
            let close = loop {
                let Some(found) = body[search..].find('\n') else {
                    break None;
                };
                let close_newline = search + found;
                let close_end = close_newline + 1 + fence_len;
                if close_end <= bytes.len()
                    && bytes[close_newline + 1..close_end]
                        .iter()
                        .all(|b| *b == b'`')
                {
                    break Some(close_newline);
                }
                search = close_newline + 1;
            };
            let Some(close_newline) = close else {
                continue;
            };
            matches.push(FenceMatch {
                start: position,
                language: &body[after_open..language_end],
                contents: &body[contents_start..close_newline],
            });
            position = close_newline + 1 + fence_len;
            matched = true;
            break;
        }
        if !matched {
            position += 1;
        }
    }
    matches
}

// ---------------------------------------------------------------------------
// Serialize / parse.
// ---------------------------------------------------------------------------

fn escape_review_comment_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unescape_review_comment_attribute(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// `/^\d+$/` then `Number(...)`.
fn read_non_negative_integer(value: Option<&str>) -> Option<usize> {
    let value = value?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

struct ReviewCommentBody {
    text: String,
    language: String,
    contents: String,
}

/// `extractReviewCommentBody`: the LAST fenced block is the context diff; the
/// text is everything before it.
fn extract_review_comment_body(raw_body: &str) -> ReviewCommentBody {
    match fence_matches(raw_body).last() {
        Some(fence) => ReviewCommentBody {
            text: raw_body[..fence.start].trim().to_owned(),
            language: {
                let trimmed = fence.language.trim();
                if trimmed.is_empty() { "diff" } else { trimmed }.to_owned()
            },
            contents: fence.contents.to_owned(),
        },
        None => ReviewCommentBody {
            text: raw_body.trim().to_owned(),
            language: "diff".to_owned(),
            contents: String::new(),
        },
    }
}

fn parse_review_comment_context(
    raw_attributes: &str,
    raw_body: &str,
    index: usize,
) -> Option<ReviewCommentContext> {
    let attributes = read_review_comment_attributes(raw_attributes);
    let start_index = read_non_negative_integer(attribute(&attributes, "startIndex"));
    let end_index = read_non_negative_integer(attribute(&attributes, "endIndex"));
    let file_path = attribute(&attributes, "filePath").map_or("", str::trim);
    let section_id = attribute(&attributes, "sectionId").map_or("", str::trim);
    if file_path.is_empty() || section_id.is_empty() {
        return None;
    }
    let (start_index, end_index) = (start_index?, end_index?);
    let body = extract_review_comment_body(raw_body);
    let section_title = attribute(&attributes, "sectionTitle").map_or("", str::trim);
    let range_label = attribute(&attributes, "rangeLabel").map_or("", str::trim);

    Some(ReviewCommentContext {
        // The id keeps the raw attribute order, before min/max normalization.
        id: format!("review-comment:{index}:{section_id}:{file_path}:{start_index}:{end_index}"),
        section_id: section_id.to_owned(),
        section_title: if section_title.is_empty() {
            "Review".to_owned()
        } else {
            section_title.to_owned()
        },
        file_path: file_path.to_owned(),
        start_index: start_index.min(end_index),
        end_index: start_index.max(end_index),
        range_label: if range_label.is_empty() {
            "line".to_owned()
        } else {
            range_label.to_owned()
        },
        text: body.text,
        diff: body.contents,
        fence_language: Some(body.language),
    })
}

/// `parseReviewCommentMessageSegments`: split a sent message into plain-text
/// runs and parsed review-comment blocks; malformed blocks stay as text.
pub fn parse_review_comment_message_segments(value: &str) -> Vec<ReviewCommentMessageSegment> {
    let mut segments = Vec::new();
    let mut cursor = 0;
    let mut parsed_comment_index = 0;

    for block in review_comment_block_matches(value) {
        let before_text = &value[cursor..block.start];
        if !before_text.is_empty() {
            segments.push(ReviewCommentMessageSegment::Text {
                id: format!("review-comment-text:{cursor}"),
                text: before_text.to_owned(),
            });
        }

        match parse_review_comment_context(block.attributes, block.body, parsed_comment_index) {
            Some(comment) => {
                segments.push(ReviewCommentMessageSegment::ReviewComment { comment });
                parsed_comment_index += 1;
            }
            None => segments.push(ReviewCommentMessageSegment::Text {
                id: format!("review-comment-invalid:{}", block.start),
                text: value[block.start..block.end].to_owned(),
            }),
        }

        cursor = block.end;
    }

    let rest = &value[cursor..];
    if !rest.is_empty() {
        segments.push(ReviewCommentMessageSegment::Text {
            id: format!("review-comment-text:{cursor}"),
            text: rest.to_owned(),
        });
    }

    segments
}

pub fn has_review_comment_message_segments(value: &str) -> bool {
    parse_review_comment_message_segments(value)
        .iter()
        .any(|segment| matches!(segment, ReviewCommentMessageSegment::ReviewComment { .. }))
}

/// `formatReviewCommentFence`: the fence is one backtick longer than the
/// longest run inside the contents (minimum three).
pub fn format_review_comment_fence(language: &str, contents: &str) -> String {
    let mut longest_backtick_run = 0usize;
    let mut run = 0usize;
    for byte in contents.bytes() {
        if byte == b'`' {
            run += 1;
            longest_backtick_run = longest_backtick_run.max(run);
        } else {
            run = 0;
        }
    }
    let fence = "`".repeat(3.max(longest_backtick_run + 1));
    format!("{fence}{language}\n{}\n{fence}", contents.trim_end())
}

/// `formatReviewCommentContext`: one serialized `<review_comment>` block.
pub fn format_review_comment_context(comment: &ReviewCommentContext) -> String {
    [
        format!(
            "<review_comment sectionId=\"{}\" sectionTitle=\"{}\" filePath=\"{}\" startIndex=\"{}\" endIndex=\"{}\" rangeLabel=\"{}\">",
            escape_review_comment_attribute(&comment.section_id),
            escape_review_comment_attribute(&comment.section_title),
            escape_review_comment_attribute(&comment.file_path),
            comment.start_index,
            comment.end_index,
            escape_review_comment_attribute(&comment.range_label),
        ),
        comment.text.trim().to_owned(),
        format_review_comment_fence(
            comment.fence_language.as_deref().unwrap_or("diff"),
            &comment.diff,
        ),
        "</review_comment>".to_owned(),
    ]
    .join("\n")
}

/// `appendReviewCommentsToPrompt`: blocks go after the trimmed prompt,
/// separated by blank lines.
pub fn append_review_comments_to_prompt(prompt: &str, comments: &[ReviewCommentContext]) -> String {
    if comments.is_empty() {
        return prompt.to_owned();
    }
    let blocks = comments
        .iter()
        .map(format_review_comment_context)
        .collect::<Vec<_>>()
        .join("\n\n");
    let trimmed_prompt = prompt.trim();
    if trimmed_prompt.is_empty() {
        blocks
    } else {
        format!("{trimmed_prompt}\n\n{blocks}")
    }
}

// ---------------------------------------------------------------------------
// File comments (editor selections).
// ---------------------------------------------------------------------------

pub struct FileReviewCommentInput<'a> {
    pub id: &'a str,
    pub file_path: &'a str,
    /// 1-based, in either order.
    pub start_line: usize,
    pub end_line: usize,
    pub text: &'a str,
    pub contents: &'a str,
}

/// `buildFileReviewComment`: a comment on raw file lines (no diff).
pub fn build_file_review_comment(input: &FileReviewCommentInput) -> ReviewCommentContext {
    let start_line = 1.max(input.start_line.min(input.end_line));
    let end_line = start_line.max(input.start_line.max(input.end_line));
    let lines: Vec<&str> = input.contents.split('\n').collect();
    let slice_start = (start_line - 1).min(lines.len());
    let slice_end = end_line.min(lines.len());
    ReviewCommentContext {
        id: input.id.to_owned(),
        section_id: format!("file:{}", input.file_path),
        section_title: "File comment".to_owned(),
        file_path: input.file_path.to_owned(),
        start_index: start_line - 1,
        end_index: end_line - 1,
        range_label: if start_line == end_line {
            format!("L{start_line}")
        } else {
            format!("L{start_line} to L{end_line}")
        },
        text: input.text.trim().to_owned(),
        diff: lines[slice_start..slice_end].join("\n"),
        fence_language: Some(infer_review_comment_fence_language(input.file_path)),
    }
}

/// `inferReviewCommentFenceLanguage`: file extension, dotfile name, or "text".
pub fn infer_review_comment_fence_language(file_path: &str) -> String {
    let normalized_path = file_path.replace('\\', "/");
    let file_name = normalized_path
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_lowercase();
    if let Some(extension_index) = file_name.rfind('.')
        && extension_index > 0
        && extension_index < file_name.len() - 1
    {
        return file_name[extension_index + 1..].to_owned();
    }
    if file_name.starts_with('.') && file_name.len() > 1 {
        return file_name[1..].to_owned();
    }
    "text".to_owned()
}

// ---------------------------------------------------------------------------
// Diff comments (diff-panel selections).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffReviewChange {
    Context,
    Add,
    Delete,
}

/// One flattened diff row — Electron's `DiffReviewLine`, built directly from
/// `PatchLine` (which already carries both side line numbers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffReviewLine<'a> {
    pub change: DiffReviewChange,
    pub old_line_number: Option<u32>,
    pub new_line_number: Option<u32>,
    pub content: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionSide {
    Deletions,
    Additions,
}

/// `SelectedLineRange` — a side-qualified line selection over a rendered diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedLineRange {
    pub start: u32,
    pub side: SelectionSide,
    pub end: u32,
    /// Falls back to `side` when absent.
    pub end_side: Option<SelectionSide>,
}

/// `buildDiffReviewLines`: flatten a file's hunks into ordered review rows.
pub fn build_diff_review_lines(file_diff: &PatchFile) -> Vec<DiffReviewLine<'_>> {
    file_diff
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| DiffReviewLine {
            change: match line.kind {
                PatchLineKind::Context => DiffReviewChange::Context,
                PatchLineKind::Add => DiffReviewChange::Add,
                PatchLineKind::Del => DiffReviewChange::Delete,
            },
            old_line_number: line.old_line,
            new_line_number: line.new_line,
            content: &line.content,
        })
        .collect()
}

/// `getDiffReviewSelectionPoint`: deletions anchor to the old side; everything
/// else prefers the new side.
fn diff_review_selection_point(line: &DiffReviewLine) -> Option<(u32, SelectionSide)> {
    if line.change == DiffReviewChange::Delete
        && let Some(number) = line.old_line_number
    {
        return Some((number, SelectionSide::Deletions));
    }
    if let Some(number) = line.new_line_number {
        return Some((number, SelectionSide::Additions));
    }
    if let Some(number) = line.old_line_number {
        return Some((number, SelectionSide::Deletions));
    }
    None
}

/// `restoreDiffReviewCommentRange`: persisted row indexes back to a
/// side-qualified selection.
pub fn restore_diff_review_comment_range(
    file_diff: &PatchFile,
    comment: &ReviewCommentContext,
) -> Option<SelectedLineRange> {
    let lines = build_diff_review_lines(file_diff);
    let start_line = lines.get(comment.start_index)?;
    let end_line = lines.get(comment.end_index)?;
    let (start, side) = diff_review_selection_point(start_line)?;
    let (end, end_side) = diff_review_selection_point(end_line)?;
    Some(SelectedLineRange {
        start,
        side,
        end,
        end_side: Some(end_side),
    })
}

/// `annotationSide` (AnnotatableCodeView): annotation groups anchor on the
/// range's END side — deletions when the end point sits on the old side.
pub fn annotation_side_is_deletions(range: &SelectedLineRange) -> bool {
    range.end_side.unwrap_or(range.side) == SelectionSide::Deletions
}

/// Vitre extension: a side-qualified selection from flattened row indices —
/// the diff panel's gutter drag anchors on row indices (Pierre's CodeView
/// resolves this internally in Electron), in either order.
pub fn selected_line_range_from_row_indices(
    file_diff: &PatchFile,
    anchor_index: usize,
    head_index: usize,
) -> Option<SelectedLineRange> {
    let lines = build_diff_review_lines(file_diff);
    let start_line = lines.get(anchor_index.min(head_index))?;
    let end_line = lines.get(anchor_index.max(head_index))?;
    let (start, side) = diff_review_selection_point(start_line)?;
    let (end, end_side) = diff_review_selection_point(end_line)?;
    Some(SelectedLineRange {
        start,
        side,
        end,
        end_side: Some(end_side),
    })
}

/// `findDiffReviewLineIndex`: prefer the selected side's numbering, fall back
/// to the other side.
fn find_diff_review_line_index(
    lines: &[DiffReviewLine],
    line_number: u32,
    side: SelectionSide,
) -> Option<usize> {
    let preferred = |line: &DiffReviewLine| match side {
        SelectionSide::Deletions => line.old_line_number,
        SelectionSide::Additions => line.new_line_number,
    };
    let fallback = |line: &DiffReviewLine| match side {
        SelectionSide::Deletions => line.new_line_number,
        SelectionSide::Additions => line.old_line_number,
    };
    lines
        .iter()
        .position(|line| preferred(line) == Some(line_number))
        .or_else(|| {
            lines
                .iter()
                .position(|line| fallback(line) == Some(line_number))
        })
}

/// `getDiffRange`: first numbered line and count on one side.
fn diff_range(lines: &[DiffReviewLine], old_side: bool) -> (u32, usize) {
    let mut start = None;
    let mut count = 0;
    for line in lines {
        let number = if old_side {
            line.old_line_number
        } else {
            line.new_line_number
        };
        if let Some(number) = number {
            start.get_or_insert(number);
            count += 1;
        }
    }
    (start.unwrap_or(0), count)
}

fn diff_change_marker(change: DiffReviewChange) -> char {
    match change {
        DiffReviewChange::Add => '+',
        DiffReviewChange::Delete => '-',
        DiffReviewChange::Context => ' ',
    }
}

/// `formatDiffReviewRangeLabel`: `+a to +b` / `-a` / `a to b` / `N lines`.
fn format_diff_review_range_label(lines: &[DiffReviewLine]) -> String {
    let (Some(first_line), Some(last_line)) = (lines.first(), lines.last()) else {
        return "line".to_owned();
    };
    let first_number = first_line.new_line_number.or(first_line.old_line_number);
    let last_number = last_line.new_line_number.or(last_line.old_line_number);
    let (Some(first_number), Some(last_number)) = (first_number, last_number) else {
        return if lines.len() == 1 {
            "line".to_owned()
        } else {
            format!("{} lines", lines.len())
        };
    };

    let first_marker = diff_change_marker(first_line.change);
    let marker = if first_marker != ' ' && lines.iter().all(|line| line.change == first_line.change)
    {
        first_marker.to_string()
    } else {
        String::new()
    };
    if first_number == last_number {
        format!("{marker}{first_number}")
    } else {
        format!("{marker}{first_number} to {marker}{last_number}")
    }
}

pub struct DiffReviewCommentInput<'a> {
    pub id: &'a str,
    pub section_id: &'a str,
    pub section_title: &'a str,
    pub file_path: &'a str,
    pub file_diff: &'a PatchFile,
    pub range: SelectedLineRange,
    pub text: &'a str,
}

/// `buildDiffReviewComment`: a comment on a diff selection, carrying a
/// synthesized `@@` hunk of exactly the selected rows.
pub fn build_diff_review_comment(input: &DiffReviewCommentInput) -> Option<ReviewCommentContext> {
    let lines = build_diff_review_lines(input.file_diff);
    let start_index = find_diff_review_line_index(&lines, input.range.start, input.range.side)?;
    let end_index = find_diff_review_line_index(
        &lines,
        input.range.end,
        input.range.end_side.unwrap_or(input.range.side),
    )?;

    let normalized_start_index = start_index.min(end_index);
    let normalized_end_index = start_index.max(end_index);
    let selected_lines = &lines[normalized_start_index..=normalized_end_index];
    let (old_start, old_count) = diff_range(selected_lines, true);
    let (new_start, new_count) = diff_range(selected_lines, false);

    let mut diff = format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@");
    for line in selected_lines {
        diff.push('\n');
        diff.push(diff_change_marker(line.change));
        diff.push_str(line.content);
    }

    Some(ReviewCommentContext {
        id: input.id.to_owned(),
        section_id: input.section_id.to_owned(),
        section_title: input.section_title.to_owned(),
        file_path: input.file_path.to_owned(),
        start_index: normalized_start_index,
        end_index: normalized_end_index,
        range_label: format_diff_review_range_label(selected_lines),
        text: input.text.trim().to_owned(),
        diff,
        fence_language: Some("diff".to_owned()),
    })
}

/// `buildReviewCommentRenderablePatch`: wrap a bare `@@` hunk in `diff --git`
/// headers so the patch parser can render it; non-diff fences render as code.
pub fn build_review_comment_renderable_patch(comment: &ReviewCommentContext) -> String {
    if comment.fence_language.as_deref().unwrap_or("diff") != "diff" {
        return String::new();
    }
    let diff = comment.diff.trim();
    if diff.is_empty() {
        return String::new();
    }
    if diff.starts_with("diff --git ") {
        return diff.to_owned();
    }

    let normalized_path = comment.file_path.replace('\\', "/");
    format!(
        "diff --git a/{normalized_path} b/{normalized_path}\n--- a/{normalized_path}\n+++ b/{normalized_path}\n{diff}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff_patch::parse_unified_patch;

    fn expect_comment(segment: &ReviewCommentMessageSegment) -> &ReviewCommentContext {
        match segment {
            ReviewCommentMessageSegment::ReviewComment { comment } => comment,
            other => panic!("expected a review-comment segment, got {other:?}"),
        }
    }

    fn expect_text(segment: &ReviewCommentMessageSegment) -> &str {
        match segment {
            ReviewCommentMessageSegment::Text { text, .. } => text,
            other => panic!("expected a text segment, got {other:?}"),
        }
    }

    #[test]
    fn extracts_comment_metadata_user_text_and_fenced_diff_without_raw_wrapper_text() {
        let segments = parse_review_comment_message_segments(
            &[
                "Before <review_comment sectionId=\"turn:2\" sectionTitle=\"Turn 2\" filePath=\"apps/web/src/lib/contextWindow.test.ts\" startIndex=\"3\" endIndex=\"14\" rangeLabel=\"+47 to +58\">",
                "Wadduo",
                "```diff",
                "@@ -0,0 +47,2 @@",
                "+  it(\"keeps valid zero-usage snapshots\", () => {",
                "+    expect(snapshot).not.toBeNull();",
                "```",
                "</review_comment> after",
            ]
            .join("\n"),
        );

        assert_eq!(segments.len(), 3);
        assert!(expect_text(&segments[0]).contains("Before"));
        let comment = expect_comment(&segments[1]);
        assert_eq!(comment.file_path, "apps/web/src/lib/contextWindow.test.ts");
        assert_eq!(comment.range_label, "+47 to +58");
        assert_eq!(comment.text, "Wadduo");
        assert!(
            comment
                .diff
                .contains("it(\"keeps valid zero-usage snapshots\"")
        );
        assert_eq!(comment.start_index, 3);
        assert_eq!(comment.end_index, 14);
        assert_eq!(expect_text(&segments[2]), " after");
    }

    #[test]
    fn wraps_hunk_only_review_diffs_in_a_renderable_file_patch() {
        let segments = parse_review_comment_message_segments(
            &[
                "<review_comment sectionId=\"s\" filePath=\"src/app.ts\" startIndex=\"0\" endIndex=\"0\">",
                "Please check this.",
                "```diff",
                "@@ -1,1 +1,1 @@",
                "-old",
                "+new",
                "```",
                "</review_comment>",
            ]
            .join("\n"),
        );

        let comment = expect_comment(&segments[0]);
        assert_eq!(
            build_review_comment_renderable_patch(comment),
            [
                "diff --git a/src/app.ts b/src/app.ts",
                "--- a/src/app.ts",
                "+++ b/src/app.ts",
                "@@ -1,1 +1,1 @@",
                "-old",
                "+new",
            ]
            .join("\n")
        );
    }

    #[test]
    fn formats_editable_file_comments_with_the_mobile_review_comment_contract() {
        let comment = build_file_review_comment(&FileReviewCommentInput {
            id: "comment-1",
            file_path: "src/app.ts",
            start_line: 2,
            end_line: 3,
            text: "Keep this configurable.",
            contents: "one\ntwo\nthree\nfour",
        });
        let prompt = append_review_comments_to_prompt("Please update this.", &[comment]);
        let segments = parse_review_comment_message_segments(&prompt);

        assert_eq!(segments.len(), 2);
        let parsed = expect_comment(&segments[1]);
        assert_eq!(parsed.file_path, "src/app.ts");
        assert_eq!(parsed.start_index, 1);
        assert_eq!(parsed.end_index, 2);
        assert_eq!(parsed.range_label, "L2 to L3");
        assert_eq!(parsed.text, "Keep this configurable.");
        assert_eq!(parsed.diff, "two\nthree");
        assert_eq!(parsed.fence_language.as_deref(), Some("ts"));
        assert!(prompt.contains("```ts\ntwo\nthree\n```"));
    }

    #[test]
    fn formats_mixed_diff_side_selections_with_the_mobile_review_comment_contract() {
        let files = parse_unified_patch(
            &[
                "diff --git a/src/app.ts b/src/app.ts",
                "--- a/src/app.ts",
                "+++ b/src/app.ts",
                "@@ -1,4 +1,4 @@",
                " one",
                "-two",
                "+TWO",
                " three",
                " four",
            ]
            .join("\n"),
        );

        let comment = build_diff_review_comment(&DiffReviewCommentInput {
            id: "comment-2",
            section_id: "turn:2",
            section_title: "Turn 2",
            file_path: "src/app.ts",
            file_diff: &files[0],
            range: SelectedLineRange {
                start: 2,
                side: SelectionSide::Deletions,
                end: 2,
                end_side: Some(SelectionSide::Additions),
            },
            text: "Keep this compatible.",
        })
        .expect("selection resolves to diff rows");

        assert_eq!(comment.section_id, "turn:2");
        assert_eq!(comment.section_title, "Turn 2");
        assert_eq!(comment.file_path, "src/app.ts");
        assert_eq!(comment.start_index, 1);
        assert_eq!(comment.end_index, 2);
        assert_eq!(comment.range_label, "2");
        assert_eq!(comment.text, "Keep this compatible.");
        assert_eq!(comment.diff, "@@ -2,1 +2,1 @@\n-two\n+TWO");
        assert_eq!(comment.fence_language.as_deref(), Some("diff"));
    }

    #[test]
    fn uses_file_extensions_for_source_comments_and_preserves_nested_markdown_fences() {
        assert_eq!(infer_review_comment_fence_language("docs/plan.md"), "md");
        assert_eq!(infer_review_comment_fence_language("src/view.tsx"), "tsx");

        let serialized = format_review_comment_context(&ReviewCommentContext {
            id: "comment-3".to_owned(),
            section_id: "file:docs/plan.md".to_owned(),
            section_title: "File comment".to_owned(),
            file_path: "docs/plan.md".to_owned(),
            start_index: 0,
            end_index: 2,
            range_label: "L1 to L3".to_owned(),
            text: "Update this example.".to_owned(),
            diff: ["# Example", "```ts", "const value = 1;", "```"].join("\n"),
            fence_language: Some("md".to_owned()),
        });
        let segments = parse_review_comment_message_segments(&serialized);

        assert!(serialized.contains("````md"));
        let comment = expect_comment(&segments[0]);
        assert_eq!(comment.fence_language.as_deref(), Some("md"));
        assert_eq!(
            comment.diff,
            ["# Example", "```ts", "const value = 1;", "```"].join("\n")
        );
    }

    #[test]
    fn round_trips_greater_than_signs_in_attributes() {
        let serialized = format_review_comment_context(&ReviewCommentContext {
            id: "comment-4".to_owned(),
            section_id: "turn:4".to_owned(),
            section_title: "Changes > 5".to_owned(),
            file_path: "src/app.ts".to_owned(),
            start_index: 0,
            end_index: 0,
            range_label: "+1".to_owned(),
            text: "Check this.".to_owned(),
            diff: "@@ -0,0 +1,1 @@\n+one".to_owned(),
            fence_language: Some("diff".to_owned()),
        });
        let segments = parse_review_comment_message_segments(&serialized);

        assert!(serialized.contains("sectionTitle=\"Changes &gt; 5\""));
        assert_eq!(expect_comment(&segments[0]).section_title, "Changes > 5");
    }

    #[test]
    fn keeps_fenced_examples_in_comment_text_separate_from_the_final_context_fence() {
        let text = [
            "Try this:",
            "```ts",
            "const value = 1;",
            "```",
            "Then retry.",
        ]
        .join("\n");
        let serialized = format_review_comment_context(&ReviewCommentContext {
            id: "comment-5".to_owned(),
            section_id: "turn:5".to_owned(),
            section_title: "Turn 5".to_owned(),
            file_path: "src/app.ts".to_owned(),
            start_index: 0,
            end_index: 0,
            range_label: "+1".to_owned(),
            text: text.clone(),
            diff: "@@ -0,0 +1,1 @@\n+one".to_owned(),
            fence_language: Some("diff".to_owned()),
        });
        let segments = parse_review_comment_message_segments(&serialized);

        let comment = expect_comment(&segments[0]);
        assert_eq!(comment.text, text);
        assert_eq!(comment.diff, "@@ -0,0 +1,1 @@\n+one");
        assert_eq!(comment.fence_language.as_deref(), Some("diff"));
    }

    #[test]
    fn restores_line_selections_from_persisted_diff_comment_row_indexes() {
        let files = parse_unified_patch(
            &[
                "diff --git a/src/app.ts b/src/app.ts",
                "--- a/src/app.ts",
                "+++ b/src/app.ts",
                "@@ -1,3 +1,3 @@",
                " one",
                "-two",
                "+TWO",
                " three",
            ]
            .join("\n"),
        );
        let comment = build_diff_review_comment(&DiffReviewCommentInput {
            id: "comment-6",
            section_id: "turn:6",
            section_title: "Turn 6",
            file_path: "src/app.ts",
            file_diff: &files[0],
            range: SelectedLineRange {
                start: 2,
                side: SelectionSide::Deletions,
                end: 2,
                end_side: Some(SelectionSide::Additions),
            },
            text: "Keep both sides.",
        })
        .expect("selection resolves to diff rows");

        assert_eq!(
            restore_diff_review_comment_range(&files[0], &comment),
            Some(SelectedLineRange {
                start: 2,
                side: SelectionSide::Deletions,
                end: 2,
                end_side: Some(SelectionSide::Additions),
            })
        );
    }

    #[test]
    fn bodies_without_a_fence_become_trimmed_text_with_an_empty_diff() {
        let segments = parse_review_comment_message_segments(
            "<review_comment sectionId=\"s\" filePath=\"a.txt\" startIndex=\"0\" endIndex=\"0\">\n  Just words.  \n</review_comment>",
        );
        let comment = expect_comment(&segments[0]);
        assert_eq!(comment.text, "Just words.");
        assert_eq!(comment.diff, "");
        assert_eq!(comment.fence_language.as_deref(), Some("diff"));
        assert_eq!(comment.section_title, "Review");
        assert_eq!(comment.range_label, "line");
    }

    #[test]
    fn blocks_missing_required_attributes_stay_as_raw_text() {
        let value = "<review_comment sectionId=\"s\" startIndex=\"0\" endIndex=\"0\">no file</review_comment>";
        let segments = parse_review_comment_message_segments(value);
        assert_eq!(segments.len(), 1);
        assert_eq!(expect_text(&segments[0]), value);
        assert!(!has_review_comment_message_segments(value));
    }

    #[test]
    fn swapped_indexes_normalize_but_the_id_keeps_the_raw_order() {
        let segments = parse_review_comment_message_segments(
            "<review_comment sectionId=\"s\" filePath=\"a.txt\" startIndex=\"5\" endIndex=\"2\">x</review_comment>",
        );
        let comment = expect_comment(&segments[0]);
        assert_eq!(comment.start_index, 2);
        assert_eq!(comment.end_index, 5);
        assert_eq!(comment.id, "review-comment:0:s:a.txt:5:2");
    }

    #[test]
    fn fences_grow_past_the_longest_backtick_run_in_the_contents() {
        let fence = format_review_comment_fence("md", "````\ncode\n````");
        assert!(fence.starts_with("`````md\n"));
        assert!(fence.ends_with("\n`````"));
    }

    #[test]
    fn row_index_selections_resolve_sides_like_the_persisted_restore_path() {
        let files = parse_unified_patch(
            &[
                "diff --git a/src/app.ts b/src/app.ts",
                "--- a/src/app.ts",
                "+++ b/src/app.ts",
                "@@ -1,3 +1,3 @@",
                " one",
                "-two",
                "+TWO",
                " three",
            ]
            .join("\n"),
        );
        // Rows: 0 ctx one, 1 del two, 2 add TWO, 3 ctx three. Head before
        // anchor normalizes.
        let range = selected_line_range_from_row_indices(&files[0], 2, 1)
            .expect("both rows resolve to selection points");
        assert_eq!(
            range,
            SelectedLineRange {
                start: 2,
                side: SelectionSide::Deletions,
                end: 2,
                end_side: Some(SelectionSide::Additions),
            }
        );
        assert!(!annotation_side_is_deletions(&range));
        assert!(annotation_side_is_deletions(&SelectedLineRange {
            start: 2,
            side: SelectionSide::Deletions,
            end: 2,
            end_side: None,
        }));
        assert_eq!(selected_line_range_from_row_indices(&files[0], 0, 99), None);
    }

    #[test]
    fn appending_to_an_empty_prompt_emits_only_the_blocks() {
        let comment = build_file_review_comment(&FileReviewCommentInput {
            id: "comment-7",
            file_path: "a.txt",
            start_line: 1,
            end_line: 1,
            text: "Note.",
            contents: "only",
        });
        let prompt = append_review_comments_to_prompt("   ", &[comment]);
        assert!(prompt.starts_with("<review_comment "));
        assert_eq!(append_review_comments_to_prompt("keep me", &[]), "keep me");
    }
}
