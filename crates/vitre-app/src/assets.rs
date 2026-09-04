//! Vitre's asset source: the fork's icon set plus the handful of Lucide
//! icons Electron uses that the fork does not ship (ISC-licensed, vendored
//! under `icons/`). Serving them from here keeps the vendored fork pristine.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};
use gpui_component::IconNamed;

pub struct VitreAssets;

const EXTRA_ICONS: &[(&str, &[u8])] = &[
    (
        "icons/chevrons-down-up.svg",
        include_bytes!("../icons/chevrons-down-up.svg"),
    ),
    (
        "icons/cloud-upload.svg",
        include_bytes!("../icons/cloud-upload.svg"),
    ),
    (
        "icons/columns-2.svg",
        include_bytes!("../icons/columns-2.svg"),
    ),
    (
        "icons/git-branch-plus.svg",
        include_bytes!("../icons/git-branch-plus.svg"),
    ),
    (
        "icons/git-commit-horizontal.svg",
        include_bytes!("../icons/git-commit-horizontal.svg"),
    ),
    (
        "icons/git-pull-request.svg",
        include_bytes!("../icons/git-pull-request.svg"),
    ),
    (
        "icons/file-diff.svg",
        include_bytes!("../icons/file-diff.svg"),
    ),
    (
        "icons/folder-git.svg",
        include_bytes!("../icons/folder-git.svg"),
    ),
    ("icons/info.svg", include_bytes!("../icons/info.svg")),
    ("icons/files.svg", include_bytes!("../icons/files.svg")),
    ("icons/pilcrow.svg", include_bytes!("../icons/pilcrow.svg")),
    ("icons/rows-3.svg", include_bytes!("../icons/rows-3.svg")),
    (
        "icons/square-split-horizontal.svg",
        include_bytes!("../icons/square-split-horizontal.svg"),
    ),
    (
        "icons/square-split-vertical.svg",
        include_bytes!("../icons/square-split-vertical.svg"),
    ),
    (
        "icons/text-search.svg",
        include_bytes!("../icons/text-search.svg"),
    ),
    ("icons/trash-2.svg", include_bytes!("../icons/trash-2.svg")),
    (
        "icons/wrap-text.svg",
        include_bytes!("../icons/wrap-text.svg"),
    ),
];

impl AssetSource for VitreAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = EXTRA_ICONS.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_component_assets::Assets.list(path)
    }
}

/// The vendored icons, as a drop-in for `IconName`. The fork's blanket
/// `impl<T: IconNamed> From<T> for Icon` makes these usable anywhere an
/// `Into<Icon>` is expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VitreIcon {
    ChevronsDownUp,
    CloudUpload,
    Columns2,
    FileDiff,
    FolderGit,
    GitBranchPlus,
    GitCommitHorizontal,
    GitPullRequest,
    Files,
    Info,
    Pilcrow,
    Rows3,
    SquareSplitHorizontal,
    SquareSplitVertical,
    TextSearch,
    Trash2,
    WrapText,
}

impl IconNamed for VitreIcon {
    fn path(self) -> SharedString {
        match self {
            Self::ChevronsDownUp => "icons/chevrons-down-up.svg".into(),
            Self::CloudUpload => "icons/cloud-upload.svg".into(),
            Self::Columns2 => "icons/columns-2.svg".into(),
            Self::FileDiff => "icons/file-diff.svg".into(),
            Self::FolderGit => "icons/folder-git.svg".into(),
            Self::GitBranchPlus => "icons/git-branch-plus.svg".into(),
            Self::GitCommitHorizontal => "icons/git-commit-horizontal.svg".into(),
            Self::GitPullRequest => "icons/git-pull-request.svg".into(),
            Self::Files => "icons/files.svg".into(),
            Self::Info => "icons/info.svg".into(),
            Self::Pilcrow => "icons/pilcrow.svg".into(),
            Self::Rows3 => "icons/rows-3.svg".into(),
            Self::SquareSplitHorizontal => "icons/square-split-horizontal.svg".into(),
            Self::SquareSplitVertical => "icons/square-split-vertical.svg".into(),
            Self::TextSearch => "icons/text-search.svg".into(),
            Self::Trash2 => "icons/trash-2.svg".into(),
            Self::WrapText => "icons/wrap-text.svg".into(),
        }
    }
}
