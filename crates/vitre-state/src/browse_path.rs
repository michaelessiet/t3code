//! Path arithmetic for the add-project filesystem browser, ported from
//! `packages/client-runtime/src/state/projects.ts` and the pieces of
//! `packages/shared/src/path.ts` that [`crate::project_grouping`] does not
//! already carry.
//!
//! Everything here is display/segmentation logic: the sidecar expands `~` and
//! resolves relative paths itself, so the client only ever cuts a query into
//! a directory portion (what `filesystem.browse` is asked for) and a leaf
//! (what filters the returned entries).

use crate::project_grouping::{is_unc_path, is_windows_drive_path, trim_trailing_path_separators};

/// `isWindowsAbsolutePath` (`packages/shared/src/path.ts`).
pub fn is_windows_absolute_path(value: &str) -> bool {
    is_unc_path(value) || is_windows_drive_path(value)
}

/// `isExplicitRelativePath` (`packages/shared/src/path.ts`).
pub fn is_explicit_relative_path(value: &str) -> bool {
    value == "."
        || value == ".."
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with(".\\")
        || value.starts_with("..\\")
}

/// `normalizeProjectPathForDispatch` (`packages/shared/src/path.ts`).
pub fn normalize_project_path_for_dispatch(value: &str) -> String {
    trim_trailing_path_separators(value.trim())
}

/// `isFilesystemBrowseQuery`: does this query switch the palette into
/// filesystem browsing? Windows absolute paths only count against a Windows
/// environment — `windows_platform` is the *server's* platform, not ours.
pub fn is_filesystem_browse_query(value: &str, windows_platform: bool) -> bool {
    value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with(".\\")
        || value.starts_with("..\\")
        || value.starts_with('/')
        || value.starts_with("~/")
        || (windows_platform && is_windows_absolute_path(value))
}

/// `isUnsupportedWindowsProjectPath`: a Windows-style path typed against a
/// non-Windows environment.
pub fn is_unsupported_windows_project_path(value: &str, windows_platform: bool) -> bool {
    is_windows_absolute_path(value) && !windows_platform
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AbsolutePathKind {
    Unix,
    Windows,
}

fn absolute_path_kind(value: &str) -> Option<AbsolutePathKind> {
    if is_windows_drive_path(value) || is_unc_path(value) {
        return Some(AbsolutePathKind::Windows);
    }
    if value.starts_with('/') {
        return Some(AbsolutePathKind::Unix);
    }
    None
}

/// `preferredPathSeparator`: the separator new segments of this path get.
fn preferred_path_separator(value: &str) -> char {
    match absolute_path_kind(value) {
        Some(AbsolutePathKind::Windows) => '\\',
        Some(AbsolutePathKind::Unix) => '/',
        None => {
            if value.contains('\\') {
                '\\'
            } else {
                '/'
            }
        }
    }
}

/// `hasTrailingPathSeparator`: a Unix-rooted path only ends on `/`; anything
/// else ends on either separator.
pub fn has_trailing_path_separator(value: &str) -> bool {
    if absolute_path_kind(value) == Some(AbsolutePathKind::Unix) {
        value.ends_with('/')
    } else {
        value.ends_with('/') || value.ends_with('\\')
    }
}

/// `getLastPathSeparatorIndex`: byte index, honoring the Unix-rooted rule.
fn last_path_separator_index(value: &str) -> Option<usize> {
    if absolute_path_kind(value) == Some(AbsolutePathKind::Unix) {
        return value.rfind('/');
    }
    match (value.rfind('/'), value.rfind('\\')) {
        (Some(slash), Some(backslash)) => Some(slash.max(backslash)),
        (Some(slash), None) => Some(slash),
        (None, Some(backslash)) => Some(backslash),
        (None, None) => None,
    }
}

fn split_path_segments(value: &str, separator: char) -> Vec<String> {
    let split: Box<dyn Iterator<Item = &str>> = if separator == '/' {
        Box::new(value.split('/'))
    } else {
        Box::new(value.split(['/', '\\']))
    };
    split
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

struct AbsolutePath {
    root: String,
    separator: char,
    segments: Vec<String>,
}

/// `splitAbsolutePath`: root + separator + segments, or `None` for a path
/// that is not absolute in either convention.
fn split_absolute_path(value: &str) -> Option<AbsolutePath> {
    if is_windows_drive_path(value) {
        let root = format!("{}\\", &value[..2]);
        let segments = split_path_segments(value.get(3..).unwrap_or(""), '\\');
        return Some(AbsolutePath {
            root,
            separator: '\\',
            segments,
        });
    }
    if is_unc_path(value) {
        let mut segments = split_path_segments(value, '\\').into_iter();
        let server = segments.next()?;
        let share = segments.next()?;
        return Some(AbsolutePath {
            root: format!("\\\\{server}\\{share}\\"),
            separator: '\\',
            segments: segments.collect(),
        });
    }
    if let Some(rest) = value.strip_prefix('/') {
        return Some(AbsolutePath {
            root: "/".to_owned(),
            separator: '/',
            segments: split_path_segments(rest, '/'),
        });
    }
    None
}

/// `resolveProjectPathForDispatch`: apply an explicit `./`-relative path to
/// the active project's cwd; anything else just normalizes.
pub fn resolve_project_path_for_dispatch(value: &str, cwd: Option<&str>) -> String {
    let trimmed = value.trim();
    let Some(cwd) = cwd.filter(|_| is_explicit_relative_path(trimmed)) else {
        return normalize_project_path_for_dispatch(trimmed);
    };
    let Some(base) = split_absolute_path(&normalize_project_path_for_dispatch(cwd)) else {
        return normalize_project_path_for_dispatch(trimmed);
    };

    let mut segments = base.segments;
    for segment in trimmed.split(['/', '\\']) {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other.to_owned()),
        }
    }

    let joined = segments.join(&base.separator.to_string());
    normalize_project_path_for_dispatch(&if joined.is_empty() {
        base.root
    } else {
        format!("{}{joined}", base.root)
    })
}

/// `inferProjectTitleFromPath`: the last real path segment.
pub fn infer_project_title_from_path(value: &str) -> String {
    let normalized = normalize_project_path_for_dispatch(value);
    if let Some(absolute) = split_absolute_path(&normalized) {
        return absolute
            .segments
            .into_iter()
            .next_back()
            .unwrap_or(normalized);
    }
    normalized
        .split(['/', '\\'])
        .rfind(|segment| !segment.is_empty())
        .map(str::to_owned)
        .unwrap_or(normalized)
}

/// `appendBrowsePathSegment`: descend into `segment`, leaving a trailing
/// separator so the next `filesystem.browse` lists the new directory.
pub fn append_browse_path_segment(current_path: &str, segment: &str) -> String {
    let separator = preferred_path_separator(current_path);
    format!(
        "{}{segment}{separator}",
        get_browse_directory_path(current_path)
    )
}

/// `getBrowseLeafPathSegment`: the text after the last separator — what
/// filters the fetched entries.
pub fn get_browse_leaf_path_segment(current_path: &str) -> &str {
    match last_path_separator_index(current_path) {
        Some(index) => &current_path[index + 1..],
        None => current_path,
    }
}

/// `getBrowseDirectoryPath`: the directory portion, separator included —
/// what `filesystem.browse` is asked for.
pub fn get_browse_directory_path(current_path: &str) -> &str {
    if has_trailing_path_separator(current_path) {
        return current_path;
    }
    match last_path_separator_index(current_path) {
        Some(index) => &current_path[..=index],
        None => current_path,
    }
}

/// `ensureBrowseDirectoryPath`: seed value for a browse view — trimmed, with
/// a trailing separator appended when it lacks one.
pub fn ensure_browse_directory_path(current_path: &str) -> String {
    let trimmed = current_path.trim();
    if trimmed.is_empty() || has_trailing_path_separator(trimmed) {
        return trimmed.to_owned();
    }
    format!("{trimmed}{}", preferred_path_separator(trimmed))
}

/// `getBrowseParentPath`: one level up, trailing separator kept; `None` at a
/// root.
pub fn get_browse_parent_path(current_path: &str) -> Option<String> {
    let trimmed = normalize_project_path_for_dispatch(current_path);
    if let Some(absolute) = split_absolute_path(&trimmed) {
        match absolute.segments.len() {
            0 => return None,
            1 => return Some(absolute.root),
            _ => {
                let parent = absolute.segments[..absolute.segments.len() - 1]
                    .join(&absolute.separator.to_string());
                return Some(format!("{}{parent}{}", absolute.root, absolute.separator));
            }
        }
    }

    let separator = preferred_path_separator(current_path);
    let index = last_path_separator_index(&trimmed)?;
    // "C:relative\x" keeps its drive prefix when reduced to the root.
    if index == 2 && trimmed.len() >= 2 && trimmed[1..].starts_with(':') {
        return Some(format!("{}{separator}", &trimmed[..2]));
    }
    Some(trimmed[..=index].to_owned())
}

/// `canNavigateUp`: the `..` row only appears on a fully-typed directory.
pub fn can_navigate_up(current_path: &str) -> bool {
    has_trailing_path_separator(current_path) && get_browse_parent_path(current_path).is_some()
}

/// `filterBrowseEntries` visibility rule (`CommandPalette.logic.ts`): a
/// case-insensitive prefix match on the leaf, with dot-directories hidden
/// until the leaf itself starts with a dot.
pub fn browse_entry_visible(name: &str, leaf: &str) -> bool {
    name.to_lowercase().starts_with(&leaf.to_lowercase())
        && (leaf.starts_with('.') || !name.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_queries_are_path_shaped_prefixes() {
        for query in ["/tmp", "~/code", "./x", "../x", ".\\x", "..\\x"] {
            assert!(is_filesystem_browse_query(query, false), "{query}");
        }
        for query in ["projects", ">add", "", "~x"] {
            assert!(!is_filesystem_browse_query(query, false), "{query}");
        }
        // Windows absolute paths only browse against a Windows server.
        assert!(!is_filesystem_browse_query("C:\\code", false));
        assert!(is_filesystem_browse_query("C:\\code", true));
        assert!(is_filesystem_browse_query("\\\\server\\share", true));
    }

    #[test]
    fn directory_and_leaf_split_at_the_last_separator() {
        assert_eq!(get_browse_directory_path("~/pro"), "~/");
        assert_eq!(get_browse_leaf_path_segment("~/pro"), "pro");
        assert_eq!(get_browse_directory_path("~/projects/"), "~/projects/");
        assert_eq!(get_browse_leaf_path_segment("~/projects/"), "");
        assert_eq!(get_browse_directory_path("/a/b/c"), "/a/b/");
        // A Unix-rooted path treats backslash as part of the name.
        assert_eq!(get_browse_directory_path("/a/b\\c"), "/a/");
        assert_eq!(get_browse_leaf_path_segment("/a/b\\c"), "b\\c");
        assert_eq!(get_browse_directory_path("C:\\a\\b"), "C:\\a\\");
    }

    #[test]
    fn descending_appends_a_segment_with_the_paths_own_separator() {
        assert_eq!(
            append_browse_path_segment("~/pro", "projects"),
            "~/projects/"
        );
        assert_eq!(append_browse_path_segment("/a/", "b"), "/a/b/");
        assert_eq!(append_browse_path_segment("C:\\a\\", "b"), "C:\\a\\b\\");
    }

    #[test]
    fn parent_navigation_stops_at_roots() {
        assert_eq!(get_browse_parent_path("/a/b/").as_deref(), Some("/a/"));
        assert_eq!(get_browse_parent_path("/a/").as_deref(), Some("/"));
        assert_eq!(get_browse_parent_path("/"), None);
        assert_eq!(get_browse_parent_path("C:\\a\\").as_deref(), Some("C:\\"));
        assert_eq!(get_browse_parent_path("C:\\"), None);
        assert_eq!(
            get_browse_parent_path("\\\\server\\share\\a\\").as_deref(),
            Some("\\\\server\\share\\")
        );
        assert_eq!(get_browse_parent_path("\\\\server\\share\\"), None);

        assert!(can_navigate_up("/a/"));
        assert!(!can_navigate_up("/a"));
        assert!(!can_navigate_up("/"));
    }

    #[test]
    fn seed_paths_gain_a_trailing_separator() {
        assert_eq!(ensure_browse_directory_path(" ~/code "), "~/code/");
        assert_eq!(ensure_browse_directory_path("~/code/"), "~/code/");
        assert_eq!(ensure_browse_directory_path(""), "");
        assert_eq!(ensure_browse_directory_path("C:\\code"), "C:\\code\\");
    }

    #[test]
    fn explicit_relative_paths_resolve_against_the_cwd() {
        assert_eq!(
            resolve_project_path_for_dispatch("../sibling", Some("/repo/app")),
            "/repo/sibling"
        );
        assert_eq!(
            resolve_project_path_for_dispatch("./x/./y", Some("/repo")),
            "/repo/x/y"
        );
        // No cwd: the relative path just normalizes.
        assert_eq!(resolve_project_path_for_dispatch("./x/", None), "./x");
        // Absolute input ignores the cwd.
        assert_eq!(
            resolve_project_path_for_dispatch("/tmp/z/", Some("/repo")),
            "/tmp/z"
        );
        // Walking above the root lands on the root.
        assert_eq!(
            resolve_project_path_for_dispatch("../../..", Some("/a/b")),
            "/"
        );
    }

    #[test]
    fn titles_are_the_last_real_segment() {
        assert_eq!(infer_project_title_from_path("/a/b/my-app/"), "my-app");
        assert_eq!(infer_project_title_from_path("C:\\code\\app"), "app");
        assert_eq!(infer_project_title_from_path("~/x"), "x");
        assert_eq!(infer_project_title_from_path("/"), "/");
    }

    #[test]
    fn hidden_directories_need_an_explicit_dot() {
        assert!(browse_entry_visible("src", "s"));
        assert!(browse_entry_visible("SRC", "sr"));
        assert!(!browse_entry_visible(".git", ""));
        assert!(!browse_entry_visible(".git", "g"));
        assert!(browse_entry_visible(".git", "."));
        assert!(browse_entry_visible(".git", ".g"));
    }

    #[test]
    fn windows_guard_only_fires_off_platform() {
        assert!(is_unsupported_windows_project_path("C:\\x", false));
        assert!(!is_unsupported_windows_project_path("C:\\x", true));
        assert!(!is_unsupported_windows_project_path("/x", false));
    }
}
