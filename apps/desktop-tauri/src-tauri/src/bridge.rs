//! Tauri command handlers backing the desktopBridge shim (shim/src/index.ts).
//! Each command mirrors the semantics of the Electron IPC handler of the same
//! name in apps/desktop/src/ipc/methods/*.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_opener::OpenerExt;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickFolderOptions {
    pub initial_path: Option<String>,
    #[allow(dead_code)] // Multi-backend targeting is not meaningful in M1.
    pub target_environment_id: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct ContextMenuNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub header: bool,
    pub children: Option<Vec<ContextMenuNode>>,
}

#[derive(serde::Deserialize)]
pub struct MenuPosition {
    pub x: f64,
    pub y: f64,
}

/// Filled by the app-level menu-event handler when a `ctx:`-prefixed item is
/// clicked; read by `show_context_menu` after the popup's tracking loop ends.
pub static CONTEXT_MENU_RESULT: Mutex<Option<String>> = Mutex::new(None);

fn config_file(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir.join(name))
}

#[tauri::command]
pub async fn pick_folder(
    app: AppHandle,
    options: Option<PickFolderOptions>,
) -> Result<Option<String>, String> {
    let mut dialog = app.dialog().file();
    if let Some(initial_path) = options.and_then(|options| options.initial_path) {
        dialog = dialog.set_directory(initial_path);
    }
    Ok(dialog.blocking_pick_folder().map(|path| path.to_string()))
}

#[tauri::command]
pub async fn confirm_dialog(app: AppHandle, message: String) -> Result<bool, String> {
    Ok(app
        .dialog()
        .message(message)
        .buttons(MessageDialogButtons::OkCancel)
        .blocking_show())
}

#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<bool, String> {
    // Same safe-scheme gate as ElectronShell.openExternal.
    let Ok(parsed) = tauri::Url::parse(&url) else {
        return Ok(false);
    };
    if !matches!(parsed.scheme(), "http" | "https" | "mailto") {
        return Ok(false);
    }
    app.opener()
        .open_url(url, None::<String>)
        .map_err(|error| error.to_string())?;
    Ok(true)
}

#[tauri::command]
pub fn set_theme(app: AppHandle, theme: String) -> Result<(), String> {
    let theme = match theme.as_str() {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    };
    app.set_theme(theme);
    Ok(())
}

#[tauri::command]
pub fn get_client_settings(app: AppHandle) -> Result<Option<serde_json::Value>, String> {
    let path = config_file(&app, "client-settings.json")?;
    match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw)
            .map(Some)
            .map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub fn set_client_settings(app: AppHandle, settings: serde_json::Value) -> Result<(), String> {
    let path = config_file(&app, "client-settings.json")?;
    std::fs::write(path, settings.to_string()).map_err(|error| error.to_string())
}

// Plaintext in M1 — the Electron app encrypts this with safeStorage; a
// keychain-backed store is a milestone-3 item (see README).
#[tauri::command]
pub fn get_connection_catalog(app: AppHandle) -> Result<Option<String>, String> {
    let path = config_file(&app, "connection-catalog.json")?;
    match std::fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub fn set_connection_catalog(app: AppHandle, catalog: String) -> Result<bool, String> {
    let path = config_file(&app, "connection-catalog.json")?;
    std::fs::write(path, catalog).map_err(|error| error.to_string())?;
    Ok(true)
}

#[tauri::command]
pub fn clear_connection_catalog(app: AppHandle) -> Result<(), String> {
    let path = config_file(&app, "connection-catalog.json")?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn build_menu_items(
    app: &AppHandle,
    nodes: &[ContextMenuNode],
) -> Result<Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>>, String> {
    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();
    for node in nodes {
        if let Some(children) = &node.children {
            let child_items = build_menu_items(app, children)?;
            let child_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
                child_items.iter().map(|item| item.as_ref()).collect();
            let submenu =
                tauri::menu::Submenu::with_items(app, &node.label, !node.disabled, &child_refs)
                    .map_err(|error| error.to_string())?;
            items.push(Box::new(submenu));
            continue;
        }
        if node.header {
            let header = tauri::menu::MenuItem::new(app, &node.label, false, None::<&str>)
                .map_err(|error| error.to_string())?;
            items.push(Box::new(header));
            continue;
        }
        let item = tauri::menu::MenuItem::with_id(
            app,
            format!("ctx:{}", node.id),
            &node.label,
            !node.disabled,
            None::<&str>,
        )
        .map_err(|error| error.to_string())?;
        items.push(Box::new(item));
    }
    Ok(items)
}

#[tauri::command]
pub async fn show_context_menu(
    app: AppHandle,
    items: Vec<ContextMenuNode>,
    position: Option<MenuPosition>,
) -> Result<Option<String>, String> {
    let window = app
        .get_webview_window("main")
        .ok_or("main window is not available")?;

    *CONTEXT_MENU_RESULT.lock().expect("context menu mutex") = None;

    // Menu types are !Send, so the menu is built inside the main-thread
    // closure. popup_menu runs muda's tracking loop on the main thread and
    // only returns once the menu is dismissed, so `done` doubles as the
    // dismissal signal — including "closed with no selection", which never
    // produces a menu event.
    let (done_tx, done_rx) = std::sync::mpsc::channel::<Option<String>>();
    let app_for_menu = app.clone();
    app.run_on_main_thread(move || {
        let outcome = (|| -> Result<(), String> {
            let menu_items = build_menu_items(&app_for_menu, &items)?;
            let item_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
                menu_items.iter().map(|item| item.as_ref()).collect();
            let menu = tauri::menu::Menu::with_items(&app_for_menu, &item_refs)
                .map_err(|error| error.to_string())?;
            match position {
                Some(position) => window.popup_menu_at(
                    &menu,
                    tauri::Position::Logical(tauri::LogicalPosition::new(position.x, position.y)),
                ),
                None => window.popup_menu(&menu),
            }
            .map_err(|error| error.to_string())
        })();
        let _ = done_tx.send(outcome.err());
    })
    .map_err(|error| error.to_string())?;

    let popup_error = tauri::async_runtime::spawn_blocking(move || done_rx.recv())
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    if let Some(error) = popup_error {
        return Err(error);
    }

    // The click's menu event is delivered through the run loop and may land
    // just after the popup returns; poll briefly before concluding "no
    // selection".
    for _ in 0..20 {
        if let Some(chosen) = CONTEXT_MENU_RESULT
            .lock()
            .expect("context menu mutex")
            .take()
        {
            return Ok(Some(chosen));
        }
        let _ = tauri::async_runtime::spawn_blocking(|| {
            std::thread::sleep(Duration::from_millis(25));
        })
        .await;
    }
    Ok(None)
}
