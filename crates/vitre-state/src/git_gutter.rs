//! Git diff gutter model: the pure half of Electron's
//! `apps/web/src/components/files/codemirror/gitDiffGutter.ts`.
//!
//! The buffer is diffed live against the HEAD baseline (debounced, capped at
//! [`MAX_DIFF_LINES`], so markers track *unsaved* edits); the index baseline
//! only classifies each hunk as staged (hollow markers) or unstaged (solid).
//!
//! ## Why this is not just a line diff
//!
//! Electron gets its hunks from `@codemirror/merge`'s `Chunk.build`, which
//! runs a *character* diff and then snaps each change out to line boundaries
//! (`fromLine`/`toLine`). The snapping is not cosmetic: it is what makes a
//! deletion that runs to the end of the file read as "the last surviving line
//! is modified, with a deletion hanging below it" rather than as a bare
//! boundary wedge, because such a deletion owns the *previous* line's line
//! break. [`build_chunks`] reproduces that by running a line diff and then
//! putting its hunks through ports of the same two functions, which is
//! equivalent for line-oriented edits and far cheaper than a character diff
//! over a 20k-line file.
//!
//! ## Line terminators
//!
//! CodeMirror normalises the document it loads (`/\r\n?|\n/`), so Electron
//! never diffs CRLF against LF. Here the editor holds whatever the file
//! contained, so instead of normalising — which would desynchronise every
//! offset from the editor's own — lines are *tokenised* without their
//! terminators. A CRLF blob and an LF buffer then intern identically while
//! every offset this module reports stays a real offset into the text it came
//! from. The one place terminators are re-normalised is text copied *out* of
//! the baseline ([`DiffText::slice_lf`]), so reverting a hunk cannot smuggle
//! CRLF into an LF buffer.

use std::ops::Range;

use imara_diff::{Algorithm, Diff, InternedInput};

/// Beyond this many lines (buffer or baseline) the gutter stays empty.
pub const MAX_DIFF_LINES: usize = 20_000;

/// Line-indexed text, the `@codemirror/state` `Text` this module is ported
/// against. Line numbers are 1-based, offsets are byte offsets into the
/// original contents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffText {
    contents: String,
    /// Byte offset each line begins at.
    line_starts: Vec<usize>,
    /// Byte offset each line's content ends at, before its terminator.
    line_ends: Vec<usize>,
}

impl DiffText {
    /// Index `contents`, splitting on `\r\n`, a lone `\r`, or `\n`.
    ///
    /// A trailing terminator yields a final empty line, so `"a\n"` is two
    /// lines and `""` is one — CodeMirror's line model, which the marker
    /// arithmetic below depends on.
    pub fn new(contents: impl Into<String>) -> Self {
        let contents = contents.into();
        let bytes = contents.as_bytes();
        let mut line_starts = vec![0usize];
        let mut line_ends = Vec::new();
        let mut index = 0usize;
        while index < bytes.len() {
            match bytes[index] {
                b'\n' => {
                    line_ends.push(index);
                    index += 1;
                    line_starts.push(index);
                }
                b'\r' => {
                    line_ends.push(index);
                    index += if bytes.get(index + 1) == Some(&b'\n') {
                        2
                    } else {
                        1
                    };
                    line_starts.push(index);
                }
                _ => index += 1,
            }
        }
        line_ends.push(bytes.len());
        Self {
            contents,
            line_starts,
            line_ends,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.contents
    }

    pub fn len(&self) -> usize {
        self.contents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }

    /// Number of lines; never zero.
    pub fn lines_len(&self) -> usize {
        self.line_starts.len()
    }

    /// 1-based number of the line containing `offset`, clamped to the document.
    pub fn line_at(&self, offset: usize) -> usize {
        let offset = offset.min(self.len());
        self.line_starts.partition_point(|&start| start <= offset)
    }

    /// Byte offset 1-based `line` begins at, clamped to the last line.
    pub fn line_from(&self, line: usize) -> usize {
        let index = line.max(1).min(self.lines_len()) - 1;
        self.line_starts[index]
    }

    /// Byte offset 1-based `line`'s content ends at, before its terminator.
    pub fn line_to(&self, line: usize) -> usize {
        let index = line.max(1).min(self.lines_len()) - 1;
        self.line_ends[index]
    }

    /// Byte offset 1-based `line` begins at, or one *past* the document for
    /// the line after the last.
    ///
    /// CodeMirror spells this `lineAt(pos).to + 1`, and the overshoot is
    /// load-bearing: it is the only thing that distinguishes "this chunk
    /// covers the last line" from "this chunk covers nothing" once the
    /// terminator-less final line is reached, which is what decides whether
    /// new content against an empty baseline reads as modified or added.
    fn next_line_start(&self, line: usize) -> usize {
        if line <= self.lines_len() {
            self.line_from(line)
        } else {
            self.len() + 1
        }
    }

    /// Raw slice, clamped to the document.
    pub fn slice(&self, range: Range<usize>) -> &str {
        let start = range.start.min(self.len());
        let end = range.end.max(start).min(self.len());
        &self.contents[start..end]
    }

    /// [`Self::slice`] with every terminator collapsed to `\n`, for text
    /// copied out of a baseline into the editor's buffer.
    pub fn slice_lf(&self, range: Range<usize>) -> String {
        let raw = self.slice(range);
        if raw.as_bytes().contains(&b'\r') {
            raw.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            raw.to_owned()
        }
    }

    /// Line contents *without* terminators — the diff tokens.
    fn line_slices(&self) -> impl Iterator<Item = &str> {
        (0..self.lines_len())
            .map(|index| &self.contents[self.line_starts[index]..self.line_ends[index]])
    }

    /// Byte offset a 0-based line index begins at, or the end of the document
    /// for the index one past the last line.
    fn line_index_offset(&self, index: usize) -> usize {
        self.line_starts.get(index).copied().unwrap_or(self.len())
    }
}

/// A run of changed lines, the `@codemirror/merge` `Chunk` this module is
/// ported against. `to_a`/`to_b` sit at the start of the line *after* the
/// chunk, and may point one past their document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffChunk {
    pub from_a: usize,
    pub to_a: usize,
    pub from_b: usize,
    pub to_b: usize,
}

impl DiffChunk {
    /// `from_a` when the chunk covers no lines in A, otherwise the end of its
    /// last line.
    pub fn end_a(&self) -> usize {
        self.to_a.saturating_sub(1).max(self.from_a)
    }

    /// `from_b` when the chunk covers no lines in B, otherwise the end of its
    /// last line.
    pub fn end_b(&self) -> usize {
        self.to_b.saturating_sub(1).max(self.from_b)
    }
}

/// A raw pre-snapping change, `@codemirror/merge`'s `Change`.
struct RawChange {
    from_a: usize,
    to_a: usize,
    from_b: usize,
    to_b: usize,
}

/// Port of `@codemirror/merge`'s `fromLine`: a change that begins exactly at
/// the end of a line on *both* sides belongs to the next line, not to the one
/// whose terminator it starts on.
fn from_line(from_a: usize, from_b: usize, a: &DiffText, b: &DiffText) -> (usize, usize) {
    let line_a = a.line_at(from_a);
    let line_b = b.line_at(from_b);
    if a.line_to(line_a) == from_a
        && b.line_to(line_b) == from_b
        && from_a < a.len()
        && from_b < b.len()
    {
        (a.next_line_start(line_a + 1), b.next_line_start(line_b + 1))
    } else {
        (a.line_from(line_a), b.line_from(line_b))
    }
}

/// Port of `@codemirror/merge`'s `toLine`: a change that does not already end
/// on a line boundary on both sides is extended past the end of the lines it
/// touches.
fn to_line(to_a: usize, to_b: usize, a: &DiffText, b: &DiffText) -> (usize, usize) {
    let line_a = a.line_at(to_a);
    let line_b = b.line_at(to_b);
    if a.line_from(line_a) == to_a && b.line_from(line_b) == to_b {
        (to_a, to_b)
    } else {
        (a.next_line_start(line_a + 1), b.next_line_start(line_b + 1))
    }
}

/// Port of `@codemirror/merge`'s `toChunks`: snap each change to line
/// boundaries and merge changes that end up adjacent.
fn to_chunks(changes: &[RawChange], a: &DiffText, b: &DiffText) -> Vec<DiffChunk> {
    let mut chunks = Vec::new();
    let mut index = 0;
    while index < changes.len() {
        let change = &changes[index];
        let (from_a, from_b) = from_line(change.from_a, change.from_b, a, b);
        let (mut to_a, mut to_b) = to_line(change.to_a, change.to_b, a, b);
        while index + 1 < changes.len() {
            let next = &changes[index + 1];
            let (next_a, next_b) = from_line(next.from_a, next.from_b, a, b);
            if next_a > to_a + 1 && next_b > to_b + 1 {
                break;
            }
            (to_a, to_b) = to_line(next.to_a, next.to_b, a, b);
            index += 1;
        }
        chunks.push(DiffChunk {
            from_a,
            to_a: to_a.max(from_a),
            from_b,
            to_b: to_b.max(from_b),
        });
        index += 1;
    }
    chunks
}

/// Changed chunks between two documents.
pub fn build_chunks(a: &DiffText, b: &DiffText) -> Vec<DiffChunk> {
    if a.contents == b.contents {
        return Vec::new();
    }
    let mut input = InternedInput::default();
    input.update_before(a.line_slices());
    input.update_after(b.line_slices());
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    // git's slider heuristic: a hunk that could sit at several equally valid
    // offsets is pushed to the one a reader would have picked.
    diff.postprocess_lines(&input);

    let changes: Vec<RawChange> = diff
        .hunks()
        .map(|hunk| RawChange {
            from_a: a.line_index_offset(hunk.before.start as usize),
            to_a: a.line_index_offset(hunk.before.end as usize),
            from_b: b.line_index_offset(hunk.after.start as usize),
            to_b: b.line_index_offset(hunk.after.end as usize),
        })
        .collect();
    to_chunks(&changes, a, b)
}

/// The bar a changed line carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    Added,
    Modified,
}

/// Where a deletion wedge hangs off a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerWedge {
    Above,
    Below,
}

/// One line's gutter decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerSpec {
    /// 1-based document line number.
    pub line: usize,
    /// `None` for a boundary-only line (a pure-deletion wedge with no bar).
    pub kind: Option<MarkerKind>,
    /// True when the whole hunk is clean against the index baseline.
    pub staged: bool,
    pub wedge: Option<MarkerWedge>,
}

/// Lines a chunk spans in `text`; zero when it covers none (a pure deletion
/// on the B side, or a pure insertion on the A side).
fn chunk_line_span(text: &DiffText, from: usize, to: usize, end: usize) -> usize {
    if from == to {
        return 0;
    }
    let last = end.min(text.len());
    text.line_at(last) - text.line_at(from.min(text.len())) + 1
}

/// A hunk is staged when no buffer-vs-index chunk touches its document range.
/// Bounds are inclusive so a zero-width deletion chunk still counts.
fn chunk_is_staged(chunk: &DiffChunk, index_chunks: Option<&[DiffChunk]>) -> bool {
    let Some(index_chunks) = index_chunks else {
        return false;
    };
    !index_chunks
        .iter()
        .any(|other| other.from_b <= chunk.to_b && chunk.from_b <= other.to_b)
}

/// Per-line gutter markers for `doc` against the HEAD baseline `head_text`,
/// classified staged/unstaged against `index_text`.
pub fn compute_marker_specs(
    head_text: &DiffText,
    doc: &DiffText,
    index_text: Option<&DiffText>,
) -> Vec<MarkerSpec> {
    let chunks = build_chunks(head_text, doc);
    if chunks.is_empty() {
        return Vec::new();
    }
    let index_chunks = index_text.map(|index| build_chunks(index, doc));
    let mut specs = Vec::new();
    for chunk in &chunks {
        let staged = chunk_is_staged(chunk, index_chunks.as_deref());
        let lines_a = chunk_line_span(head_text, chunk.from_a, chunk.to_a, chunk.end_a());
        let lines_b = chunk_line_span(doc, chunk.from_b, chunk.to_b, chunk.end_b());
        if lines_b == 0 {
            // Pure deletion: a wedge on the boundary line. A deletion at EOF
            // has no line below it, so the wedge hangs off the last line.
            let at_eof = chunk.from_b >= doc.len();
            specs.push(MarkerSpec {
                line: doc.line_at(chunk.from_b),
                kind: None,
                staged,
                wedge: Some(if at_eof {
                    MarkerWedge::Below
                } else {
                    MarkerWedge::Above
                }),
            });
            continue;
        }
        let first_line = doc.line_at(chunk.from_b);
        let modified_count = lines_a.min(lines_b);
        for offset in 0..lines_b {
            let is_last = offset == lines_b - 1;
            specs.push(MarkerSpec {
                line: first_line + offset,
                kind: Some(if offset < modified_count {
                    MarkerKind::Modified
                } else {
                    MarkerKind::Added
                }),
                staged,
                // Net-shrinking replacement: the trailing deletion hangs below
                // the hunk's last surviving line (VS Code semantics).
                wedge: (is_last && lines_a > lines_b).then_some(MarkerWedge::Below),
            });
        }
    }
    specs
}

/// What the overview ruler paints per change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverviewKind {
    Added,
    Modified,
    Deleted,
}

/// Contiguous same-kind marker lines, drawn as one mark on the ruler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverviewRun {
    pub kind: OverviewKind,
    pub staged: bool,
    pub first_line: usize,
    pub last_line: usize,
}

/// Collapse per-line specs into hunk-sized runs. A spec carrying both a bar
/// and a deletion wedge stays one run of its bar kind: the ruler shows one
/// mark per change, not one per line.
pub fn build_overview_runs(specs: &[MarkerSpec]) -> Vec<OverviewRun> {
    let mut runs: Vec<OverviewRun> = Vec::new();
    for spec in specs {
        let kind = match spec.kind {
            Some(MarkerKind::Added) => OverviewKind::Added,
            Some(MarkerKind::Modified) => OverviewKind::Modified,
            None => OverviewKind::Deleted,
        };
        if let Some(previous) = runs.last_mut()
            && previous.kind == kind
            && previous.staged == spec.staged
            && spec.line <= previous.last_line + 1
        {
            previous.last_line = previous.last_line.max(spec.line);
            continue;
        }
        runs.push(OverviewRun {
            kind,
            staged: spec.staged,
            first_line: spec.line,
            last_line: spec.line,
        });
    }
    runs
}

/// First and last document lines a chunk visually owns, including the
/// boundary line of a pure deletion.
pub fn chunk_line_range(doc: &DiffText, chunk: &DiffChunk) -> (usize, usize) {
    let first = doc.line_at(chunk.from_b);
    if chunk.from_b == chunk.to_b {
        return (first, first);
    }
    (first, doc.line_at(chunk.end_b()))
}

/// Index of the chunk owning 1-based `line`.
pub fn chunk_index_at_line(doc: &DiffText, chunks: &[DiffChunk], line: usize) -> Option<usize> {
    chunks.iter().position(|chunk| {
        let (first, last) = chunk_line_range(doc, chunk);
        (first..=last).contains(&line)
    })
}

/// Line the peek attaches under: the hunk's last, or its boundary line.
pub fn chunk_anchor_line(doc: &DiffText, chunk: &DiffChunk) -> usize {
    if chunk.from_b == chunk.to_b {
        doc.line_at(chunk.from_b)
    } else {
        doc.line_at(chunk.end_b())
    }
}

/// The chunk's HEAD text, for the peek body. Empty for a pure addition.
pub fn chunk_original_text(head_text: &DiffText, chunk: &DiffChunk) -> String {
    head_text.slice_lf(chunk.from_a..chunk.end_a())
}

/// Buffer edit that restores a chunk to its HEAD contents.
///
/// Both ends are clamped: chunk ends include the trailing line break and may
/// point one past a document that lacks one, and clipping both sides keeps
/// the replacement newline-balanced.
pub fn chunk_revert_edit(
    head_text: &DiffText,
    doc: &DiffText,
    chunk: &DiffChunk,
) -> (Range<usize>, String) {
    let replace = chunk.from_b..chunk.to_b.min(doc.len());
    (replace, head_text.slice_lf(chunk.from_a..chunk.to_a))
}

/// Chunk to jump to from `offset`, wrapping at either end. `forward` picks the
/// first chunk starting after the caret, otherwise the last starting before it.
pub fn goto_chunk_index(
    doc: &DiffText,
    chunks: &[DiffChunk],
    offset: usize,
    forward: bool,
) -> Option<usize> {
    if chunks.is_empty() {
        return None;
    }
    let starts: Vec<usize> = chunks
        .iter()
        .map(|chunk| chunk.from_b.min(doc.len()))
        .collect();
    let index = if forward {
        starts.iter().position(|&start| start > offset).unwrap_or(0)
    } else {
        starts
            .iter()
            .rposition(|&start| start < offset)
            .unwrap_or(starts.len() - 1)
    };
    Some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(head: &str, buffer: &str, index: Option<&str>) -> Vec<MarkerSpec> {
        let index = index.map(DiffText::new);
        compute_marker_specs(&DiffText::new(head), &DiffText::new(buffer), index.as_ref())
    }

    fn bar(line: usize, kind: MarkerKind) -> MarkerSpec {
        MarkerSpec {
            line,
            kind: Some(kind),
            staged: false,
            wedge: None,
        }
    }

    fn staged_bar(line: usize, kind: MarkerKind) -> MarkerSpec {
        MarkerSpec {
            staged: true,
            ..bar(line, kind)
        }
    }

    #[test]
    fn identical_documents_have_no_specs() {
        assert_eq!(specs("a\nb\nc\n", "a\nb\nc\n", None), vec![]);
        assert_eq!(specs("", "", None), vec![]);
    }

    #[test]
    fn a_changed_line_is_modified() {
        assert_eq!(
            specs("a\nb\nc\n", "a\nB\nc\n", None),
            vec![bar(2, MarkerKind::Modified)]
        );
    }

    #[test]
    fn inserted_lines_are_added() {
        assert_eq!(
            specs("a\nb\n", "a\nx\ny\nb\n", None),
            vec![bar(2, MarkerKind::Added), bar(3, MarkerKind::Added)]
        );
    }

    #[test]
    fn appended_lines_at_the_end_of_the_file_are_added() {
        assert_eq!(
            specs("a\n", "a\nb\nc\n", None),
            vec![bar(2, MarkerKind::Added), bar(3, MarkerKind::Added)]
        );
    }

    #[test]
    fn a_growing_replacement_splits_into_modified_then_added() {
        assert_eq!(
            specs("a\nb\nc\nd\n", "a\nX\nY\nZ\nd\n", None),
            vec![
                bar(2, MarkerKind::Modified),
                bar(3, MarkerKind::Modified),
                bar(4, MarkerKind::Added),
            ]
        );
    }

    #[test]
    fn a_shrinking_replacement_hangs_a_wedge_below() {
        assert_eq!(
            specs("a\nb\nc\nd\ne\n", "a\nX\nY\ne\n", None),
            vec![
                bar(2, MarkerKind::Modified),
                MarkerSpec {
                    wedge: Some(MarkerWedge::Below),
                    ..bar(3, MarkerKind::Modified)
                },
            ]
        );
    }

    #[test]
    fn a_pure_deletion_puts_a_wedge_above_the_boundary_line() {
        assert_eq!(
            specs("a\nb\nc\n", "a\nc\n", None),
            vec![MarkerSpec {
                line: 2,
                kind: None,
                staged: false,
                wedge: Some(MarkerWedge::Above),
            }]
        );
    }

    #[test]
    fn deleting_the_leading_lines_wedges_above_the_first_line() {
        assert_eq!(
            specs("a\nb\nc\n", "c\n", None),
            vec![MarkerSpec {
                line: 1,
                kind: None,
                staged: false,
                wedge: Some(MarkerWedge::Above),
            }]
        );
    }

    #[test]
    fn a_trailing_deletion_hangs_below_the_last_line() {
        // A deletion at EOF owns the previous line's line break, so the chunk
        // includes the surviving line: it reads as modified with a wedge below.
        assert_eq!(
            specs("a\nb\nc", "a", None),
            vec![MarkerSpec {
                wedge: Some(MarkerWedge::Below),
                ..bar(1, MarkerKind::Modified)
            }]
        );
    }

    #[test]
    fn a_crlf_baseline_against_an_lf_buffer_is_unchanged() {
        assert_eq!(specs("a\r\nb\r\nc\r\n", "a\nb\nc\n", None), vec![]);
    }

    #[test]
    fn an_lf_baseline_against_a_crlf_buffer_is_unchanged() {
        assert_eq!(specs("a\nb\nc\n", "a\r\nb\r\nc\r\n", None), vec![]);
    }

    #[test]
    fn new_content_against_an_empty_baseline() {
        // An empty-but-committed file is one empty line, so the first content
        // line reads as modified and the rest as added.
        assert_eq!(
            specs("", "a\nb", None),
            vec![bar(1, MarkerKind::Modified), bar(2, MarkerKind::Added)]
        );
    }

    #[test]
    fn a_hunk_is_staged_when_the_buffer_matches_the_index() {
        assert_eq!(
            specs("a\nb\nc\n", "a\nB\nc\n", Some("a\nB\nc\n")),
            vec![staged_bar(2, MarkerKind::Modified)]
        );
    }

    #[test]
    fn a_hunk_is_unstaged_when_the_index_still_matches_head() {
        assert_eq!(
            specs("a\nb\nc\n", "a\nB\nc\n", Some("a\nb\nc\n")),
            vec![bar(2, MarkerKind::Modified)]
        );
    }

    #[test]
    fn hunks_are_classified_independently() {
        assert_eq!(
            specs(
                "a\nb\nc\nd\ne\n",
                "a\nB\nc\nD\ne\n",
                Some("a\nB\nc\nd\ne\n")
            ),
            vec![
                staged_bar(2, MarkerKind::Modified),
                bar(4, MarkerKind::Modified),
            ]
        );
    }

    #[test]
    fn a_missing_index_baseline_is_unstaged() {
        assert_eq!(
            specs("a\n", "A\n", None),
            vec![bar(1, MarkerKind::Modified)]
        );
    }

    #[test]
    fn contiguous_same_kind_lines_collapse_into_one_run() {
        assert_eq!(
            build_overview_runs(&specs("a\nb\nc\n", "a\nX\nY\nZ\nc\n", None)),
            vec![
                OverviewRun {
                    kind: OverviewKind::Modified,
                    staged: false,
                    first_line: 2,
                    last_line: 2
                },
                OverviewRun {
                    kind: OverviewKind::Added,
                    staged: false,
                    first_line: 3,
                    last_line: 4
                },
            ]
        );
    }

    #[test]
    fn separate_hunks_stay_separate_runs() {
        assert_eq!(
            build_overview_runs(&specs("a\nb\nc\nd\ne\n", "a\nB\nc\nD\ne\n", None)),
            vec![
                OverviewRun {
                    kind: OverviewKind::Modified,
                    staged: false,
                    first_line: 2,
                    last_line: 2
                },
                OverviewRun {
                    kind: OverviewKind::Modified,
                    staged: false,
                    first_line: 4,
                    last_line: 4
                },
            ]
        );
    }

    #[test]
    fn runs_that_differ_in_staged_state_do_not_merge() {
        assert_eq!(
            build_overview_runs(&[
                staged_bar(1, MarkerKind::Modified),
                bar(2, MarkerKind::Modified),
            ]),
            vec![
                OverviewRun {
                    kind: OverviewKind::Modified,
                    staged: true,
                    first_line: 1,
                    last_line: 1
                },
                OverviewRun {
                    kind: OverviewKind::Modified,
                    staged: false,
                    first_line: 2,
                    last_line: 2
                },
            ]
        );
    }

    #[test]
    fn a_pure_deletion_boundary_emits_a_deleted_run() {
        assert_eq!(
            build_overview_runs(&specs("a\nb\nc\n", "a\nc\n", None)),
            vec![OverviewRun {
                kind: OverviewKind::Deleted,
                staged: false,
                first_line: 2,
                last_line: 2
            }]
        );
    }

    #[test]
    fn a_bar_hunk_carrying_a_wedge_stays_one_run() {
        assert_eq!(
            build_overview_runs(&specs("a\nb\nc\nd\n", "a\nX\nd\n", None)),
            vec![OverviewRun {
                kind: OverviewKind::Modified,
                staged: false,
                first_line: 2,
                last_line: 2
            }]
        );
    }

    #[test]
    fn a_clean_document_produces_no_runs() {
        assert_eq!(
            build_overview_runs(&specs("a\nb\n", "a\nb\n", None)),
            vec![]
        );
    }

    #[test]
    fn reverting_a_hunk_restores_the_head_lines() {
        let head = DiffText::new("a\nb\nc\n");
        let doc = DiffText::new("a\nX\nY\nc\n");
        let chunks = build_chunks(&head, &doc);
        assert_eq!(chunks.len(), 1);
        let (range, insert) = chunk_revert_edit(&head, &doc, &chunks[0]);
        let mut reverted = doc.as_str().to_owned();
        reverted.replace_range(range, &insert);
        assert_eq!(reverted, "a\nb\nc\n");
    }

    #[test]
    fn reverting_a_trailing_hunk_restores_the_head_lines() {
        let head = DiffText::new("a\nb\nc");
        let doc = DiffText::new("a\nb\nZ");
        let chunks = build_chunks(&head, &doc);
        assert_eq!(chunks.len(), 1);
        let (range, insert) = chunk_revert_edit(&head, &doc, &chunks[0]);
        let mut reverted = doc.as_str().to_owned();
        reverted.replace_range(range, &insert);
        assert_eq!(reverted, "a\nb\nc");
    }

    #[test]
    fn reverting_a_pure_deletion_puts_the_lines_back() {
        let head = DiffText::new("a\nb\nc\n");
        let doc = DiffText::new("a\nc\n");
        let chunks = build_chunks(&head, &doc);
        assert_eq!(chunks.len(), 1);
        let (range, insert) = chunk_revert_edit(&head, &doc, &chunks[0]);
        let mut reverted = doc.as_str().to_owned();
        reverted.replace_range(range, &insert);
        assert_eq!(reverted, "a\nb\nc\n");
    }

    #[test]
    fn reverting_a_crlf_baseline_inserts_lf() {
        let head = DiffText::new("a\r\nb\r\nc\r\n");
        let doc = DiffText::new("a\nX\nc\n");
        let chunks = build_chunks(&head, &doc);
        assert_eq!(chunks.len(), 1);
        let (range, insert) = chunk_revert_edit(&head, &doc, &chunks[0]);
        let mut reverted = doc.as_str().to_owned();
        reverted.replace_range(range, &insert);
        assert_eq!(reverted, "a\nb\nc\n");
    }

    #[test]
    fn the_peek_body_is_the_head_lines_without_a_trailing_break() {
        let head = DiffText::new("a\nb\nc\nd\n");
        let doc = DiffText::new("a\nX\nY\nd\n");
        let chunks = build_chunks(&head, &doc);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunk_original_text(&head, &chunks[0]), "b\nc");
    }

    #[test]
    fn a_pure_addition_has_an_empty_peek_body() {
        let head = DiffText::new("a\nb\n");
        let doc = DiffText::new("a\nx\nb\n");
        let chunks = build_chunks(&head, &doc);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunk_original_text(&head, &chunks[0]), "");
    }

    #[test]
    fn chunk_lookup_matches_the_lines_the_gutter_marks() {
        let head = DiffText::new("a\nb\nc\nd\ne\n");
        let doc = DiffText::new("a\nB\nc\nD\ne\n");
        let chunks = build_chunks(&head, &doc);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunk_index_at_line(&doc, &chunks, 2), Some(0));
        assert_eq!(chunk_index_at_line(&doc, &chunks, 4), Some(1));
        assert_eq!(chunk_index_at_line(&doc, &chunks, 3), None);
        assert_eq!(chunk_anchor_line(&doc, &chunks[1]), 4);
    }

    #[test]
    fn hunk_navigation_wraps_at_both_ends() {
        let head = DiffText::new("a\nb\nc\nd\ne\n");
        let doc = DiffText::new("a\nB\nc\nD\ne\n");
        let chunks = build_chunks(&head, &doc);
        assert_eq!(goto_chunk_index(&doc, &chunks, 0, true), Some(0));
        assert_eq!(goto_chunk_index(&doc, &chunks, 2, true), Some(1));
        assert_eq!(goto_chunk_index(&doc, &chunks, 6, true), Some(0));
        assert_eq!(goto_chunk_index(&doc, &chunks, 6, false), Some(0));
        assert_eq!(goto_chunk_index(&doc, &chunks, 0, false), Some(1));
        assert_eq!(goto_chunk_index(&doc, &[], 0, true), None);
    }
}
