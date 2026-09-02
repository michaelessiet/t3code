//! Sidebar project grouping: port of
//! `packages/client-runtime/src/state/projectGrouping.ts` plus the
//! snapshot-building half of `apps/web/src/sidebarProjectGrouping.ts`.
//!
//! Two projects that check out the same repository — a worktree and its main
//! tree, or two clones — collapse into one sidebar row. The row's identity is
//! the *logical* key, derived from the repository identity the server stamps
//! on each project; the *physical* key (one per workspace root) stays the unit
//! of deduplication and of the per-project grouping override.
//!
//! ## Single environment
//!
//! Electron groups across environments (local + remote backends), so its
//! snapshots carry `environmentPresence`, remote environment labels and a
//! primary-environment tiebreak. Vitre talks to exactly one backend — the
//! bundled sidecar — so every project is local: the presence is always
//! "local-only", the remote badge never renders, and the primary-environment
//! arm of the duplicate tiebreak can never fire. The environment half of each
//! key is still emitted ([`LOCAL_ENVIRONMENT_ID`]) so the key shape, and with
//! it the override-key shape, matches the TS layer exactly.

use std::collections::{HashMap, HashSet};

use vitre_contracts::OrchestrationProjectShell;

use crate::wire_opt::defined3;

/// Environment half of every project key. Vitre is single-environment (see
/// the module docs), so this is a constant rather than a real environment id.
pub const LOCAL_ENVIRONMENT_ID: &str = "local";

/// `SidebarProjectGroupingMode` (`packages/contracts/src/settings.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ProjectGroupingMode {
    /// Every project sharing a repository shares a row.
    #[default]
    Repository,
    /// Projects group only when repository *and* repo-relative path match.
    RepositoryPath,
    /// Every project path gets its own row.
    Separate,
}

impl ProjectGroupingMode {
    pub const ALL: [Self; 3] = [Self::Repository, Self::RepositoryPath, Self::Separate];

    /// The wire/settings literal, so persisted values round-trip with
    /// Electron's `settings.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::RepositoryPath => "repository_path",
            Self::Separate => "separate",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "repository" => Some(Self::Repository),
            "repository_path" => Some(Self::RepositoryPath),
            "separate" => Some(Self::Separate),
            _ => None,
        }
    }

    /// The menu label Electron shows for this mode (`Sidebar.tsx`'s
    /// `PROJECT_GROUPING_MODE_LABELS`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Repository => "Group by repository",
            Self::RepositoryPath => "Group by repository path",
            Self::Separate => "Keep separate",
        }
    }

    /// `projectGroupingModeDescription` (`Sidebar.tsx`).
    pub fn description(self) -> &'static str {
        match self {
            Self::Repository => "Projects from the same repository share one sidebar row.",
            Self::RepositoryPath => {
                "Projects group only when both the repository and repo-relative path match."
            }
            Self::Separate => "Every project path gets its own sidebar row.",
        }
    }
}

/// `ProjectGroupingSettings` — the global mode plus per-physical-key
/// overrides.
#[derive(Debug, Clone, Default)]
pub struct ProjectGroupingSettings {
    pub mode: ProjectGroupingMode,
    pub overrides: HashMap<String, ProjectGroupingMode>,
}

impl ProjectGroupingSettings {
    /// `resolveProjectGroupingMode`: the override for this project's physical
    /// key, else the global mode.
    pub fn resolve(&self, project: &OrchestrationProjectShell) -> ProjectGroupingMode {
        self.overrides
            .get(&derive_physical_project_key(project))
            .copied()
            .unwrap_or(self.mode)
    }
}

/// One sidebar row: the logical project and the physical projects behind it.
///
/// Members are indices into the slice passed to [`build_project_groups`] —
/// project shells are large and the caller already owns them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectGroup {
    /// Logical key: the row's identity (and its expansion-preference key).
    pub key: String,
    /// Label shown on the row: the shared repository name for a real group,
    /// the representative's title for a group of one.
    pub display_name: String,
    /// Index of the project whose fields the row inherits (workspace root,
    /// timestamps, title).
    pub representative: usize,
    /// Indices of the deduplicated member projects, in first-seen order.
    pub members: Vec<usize>,
}

impl ProjectGroup {
    /// `groupedProjectCount` — drives the "{n} projects" chip.
    pub fn grouped_project_count(&self) -> usize {
        self.members.len()
    }
}

fn is_windows_drive_path(value: &str) -> bool {
    let mut chars = value.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic() => {
            matches!(chars.next(), None | Some('/') | Some('\\'))
        }
        _ => false,
    }
}

fn is_unc_path(value: &str) -> bool {
    value.starts_with("\\\\")
}

fn is_root_path(value: &str) -> bool {
    if value == "/" || value == "\\" {
        return true;
    }
    // /^[a-zA-Z]:[/\\]?$/
    let mut chars = value.chars();
    matches!((chars.next(), chars.next()), (Some(letter), Some(':')) if letter.is_ascii_alphabetic())
        && matches!(chars.next(), None | Some('/') | Some('\\'))
        && chars.next().is_none()
}

fn trim_trailing_path_separators(value: &str) -> String {
    if value.is_empty() || is_root_path(value) {
        return value.to_owned();
    }
    // A POSIX-absolute path only sheds `/`; anything else sheds both
    // separators, matching the TS regex pair.
    let trimmed = if value.starts_with('/') {
        value.trim_end_matches('/')
    } else {
        value.trim_end_matches(['\\', '/'])
    };
    if trimmed.is_empty() {
        return value.to_owned();
    }
    // "C:" would otherwise stop being a path; the TS layer re-roots it.
    let mut chars = trimmed.chars();
    let bare_drive = matches!((chars.next(), chars.next(), chars.next()), (Some(letter), Some(':'), None) if letter.is_ascii_alphabetic());
    if bare_drive {
        format!("{trimmed}\\")
    } else {
        trimmed.to_owned()
    }
}

/// `normalizeProjectPathForComparison` (`packages/shared/src/path.ts`).
pub fn normalize_project_path_for_comparison(value: &str) -> String {
    let normalized = trim_trailing_path_separators(value.trim());
    if is_windows_drive_path(&normalized) || is_unc_path(&normalized) {
        return normalized.replace('/', "\\").to_lowercase();
    }
    normalized
}

/// `derivePhysicalProjectKey`: one key per workspace root.
pub fn derive_physical_project_key(project: &OrchestrationProjectShell) -> String {
    format!(
        "{LOCAL_ENVIRONMENT_ID}:{}",
        normalize_project_path_for_comparison(&project.workspace_root.0)
    )
}

fn repository_identity(
    project: &OrchestrationProjectShell,
) -> Option<&vitre_contracts::RepositoryIdentity> {
    defined3(&project.repository_identity).flatten()
}

/// `deriveRepositoryRelativeProjectPath`: where this project sits inside its
/// repository. `Some("")` means it *is* the repository root; `None` means the
/// repository root is unknown or does not contain the project.
fn derive_repository_relative_project_path(project: &OrchestrationProjectShell) -> Option<String> {
    let root_path = repository_identity(project)?.root_path.as_ref()?.0.trim();
    if root_path.is_empty() {
        return None;
    }
    let normalized_project_path = normalize_project_path_for_comparison(&project.workspace_root.0);
    let normalized_root_path = normalize_project_path_for_comparison(root_path);
    if normalized_project_path.is_empty() || normalized_root_path.is_empty() {
        return None;
    }
    if normalized_project_path == normalized_root_path {
        return Some(String::new());
    }
    let separator = if normalized_root_path.contains('\\') {
        '\\'
    } else {
        '/'
    };
    let root_prefix = format!("{normalized_root_path}{separator}");
    let relative = normalized_project_path.strip_prefix(&root_prefix)?;
    Some(relative.replace('\\', "/"))
}

fn derive_repository_scoped_key(
    project: &OrchestrationProjectShell,
    mode: ProjectGroupingMode,
) -> Option<String> {
    let canonical_key = &repository_identity(project)?.canonical_key.0;
    if canonical_key.is_empty() {
        return None;
    }
    if mode == ProjectGroupingMode::Repository {
        return Some(canonical_key.clone());
    }
    match derive_repository_relative_project_path(project) {
        // Repository root, or a root we cannot resolve: the repository key is
        // as specific as this mode can get.
        None => Some(canonical_key.clone()),
        Some(relative) if relative.is_empty() => Some(canonical_key.clone()),
        Some(relative) => Some(format!("{canonical_key}::{relative}")),
    }
}

/// `deriveLogicalProjectKey`: the sidebar row a project belongs to.
pub fn derive_logical_project_key(
    project: &OrchestrationProjectShell,
    mode: ProjectGroupingMode,
) -> String {
    if mode == ProjectGroupingMode::Separate {
        return derive_physical_project_key(project);
    }
    derive_repository_scoped_key(project, mode)
        .unwrap_or_else(|| derive_physical_project_key(project))
}

fn unique_non_empty<'a>(values: impl Iterator<Item = Option<&'a str>>) -> Vec<&'a str> {
    let mut unique: Vec<&str> = Vec::new();
    for value in values.flatten() {
        let trimmed = value.trim();
        if trimmed.is_empty() || unique.contains(&trimmed) {
            continue;
        }
        unique.push(trimmed);
    }
    unique
}

/// `deriveProjectGroupLabel`: one shared repository display name, else one
/// shared repository name, else the representative's own title.
pub fn derive_project_group_label(
    representative: &OrchestrationProjectShell,
    members: &[&OrchestrationProjectShell],
) -> String {
    let display_names = unique_non_empty(members.iter().map(|member| {
        repository_identity(member).and_then(|identity| {
            identity
                .display_name
                .as_ref()
                .map(|display_name| display_name.0.as_str())
        })
    }));
    if let [only] = display_names[..] {
        return only.to_owned();
    }
    let names = unique_non_empty(members.iter().map(|member| {
        repository_identity(member).and_then(|identity| {
            identity
                .name
                .as_ref()
                .map(|repository_name| repository_name.0.as_str())
        })
    }));
    if let [only] = names[..] {
        return only.to_owned();
    }
    representative.title.0.clone()
}

fn freshness_ms(project: &OrchestrationProjectShell) -> i64 {
    crate::sidebar::parse_timestamp_ms(&project.updated_at.0)
        .or_else(|| crate::sidebar::parse_timestamp_ms(&project.created_at.0))
        .unwrap_or(0)
}

/// `shouldReplaceDuplicateMember`, minus the primary-environment arm (see the
/// module docs): the fresher project wins a physical-key collision, ties going
/// to the larger id.
fn should_replace_duplicate(
    existing: &OrchestrationProjectShell,
    candidate: &OrchestrationProjectShell,
) -> bool {
    let existing_freshness = freshness_ms(existing);
    let candidate_freshness = freshness_ms(candidate);
    if candidate_freshness != existing_freshness {
        return candidate_freshness > existing_freshness;
    }
    candidate.id.0 > existing.id.0
}

/// `collectProjectWinnersByPhysicalKey`, in first-seen physical-key order (the
/// TS layer relies on JS `Map` insertion order for the member order downstream).
fn collect_winners(projects: &[OrchestrationProjectShell]) -> Vec<(String, usize)> {
    let mut winners: Vec<(String, usize)> = Vec::new();
    let mut slot_by_key: HashMap<String, usize> = HashMap::new();
    for (index, project) in projects.iter().enumerate() {
        let physical_key = derive_physical_project_key(project);
        match slot_by_key.get(&physical_key) {
            None => {
                slot_by_key.insert(physical_key.clone(), winners.len());
                winners.push((physical_key, index));
            }
            Some(&slot) => {
                if should_replace_duplicate(&projects[winners[slot].1], project) {
                    winners[slot].1 = index;
                }
            }
        }
    }
    winners
}

/// `buildSidebarProjectSnapshots`: deduplicate by physical key, group by
/// logical key, and emit one row per group in first-appearance order.
pub fn build_project_groups(
    projects: &[OrchestrationProjectShell],
    settings: &ProjectGroupingSettings,
) -> Vec<ProjectGroup> {
    let winners = collect_winners(projects);

    // logical key -> winner indices, in winner order.
    let mut members_by_logical: HashMap<String, Vec<usize>> = HashMap::new();
    for (_, index) in &winners {
        let project = &projects[*index];
        let logical_key = derive_logical_project_key(project, settings.resolve(project));
        members_by_logical
            .entry(logical_key)
            .or_default()
            .push(*index);
    }

    let mut groups = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for project in projects {
        let logical_key = derive_logical_project_key(project, settings.resolve(project));
        if !seen.insert(logical_key.clone()) {
            continue;
        }

        let Some(members) = members_by_logical.get(&logical_key) else {
            continue;
        };
        // Single environment: the representative is simply the first member
        // (Electron prefers the primary-environment member first).
        let Some(&representative) = members.first() else {
            continue;
        };
        let member_refs: Vec<&OrchestrationProjectShell> =
            members.iter().map(|index| &projects[*index]).collect();
        let display_name = if members.len() > 1 {
            derive_project_group_label(&projects[representative], &member_refs)
        } else {
            projects[representative].title.0.clone()
        };
        groups.push(ProjectGroup {
            key: logical_key,
            display_name,
            representative,
            members: members.clone(),
        });
    }
    groups
}

/// `projectExpansionPreferenceKeys`: the keys a row's expanded/collapsed
/// preference is looked up under, most specific first. Keeping the physical
/// keys in the chain is what makes a row stay collapsed when the grouping mode
/// changes underneath it.
///
/// Electron appends a third tier of `legacy-project-cwd:` keys so decisions
/// migrated out of a pre-`projectExpandedById` localStorage layout still
/// resolve. Vitre's store was born with the current shape, so there is nothing
/// to migrate and the tier is omitted.
pub fn expansion_preference_keys(
    group: &ProjectGroup,
    projects: &[OrchestrationProjectShell],
) -> Vec<String> {
    let mut keys = vec![group.key.clone()];
    keys.extend(
        group
            .members
            .iter()
            .map(|index| derive_physical_project_key(&projects[*index])),
    );
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitre_contracts::{
        ProjectId, RepositoryIdentity, RepositoryIdentityLocator, RepositoryIdentityLocatorSource,
        TrimmedNonEmptyString,
    };

    fn tnes(value: &str) -> TrimmedNonEmptyString {
        TrimmedNonEmptyString(value.to_owned())
    }

    fn identity(
        canonical_key: &str,
        root_path: Option<&str>,
        display_name: Option<&str>,
        name: Option<&str>,
    ) -> RepositoryIdentity {
        RepositoryIdentity {
            canonical_key: tnes(canonical_key),
            display_name: display_name.map(tnes),
            locator: RepositoryIdentityLocator {
                remote_name: tnes("origin"),
                remote_url: tnes("git@example.com:acme/repo.git"),
                source: RepositoryIdentityLocatorSource::GitRemote,
            },
            name: name.map(tnes),
            owner: None,
            provider: None,
            root_path: root_path.map(tnes),
        }
    }

    fn project(
        id: &str,
        workspace_root: &str,
        title: &str,
        identity: Option<RepositoryIdentity>,
    ) -> OrchestrationProjectShell {
        OrchestrationProjectShell {
            additional_roots: None,
            created_at: tnes("2026-01-01T00:00:00.000Z"),
            default_model_selection: None,
            id: ProjectId(id.to_owned()),
            repository_identity: identity.map(|identity| Some(Some(identity))),
            resolved_additional_roots: None,
            scripts: Vec::new(),
            title: tnes(title),
            updated_at: tnes("2026-01-01T00:00:00.000Z"),
            workspace_root: tnes(workspace_root),
        }
    }

    fn keys(groups: &[ProjectGroup]) -> Vec<&str> {
        groups.iter().map(|group| group.key.as_str()).collect()
    }

    #[test]
    fn normalizes_trailing_separators_and_windows_case() {
        assert_eq!(
            normalize_project_path_for_comparison("/repo/app/"),
            "/repo/app"
        );
        assert_eq!(
            normalize_project_path_for_comparison("  /repo/app  "),
            "/repo/app"
        );
        assert_eq!(normalize_project_path_for_comparison("/"), "/");
        assert_eq!(
            normalize_project_path_for_comparison("C:/Repo/App/"),
            "c:\\repo\\app"
        );
        // A bare drive is already a root, so it keeps its shape; only a drive
        // that sheds separators gets re-rooted.
        assert_eq!(normalize_project_path_for_comparison("C:"), "c:");
        assert_eq!(normalize_project_path_for_comparison("C:\\\\"), "c:\\");
    }

    #[test]
    fn projects_without_repository_identity_stay_separate() {
        let projects = vec![
            project("a", "/repo/one", "one", None),
            project("b", "/repo/two", "two", None),
        ];
        let groups = build_project_groups(&projects, &ProjectGroupingSettings::default());
        assert_eq!(keys(&groups), vec!["local:/repo/one", "local:/repo/two"]);
    }

    #[test]
    fn repository_mode_collapses_worktrees_into_one_row() {
        let projects = vec![
            project(
                "a",
                "/repo",
                "repo",
                Some(identity(
                    "github.com/acme/repo",
                    Some("/repo"),
                    Some("acme/repo"),
                    Some("repo"),
                )),
            ),
            project(
                "b",
                "/worktrees/feature",
                "feature",
                Some(identity(
                    "github.com/acme/repo",
                    Some("/repo"),
                    Some("acme/repo"),
                    Some("repo"),
                )),
            ),
        ];
        let groups = build_project_groups(&projects, &ProjectGroupingSettings::default());
        assert_eq!(keys(&groups), vec!["github.com/acme/repo"]);
        assert_eq!(groups[0].grouped_project_count(), 2);
        // Both members share one repository display name, so the row takes it
        // instead of the representative's own title.
        assert_eq!(groups[0].display_name, "acme/repo");
    }

    #[test]
    fn repository_path_mode_splits_packages_of_one_repository() {
        let settings = ProjectGroupingSettings {
            mode: ProjectGroupingMode::RepositoryPath,
            overrides: HashMap::new(),
        };
        let projects = vec![
            project(
                "a",
                "/repo",
                "repo",
                Some(identity("github.com/acme/repo", Some("/repo"), None, None)),
            ),
            project(
                "b",
                "/repo/apps/web",
                "web",
                Some(identity("github.com/acme/repo", Some("/repo"), None, None)),
            ),
        ];
        let groups = build_project_groups(&projects, &settings);
        assert_eq!(
            keys(&groups),
            vec!["github.com/acme/repo", "github.com/acme/repo::apps/web"]
        );
    }

    #[test]
    fn separate_mode_and_per_project_overrides_ungroup_a_row() {
        let projects = vec![
            project(
                "a",
                "/repo",
                "repo",
                Some(identity("github.com/acme/repo", Some("/repo"), None, None)),
            ),
            project(
                "b",
                "/worktrees/feature",
                "feature",
                Some(identity("github.com/acme/repo", Some("/repo"), None, None)),
            ),
        ];
        let separate = ProjectGroupingSettings {
            mode: ProjectGroupingMode::Separate,
            overrides: HashMap::new(),
        };
        assert_eq!(
            keys(&build_project_groups(&projects, &separate)),
            vec!["local:/repo", "local:/worktrees/feature"]
        );

        // An override applies to one physical key only: the overridden project
        // leaves the group, the other keeps the repository row.
        let overridden = ProjectGroupingSettings {
            mode: ProjectGroupingMode::Repository,
            overrides: HashMap::from([(
                "local:/worktrees/feature".to_owned(),
                ProjectGroupingMode::Separate,
            )]),
        };
        assert_eq!(
            keys(&build_project_groups(&projects, &overridden)),
            vec!["github.com/acme/repo", "local:/worktrees/feature"]
        );
    }

    #[test]
    fn duplicate_workspace_roots_collapse_to_the_freshest_project() {
        let mut stale = project("a", "/repo/app", "stale", None);
        stale.updated_at = tnes("2026-01-01T00:00:00.000Z");
        let mut fresh = project("b", "/repo/app/", "fresh", None);
        fresh.updated_at = tnes("2026-02-01T00:00:00.000Z");
        let projects = vec![stale, fresh];

        let groups = build_project_groups(&projects, &ProjectGroupingSettings::default());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].grouped_project_count(), 1);
        assert_eq!(projects[groups[0].representative].title.0, "fresh");
    }

    #[test]
    fn group_label_falls_back_through_name_then_representative_title() {
        // Members disagree on display name but share a repository name.
        let projects = vec![
            project(
                "a",
                "/repo",
                "repo",
                Some(identity("key", Some("/repo"), Some("Acme"), Some("repo"))),
            ),
            project(
                "b",
                "/worktrees/feature",
                "feature",
                Some(identity(
                    "key",
                    Some("/repo"),
                    Some("Acme Fork"),
                    Some("repo"),
                )),
            ),
        ];
        let groups = build_project_groups(&projects, &ProjectGroupingSettings::default());
        assert_eq!(groups[0].display_name, "repo");

        // Nothing shared: the representative's title wins.
        let projects = vec![
            project(
                "a",
                "/repo",
                "repo",
                Some(identity("key", Some("/repo"), None, Some("one"))),
            ),
            project(
                "b",
                "/worktrees/feature",
                "feature",
                Some(identity("key", Some("/repo"), None, Some("two"))),
            ),
        ];
        let groups = build_project_groups(&projects, &ProjectGroupingSettings::default());
        assert_eq!(groups[0].display_name, "repo");
    }

    #[test]
    fn expansion_preference_keys_run_logical_then_physical() {
        let projects = vec![
            project(
                "a",
                "/repo",
                "repo",
                Some(identity("key", Some("/repo"), None, None)),
            ),
            project(
                "b",
                "/worktrees/feature",
                "feature",
                Some(identity("key", Some("/repo"), None, None)),
            ),
        ];
        let groups = build_project_groups(&projects, &ProjectGroupingSettings::default());
        assert_eq!(
            expansion_preference_keys(&groups[0], &projects),
            vec!["key", "local:/repo", "local:/worktrees/feature"]
        );
    }
}
