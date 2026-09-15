//! Regression pass using the disposable native fixture and real file reads.
use super::*;
use crate::chat::ui_verification;
use gpui::AsyncWindowContext;
use std::time::Duration;

const SOURCE: &str = "src/components/Workspace.tsx";
const DEFINITION: &str = "src/main.rs";

async fn wait_for_file(
    app: &WeakEntity<ChatApp>,
    path: &str,
    text: &str,
    cx: &mut AsyncWindowContext,
) -> Result<(), String> {
    for _ in 0..50 {
        cx.background_executor()
            .timer(Duration::from_millis(80))
            .await;
        let ready = app.update(cx, |app, cx| {
            let key = app.dock_thread_key().unwrap();
            let panel = app.files.as_ref().unwrap().read(cx);
            matches!(app.right_panel.map.active_surface(&key), Some(RightPanelSurface::File { relative_path, .. }) if relative_path == path)
                && panel.active_file_status().is_some_and(|(open, _)| open == path)
                && panel.verification_editor().read(cx).value().contains(text)
        }).map_err(|e| e.to_string())?;
        if ready {
            return Ok(());
        }
    }
    Err(format!(
        "Selected tab and editor contents did not converge on {path}"
    ))
}

impl ChatApp {
    pub(in crate::chat) fn verify_dock_tabs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |app, cx| {
            let run = async {
                ui_verification::float_fixture(cx).await?;
                cx.update(|w, _| w.resize(gpui::size(px(1440.), px(960.)))).map_err(|e| e.to_string())?;
                for maximized in [false, true] {
                    app.update_in(cx, |app, w, cx| {
                        app.right_panel_maximized = maximized;
                        app.dock_open_file(SOURCE.into(), None, w, cx);
                    }).map_err(|e| e.to_string())?;
                    wait_for_file(&app, SOURCE, "export function Workspace", cx).await?;
                    executor.timer(Duration::from_millis(500)).await;
                    let epoch = app.update(cx, |app, _| app.dock_reveal_epoch).map_err(|e| e.to_string())?;
                    // Exercise tree/file activation without changing dock visibility.
                    app.update_in(cx, |app, w, cx| app.dock_open_files_surface(w, cx)).map_err(|e| e.to_string())?;
                    executor.timer(Duration::from_millis(120)).await;
                    app.update_in(cx, |app, w, cx| app.dock_activate_surface(&vitre_state::right_panel::file_surface_id(None, SOURCE), w, cx)).map_err(|e| e.to_string())?;
                    wait_for_file(&app, SOURCE, "export function Workspace", cx).await?;
                    let stable = app.update(cx, |app, _| app.dock_reveal_epoch == epoch).map_err(|e| e.to_string())?;
                    if !stable { eprintln!("[vitre-tabs-test] REGRESSION: tab switching restarted dock entrance"); }

                    // Feed a resolved cross-file location through the editor's real
                    // definition-navigation callback. No language-server install needed.
                    let (show, uri) = app.update(cx, |app, cx| {
                        let panel = app.files.as_ref().unwrap().read(cx);
                        let show = panel.verification_editor().read(cx).lsp().show_document.clone().unwrap();
                        (show, crate::lsp::bridge::file_uri(panel.cwd(), DEFINITION).unwrap())
                    }).map_err(|e| e.to_string())?;
                    let params = serde_json::from_value(serde_json::json!({"uri":uri,"selection":{"start":{"line":0,"character":3},"end":{"line":0,"character":7}}})).map_err(|e| e.to_string())?;
                    cx.update(|w, cx| show(&params, w, cx)).map_err(|e| e.to_string())?;
                    wait_for_file(&app, DEFINITION, "Hello, Vitre", cx).await?;
                    // Close with the same keyboard action as the normal editor UI.
                    ui_verification::key("cmd-w", cx).await?;
                    // Files was appended after SOURCE above: close selects Files.
                    app.update_in(cx, |app, w, cx| app.dock_activate_surface(&vitre_state::right_panel::file_surface_id(None, SOURCE), w, cx)).map_err(|e| e.to_string())?;
                    wait_for_file(&app, SOURCE, "export function Workspace", cx).await?;

                    // Reopen, then remove the Files tab so close must automatically
                    // select SOURCE without an explicit click/focus request.
                    app.update_in(cx, |app, w, cx| app.dock_close_surface("files", w, cx)).map_err(|e| e.to_string())?;
                    cx.update(|w, cx| show(&params, w, cx)).map_err(|e| e.to_string())?;
                    wait_for_file(&app, DEFINITION, "Hello, Vitre", cx).await?;
                    ui_verification::key("cmd-w", cx).await?;
                    wait_for_file(&app, SOURCE, "export function Workspace", cx).await?;
                    if !stable { return Err("Tab switching restarted the dock entrance".into()); }

                    // A → B → A before B's read completes must keep A active.
                    app.update_in(cx, |app, w, cx| {
                        app.dock_open_file("package.json".into(), None, w, cx);
                        app.dock_activate_surface(&vitre_state::right_panel::file_surface_id(None, SOURCE), w, cx);
                    }).map_err(|e| e.to_string())?;
                    executor.timer(Duration::from_millis(700)).await;
                    wait_for_file(&app, SOURCE, "export function Workspace", cx).await?;
                    ui_verification::capture(if maximized {"tabs-overlay"} else {"tabs-inline"}, cx).await?;
                }
                Ok::<_, String>(())
            };
            let result = tokio::select! {
                result = run => result,
                _ = executor.timer(Duration::from_secs(50)) => Err("Dock verification timed out".into()),
            };
            match result {
                Ok(()) => eprintln!("[vitre-tabs-test] PASS: stable dock animation, tree/file switching, definition navigation and Cmd+W fallback, rapid tab switching; inline and overlay layouts"),
                Err(error) => eprintln!("[vitre-tabs-test] FAIL: {error}"),
            }
        }).detach();
    }
}
