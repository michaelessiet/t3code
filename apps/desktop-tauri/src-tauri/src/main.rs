//! Tauri shell for T3 Code — milestone 1 walking skeleton.
//!
//! Boots the real Node server (bootstrap envelope over stdin, the server's
//! existing `--bootstrap-fd 0` path), serves the real web UI through the
//! `t3code-tauri://app/` custom protocol (proxying to the Vite dev server or
//! the backend, mirroring Electron's ElectronProtocol.ts), and injects the
//! desktopBridge shim as an initialization script. See ../README.md.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod bridge;
mod menu;
mod protocol;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::Emitter;

fn env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_backend_config() -> backend::BackendConfig {
    let server_entry = env_var("T3CODE_TAURI_SERVER_ENTRY")
        .map(PathBuf::from)
        .expect("T3CODE_TAURI_SERVER_ENTRY must point at apps/server/dist/bin.mjs");
    let shim_path = env_var("T3CODE_TAURI_SHIM_PATH")
        .map(PathBuf::from)
        .expect("T3CODE_TAURI_SHIM_PATH must point at the built desktopBridge shim");
    let shim_source = std::fs::read_to_string(&shim_path)
        .unwrap_or_else(|error| panic!("failed to read shim at {}: {error}", shim_path.display()));

    // Isolated by default so a concurrently running Electron dev app doesn't
    // share the same SQLite state; override with T3CODE_TAURI_HOME=~/.t3.
    let t3_home = env_var("T3CODE_TAURI_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(".t3-tauri")
        });

    backend::BackendConfig {
        node_binary: env_var("T3CODE_TAURI_NODE").unwrap_or_else(|| "node".to_string()),
        server_entry,
        t3_home,
        dev_server_url: env_var("VITE_DEV_SERVER_URL"),
        fixed_port: env_var("T3CODE_PORT").and_then(|value| value.parse().ok()),
        shim_source,
    }
}

// SIGTERM/SIGINT don't reach RunEvent::Exit, so a plain `kill` (or the dev
// runner tearing the shell down) would orphan the Node child without this.
// backend::shutdown only touches atomics and libc::kill — async-signal-safe.
#[cfg(unix)]
unsafe extern "C" fn on_terminate_signal(_signal: i32) {
    backend::shutdown();
    libc::_exit(0);
}

fn main() {
    let config = read_backend_config();

    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGTERM, on_terminate_signal as *const () as usize);
        libc::signal(libc::SIGINT, on_terminate_signal as *const () as usize);
    }

    static LAST_FULLSCREEN: AtomicBool = AtomicBool::new(false);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, protocol::handler)
        .invoke_handler(tauri::generate_handler![
            bridge::pick_folder,
            bridge::confirm_dialog,
            bridge::open_external,
            bridge::set_theme,
            bridge::get_client_settings,
            bridge::set_client_settings,
            bridge::get_connection_catalog,
            bridge::set_connection_catalog,
            bridge::clear_connection_catalog,
            bridge::show_context_menu,
        ])
        .on_menu_event(|app, event| menu::handle_event(app, event))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Resized(_) = event {
                let fullscreen = window.is_fullscreen().unwrap_or(false);
                if LAST_FULLSCREEN.swap(fullscreen, Ordering::SeqCst) != fullscreen {
                    let _ = window.emit("t3code://fullscreen-changed", fullscreen);
                }
            }
        })
        .setup(move |app| {
            menu::install(app.handle())?;
            let handle = app.handle().clone();
            std::thread::spawn(move || backend::run(handle, config));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("tauri shell should build")
        .run(|_, event| {
            if let tauri::RunEvent::Exit = event {
                backend::shutdown();
            }
        });
}
