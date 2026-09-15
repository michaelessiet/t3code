//! Native regression pass, called only by the disposable icon fixture (mode `ui`).
use super::*;

/// Tile managers can immediately undo `Window::resize`. Float only this
/// disposable fixture window, if AeroSpace owns it; never touch other apps.
pub(super) async fn float_fixture(cx: &mut gpui::AsyncWindowContext) -> Result<(), String> {
    let pid = std::process::id().to_string();
    Tokio::spawn_result(cx, async move {
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let Ok(output) = std::process::Command::new("aerospace")
                .args([
                    "list-windows",
                    "--all",
                    "--format",
                    "%{window-id} %{app-pid}",
                ])
                .output()
            else {
                return Ok(());
            };
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let mut fields = line.split_whitespace();
                let Some(id) = fields.next() else {
                    continue;
                };
                if fields.next() == Some(pid.as_str()) {
                    anyhow::ensure!(
                        std::process::Command::new("aerospace")
                            .args(["layout", "--window-id", id, "floating"])
                            .status()?
                            .success(),
                        "could not float fixture window"
                    );
                }
            }
            Ok(())
        })
        .await?
    })
    .await
    .map_err(|e| e.to_string())
}

pub(crate) async fn key(key: &str, cx: &mut gpui::AsyncWindowContext) -> Result<(), String> {
    let keystroke = gpui::Keystroke::parse(key).map_err(|e| e.to_string())?;
    cx.update(|window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                keystroke,
                is_held: false,
                prefer_character_input: false,
            }),
            cx,
        );
    })
    .map_err(|e| e.to_string())
}

pub(crate) async fn capture(name: &str, cx: &mut gpui::AsyncWindowContext) -> Result<(), String> {
    cx.update(|window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                position: gpui::point(px(20.), px(400.)),
                pressed_button: None,
                modifiers: Default::default(),
            }),
            cx,
        )
    })
    .map_err(|e| e.to_string())?;
    let path = format!("/tmp/vitre-ui-{name}.png");
    let pid = std::process::id();
    Tokio::spawn_result(cx, async move {
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let script = format!(r#"import AppKit
let pid: Int32 = {pid}
NSRunningApplication(processIdentifier: pid)?.activate(options: [.activateAllWindows])
let windows = CGWindowListCopyWindowInfo([.optionAll, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
let owned = windows.filter {{ $0[kCGWindowOwnerPID as String] as? Int32 == pid && $0[kCGWindowLayer as String] as? Int == 0 }}
func area(_ w: [String: Any]) -> Double {{ let b = w[kCGWindowBounds as String] as? [String: Double] ?? [:]; return (b["Width"] ?? 0) * (b["Height"] ?? 0) }}
if let w = owned.max(by: {{area($0) < area($1)}}) {{ print(w[kCGWindowNumber as String] as! Int) }}"#);
            let output = std::process::Command::new("swift").args(["-e", &script]).output()?;
            anyhow::ensure!(output.status.success(), "window capture lookup failed");
            let id = String::from_utf8(output.stdout)?;
            anyhow::ensure!(!id.trim().is_empty(), "native window not found");
            let status = std::process::Command::new("screencapture").args(["-x", "-l", id.trim(), &path]).status()?;
            anyhow::ensure!(status.success(), "screenshot failed");
            eprintln!("[vitre-ui-test] screenshot {path}");
            Ok(())
        }).await?
    }).await.map_err(|e| e.to_string())
}

impl ChatApp {
    pub(super) fn verify_surface_ui(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let test = async {
                executor.timer(Duration::from_millis(650)).await;
                let editor = this.update_in(cx, |a, w, cx| {
                    let editor = a.files.as_ref().unwrap().read(cx).verification_editor();
                    editor.update(cx, |s, cx| { s.set_selected_range(0..0, cx); s.focus(w, cx); });
                    editor
                }).map_err(|e| e.to_string())?;
                let original = editor.read_with(cx, |s, _| s.value());
                key("cmd-/", cx).await?;
                if !editor.read_with(cx, |s, _| s.value().starts_with("// ")) { return Err("Cmd+/ did not comment in the real files panel".into()); }
                key("cmd-z", cx).await?;
                if editor.read_with(cx, |s, _| s.value()) != original {return Err("Undo did not restore the file".into());}
                capture("editor", cx).await?;
                this.update_in(cx, |a, w, cx| {
                    if a.dock_open() {a.toggle_right_panel(w, cx);}
                    // Visual-only chat data; no paid provider turn and no durable messages.
                    let view = Arc::make_mut(a.thread.as_mut().unwrap().state.view.as_mut().unwrap());
                    view.messages = (0..6).map(|i| serde_json::from_value(serde_json::json!({
                        "id": format!("ui-message-{i}"), "role": if i % 2 == 0 {"user"} else {"assistant"},
                        "text": if i % 2 == 0 {format!("Refine workspace section {}", i / 2 + 1)} else {"Use `cargo test` to verify the change. The `café` value and `src/components/Workspace.tsx` should remain readable on glass.\n\nThe editor supports **line commands**, undo, and Unicode selections.".into()},
                        "createdAt": now_iso(), "updatedAt": now_iso(), "streaming": false, "turnId": null
                    })).unwrap()).collect();
                    a.rebuild_timeline(cx); cx.notify();
                }).map_err(|e| e.to_string())?;
                executor.timer(Duration::from_millis(800)).await;
                capture("chat", cx).await?;
                this.update(cx, |a, cx| {a.sidebar_visible = false; cx.notify();}).map_err(|e| e.to_string())?;
                executor.timer(Duration::from_millis(600)).await;
                capture("sidebar-closed", cx).await?;
                this.update_in(cx, |a, w, cx| {a.sidebar_visible = true; a.open_model_picker(w, cx); cx.notify();}).map_err(|e| e.to_string())?;
                executor.timer(Duration::from_millis(700)).await;
                capture("model-menu", cx).await?;
                let claude_id = this.update(cx, |a, cx| -> Result<String, String> {
                    let picker = a.model_picker.as_ref().ok_or("Model picker did not open")?;
                    let picker_state = picker.read(cx);
                    let provider_summary = picker_state.providers.iter().map(|provider| {
                        format!("{}:{}:{:?}", provider.instance_id.0, provider.driver.0, provider.status)
                    }).collect::<Vec<_>>();
                    let has_codex = picker_state.providers.iter().any(|provider| {
                        provider_picker_ready(provider)
                            && (provider.instance_id.0 == "codex" || provider.driver.0 == "codex")
                    });
                    let claude_id = picker_state.providers.iter().find(|provider| {
                        provider_picker_ready(provider)
                            && (provider.instance_id.0 == "claudeAgent"
                                || provider.driver.0 == "claudeAgent")
                    }).map(|provider| provider.instance_id.0.clone());
                    if !has_codex || claude_id.is_none() {
                        return Err(format!(
                            "Model picker did not expose both ready Codex and Claude providers: {}",
                            provider_summary.join(", ")
                        ));
                    }
                    let claude_id = claude_id.unwrap();
                    let selected = claude_id.clone();
                    picker.update(cx, |picker, cx| {
                        picker.selected_provider = Some(selected);
                        picker.active_choice = 0;
                        cx.notify();
                    });
                    Ok(claude_id)
                }).map_err(|e| e.to_string())??;
                executor.timer(Duration::from_millis(350)).await;
                let claude_models_visible = this.update(cx, |a, cx| {
                    a.model_picker.as_ref().is_some_and(|picker| {
                        let picker = picker.read(cx);
                        !picker.choices.is_empty() && picker.choices.iter().all(|(selection, _)| {
                            model_selection_identity(selection)
                                .is_some_and(|(instance, _)| instance == claude_id)
                        })
                    })
                }).map_err(|e| e.to_string())?;
                if !claude_models_visible {
                    return Err("Selecting Claude did not render its live model list".into());
                }
                capture("model-menu-claude", cx).await?;
                key("down", cx).await?;
                let moved = this.update(cx, |a, cx| a.model_picker.as_ref().is_some_and(|p| p.read(cx).active_choice == 1 || p.read(cx).choices.len() <= 1)).map_err(|e| e.to_string())?;
                if !moved {return Err("Model menu arrow-key navigation failed".into());}
                key("escape", cx).await?;
                executor.timer(Duration::from_millis(150)).await;
                if this.update(cx, |a, _| a.model_picker.is_some()).map_err(|e| e.to_string())? {return Err("Escape did not dismiss model menu".into());}
                this.update_in(cx, |a, w, cx| a.toggle_quick_search(QuickSearchMode::Open, w, cx)).map_err(|e| e.to_string())?;
                executor.timer(Duration::from_millis(700)).await;
                capture("search-card", cx).await?;
                let command = this.update(cx, |a, cx| a.quick_search.as_ref().unwrap().read(cx).verification_state()).map_err(|e| e.to_string())?;
                cx.update(|w, cx| command.update(cx, |s, cx| s.set_query("Workspace.tsx", w, cx))).map_err(|e| e.to_string())?;
                loop {
                    executor.timer(Duration::from_millis(80)).await;
                    if this.update(cx, |a, cx| a.quick_search.as_ref().unwrap().read(cx).verification_preview_ready()).map_err(|e| e.to_string())? {break;}
                }
                executor.timer(Duration::from_millis(600)).await;
                capture("search-code", cx).await?;
                this.update_in(cx, |a, w, cx| {a.toggle_quick_search(QuickSearchMode::Open, w, cx); a.toggle_right_panel(w, cx);}).map_err(|e| e.to_string())?;
                Ok::<_, String>(())
            };
            let result = tokio::select! { result = test => result, _ = executor.timer(Duration::from_secs(75)) => Err("UI verification timed out".into()) };
            match result {
                Ok(()) => eprintln!("[vitre-ui-test] PASS: real editor Cmd+/ and Undo, chat rail/code chips, sidebar/dock transitions, live Codex+Claude model switching and keyboard navigation/Escape, search card-to-code preview; screenshots in /tmp/vitre-ui-*.png"),
                Err(error) => eprintln!("[vitre-ui-test] FAIL: {error}"),
            }
        }).detach();
    }
}
