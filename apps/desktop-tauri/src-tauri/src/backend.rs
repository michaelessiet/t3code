//! Node backend supervision: port selection, bootstrap-envelope delivery over
//! stdin (the server's existing `--bootstrap-fd 0` path, used by the Electron
//! app's WSL backend), readiness polling, and restart-with-backoff.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use crate::protocol;

const DEFAULT_BACKEND_PORT: u16 = 3773;
const READINESS_PATH: &str = "/.well-known/t3/environment";
const READINESS_TIMEOUT: Duration = Duration::from_secs(60);

/// Env vars nulled out before spawning, mirroring `backendChildEnvPatch` in
/// apps/desktop/src/backend/DesktopBackendConfiguration.ts — leaked values
/// from a parent dev runner would override the bootstrap envelope.
const T3CODE_ENV_NAMES: [&str; 10] = [
    "T3CODE_PORT",
    "T3CODE_MODE",
    "T3CODE_NO_BROWSER",
    "T3CODE_HOST",
    "T3CODE_DESKTOP_WS_URL",
    "T3CODE_DESKTOP_LAN_ACCESS",
    "T3CODE_DESKTOP_LAN_HOST",
    "T3CODE_DESKTOP_HTTPS_ENDPOINTS",
    "T3CODE_TAILSCALE_SERVE",
    "T3CODE_TAILSCALE_SERVE_PORT",
];

pub struct BackendConfig {
    pub node_binary: String,
    pub server_entry: PathBuf,
    pub t3_home: PathBuf,
    pub dev_server_url: Option<String>,
    pub fixed_port: Option<u16>,
    pub shim_source: String,
}

pub struct BackendInfo {
    pub http_base_url: String,
    pub ws_base_url: String,
    pub bootstrap_token: String,
}

pub static BACKEND_INFO: OnceLock<BackendInfo> = OnceLock::new();

/// PID of the live backend child (0 = none) and the app-quit latch. Without
/// these the Node server outlives the shell as an orphan holding the SQLite
/// home open.
static CURRENT_CHILD_PID: AtomicU32 = AtomicU32::new(0);
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

/// Called from the RunEvent::Exit handler in main.rs.
pub fn shutdown() {
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
    let pid = CURRENT_CHILD_PID.swap(0, Ordering::SeqCst);
    if pid != 0 {
        #[cfg(unix)]
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
}

fn random_hex_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn pick_port(fixed: Option<u16>) -> Option<u16> {
    if let Some(port) = fixed {
        return Some(port);
    }
    (DEFAULT_BACKEND_PORT..u16::MAX).find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok())
}

/// Minimal readiness probe: HTTP/1.1 GET over a raw socket, checking only the
/// status line. Avoids pulling blocking-client features into reqwest.
fn probe_ready(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}")
            .parse()
            .expect("loopback addr parses"),
        Duration::from_millis(500),
    ) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(1500)));
    let request = format!(
        "GET {READINESS_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut buffer = [0u8; 64];
    let Ok(read) = stream.read(&mut buffer) else {
        return false;
    };
    let status_line = String::from_utf8_lossy(&buffer[..read]);
    status_line.starts_with("HTTP/1.1 200") || status_line.starts_with("HTTP/1.0 200")
}

fn bootstraps_json(info: &BackendInfo) -> serde_json::Value {
    serde_json::json!([{
        "id": "primary",
        "label": "Local environment",
        "httpBaseUrl": info.http_base_url,
        "wsBaseUrl": info.ws_base_url,
        "bootstrapToken": info.bootstrap_token,
    }])
}

fn spawn_backend(config: &BackendConfig, port: u16, token: &str) -> std::io::Result<Child> {
    std::fs::create_dir_all(&config.t3_home)?;

    let mut command = Command::new(&config.node_binary);
    command
        .arg(&config.server_entry)
        .arg("--bootstrap-fd")
        .arg("0");
    if let Some(dev_url) = &config.dev_server_url {
        command.arg("--dev-url").arg(dev_url);
    }
    for name in T3CODE_ENV_NAMES {
        command.env_remove(name);
    }
    command.env_remove("ELECTRON_RUN_AS_NODE");
    // The dev CORS allowlist (apps/server/src/http.ts) only admits the
    // Electron renderer origins; admit this shell's custom scheme too so the
    // shim's /oauth/token exchange survives the preflight.
    command.env(
        "T3CODE_DEV_ALLOWED_ORIGINS",
        format!("{}://{}", protocol::SCHEME, protocol::HOST),
    );
    command
        .current_dir(dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn()?;

    let envelope = serde_json::json!({
        "mode": "desktop",
        "noBrowser": true,
        "port": port,
        "t3Home": config.t3_home.to_string_lossy(),
        "host": "127.0.0.1",
        "desktopBootstrapToken": token,
        "tailscaleServeEnabled": false,
        // Inert while tailscaleServeEnabled is false; PortSchema rejects 0.
        "tailscaleServePort": 443,
    });
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{envelope}\n").as_bytes());
        // Dropping stdin closes it; the server reads the envelope and keeps
        // running (same as the Electron WSL stdin-delivery path).
    }

    if let Some(stdout) = child.stdout.take() {
        forward_output("server", stdout);
    }
    if let Some(stderr) = child.stderr.take() {
        forward_output("server!", stderr);
    }

    Ok(child)
}

fn forward_output(prefix: &'static str, mut stream: impl Read + Send + 'static) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        let mut pending = Vec::new();
        while let Ok(read) = stream.read(&mut buffer) {
            if read == 0 {
                break;
            }
            pending.extend_from_slice(&buffer[..read]);
            while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = pending.drain(..=newline).collect();
                eprintln!("[{prefix}] {}", String::from_utf8_lossy(&line).trim_end());
            }
        }
    });
}

fn wait_until_ready(port: u16, child: &mut Child, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            return false;
        }
        if probe_ready(port) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn create_main_window(handle: &AppHandle, config: &BackendConfig) {
    let info = BACKEND_INFO
        .get()
        .expect("backend info set before window creation");
    let stage_label = if cfg!(debug_assertions) { Some("Dev") } else { None };
    let seed = serde_json::json!({
        "branding": {
            "baseName": "Vitre",
            "stageLabel": stage_label,
            "displayName": if cfg!(debug_assertions) { "Vitre (Dev)" } else { "Vitre" },
        },
        "bootstraps": bootstraps_json(info),
        "appVersion": env!("CARGO_PKG_VERSION"),
        "arch": if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" },
    });
    let script = format!(
        "window.__VITRE_SEED__ = {seed};\n{shim}",
        shim = config.shim_source
    );

    let handle_for_main = handle.clone();
    let result = handle.run_on_main_thread(move || {
        let url: tauri::Url = format!("{}://{}/", protocol::SCHEME, protocol::HOST)
            .parse()
            .expect("desktop scheme URL parses");
        let builder = tauri::WebviewWindowBuilder::new(
            &handle_for_main,
            "main",
            tauri::WebviewUrl::External(url),
        )
        .title(if cfg!(debug_assertions) { "Vitre (Dev)" } else { "Vitre" })
        .inner_size(1280.0, 860.0)
        .min_inner_size(840.0, 620.0)
        .initialization_script(&script);

        #[cfg(target_os = "macos")]
        let builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            // Same inset as Electron's trafficLightPosition (DesktopWindow.ts:
            // getWindowTitleBarOptions) so the web titlebar CSS lines up.
            .traffic_light_position(tauri::LogicalPosition::new(16.0, 18.0));

        if let Err(error) = builder.build() {
            eprintln!("[vitre] failed to create main window: {error}");
        }
    });
    if let Err(error) = result {
        eprintln!("[vitre] failed to dispatch window creation: {error}");
    }
}

/// Supervision loop, run on a dedicated std thread for the app's lifetime.
pub fn run(handle: AppHandle, config: BackendConfig) {
    let Some(port) = pick_port(config.fixed_port) else {
        eprintln!("[vitre] no free backend port found");
        return;
    };
    let token = random_hex_token();
    let info = BackendInfo {
        http_base_url: format!("http://127.0.0.1:{port}"),
        ws_base_url: format!("ws://127.0.0.1:{port}"),
        bootstrap_token: token.clone(),
    };
    let proxy_target = config
        .dev_server_url
        .clone()
        .unwrap_or_else(|| info.http_base_url.clone());
    let _ = BACKEND_INFO.set(info);
    protocol::set_target(&proxy_target);

    let mut window_created = false;
    let mut backoff_ms: u64 = 500;
    loop {
        eprintln!("[vitre] starting backend on 127.0.0.1:{port}");
        if SHUTTING_DOWN.load(Ordering::SeqCst) {
            return;
        }
        let mut child = match spawn_backend(&config, port, &token) {
            Ok(child) => child,
            Err(error) => {
                eprintln!("[vitre] failed to spawn backend: {error}");
                std::thread::sleep(Duration::from_millis(backoff_ms));
                backoff_ms = (backoff_ms * 2).min(10_000);
                continue;
            }
        };
        CURRENT_CHILD_PID.store(child.id(), Ordering::SeqCst);

        if wait_until_ready(port, &mut child, READINESS_TIMEOUT) {
            backoff_ms = 500;
            eprintln!("[vitre] backend ready at http://127.0.0.1:{port}");
            if window_created {
                let info = BACKEND_INFO.get().expect("backend info");
                let _ = handle.emit("t3code://bootstraps-changed", bootstraps_json(info));
            } else {
                window_created = true;
                create_main_window(&handle, &config);
            }
        } else {
            eprintln!("[vitre] backend did not become ready within the timeout");
        }

        let status = child.wait();
        CURRENT_CHILD_PID.store(0, Ordering::SeqCst);
        if SHUTTING_DOWN.load(Ordering::SeqCst) {
            return;
        }
        eprintln!("[vitre] backend exited ({status:?}); restarting in {backoff_ms}ms");
        std::thread::sleep(Duration::from_millis(backoff_ms));
        backoff_ms = (backoff_ms * 2).min(10_000);
    }
}
