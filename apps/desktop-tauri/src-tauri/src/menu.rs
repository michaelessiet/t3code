//! Application menu (subset of apps/desktop/src/window/DesktopApplicationMenu.ts)
//! and the app-level menu-event dispatcher shared with bridge context menus.

use tauri::menu::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager};

use crate::bridge;

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let app_menu = Submenu::with_items(
        app,
        "Vitre",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About Vitre"), Some(AboutMetadata::default()))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                "menu:open-settings",
                "Settings…",
                true,
                Some("CmdOrCtrl+,"),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &MenuItem::with_id(app, "menu:reload", "Reload", true, Some("CmdOrCtrl+R"))?,
            &MenuItem::with_id(
                app,
                "menu:devtools",
                "Toggle Developer Tools",
                true,
                Some("Alt+CmdOrCtrl+I"),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;
    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    let menu = Menu::with_items(app, &[&app_menu, &edit_menu, &view_menu, &window_menu])?;
    app.set_menu(menu)?;
    Ok(())
}

pub fn handle_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().0.as_str();
    if let Some(context_id) = id.strip_prefix("ctx:") {
        *bridge::CONTEXT_MENU_RESULT
            .lock()
            .expect("context menu mutex") = Some(context_id.to_string());
        return;
    }
    match id {
        "menu:open-settings" => {
            let _ = app.emit("t3code://menu-action", "open-settings");
        }
        "menu:reload" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval("location.reload()");
            }
        }
        "menu:devtools" => {
            if let Some(window) = app.get_webview_window("main") {
                if window.is_devtools_open() {
                    window.close_devtools();
                } else {
                    window.open_devtools();
                }
            }
        }
        _ => {}
    }
}
