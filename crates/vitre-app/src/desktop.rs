//! Native shell integration shared by the menu, launch and window lifecycle.
use gpui::{App, Bounds, Menu, MenuItem, Window, WindowBounds, actions, point, px, size};
use gpui_component::input;
use serde::{Deserialize, Serialize};
use std::path::Path;

actions!(
    vitre,
    [Quit, Hide, HideOthers, Minimize, ZoomIn, ZoomOut, ResetZoom]
);

pub fn init(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &Minimize, cx| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.minimize_window());
        }
    });
    cx.on_action(|_: &ZoomIn, cx| zoom(1., cx));
    cx.on_action(|_: &ZoomOut, cx| zoom(-1., cx));
    cx.on_action(|_: &ResetZoom, cx| zoom(0., cx));
    cx.bind_keys([
        gpui::KeyBinding::new(&crate::modified("q"), Quit, None),
        gpui::KeyBinding::new(&crate::modified("h"), Hide, None),
        gpui::KeyBinding::new(&crate::modified("m"), Minimize, None),
        gpui::KeyBinding::new(&crate::modified("="), ZoomIn, Some("!Preview")),
        gpui::KeyBinding::new(&crate::modified("-"), ZoomOut, Some("!Preview")),
        gpui::KeyBinding::new(&crate::modified("0"), ResetZoom, Some("!Preview")),
    ]);
    cx.set_menus([
        Menu::new("Vitre").items([
            MenuItem::action("Settings…", crate::settings::SettingsOpen),
            MenuItem::Separator,
            MenuItem::action("Hide Vitre", Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::Separator,
            MenuItem::action("Quit Vitre", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("New Thread", crate::chat::NewThread),
            MenuItem::action("Save", crate::files::SaveFile),
            MenuItem::action("Close Tab", crate::chat::RightPanelCloseSurface),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", input::Undo, gpui::OsAction::Undo),
            MenuItem::os_action("Redo", input::Redo, gpui::OsAction::Redo),
            MenuItem::Separator,
            MenuItem::os_action("Cut", input::Cut, gpui::OsAction::Cut),
            MenuItem::os_action("Copy", input::Copy, gpui::OsAction::Copy),
            MenuItem::os_action("Paste", input::Paste, gpui::OsAction::Paste),
            MenuItem::os_action("Select All", input::SelectAll, gpui::OsAction::SelectAll),
        ]),
        Menu::new("View").items([
            MenuItem::action("Command Palette", crate::chat::CommandPaletteToggle),
            MenuItem::action("Toggle Sidebar", crate::chat::SidebarToggle),
            MenuItem::action("Toggle Browser", crate::chat::PreviewToggle),
            MenuItem::action("Toggle Terminal", crate::chat::TerminalToggle),
            MenuItem::Separator,
            MenuItem::action("Zoom In", ZoomIn),
            MenuItem::action("Zoom Out", ZoomOut),
            MenuItem::action("Actual Size", ResetZoom),
        ]),
        Menu::new("Window").items([MenuItem::action("Minimize", Minimize)]),
    ]);
}

fn zoom(delta: f32, cx: &mut App) {
    let value = if delta == 0. {
        16.
    } else {
        (crate::client_settings::ClientSettings::get(cx).ui_font_size + delta).clamp(12., 24.)
    };
    crate::client_settings::ClientSettings::update(cx, |settings| settings.ui_font_size = value);
    for handle in cx.windows() {
        let _ = handle.update(cx, |_, window, cx| {
            window.set_rem_size(px(value));
            window.refresh();
            cx.refresh_windows();
        });
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedWindow {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    maximized: bool,
}

pub fn load_bounds(home: &Path, cx: &App) -> WindowBounds {
    let fallback = || WindowBounds::centered(size(px(1100.), px(720.)), cx);
    let Some(saved) = std::fs::read(home.join("window-state.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<SavedWindow>(&b).ok())
    else {
        return fallback();
    };
    if ![saved.x, saved.y, saved.width, saved.height]
        .iter()
        .all(|n| n.is_finite())
        || saved.width < 640.
        || saved.height < 480.
    {
        return fallback();
    }
    let bounds = Bounds {
        origin: point(px(saved.x), px(saved.y)),
        size: size(px(saved.width), px(saved.height)),
    };
    // A disconnected display must not strand the titlebar off-screen.
    let visible = cx.displays().iter().any(|d| {
        d.bounds()
            .contains(&(bounds.origin + point(px(100.), px(24.))))
    });
    if !visible {
        return fallback();
    }
    if saved.maximized {
        WindowBounds::Maximized(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    }
}

pub fn save_bounds(home: &Path, window: &Window) {
    let mode = window.window_bounds();
    let bounds = mode.get_bounds();
    let saved = SavedWindow {
        x: bounds.origin.x.into(),
        y: bounds.origin.y.into(),
        width: bounds.size.width.into(),
        height: bounds.size.height.into(),
        maximized: matches!(mode, WindowBounds::Maximized(_)),
    };
    if let Ok(bytes) = serde_json::to_vec(&saved) {
        let temporary = home.join("window-state.json.tmp");
        if std::fs::write(&temporary, bytes).is_ok() {
            let _ = std::fs::rename(temporary, home.join("window-state.json"));
        }
    }
}

pub fn thread_from_url(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("vitre://thread/")
        .or_else(|| url.strip_prefix("t3code://thread/"))?;
    let id = percent_encoding::percent_decode_str(rest.split(['?', '#']).next()?)
        .decode_utf8()
        .ok()?;
    (!id.is_empty() && id.len() <= 512 && !id.chars().any(|c| c.is_control() || c == '/'))
        .then(|| id.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_valid_thread_links_are_routed() {
        assert_eq!(
            thread_from_url("vitre://thread/thread-1?source=test"),
            Some("thread-1".into())
        );
        assert_eq!(thread_from_url("t3code://thread/id"), Some("id".into()));
        for url in [
            "https://thread/id",
            "vitre://thread/",
            "vitre://thread/a%2Fb",
            "vitre://thread/a%0Ab",
        ] {
            assert!(thread_from_url(url).is_none());
        }
    }
}
