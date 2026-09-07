//! The client-owned half of Electron's settings split.
//!
//! `packages/contracts/src/settings.ts` partitions settings with two comment
//! banners: `ClientSettings` are local to one client install and never leave
//! it, `ServerSettings` are server-authoritative and shared by every client
//! attached to that sidecar. Electron stores its client half as one JSON blob
//! (`t3code:client-settings:v1` in `localStorage`, or a file under the desktop
//! shell's user-data dir); Vitre stores the keys it honours in
//! `<home>/client-settings.json`, keyed exactly as Electron keys them so the
//! two files stay readable against the same contract.
//!
//! Every value here is a *global*: the fork's settings fields hand a renderer
//! `Fn(&App) -> T` / `Fn(T, &mut App)` closures with no view in scope, and a
//! global is the only thing both halves can reach. Views that must react to a
//! change (the editor, for vim mode and soft wrap) do so with
//! `cx.observe_global::<ClientSettings>`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "client-settings.json";

/// Where `vimMode` and `fileExplorerOpen` lived before there was a settings
/// screen. Read once, when no `client-settings.json` exists yet, so an
/// existing home keeps its vim preference across the upgrade.
const LEGACY_FILE_NAME: &str = "editor-state.json";

/// Electron's `AutoSaveDelayMs` bounds (`settings.ts`), enforced on the way in
/// so a hand-edited file cannot park the debounce at zero.
pub const MIN_AUTO_SAVE_DELAY_MS: u32 = 100;
pub const MAX_AUTO_SAVE_DELAY_MS: u32 = 10_000;

/// Which palette the window paints with.
///
/// Electron keeps this out of `ClientSettings` entirely — it is a raw
/// `"t3code:theme"` string in `localStorage` (`apps/web/src/hooks/useTheme.ts`)
/// mirrored to the shell — but the values and the `system` default are its
/// values, and one file is friendlier than two.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeSetting {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeSetting {
    pub const ALL: [ThemeSetting; 3] = [
        ThemeSetting::System,
        ThemeSetting::Light,
        ThemeSetting::Dark,
    ];

    /// The wire value, matching Electron's stored string.
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeSetting::System => "system",
            ThemeSetting::Light => "light",
            ThemeSetting::Dark => "dark",
        }
    }

    /// The label Electron's Theme select shows.
    pub fn label(self) -> &'static str {
        match self {
            ThemeSetting::System => "System",
            ThemeSetting::Light => "Light",
            ThemeSetting::Dark => "Dark",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        ThemeSetting::ALL
            .into_iter()
            .find(|theme| theme.as_str() == value)
    }
}

/// The on-disk shape. Keys and defaults are Electron's
/// (`ClientSettingsSchema`); a missing key takes the default, which is what
/// `Schema.withDecodingDefault` does on that side.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StoredSettings {
    pub theme: ThemeSetting,
    pub vim_mode: bool,
    pub word_wrap: bool,
    pub auto_save_enabled: bool,
    pub auto_save_delay_ms: u32,
    pub show_file_conflict_warning: bool,
    pub confirm_thread_delete: bool,
    /// Not an Electron settings row — there it is browser-local state behind
    /// the breadcrumb's FolderTree button (`t3code.fileExplorerOpen`). It
    /// rides along here because it is the same kind of value and was already
    /// persisted next to `vimMode`.
    pub file_explorer_open: bool,
}

impl Default for StoredSettings {
    fn default() -> Self {
        Self {
            theme: ThemeSetting::System,
            vim_mode: false,
            word_wrap: true,
            auto_save_enabled: true,
            auto_save_delay_ms: 500,
            show_file_conflict_warning: true,
            confirm_thread_delete: true,
            file_explorer_open: true,
        }
    }
}

impl StoredSettings {
    /// Clamp anything a hand-edited file could put out of range.
    fn sanitized(mut self) -> Self {
        self.auto_save_delay_ms = self
            .auto_save_delay_ms
            .clamp(MIN_AUTO_SAVE_DELAY_MS, MAX_AUTO_SAVE_DELAY_MS);
        self
    }
}

/// What the previous store held, for the one-time migration.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct LegacyEditorPrefs {
    vim_mode: bool,
    file_explorer_open: Option<bool>,
}

pub struct ClientSettings {
    values: StoredSettings,
    path: PathBuf,
}

impl Global for ClientSettings {}

impl ClientSettings {
    pub fn init(cx: &mut App, home: &Path) {
        cx.set_global(Self::load(home));
    }

    fn load(home: &Path) -> Self {
        let path = home.join(FILE_NAME);
        let stored = std::fs::read_to_string(&path)
            .ok()
            .and_then(|contents| serde_json::from_str::<StoredSettings>(&contents).ok())
            .map(StoredSettings::sanitized)
            .unwrap_or_else(|| Self::migrated(home));
        Self {
            values: stored,
            path,
        }
    }

    /// Defaults, with `vimMode` and `fileExplorerOpen` carried over from the
    /// editor-only store this file replaced.
    fn migrated(home: &Path) -> StoredSettings {
        let legacy = std::fs::read_to_string(home.join(LEGACY_FILE_NAME))
            .ok()
            .and_then(|contents| serde_json::from_str::<LegacyEditorPrefs>(&contents).ok())
            .unwrap_or_default();
        StoredSettings {
            vim_mode: legacy.vim_mode,
            file_explorer_open: legacy.file_explorer_open.unwrap_or(true),
            ..StoredSettings::default()
        }
    }

    /// Every value at once. `StoredSettings` is `Copy`, so callers that want
    /// two fields do not pay for two global lookups.
    pub fn get(cx: &App) -> StoredSettings {
        cx.try_global::<ClientSettings>()
            .map(|settings| settings.values)
            .unwrap_or_default()
    }

    /// Mutate and persist. The global is replaced wholesale so
    /// `cx.observe_global::<ClientSettings>` fires — that is how a live editor
    /// hears about vim mode and soft wrap.
    pub fn update(cx: &mut App, edit: impl FnOnce(&mut StoredSettings)) {
        let Some(settings) = cx.try_global::<ClientSettings>() else {
            return;
        };
        let path = settings.path.clone();
        let mut values = settings.values;
        edit(&mut values);
        let values = values.sanitized();
        if values == settings.values {
            return;
        }
        cx.set_global(Self {
            values,
            path: path.clone(),
        });
        // Best-effort, like every other Vitre UI preference: a failed write is
        // not worth interrupting the session over.
        match serde_json::to_string_pretty(&values) {
            Ok(contents) => {
                if let Some(parent) = path.parent()
                    && let Err(error) = std::fs::create_dir_all(parent)
                {
                    eprintln!("[vitre] failed to create {}: {error}", parent.display());
                }
                if let Err(error) = std::fs::write(&path, contents) {
                    eprintln!("[vitre] failed to persist {FILE_NAME}: {error}");
                }
            }
            Err(error) => eprintln!("[vitre] failed to encode {FILE_NAME}: {error}"),
        }
    }

    pub fn vim_mode(cx: &App) -> bool {
        Self::get(cx).vim_mode
    }

    /// Flip vim mode and persist it. Returns the new value.
    pub fn toggle_vim_mode(cx: &mut App) -> bool {
        let vim_mode = !Self::vim_mode(cx);
        Self::update(cx, |settings| settings.vim_mode = vim_mode);
        vim_mode
    }

    /// Whether the files panel shows its tree aside. Defaults to shown, as
    /// Electron's missing-localStorage-key fallback does.
    pub fn file_explorer_open(cx: &App) -> bool {
        Self::get(cx).file_explorer_open
    }

    pub fn set_file_explorer_open(cx: &mut App, file_explorer_open: bool) {
        Self::update(cx, |settings| {
            settings.file_explorer_open = file_explorer_open
        });
    }

    pub fn word_wrap(cx: &App) -> bool {
        Self::get(cx).word_wrap
    }

    /// The autosave debounce, or `None` when autosave is off and ⌘S / `:w` are
    /// the only way a buffer reaches disk.
    pub fn auto_save_delay(cx: &App) -> Option<Duration> {
        let settings = Self::get(cx);
        settings
            .auto_save_enabled
            .then(|| Duration::from_millis(settings.auto_save_delay_ms.into()))
    }

    pub fn show_file_conflict_warning(cx: &App) -> bool {
        Self::get(cx).show_file_conflict_warning
    }

    pub fn confirm_thread_delete(cx: &App) -> bool {
        Self::get(cx).confirm_thread_delete
    }

    pub fn theme(cx: &App) -> ThemeSetting {
        Self::get(cx).theme
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_electron_contract() {
        let defaults = StoredSettings::default();
        assert!(!defaults.vim_mode);
        assert!(defaults.word_wrap);
        assert!(defaults.auto_save_enabled);
        assert_eq!(defaults.auto_save_delay_ms, 500);
        assert!(defaults.show_file_conflict_warning);
        assert!(defaults.confirm_thread_delete);
        assert_eq!(defaults.theme, ThemeSetting::System);
    }

    #[test]
    fn a_missing_key_takes_its_default_rather_than_failing_the_file() {
        let stored: StoredSettings = serde_json::from_str(r#"{"vimMode": true}"#).expect("decodes");
        assert!(stored.vim_mode);
        assert!(stored.word_wrap, "the rest fall back to their defaults");
        assert_eq!(stored.auto_save_delay_ms, 500);
    }

    #[test]
    fn the_delay_is_clamped_to_the_contracts_bounds() {
        let fast: StoredSettings = serde_json::from_str(r#"{"autoSaveDelayMs": 5}"#).expect("ok");
        assert_eq!(fast.sanitized().auto_save_delay_ms, MIN_AUTO_SAVE_DELAY_MS);
        let slow: StoredSettings =
            serde_json::from_str(r#"{"autoSaveDelayMs": 900000}"#).expect("ok");
        assert_eq!(slow.sanitized().auto_save_delay_ms, MAX_AUTO_SAVE_DELAY_MS);
    }

    #[test]
    fn theme_round_trips_through_electrons_strings() {
        for theme in ThemeSetting::ALL {
            assert_eq!(ThemeSetting::from_str(theme.as_str()), Some(theme));
        }
        assert_eq!(
            serde_json::to_string(&ThemeSetting::Dark).expect("encodes"),
            "\"dark\""
        );
        assert_eq!(ThemeSetting::from_str("solarized"), None);
    }

    #[test]
    fn a_home_with_only_the_old_editor_file_keeps_its_vim_preference() {
        let home = std::env::temp_dir().join(format!(
            "vitre-client-settings-{}",
            std::process::id() as u64
        ));
        std::fs::create_dir_all(&home).expect("temp home");
        let legacy = home.join(LEGACY_FILE_NAME);
        std::fs::write(&legacy, r#"{"vimMode": true, "fileExplorerOpen": false}"#)
            .expect("legacy prefs");

        let migrated = ClientSettings::migrated(&home);
        assert!(migrated.vim_mode, "vim mode survives the upgrade");
        assert!(!migrated.file_explorer_open);
        assert!(migrated.word_wrap, "new keys arrive at their defaults");

        std::fs::remove_dir_all(&home).ok();
    }
}
