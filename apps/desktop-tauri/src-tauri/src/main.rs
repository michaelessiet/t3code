//! Vitre — Tauri-based desktop shell, evolved from T3 Code.
//!
//! Boots the real Node server (bootstrap envelope over stdin, the server's
//! existing `--bootstrap-fd 0` path), serves the real web UI through the
//! `vitre://app/` custom protocol (proxying to the Vite dev server or
//! the backend, mirroring Electron's ElectronProtocol.ts), and injects the
//! desktopBridge shim as an initialization script. See ../README.md.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod bridge;
mod menu;
mod preview;
mod protocol;
mod selftest;

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
    let server_entry = env_var("VITRE_SERVER_ENTRY")
        .map(PathBuf::from)
        .expect("VITRE_SERVER_ENTRY must point at apps/server/dist/bin.mjs");
    let shim_path = env_var("VITRE_SHIM_PATH")
        .map(PathBuf::from)
        .expect("VITRE_SHIM_PATH must point at the built desktopBridge shim");
    let shim_source = std::fs::read_to_string(&shim_path)
        .unwrap_or_else(|error| panic!("failed to read shim at {}: {error}", shim_path.display()));

    // Isolated by default so a concurrently running Electron dev app doesn't
    // share the same SQLite state; override with VITRE_HOME=~/.t3.
    let t3_home = env_var("VITRE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(".vitre")
        });

    backend::BackendConfig {
        node_binary: env_var("VITRE_NODE").unwrap_or_else(|| "node".to_string()),
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

fn read_preview_config() -> preview::PreviewConfig {
    let runtime_path = env_var("VITRE_PREVIEW_RUNTIME_PATH")
        .map(PathBuf::from)
        .expect("VITRE_PREVIEW_RUNTIME_PATH must point at the built preview runtime");
    let runtime_source = std::fs::read_to_string(&runtime_path).unwrap_or_else(|error| {
        panic!(
            "failed to read preview runtime at {}: {error}",
            runtime_path.display()
        )
    });
    let t3_home = env_var("VITRE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(".vitre")
        });
    preview::PreviewConfig {
        runtime_source,
        artifacts_dir: t3_home.join("preview-artifacts"),
    }
}

fn main() {
    let config = read_backend_config();
    preview::init(read_preview_config());

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
        .register_uri_scheme_protocol(preview::IPC_SCHEME, preview::ipc_handler)
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
            preview::preview_create_tab,
            preview::preview_close_tab,
            preview::preview_register_webview,
            preview::preview_set_tab_bounds,
            preview::preview_navigate,
            preview::preview_go_back,
            preview::preview_go_forward,
            preview::preview_refresh,
            preview::preview_hard_reload,
            preview::preview_zoom_in,
            preview::preview_zoom_out,
            preview::preview_reset_zoom,
            preview::preview_set_color_scheme,
            preview::preview_open_devtools,
            preview::preview_clear_cookies,
            preview::preview_clear_cache,
            preview::preview_get_config,
            preview::preview_set_annotation_theme,
            preview::preview_pick_element,
            preview::preview_cancel_pick_element,
            preview::preview_pip_open,
            preview::preview_pip_close,
            preview::preview_capture_screenshot,
            preview::preview_reveal_artifact,
            preview::preview_recording_start,
            preview::preview_recording_stop,
            preview::preview_recording_save,
            preview::preview_automation_status,
            preview::preview_automation_snapshot,
            preview::preview_automation_click,
            preview::preview_automation_type,
            preview::preview_automation_press,
            preview::preview_automation_scroll,
            preview::preview_automation_evaluate,
            preview::preview_automation_wait_for,
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
            selftest::maybe_run(app.handle());
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
