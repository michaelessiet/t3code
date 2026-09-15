//! The add-project flow's model: sources, provider readiness, and the stage
//! machine, ported from the add-project half of
//! `apps/web/src/components/CommandPalette.tsx`.
//!
//! Electron layers this flow over its command palette's view stack; the view
//! itself lives in [`super::command_palette`]. What Electron spreads over an
//! environments picker collapses here: Vitre speaks to exactly one local
//! sidecar, so the flow starts at the Sources view (`openAddProjectFlow` with
//! one environment option does the same).

use gpui_component::IconName;
use vitre_contracts::{
    SourceControlDiscoveryResult, SourceControlDiscoveryStatus, SourceControlProviderAuthStatus,
    SourceControlProviderKind, SourceControlRepositoryInfo,
};

/// Electron's `AddProjectRemoteSource`: where a clone comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RemoteSource {
    Url,
    Github,
    Gitlab,
    Bitbucket,
    AzureDevops,
}

/// `REMOTE_PROJECT_PROVIDER_SOURCES`, the sources that need a configured
/// provider; `Url` is always offered first and always ready.
pub(super) const PROVIDER_SOURCES: [RemoteSource; 4] = [
    RemoteSource::Github,
    RemoteSource::Gitlab,
    RemoteSource::Bitbucket,
    RemoteSource::AzureDevops,
];

impl RemoteSource {
    /// `remoteProjectSourceLabel`.
    pub(super) fn label(self) -> &'static str {
        match self {
            RemoteSource::Url => "Git URL",
            RemoteSource::Github => "GitHub",
            RemoteSource::Gitlab => "GitLab",
            RemoteSource::Bitbucket => "Bitbucket",
            RemoteSource::AzureDevops => "Azure DevOps",
        }
    }

    /// `remoteProjectSourcePathHint`: the repository shorthand each provider
    /// expects.
    pub(super) fn path_hint(self) -> &'static str {
        match self {
            RemoteSource::Url => "clone URL",
            RemoteSource::Github => "owner/repo",
            RemoteSource::Gitlab => "group/project",
            RemoteSource::Bitbucket => "workspace/repository",
            RemoteSource::AzureDevops => "project/repository",
        }
    }

    pub(super) fn provider_kind(self) -> Option<SourceControlProviderKind> {
        match self {
            RemoteSource::Url => None,
            RemoteSource::Github => Some(SourceControlProviderKind::Github),
            RemoteSource::Gitlab => Some(SourceControlProviderKind::Gitlab),
            RemoteSource::Bitbucket => Some(SourceControlProviderKind::Bitbucket),
            RemoteSource::AzureDevops => Some(SourceControlProviderKind::AzureDevops),
        }
    }

    pub(super) fn icon(self) -> IconName {
        match self {
            RemoteSource::Url => IconName::Link,
            RemoteSource::Github => IconName::Github,
            RemoteSource::Gitlab => IconName::Gitlab,
            RemoteSource::Bitbucket => IconName::Bitbucket,
            RemoteSource::AzureDevops => IconName::AzureDevops,
        }
    }

    /// The clone repository step's input placeholder
    /// (`remoteProjectInputPlaceholder`).
    pub(super) fn repository_placeholder(self) -> String {
        match self {
            RemoteSource::Url => "Enter Git clone URL".to_owned(),
            other => format!("Enter {} repository ({})", other.label(), other.path_hint()),
        }
    }

    /// The clone repository step's empty-state copy.
    pub(super) fn repository_empty_copy(self) -> &'static str {
        match self {
            RemoteSource::Url => "Enter a Git clone URL and press Enter to continue.",
            _ => "Enter a repository path and press Enter to look it up.",
        }
    }
}

/// Where the flow currently is. Electron models this as a view stack plus an
/// `AddProjectCloneFlow`; one enum covers both because every stage pops back
/// to [`AddProjectStage::Sources`], exactly as Electron's `popView` does.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum AddProjectStage {
    /// The Sources list: Local folder, Git URL, and the provider sources.
    Sources,
    /// In-palette filesystem browsing seeded from `addProjectBaseDirectory`.
    Browse,
    /// Clone flow, `step: "repository"` — enter a URL or `owner/repo`.
    CloneRepository { source: RemoteSource },
    /// Clone flow, `step: "confirm"` — browse for the clone destination.
    CloneDestination {
        source: RemoteSource,
        /// What the user typed on the repository step.
        repository_input: String,
        /// The lookup answer; `None` for the raw-URL source.
        repository: Option<SourceControlRepositoryInfo>,
        remote_url: String,
    },
}

/// One provider row's gating (`buildAddProjectRemoteSourceReadiness`).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SourceReadiness {
    pub ready: bool,
    /// Rendered as the `Setup Required` badge's tooltip when unready.
    pub hint: Option<String>,
}

pub(super) fn source_readiness(
    discovery: Option<&SourceControlDiscoveryResult>,
    source: RemoteSource,
) -> SourceReadiness {
    let ready = SourceReadiness {
        ready: true,
        hint: None,
    };
    let Some(kind) = source.provider_kind() else {
        return ready;
    };
    let unavailable = || SourceReadiness {
        ready: false,
        hint: Some(
            "Provider status unavailable. Open Settings -> Source Control and rescan.".to_owned(),
        ),
    };
    let Some(discovery) = discovery else {
        return unavailable();
    };
    let Some(provider) = discovery
        .source_control_providers
        .iter()
        .find(|provider| provider.kind == kind)
    else {
        return unavailable();
    };
    if provider.status != SourceControlDiscoveryStatus::Available {
        return SourceReadiness {
            ready: false,
            hint: Some(provider.install_hint.0.clone()),
        };
    }
    if provider.auth.status == SourceControlProviderAuthStatus::Unauthenticated {
        return SourceReadiness {
            ready: false,
            hint: Some(provider.auth.detail.clone().into_option().unwrap_or_else(|| {
                format!(
                    "{} is not authenticated. Open Settings -> Source Control for setup guidance.",
                    provider.label.0
                )
            })),
        };
    }
    // `unknown` auth counts as ready, as it does in Electron.
    ready
}

/// `sortAddProjectProviderSources`: ready sources first, ties broken
/// alphabetically by label.
pub(super) fn ordered_provider_sources(
    discovery: Option<&SourceControlDiscoveryResult>,
) -> Vec<(RemoteSource, SourceReadiness)> {
    let mut sources: Vec<(RemoteSource, SourceReadiness)> = PROVIDER_SOURCES
        .into_iter()
        .map(|source| (source, source_readiness(discovery, source)))
        .collect();
    sources.sort_by(|(a, a_ready), (b, b_ready)| {
        b_ready
            .ready
            .cmp(&a_ready.ready)
            .then_with(|| a.label().cmp(b.label()))
    });
    sources
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitre_contracts::{
        EffectOption, SourceControlProviderAuth, SourceControlProviderDiscoveryItem,
        TrimmedNonEmptyString,
    };

    fn provider(
        kind: SourceControlProviderKind,
        label: &str,
        status: SourceControlDiscoveryStatus,
        auth: SourceControlProviderAuthStatus,
        auth_detail: Option<&str>,
    ) -> SourceControlProviderDiscoveryItem {
        SourceControlProviderDiscoveryItem {
            auth: SourceControlProviderAuth {
                account: EffectOption::None,
                detail: auth_detail.map(str::to_owned).into(),
                host: EffectOption::None,
                status: auth,
            },
            detail: EffectOption::None,
            executable: None,
            install_hint: TrimmedNonEmptyString(format!("Install {label}.")),
            kind,
            label: TrimmedNonEmptyString(label.to_owned()),
            status,
            version: EffectOption::None,
        }
    }

    fn discovery(
        providers: Vec<SourceControlProviderDiscoveryItem>,
    ) -> SourceControlDiscoveryResult {
        SourceControlDiscoveryResult {
            source_control_providers: providers,
            version_control_systems: Vec::new(),
        }
    }

    #[test]
    fn url_is_always_ready_and_discovery_gaps_are_not() {
        assert!(source_readiness(None, RemoteSource::Url).ready);
        let gap = source_readiness(None, RemoteSource::Github);
        assert!(!gap.ready);
        assert_eq!(
            gap.hint.as_deref(),
            Some("Provider status unavailable. Open Settings -> Source Control and rescan.")
        );
        // Present discovery that omits the provider reads the same way.
        let empty = discovery(Vec::new());
        assert!(!source_readiness(Some(&empty), RemoteSource::Gitlab).ready);
    }

    #[test]
    fn missing_binaries_surface_the_install_hint() {
        let discovery = discovery(vec![provider(
            SourceControlProviderKind::Github,
            "GitHub",
            SourceControlDiscoveryStatus::Missing,
            SourceControlProviderAuthStatus::UnknownX,
            None,
        )]);
        let readiness = source_readiness(Some(&discovery), RemoteSource::Github);
        assert!(!readiness.ready);
        assert_eq!(readiness.hint.as_deref(), Some("Install GitHub."));
    }

    #[test]
    fn unauthenticated_providers_fall_back_to_the_generic_hint() {
        let with_detail = discovery(vec![provider(
            SourceControlProviderKind::Github,
            "GitHub",
            SourceControlDiscoveryStatus::Available,
            SourceControlProviderAuthStatus::Unauthenticated,
            Some("Run gh auth login."),
        )]);
        assert_eq!(
            source_readiness(Some(&with_detail), RemoteSource::Github)
                .hint
                .as_deref(),
            Some("Run gh auth login.")
        );

        let without_detail = discovery(vec![provider(
            SourceControlProviderKind::Github,
            "GitHub",
            SourceControlDiscoveryStatus::Available,
            SourceControlProviderAuthStatus::Unauthenticated,
            None,
        )]);
        assert_eq!(
            source_readiness(Some(&without_detail), RemoteSource::Github)
                .hint
                .as_deref(),
            Some(
                "GitHub is not authenticated. Open Settings -> Source Control for setup guidance."
            )
        );
    }

    #[test]
    fn unknown_auth_status_counts_as_ready() {
        let discovery = discovery(vec![provider(
            SourceControlProviderKind::Github,
            "GitHub",
            SourceControlDiscoveryStatus::Available,
            SourceControlProviderAuthStatus::UnknownX,
            None,
        )]);
        assert!(source_readiness(Some(&discovery), RemoteSource::Github).ready);
    }

    #[test]
    fn providers_sort_ready_first_then_alphabetically() {
        // Only GitLab is ready: it leads, and the unready rest are
        // alphabetical — Azure DevOps, Bitbucket, GitHub.
        let discovery = discovery(vec![provider(
            SourceControlProviderKind::Gitlab,
            "GitLab",
            SourceControlDiscoveryStatus::Available,
            SourceControlProviderAuthStatus::Authenticated,
            None,
        )]);
        let order: Vec<&str> = ordered_provider_sources(Some(&discovery))
            .into_iter()
            .map(|(source, _)| source.label())
            .collect();
        assert_eq!(order, ["GitLab", "Azure DevOps", "Bitbucket", "GitHub"]);
    }
}
