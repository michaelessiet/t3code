//! Branch-toolbar logic: the pure parts of Electron's `BranchToolbar.tsx`
//! branch selector — trigger labels, ref badges, picker filtering, page
//! merging, and checkout-target resolution (`resolveBranchSelectionTarget`).
//!
//! Vitre has no draft threads, so the draft-only worktree-base mode ("pick a
//! base ref, the worktree is created at send time") never activates today;
//! the helpers still take the flag so the shapes match Electron term for
//! term and the mode can light up later.

use vitre_contracts::{VcsListRefsResult, VcsRef, VcsStatusRemoteResultPrState};

/// `VCS_REF_LIST_LIMIT` — every page requests exactly 100 refs.
pub const REF_LIST_LIMIT: i64 = 100;

/// `resolveBranchToolbarValue`: in worktree mode without a worktree yet the
/// thread branch (the picked base) wins; otherwise git's live ref wins.
pub fn resolve_branch_toolbar_value(
    worktree_base_mode: bool,
    thread_branch: Option<&str>,
    current_git_branch: Option<&str>,
) -> Option<String> {
    let value = if worktree_base_mode {
        thread_branch.or(current_git_branch)
    } else {
        current_git_branch.or(thread_branch)
    };
    value.map(str::to_string)
}

/// `getBranchTriggerLabel`. Quirk strings verbatim, per the parity directive.
pub fn branch_trigger_label(worktree_base_mode: bool, value: Option<&str>) -> String {
    match value {
        None => "Select ref".to_string(),
        Some(branch) if worktree_base_mode => format!("From {branch}"),
        Some(branch) => branch.to_string(),
    }
}

/// The single badge on a ref row (first match wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefBadge {
    Current,
    Worktree,
    Remote,
    Default,
}

impl RefBadge {
    pub fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Worktree => "worktree",
            Self::Remote => "remote",
            Self::Default => "default",
        }
    }
}

/// Badge precedence: current → worktree (only when the tree isn't the project
/// checkout itself) → remote → default → none.
pub fn resolve_ref_badge(git_ref: &VcsRef, project_cwd: &str) -> Option<RefBadge> {
    if git_ref.current {
        return Some(RefBadge::Current);
    }
    if git_ref
        .worktree_path
        .as_ref()
        .is_some_and(|path| path.0 != project_cwd)
    {
        return Some(RefBadge::Worktree);
    }
    if git_ref.is_remote.flatten().unwrap_or(false) {
        return Some(RefBadge::Remote);
    }
    if git_ref.is_default {
        return Some(RefBadge::Default);
    }
    None
}

/// `shouldIncludeBranchPickerItem` for plain ref rows: empty query passes
/// everything; otherwise case-insensitive substring on the trimmed query.
/// (The server also filters by `query`; this covers the deferred window.)
pub fn should_include_branch_picker_item(query: &str, name: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    name.to_lowercase().contains(&query.to_lowercase())
}

/// The appended "Create new ref" row exists when not picking a worktree
/// base, the trimmed query is non-empty, and no ref matches it exactly.
pub fn create_item_query(
    selecting_worktree_base: bool,
    query: &str,
    refs: &[VcsRef],
) -> Option<String> {
    if selecting_worktree_base {
        return None;
    }
    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    if refs.iter().any(|git_ref| git_ref.name.0 == query) {
        return None;
    }
    Some(query.to_string())
}

/// The create row's label, quirk quoting included.
pub fn create_item_label(query: &str) -> String {
    format!("Create new ref \"{query}\"")
}

/// The merged view over every fetched page (`usePaginatedBranches`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MergedRefs {
    /// First-seen order; later pages replace an existing name in place.
    pub refs: Vec<VcsRef>,
    /// From the **last** page.
    pub next_cursor: Option<i64>,
    /// Max across pages (a ref moving pages between revalidations must not
    /// shrink the reported corpus).
    pub total_count: i64,
}

pub fn merge_ref_pages(pages: &[VcsListRefsResult]) -> MergedRefs {
    let mut refs: Vec<VcsRef> = Vec::new();
    let mut index_by_name = std::collections::HashMap::new();
    let mut total_count = 0;
    for page in pages {
        total_count = total_count.max(page.total_count.0);
        for git_ref in &page.refs {
            match index_by_name.get(&git_ref.name.0) {
                Some(&at) => refs[at] = git_ref.clone(),
                None => {
                    index_by_name.insert(git_ref.name.0.clone(), refs.len());
                    refs.push(git_ref.clone());
                }
            }
        }
    }
    MergedRefs {
        refs,
        next_cursor: pages.last().and_then(|page| {
            page.next_cursor
                .as_ref()
                .map(|cursor: &vitre_contracts::NonNegativeInt| cursor.0)
        }),
        total_count,
    }
}

/// The popup footer's status row. Precedence per Electron: initial load →
/// next-page fetch → "Showing X of Y refs" while more pages exist → hidden.
pub fn refs_status_text(
    initial_pending: bool,
    fetching_next_page: bool,
    has_next_page: bool,
    shown: usize,
    total_count: i64,
) -> Option<String> {
    if initial_pending {
        return Some("Loading refs...".to_string());
    }
    if fetching_next_page {
        return Some("Loading more refs...".to_string());
    }
    if has_next_page {
        return Some(format!("Showing {shown} of {total_count} refs"));
    }
    None
}

/// What selecting a ref does (`resolveBranchSelectionTarget`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchSelectionTarget {
    /// The ref already lives in a worktree — adopt that tree, no git call.
    /// `next_worktree_path` is `None` when that tree *is* the main checkout.
    Reuse { next_worktree_path: Option<String> },
    /// Check the ref out via `vcs.switchRef` in `checkout_cwd`.
    Checkout {
        checkout_cwd: String,
        next_worktree_path: Option<String>,
    },
}

pub fn resolve_branch_selection_target(
    picked: &VcsRef,
    project_cwd: &str,
    active_worktree_path: Option<&str>,
) -> BranchSelectionTarget {
    if let Some(worktree) = picked.worktree_path.as_ref() {
        let next_worktree_path = (worktree.0 != project_cwd).then(|| worktree.0.clone());
        return BranchSelectionTarget::Reuse { next_worktree_path };
    }
    // Picking the default branch from inside a worktree hops back to the
    // main checkout.
    let next_worktree_path = if active_worktree_path.is_some() && picked.is_default {
        None
    } else {
        active_worktree_path.map(str::to_string)
    };
    let checkout_cwd = next_worktree_path
        .clone()
        .unwrap_or_else(|| project_cwd.to_string());
    BranchSelectionTarget::Checkout {
        checkout_cwd,
        next_worktree_path,
    }
}

/// `resolveLockedWorkspaceLabel` — the static env-mode label on locked
/// threads (every Vitre thread: no drafts, no pre-send mode override).
pub fn locked_workspace_label(has_worktree: bool) -> &'static str {
    if has_worktree {
        "Worktree"
    } else {
        "Local checkout"
    }
}

/// Human name of a PR state, for the pill tooltip.
pub fn pr_state_label(state: &VcsStatusRemoteResultPrState) -> &'static str {
    match state {
        VcsStatusRemoteResultPrState::Open => "open",
        VcsStatusRemoteResultPrState::Closed => "closed",
        VcsStatusRemoteResultPrState::Merged => "merged",
        VcsStatusRemoteResultPrState::Unknown(_) => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitre_contracts::{NonNegativeInt, TrimmedNonEmptyString};

    fn tnes(value: &str) -> TrimmedNonEmptyString {
        TrimmedNonEmptyString(value.to_string())
    }

    fn git_ref(name: &str) -> VcsRef {
        VcsRef {
            current: false,
            is_default: false,
            is_remote: None,
            name: tnes(name),
            remote_name: None,
            worktree_path: None,
        }
    }

    fn page(refs: Vec<VcsRef>, next_cursor: Option<i64>, total: i64) -> VcsListRefsResult {
        VcsListRefsResult {
            has_primary_remote: true,
            is_repo: true,
            next_cursor: next_cursor.map(NonNegativeInt),
            refs,
            total_count: NonNegativeInt(total),
        }
    }

    #[test]
    fn toolbar_value_prefers_git_branch_outside_base_mode() {
        assert_eq!(
            resolve_branch_toolbar_value(false, Some("thread"), Some("git")),
            Some("git".to_string())
        );
        assert_eq!(
            resolve_branch_toolbar_value(false, Some("thread"), None),
            Some("thread".to_string())
        );
        // Worktree-base mode: the picked base (thread branch) wins.
        assert_eq!(
            resolve_branch_toolbar_value(true, Some("thread"), Some("git")),
            Some("thread".to_string())
        );
        assert_eq!(resolve_branch_toolbar_value(true, None, None), None);
    }

    #[test]
    fn trigger_label_quirk_strings() {
        assert_eq!(branch_trigger_label(false, None), "Select ref");
        assert_eq!(branch_trigger_label(true, Some("main")), "From main");
        assert_eq!(branch_trigger_label(false, Some("main")), "main");
    }

    #[test]
    fn badge_precedence_first_match_wins() {
        let mut r = git_ref("feat/x");
        r.current = true;
        r.is_default = true;
        assert_eq!(resolve_ref_badge(&r, "/repo"), Some(RefBadge::Current));

        let mut r = git_ref("feat/x");
        r.worktree_path = Some(tnes("/trees/x"));
        r.is_remote = Some(Some(true));
        assert_eq!(resolve_ref_badge(&r, "/repo"), Some(RefBadge::Worktree));

        // A worktree that IS the project checkout doesn't badge as worktree.
        let mut r = git_ref("main");
        r.worktree_path = Some(tnes("/repo"));
        r.is_default = true;
        assert_eq!(resolve_ref_badge(&r, "/repo"), Some(RefBadge::Default));

        let mut r = git_ref("origin/x");
        r.is_remote = Some(Some(true));
        assert_eq!(resolve_ref_badge(&r, "/repo"), Some(RefBadge::Remote));

        assert_eq!(resolve_ref_badge(&git_ref("plain"), "/repo"), None);
    }

    #[test]
    fn picker_filter_is_case_insensitive_substring() {
        assert!(should_include_branch_picker_item("", "anything"));
        assert!(should_include_branch_picker_item("  ", "anything"));
        assert!(should_include_branch_picker_item("FEAT", "feat/vitre"));
        assert!(!should_include_branch_picker_item("fix", "feat/vitre"));
    }

    #[test]
    fn create_item_rules() {
        let refs = vec![git_ref("main"), git_ref("feat/x")];
        assert_eq!(
            create_item_query(false, " new-branch ", &refs),
            Some("new-branch".to_string())
        );
        // Exact match suppresses the row; substring does not.
        assert_eq!(create_item_query(false, "main", &refs), None);
        assert_eq!(
            create_item_query(false, "mai", &refs),
            Some("mai".to_string())
        );
        assert_eq!(create_item_query(false, "", &refs), None);
        assert_eq!(create_item_query(true, "new-branch", &refs), None);
        assert_eq!(
            create_item_label("new-branch"),
            "Create new ref \"new-branch\""
        );
    }

    #[test]
    fn merge_keeps_first_seen_order_and_later_pages_win() {
        let mut moved = git_ref("b");
        moved.current = true;
        let pages = vec![
            page(vec![git_ref("a"), git_ref("b")], Some(2), 5),
            page(vec![moved.clone(), git_ref("c")], None, 4),
        ];
        let merged = merge_ref_pages(&pages);
        let names: Vec<&str> = merged
            .refs
            .iter()
            .map(|git_ref| git_ref.name.0.as_str())
            .collect();
        assert_eq!(names, vec!["a", "b", "c"]);
        // "b" kept its slot but took the later page's data.
        assert!(merged.refs[1].current);
        // next_cursor from the LAST page; total is the max across pages.
        assert_eq!(merged.next_cursor, None);
        assert_eq!(merged.total_count, 5);
    }

    #[test]
    fn status_text_precedence() {
        assert_eq!(
            refs_status_text(true, true, true, 3, 9),
            Some("Loading refs...".to_string())
        );
        assert_eq!(
            refs_status_text(false, true, true, 3, 9),
            Some("Loading more refs...".to_string())
        );
        assert_eq!(
            refs_status_text(false, false, true, 3, 9),
            Some("Showing 3 of 9 refs".to_string())
        );
        assert_eq!(refs_status_text(false, false, false, 9, 9), None);
    }

    #[test]
    fn selection_target_reuses_existing_worktrees() {
        let mut r = git_ref("feat/x");
        r.worktree_path = Some(tnes("/trees/x"));
        assert_eq!(
            resolve_branch_selection_target(&r, "/repo", None),
            BranchSelectionTarget::Reuse {
                next_worktree_path: Some("/trees/x".to_string())
            }
        );
        // A "worktree" that is the main checkout clears the thread worktree.
        r.worktree_path = Some(tnes("/repo"));
        assert_eq!(
            resolve_branch_selection_target(&r, "/repo", Some("/trees/old")),
            BranchSelectionTarget::Reuse {
                next_worktree_path: None
            }
        );
    }

    #[test]
    fn selection_target_checkout_paths() {
        // Plain ref from the main checkout: checkout in the project root.
        assert_eq!(
            resolve_branch_selection_target(&git_ref("feat/x"), "/repo", None),
            BranchSelectionTarget::Checkout {
                checkout_cwd: "/repo".to_string(),
                next_worktree_path: None
            }
        );
        // Plain ref from inside a worktree: stay in the worktree.
        assert_eq!(
            resolve_branch_selection_target(&git_ref("feat/x"), "/repo", Some("/trees/x")),
            BranchSelectionTarget::Checkout {
                checkout_cwd: "/trees/x".to_string(),
                next_worktree_path: Some("/trees/x".to_string())
            }
        );
        // The default ref from inside a worktree hops back to the checkout.
        let mut main = git_ref("main");
        main.is_default = true;
        assert_eq!(
            resolve_branch_selection_target(&main, "/repo", Some("/trees/x")),
            BranchSelectionTarget::Checkout {
                checkout_cwd: "/repo".to_string(),
                next_worktree_path: None
            }
        );
    }

    #[test]
    fn locked_labels() {
        assert_eq!(locked_workspace_label(true), "Worktree");
        assert_eq!(locked_workspace_label(false), "Local checkout");
    }
}
