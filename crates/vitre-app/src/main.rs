//! Vitre app shell: boots the Node sidecar under supervision and renders the
//! M1 chat core ([`chat::ChatApp`]) — thread list, chat view, composer.

mod chat;
mod files;
mod lsp;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    App, AppContext as _, Bounds, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions, point,
    px, size,
};
use gpui_component::{Root, Theme, ThemeRegistry};
use serde_json::Value;
use vitre_rpc::{EnvironmentHttp, RpcSession};
use vitre_sidecar::{BackendInfo, SidecarConfig, Supervisor};

#[derive(Debug, Clone)]
#[allow(dead_code)] // fields exist for the selftest `{summary:?}` print
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

        // T3 parity: the Electron app renders in DM Sans (OFL, vendored).
        if let Err(error) = cx.text_system().add_fonts(vec![
            include_bytes!("../fonts/DMSans_400Regular.ttf").into(),
            include_bytes!("../fonts/DMSans_400Regular_Italic.ttf").into(),
            include_bytes!("../fonts/DMSans_500Medium.ttf").into(),
            include_bytes!("../fonts/DMSans_600SemiBold.ttf").into(),
            include_bytes!("../fonts/DMSans_700Bold.ttf").into(),
        ]) {
            eprintln!("[vitre] failed to load bundled fonts: {error:#}");
        }
        // Vitre themes transcribe the Electron app's design tokens
        // (apps/web/src/index.css); the dark palette is the pure-black
        // `data-sidebar-version` variant the shell actually runs with. Their
        // `highlight` block is the editor half, transcribed from the
        // CodeMirror theme (codemirror/theme.ts) — without it the editor keeps
        // gpui-component's default *light* highlight theme in both modes.
        ThemeRegistry::global_mut(cx)
            .load_themes_from_str(include_str!("../themes/vitre.json"))
            .expect("vitre.json parses");
        {
            let registry = ThemeRegistry::global(cx);
            let light = registry.themes().get("Vitre Light").cloned();
            let dark = registry.themes().get("Vitre Dark").cloned();
            let theme = Theme::global_mut(cx);
            if let Some(light) = light {
                theme.light_theme = light;
            }
            if let Some(dark) = dark {
                theme.dark_theme = dark;
            }
        }

        // Editor commands. `file.save` is `mod+s` in Electron's
        // DEFAULT_KEYBINDINGS; formatting is a fixed editor chord there
        // (`Shift-Alt-f`), not a rebindable command. Both dispatch up the
        // focus path, so they reach the files panel from the editor and the
        // tree alike and do nothing when no file is open.
        cx.bind_keys([
            KeyBinding::new("cmd-s", files::SaveFile, None),
            KeyBinding::new("ctrl-s", files::SaveFile, None),
            KeyBinding::new("shift-alt-f", files::FormatDocument, None),
        ]);

        let supervisor = Arc::new(Supervisor::start(config));
        let status_rx = supervisor.status();
        cx.on_app_quit(move |_| {
            supervisor.shutdown();
            async {}
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // Electron parity: hiddenInset titlebar, traffic lights at
                // (16, 18) (apps/desktop/src/window/DesktopWindow.ts).
                titlebar: Some(TitlebarOptions {
                    title: Some("Vitre".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(16.), px(18.))),
                }),
                ..Default::default()
            },
            |window, cx| {
                gpui_component::Theme::sync_system_appearance(Some(window), cx);
                window
                    .observe_window_appearance(|window, cx| {
                        gpui_component::Theme::sync_system_appearance(Some(window), cx);
                    })
                    .detach();
                let shell = cx.new(|cx| chat::ChatApp::new(status_rx, window, cx));
                cx.new(|cx| Root::new(shell, window, cx))
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
