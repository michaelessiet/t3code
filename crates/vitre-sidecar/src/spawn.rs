//! Spawning one sidecar process: port selection, bootstrap-envelope delivery
//! over stdin (the server's `--bootstrap-fd 0` path), and readiness polling.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const DEFAULT_BACKEND_PORT: u16 = 3773;
const READINESS_PATH: &str = "/.well-known/t3/environment";

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

pub struct SidecarConfig {
    pub node_binary: String,
    pub server_entry: PathBuf,
    pub t3_home: PathBuf,
    pub fixed_port: Option<u16>,
}

/// Where the running sidecar lives and how to authenticate against it.
/// Always 127.0.0.1, never localhost (the server binds IPv4 only; `localhost`
/// can resolve IPv6-first).
#[derive(Debug, Clone)]
pub struct BackendInfo {
    pub port: u16,
    pub bootstrap_token: String,
}

impl BackendInfo {
    pub fn http_base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn ws_base_url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.port)
    }
}

pub(crate) fn random_hex_token() -> String {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).expect("OS randomness available");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn pick_port(fixed: Option<u16>) -> Option<u16> {
    if let Some(port) = fixed {
        return Some(port);
    }
    (DEFAULT_BACKEND_PORT..u16::MAX).find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok())
}

pub(crate) fn spawn_backend(
    config: &SidecarConfig,
    port: u16,
    bootstrap_token: &str,
) -> std::io::Result<Child> {
    std::fs::create_dir_all(&config.t3_home)?;

    // The child runs with its cwd set to $HOME (below), so a relative entry
    // path must be resolved against OUR cwd before the spawn.
    let server_entry = config.server_entry.canonicalize()?;

    let mut command = Command::new(&config.node_binary);
    command.arg(&server_entry).arg("--bootstrap-fd").arg("0");
    for name in T3CODE_ENV_NAMES {
        command.env_remove(name);
    }
    command.env_remove("ELECTRON_RUN_AS_NODE");
    command
        .current_dir(
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/")),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Put the backend and any descendants it launches in a dedicated process
    // group. The supervisor can then terminate the whole tree on app exit.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }

    let mut child = command.spawn()?;

    let envelope = serde_json::json!({
        "mode": "desktop",
        "noBrowser": true,
        "port": port,
        "t3Home": config.t3_home.to_string_lossy(),
        "host": "127.0.0.1",
        "desktopBootstrapToken": bootstrap_token,
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

/// Poll readiness until 200, child exit, or timeout.
pub(crate) fn wait_until_ready(port: u16, child: &mut Child, timeout: Duration) -> bool {
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

/// One-shot sidecar (no restart supervision) — used by spikes and selftests.
pub struct Sidecar {
    child: Child,
    pub port: u16,
    pub bootstrap_token: String,
}

impl Sidecar {
    pub fn spawn(config: &SidecarConfig) -> std::io::Result<Self> {
        let port = pick_port(config.fixed_port)
            .ok_or_else(|| std::io::Error::other("no free backend port from 3773 upward"))?;
        let bootstrap_token = random_hex_token();
        let child = spawn_backend(config, port, &bootstrap_token)?;
        Ok(Self {
            child,
            port,
            bootstrap_token,
        })
    }

    pub fn info(&self) -> BackendInfo {
        BackendInfo {
            port: self.port,
            bootstrap_token: self.bootstrap_token.clone(),
        }
    }

    pub fn http_base_url(&self) -> String {
        self.info().http_base_url()
    }

    pub fn ws_base_url(&self) -> String {
        self.info().ws_base_url()
    }

    pub fn wait_ready(&mut self, timeout: Duration) -> std::io::Result<()> {
        if wait_until_ready(self.port, &mut self.child, timeout) {
            Ok(())
        } else if let Ok(Some(status)) = self.child.try_wait() {
            Err(std::io::Error::other(format!(
                "sidecar exited before readiness: {status}"
            )))
        } else {
            Err(std::io::Error::other("sidecar readiness timeout"))
        }
    }

    /// Graceful teardown: SIGTERM (lets the server close its SQLite home),
    /// then SIGKILL if it lingers past five seconds.
    pub fn shutdown(mut self) {
        terminate(&mut self.child);
    }
}

pub(crate) fn terminate(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
}
