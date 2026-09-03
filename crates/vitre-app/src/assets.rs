//! Vitre's asset source: the fork's icon set plus the handful of Lucide
//! icons Electron uses that the fork does not ship (ISC-licensed, vendored
//! under `icons/`). Serving them from here keeps the vendored fork pristine.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};
use gpui_component::IconNamed;

pub struct VitreAssets;

const EXTRA_ICONS: &[(&str, &[u8])] = &[
    (
        "icons/file-diff.svg",
        include_bytes!("../icons/file-diff.svg"),
    ),
    ("icons/files.svg", include_bytes!("../icons/files.svg")),
    (
        "icons/text-search.svg",
        include_bytes!("../icons/text-search.svg"),
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
    FileDiff,
    Files,
    TextSearch,
}

impl IconNamed for VitreIcon {
    fn path(self) -> SharedString {
        match self {
            Self::FileDiff => "icons/file-diff.svg".into(),
            Self::Files => "icons/files.svg".into(),
            Self::TextSearch => "icons/text-search.svg".into(),
        }
    }
}
