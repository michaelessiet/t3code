//! Vitre app shell: boots the Node sidecar under supervision and renders the
//! M1 chat core ([`chat::ChatApp`]) — thread list, chat view, composer.

mod assets;
mod chat;
mod client_settings;
mod desktop;
mod files;
mod git_gutter;
mod glass;
mod graph;
mod icons;
mod keymap;
mod lsp;
mod palette;
mod search;
mod settings;
mod sidebar_prefs;
mod ui;
mod vim;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    App, AppContext as _, KeyBinding, Styled as _, TitlebarOptions, WindowBackgroundAppearance,
    WindowOptions, point, px, size,
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

fn packaged_resource_for_executable(executable: &Path, relative: &str) -> Option<PathBuf> {
    executable
        .parent()
        .and_then(Path::parent)
        .map(|contents| contents.join("Resources").join(relative))
}

fn packaged_resource(relative: &str) -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|executable| packaged_resource_for_executable(&executable, relative))
        .filter(|path| path.exists())
}

fn resolve_server_entry() -> PathBuf {
    if let Ok(entry) = std::env::var("VITRE_SERVER_ENTRY") {
        return PathBuf::from(entry);
    }
    // Packaged layout (M6): Resources/server/bin.mjs next to the executable.
    if let Some(entry) = packaged_resource("server/bin.mjs") {
        return entry;
    }
    // Dev layout: run from the repo root.
    PathBuf::from("apps/server/dist/bin.mjs")
}

fn resolve_node_binary() -> String {
    if let Ok(binary) = std::env::var("VITRE_NODE") {
        return binary;
    }
    packaged_resource("node/bin/node")
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "node".into())
}

/// Electron's `mod+` chord prefix, resolved for this platform. Binding both
/// `cmd-` and `ctrl-` everywhere would work, but the command palette resolves
/// its shortcut chips from the live keymap and would then advertise the wrong
/// one of the pair.
fn modified(chord: &str) -> String {
    #[cfg(target_os = "macos")]
    let prefix = "cmd";
    #[cfg(not(target_os = "macos"))]
    let prefix = "ctrl";
    format!("{prefix}-{chord}")
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
    let home = vitre_home();
    let config = SidecarConfig {
        node_binary: resolve_node_binary(),
        server_entry: resolve_server_entry(),
        t3_home: home.clone(),
        fixed_port: None,
    };

    if std::env::var("VITRE_SELFTEST").is_ok_and(|value| !value.is_empty() && value != "0") {
        selftest(config);
    }

    let app = gpui_platform::application().with_assets(assets::VitreAssets);
    let (link_tx, mut link_rx) = tokio::sync::mpsc::unbounded_channel();
    app.on_open_urls(move |urls| {
        for url in urls {
            if let Some(id) = desktop::thread_from_url(&url) {
                let _ = link_tx.send(id);
            }
        }
    });
    app.on_reopen(|cx| cx.activate(true));
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

        // Vim mode: the preference plus the modal keymap. Bound after
        // `gpui_component::init` on purpose — gpui breaks same-depth binding
        // ties by registration order, which is what lets vim claim `escape`
        // and friends back from the input's own keymap (see `vim`).
        client_settings::ClientSettings::init(cx, &home);
        vim::init(cx);
        desktop::init(cx);
        files::commands::init(cx);
        files::debugger::init(cx);

        // Editor commands. `file.save` is `mod+s` in Electron's
        // DEFAULT_KEYBINDINGS; formatting is a fixed editor chord there
        // (`Shift-Alt-f`), not a rebindable command. Both dispatch up the
        // focus path, so they reach the files panel from the editor and the
        // tree alike and do nothing when no file is open.
        cx.bind_keys([
            KeyBinding::new("down", chat::ModelPickerNext, Some("ModelPickerInput")),
            KeyBinding::new("up", chat::ModelPickerPrevious, Some("ModelPickerInput")),
            KeyBinding::new("enter", chat::ModelPickerConfirm, Some("ModelPickerInput")),
            KeyBinding::new("shift-alt-f", files::FormatDocument, None),
            // Git hunk navigation, the chords Electron's git gutter binds on
            // the editor itself (`gitDiffGutter.ts`), not rebindable commands.
            KeyBinding::new("alt-f5", files::GotoNextHunk, None),
            KeyBinding::new("shift-alt-f5", files::GotoPreviousHunk, None),
            // Go to definition. Electron binds `F12` in the editor's own LSP
            // keymap (`lspBridge.ts`), alongside mod-click and vim `gd`.
            KeyBinding::new("f12", files::GoToDefinition, None),
            KeyBinding::new("shift-f12", files::FindReferences, Some("Editor")),
            KeyBinding::new("f2", files::RenameSymbol, Some("Editor && !vim_command")),
            KeyBinding::new(
                "secondary-.",
                files::ShowCodeActions,
                Some("Editor && !vim_command"),
            ),
            KeyBinding::new(
                "secondary-alt-j",
                files::InsertSnippet,
                Some("Editor && !vim_command"),
            ),
            KeyBinding::new(
                "secondary-d",
                files::SelectNextOccurrence,
                Some("Editor && !vim_command"),
            ),
            KeyBinding::new(
                "secondary-shift-l",
                files::SelectAllOccurrences,
                Some("Editor && !vim_command"),
            ),
            KeyBinding::new(
                "secondary-alt-up",
                files::AddCursorAbove,
                Some("Editor && !vim_command"),
            ),
            KeyBinding::new(
                "secondary-alt-down",
                files::AddCursorBelow,
                Some("Editor && !vim_command"),
            ),
            KeyBinding::new(
                "secondary-shift-space",
                files::ShowSignatureHelp,
                Some("Editor"),
            ),
            KeyBinding::new("ctrl-space", files::ShowCompletions, Some("Editor")),
            // `editor.showCompletions`, `mod+i` when `editorFocus` in
            // DEFAULT_KEYBINDINGS. The `editorFocus` half is the panel's own
            // check that the buffer holds focus.
        ]);

        // File-tree keyboard navigation. Electron gets the arrows, Home/End
        // and Enter from `@pierre/trees` (its roving-focus keydown handler,
        // plus rows being `<button>`s), and layers the vim motions on top by
        // re-dispatching them as those same keys — unconditionally, not gated
        // on the `vimMode` setting, since a tree row is never a text field.
        // Here both halves are ordinary bindings in the tree's own context,
        // which is present only while no inline rename/create is being typed.
        cx.bind_keys([
            KeyBinding::new("down", files::TreeFocusNext, Some("FileTree")),
            KeyBinding::new("up", files::TreeFocusPrevious, Some("FileTree")),
            KeyBinding::new("right", files::TreeExpandOrNext, Some("FileTree")),
            KeyBinding::new("left", files::TreeCollapseOrParent, Some("FileTree")),
            KeyBinding::new("home", files::TreeFocusFirst, Some("FileTree")),
            KeyBinding::new("end", files::TreeFocusLast, Some("FileTree")),
            KeyBinding::new("enter", files::TreeActivate, Some("FileTree")),
            KeyBinding::new(
                if cfg!(target_os = "macos") {
                    "cmd-c"
                } else {
                    "ctrl-c"
                },
                files::TreeCopy,
                Some("FileTree"),
            ),
            KeyBinding::new(
                if cfg!(target_os = "macos") {
                    "cmd-v"
                } else {
                    "ctrl-v"
                },
                files::TreePaste,
                Some("FileTree"),
            ),
            KeyBinding::new("space", files::TreeActivate, Some("FileTree")),
            KeyBinding::new("j", files::TreeFocusNext, Some("FileTree")),
            KeyBinding::new("k", files::TreeFocusPrevious, Some("FileTree")),
            KeyBinding::new("l", files::TreeExpandOrNext, Some("FileTree")),
            KeyBinding::new("h", files::TreeCollapseOrParent, Some("FileTree")),
            KeyBinding::new("shift-g", files::TreeFocusLast, Some("FileTree")),
            KeyBinding::new("g g", files::TreeFocusFirst, Some("FileTree")),
        ]);

        // Palette surfaces and the workspace commands their rows advertise.
        // Every chord here is the Electron DEFAULT_KEYBINDINGS default for the
        // named command (`quickSearch.open`, `quickSearch.content`,
        // `commandPalette.toggle`, `chat.new`, `rightPanel.toggle`); they are
        // rebindable there, which lands with the keymap work. The command
        // palette resolves its shortcut chips from these bindings, so a chord
        // changed here changes the chip too.
        cx.bind_keys([
            // Electron has no rebindable command for settings: `mod+,` is an
            // application-menu accelerator (DesktopApplicationMenu.ts), and in
            // the browser there is no shortcut at all. Escape leaves, scoped to
            // the settings surface's own key context so it never shadows the
            // editor's or a dialog's escape.
            KeyBinding::new(&modified(","), settings::SettingsOpen, None),
            KeyBinding::new("escape", settings::SettingsClose, Some("Settings")),
        ]);

        // Server-owned shortcuts are installed by keymap::install after the
        // connection arrives. Fixed copies here would survive a user's remove
        // or rebind and silently restore the old chord.

        let supervisor = Arc::new(Supervisor::start(config));
        let status_rx = supervisor.status();
        cx.on_app_quit(move |_| {
            supervisor.shutdown();
            async {}
        })
        .detach();

        let bounds = desktop::load_bounds(&home, cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(bounds),
                window_min_size: Some(size(px(640.), px(480.))),
                // Electron parity: hiddenInset titlebar, traffic lights at
                // (16, 18) (apps/desktop/src/window/DesktopWindow.ts).
                titlebar: Some(TitlebarOptions {
                    title: Some("Vitre".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(16.), px(18.))),
                }),
                window_background: WindowBackgroundAppearance::Blurred,
                ..Default::default()
            },
            |window, cx| {
                // The theme setting decides whether the OS gets a vote:
                // "System" follows the window appearance, light/dark pin the
                // corresponding Vitre palette and ignore it (Electron's
                // `useTheme` resolves the stored `t3code:theme` the same way).
                let apply = |window: &mut gpui::Window, cx: &mut App| {
                    settings::apply_theme(
                        client_settings::ClientSettings::theme(cx),
                        Some(window),
                        cx,
                    );
                };
                apply(window, cx);
                window
                    .observe_window_appearance(move |window, cx| apply(window, cx))
                    .detach();
                let shell = cx.new(|cx| chat::ChatApp::new(&home, status_rx, window, cx));
                let shell_links = shell.downgrade();
                cx.spawn(async move |cx| {
                    while let Some(id) = link_rx.recv().await {
                        if shell_links
                            .update(cx, |shell, cx| shell.open_deep_link(id, cx))
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .detach();
                window.set_rem_size(px(client_settings::ClientSettings::get(cx).ui_font_size));
                let saved_home = home.clone();
                shell.update(cx, |_, cx| {
                    cx.observe_window_bounds(window, move |_, window, _| {
                        desktop::save_bounds(&saved_home, window)
                    })
                    .detach();
                });
                window.on_window_should_close(cx, |_, cx| {
                    cx.quit();
                    true
                });
                // Root paints an opaque theme background by default. Clear
                // that outer layer too, so the pane tints reach the native blur.
                cx.new(|cx| Root::new(shell, window, cx).bg(glass::root(cx)))
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}

#[cfg(test)]
mod packaging_tests {
    use super::*;

    #[test]
    fn packaged_resources_resolve_from_the_macos_bundle_executable() {
        let executable = Path::new("/Applications/Vitre.app/Contents/MacOS/Vitre");
        assert_eq!(
            packaged_resource_for_executable(executable, "server/bin.mjs"),
            Some(PathBuf::from(
                "/Applications/Vitre.app/Contents/Resources/server/bin.mjs"
            ))
        );
        assert_eq!(
            packaged_resource_for_executable(executable, "node/bin/node"),
            Some(PathBuf::from(
                "/Applications/Vitre.app/Contents/Resources/node/bin/node"
            ))
        );
    }
}
