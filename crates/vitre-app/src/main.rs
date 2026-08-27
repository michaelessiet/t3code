//! Vitre shell, milestone M0: boot the Node sidecar under supervision,
//! exchange the bootstrap token, connect the RPC session, and render the
//! authenticated environment. Later milestones grow this into the full app.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    App, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    size,
};
use gpui_component::{ActiveTheme as _, Root, h_flex, v_flex};
use gpui_tokio::Tokio;
use serde_json::Value;
use tokio::sync::watch;
use vitre_rpc::{EnvironmentHttp, RpcSession};
use vitre_sidecar::{BackendInfo, SidecarConfig, Supervisor, SupervisorStatus};

struct VitreShell {
    sidecar_status: SharedString,
    connection: ConnectionStage,
}

enum ConnectionStage {
    Waiting,
    Connecting,
    Connected(Box<EnvSummary>),
    Error(SharedString),
}

#[derive(Debug, Clone)]
struct EnvSummary {
    http_base_url: String,
    scopes: String,
    cwd: String,
    providers: usize,
    keybindings: usize,
    ping: Duration,
}

/// The full M0 client path: token exchange → ticket → WS → `server.getConfig`
/// → application-level Ping. Runs on the tokio runtime (reqwest/tungstenite).
async fn fetch_environment(info: BackendInfo) -> anyhow::Result<EnvSummary> {
    let http = EnvironmentHttp::new(info.http_base_url());
    let token = http.exchange_bootstrap_token(&info.bootstrap_token).await?;
    let ticket = http.websocket_ticket(&token.access_token).await?;
    let session = RpcSession::connect(&http.ws_url(&ticket)).await?;
    let config = session
        .call("server.getConfig", serde_json::json!({}))
        .await?;
    let ping = session.ping(Duration::from_secs(5)).await?;

    let count = |key: &str| -> usize {
        match config.get(key) {
            Some(Value::Array(items)) => items.len(),
            Some(Value::Object(map)) => map.len(),
            _ => 0,
        }
    };
    Ok(EnvSummary {
        http_base_url: http.base_url.clone(),
        scopes: token.scope,
        cwd: config
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string(),
        providers: count("providers"),
        keybindings: count("keybindings"),
        ping,
    })
}

impl VitreShell {
    fn new(mut status_rx: watch::Receiver<SupervisorStatus>, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                let status = status_rx.borrow_and_update().clone();
                let ready = match &status {
                    SupervisorStatus::Ready { info } => Some(info.clone()),
                    _ => None,
                };
                let line: SharedString = describe_status(&status).into();
                let updated = this.update(cx, |shell, cx| {
                    shell.sidecar_status = line;
                    if ready.is_some() {
                        shell.connection = ConnectionStage::Connecting;
                    }
                    cx.notify();
                });
                if updated.is_err() {
                    return;
                }
                if let Some(info) = ready {
                    let outcome = cx
                        .update(|cx| Tokio::spawn_result(cx, fetch_environment(info)))
                        .await;
                    let updated = this.update(cx, |shell, cx| {
                        shell.connection = match outcome {
                            Ok(env) => ConnectionStage::Connected(Box::new(env)),
                            Err(error) => ConnectionStage::Error(format!("{error:#}").into()),
                        };
                        cx.notify();
                    });
                    if updated.is_err() {
                        return;
                    }
                }
                if status_rx.changed().await.is_err() {
                    return;
                }
            }
        })
        .detach();

        Self {
            sidecar_status: "starting…".into(),
            connection: ConnectionStage::Waiting,
        }
    }
}

fn describe_status(status: &SupervisorStatus) -> String {
    match status {
        SupervisorStatus::Idle => "idle".into(),
        SupervisorStatus::Starting { attempt } => format!("starting (attempt {attempt})"),
        SupervisorStatus::Ready { info } => format!("ready on 127.0.0.1:{}", info.port),
        SupervisorStatus::Backoff { delay_ms, .. } => {
            format!("exited — restarting in {delay_ms}ms")
        }
        SupervisorStatus::Stopped { reason } => format!("stopped: {reason}"),
    }
}

impl Render for VitreShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let row = |label: &'static str, value: SharedString| {
            h_flex()
                .gap_3()
                .items_baseline()
                .child(div().w(px(110.)).text_sm().text_color(muted).child(label))
                .child(div().text_sm().child(value))
        };

        let mut body = v_flex()
            .gap_2()
            .child(row("sidecar", self.sidecar_status.clone()));
        body = match &self.connection {
            ConnectionStage::Waiting => {
                body.child(row("environment", "waiting for sidecar…".into()))
            }
            ConnectionStage::Connecting => body.child(row("environment", "authenticating…".into())),
            ConnectionStage::Error(error) => body.child(row("error", error.clone())),
            ConnectionStage::Connected(env) => body
                .child(row("environment", env.http_base_url.clone().into()))
                .child(row("cwd", env.cwd.clone().into()))
                .child(row(
                    "rpc",
                    format!("connected — ping {:?}", env.ping).into(),
                ))
                .child(row(
                    "config",
                    format!(
                        "{} provider(s), {} keybinding(s)",
                        env.providers, env.keybindings
                    )
                    .into(),
                ))
                .child(row("scopes", env.scopes.clone().into())),
        };

        v_flex()
            .size_full()
            .p_6()
            .gap_4()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(div().text_2xl().child("Vitre"))
            .child(body)
    }
}

fn resolve_server_entry() -> PathBuf {
    if let Ok(entry) = std::env::var("VITRE_SERVER_ENTRY") {
        return PathBuf::from(entry);
    }
    // Packaged layout (M6): Resources/server/bin.mjs next to the executable.
    if let Ok(exe) = std::env::current_exe()
        && let Some(resources) = exe
            .parent()
            .and_then(|dir| dir.parent())
            .map(|dir| dir.join("Resources/server/bin.mjs"))
        && resources.exists()
    {
        return resources;
    }
    // Dev layout: run from the repo root.
    PathBuf::from("apps/server/dist/bin.mjs")
}

fn vitre_home() -> PathBuf {
    if let Ok(home) = std::env::var("VITRE_HOME") {
        return PathBuf::from(home);
    }
    std::env::var("HOME")
        .map(PathBuf::from)
        .expect("HOME is set")
        .join(".vitre")
}

/// Headless verification for packaged builds (`VITRE_SELFTEST=1`, typically
/// under `env -i`): boot the sidecar, run the full auth + RPC path, print the
/// result, exit non-zero on any failure. No window, no gpui.
fn selftest(config: SidecarConfig) -> ! {
    let outcome = (|| -> anyhow::Result<EnvSummary> {
        let mut sidecar = vitre_sidecar::Sidecar::spawn(&config)?;
        sidecar.wait_ready(Duration::from_secs(60))?;
        let info = sidecar.info();
        let runtime = tokio::runtime::Runtime::new()?;
        let summary = runtime.block_on(fetch_environment(info))?;
        sidecar.shutdown();
        Ok(summary)
    })();
    match outcome {
        Ok(summary) => {
            println!("[vitre-selftest] PASS {summary:?}");
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("[vitre-selftest] FAIL {error:#}");
            std::process::exit(1);
        }
    }
}

fn main() {
    let config = SidecarConfig {
        node_binary: std::env::var("VITRE_NODE").unwrap_or_else(|_| "node".into()),
        server_entry: resolve_server_entry(),
        t3_home: vitre_home(),
        fixed_port: None,
    };

    if std::env::var("VITRE_SELFTEST").is_ok_and(|value| !value.is_empty() && value != "0") {
        selftest(config);
    }

    let app = gpui_platform::application().with_assets(gpui_component_assets::Assets);
    app.run(move |cx: &mut App| {
        gpui_tokio::init(cx);
        gpui_component::init(cx);

        let supervisor = Arc::new(Supervisor::start(config));
        let status_rx = supervisor.status();
        cx.on_app_quit(move |_| {
            supervisor.shutdown();
            async {}
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(760.), px(500.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                gpui_component::Theme::sync_system_appearance(Some(window), cx);
                window
                    .observe_window_appearance(|window, cx| {
                        gpui_component::Theme::sync_system_appearance(Some(window), cx);
                    })
                    .detach();
                let shell = cx.new(|cx| VitreShell::new(status_rx, cx));
                cx.new(|cx| Root::new(shell, window, cx))
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
