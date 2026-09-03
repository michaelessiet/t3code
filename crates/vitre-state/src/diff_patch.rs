//! Unified git-patch parsing for the diff panel.
//!
//! `review.getDiffPreview` returns raw `git diff` patch text per source; the
//! Electron app hands that to `@pierre/diffs` for parsing + rendering. This is
//! the parsing half, ported clean-room from the git patch format itself:
//! `diff --git` file sections, extended headers (renames, mode changes,
//! binary), and `@@` hunks with per-line old/new numbering.

/// One file section of a patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchFile {
    /// Path on the base side (`None` for added files).
    pub old_path: Option<String>,
    /// Path on the head side (`None` for deleted files).
    pub new_path: Option<String>,
    pub kind: PatchFileKind,
    /// Binary sections carry no hunks; the UI renders a placeholder row.
    pub binary: bool,
    pub hunks: Vec<PatchHunk>,
    pub additions: usize,
    pub deletions: usize,
}

impl PatchFile {
    /// The path the UI files this diff under: the head-side name when it
    /// exists (added/modified/renamed), otherwise the base-side name.
    pub fn display_path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchFileKind {
    Added,
    Deleted,
    Modified,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// The function-context trailer git appends after the second `@@`.
    pub header: String,
    pub lines: Vec<PatchLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchLineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchLine {
    pub kind: PatchLineKind,
    /// Line content without the leading marker character.
    pub content: String,
    /// Base-side line number (`None` on added lines).
    pub old_line: Option<u32>,
    /// Head-side line number (`None` on deleted lines).
    pub new_line: Option<u32>,
}

/// Parse a full multi-file unified git patch.
///
/// Tolerant by construction: unknown header lines inside a file section are
/// skipped, and a malformed hunk ends the current file rather than poisoning
/// the rest of the patch.
pub fn parse_unified_patch(text: &str) -> Vec<PatchFile> {
    let mut files = Vec::new();
    let mut current: Option<FileBuilder> = None;
    let mut lines = text.lines().peekable();

    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(builder) = current.take() {
                files.push(builder.finish());
            }
            current = Some(FileBuilder::start(rest));
            continue;
        }
        let Some(builder) = current.as_mut() else {
            continue;
        };
        if let Some(header) = line.strip_prefix("@@ ") {
            if let Some(mut hunk) = parse_hunk_header(header) {
                consume_hunk_lines(&mut hunk, &mut lines);
                builder.push_hunk(hunk);
            }
            continue;
        }
        builder.header_line(line);
    }
    if let Some(builder) = current.take() {
        files.push(builder.finish());
    }
    files
}

struct FileBuilder {
    /// Fallback names recovered from the `diff --git a/X b/Y` line.
    git_old: Option<String>,
    git_new: Option<String>,
    old_path: Option<Option<String>>,
    new_path: Option<Option<String>>,
    renamed: bool,
    added: bool,
    deleted: bool,
    binary: bool,
    hunks: Vec<PatchHunk>,
}

impl FileBuilder {
    fn start(git_line_rest: &str) -> Self {
        let (git_old, git_new) = split_git_paths(git_line_rest);
        Self {
            git_old,
            git_new,
            old_path: None,
            new_path: None,
            renamed: false,
            added: false,
            deleted: false,
            binary: false,
            hunks: Vec::new(),
        }
    }

    fn header_line(&mut self, line: &str) {
        if let Some(rest) = line.strip_prefix("--- ") {
            self.old_path = Some(parse_marker_path(rest, "a/"));
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            self.new_path = Some(parse_marker_path(rest, "b/"));
        } else if let Some(rest) = line.strip_prefix("rename from ") {
            self.renamed = true;
            self.old_path = Some(Some(unquote_git_path(rest)));
        } else if let Some(rest) = line.strip_prefix("rename to ") {
            self.renamed = true;
            self.new_path = Some(Some(unquote_git_path(rest)));
        } else if line.starts_with("new file mode ") {
            self.added = true;
        } else if line.starts_with("deleted file mode ") {
            self.deleted = true;
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            self.binary = true;
        }
    }

    fn push_hunk(&mut self, hunk: PatchHunk) {
        self.hunks.push(hunk);
    }

    fn finish(self) -> PatchFile {
        let old_path = match self.old_path {
            Some(path) => path,
            // Header-only sections (mode changes, binary) never carry ---/+++
            // markers; fall back to the `diff --git` names.
            None if self.added => None,
            None => self.git_old,
        };
        let new_path = match self.new_path {
            Some(path) => path,
            None if self.deleted => None,
            None => self.git_new,
        };
        let kind = if self.renamed {
            PatchFileKind::Renamed
        } else if self.added || old_path.is_none() {
            PatchFileKind::Added
        } else if self.deleted || new_path.is_none() {
            PatchFileKind::Deleted
        } else {
            PatchFileKind::Modified
        };
        let additions = self
            .hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.kind == PatchLineKind::Add)
            .count();
        let deletions = self
            .hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.kind == PatchLineKind::Del)
            .count();
        PatchFile {
            old_path,
            new_path,
            kind,
            binary: self.binary,
            hunks: self.hunks,
            additions,
            deletions,
        }
    }
}

/// `-start[,count] +start[,count] @@[ trailer]`, the `@@ ` prefix stripped.
fn parse_hunk_header(rest: &str) -> Option<PatchHunk> {
    let close = rest.find(" @@")?;
    let (ranges, trailer) = rest.split_at(close);
    let trailer = trailer
        .strip_prefix(" @@")
        .unwrap_or("")
        .strip_prefix(' ')
        .unwrap_or("")
        .to_string();
    let mut parts = ranges.split_whitespace();
    let (old_start, old_lines) = parse_hunk_range(parts.next()?, '-')?;
    let (new_start, new_lines) = parse_hunk_range(parts.next()?, '+')?;
    Some(PatchHunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        header: trailer,
        lines: Vec::new(),
    })
}

fn parse_hunk_range(token: &str, sign: char) -> Option<(u32, u32)> {
    let token = token.strip_prefix(sign)?;
    match token.split_once(',') {
        Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
        None => Some((token.parse().ok()?, 1)),
    }
}

fn consume_hunk_lines(hunk: &mut PatchHunk, lines: &mut std::iter::Peekable<std::str::Lines<'_>>) {
    let mut old_line = hunk.old_start;
    let mut new_line = hunk.new_start;
    let mut old_remaining = hunk.old_lines;
    let mut new_remaining = hunk.new_lines;
    while old_remaining > 0 || new_remaining > 0 {
        // `\ No newline at end of file` annotates the previous line; it does
        // not consume hunk budget.
        let Some(line) = lines.peek() else {
            break;
        };
        if let Some(content) = line.strip_prefix('\\') {
            let _ = content;
            lines.next();
            continue;
        }
        let (kind, content) = match line.as_bytes().first() {
            Some(b' ') => (PatchLineKind::Context, &line[1..]),
            Some(b'+') => (PatchLineKind::Add, &line[1..]),
            Some(b'-') => (PatchLineKind::Del, &line[1..]),
            // Some tools emit entirely-empty lines for empty context lines.
            None => (PatchLineKind::Context, ""),
            _ => break,
        };
        lines.next();
        let (old, new) = match kind {
            PatchLineKind::Context => {
                let pair = (Some(old_line), Some(new_line));
                old_line += 1;
                new_line += 1;
                old_remaining = old_remaining.saturating_sub(1);
                new_remaining = new_remaining.saturating_sub(1);
                pair
            }
            PatchLineKind::Del => {
                let pair = (Some(old_line), None);
                old_line += 1;
                old_remaining = old_remaining.saturating_sub(1);
                pair
            }
            PatchLineKind::Add => {
                let pair = (None, Some(new_line));
                new_line += 1;
                new_remaining = new_remaining.saturating_sub(1);
                pair
            }
        };
        hunk.lines.push(PatchLine {
            kind,
            content: content.to_string(),
            old_line: old,
            new_line: new,
        });
    }
    // Trailing no-newline marker after the final hunk line.
    if lines.peek().is_some_and(|line| line.starts_with('\\')) {
        lines.next();
    }
}

/// Split the `a/X b/Y` tail of a `diff --git` line into the two paths.
///
/// Quoted paths (spaces, unicode escapes) are handled; the unquoted ambiguous
/// case (paths themselves containing " b/") resolves the way git renders it —
/// the last ` b/` occurrence splits the sides.
fn split_git_paths(rest: &str) -> (Option<String>, Option<String>) {
    let rest = rest.trim();
    if rest.starts_with('"') {
        let (first, remainder) = take_quoted(rest);
        let second = remainder.trim();
        let second = if second.starts_with('"') {
            take_quoted(second).0
        } else {
            second.to_string()
        };
        return (strip_side(&first, "a/"), strip_side(&second, "b/"));
    }
    match rest.rfind(" b/") {
        Some(split) => {
            let (first, second) = rest.split_at(split);
            (
                strip_side(first, "a/"),
                strip_side(second.trim_start(), "b/"),
            )
        }
        None => (None, None),
    }
}

fn strip_side(token: &str, prefix: &str) -> Option<String> {
    Some(token.trim().strip_prefix(prefix)?.to_string())
}

/// A `---`/`+++` marker path: `/dev/null`, or an `a/`-`b/`-prefixed
/// (possibly quoted) path, with any `\t`-separated timestamp dropped.
fn parse_marker_path(rest: &str, prefix: &str) -> Option<String> {
    let rest = rest.split('\t').next().unwrap_or(rest).trim();
    if rest == "/dev/null" {
        return None;
    }
    let unquoted = unquote_git_path(rest);
    Some(
        unquoted
            .strip_prefix(prefix)
            .map(str::to_string)
            .unwrap_or(unquoted),
    )
}

fn take_quoted(rest: &str) -> (String, &str) {
    debug_assert!(rest.starts_with('"'));
    let mut out = String::new();
    let mut chars = rest[1..].char_indices();
    while let Some((idx, ch)) = chars.next() {
        match ch {
            '"' => return (out, &rest[1 + idx + 1..]),
            '\\' => {
                if let Some((_, escaped)) = chars.next() {
                    out.push(unescape_git_char(escaped));
                }
            }
            _ => out.push(ch),
        }
    }
    (out, "")
}

fn unquote_git_path(path: &str) -> String {
    if path.starts_with('"') {
        take_quoted(path).0
    } else {
        path.to_string()
    }
}

fn unescape_git_char(ch: char) -> char {
    match ch {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        _ => ch,
    }
}

/// Byte ranges (into `PatchLine::content`) that changed between a paired
/// deleted/added line — the "intra-line" highlight a diff renderer paints on
/// top of the row background.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntralineRanges {
    pub del: Vec<std::ops::Range<usize>>,
    pub add: Vec<std::ops::Range<usize>>,
}

/// A del line index, add line index (both into `hunk.lines`), and their
/// changed ranges.
pub type IntralinePair = (usize, usize, IntralineRanges);

/// Pair up del/add runs inside a hunk and compute word-level change ranges.
///
/// Git emits changed regions as a run of `-` lines followed by a run of `+`
/// lines; the k-th deletion pairs with the k-th addition. Lines whose pair is
/// missing (unbalanced runs) get no intra-line marks, as do pairs with no
/// common token — a fully-rewritten line reads better as a solid row than as
/// one giant highlight.
pub fn hunk_intraline_pairs(hunk: &PatchHunk) -> Vec<IntralinePair> {
    let mut pairs = Vec::new();
    let mut idx = 0;
    while idx < hunk.lines.len() {
        if hunk.lines[idx].kind != PatchLineKind::Del {
            idx += 1;
            continue;
        }
        let del_start = idx;
        while idx < hunk.lines.len() && hunk.lines[idx].kind == PatchLineKind::Del {
            idx += 1;
        }
        let add_start = idx;
        while idx < hunk.lines.len() && hunk.lines[idx].kind == PatchLineKind::Add {
            idx += 1;
        }
        let dels = add_start - del_start;
        let adds = idx - add_start;
        for offset in 0..dels.min(adds) {
            let del_idx = del_start + offset;
            let add_idx = add_start + offset;
            if let Some(ranges) =
                intraline_ranges(&hunk.lines[del_idx].content, &hunk.lines[add_idx].content)
            {
                pairs.push((del_idx, add_idx, ranges));
            }
        }
    }
    pairs
}

/// Token budget past which intra-line diffing is skipped (quadratic LCS).
const INTRALINE_MAX_TOKENS: usize = 200;

/// Word-level changed ranges between two lines, or `None` when highlighting
/// would not help (identical lines, oversized lines, or nothing in common).
fn intraline_ranges(del: &str, add: &str) -> Option<IntralineRanges> {
    if del == add {
        return None;
    }
    let del_tokens = tokenize_line(del);
    let add_tokens = tokenize_line(add);
    if del_tokens.len() > INTRALINE_MAX_TOKENS || add_tokens.len() > INTRALINE_MAX_TOKENS {
        return None;
    }
    let common = token_lcs(&del_tokens, &add_tokens, del, add);
    // No shared non-whitespace token: treat as a rewrite, not an edit.
    if !common
        .iter()
        .any(|&(d, _)| !del[del_tokens[d].clone()].trim().is_empty())
    {
        return None;
    }
    let mut in_del = vec![false; del_tokens.len()];
    let mut in_add = vec![false; add_tokens.len()];
    for &(d, a) in &common {
        in_del[d] = true;
        in_add[a] = true;
    }
    Some(IntralineRanges {
        del: changed_token_ranges(&del_tokens, &in_del),
        add: changed_token_ranges(&add_tokens, &in_add),
    })
}

/// Word / single-symbol / whitespace-run tokens as byte ranges.
fn tokenize_line(line: &str) -> Vec<std::ops::Range<usize>> {
    let mut tokens = Vec::new();
    let mut iter = line.char_indices().peekable();
    while let Some((start, ch)) = iter.next() {
        let class = char_class(ch);
        let end = loop {
            match iter.peek() {
                // Words and whitespace agglomerate; symbols stay single chars.
                Some(&(_, next_ch)) if class != 2 && char_class(next_ch) == class => {
                    iter.next();
                }
                Some(&(next_idx, _)) => break next_idx,
                None => break line.len(),
            }
        };
        tokens.push(start..end);
    }
    tokens
}

/// 0 = word (alphanumeric/underscore), 1 = whitespace, 2 = single symbol.
fn char_class(ch: char) -> u8 {
    if ch.is_alphanumeric() || ch == '_' {
        0
    } else if ch.is_whitespace() {
        1
    } else {
        2
    }
}

/// Classic O(n·m) LCS over token text; input sizes are single lines.
fn token_lcs(
    del_tokens: &[std::ops::Range<usize>],
    add_tokens: &[std::ops::Range<usize>],
    del: &str,
    add: &str,
) -> Vec<(usize, usize)> {
    let n = del_tokens.len();
    let m = add_tokens.len();
    let mut table = vec![0u16; (n + 1) * (m + 1)];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let idx = i * (m + 1) + j;
            table[idx] = if del[del_tokens[i].clone()] == add[add_tokens[j].clone()] {
                table[(i + 1) * (m + 1) + j + 1] + 1
            } else {
                table[(i + 1) * (m + 1) + j].max(table[i * (m + 1) + j + 1])
            };
        }
    }
    let mut common = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if del[del_tokens[i].clone()] == add[add_tokens[j].clone()] {
            common.push((i, j));
            i += 1;
            j += 1;
        } else if table[(i + 1) * (m + 1) + j] >= table[i * (m + 1) + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    common
}

/// Merge consecutive non-common tokens into contiguous byte ranges.
fn changed_token_ranges(
    tokens: &[std::ops::Range<usize>],
    common: &[bool],
) -> Vec<std::ops::Range<usize>> {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    for (token, &keep) in tokens.iter().zip(common) {
        if keep {
            continue;
        }
        match ranges.last_mut() {
            Some(last) if last.end == token.start => last.end = token.end,
            _ => ranges.push(token.clone()),
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE: &str = "\
diff --git a/src/main.rs b/src/main.rs
index 1111111..2222222 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,5 @@ fn main
 use std::fmt;
-let a = 1;
+let a = 2;
+let b = 3;
 println!();
";

    #[test]
    fn parses_a_modified_file() {
        let files = parse_unified_patch(SIMPLE);
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.display_path(), "src/main.rs");
        assert_eq!(file.kind, PatchFileKind::Modified);
        assert_eq!((file.additions, file.deletions), (2, 1));
        let hunk = &file.hunks[0];
        assert_eq!(hunk.header, "fn main");
        assert_eq!(
            (
                hunk.old_start,
                hunk.old_lines,
                hunk.new_start,
                hunk.new_lines
            ),
            (1, 4, 1, 5)
        );
        assert_eq!(hunk.lines.len(), 5);
        assert_eq!(hunk.lines[0].kind, PatchLineKind::Context);
        assert_eq!(hunk.lines[0].old_line, Some(1));
        assert_eq!(hunk.lines[0].new_line, Some(1));
        assert_eq!(hunk.lines[1].kind, PatchLineKind::Del);
        assert_eq!(hunk.lines[1].old_line, Some(2));
        assert_eq!(hunk.lines[1].new_line, None);
        assert_eq!(hunk.lines[2].kind, PatchLineKind::Add);
        assert_eq!(hunk.lines[2].new_line, Some(2));
        assert_eq!(hunk.lines[3].kind, PatchLineKind::Add);
        assert_eq!(hunk.lines[3].new_line, Some(3));
        assert_eq!(hunk.lines[4].kind, PatchLineKind::Context);
        assert_eq!(hunk.lines[4].old_line, Some(3));
        assert_eq!(hunk.lines[4].new_line, Some(4));
    }

    #[test]
    fn parses_added_and_deleted_files() {
        let patch = "\
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..e69de29
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+hello
+world
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index e69de29..0000000
--- a/gone.txt
+++ /dev/null
@@ -1,1 +0,0 @@
-bye
";
        let files = parse_unified_patch(patch);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].kind, PatchFileKind::Added);
        assert_eq!(files[0].old_path, None);
        assert_eq!(files[0].display_path(), "new.txt");
        assert_eq!(files[0].additions, 2);
        assert_eq!(files[0].hunks[0].lines[0].new_line, Some(1));
        assert_eq!(files[1].kind, PatchFileKind::Deleted);
        assert_eq!(files[1].new_path, None);
        assert_eq!(files[1].display_path(), "gone.txt");
        assert_eq!(files[1].deletions, 1);
    }

    #[test]
    fn parses_renames_and_binary() {
        let patch = "\
diff --git a/old name.txt b/new name.txt
similarity index 90%
rename from old name.txt
rename to new name.txt
diff --git a/img.png b/img.png
index 1111111..2222222 100644
Binary files a/img.png and b/img.png differ
";
        let files = parse_unified_patch(patch);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].kind, PatchFileKind::Renamed);
        assert_eq!(files[0].old_path.as_deref(), Some("old name.txt"));
        assert_eq!(files[0].new_path.as_deref(), Some("new name.txt"));
        assert!(files[1].binary);
        assert_eq!(files[1].kind, PatchFileKind::Modified);
        assert_eq!(files[1].display_path(), "img.png");
    }

    #[test]
    fn handles_quoted_paths_and_no_newline() {
        let patch = "\
diff --git \"a/sp ace.txt\" \"b/sp ace.txt\"
index 1111111..2222222 100644
--- \"a/sp ace.txt\"
+++ \"b/sp ace.txt\"
@@ -1 +1 @@
-old
\\ No newline at end of file
+new
\\ No newline at end of file
";
        let files = parse_unified_patch(patch);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].display_path(), "sp ace.txt");
        let lines = &files[0].hunks[0].lines;
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].kind, PatchLineKind::Del);
        assert_eq!(lines[1].kind, PatchLineKind::Add);
        assert_eq!(lines[1].content, "new");
    }

    #[test]
    fn empty_and_garbage_input_yield_nothing() {
        assert!(parse_unified_patch("").is_empty());
        assert!(parse_unified_patch("not a patch\nat all\n").is_empty());
    }

    #[test]
    fn intraline_marks_the_changed_word() {
        let files = parse_unified_patch(SIMPLE);
        let hunk = &files[0].hunks[0];
        let pairs = hunk_intraline_pairs(hunk);
        // `let a = 1;` → `let a = 2;` pairs; the extra `let b = 3;` is
        // unbalanced and gets no pair.
        assert_eq!(pairs.len(), 1);
        let (del_idx, add_idx, ranges) = &pairs[0];
        assert_eq!(*del_idx, 1);
        assert_eq!(*add_idx, 2);
        let del_line = &hunk.lines[1].content;
        let add_line = &hunk.lines[2].content;
        assert_eq!(&del_line[ranges.del[0].clone()], "1");
        assert_eq!(&add_line[ranges.add[0].clone()], "2");
    }

    #[test]
    fn intraline_skips_full_rewrites() {
        assert_eq!(intraline_ranges("alpha beta", "gamma delta"), None);
        assert_eq!(intraline_ranges("same", "same"), None);
        // A shared token keeps highlighting on.
        let ranges = intraline_ranges("foo(bar)", "foo(baz)").unwrap();
        assert_eq!(ranges.del.len(), 1);
        assert_eq!(ranges.add.len(), 1);
    }

    #[test]
    fn intraline_merges_adjacent_changed_tokens() {
        let ranges = intraline_ranges("value.method()", "other.thing()").unwrap();
        // "value" + "." unchanged-boundary check: "." and "()" are common, the
        // identifiers differ; ranges stay per-identifier (non-adjacent).
        assert_eq!(ranges.del.len(), 2);
        assert_eq!(ranges.add.len(), 2);
    }

    #[test]
    fn multiple_hunks_number_correctly() {
        let patch = "\
diff --git a/f.txt b/f.txt
index 1111111..2222222 100644
--- a/f.txt
+++ b/f.txt
@@ -1,2 +1,2 @@
 one
-two
+TWO
@@ -10,2 +10,3 @@
 ten
+ten-and-a-half
 eleven
";
        let files = parse_unified_patch(patch);
        let hunks = &files[0].hunks;
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[1].lines[0].old_line, Some(10));
        assert_eq!(hunks[1].lines[1].new_line, Some(11));
        assert_eq!(hunks[1].lines[2].old_line, Some(11));
        assert_eq!(hunks[1].lines[2].new_line, Some(12));
    }
}
