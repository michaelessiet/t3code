//! Shared tint colors for Vitre's native frosted window.
//!
//! GPUI supplies the platform blur behind the window. These translucent theme
//! colors keep text readable while allowing the desktop color and light to
//! show through, in the same spirit as Cursor's macOS chrome.

use gpui::{App, Hsla};
use gpui_component::ActiveTheme as _;

use crate::client_settings::ClientSettings;

/// A completely clear root lets every top-level pane composite directly onto
/// the platform blur rather than stacking two tints and becoming opaque.
pub fn root(cx: &App) -> Hsla {
    cx.theme().background.opacity(0.0)
}

pub fn background(cx: &App) -> Hsla {
    cx.theme()
        .background
        .opacity(ClientSettings::glass_opacity(cx))
}

pub fn sidebar(cx: &App) -> Hsla {
    cx.theme()
        .sidebar
        .opacity(ClientSettings::glass_opacity(cx))
}

/// Slightly denser than the main canvas so utility panes and floating controls
/// retain their hierarchy without losing the frosted character.
pub fn elevated(cx: &App) -> Hsla {
    cx.theme()
        .background
        .opacity((ClientSettings::glass_opacity(cx) + 0.08).min(1.0))
}

pub fn control(cx: &App) -> Hsla {
    cx.theme()
        .muted
        .opacity((ClientSettings::glass_opacity(cx) + 0.06).min(1.0))
}
