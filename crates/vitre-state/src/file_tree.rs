//! File-tree projection: flat `projects.listEntries` paths → renderable rows.
//!
//! Port of the tree model the Electron app gets from @pierre/trees as
//! configured in `apps/web/src/components/files/FileBrowserPanel.tsx`
//! (`flattenEmptyDirectories: true`, `initialExpansion: "closed"`, directory
//! paths marked with a trailing slash). This is the pure model only: the
//! caller owns the expanded set and passes it into [`FileTreeModel::visible_rows`].

use std::collections::{HashMap, HashSet};

use vitre_contracts::{ProjectEntry, ProjectEntryKind};

/// One rendered line of the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct FileTreeRow {
    /// Workspace-relative path of this node (for flattened chains, the
    /// DEEPEST directory's path).
    pub path: String,
    /// Name to render: last path segment, or the joined chain ("a/b/c") for
    /// flattened empty-directory chains.
    pub display_name: String,
    /// Nesting depth in the rendered tree (root entries = 0; flattened chains
    /// count as ONE level).
    pub depth: usize,
    pub is_dir: bool,
    pub ignored: bool,
    pub expanded: bool,
}

#[derive(Debug, Clone)]
struct Node {
    /// Last path segment.
    name: String,
    /// Full workspace-relative path, no trailing slash.
    path: String,
    is_dir: bool,
    ignored: bool,
    parent: Option<usize>,
    /// Sorted at build time: directories first, then case-insensitive name,
    /// ties case-sensitive.
    children: Vec<usize>,
}

/// Immutable tree built from one `projects.listEntries` result.
#[derive(Debug, Clone)]
pub struct FileTreeModel {
    nodes: Vec<Node>,
    /// Sorted like any sibling list.
    roots: Vec<usize>,
    index: HashMap<String, usize>,
}

/// listEntries paths are '/'-separated with no leading './' or trailing
/// slash; normalization is defensive only.
fn normalize(path: &str) -> &str {
    let path = path.strip_prefix("./").unwrap_or(path);
    path.strip_suffix('/').unwrap_or(path)
}

/// Directories before files, then case-insensitive alphabetical, ties broken
/// case-sensitively. Flattened chains sort by their top segment's name (the
/// chain is a single child node of its parent in the underlying tree).
fn cmp_siblings(a: &Node, b: &Node) -> std::cmp::Ordering {
    b.is_dir
        .cmp(&a.is_dir)
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.name.cmp(&b.name))
}

/// Inserts `path` (and any missing ancestors, synthesized as directories) and
/// returns its node index. New nodes start as non-ignored files; kind and
/// ignored evidence is OR-ed in afterwards so input order never matters.
fn ensure_path(nodes: &mut Vec<Node>, index: &mut HashMap<String, usize>, path: &str) -> usize {
    if let Some(&idx) = index.get(path) {
        return idx;
    }
    let (parent, name) = match path.rsplit_once('/') {
        Some((parent_path, name)) => {
            let parent_idx = ensure_path(nodes, index, parent_path);
            // Anything with a descendant is a directory, even if it was first
            // seen as (or explicitly listed as) a file.
            nodes[parent_idx].is_dir = true;
            (Some(parent_idx), name)
        }
        None => (None, path),
    };
    let idx = nodes.len();
    nodes.push(Node {
        name: name.to_string(),
        path: path.to_string(),
        is_dir: false,
        ignored: false,
        parent,
        children: Vec::new(),
    });
    index.insert(path.to_string(), idx);
    idx
}

impl FileTreeModel {
    /// Build from the flat listEntries result. Order of input entries is not
    /// guaranteed; missing ancestor directories are synthesized. Unknown
    /// entry kinds count as files.
    pub fn build(entries: &[ProjectEntry]) -> Self {
        let mut nodes: Vec<Node> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for entry in entries {
            let path = normalize(&entry.path.0);
            if path.is_empty() {
                continue;
            }
            let idx = ensure_path(&mut nodes, &mut index, path);
            if entry.kind == ProjectEntryKind::Directory {
                nodes[idx].is_dir = true;
            }
            if entry.ignored == Some(Some(true)) {
                nodes[idx].ignored = true;
            }
        }

        let mut children: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
        let mut roots: Vec<usize> = Vec::new();
        for (idx, node) in nodes.iter().enumerate() {
            match node.parent {
                Some(parent) => children[parent].push(idx),
                None => roots.push(idx),
            }
        }
        roots.sort_by(|&a, &b| cmp_siblings(&nodes[a], &nodes[b]));
        for list in &mut children {
            list.sort_by(|&a, &b| cmp_siblings(&nodes[a], &nodes[b]));
        }
        for (idx, list) in children.into_iter().enumerate() {
            nodes[idx].children = list;
        }
        Self {
            nodes,
            roots,
            index,
        }
    }

    /// Whether `idx` merges into its parent under flattenEmptyDirectories:
    /// the parent is a directory whose only child is this directory.
    fn merges_into_parent(&self, idx: usize) -> Option<usize> {
        let parent = self.nodes[idx].parent?;
        (self.nodes[idx].is_dir
            && self.nodes[parent].is_dir
            && self.nodes[parent].children.len() == 1)
            .then_some(parent)
    }

    /// Topmost directory of the flattened chain containing `idx`.
    fn chain_start(&self, mut idx: usize) -> usize {
        while let Some(parent) = self.merges_into_parent(idx) {
            idx = parent;
        }
        idx
    }

    fn push_rows(
        &self,
        idx: usize,
        depth: usize,
        expanded: &HashSet<String>,
        out: &mut Vec<FileTreeRow>,
    ) {
        let node = &self.nodes[idx];
        if !node.is_dir {
            out.push(FileTreeRow {
                path: node.path.clone(),
                display_name: node.name.clone(),
                depth,
                is_dir: false,
                ignored: node.ignored,
                expanded: false,
            });
            return;
        }
        // `idx` is always a chain start here (roots, and children of a chain's
        // deepest directory, never merge upward). Walk the chain down.
        let mut display_name = node.name.clone();
        let mut ignored = node.ignored;
        let mut deepest = idx;
        loop {
            let current = &self.nodes[deepest];
            match current.children.as_slice() {
                [only] if self.nodes[*only].is_dir => {
                    deepest = *only;
                    display_name.push('/');
                    display_name.push_str(&self.nodes[deepest].name);
                    ignored |= self.nodes[deepest].ignored;
                }
                _ => break,
            }
        }
        let deep_node = &self.nodes[deepest];
        // Expansion of a flattened row is keyed by its deepest path only.
        let is_expanded = expanded.contains(&deep_node.path);
        out.push(FileTreeRow {
            path: deep_node.path.clone(),
            display_name,
            depth,
            is_dir: true,
            ignored,
            expanded: is_expanded,
        });
        if is_expanded {
            for &child in &deep_node.children {
                self.push_rows(child, depth + 1, expanded, out);
            }
        }
    }

    /// Rows currently visible given the set of expanded directory paths
    /// (collapsed-by-default; a directory's children are visible only when
    /// its path is in `expanded`). Sorted: directories before files at each
    /// level, then case-insensitive alphabetical (ties broken
    /// case-sensitively).
    pub fn visible_rows(&self, expanded: &HashSet<String>) -> Vec<FileTreeRow> {
        let mut out = Vec::new();
        for &root in &self.roots {
            self.push_rows(root, 0, expanded, &mut out);
        }
        out
    }

    /// All directory paths in the tree, including synthesized ancestors and
    /// interior segments of flattened chains (used for VCS decoration
    /// propagation). Lexicographically sorted for determinism.
    pub fn dir_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .nodes
            .iter()
            .filter(|node| node.is_dir)
            .map(|node| node.path.clone())
            .collect();
        paths.sort();
        paths
    }

    /// The directory paths that must be inserted into the expanded set so
    /// that `path` becomes visible: the deepest-dir path of each rendered
    /// ancestor row, root first. A path rendered as part of a flattened
    /// chain needs only the ancestors of the chain row itself. Unknown paths
    /// yield an empty list (mirrors the Electron reveal, which skips files
    /// absent from a truncated listing).
    pub fn reveal_dirs(&self, path: &str) -> Vec<String> {
        let Some(&idx) = self.index.get(normalize(path)) else {
            return Vec::new();
        };
        let row_top = if self.nodes[idx].is_dir {
            self.chain_start(idx)
        } else {
            idx
        };
        let mut result = Vec::new();
        let mut parent = self.nodes[row_top].parent;
        while let Some(ancestor) = parent {
            // A rendered row's parent node never merges downward (a chain
            // start below it or a file child forbids it), so `ancestor` is
            // the deepest node — the expansion key — of its own chain.
            result.push(self.nodes[ancestor].path.clone());
            parent = self.nodes[self.chain_start(ancestor)].parent;
        }
        result.reverse();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitre_contracts::TrimmedNonEmptyString;

    fn entry(path: &str, kind: ProjectEntryKind, ignored: Option<Option<bool>>) -> ProjectEntry {
        ProjectEntry {
            ignored,
            kind,
            path: TrimmedNonEmptyString(path.to_string()),
        }
    }

    fn file(path: &str) -> ProjectEntry {
        entry(path, ProjectEntryKind::File, None)
    }

    fn dir(path: &str) -> ProjectEntry {
        entry(path, ProjectEntryKind::Directory, None)
    }

    fn expanded(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    fn row_keys(rows: &[FileTreeRow]) -> Vec<(String, usize)> {
        rows.iter().map(|r| (r.path.clone(), r.depth)).collect()
    }

    #[test]
    fn empty_tree_has_no_rows_or_dirs() {
        let model = FileTreeModel::build(&[]);
        assert!(model.visible_rows(&HashSet::new()).is_empty());
        assert!(model.dir_paths().is_empty());
        assert!(model.reveal_dirs("anything").is_empty());
    }

    #[test]
    fn root_rows_sort_dirs_first_then_case_insensitive_with_case_sensitive_ties() {
        let model = FileTreeModel::build(&[
            file("zeta.txt"),
            file("Alpha.txt"),
            dir("src"),
            file("src/keep"), // makes src non-empty so it does not vanish
            dir("Docs"),
            file("Docs/keep"),
            file("apple"),
            file("Apple"),
        ]);
        let rows = model.visible_rows(&HashSet::new());
        let names: Vec<&str> = rows.iter().map(|r| r.display_name.as_str()).collect();
        // Dirs first (Docs < src case-insensitively), then files; "Apple" ties
        // "apple" case-insensitively and wins case-sensitively.
        assert_eq!(
            names,
            vec!["Docs", "src", "Alpha.txt", "Apple", "apple", "zeta.txt"]
        );
        assert!(rows.iter().all(|r| r.depth == 0));
    }

    #[test]
    fn collapsed_by_default_then_expansion_reveals_children_per_level() {
        let model = FileTreeModel::build(&[
            dir("a"),
            dir("a/b"),
            file("a/b/deep.txt"),
            file("a/top.txt"),
            file("root.txt"),
        ]);
        // "a" has two children so nothing flattens.
        assert_eq!(
            row_keys(&model.visible_rows(&HashSet::new())),
            vec![("a".into(), 0), ("root.txt".into(), 0)]
        );
        let one = model.visible_rows(&expanded(&["a"]));
        assert_eq!(
            row_keys(&one),
            vec![
                ("a".into(), 0),
                ("a/b".into(), 1),
                ("a/top.txt".into(), 1),
                ("root.txt".into(), 0),
            ]
        );
        assert!(one[0].expanded);
        assert!(!one[1].expanded);
        let both = model.visible_rows(&expanded(&["a", "a/b"]));
        assert_eq!(
            row_keys(&both),
            vec![
                ("a".into(), 0),
                ("a/b".into(), 1),
                ("a/b/deep.txt".into(), 2),
                ("a/top.txt".into(), 1),
                ("root.txt".into(), 0),
            ]
        );
        // Expanding a child without its parent shows nothing extra.
        assert_eq!(
            row_keys(&model.visible_rows(&expanded(&["a/b"]))),
            vec![("a".into(), 0), ("root.txt".into(), 0)]
        );
    }

    #[test]
    fn empty_directory_chain_flattens_into_one_row_keyed_by_deepest_path() {
        let model = FileTreeModel::build(&[
            dir("a"),
            dir("a/b"),
            dir("a/b/c"),
            file("a/b/c/one.txt"),
            dir("a/b/c/sub"),
        ]);
        let rows = model.visible_rows(&HashSet::new());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "a/b/c");
        assert_eq!(rows[0].display_name, "a/b/c");
        assert_eq!(rows[0].depth, 0);
        assert!(rows[0].is_dir);
        assert!(!rows[0].expanded);

        // Interior paths are not expansion keys.
        for key in ["a", "a/b"] {
            let rows = model.visible_rows(&expanded(&[key]));
            assert_eq!(rows.len(), 1, "interior key {key:?} must not expand");
            assert!(!rows[0].expanded);
        }

        // Children of the expanded chain sit one level below the chain row.
        let rows = model.visible_rows(&expanded(&["a/b/c"]));
        assert!(rows[0].expanded);
        assert_eq!(
            row_keys(&rows),
            vec![
                ("a/b/c".into(), 0),
                ("a/b/c/sub".into(), 1),
                ("a/b/c/one.txt".into(), 1),
            ]
        );
    }

    #[test]
    fn directories_with_a_file_child_or_multiple_children_do_not_flatten() {
        let model = FileTreeModel::build(&[
            dir("solo"),
            file("solo/readme.md"), // single child, but a file
            dir("multi"),
            dir("multi/x"),
            dir("multi/y"),
        ]);
        let rows = model.visible_rows(&expanded(&["solo", "multi"]));
        assert_eq!(
            row_keys(&rows),
            vec![
                ("multi".into(), 0),
                ("multi/x".into(), 1),
                ("multi/y".into(), 1),
                ("solo".into(), 0),
                ("solo/readme.md".into(), 1),
            ]
        );
    }

    #[test]
    fn empty_leaf_directory_renders_as_its_own_row() {
        let model = FileTreeModel::build(&[dir("empty")]);
        let rows = model.visible_rows(&HashSet::new());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "empty");
        assert_eq!(rows[0].display_name, "empty");
        assert!(rows[0].is_dir);
    }

    #[test]
    fn nested_chain_below_a_regular_directory_keeps_parent_depth_plus_one() {
        let model = FileTreeModel::build(&[
            dir("src"),
            file("src/main.rs"),
            dir("src/x"),
            dir("src/x/y"),
            file("src/x/y/mod.rs"),
        ]);
        let rows = model.visible_rows(&expanded(&["src", "src/x/y"]));
        assert_eq!(
            row_keys(&rows),
            vec![
                ("src".into(), 0),
                ("src/x/y".into(), 1),
                ("src/x/y/mod.rs".into(), 2),
                ("src/main.rs".into(), 1),
            ]
        );
        assert_eq!(rows[1].display_name, "x/y");
    }

    #[test]
    fn missing_ancestor_directories_are_synthesized() {
        // Only file entries: a and a/b never appear explicitly.
        let model = FileTreeModel::build(&[file("a/b/c.txt")]);
        assert_eq!(model.dir_paths(), vec!["a".to_string(), "a/b".to_string()]);
        let rows = model.visible_rows(&HashSet::new());
        // a merges with b (its only child is the directory b; b's only child
        // is a file, so the chain stops at b).
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "a/b");
        assert_eq!(rows[0].display_name, "a/b");
        let rows = model.visible_rows(&expanded(&["a/b"]));
        assert_eq!(
            row_keys(&rows),
            vec![("a/b".into(), 0), ("a/b/c.txt".into(), 1)]
        );
    }

    #[test]
    fn explicit_file_entry_that_is_also_an_ancestor_becomes_a_directory() {
        let model = FileTreeModel::build(&[file("weird"), file("weird/child.txt")]);
        assert_eq!(model.dir_paths(), vec!["weird".to_string()]);
        let rows = model.visible_rows(&expanded(&["weird"]));
        assert!(rows[0].is_dir);
        assert_eq!(rows[1].path, "weird/child.txt");
    }

    #[test]
    fn reveal_dirs_returns_rendered_ancestor_keys_root_first() {
        let model = FileTreeModel::build(&[
            dir("src"),
            file("src/main.rs"),
            dir("src/x"),
            dir("src/x/y"),
            file("src/x/y/mod.rs"),
        ]);
        // Through the flattened chain: the chain's key is its deepest path.
        assert_eq!(model.reveal_dirs("src/x/y/mod.rs"), vec!["src", "src/x/y"]);
        assert_eq!(model.reveal_dirs("src/main.rs"), vec!["src"]);
        // A directory rendered as part of a chain only needs the chain row's
        // own ancestors, not the chain itself.
        assert_eq!(model.reveal_dirs("src/x"), vec!["src"]);
        assert_eq!(model.reveal_dirs("src/x/y"), vec!["src"]);
        // Root-level nodes need nothing; unknown paths yield nothing.
        assert!(model.reveal_dirs("src").is_empty());
        assert!(model.reveal_dirs("nope/nothing.txt").is_empty());
    }

    #[test]
    fn reveal_dirs_expansion_actually_makes_the_path_visible() {
        let model = FileTreeModel::build(&[
            dir("a"),
            dir("a/b"),
            dir("a/b/c"),
            file("a/b/c/deep.txt"),
            file("a/b/c/other.txt"),
        ]);
        let reveal: HashSet<String> = model.reveal_dirs("a/b/c/deep.txt").into_iter().collect();
        assert_eq!(reveal, expanded(&["a/b/c"]));
        let rows = model.visible_rows(&reveal);
        assert!(rows.iter().any(|r| r.path == "a/b/c/deep.txt"));
    }

    #[test]
    fn ignored_maps_only_from_some_some_true_and_unknown_kind_is_a_file() {
        let model = FileTreeModel::build(&[
            entry("ignored.log", ProjectEntryKind::File, Some(Some(true))),
            entry(
                "explicit-false.txt",
                ProjectEntryKind::File,
                Some(Some(false)),
            ),
            entry("effect-none.txt", ProjectEntryKind::File, Some(None)),
            entry("absent.txt", ProjectEntryKind::File, None),
            entry(
                "mystery",
                ProjectEntryKind::Unknown("symlink".to_string()),
                None,
            ),
        ]);
        let rows = model.visible_rows(&HashSet::new());
        let by_path = |path: &str| rows.iter().find(|r| r.path == path).unwrap();
        assert!(by_path("ignored.log").ignored);
        assert!(!by_path("explicit-false.txt").ignored);
        assert!(!by_path("effect-none.txt").ignored);
        assert!(!by_path("absent.txt").ignored);
        assert!(!by_path("mystery").is_dir);
        assert!(model.dir_paths().is_empty());
    }

    #[test]
    fn ignored_directory_taints_its_flattened_chain_row() {
        let model = FileTreeModel::build(&[
            entry(
                "node_modules",
                ProjectEntryKind::Directory,
                Some(Some(true)),
            ),
            dir("node_modules/pkg"),
            file("node_modules/pkg/index.js"),
        ]);
        let rows = model.visible_rows(&HashSet::new());
        assert_eq!(rows[0].path, "node_modules/pkg");
        assert!(rows[0].ignored);
    }

    #[test]
    fn build_is_order_independent_and_tolerates_duplicates() {
        let forward = vec![dir("a"), dir("a/b"), file("a/b/f.txt")];
        let backward = vec![file("a/b/f.txt"), dir("a/b"), dir("a"), dir("a")];
        let forward_model = FileTreeModel::build(&forward);
        let backward_model = FileTreeModel::build(&backward);
        let all = expanded(&["a/b"]);
        assert_eq!(
            forward_model.visible_rows(&all),
            backward_model.visible_rows(&all)
        );
        assert_eq!(forward_model.dir_paths(), backward_model.dir_paths());
    }
}
