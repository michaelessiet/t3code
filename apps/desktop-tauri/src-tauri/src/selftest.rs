//! Headless end-to-end self-test for the preview subsystem (M2).
//!
//! Enabled by `T3CODE_TAURI_PREVIEW_SELFTEST=1`. Waits for the main window
//! (i.e. backend + web dev server up), serves a local guest page, then drives
//! the preview manager through the same command functions the shim invokes:
//! tab lifecycle, bounds, navigation, Playwright locator click, type, press,
//! scroll, waitFor, evaluate, snapshot (incl. console/network capture),
//! screenshot artifact, zoom, color-scheme emulation, the element-picker
//! annotation flow (M3), and picture-in-picture (M3). Prints a
//! `[PASS]`/`[FAIL]` report to stderr and exits the app with 0/1.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::preview;

const GUEST_PORT: u16 = 43171;
const TAB: &str = "selftest";

fn guest_page(label: &str) -> String {
    format!(
        r##"<!doctype html>
<html><head><title>t3p-{label}</title></head>
<body style="margin:16px;font-family:sans-serif;height:3000px">
<button id="btn" onclick="document.title='clicked-ok';document.getElementById('out').textContent='clicked-marker'">Click me</button>
<a href="/two">second page</a>
<input id="inp" placeholder="type here">
<div id="out"></div>
<script>
  console.log("hello-from-guest");
  fetch("/ping").catch(() => null);
</script>
</body></html>"##
    )
}

fn spawn_guest_server() {
    std::thread::spawn(|| {
        let Ok(listener) = TcpListener::bind(("127.0.0.1", GUEST_PORT)) else {
            eprintln!("[selftest] guest port {GUEST_PORT} unavailable");
            return;
        };
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    return;
                }
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                        break;
                    }
                }
                let (content_type, body) = match path.as_str() {
                    "/ping" => ("text/plain", "pong".to_string()),
                    "/two" => ("text/html", guest_page("two")),
                    _ => ("text/html", guest_page("one")),
                };
                let mut stream = reader.into_inner();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len(),
                );
            });
        }
    });
}

pub fn maybe_run(app: &AppHandle) {
    if std::env::var("T3CODE_TAURI_PREVIEW_SELFTEST").is_err() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        spawn_guest_server();
        let mut report = String::from("\n===== PREVIEW SELFTEST =====\n");
        let failed = run(&app, &mut report).is_err();
        report.push_str(&format!(
            "===== PREVIEW SELFTEST {} =====\n",
            if failed { "FAILED" } else { "PASSED" }
        ));
        eprintln!("{report}");
        std::thread::sleep(Duration::from_millis(250));
        app.exit(if failed { 1 } else { 0 });
    });
}

fn check(report: &mut String, name: &str, result: Result<String, String>) -> Result<(), String> {
    match result {
        Ok(detail) => {
            report.push_str(&format!("[PASS] {name}: {detail}\n"));
            Ok(())
        }
        Err(error) => {
            report.push_str(&format!("[FAIL] {name}: {error}\n"));
            Err(error)
        }
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(future)
}

fn eval(app: &AppHandle, body: &str) -> Result<serde_json::Value, String> {
    preview::run_eval(app, TAB, body, false, Duration::from_secs(10))
}

fn wait_until(
    timeout: Duration,
    mut condition: impl FnMut() -> Result<bool, String>,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if condition()? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("condition not met before timeout".to_string());
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn run(app: &AppHandle, report: &mut String) -> Result<(), String> {
    // 0. The main window must exist before child webviews can attach.
    check(
        report,
        "main window",
        wait_until(Duration::from_secs(120), || {
            Ok(app.get_window("main").is_some())
        })
        .map(|_| "present".to_string()),
    )?;
    // Give the main webview a beat to finish its own first paint.
    std::thread::sleep(Duration::from_secs(2));

    let url = format!("http://127.0.0.1:{GUEST_PORT}/");

    // 1. Tab lifecycle + navigation + visible bounds.
    check(
        report,
        "create + navigate + bounds",
        (|| {
            block_on(preview::preview_create_tab(TAB.to_string()))?;
            block_on(preview::preview_navigate(
                app.clone(),
                TAB.to_string(),
                url.clone(),
            ))?;
            block_on(preview::preview_set_tab_bounds(
                app.clone(),
                TAB.to_string(),
                Some(serde_json::from_value(serde_json::json!({
                    "x": 40.0, "y": 40.0, "width": 640.0, "height": 420.0,
                    "scale": 1.0, "visible": true
                }))
                .map_err(|error| error.to_string())?),
            ))?;
            wait_until(Duration::from_secs(20), || {
                let state = preview::selftest_tab_state(TAB).ok_or("tab state missing")?;
                Ok(state["navStatus"]["kind"] == "Success"
                    && state["navStatus"]["url"]
                        .as_str()
                        .is_some_and(|current| current.contains("127.0.0.1")))
            })?;
            Ok("navigated to Success state".to_string())
        })(),
    )?;

    // 2. Eval round-trip through the injected runtime.
    check(
        report,
        "eval round-trip",
        (|| {
            let title = eval(app, "document.title")?;
            if title == "t3p-one" {
                Ok(format!("document.title = {title}"))
            } else {
                Err(format!("unexpected title: {title}"))
            }
        })(),
    )?;

    // 3. Playwright locator click (exercises the injected InjectedScript).
    check(
        report,
        "click via role locator",
        (|| {
            block_on(preview::preview_automation_click(
                app.clone(),
                TAB.to_string(),
                serde_json::from_value(serde_json::json!({
                    "locator": "role=button[name='Click me']"
                }))
                .map_err(|error| error.to_string())?,
            ))?;
            Ok("dispatched".to_string())
        })(),
    )?;

    // 4. waitFor on the text the click produced.
    check(
        report,
        "waitFor clicked marker",
        (|| {
            block_on(preview::preview_automation_wait_for(
                app.clone(),
                TAB.to_string(),
                serde_json::from_value(serde_json::json!({
                    "text": "clicked-marker", "timeoutMs": 5000
                }))
                .map_err(|error| error.to_string())?,
            ))?;
            let title = eval(app, "document.title")?;
            if title == "clicked-ok" {
                Ok("marker appeared, title updated".to_string())
            } else {
                Err(format!("click handler did not run (title {title})"))
            }
        })(),
    )?;

    // 5. Type into a CSS-selector target, then press a key into it.
    check(
        report,
        "type + press",
        (|| {
            block_on(preview::preview_automation_type(
                app.clone(),
                TAB.to_string(),
                serde_json::from_value(serde_json::json!({
                    "selector": "#inp", "text": "hello tauri"
                }))
                .map_err(|error| error.to_string())?,
            ))?;
            block_on(preview::preview_automation_press(
                app.clone(),
                TAB.to_string(),
                serde_json::from_value(serde_json::json!({ "key": "!" }))
                    .map_err(|error| error.to_string())?,
            ))?;
            let value = eval(app, "document.getElementById('inp').value")?;
            if value == "hello tauri!" {
                Ok(format!("input value = {value}"))
            } else {
                Err(format!("unexpected input value: {value}"))
            }
        })(),
    )?;

    // 6. Scroll the viewport.
    check(
        report,
        "scroll",
        (|| {
            block_on(preview::preview_automation_scroll(
                app.clone(),
                TAB.to_string(),
                serde_json::from_value(serde_json::json!({ "deltaY": 400 }))
                    .map_err(|error| error.to_string())?,
            ))?;
            wait_until(Duration::from_secs(5), || {
                Ok(eval(app, "window.scrollY")?.as_f64().unwrap_or(0.0) > 100.0)
            })?;
            Ok("window.scrollY > 100".to_string())
        })(),
    )?;

    // 7. Snapshot: page collection + capture + console/network buffers.
    check(
        report,
        "automation snapshot",
        (|| {
            let snapshot = block_on(preview::preview_automation_snapshot(
                app.clone(),
                TAB.to_string(),
            ))?;
            let elements = snapshot["interactiveElements"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0);
            let screenshot_bytes = snapshot["screenshot"]["data"]
                .as_str()
                .map(str::len)
                .unwrap_or(0);
            let console_hit = snapshot["consoleEntries"]
                .as_array()
                .is_some_and(|entries| {
                    entries.iter().any(|entry| {
                        entry["text"].as_str().unwrap_or("").contains("hello-from-guest")
                    })
                });
            let network_hit = snapshot["networkEntries"]
                .as_array()
                .is_some_and(|entries| {
                    entries
                        .iter()
                        .any(|entry| entry["url"].as_str().unwrap_or("").contains("/ping"))
                });
            let timeline_len = snapshot["actionTimeline"].as_array().map(Vec::len).unwrap_or(0);
            if elements >= 3 && screenshot_bytes > 1_000 && console_hit && network_hit {
                Ok(format!(
                    "{elements} elements, {screenshot_bytes}b screenshot, console+network captured, {timeline_len} timeline entries"
                ))
            } else {
                Err(format!(
                    "elements={elements} screenshot={screenshot_bytes}b console={console_hit} network={network_hit}"
                ))
            }
        })(),
    )?;

    // 8. Screenshot artifact on disk.
    check(
        report,
        "screenshot artifact",
        (|| {
            let artifact = block_on(preview::preview_capture_screenshot(
                app.clone(),
                TAB.to_string(),
            ))?;
            let path = artifact["path"].as_str().unwrap_or_default().to_string();
            let size = std::fs::metadata(&path)
                .map(|meta| meta.len())
                .map_err(|error| format!("artifact missing at {path}: {error}"))?;
            if size > 1_000 {
                Ok(format!("{path} ({size} bytes)"))
            } else {
                Err(format!("artifact too small: {size} bytes"))
            }
        })(),
    )?;

    // 9. Zoom step reflected in state + page.
    check(
        report,
        "zoom in",
        (|| {
            block_on(preview::preview_zoom_in(app.clone(), TAB.to_string()))?;
            let state = preview::selftest_tab_state(TAB).ok_or("tab state missing")?;
            let zoom = state["zoomFactor"].as_f64().unwrap_or(0.0);
            if (zoom - 1.1).abs() < 0.001 {
                Ok(format!("zoomFactor = {zoom}"))
            } else {
                Err(format!("unexpected zoomFactor {zoom}"))
            }
        })(),
    )?;

    // 10. Color-scheme emulation via NSAppearance.
    check(
        report,
        "dark color scheme",
        (|| {
            block_on(preview::preview_set_color_scheme(
                app.clone(),
                TAB.to_string(),
                "dark".to_string(),
            ))?;
            wait_until(Duration::from_secs(5), || {
                Ok(eval(app, "matchMedia('(prefers-color-scheme: dark)').matches")?
                    == serde_json::Value::Bool(true))
            })?;
            block_on(preview::preview_set_color_scheme(
                app.clone(),
                TAB.to_string(),
                "system".to_string(),
            ))?;
            Ok("prefers-color-scheme: dark emulated".to_string())
        })(),
    )?;

    // 11. History: navigate to /two, then back.
    check(
        report,
        "navigate + goBack",
        (|| {
            block_on(preview::preview_navigate(
                app.clone(),
                TAB.to_string(),
                format!("http://127.0.0.1:{GUEST_PORT}/two"),
            ))?;
            wait_until(Duration::from_secs(10), || {
                Ok(eval(app, "document.title")? == "t3p-two")
            })?;
            block_on(preview::preview_go_back(app.clone(), TAB.to_string()))?;
            wait_until(Duration::from_secs(10), || {
                Ok(eval(app, "location.pathname")? == "/")
            })?;
            // The forward flag is refreshed by the pageshow nav report, which
            // may land slightly after the location changes.
            wait_until(Duration::from_secs(5), || {
                let state = preview::selftest_tab_state(TAB).ok_or("tab state missing")?;
                Ok(state["canGoForward"] == true)
            })
            .map_err(|_| "canGoForward did not become true after goBack".to_string())?;
            Ok("back at /, canGoForward = true".to_string())
        })(),
    )?;

    // 12. Element picker (M3): arm → overlay + theme in the guest DOM →
    // synthetic Escape settles null.
    check(
        report,
        "pick element: arm + escape cancels",
        (|| {
            block_on(preview::preview_set_annotation_theme(
                app.clone(),
                serde_json::json!({
                    "colorScheme": "dark",
                    "radius": "0.5rem",
                    "background": "black",
                    "foreground": "white",
                    "popover": "black",
                    "popoverForeground": "white",
                    "primary": "rgb(1, 2, 3)",
                    "primaryForeground": "white",
                    "muted": "gray",
                    "mutedForeground": "white",
                    "accent": "gray",
                    "accentForeground": "white",
                    "border": "gray",
                    "input": "gray",
                    "ring": "rgb(1, 2, 3)",
                    "fontSans": "sans-serif",
                    "fontMono": "monospace",
                }),
            ))?;
            let app_for_pick = app.clone();
            let pick = std::thread::spawn(move || {
                block_on(preview::preview_pick_element(
                    app_for_pick,
                    TAB.to_string(),
                ))
            });
            wait_until(Duration::from_secs(10), || {
                Ok(eval(
                    app,
                    "!!document.querySelector('div[data-t3code-annotation-ui]')",
                )? == true)
            })
            .map_err(|_| "annotation overlay never appeared".to_string())?;
            let primary = eval(
                app,
                "document.querySelector('div[data-t3code-annotation-ui]').style.getPropertyValue('--t3-primary')",
            )?;
            if primary != "rgb(1, 2, 3)" {
                return Err(format!("annotation theme not applied (--t3-primary = {primary})"));
            }
            eval(
                app,
                "(window.dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape', bubbles: true})), true)",
            )?;
            let outcome = pick
                .join()
                .map_err(|_| "pick thread panicked".to_string())??;
            if !outcome.is_null() {
                return Err(format!("escape should settle null, got {outcome}"));
            }
            wait_until(Duration::from_secs(5), || {
                Ok(eval(
                    app,
                    "!document.querySelector('div[data-t3code-annotation-ui]')",
                )? == true)
            })
            .map_err(|_| "overlay not torn down after escape".to_string())?;
            Ok("overlay shown, themed, escape settled null".to_string())
        })(),
    )?;

    // 13. Element picker submit path: a guest-posted pick settles the pending
    // command with the annotation plus a cropped screenshot.
    check(
        report,
        "pick element: submit + cropped screenshot",
        (|| {
            let app_for_pick = app.clone();
            let pick = std::thread::spawn(move || {
                block_on(preview::preview_pick_element(
                    app_for_pick,
                    TAB.to_string(),
                ))
            });
            wait_until(Duration::from_secs(10), || {
                Ok(eval(
                    app,
                    "!!document.querySelector('div[data-t3code-annotation-ui]')",
                )? == true)
            })?;
            // Bypass the closed-shadow-DOM UI and post the picker's own
            // transport message, exactly as PickPreload's submit handler does.
            eval(
                app,
                r#"(window.__t3pPost({kind: "pick", annotation: {id: "annotation_test", pageUrl: location.href, pageTitle: document.title, comment: "selftest", elements: [], regions: [], strokes: [], styleChanges: [], screenshot: null, createdAt: "2026-01-01T00:00:00.000Z"}, rect: {x: 10, y: 10, width: 200, height: 120}}), true)"#,
            )?;
            let outcome = pick
                .join()
                .map_err(|_| "pick thread panicked".to_string())??;
            let data_url = outcome["screenshot"]["dataUrl"]
                .as_str()
                .ok_or("pick outcome has no screenshot dataUrl")?;
            if !data_url.starts_with("data:image/png;base64,") {
                return Err(format!("unexpected screenshot dataUrl prefix: {:.40}", data_url));
            }
            if outcome["screenshot"]["cropRect"]["width"] != 200.0 {
                return Err(format!(
                    "unexpected cropRect: {}",
                    outcome["screenshot"]["cropRect"]
                ));
            }
            if outcome["comment"] != "selftest" {
                return Err("annotation payload not passed through".to_string());
            }
            Ok(format!(
                "annotation round-tripped, screenshot {} bytes (base64)",
                data_url.len()
            ))
        })(),
    )?;

    // 14. Picture-in-picture (M3): open spawns the always-on-top frame
    // window and flips tab state; close tears both down.
    check(
        report,
        "picture in picture: open + close",
        (|| {
            let pip_window_count = || {
                app.webview_windows()
                    .keys()
                    .filter(|label| label.starts_with("pip-"))
                    .count()
            };
            block_on(preview::preview_pip_open(app.clone(), TAB.to_string()))?;
            let state = preview::selftest_tab_state(TAB).ok_or("tab state missing")?;
            if state["pictureInPicture"] != true {
                return Err(format!("pictureInPicture flag not set: {state}"));
            }
            if pip_window_count() != 1 {
                return Err("PiP window was not created".to_string());
            }
            // Let the shared frame loop deliver a few frames into the window.
            std::thread::sleep(Duration::from_millis(500));
            block_on(preview::preview_pip_close(app.clone(), TAB.to_string()))?;
            let state = preview::selftest_tab_state(TAB).ok_or("tab state missing")?;
            if state["pictureInPicture"] != false {
                return Err(format!("pictureInPicture flag not cleared: {state}"));
            }
            wait_until(Duration::from_secs(5), || Ok(pip_window_count() == 0))
                .map_err(|_| "PiP window was not closed".to_string())?;
            Ok("window opened, frames flowed, window closed".to_string())
        })(),
    )?;

    // 15. Close the tab; the webview must be gone.
    check(
        report,
        "close tab",
        (|| {
            block_on(preview::preview_close_tab(app.clone(), TAB.to_string()))?;
            wait_until(Duration::from_secs(5), || {
                Ok(preview::selftest_tab_state(TAB).is_none())
            })?;
            Ok("tab state removed".to_string())
        })(),
    )?;

    Ok(())
}
