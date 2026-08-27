//! VCS decoration propagation for the file tree.
//!
//! Port of `apps/web/src/components/files/fileTreeGitStatus.ts`
//! (`buildFileTreeGitDecorations`): projects `vcs.getFileStatuses` entries
//! onto per-row tree decorations — a status for every reported file plus an
//! aggregated status for every ancestor directory, so a change buried deep in
//! the tree stays visible from the collapsed root.
//!
//! Differences forced by the Rust API shape (the semantics are unchanged):
//! - The TS layer emits separate file (`path`) and directory (`path/`)
//!   entries; here both land in one slash-free map. On the pathological
//!   file/directory name collision (file deleted, same-named directory
//!   created) the directory status wins, which is what the TS tree renders
//!   for the row that actually exists.
//! - `VcsFileStatusCode::Unknown` is the wire's forward-compatibility escape
//!   hatch; the TS status table has no arm for it, so such entries contribute
//!   no decoration at all (and an unknown-status directory record does not
//!   trigger untracked expansion).

use std::collections::{HashMap, HashSet};

use vitre_contracts::{VcsFileStatusCode, VcsFileStatusEntry};

/// Status rendered on a tree row (order = display priority, highest first).
///
/// `Ignored` exists because the tree's priority ladder has it (lowest), but
/// `vcs.getFileStatuses` never reports it, so it never appears in outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeVcsStatus {
    Modified,
    Renamed,
    Deleted,
    Added,
    Untracked,
    Ignored,
}

#[derive(Debug, Default, PartialEq)]
pub struct TreeVcsDecorations {
    /// Per-path decoration: files with a status plus every ancestor directory
    /// (a directory shows the highest-priority status among its descendants).
    pub statuses: HashMap<String, TreeVcsStatus>,
    /// Paths whose underlying git status is conflicted (rendered as Modified
    /// color + a "!" letter).
    pub conflicted: HashSet<String>,
}

/// A folder takes the most attention-worthy status among its descendants, so
/// a directory holding one edit plus ten new files still reads as modified.
fn rank(status: TreeVcsStatus) -> u8 {
    match status {
        TreeVcsStatus::Modified => 0,
        TreeVcsStatus::Renamed => 1,
        TreeVcsStatus::Deleted => 2,
        TreeVcsStatus::Added => 3,
        TreeVcsStatus::Untracked => 4,
        TreeVcsStatus::Ignored => 5,
    }
}

/// The tree has no conflicted status; conflicts read as modified and carry a
/// "!" in the decoration lane instead. `Unknown` has no rendering: `None`.
fn tree_status(code: &VcsFileStatusCode) -> Option<TreeVcsStatus> {
    match code {
        VcsFileStatusCode::Modified => Some(TreeVcsStatus::Modified),
        VcsFileStatusCode::Added => Some(TreeVcsStatus::Added),
        VcsFileStatusCode::Deleted => Some(TreeVcsStatus::Deleted),
        VcsFileStatusCode::Renamed => Some(TreeVcsStatus::Renamed),
        VcsFileStatusCode::Untracked => Some(TreeVcsStatus::Untracked),
        VcsFileStatusCode::Conflicted => Some(TreeVcsStatus::Modified),
        VcsFileStatusCode::Unknown(_) => None,
    }
}

fn without_trailing_slash(path: &str) -> &str {
    path.strip_suffix('/').unwrap_or(path)
}

/// Every directory above `path`, shallowest first, without trailing slashes.
fn ancestor_directories(path: &str) -> impl Iterator<Item = &str> {
    let normalized = without_trailing_slash(path);
    normalized
        .match_indices('/')
        .map(move |(index, _)| &normalized[..index])
}

/// Keep the highest-priority (lowest-rank) status seen for a directory.
fn raise_folder(folders: &mut HashMap<String, TreeVcsStatus>, path: &str, status: TreeVcsStatus) {
    let should_set = folders
        .get(path)
        .is_none_or(|current| rank(status) < rank(*current));
    if should_set {
        folders.insert(path.to_owned(), status);
    }
}

fn assign_path(
    files: &mut HashMap<String, TreeVcsStatus>,
    folders: &mut HashMap<String, TreeVcsStatus>,
    path: &str,
    status: TreeVcsStatus,
    is_directory: bool,
) {
    let normalized = without_trailing_slash(path);
    if normalized.is_empty() {
        return;
    }
    if is_directory {
        raise_folder(folders, normalized, status);
    } else {
        files.insert(normalized.to_owned(), status);
    }
    for ancestor in ancestor_directories(normalized) {
        raise_folder(folders, ancestor, status);
    }
}

/// `tree_paths` = every path the rendered tree knows (files and directories,
/// workspace-relative, no trailing slashes) — used to expand wholly-untracked
/// directories (git reports one record for the dir) onto their descendants.
pub fn build_tree_vcs_decorations(
    entries: &[VcsFileStatusEntry],
    tree_paths: &[String],
) -> TreeVcsDecorations {
    if entries.is_empty() {
        return TreeVcsDecorations::default();
    }

    let mut file_statuses: HashMap<String, TreeVcsStatus> = HashMap::new();
    let mut folder_statuses: HashMap<String, TreeVcsStatus> = HashMap::new();
    let mut conflicted: HashSet<String> = HashSet::new();
    let mut untracked_directories: Vec<&str> = Vec::new();

    for entry in entries {
        let path = entry.path.0.as_str();
        let Some(status) = tree_status(&entry.status) else {
            continue;
        };
        let is_directory = path.ends_with('/');
        if is_directory {
            // git collapses a wholly untracked directory into one record.
            untracked_directories.push(path);
        } else if entry.status == VcsFileStatusCode::Conflicted {
            conflicted.insert(path.to_owned());
        }
        assign_path(
            &mut file_statuses,
            &mut folder_statuses,
            path,
            status,
            is_directory,
        );
    }

    if !untracked_directories.is_empty() {
        for tree_path in tree_paths {
            let normalized = without_trailing_slash(tree_path);
            if normalized.is_empty() {
                continue;
            }
            if file_statuses.contains_key(normalized) || folder_statuses.contains_key(normalized) {
                continue;
            }
            // The git record's trailing slash keeps prefix-only siblings
            // ("newdir2" vs "newdir/") from matching.
            if untracked_directories
                .iter()
                .any(|directory| tree_path.starts_with(directory))
            {
                let is_directory = tree_path.ends_with('/');
                assign_path(
                    &mut file_statuses,
                    &mut folder_statuses,
                    tree_path,
                    TreeVcsStatus::Untracked,
                    is_directory,
                );
            }
        }
    }

    let mut statuses = file_statuses;
    for (path, status) in folder_statuses {
        statuses.insert(path, status);
    }
    TreeVcsDecorations {
        statuses,
        conflicted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitre_contracts::TrimmedNonEmptyString;

    fn entry(path: &str, status: VcsFileStatusCode) -> VcsFileStatusEntry {
        entry_staged(path, status, false)
    }

    fn entry_staged(path: &str, status: VcsFileStatusCode, staged: bool) -> VcsFileStatusEntry {
        VcsFileStatusEntry {
            path: TrimmedNonEmptyString(path.to_owned()),
            staged,
            status,
        }
    }

    fn paths(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn empty_statuses_produce_empty_decorations() {
        let result = build_tree_vcs_decorations(&[], &paths(&["dir", "dir/file.txt"]));
        assert_eq!(result, TreeVcsDecorations::default());
    }

    #[test]
    fn per_file_status_mapping_and_ancestor_propagation() {
        let cases: &[(VcsFileStatusCode, Option<TreeVcsStatus>)] = &[
            (VcsFileStatusCode::Modified, Some(TreeVcsStatus::Modified)),
            (VcsFileStatusCode::Added, Some(TreeVcsStatus::Added)),
            (VcsFileStatusCode::Deleted, Some(TreeVcsStatus::Deleted)),
            (VcsFileStatusCode::Renamed, Some(TreeVcsStatus::Renamed)),
            (VcsFileStatusCode::Untracked, Some(TreeVcsStatus::Untracked)),
            // Conflicts read as modified; the "!" comes from `conflicted`.
            (VcsFileStatusCode::Conflicted, Some(TreeVcsStatus::Modified)),
            // Forward-compat literal this build cannot render: no decoration.
            (VcsFileStatusCode::Unknown("copied".to_owned()), None),
        ];
        for (code, expected) in cases {
            let result =
                build_tree_vcs_decorations(&[entry("dir/sub/file.txt", code.clone())], &[]);
            assert_eq!(
                result.statuses.get("dir/sub/file.txt").copied(),
                *expected,
                "file status for {code:?}"
            );
            assert_eq!(
                result.statuses.get("dir").copied(),
                *expected,
                "shallow ancestor for {code:?}"
            );
            assert_eq!(
                result.statuses.get("dir/sub").copied(),
                *expected,
                "deep ancestor for {code:?}"
            );
        }
    }

    #[test]
    fn conflicted_files_collected_and_ancestors_read_modified() {
        let result = build_tree_vcs_decorations(
            &[
                entry("src/merge.rs", VcsFileStatusCode::Conflicted),
                entry("src/other.rs", VcsFileStatusCode::Added),
            ],
            &[],
        );
        assert!(result.conflicted.contains("src/merge.rs"));
        assert!(!result.conflicted.contains("src/other.rs"));
        assert_eq!(
            result.statuses.get("src/merge.rs"),
            Some(&TreeVcsStatus::Modified)
        );
        // Modified (via conflict) outranks the sibling's Added.
        assert_eq!(result.statuses.get("src"), Some(&TreeVcsStatus::Modified));
    }

    #[test]
    fn directory_takes_highest_priority_descendant_status() {
        // Adjacent rungs of the priority ladder, each checked in both entry
        // orders (Ignored is unreachable from wire statuses).
        let ladder: &[(VcsFileStatusCode, VcsFileStatusCode, TreeVcsStatus)] = &[
            (
                VcsFileStatusCode::Modified,
                VcsFileStatusCode::Renamed,
                TreeVcsStatus::Modified,
            ),
            (
                VcsFileStatusCode::Renamed,
                VcsFileStatusCode::Deleted,
                TreeVcsStatus::Renamed,
            ),
            (
                VcsFileStatusCode::Deleted,
                VcsFileStatusCode::Added,
                TreeVcsStatus::Deleted,
            ),
            (
                VcsFileStatusCode::Added,
                VcsFileStatusCode::Untracked,
                TreeVcsStatus::Added,
            ),
        ];
        for (high, low, expected) in ladder {
            for (first, second) in [(high, low), (low, high)] {
                let result = build_tree_vcs_decorations(
                    &[entry("d/a", first.clone()), entry("d/b", second.clone())],
                    &[],
                );
                assert_eq!(
                    result.statuses.get("d").copied(),
                    Some(*expected),
                    "{first:?} + {second:?}"
                );
            }
        }
    }

    #[test]
    fn untracked_directory_record_expands_over_tree_descendants() {
        let result = build_tree_vcs_decorations(
            &[entry("newdir/", VcsFileStatusCode::Untracked)],
            &paths(&[
                "newdir",
                "newdir/a.txt",
                "newdir/sub",
                "newdir/sub/b.txt",
                "newdir2",
                "other.txt",
            ]),
        );
        for path in ["newdir", "newdir/a.txt", "newdir/sub", "newdir/sub/b.txt"] {
            assert_eq!(
                result.statuses.get(path),
                Some(&TreeVcsStatus::Untracked),
                "{path}"
            );
        }
        // Prefix-only sibling and unrelated file stay undecorated.
        assert!(!result.statuses.contains_key("newdir2"));
        assert!(!result.statuses.contains_key("other.txt"));
        assert!(result.conflicted.is_empty());
    }

    #[test]
    fn untracked_expansion_never_overrides_existing_statuses() {
        let result = build_tree_vcs_decorations(
            &[
                entry("d/", VcsFileStatusCode::Untracked),
                entry("d/inner.txt", VcsFileStatusCode::Added),
            ],
            &paths(&["d", "d/inner.txt", "d/other.txt"]),
        );
        assert_eq!(
            result.statuses.get("d/inner.txt"),
            Some(&TreeVcsStatus::Added)
        );
        assert_eq!(
            result.statuses.get("d/other.txt"),
            Some(&TreeVcsStatus::Untracked)
        );
        // Added (from the inner file) outranks the directory's own Untracked.
        assert_eq!(result.statuses.get("d"), Some(&TreeVcsStatus::Added));
    }

    #[test]
    fn unknown_status_directory_record_does_not_trigger_expansion() {
        let result = build_tree_vcs_decorations(
            &[entry("d/", VcsFileStatusCode::Unknown("weird".to_owned()))],
            &paths(&["d", "d/x.txt"]),
        );
        assert_eq!(result, TreeVcsDecorations::default());
    }

    #[test]
    fn deleted_file_absent_from_tree_still_decorates_ancestors() {
        let result = build_tree_vcs_decorations(
            &[entry("src/gone.rs", VcsFileStatusCode::Deleted)],
            &paths(&["src", "src/main.rs"]),
        );
        assert_eq!(
            result.statuses.get("src/gone.rs"),
            Some(&TreeVcsStatus::Deleted)
        );
        assert_eq!(result.statuses.get("src"), Some(&TreeVcsStatus::Deleted));
        assert!(!result.statuses.contains_key("src/main.rs"));
    }

    #[test]
    fn staged_flag_is_irrelevant_to_decorations() {
        let unstaged = build_tree_vcs_decorations(
            &[entry_staged("a/f.txt", VcsFileStatusCode::Modified, false)],
            &[],
        );
        let staged = build_tree_vcs_decorations(
            &[entry_staged("a/f.txt", VcsFileStatusCode::Modified, true)],
            &[],
        );
        assert_eq!(unstaged, staged);
    }

    #[test]
    fn duplicate_file_entries_last_wins_but_ancestors_keep_highest_priority() {
        // git can report a path twice (staged + unstaged records).
        let result = build_tree_vcs_decorations(
            &[
                entry_staged("a/f", VcsFileStatusCode::Modified, true),
                entry_staged("a/f", VcsFileStatusCode::Added, false),
            ],
            &[],
        );
        assert_eq!(result.statuses.get("a/f"), Some(&TreeVcsStatus::Added));
        assert_eq!(result.statuses.get("a"), Some(&TreeVcsStatus::Modified));

        let reversed = build_tree_vcs_decorations(
            &[
                entry_staged("a/f", VcsFileStatusCode::Added, true),
                entry_staged("a/f", VcsFileStatusCode::Modified, false),
            ],
            &[],
        );
        assert_eq!(reversed.statuses.get("a/f"), Some(&TreeVcsStatus::Modified));
        assert_eq!(reversed.statuses.get("a"), Some(&TreeVcsStatus::Modified));
    }
}
