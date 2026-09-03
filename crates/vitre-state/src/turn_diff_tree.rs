//! Changed-files tree + presentation logic for the chat turn-diff card.
//!
//! Ports `apps/web/src/lib/turnDiffTree.ts`,
//! `apps/web/src/components/chat/changedFilesPresentation.ts`, the compact
//! count formatting from `DiffStatLabel.tsx`, and the auto-expand rule from
//! `ChangedFilesTree.tsx`. Pure functions over the checkpoint summary's file
//! list — no I/O, no gpui.

use vitre_contracts::OrchestrationCheckpointFile;

/// `CHANGED_FILES_AUTO_EXPAND_FILE_LIMIT`.
pub const AUTO_EXPAND_FILE_LIMIT: usize = 5;
/// `CHANGED_FILES_AUTO_EXPAND_LINE_LIMIT`.
pub const AUTO_EXPAND_LINE_LIMIT: i64 = 200;
/// `CHANGED_FILES_PREVIEW_SCOPE_LIMIT`.
pub const PREVIEW_SCOPE_LIMIT: usize = 4;
/// `CHANGED_FILES_PREVIEW_FILE_LIMIT`.
pub const PREVIEW_FILE_LIMIT: usize = 3;

/// Summed additions/deletions. Electron models a per-file stat as nullable
/// for legacy servers; this wire's contract makes both counts required, so
/// the stat is always present (and file rows may legitimately show `+0 −0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TurnDiffStat {
    pub additions: i64,
    pub deletions: i64,
}

impl TurnDiffStat {
    pub fn is_zero(&self) -> bool {
        self.additions == 0 && self.deletions == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnDiffFileNode {
    /// Last normalized path segment.
    pub name: String,
    /// Normalized path (backslashes → slashes, empty segments dropped).
    pub path: String,
    pub stat: TurnDiffStat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnDiffDirNode {
    /// Display name; chain compaction folds single-directory chains into
    /// `parent/child` names.
    pub name: String,
    pub path: String,
    /// Accumulated over all descendant files.
    pub stat: TurnDiffStat,
    pub children: Vec<TurnDiffNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnDiffNode {
    Dir(TurnDiffDirNode),
    File(TurnDiffFileNode),
}

/// `localeCompare(undefined, { numeric: true, sensitivity: "base" })`:
/// case-insensitive with digit runs compared by numeric value
/// (`file2 < file10`).
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut a_chars = a.chars().flat_map(char::to_lowercase).peekable();
    let mut b_chars = b.chars().flat_map(char::to_lowercase).peekable();
    loop {
        match (a_chars.peek().copied(), b_chars.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut run_a = String::new();
                while let Some(c) = a_chars.peek().copied().filter(char::is_ascii_digit) {
                    run_a.push(c);
                    a_chars.next();
                }
                let mut run_b = String::new();
                while let Some(c) = b_chars.peek().copied().filter(char::is_ascii_digit) {
                    run_b.push(c);
                    b_chars.next();
                }
                // Compare by value without parsing: strip leading zeros,
                // longer run wins, then lexical on equal length.
                let trim_a = run_a.trim_start_matches('0');
                let trim_b = run_b.trim_start_matches('0');
                let by_value = trim_a
                    .len()
                    .cmp(&trim_b.len())
                    .then_with(|| trim_a.cmp(trim_b));
                if by_value != Ordering::Equal {
                    return by_value;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a_chars.next();
                b_chars.next();
            }
        }
    }
}

/// Normalized path segments: backslashes to slashes, empty segments dropped.
fn segments(path: &str) -> Vec<String> {
    path.replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

fn file_stat(file: &OrchestrationCheckpointFile) -> TurnDiffStat {
    TurnDiffStat {
        additions: file.additions.0,
        deletions: file.deletions.0,
    }
}

/// `summarizeTurnDiffStats`: totals over the summary's files.
pub fn summarize_turn_diff_stats(files: &[OrchestrationCheckpointFile]) -> TurnDiffStat {
    files.iter().fold(TurnDiffStat::default(), |acc, file| {
        let stat = file_stat(file);
        TurnDiffStat {
            additions: acc.additions + stat.additions,
            deletions: acc.deletions + stat.deletions,
        }
    })
}

/// `shouldAutoExpandChangedFiles`: latest turn, few files, small change.
pub fn should_auto_expand_changed_files(
    files: &[OrchestrationCheckpointFile],
    is_latest_turn: bool,
) -> bool {
    if !is_latest_turn || files.len() > AUTO_EXPAND_FILE_LIMIT {
        return false;
    }
    let totals = summarize_turn_diff_stats(files);
    totals.additions + totals.deletions <= AUTO_EXPAND_LINE_LIMIT
}

/// Intermediate mutable tree used during construction.
#[derive(Default)]
struct Builder {
    stat: TurnDiffStat,
    dirs: Vec<(String, Builder)>,
    files: Vec<TurnDiffFileNode>,
}

impl Builder {
    fn child_dir(&mut self, name: &str) -> &mut Builder {
        if let Some(index) = self.dirs.iter().position(|(n, _)| n == name) {
            &mut self.dirs[index].1
        } else {
            self.dirs.push((name.to_string(), Builder::default()));
            &mut self.dirs.last_mut().unwrap().1
        }
    }
}

/// `buildTurnDiffTree`: normalize, group into directories, accumulate stats,
/// sort each level directories-first with the natural comparator, then fold
/// single-directory chains (`a` → `a/b`). Returns the root's children plus
/// the root totals.
pub fn build_turn_diff_tree(
    files: &[OrchestrationCheckpointFile],
) -> (Vec<TurnDiffNode>, TurnDiffStat) {
    let mut root = Builder::default();
    for file in files {
        let segments = segments(&file.path.0);
        let Some((name, dirs)) = segments.split_last() else {
            continue;
        };
        let stat = file_stat(file);
        root.stat.additions += stat.additions;
        root.stat.deletions += stat.deletions;
        let mut node = &mut root;
        for dir in dirs {
            node = node.child_dir(dir);
            node.stat.additions += stat.additions;
            node.stat.deletions += stat.deletions;
        }
        node.files.push(TurnDiffFileNode {
            name: name.clone(),
            path: segments.join("/"),
            stat,
        });
    }

    fn finish(name: String, path_prefix: &str, builder: Builder) -> TurnDiffDirNode {
        let path = if path_prefix.is_empty() {
            name.clone()
        } else {
            format!("{path_prefix}/{name}")
        };
        let mut children: Vec<TurnDiffNode> = Vec::new();
        let mut dirs: Vec<(String, Builder)> = builder.dirs;
        dirs.sort_by(|a, b| natural_cmp(&a.0, &b.0));
        for (child_name, child) in dirs {
            children.push(TurnDiffNode::Dir(compact(finish(child_name, &path, child))));
        }
        let mut files = builder.files;
        files.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        children.extend(files.into_iter().map(TurnDiffNode::File));
        TurnDiffDirNode {
            name,
            path,
            stat: builder.stat,
            children,
        }
    }

    /// Fold `dir` with its only child while that child is a lone directory.
    fn compact(mut dir: TurnDiffDirNode) -> TurnDiffDirNode {
        while dir.children.len() == 1 {
            let only_dir = matches!(dir.children[0], TurnDiffNode::Dir(_));
            if !only_dir {
                break;
            }
            let TurnDiffNode::Dir(child) = dir.children.remove(0) else {
                unreachable!()
            };
            dir.name = format!("{}/{}", dir.name, child.name);
            dir.path = child.path;
            dir.stat = child.stat;
            dir.children = child.children;
        }
        dir
    }

    let stat = root.stat;
    let root = finish(String::new(), "", root);
    // The root's own children are never folded into it.
    (root.children, stat)
}

/// Pre-order directory paths, feeding the expansion-state key.
pub fn collect_directory_paths(nodes: &[TurnDiffNode]) -> Vec<String> {
    let mut paths = Vec::new();
    fn walk(nodes: &[TurnDiffNode], paths: &mut Vec<String>) {
        for node in nodes {
            if let TurnDiffNode::Dir(dir) = node {
                paths.push(dir.path.clone());
                walk(&dir.children, paths);
            }
        }
    }
    walk(nodes, &mut paths);
    paths
}

/// The key under which manual per-directory expansion overrides stay valid:
/// any toggle-all press or change to the directory set discards them.
pub fn expansion_state_key(all_expanded: bool, nodes: &[TurnDiffNode]) -> String {
    let state = if all_expanded {
        "expanded"
    } else {
        "collapsed"
    };
    format!(
        "{state}\u{0}{}",
        collect_directory_paths(nodes).join("\u{0}")
    )
}

/// `changedFileScope`: first segment of a multi-segment path, else `"root"`.
pub fn changed_file_scope(path: &str) -> String {
    let segments = segments(path);
    if segments.len() > 1 {
        segments[0].clone()
    } else {
        "root".to_string()
    }
}

/// `changedFileName`: last normalized segment, falling back to the raw path.
pub fn changed_file_name(path: &str) -> String {
    segments(path)
        .last()
        .cloned()
        .unwrap_or_else(|| path.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFileScopeSummary {
    pub label: String,
    pub file_count: usize,
}

/// `summarizeChangedFileScopes`: per-scope file counts, ordered by count
/// descending → first appearance → label, truncated to the scope limit.
pub fn summarize_changed_file_scopes(
    files: &[OrchestrationCheckpointFile],
) -> Vec<ChangedFileScopeSummary> {
    let mut order: Vec<(String, usize, usize)> = Vec::new(); // label, count, first index
    for (index, file) in files.iter().enumerate() {
        let scope = changed_file_scope(&file.path.0);
        if let Some(entry) = order.iter_mut().find(|(label, ..)| *label == scope) {
            entry.1 += 1;
        } else {
            order.push((scope, 1, index));
        }
    }
    order.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    order
        .into_iter()
        .take(PREVIEW_SCOPE_LIMIT)
        .map(|(label, file_count, _)| ChangedFileScopeSummary { label, file_count })
        .collect()
}

/// `selectChangedFilePreview`: pass one takes the first file of each distinct
/// scope in file order; pass two backfills with remaining files. Returns
/// indices into `files` so callers keep the raw (un-normalized) paths.
pub fn select_changed_file_preview(files: &[OrchestrationCheckpointFile]) -> Vec<usize> {
    let mut selected: Vec<usize> = Vec::new();
    let mut seen_scopes: Vec<String> = Vec::new();
    for (index, file) in files.iter().enumerate() {
        if selected.len() >= PREVIEW_FILE_LIMIT {
            return selected;
        }
        let scope = changed_file_scope(&file.path.0);
        if !seen_scopes.contains(&scope) {
            seen_scopes.push(scope);
            selected.push(index);
        }
    }
    for index in 0..files.len() {
        if selected.len() >= PREVIEW_FILE_LIMIT {
            break;
        }
        if !selected.contains(&index) {
            selected.push(index);
        }
    }
    selected
}

/// `DiffStatLabel`'s compact count: `<1000` as-is; then `Xk`/`Xm`/`Xb` with
/// one decimal below 10 (trailing `.0` stripped), rounded above.
pub fn format_compact_count(value: i64) -> String {
    fn scaled(value: i64, unit: i64, suffix: &str) -> String {
        let scaled = value as f64 / unit as f64;
        if scaled < 10.0 {
            let text = format!("{scaled:.1}");
            let text = text.strip_suffix(".0").unwrap_or(&text);
            format!("{text}{suffix}")
        } else {
            format!("{}{suffix}", scaled.round() as i64)
        }
    }
    if value < 1_000 {
        value.to_string()
    } else if value < 1_000_000 {
        scaled(value, 1_000, "k")
    } else if value < 1_000_000_000 {
        scaled(value, 1_000_000, "m")
    } else {
        scaled(value, 1_000_000_000, "b")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitre_contracts::{NonNegativeInt, TrimmedNonEmptyString};

    fn file(path: &str, additions: i64, deletions: i64) -> OrchestrationCheckpointFile {
        OrchestrationCheckpointFile {
            additions: NonNegativeInt(additions),
            deletions: NonNegativeInt(deletions),
            kind: TrimmedNonEmptyString("modified".into()),
            path: TrimmedNonEmptyString(path.into()),
        }
    }

    #[test]
    fn natural_cmp_is_numeric_and_case_insensitive() {
        use std::cmp::Ordering;
        assert_eq!(natural_cmp("file2", "file10"), Ordering::Less);
        assert_eq!(natural_cmp("File2", "file2"), Ordering::Equal);
        assert_eq!(natural_cmp("abc", "abd"), Ordering::Less);
        assert_eq!(natural_cmp("a10b2", "a10b10"), Ordering::Less);
    }

    #[test]
    fn tree_sorts_dirs_first_accumulates_stats_and_compacts_chains() {
        let files = [
            file("src/deep/only/child.rs", 3, 1),
            file("readme.md", 1, 0),
            file("src\\lib.rs", 2, 2),
        ];
        let (nodes, totals) = build_turn_diff_tree(&files);
        assert_eq!(
            totals,
            TurnDiffStat {
                additions: 6,
                deletions: 3
            }
        );
        // Directory before the root file.
        assert_eq!(nodes.len(), 2);
        let TurnDiffNode::Dir(src) = &nodes[0] else {
            panic!("expected src dir first");
        };
        assert_eq!(src.name, "src");
        assert_eq!(
            src.stat,
            TurnDiffStat {
                additions: 5,
                deletions: 3
            }
        );
        // `deep/only` chain folds into one row; the backslash path normalized.
        let TurnDiffNode::Dir(chain) = &src.children[0] else {
            panic!("expected compacted chain");
        };
        assert_eq!(chain.name, "deep/only");
        assert_eq!(chain.path, "src/deep/only");
        let TurnDiffNode::File(lib) = &src.children[1] else {
            panic!("expected lib.rs");
        };
        assert_eq!(lib.path, "src/lib.rs");
        let TurnDiffNode::File(readme) = &nodes[1] else {
            panic!("expected root file last");
        };
        assert_eq!(readme.name, "readme.md");
    }

    #[test]
    fn preview_prefers_distinct_scopes_then_backfills() {
        let files = [
            file("web/a.ts", 1, 0),
            file("web/b.ts", 1, 0),
            file("server/c.ts", 1, 0),
            file("readme.md", 1, 0),
        ];
        // Scopes in order: web, server, root — one file each, then done.
        assert_eq!(select_changed_file_preview(&files), vec![0, 2, 3]);
        let scopes = summarize_changed_file_scopes(&files);
        assert_eq!(scopes[0].label, "web");
        assert_eq!(scopes[0].file_count, 2);
        assert_eq!(scopes[1].label, "server");
        assert_eq!(scopes[2].label, "root");
    }

    #[test]
    fn auto_expand_honors_file_and_line_limits() {
        let small = [file("a.rs", 10, 5)];
        assert!(should_auto_expand_changed_files(&small, true));
        assert!(!should_auto_expand_changed_files(&small, false));
        let big = [file("a.rs", 150, 51)];
        assert!(!should_auto_expand_changed_files(&big, true));
        let many: Vec<_> = (0..6).map(|i| file(&format!("f{i}.rs"), 1, 0)).collect();
        assert!(!should_auto_expand_changed_files(&many, true));
    }

    #[test]
    fn compact_counts_follow_diff_stat_label_rules() {
        assert_eq!(format_compact_count(999), "999");
        assert_eq!(format_compact_count(1_234), "1.2k");
        assert_eq!(format_compact_count(9_999), "10k");
        assert_eq!(format_compact_count(12_345), "12k");
        assert_eq!(format_compact_count(2_500_000), "2.5m");
    }
}
