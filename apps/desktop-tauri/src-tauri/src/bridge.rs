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

// --- connection catalog ------------------------------------------------------
// The catalog holds bearer tokens, DPoP tokens, and SSH connection info
// (ConnectionCatalogDocument in packages/client-runtime), so it must not sit
// on disk in plaintext. Mirroring Electron safeStorage's shape: an encryption
// key lives in the OS keychain and the file holds only ciphertext
// (`{version, cipher, nonce, ciphertext}`). The M1 plaintext file is migrated
// on first read. macOS-only for now — other platforms keep the M1 plaintext
// file until the M4 platform pass (DPAPI / secret-service).

/// Same atomic write discipline as Electron's DesktopConnectionCatalogStore:
/// write a pid-suffixed temp file, then rename over the real path.
fn write_atomic(path: &std::path::Path, contents: &str) -> Result<(), String> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, contents).map_err(|error| error.to_string())?;
    std::fs::rename(&tmp, path).map_err(|error| {
        let _ = std::fs::remove_file(&tmp);
        error.to_string()
    })
}

#[cfg(target_os = "macos")]
mod catalog_crypto {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine as _;
    use rand::RngCore as _;

    const KEYCHAIN_SERVICE: &str = "com.t3code.desktop-tauri";
    const KEYCHAIN_ACCOUNT: &str = "connection-catalog-key";

    fn keychain_service() -> String {
        std::env::var("T3CODE_TAURI_KEYCHAIN_SERVICE")
            .unwrap_or_else(|_| KEYCHAIN_SERVICE.to_string())
    }

    /// Reads the catalog encryption key from the keychain, generating and
    /// storing one when `create` is set. `Ok(None)` means "no key and not
    /// asked to create one"; keychain access failures are errors (the caller
    /// treats them as "secure storage unavailable" on the write path).
    fn load_key(create: bool) -> Result<Option<[u8; 32]>, String> {
        let entry = keyring::Entry::new(&keychain_service(), KEYCHAIN_ACCOUNT)
            .map_err(|error| error.to_string())?;
        match entry.get_password() {
            Ok(encoded) => {
                let bytes = B64
                    .decode(encoded.trim())
                    .map_err(|error| error.to_string())?;
                let key: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| "keychain catalog key has the wrong length".to_string())?;
                Ok(Some(key))
            }
            Err(keyring::Error::NoEntry) => {
                if !create {
                    return Ok(None);
                }
                let mut key = [0u8; 32];
                rand::rngs::OsRng.fill_bytes(&mut key);
                entry
                    .set_password(&B64.encode(key))
                    .map_err(|error| error.to_string())?;
                Ok(Some(key))
            }
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn encrypt(plaintext: &str) -> Result<serde_json::Value, String> {
        let key = load_key(true)?.ok_or("keychain catalog key unavailable")?;
        let cipher = Aes256Gcm::new((&key).into());
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
            .map_err(|error| error.to_string())?;
        Ok(serde_json::json!({
            "version": 1,
            "cipher": "aes-256-gcm",
            "nonce": B64.encode(nonce),
            "ciphertext": B64.encode(ciphertext),
        }))
    }

    pub fn decrypt(document: &serde_json::Value) -> Result<String, String> {
        let field = |name: &str| -> Result<Vec<u8>, String> {
            let value = document
                .get(name)
                .and_then(|value| value.as_str())
                .ok_or_else(|| format!("catalog document is missing `{name}`"))?;
            B64.decode(value).map_err(|error| error.to_string())
        };
        let nonce = field("nonce")?;
        let ciphertext = field("ciphertext")?;
        let key = load_key(false)?
            .ok_or("the catalog is encrypted but its keychain key is missing")?;
        let cipher = Aes256Gcm::new((&key).into());
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_slice())
            .map_err(|_| "failed to decrypt the connection catalog".to_string())?;
        String::from_utf8(plaintext).map_err(|error| error.to_string())
    }
}

/// Keychain-backed encryption is release-default. A dev (debug) binary's
/// ad-hoc code signature changes on every rebuild, so the keychain would show
/// a blocking permission prompt for an item the previous build created —
/// Electron avoids this only because its dev binary is the stable prebuilt
/// Electron. Dev builds therefore keep the M1 plaintext file unless
/// T3CODE_TAURI_SECURE_CATALOG=1 opts in (=0 opts a release build out).
#[cfg(target_os = "macos")]
fn secure_catalog_enabled() -> bool {
    match std::env::var("T3CODE_TAURI_SECURE_CATALOG").as_deref() {
        Ok("1") => true,
        Ok("0") => false,
        _ => !cfg!(debug_assertions),
    }
}

#[cfg(target_os = "macos")]
fn read_catalog(path: PathBuf) -> Result<Option<String>, String> {
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    if parsed.get("ciphertext").is_some() {
        // Always decrypt an already-encrypted file, whatever the mode.
        return catalog_crypto::decrypt(&parsed).map(Some);
    }
    if secure_catalog_enabled() {
        // An M1/dev plaintext catalog: migrate it to the encrypted shape in
        // place. Best-effort — when the keychain is unavailable the plaintext
        // stays and migration retries on the next read.
        match catalog_crypto::encrypt(&raw) {
            Ok(document) => {
                let mut contents = document.to_string();
                contents.push('\n');
                write_atomic(&path, &contents)?;
            }
            Err(error) => {
                eprintln!("[desktop-tauri] catalog migration deferred: {error}");
            }
        }
    }
    Ok(Some(raw))
}

#[cfg(not(target_os = "macos"))]
fn read_catalog(path: PathBuf) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(target_os = "macos")]
fn write_catalog(path: PathBuf, catalog: String) -> Result<bool, String> {
    if !secure_catalog_enabled() {
        write_atomic(&path, &catalog)?;
        return Ok(true);
    }
    match catalog_crypto::encrypt(&catalog) {
        Ok(document) => {
            let mut contents = document.to_string();
            contents.push('\n');
            write_atomic(&path, &contents)?;
            Ok(true)
        }
        Err(error) => {
            // `false` is the contract for "secure storage unavailable" — the
            // web app surfaces it (apps/web/src/connection/storage.ts) instead
            // of persisting secrets in the clear.
            eprintln!("[desktop-tauri] connection catalog not saved: {error}");
            Ok(false)
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn write_catalog(path: PathBuf, catalog: String) -> Result<bool, String> {
    write_atomic(&path, &catalog)?;
    Ok(true)
}

#[cfg(all(test, target_os = "macos"))]
mod catalog_tests {
    // One #[test] fn: the keychain-service and secure-mode env vars are
    // process-global, and the sequence (migrate → decrypt → round-trip) is
    // order-dependent.
    #[test]
    fn catalog_migration_and_roundtrip() {
        let service = format!("com.t3code.desktop-tauri.test-{}", std::process::id());
        std::env::set_var("T3CODE_TAURI_KEYCHAIN_SERVICE", &service);
        std::env::set_var("T3CODE_TAURI_SECURE_CATALOG", "1");

        let dir = std::env::temp_dir().join(format!("t3-catalog-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("connection-catalog.json");
        let plaintext = r#"{"schemaVersion":1,"targets":[],"profiles":[]}"#;

        // An M1 plaintext file is returned verbatim and re-written encrypted.
        std::fs::write(&path, plaintext).unwrap();
        let migrated = super::read_catalog(path.clone()).unwrap().unwrap();
        assert_eq!(migrated, plaintext);
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("ciphertext"), "migration left plaintext on disk");
        assert!(!on_disk.contains("schemaVersion"), "catalog content leaked into wrapper");

        // The encrypted file decrypts back to the original catalog.
        assert_eq!(super::read_catalog(path.clone()).unwrap().unwrap(), plaintext);

        // A fresh write round-trips through the keychain-backed cipher.
        assert!(super::write_catalog(path.clone(), "updated-secret".into()).unwrap());
        assert_eq!(
            super::read_catalog(path.clone()).unwrap().unwrap(),
            "updated-secret"
        );

        let _ = std::fs::remove_dir_all(&dir);
        let entry = keyring::Entry::new(&service, "connection-catalog-key").unwrap();
        let _ = entry.delete_credential();
        std::env::remove_var("T3CODE_TAURI_KEYCHAIN_SERVICE");
        std::env::remove_var("T3CODE_TAURI_SECURE_CATALOG");
    }
}

#[tauri::command]
pub async fn get_connection_catalog(app: AppHandle) -> Result<Option<String>, String> {
    let path = config_file(&app, "connection-catalog.json")?;
    // Keychain access can block (and can prompt in dev builds whose code
    // signature changed); keep it off the main thread.
    tauri::async_runtime::spawn_blocking(move || read_catalog(path))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn set_connection_catalog(app: AppHandle, catalog: String) -> Result<bool, String> {
    let path = config_file(&app, "connection-catalog.json")?;
    tauri::async_runtime::spawn_blocking(move || write_catalog(path, catalog))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn clear_connection_catalog(app: AppHandle) -> Result<(), String> {
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
