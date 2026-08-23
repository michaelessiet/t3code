//! Preview subsystem: native child webviews implementing DesktopPreviewBridge.
//!
//! The Electron app renders preview tabs as `<webview>` tags in the renderer
//! and drives them over CDP from the main process. WKWebView has no CDP, so
//! this module reproduces the same contract with the patterns the WS5 probe
//! validated (PR #27):
//!
//! - **Child webviews**: `Window::add_child` (tauri `unstable` feature), one
//!   per preview tab, positioned by the renderer's `setTabBounds` calls
//!   (the bounds-sync mode of HostedBrowserWebview in apps/web).
//! - **Eval with results**: `Webview::eval` is fire-and-forget, so every
//!   script the shell runs wraps itself in a `window.__t3pPost({kind:
//!   "result", id, ...})` call that travels back over the `t3preview://`
//!   custom protocol; `run_eval` correlates by id.
//! - **Injected runtime**: built by scripts/build-preview-runtime.mjs —
//!   Playwright's InjectedScript (identical extraction to the Electron app's
//!   PlaywrightInjectedRuntime.ts) plus shim/preview-runtime.src.js (nav +
//!   console + fetch/XHR reporting, human-controller detection).
//! - **Capture**: `takeSnapshotWithConfiguration:` via objc2 dynamic
//!   messaging, re-encoded PNG/JPEG through NSBitmapImageRep.
//!
//! M3 added the element picker (apps/desktop's PickPreload.ts bundled into
//! the runtime) and picture-in-picture (per-tab always-on-top window fed by
//! the shared frame loop). Remaining gaps (documented in README):
//! copy-artifact-to-clipboard, LoadFailed nav status, and network capture is
//! fetch/XHR-only (no subresources — no CDP Network domain equivalent).

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

pub const IPC_SCHEME: &str = "t3preview";

const STATE_EVENT: &str = "t3code://preview-state";
const POINTER_EVENT: &str = "t3code://preview-pointer";
const FRAME_EVENT: &str = "t3code://preview-frame";

// Mirrors the Electron PreviewManager constants (Manager.ts).
const MAX_EVALUATION_BYTES: usize = 64_000;
const MAX_VISIBLE_TEXT_LENGTH: u32 = 20_000;
const MAX_INTERACTIVE_ELEMENTS: u32 = 200;
const MAX_SCREENSHOT_WIDTH: f64 = 1280.0;
const AGENT_CURSOR_MOVE_MS: u64 = 160;
const AGENT_CURSOR_CLICK_LEAD_MS: u64 = 40;
const HUMAN_CONTROLLER_WINDOW_MS: u64 = 750;
const AGENT_CONTROLLER_WINDOW_MS: u64 = 1_500;
const EVAL_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_WAIT_FOR_TIMEOUT_MS: u64 = 15_000;
const DIAGNOSTICS_RING_CAP: usize = 100;
const TIMELINE_CAP: usize = 20;
const RECORDING_FPS: u64 = 12;
const DEFAULT_TAB_WIDTH: f64 = 1024.0;
const DEFAULT_TAB_HEIGHT: f64 = 768.0;
const ZOOM_LEVELS: [f64; 14] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5,
];

pub struct PreviewConfig {
    /// Contents of shim/dist/preview-runtime.js.
    pub runtime_source: String,
    /// Directory for screenshot/recording artifacts.
    pub artifacts_dir: PathBuf,
}

static CONFIG: OnceLock<PreviewConfig> = OnceLock::new();

pub fn init(config: PreviewConfig) {
    let _ = CONFIG.set(config);
}

#[derive(Clone, Deserialize)]
pub struct TabBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub visible: bool,
}

struct Tab {
    label: String,
    surrogate_id: u32,
    webview_created: bool,
    url: String,
    title: String,
    loading: bool,
    pending_url: Option<String>,
    can_go_back: bool,
    can_go_forward: bool,
    zoom: f64,
    color_scheme: String,
    bounds: Option<TabBounds>,
    human_until: Option<Instant>,
    agent_until: Option<Instant>,
    recording: bool,
    /// A picture-in-picture window (label `pip-{surrogate}`) is consuming
    /// frames.
    pip: bool,
    /// Guards against spawning a second frame-capture loop; the loop clears
    /// it under the tabs lock as it exits.
    frame_loop: bool,
    console_entries: VecDeque<Value>,
    network_entries: VecDeque<Value>,
    timeline: VecDeque<Value>,
}

impl Tab {
    fn pip_label(&self) -> String {
        format!("pip-{}", self.surrogate_id)
    }
}

fn tabs() -> &'static Mutex<HashMap<String, Tab>> {
    static TABS: OnceLock<Mutex<HashMap<String, Tab>>> = OnceLock::new();
    TABS.get_or_init(|| Mutex::new(HashMap::new()))
}

type EvalSender = mpsc::Sender<Result<Value, String>>;

fn pending_evals() -> &'static Mutex<HashMap<u64, EvalSender>> {
    static PENDING: OnceLock<Mutex<HashMap<u64, EvalSender>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A settled pick: `None` = cancelled; `Some((annotation, screenshotRect))`
/// mirrors the guest picker's ELEMENT_PICKED_CHANNEL arguments.
type PickOutcome = Option<(Value, Option<Value>)>;
type PickSender = mpsc::Sender<PickOutcome>;

/// One pending pick session per tab, keyed by tab id (Electron's
/// pickSessionsRef equivalent).
fn pending_picks() -> &'static Mutex<HashMap<String, PickSender>> {
    static PENDING: OnceLock<Mutex<HashMap<String, PickSender>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

fn settle_pick(tab_id: &str, outcome: PickOutcome) {
    let sender = pending_picks().lock().unwrap().remove(tab_id);
    if let Some(sender) = sender {
        let _ = sender.send(outcome);
    }
}

/// The renderer-computed annotation theme, broadcast to guests (Electron's
/// annotationThemeRef). Falls back to Manager.ts's DEFAULT_ANNOTATION_THEME
/// until the renderer pushes one.
fn annotation_theme() -> &'static Mutex<Option<Value>> {
    static THEME: OnceLock<Mutex<Option<Value>>> = OnceLock::new();
    THEME.get_or_init(|| Mutex::new(None))
}

fn default_annotation_theme() -> Value {
    json!({
        "colorScheme": "light",
        "radius": "0.625rem",
        "background": "white",
        "foreground": "oklch(0.269 0 0)",
        "popover": "white",
        "popoverForeground": "oklch(0.269 0 0)",
        "primary": "oklch(0.488 0.217 264)",
        "primaryForeground": "white",
        "muted": "rgb(0 0 0 / 4%)",
        "mutedForeground": "oklch(0.556 0 0)",
        "accent": "rgb(0 0 0 / 4%)",
        "accentForeground": "oklch(0.269 0 0)",
        "border": "rgb(0 0 0 / 8%)",
        "input": "rgb(0 0 0 / 10%)",
        "ring": "oklch(0.488 0.217 264)",
        "fontSans": "system-ui, sans-serif",
        "fontMono": "ui-monospace, monospace",
    })
}

/// Fire-and-forget delivery of a GuestProtocol channel message into the
/// guest's picker transport (shim/picker/electron-adapter.ts registers
/// `__t3pPickerDispatch` ahead of the PickPreload bundle).
fn picker_dispatch(app: &AppHandle, tab_id: &str, channel: &str, args: &[Value]) -> Result<(), String> {
    let webview = webview_of(app, tab_id)?;
    let mut call_args = json_string(channel);
    for arg in args {
        call_args.push_str(", ");
        call_args.push_str(&arg.to_string());
    }
    let script =
        format!("window.__t3pPickerDispatch && window.__t3pPickerDispatch({call_args});");
    webview.eval(&script).map_err(|error| error.to_string())
}

static EVAL_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static POINTER_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static TIMELINE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static ARTIFACT_SEQUENCE: AtomicU64 = AtomicU64::new(1);
// Surrogate webContentsId: the web UI only uses it for null-vs-present gating
// ("hasWebContents"), so any stable positive integer per tab works.
static SURROGATE_SEQUENCE: AtomicU32 = AtomicU32::new(10_001);

fn now_iso() -> String {
    chrono::Utc::now()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

// ---------------------------------------------------------------------------
// State snapshots + events

fn tab_state_json(tab_id: &str, tab: &Tab) -> Value {
    let nav_status = if tab.url.is_empty() {
        json!({ "kind": "Idle" })
    } else if tab.loading {
        json!({ "kind": "Loading", "url": tab.url, "title": tab.title })
    } else {
        json!({ "kind": "Success", "url": tab.url, "title": tab.title })
    };
    let now = Instant::now();
    let controller = if tab.human_until.is_some_and(|until| until > now) {
        "human"
    } else if tab.agent_until.is_some_and(|until| until > now) {
        "agent"
    } else {
        "none"
    };
    json!({
        "tabId": tab_id,
        "webContentsId": if tab.webview_created { Value::from(tab.surrogate_id) } else { Value::Null },
        "navStatus": nav_status,
        "canGoBack": tab.can_go_back,
        "canGoForward": tab.can_go_forward,
        "zoomFactor": tab.zoom,
        "pictureInPicture": tab.pip,
        "colorScheme": tab.color_scheme,
        "controller": controller,
        "updatedAt": now_iso(),
    })
}

fn emit_state(app: &AppHandle, tab_id: &str) {
    let state = {
        let tabs = tabs().lock().unwrap();
        tabs.get(tab_id).map(|tab| tab_state_json(tab_id, tab))
    };
    if let Some(state) = state {
        let _ = app.emit(STATE_EVENT, json!({ "tabId": tab_id, "state": state }));
    }
}

fn emit_pointer(app: &AppHandle, tab_id: &str, phase: &str, x: f64, y: f64) {
    let _ = app.emit(
        POINTER_EVENT,
        json!({
            "tabId": tab_id,
            "phase": phase,
            "x": x,
            "y": y,
            "sequence": POINTER_SEQUENCE.fetch_add(1, Ordering::SeqCst),
            "createdAt": now_iso(),
        }),
    );
}

fn mark_agent(app: &AppHandle, tab_id: &str) {
    if let Some(tab) = tabs().lock().unwrap().get_mut(tab_id) {
        tab.agent_until = Some(Instant::now() + Duration::from_millis(AGENT_CONTROLLER_WINDOW_MS));
    }
    emit_state(app, tab_id);
}

// ---------------------------------------------------------------------------
// Action timeline (returned inside automation snapshots)

fn timeline_start(tab_id: &str, action: &str) -> String {
    let id = format!("act-{}", TIMELINE_SEQUENCE.fetch_add(1, Ordering::SeqCst));
    if let Some(tab) = tabs().lock().unwrap().get_mut(tab_id) {
        tab.timeline.push_back(json!({
            "id": id,
            "action": action,
            "status": "running",
            "startedAt": now_iso(),
        }));
        while tab.timeline.len() > TIMELINE_CAP {
            tab.timeline.pop_front();
        }
    }
    id
}

fn timeline_finish(tab_id: &str, id: &str, error: Option<&str>) {
    if let Some(tab) = tabs().lock().unwrap().get_mut(tab_id) {
        for entry in tab.timeline.iter_mut() {
            if entry["id"] == id {
                entry["status"] = Value::from(if error.is_some() { "failed" } else { "succeeded" });
                entry["completedAt"] = Value::from(now_iso());
                if let Some(error) = error {
                    let mut truncated = error.to_string();
                    truncated.truncate(500);
                    entry["error"] = Value::from(truncated);
                }
            }
        }
    }
}

/// Wraps an automation operation with timeline bookkeeping + agent marking.
fn with_timeline<T>(
    app: &AppHandle,
    tab_id: &str,
    action: &str,
    run: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    mark_agent(app, tab_id);
    let entry = timeline_start(tab_id, action);
    let result = run();
    timeline_finish(tab_id, &entry, result.as_ref().err().map(String::as_str));
    result
}

// ---------------------------------------------------------------------------
// Main-thread + eval plumbing

/// Runs a closure on the main thread and waits for its result. Executes
/// inline when already on the main thread (tauri's run_on_main_thread
/// short-circuits), so this is safe from any thread except when the main
/// thread is blocked elsewhere.
fn on_main<T: Send + 'static>(
    app: &AppHandle,
    run: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(run());
    })
    .map_err(|error| error.to_string())?;
    rx.recv_timeout(Duration::from_secs(10))
        .map_err(|_| "main-thread call timed out".to_string())
}

fn webview_label(tab_id: &str) -> Result<String, String> {
    tabs()
        .lock()
        .unwrap()
        .get(tab_id)
        .map(|tab| tab.label.clone())
        .ok_or_else(|| format!("Unknown preview tab: {tab_id}"))
}

fn webview_of(app: &AppHandle, tab_id: &str) -> Result<tauri::webview::Webview, String> {
    let label = webview_label(tab_id)?;
    app.get_webview(&label)
        .ok_or_else(|| format!("Preview webview for tab {tab_id} does not exist yet"))
}

/// Read-only tab state snapshot for the preview self-test.
pub(crate) fn selftest_tab_state(tab_id: &str) -> Option<Value> {
    tabs()
        .lock()
        .unwrap()
        .get(tab_id)
        .map(|tab| tab_state_json(tab_id, tab))
}

/// Evaluates a JS expression in the guest page and waits for the result the
/// injected runtime posts back over `t3preview://`. `body` must be an
/// expression (it is wrapped in parentheses / `await (...)`).
pub(crate) fn run_eval(
    app: &AppHandle,
    tab_id: &str,
    body: &str,
    await_promise: bool,
    timeout: Duration,
) -> Result<Value, String> {
    let webview = webview_of(app, tab_id)?;
    let id = EVAL_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = mpsc::channel();
    pending_evals().lock().unwrap().insert(id, tx);

    let awaited = if await_promise {
        format!("await ({body})")
    } else {
        format!("({body})")
    };
    let script = r##"(async () => {
  try {
    const __r = __AWAITED__;
    window.__t3pPost({ kind: "result", id: __ID__, ok: true, value: (typeof __r === "undefined" ? null : __r) });
  } catch (error) {
    window.__t3pPost({ kind: "result", id: __ID__, ok: false, error: String(error && (error.stack || error.message) || error) });
  }
})();"##
        .replace("__ID__", &id.to_string())
        .replace("__AWAITED__", &awaited);

    let eval_result = webview.eval(&script);
    let outcome = match eval_result {
        Ok(()) => rx.recv_timeout(timeout).map_err(|_| {
            "Evaluation timed out: the page returned no result (it may block script \
             evaluation, still be loading, or the expression may not parse)"
                .to_string()
        }),
        Err(error) => Ok(Err(error.to_string())),
    };
    pending_evals().lock().unwrap().remove(&id);
    outcome?
}

// ---------------------------------------------------------------------------
// t3preview:// scheme handler — the guest → shell message channel

pub fn ipc_handler(
    ctx: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let app = ctx.app_handle().clone();
    if let Ok(message) = serde_json::from_slice::<Value>(request.body()) {
        handle_guest_message(&app, message);
    }
    tauri::http::Response::builder()
        .status(200)
        // Guest pages live on arbitrary dev-server origins.
        .header("Access-Control-Allow-Origin", "*")
        .header("Content-Type", "text/plain")
        .body(b"ok".to_vec())
        .expect("static response builds")
}

fn handle_guest_message(app: &AppHandle, message: Value) {
    let kind = message["kind"].as_str().unwrap_or_default();
    if kind == "result" {
        let Some(id) = message["id"].as_u64() else {
            return;
        };
        let sender = pending_evals().lock().unwrap().remove(&id);
        if let Some(sender) = sender {
            let outcome = if message["ok"].as_bool() == Some(true) {
                Ok(message["value"].clone())
            } else {
                Err(message["error"].as_str().unwrap_or("evaluation failed").to_string())
            };
            let _ = sender.send(outcome);
        }
        return;
    }

    let Some(tab_id) = message["tabId"].as_str().map(String::from) else {
        return;
    };
    match kind {
        "nav" | "title" => {
            let mut nav_finished = false;
            {
                let mut tabs = tabs().lock().unwrap();
                let Some(tab) = tabs.get_mut(&tab_id) else {
                    return;
                };
                if let Some(title) = message["title"].as_str() {
                    tab.title = title.to_string();
                }
                if kind == "nav" {
                    if let Some(url) = message["url"].as_str() {
                        tab.url = url.to_string();
                    }
                    if let Some(loading) = message["loading"].as_bool() {
                        tab.loading = loading;
                        nav_finished = !loading;
                    }
                }
            }
            // A `loading: true` nav means a NEW document's runtime came up
            // (DOMContentLoaded) — any picker overlay died with the old
            // document. Mirrors Electron settling picks on
            // did-start-navigation. SPA pushState/popstate report
            // loading: false and keep the session alive.
            if kind == "nav" && message["loading"].as_bool() == Some(true) {
                settle_pick(&tab_id, None);
            }
            if nav_finished {
                refresh_nav_flags(app, &tab_id);
            }
            emit_state(app, &tab_id);
        }
        "pick" => {
            let annotation = message["annotation"].clone();
            if annotation.is_null() {
                settle_pick(&tab_id, None);
            } else {
                let rect = match message["rect"].clone() {
                    Value::Null => None,
                    rect => Some(rect),
                };
                settle_pick(&tab_id, Some((annotation, rect)));
            }
        }
        "human" => {
            {
                let mut tabs = tabs().lock().unwrap();
                let Some(tab) = tabs.get_mut(&tab_id) else {
                    return;
                };
                tab.human_until =
                    Some(Instant::now() + Duration::from_millis(HUMAN_CONTROLLER_WINDOW_MS));
            }
            emit_state(app, &tab_id);
        }
        "console" => {
            let entry = json!({
                "level": message["level"].as_str().unwrap_or("log"),
                "text": message["text"].as_str().unwrap_or_default(),
                "timestamp": now_iso(),
            });
            push_diagnostic(&tab_id, entry, true);
        }
        "net" => {
            let entry = json!({
                "url": message["url"].as_str().unwrap_or_default(),
                "method": message["method"].as_str().unwrap_or("GET"),
                "status": message["status"].clone(),
                "failed": message["failed"].as_bool().unwrap_or(false),
                "errorText": message["errorText"].clone(),
                "timestamp": now_iso(),
            });
            push_diagnostic(&tab_id, entry, false);
        }
        _ => {}
    }
}

fn push_diagnostic(tab_id: &str, entry: Value, console: bool) {
    let mut tabs = tabs().lock().unwrap();
    let Some(tab) = tabs.get_mut(tab_id) else {
        return;
    };
    let ring = if console {
        &mut tab.console_entries
    } else {
        &mut tab.network_entries
    };
    ring.push_back(entry);
    while ring.len() > DIAGNOSTICS_RING_CAP {
        ring.pop_front();
    }
}

/// Re-reads canGoBack/canGoForward from the platform webview (fire-and-forget
/// onto the main thread) and re-emits state when they changed.
fn refresh_nav_flags(app: &AppHandle, tab_id: &str) {
    let Ok(webview) = webview_of(app, tab_id) else {
        return;
    };
    let app = app.clone();
    let tab_id = tab_id.to_string();
    let _ = app.clone().run_on_main_thread(move || {
        let Some((back, forward)) = platform::nav_flags(&webview) else {
            return;
        };
        let changed = {
            let mut tabs = tabs().lock().unwrap();
            let Some(tab) = tabs.get_mut(&tab_id) else {
                return;
            };
            let changed = tab.can_go_back != back || tab.can_go_forward != forward;
            tab.can_go_back = back;
            tab.can_go_forward = forward;
            changed
        };
        if changed {
            emit_state(&app, &tab_id);
        }
    });
}

// ---------------------------------------------------------------------------
// Webview lifecycle

fn sanitize_label(tab_id: &str, surrogate: u32) -> String {
    let cleaned: String = tab_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(40)
        .collect();
    format!("preview-{surrogate}-{cleaned}")
}

/// Creates the child webview for a tab if it doesn't exist yet. Runs the
/// creation on the main thread; safe to call repeatedly.
fn ensure_webview(app: &AppHandle, tab_id: &str) -> Result<tauri::webview::Webview, String> {
    if let Ok(webview) = webview_of(app, tab_id) {
        return Ok(webview);
    }
    let config = CONFIG.get().ok_or("preview subsystem is not configured")?;
    let (label, bounds, initial_url) = {
        let mut tabs = tabs().lock().unwrap();
        let tab = tabs
            .get_mut(tab_id)
            .ok_or_else(|| format!("Unknown preview tab: {tab_id}"))?;
        (
            tab.label.clone(),
            tab.bounds.clone(),
            tab.pending_url.take().unwrap_or_else(|| "about:blank".to_string()),
        )
    };
    let script = format!(
        "window.__T3P_TAB_ID = {};\n{}",
        json_string(tab_id),
        config.runtime_source
    );

    let app_for_create = app.clone();
    let tab_for_create = tab_id.to_string();
    on_main(app, move || -> Result<(), String> {
        let window = app_for_create
            .get_window("main")
            .ok_or("main window is not ready yet")?;
        let url: tauri::Url = initial_url
            .parse()
            .map_err(|error| format!("invalid preview URL {initial_url}: {error}"))?;
        let app_for_load = app_for_create.clone();
        let tab_for_load = tab_for_create.clone();
        let builder =
            tauri::webview::WebviewBuilder::new(&label, tauri::WebviewUrl::External(url))
                .initialization_script(&script)
                .on_page_load(move |webview, payload| {
                    handle_page_load(&app_for_load, &tab_for_load, &webview, payload);
                });
        let (position, size, visible) = match &bounds {
            Some(bounds) => (
                (bounds.x, bounds.y),
                (bounds.width.max(1.0), bounds.height.max(1.0)),
                bounds.visible,
            ),
            None => ((0.0, 0.0), (DEFAULT_TAB_WIDTH, DEFAULT_TAB_HEIGHT), false),
        };
        let webview = window
            .add_child(
                builder,
                tauri::LogicalPosition::new(position.0, position.1),
                tauri::LogicalSize::new(size.0, size.1),
            )
            .map_err(|error| format!("failed to create preview webview: {error}"))?;
        if !visible {
            let _ = webview.hide();
        }
        Ok(())
    })??;

    {
        let mut tabs = tabs().lock().unwrap();
        if let Some(tab) = tabs.get_mut(tab_id) {
            tab.webview_created = true;
        }
    }
    apply_zoom(app, tab_id);
    apply_color_scheme(app, tab_id);
    emit_state(app, tab_id);
    webview_of(app, tab_id)
}

fn handle_page_load(
    app: &AppHandle,
    tab_id: &str,
    _webview: &tauri::webview::Webview,
    payload: tauri::webview::PageLoadPayload<'_>,
) {
    let url = payload.url().to_string();
    let started = matches!(payload.event(), tauri::webview::PageLoadEvent::Started);
    {
        let mut tabs = tabs().lock().unwrap();
        let Some(tab) = tabs.get_mut(tab_id) else {
            return;
        };
        if url != "about:blank" {
            tab.url = url;
        }
        tab.loading = started;
    }
    if !started {
        refresh_nav_flags(app, tab_id);
        // pageZoom persists across navigations on WKWebView, but re-assert to
        // stay robust against process swaps.
        apply_zoom(app, tab_id);
    }
    emit_state(app, tab_id);
}

fn apply_zoom(app: &AppHandle, tab_id: &str) {
    let zoom = {
        let tabs = tabs().lock().unwrap();
        let Some(tab) = tabs.get(tab_id) else { return };
        tab.zoom * tab.bounds.as_ref().map_or(1.0, |bounds| bounds.scale)
    };
    let Ok(webview) = webview_of(app, tab_id) else {
        return;
    };
    let _ = on_main(app, move || platform::set_page_zoom(&webview, zoom));
}

fn apply_color_scheme(app: &AppHandle, tab_id: &str) {
    let scheme = {
        let tabs = tabs().lock().unwrap();
        let Some(tab) = tabs.get(tab_id) else { return };
        tab.color_scheme.clone()
    };
    let Ok(webview) = webview_of(app, tab_id) else {
        return;
    };
    let _ = on_main(app, move || platform::set_color_scheme(&webview, &scheme));
}

// ---------------------------------------------------------------------------
// In-page automation scripts (ports of the Electron Manager.ts scripts, which
// are already plain page JS — CDP was only the transport)

const SNAPSHOT_PAGE_SCRIPT: &str = r##"(() => {
  const selectorFor = (element) => {
    if (element.id) return "#" + CSS.escape(element.id);
    for (const attribute of ["data-testid", "name"]) {
      const value = element.getAttribute(attribute);
      if (value) return element.tagName.toLowerCase() + "[" + attribute + "=" + JSON.stringify(value) + "]";
    }
    const buildParts = (current, parts = []) => {
      if (!current || current.nodeType !== Node.ELEMENT_NODE || parts.length >= 8) {
        return parts;
      }
      const parent = current.parentElement;
      const siblings = parent
        ? Array.from(parent.children).filter((child) => child.tagName === current.tagName)
        : [];
      const base = current.tagName.toLowerCase();
      const part = siblings.length > 1
        ? base + ":nth-of-type(" + (siblings.indexOf(current) + 1) + ")"
        : base;
      return buildParts(parent, [part, ...parts]);
    };
    return buildParts(element).join(" > ");
  };
  const visible = (element) => {
    const style = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== "hidden" && style.display !== "none" && rect.width > 0 && rect.height > 0;
  };
  const elements = Array.from(document.querySelectorAll(
    "a[href],button,input,textarea,select,[role],[tabindex]"
  )).filter(visible).slice(0, __MAX_ELEMENTS__).map((element) => {
    const rect = element.getBoundingClientRect();
    return {
      tag: element.tagName.toLowerCase(),
      role: element.getAttribute("role"),
      name: element.getAttribute("aria-label") || element.innerText || element.getAttribute("name") || "",
      selector: selectorFor(element),
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height
    };
  });
  return {
    url: location.href,
    title: document.title,
    loading: document.readyState !== "complete",
    visibleText: (document.body?.innerText || "").slice(0, __MAX_TEXT__),
    interactiveElements: elements
  };
})()"##;

const RESOLVE_CLICK_POINT_SCRIPT: &str = r##"(() => {
  try {
    const injected = globalThis.__t3PlaywrightInjected;
    const parsed = injected.parseSelector(__LOCATOR_JSON__);
    const element = injected.querySelector(parsed, document, true);
    if (!element) return { notFound: true };
    const visible = injected.elementState(element, "visible");
    const enabled = injected.elementState(element, "enabled");
    if (!visible.matches || !enabled.matches) return { notFound: true };
    element.scrollIntoView({ block: "center", inline: "center" });
    const rect = element.getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  } catch (error) {
    return { invalidSelector: true, message: String(error) };
  }
})()"##;

// Replaces CDP Input.dispatchMouseEvent: synthesize the full pointer/mouse
// sequence at the resolved point. isTrusted is false, which application JS
// almost never checks; native behaviors (text caret placement) are
// approximated by the explicit focus() call.
const CLICK_DISPATCH_SCRIPT: &str = r##"(() => {
  const x = __X__;
  const y = __Y__;
  const target = document.elementFromPoint(x, y) || document.body;
  const base = { bubbles: true, cancelable: true, composed: true, clientX: x, clientY: y, view: window, button: 0 };
  const pointer = { ...base, pointerId: 1, pointerType: "mouse", isPrimary: true };
  target.dispatchEvent(new PointerEvent("pointerover", pointer));
  target.dispatchEvent(new MouseEvent("mouseover", base));
  target.dispatchEvent(new PointerEvent("pointermove", pointer));
  target.dispatchEvent(new MouseEvent("mousemove", base));
  target.dispatchEvent(new PointerEvent("pointerdown", { ...pointer, buttons: 1, pressure: 0.5 }));
  target.dispatchEvent(new MouseEvent("mousedown", { ...base, buttons: 1, detail: 1 }));
  const focusable = target.closest("a,button,input,textarea,select,[tabindex],[contenteditable]");
  if (focusable instanceof HTMLElement) focusable.focus();
  target.dispatchEvent(new PointerEvent("pointerup", { ...pointer, buttons: 0, pressure: 0 }));
  target.dispatchEvent(new MouseEvent("mouseup", { ...base, buttons: 0, detail: 1 }));
  target.dispatchEvent(new MouseEvent("click", { ...base, buttons: 0, detail: 1 }));
  return { clicked: true, tag: target.tagName.toLowerCase() };
})()"##;

const TYPE_SCRIPT: &str = r##"(() => {
  try {
    const element = __ELEMENT_EXPR__;
    if (!element) return { notFound: true };
    const textControl =
      element instanceof HTMLTextAreaElement ||
      (element instanceof HTMLInputElement &&
        !new Set(["button", "checkbox", "color", "file", "hidden", "image", "radio", "range", "reset", "submit"]).has(element.type));
    const editable = textControl || element.isContentEditable;
    if (!editable || element.disabled || element.readOnly) return { notEditable: true };
    element.focus();
    if (document.activeElement !== element) return { notEditable: true };
    const clear = __CLEAR__;
    if (clear) {
      if (textControl) {
        element.select();
      } else {
        const range = document.createRange();
        range.selectNodeContents(element);
        const selection = document.getSelection();
        selection?.removeAllRanges();
        selection?.addRange(range);
      }
    }
    const text = __TEXT_JSON__;
    let inserted = true;
    if (text.length > 0) {
      inserted = document.execCommand("insertText", false, text);
    } else if (clear) {
      document.execCommand("delete", false);
      const cleared = textControl
        ? element.value.length === 0
        : (element.textContent ?? "").length === 0;
      if (!cleared) {
        if (textControl) {
          const prototype = element instanceof HTMLTextAreaElement
            ? HTMLTextAreaElement.prototype
            : HTMLInputElement.prototype;
          const valueSetter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
          if (valueSetter) valueSetter.call(element, "");
          else element.value = "";
        } else {
          element.replaceChildren();
        }
        element.dispatchEvent(new InputEvent("input", {
          bubbles: true,
          inputType: "deleteContentBackward",
        }));
      }
    }
    if (!inserted) return { notEditable: true };
    element.dispatchEvent(new Event("change", { bubbles: true }));
    return { ok: true };
  } catch (error) {
    return { invalidSelector: true, message: String(error) };
  }
})()"##;

const SCROLL_SCRIPT: &str = r##"(() => {
  try {
    const target = __TARGET_EXPR__;
    if (!target) return { notFound: true };
    target.scrollBy({ left: __DX__, top: __DY__, behavior: "instant" });
    return { ok: true };
  } catch (error) {
    return { invalidSelector: true, message: String(error) };
  }
})()"##;

const WAIT_FOR_SCRIPT: &str = r##"(() => {
  try {
    const selectorMatched = __SELECTOR_EXPR__;
    const textMatched = __TEXT_EXPR__;
    const urlMatched = __URL_EXPR__;
    return { matched: selectorMatched && textMatched && urlMatched };
  } catch (error) {
    return { invalidSelector: true, message: String(error) };
  }
})()"##;

// Replaces CDP Input.dispatchKeyEvent: dispatch keydown/keypress/keyup to the
// focused element, inserting text for printable keys and approximating
// Enter-submits-form.
const PRESS_SCRIPT: &str = r##"(() => {
  const key = __KEY_JSON__;
  const modifiers = __MODS_JSON__;
  const target = (document.activeElement && document.activeElement !== document.body)
    ? document.activeElement
    : (document.body || document.documentElement);
  const code = (() => {
    if (key.length === 1) {
      if (/[a-z]/i.test(key)) return "Key" + key.toUpperCase();
      if (/[0-9]/.test(key)) return "Digit" + key;
      if (key === " ") return "Space";
    }
    return key;
  })();
  const options = {
    key,
    code,
    bubbles: true,
    cancelable: true,
    composed: true,
    altKey: modifiers.includes("Alt"),
    ctrlKey: modifiers.includes("Control"),
    metaKey: modifiers.includes("Meta"),
    shiftKey: modifiers.includes("Shift"),
  };
  const plain = !options.altKey && !options.ctrlKey && !options.metaKey;
  const downOk = target.dispatchEvent(new KeyboardEvent("keydown", options));
  if (downOk && (key.length === 1 || key === "Enter")) {
    target.dispatchEvent(new KeyboardEvent("keypress", options));
  }
  if (downOk && plain && key.length === 1) {
    document.execCommand("insertText", false, key);
  }
  if (downOk && plain && key === "Enter") {
    if (target instanceof HTMLInputElement && target.form) {
      target.form.requestSubmit();
    } else if (target instanceof HTMLTextAreaElement || target.isContentEditable) {
      document.execCommand("insertText", false, "\n");
    }
  }
  target.dispatchEvent(new KeyboardEvent("keyup", options));
  return { ok: true };
})()"##;

fn playwright_element_expr(locator_json: &str) -> String {
    format!(
        "(() => {{ const injected = globalThis.__t3PlaywrightInjected; return injected.querySelector(injected.parseSelector({locator_json}), document, true); }})()"
    )
}

fn automation_locator(selector: Option<&str>, locator: Option<&str>) -> Option<String> {
    locator
        .map(String::from)
        .or_else(|| selector.map(|selector| format!("css={selector}")))
}

fn check_automation_result(operation: &str, result: &Value) -> Result<(), String> {
    if result["invalidSelector"].as_bool() == Some(true) {
        return Err(format!(
            "{operation}: invalid selector — {}",
            result["message"].as_str().unwrap_or("unknown parse error")
        ));
    }
    if result["notFound"].as_bool() == Some(true) {
        return Err(format!("{operation}: no matching element found"));
    }
    if result["notEditable"].as_bool() == Some(true) {
        return Err(format!("{operation}: target element is not editable"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Automation input shapes (subset of the contracts schemas; tabId and unknown
// fields are ignored)

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClickInput {
    pub selector: Option<String>,
    pub locator: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub timeout_ms: Option<u64>,
    #[serde(rename = "tabId")]
    pub _tab_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeInput {
    pub text: String,
    pub selector: Option<String>,
    pub locator: Option<String>,
    pub clear: Option<bool>,
    pub timeout_ms: Option<u64>,
    #[serde(rename = "tabId")]
    pub _tab_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PressInput {
    pub key: String,
    pub modifiers: Option<Vec<String>>,
    #[serde(rename = "tabId")]
    pub _tab_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrollInput {
    pub delta_x: Option<f64>,
    pub delta_y: Option<f64>,
    pub selector: Option<String>,
    pub locator: Option<String>,
    #[serde(rename = "tabId")]
    pub _tab_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateInput {
    pub expression: String,
    pub await_promise: Option<bool>,
    // returnByValue is accepted but meaningless here: without CDP there are no
    // remote object references — results are always serialized by value.
    #[serde(rename = "returnByValue")]
    pub _return_by_value: Option<bool>,
    #[serde(rename = "tabId")]
    pub _tab_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitForInput {
    pub selector: Option<String>,
    pub locator: Option<String>,
    pub text: Option<String>,
    pub url_includes: Option<String>,
    pub timeout_ms: Option<u64>,
    #[serde(rename = "tabId")]
    pub _tab_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Commands: tab lifecycle + placement

#[tauri::command]
pub async fn preview_create_tab(tab_id: String) -> Result<(), String> {
    let surrogate = SURROGATE_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    let mut tabs = tabs().lock().unwrap();
    tabs.entry(tab_id.clone()).or_insert_with(|| Tab {
        label: sanitize_label(&tab_id, surrogate),
        surrogate_id: surrogate,
        webview_created: false,
        url: String::new(),
        title: String::new(),
        loading: false,
        pending_url: None,
        can_go_back: false,
        can_go_forward: false,
        zoom: 1.0,
        color_scheme: "system".to_string(),
        bounds: None,
        human_until: None,
        agent_until: None,
        recording: false,
        pip: false,
        frame_loop: false,
        console_entries: VecDeque::new(),
        network_entries: VecDeque::new(),
        timeline: VecDeque::new(),
    });
    Ok(())
}

#[tauri::command]
pub async fn preview_close_tab(app: AppHandle, tab_id: String) -> Result<(), String> {
    settle_pick(&tab_id, None);
    close_pip_window(&app, &tab_id);
    let webview = webview_of(&app, &tab_id).ok();
    {
        let mut tabs = tabs().lock().unwrap();
        tabs.remove(&tab_id);
    }
    if let Some(webview) = webview {
        let _ = on_main(&app, move || webview.close());
    }
    Ok(())
}

/// Electron-only handshake; the Tauri shell owns its webviews natively.
#[tauri::command]
pub async fn preview_register_webview(_tab_id: String, _web_contents_id: i64) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn preview_set_tab_bounds(
    app: AppHandle,
    tab_id: String,
    bounds: Option<TabBounds>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        {
            let mut tabs = tabs().lock().unwrap();
            let tab = tabs
                .get_mut(&tab_id)
                .ok_or_else(|| format!("Unknown preview tab: {tab_id}"))?;
            tab.bounds = bounds.clone();
        }
        match bounds {
            None => {
                if let Ok(webview) = webview_of(&app, &tab_id) {
                    let _ = on_main(&app, move || webview.hide());
                }
                Ok(())
            }
            Some(bounds) => {
                let webview = ensure_webview(&app, &tab_id)?;
                on_main(&app, move || -> Result<(), String> {
                    webview
                        .set_position(tauri::LogicalPosition::new(bounds.x, bounds.y))
                        .map_err(|error| error.to_string())?;
                    webview
                        .set_size(tauri::LogicalSize::new(
                            bounds.width.max(1.0),
                            bounds.height.max(1.0),
                        ))
                        .map_err(|error| error.to_string())?;
                    if bounds.visible {
                        webview.show().map_err(|error| error.to_string())?;
                    } else {
                        webview.hide().map_err(|error| error.to_string())?;
                    }
                    Ok(())
                })??;
                apply_zoom(&app, &tab_id);
                Ok(())
            }
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

// ---------------------------------------------------------------------------
// Commands: navigation + appearance

#[tauri::command]
pub async fn preview_navigate(app: AppHandle, tab_id: String, url: String) -> Result<(), String> {
    let parsed: tauri::Url = url
        .parse()
        .map_err(|error| format!("invalid URL {url}: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https" | "about") {
        return Err(format!("refusing to navigate to scheme {}", parsed.scheme()));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let exists = webview_of(&app, &tab_id).is_ok();
        if exists {
            let webview = webview_of(&app, &tab_id)?;
            on_main(&app, move || {
                webview.navigate(parsed).map_err(|error| error.to_string())
            })??;
        } else {
            {
                let mut tabs = tabs().lock().unwrap();
                let tab = tabs
                    .get_mut(&tab_id)
                    .ok_or_else(|| format!("Unknown preview tab: {tab_id}"))?;
                tab.pending_url = Some(url.clone());
            }
            ensure_webview(&app, &tab_id)?;
        }
        {
            let mut tabs = tabs().lock().unwrap();
            if let Some(tab) = tabs.get_mut(&tab_id) {
                tab.url = url;
                tab.loading = true;
            }
        }
        emit_state(&app, &tab_id);
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

fn nav_action(app: AppHandle, tab_id: String, action: &'static str) -> Result<(), String> {
    let webview = webview_of(&app, &tab_id)?;
    on_main(&app, move || platform::nav_action(&webview, action))??;
    refresh_nav_flags(&app, &tab_id);
    emit_state(&app, &tab_id);
    Ok(())
}

#[tauri::command]
pub async fn preview_go_back(app: AppHandle, tab_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || nav_action(app, tab_id, "back"))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_go_forward(app: AppHandle, tab_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || nav_action(app, tab_id, "forward"))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_refresh(app: AppHandle, tab_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || nav_action(app, tab_id, "reload"))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_hard_reload(app: AppHandle, tab_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || nav_action(app, tab_id, "reload-from-origin"))
        .await
        .map_err(|error| error.to_string())?
}

fn step_zoom(app: AppHandle, tab_id: String, direction: i32) -> Result<(), String> {
    {
        let mut tabs = tabs().lock().unwrap();
        let tab = tabs
            .get_mut(&tab_id)
            .ok_or_else(|| format!("Unknown preview tab: {tab_id}"))?;
        let current = ZOOM_LEVELS
            .iter()
            .position(|level| (level - tab.zoom).abs() < 0.001)
            .unwrap_or(7);
        let next = match direction {
            0 => 7, // reset → 1.0
            d if d > 0 => (current + 1).min(ZOOM_LEVELS.len() - 1),
            _ => current.saturating_sub(1),
        };
        tab.zoom = ZOOM_LEVELS[next];
    }
    apply_zoom(&app, &tab_id);
    emit_state(&app, &tab_id);
    Ok(())
}

#[tauri::command]
pub async fn preview_zoom_in(app: AppHandle, tab_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || step_zoom(app, tab_id, 1))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_zoom_out(app: AppHandle, tab_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || step_zoom(app, tab_id, -1))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_reset_zoom(app: AppHandle, tab_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || step_zoom(app, tab_id, 0))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_set_color_scheme(
    app: AppHandle,
    tab_id: String,
    color_scheme: String,
) -> Result<(), String> {
    if !matches!(color_scheme.as_str(), "system" | "light" | "dark") {
        return Err(format!("invalid color scheme: {color_scheme}"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        {
            let mut tabs = tabs().lock().unwrap();
            let tab = tabs
                .get_mut(&tab_id)
                .ok_or_else(|| format!("Unknown preview tab: {tab_id}"))?;
            tab.color_scheme = color_scheme;
        }
        apply_color_scheme(&app, &tab_id);
        emit_state(&app, &tab_id);
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_open_devtools(app: AppHandle, tab_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let webview = webview_of(&app, &tab_id)?;
        on_main(&app, move || {
            webview.open_devtools();
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_clear_cookies(app: AppHandle) -> Result<(), String> {
    // Cookies only: the preview webviews share the default WKWebsiteDataStore
    // with the app webview, and the app's auth lives in localStorage — which
    // this deliberately does not touch.
    clear_website_data(app, &["WKWebsiteDataTypeCookies"]).await
}

#[tauri::command]
pub async fn preview_clear_cache(app: AppHandle) -> Result<(), String> {
    clear_website_data(
        app,
        &["WKWebsiteDataTypeDiskCache", "WKWebsiteDataTypeMemoryCache"],
    )
    .await
}

async fn clear_website_data(app: AppHandle, types: &'static [&'static str]) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        let dispatch = app.run_on_main_thread(move || {
            platform::clear_website_data(types, tx);
        });
        dispatch.map_err(|error| error.to_string())?;
        rx.recv_timeout(Duration::from_secs(10))
            .map_err(|_| "clearing website data timed out".to_string())?
    })
    .await
    .map_err(|error| error.to_string())?
}

// ---------------------------------------------------------------------------
// Commands: config + artifacts

#[tauri::command]
pub async fn preview_get_config(_environment_id: String) -> Result<Value, String> {
    // Only consumed by the Electron `<webview>` mount path; the bounds-sync
    // host never mounts one. preloadUrl null also tells the renderer to
    // disable element-pick affordances (not supported in M2).
    Ok(json!({
        "partition": "persist:t3code-preview",
        "webPreferences": "",
        "preloadUrl": Value::Null,
    }))
}

#[tauri::command]
pub async fn preview_set_annotation_theme(app: AppHandle, theme: Value) -> Result<(), String> {
    // Mirror of Manager.ts setAnnotationTheme: store for future picks and
    // broadcast to every live guest so an active overlay restyles live.
    *annotation_theme().lock().unwrap() = Some(theme.clone());
    let tab_ids: Vec<String> = tabs().lock().unwrap().keys().cloned().collect();
    for tab_id in tab_ids {
        let _ = picker_dispatch(&app, &tab_id, "preview:annotation-theme", &[theme.clone()]);
    }
    Ok(())
}

/// Mirror of Manager.ts normalizeCaptureRect: clamp to non-negative integers,
/// reject degenerate rects. Input/output are CSS px.
fn normalize_capture_rect(rect: &Value) -> Option<(f64, f64, f64, f64)> {
    let read = |name: &str| rect.get(name).and_then(Value::as_f64).filter(|v| v.is_finite());
    let (x, y, width, height) = (read("x")?, read("y")?, read("width")?, read("height")?);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some((
        x.max(0.0).floor(),
        y.max(0.0).floor(),
        width.ceil().max(1.0),
        height.ceil().max(1.0),
    ))
}

/// Captures the annotation screenshot (cropped when the guest sent a union
/// rect). Failures degrade to a null screenshot — the structured annotation
/// is still worth returning.
fn capture_annotation_screenshot(app: &AppHandle, tab_id: &str, rect: Option<&Value>) -> Value {
    let crop = rect.and_then(normalize_capture_rect);
    // The guest reports CSS px; WKSnapshotConfiguration.rect is in view
    // points, which differ by the applied pageZoom (tab zoom × panel scale).
    let effective_zoom = {
        let tabs = tabs().lock().unwrap();
        tabs.get(tab_id)
            .map(|tab| tab.zoom * tab.bounds.as_ref().map(|bounds| bounds.scale).unwrap_or(1.0))
            .unwrap_or(1.0)
    };
    let view_rect =
        crop.map(|(x, y, w, h)| (x * effective_zoom, y * effective_zoom, w * effective_zoom, h * effective_zoom));
    match capture_image(app, tab_id, None, view_rect, platform::ImageFormat::Png) {
        Ok((bytes, width, height)) => {
            let crop_rect = match crop {
                Some((x, y, w, h)) => json!({"x": x, "y": y, "width": w, "height": h}),
                None => json!({"x": 0, "y": 0, "width": width, "height": height}),
            };
            json!({
                "dataUrl": format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(&bytes)
                ),
                "width": width,
                "height": height,
                "cropRect": crop_rect,
            })
        }
        Err(error) => {
            eprintln!("[desktop-tauri] annotation screenshot failed: {error}");
            Value::Null
        }
    }
}

fn cancel_pick(app: &AppHandle, tab_id: &str) {
    let existing = pending_picks().lock().unwrap().remove(tab_id);
    if let Some(sender) = existing {
        let _ = picker_dispatch(app, tab_id, "preview:cancel-pick", &[]);
        let _ = sender.send(None);
    }
}

/// Arms the guest picker and blocks until the annotation studio settles:
/// submitted payload, Escape/cancel (null), navigation (null), or tab close
/// (null). No timeout, matching Electron — a pick can stay open for minutes.
#[tauri::command]
pub async fn preview_pick_element(app: AppHandle, tab_id: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_webview(&app, &tab_id)?;
        // Only one session per tab; arming again cancels the previous one.
        cancel_pick(&app, &tab_id);
        let (tx, rx) = mpsc::channel();
        pending_picks().lock().unwrap().insert(tab_id.clone(), tx);
        let theme = annotation_theme()
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(default_annotation_theme);
        if let Err(error) = picker_dispatch(&app, &tab_id, "preview:start-pick", &[theme]) {
            pending_picks().lock().unwrap().remove(&tab_id);
            return Err(error);
        }
        let outcome = rx
            .recv()
            .map_err(|_| "pick session dropped without settling".to_string())?;
        match outcome {
            None => Ok(Value::Null),
            Some((mut annotation, rect)) => {
                let screenshot = capture_annotation_screenshot(&app, &tab_id, rect.as_ref());
                if let Value::Object(map) = &mut annotation {
                    map.insert("screenshot".to_string(), screenshot);
                }
                // Screenshot done — let the guest tear the overlay down
                // (ANNOTATION_CAPTURED_CHANNEL in Electron).
                let _ = picker_dispatch(&app, &tab_id, "preview:annotation-captured", &[]);
                Ok(annotation)
            }
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_cancel_pick_element(app: AppHandle, tab_id: String) -> Result<(), String> {
    cancel_pick(&app, &tab_id);
    Ok(())
}

fn artifacts_dir(kind: &str) -> Result<PathBuf, String> {
    let config = CONFIG.get().ok_or("preview subsystem is not configured")?;
    let dir = config.artifacts_dir.join(kind);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

#[tauri::command]
pub async fn preview_capture_screenshot(app: AppHandle, tab_id: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (bytes, _, _) = capture_image(&app, &tab_id, None, None, platform::ImageFormat::Png)?;
        let sequence = ARTIFACT_SEQUENCE.fetch_add(1, Ordering::SeqCst);
        let id = format!("screenshot-{sequence}");
        let path = artifacts_dir("screenshots")?.join(format!("{id}.png"));
        std::fs::write(&path, &bytes).map_err(|error| error.to_string())?;
        Ok(json!({
            "id": id,
            "tabId": tab_id,
            "path": path.to_string_lossy(),
            "mimeType": "image/png",
            "sizeBytes": bytes.len(),
            "createdAt": now_iso(),
        }))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_reveal_artifact(path: String) -> Result<(), String> {
    let config = CONFIG.get().ok_or("preview subsystem is not configured")?;
    let resolved = PathBuf::from(&path);
    let canonical = resolved
        .canonicalize()
        .map_err(|error| format!("artifact not found: {error}"))?;
    let base = config
        .artifacts_dir
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !canonical.starts_with(&base) {
        return Err("artifact path is outside the preview artifacts directory".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&canonical)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("reveal is only implemented on macOS".to_string())
    }
}

// ---------------------------------------------------------------------------
// Shared frame-capture loop. Two consumers, mirroring the Electron
// screencast consumer set (Manager.ts): recording (frames broadcast on
// FRAME_EVENT for the renderer's MediaRecorder) and picture-in-picture
// (frames evaled straight into the PiP window's <img>). One thread per tab,
// alive while either consumer flag is set.

fn ensure_frame_loop(app: AppHandle, tab_id: String) {
    {
        let mut tabs = tabs().lock().unwrap();
        let Some(tab) = tabs.get_mut(&tab_id) else {
            return;
        };
        if tab.frame_loop {
            return;
        }
        tab.frame_loop = true;
    }
    std::thread::spawn(move || {
        let interval = Duration::from_millis(1_000 / RECORDING_FPS);
        let mut pip_aspect: Option<f64> = None;
        loop {
            // Consumer check + exit share one lock acquisition so a consumer
            // arriving between them still sees frame_loop cleared and
            // re-spawns the loop.
            let consumers = {
                let mut tabs = tabs().lock().unwrap();
                match tabs.get_mut(&tab_id) {
                    Some(tab) if tab.recording || tab.pip => {
                        Some((tab.recording, tab.pip, tab.pip_label()))
                    }
                    Some(tab) => {
                        tab.frame_loop = false;
                        None
                    }
                    None => None,
                }
            };
            let Some((recording, pip, pip_label)) = consumers else {
                break;
            };
            let started = Instant::now();
            match capture_image(
                &app,
                &tab_id,
                Some(MAX_SCREENSHOT_WIDTH),
                None,
                platform::ImageFormat::Jpeg,
            ) {
                Ok((bytes, width, height)) => {
                    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    if recording {
                        let _ = app.emit(
                            FRAME_EVENT,
                            json!({
                                "tabId": tab_id,
                                "data": data.as_str(),
                                "width": width,
                                "height": height,
                                "receivedAt": now_iso(),
                            }),
                        );
                    }
                    if pip {
                        if let Some(window) = app.get_webview_window(&pip_label) {
                            let _ = window.eval(&format!(
                                "document.getElementById('preview-frame').src = 'data:image/jpeg;base64,{data}';"
                            ));
                            // Keep the window aspect matched to the content —
                            // Electron's fitPictureInPictureContentSize.
                            if width > 0 && height > 0 {
                                let aspect = f64::from(width) / f64::from(height);
                                if pip_aspect.map_or(true, |last| (last - aspect).abs() > 0.01) {
                                    pip_aspect = Some(aspect);
                                    let _ = on_main(&app, move || {
                                        let (Ok(size), Ok(scale)) =
                                            (window.inner_size(), window.scale_factor())
                                        else {
                                            return;
                                        };
                                        let logical: tauri::LogicalSize<f64> =
                                            size.to_logical(scale);
                                        let _ = window.set_size(tauri::LogicalSize::new(
                                            logical.width,
                                            (logical.width / aspect).max(90.0),
                                        ));
                                    });
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    // Webview gone (tab closed mid-stream) — stop quietly and
                    // drop the consumers that can no longer be served.
                    if webview_of(&app, &tab_id).is_err() {
                        if let Some(tab) = tabs().lock().unwrap().get_mut(&tab_id) {
                            tab.recording = false;
                            tab.pip = false;
                            tab.frame_loop = false;
                        }
                        break;
                    }
                }
            }
            let elapsed = started.elapsed();
            if elapsed < interval {
                std::thread::sleep(interval - elapsed);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Commands: picture-in-picture — a small always-on-top window per tab fed
// by the shared frame loop (Electron parity: Manager.ts
// openPictureInPicture / buildPreviewPictureInPictureDataUrl).

const PIP_HTML: &str = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Preview</title><style>html,body{margin:0;height:100%;background:#000;overflow:hidden}img{display:block;width:100%;height:100%;object-fit:contain}</style></head><body><img id="preview-frame" alt=""></body></html>"#;

#[tauri::command]
pub async fn preview_pip_open(app: AppHandle, tab_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        ensure_webview(&app, &tab_id)?;
        let (label, content_width, content_height, already_open) = {
            let tabs = tabs().lock().unwrap();
            let tab = tabs
                .get(&tab_id)
                .ok_or_else(|| format!("Unknown preview tab: {tab_id}"))?;
            let (width, height) = tab
                .bounds
                .as_ref()
                .map(|bounds| (bounds.width.max(1.0), bounds.height.max(1.0)))
                .unwrap_or((DEFAULT_TAB_WIDTH, DEFAULT_TAB_HEIGHT));
            (tab.pip_label(), width, height, tab.pip)
        };
        if already_open && app.get_webview_window(&label).is_some() {
            return Ok(());
        }
        let pip_width = 480.0_f64;
        let pip_height = (pip_width * content_height / content_width).clamp(90.0, 480.0);
        let app_for_build = app.clone();
        let label_for_build = label.clone();
        on_main(&app, move || -> Result<(), String> {
            if app_for_build.get_webview_window(&label_for_build).is_some() {
                return Ok(());
            }
            let url = tauri::Url::parse(&format!(
                "data:text/html;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(PIP_HTML)
            ))
            .map_err(|error| error.to_string())?;
            tauri::WebviewWindowBuilder::new(
                &app_for_build,
                &label_for_build,
                tauri::WebviewUrl::External(url),
            )
            .title("Preview")
            .inner_size(pip_width, pip_height)
            .min_inner_size(160.0, 90.0)
            .always_on_top(true)
            .build()
            .map_err(|error| error.to_string())?;
            Ok(())
        })??;
        // The user can close the window directly (title-bar close button);
        // reflect that back into tab state.
        if let Some(window) = app.get_webview_window(&label) {
            let app_for_event = app.clone();
            let tab_for_event = tab_id.clone();
            window.on_window_event(move |event| {
                if matches!(event, tauri::WindowEvent::Destroyed) {
                    if let Some(tab) = tabs().lock().unwrap().get_mut(&tab_for_event) {
                        tab.pip = false;
                    }
                    emit_state(&app_for_event, &tab_for_event);
                }
            });
        }
        if let Some(tab) = tabs().lock().unwrap().get_mut(&tab_id) {
            tab.pip = true;
        }
        ensure_frame_loop(app.clone(), tab_id.clone());
        emit_state(&app, &tab_id);
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_pip_close(app: AppHandle, tab_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        close_pip_window(&app, &tab_id);
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Clears the PiP consumer flag and closes the tab's PiP window if open.
fn close_pip_window(app: &AppHandle, tab_id: &str) {
    let label = {
        let mut tabs = tabs().lock().unwrap();
        let Some(tab) = tabs.get_mut(tab_id) else {
            return;
        };
        tab.pip = false;
        tab.pip_label()
    };
    if let Some(window) = app.get_webview_window(&label) {
        let _ = on_main(app, move || window.close());
    }
    emit_state(app, tab_id);
}

// ---------------------------------------------------------------------------
// Commands: recording (shell captures frames; renderer encodes with
// MediaRecorder and sends the finished blob back through recording_save)

#[tauri::command]
pub async fn preview_recording_start(app: AppHandle, tab_id: String) -> Result<(), String> {
    {
        let mut tabs = tabs().lock().unwrap();
        let tab = tabs
            .get_mut(&tab_id)
            .ok_or_else(|| format!("Unknown preview tab: {tab_id}"))?;
        if tab.recording {
            return Ok(());
        }
        tab.recording = true;
    }
    webview_of(&app, &tab_id)?;
    ensure_frame_loop(app, tab_id);
    Ok(())
}

#[tauri::command]
pub async fn preview_recording_stop(_app: AppHandle, tab_id: String) -> Result<(), String> {
    if let Some(tab) = tabs().lock().unwrap().get_mut(&tab_id) {
        tab.recording = false;
    }
    Ok(())
}

#[tauri::command]
pub async fn preview_recording_save(
    tab_id: String,
    mime_type: String,
    data_base64: String,
) -> Result<Value, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|error| format!("invalid recording payload: {error}"))?;
    let extension = match mime_type.split(';').next().unwrap_or_default() {
        "video/webm" => "webm",
        "video/mp4" => "mp4",
        _ => "bin",
    };
    let sequence = ARTIFACT_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    let id = format!("recording-{sequence}");
    let path = artifacts_dir("recordings")?.join(format!("{id}.{extension}"));
    std::fs::write(&path, &bytes).map_err(|error| error.to_string())?;
    Ok(json!({
        "id": id,
        "tabId": tab_id,
        "path": path.to_string_lossy(),
        "mimeType": mime_type,
        "sizeBytes": bytes.len(),
        "createdAt": now_iso(),
    }))
}

// ---------------------------------------------------------------------------
// Commands: automation

#[tauri::command]
pub async fn preview_automation_status(app: AppHandle, tab_id: String) -> Result<Value, String> {
    let tabs = tabs().lock().unwrap();
    let Some(tab) = tabs.get(&tab_id) else {
        return Ok(json!({
            "available": false,
            "visible": false,
            "tabId": Value::Null,
            "url": Value::Null,
            "title": Value::Null,
            "loading": false,
        }));
    };
    let available = tab.webview_created && app.get_webview(&tab.label).is_some();
    Ok(json!({
        "available": available,
        "visible": tab.bounds.as_ref().is_some_and(|bounds| bounds.visible),
        "tabId": tab_id,
        "url": if tab.url.is_empty() { Value::Null } else { Value::from(tab.url.clone()) },
        "title": if tab.title.is_empty() { Value::Null } else { Value::from(tab.title.clone()) },
        "loading": tab.loading,
    }))
}

#[tauri::command]
pub async fn preview_automation_snapshot(app: AppHandle, tab_id: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_timeline(&app, &tab_id, "snapshot", || {
            let script = SNAPSHOT_PAGE_SCRIPT
                .replace("__MAX_ELEMENTS__", &MAX_INTERACTIVE_ELEMENTS.to_string())
                .replace("__MAX_TEXT__", &MAX_VISIBLE_TEXT_LENGTH.to_string());
            let page = run_eval(
                &app,
                &tab_id,
                &script,
                false,
                Duration::from_millis(EVAL_TIMEOUT_MS),
            )?;
            let (image_bytes, width, height) = capture_image(
                &app,
                &tab_id,
                Some(MAX_SCREENSHOT_WIDTH),
                None,
                platform::ImageFormat::Png,
            )?;
            let (console_entries, network_entries, timeline) = {
                let tabs = tabs().lock().unwrap();
                let tab = tabs
                    .get(&tab_id)
                    .ok_or_else(|| format!("Unknown preview tab: {tab_id}"))?;
                (
                    Value::Array(tab.console_entries.iter().cloned().collect()),
                    Value::Array(tab.network_entries.iter().cloned().collect()),
                    Value::Array(tab.timeline.iter().cloned().collect()),
                )
            };
            Ok(json!({
                "url": page["url"],
                "title": page["title"],
                "loading": page["loading"],
                "visibleText": page["visibleText"],
                "interactiveElements": page["interactiveElements"],
                // No CDP: WKWebView exposes no full-AX-tree equivalent.
                "accessibilityTree": Value::Null,
                "consoleEntries": console_entries,
                "networkEntries": network_entries,
                "actionTimeline": timeline,
                "screenshot": {
                    "mimeType": "image/png",
                    "data": base64::engine::general_purpose::STANDARD.encode(&image_bytes),
                    "width": width,
                    "height": height,
                },
            }))
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_automation_click(
    app: AppHandle,
    tab_id: String,
    input: ClickInput,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_timeline(&app, &tab_id, "click", || {
            let timeout =
                Duration::from_millis(input.timeout_ms.unwrap_or(EVAL_TIMEOUT_MS));
            let point = match automation_locator(input.selector.as_deref(), input.locator.as_deref())
            {
                None => (
                    input.x.ok_or("click requires locator, selector, or x/y")?,
                    input.y.ok_or("click requires locator, selector, or x/y")?,
                ),
                Some(locator) => {
                    let script = RESOLVE_CLICK_POINT_SCRIPT
                        .replace("__LOCATOR_JSON__", &json_string(&locator));
                    let resolved = run_eval(&app, &tab_id, &script, false, timeout)?;
                    check_automation_result("click", &resolved)?;
                    (
                        resolved["x"].as_f64().ok_or("click point resolution failed")?,
                        resolved["y"].as_f64().ok_or("click point resolution failed")?,
                    )
                }
            };
            let viewport = run_eval(
                &app,
                &tab_id,
                "({ width: window.innerWidth, height: window.innerHeight })",
                false,
                timeout,
            )?;
            let viewport_width = viewport["width"].as_f64().unwrap_or(0.0);
            let viewport_height = viewport["height"].as_f64().unwrap_or(0.0);
            if point.0 < 0.0
                || point.1 < 0.0
                || point.0 > viewport_width
                || point.1 > viewport_height
            {
                return Err(format!(
                    "click point ({}, {}) is outside the viewport ({viewport_width} x {viewport_height})",
                    point.0, point.1
                ));
            }
            emit_pointer(&app, &tab_id, "move", point.0, point.1);
            std::thread::sleep(Duration::from_millis(AGENT_CURSOR_MOVE_MS));
            emit_pointer(&app, &tab_id, "click", point.0, point.1);
            std::thread::sleep(Duration::from_millis(AGENT_CURSOR_CLICK_LEAD_MS));
            let script = CLICK_DISPATCH_SCRIPT
                .replace("__X__", &point.0.to_string())
                .replace("__Y__", &point.1.to_string());
            let result = run_eval(&app, &tab_id, &script, false, timeout)?;
            check_automation_result("click", &result)
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_automation_type(
    app: AppHandle,
    tab_id: String,
    input: TypeInput,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_timeline(&app, &tab_id, "type", || {
            let timeout = Duration::from_millis(input.timeout_ms.unwrap_or(EVAL_TIMEOUT_MS));
            let element_expr =
                match automation_locator(input.selector.as_deref(), input.locator.as_deref()) {
                    Some(locator) => playwright_element_expr(&json_string(&locator)),
                    None => "document.activeElement".to_string(),
                };
            let script = TYPE_SCRIPT
                .replace("__ELEMENT_EXPR__", &element_expr)
                .replace("__CLEAR__", if input.clear.unwrap_or(false) { "true" } else { "false" })
                .replace("__TEXT_JSON__", &json_string(&input.text));
            let result = run_eval(&app, &tab_id, &script, false, timeout)?;
            check_automation_result("type", &result)
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_automation_press(
    app: AppHandle,
    tab_id: String,
    input: PressInput,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_timeline(&app, &tab_id, "press", || {
            let modifiers = serde_json::to_string(&input.modifiers.unwrap_or_default())
                .map_err(|error| error.to_string())?;
            let script = PRESS_SCRIPT
                .replace("__KEY_JSON__", &json_string(&input.key))
                .replace("__MODS_JSON__", &modifiers);
            let result = run_eval(
                &app,
                &tab_id,
                &script,
                false,
                Duration::from_millis(EVAL_TIMEOUT_MS),
            )?;
            check_automation_result("press", &result)
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_automation_scroll(
    app: AppHandle,
    tab_id: String,
    input: ScrollInput,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_timeline(&app, &tab_id, "scroll", || {
            let target_expr =
                match automation_locator(input.selector.as_deref(), input.locator.as_deref()) {
                    Some(locator) => playwright_element_expr(&json_string(&locator)),
                    None => "window".to_string(),
                };
            let script = SCROLL_SCRIPT
                .replace("__TARGET_EXPR__", &target_expr)
                .replace("__DX__", &input.delta_x.unwrap_or(0.0).to_string())
                .replace("__DY__", &input.delta_y.unwrap_or(0.0).to_string());
            let result = run_eval(
                &app,
                &tab_id,
                &script,
                false,
                Duration::from_millis(EVAL_TIMEOUT_MS),
            )?;
            check_automation_result("scroll", &result)
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_automation_evaluate(
    app: AppHandle,
    tab_id: String,
    input: EvaluateInput,
) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_timeline(&app, &tab_id, "evaluate", || {
            let value = run_eval(
                &app,
                &tab_id,
                &input.expression,
                input.await_promise.unwrap_or(true),
                Duration::from_millis(EVAL_TIMEOUT_MS),
            )?;
            let serialized = serde_json::to_string(&value).map_err(|error| error.to_string())?;
            if serialized.len() > MAX_EVALUATION_BYTES {
                return Err(format!(
                    "evaluation result is {} bytes; maximum is {MAX_EVALUATION_BYTES}",
                    serialized.len()
                ));
            }
            Ok(value)
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn preview_automation_wait_for(
    app: AppHandle,
    tab_id: String,
    input: WaitForInput,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_timeline(&app, &tab_id, "waitFor", || {
            let timeout_ms = input.timeout_ms.unwrap_or(DEFAULT_WAIT_FOR_TIMEOUT_MS);
            let selector_expr =
                match automation_locator(input.selector.as_deref(), input.locator.as_deref()) {
                    Some(locator) => format!(
                        "(() => {{ const injected = globalThis.__t3PlaywrightInjected; return injected.querySelector(injected.parseSelector({}), document, false) !== null; }})()",
                        json_string(&locator)
                    ),
                    None => "true".to_string(),
                };
            let text_expr = match &input.text {
                Some(text) => format!(
                    "(document.body?.innerText || \"\").includes({})",
                    json_string(text)
                ),
                None => "true".to_string(),
            };
            let url_expr = match &input.url_includes {
                Some(fragment) => format!("location.href.includes({})", json_string(fragment)),
                None => "true".to_string(),
            };
            let script = WAIT_FOR_SCRIPT
                .replace("__SELECTOR_EXPR__", &selector_expr)
                .replace("__TEXT_EXPR__", &text_expr)
                .replace("__URL_EXPR__", &url_expr);
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            loop {
                let result = run_eval(
                    &app,
                    &tab_id,
                    &script,
                    false,
                    Duration::from_millis(EVAL_TIMEOUT_MS),
                )?;
                check_automation_result("waitFor", &result)?;
                if result["matched"].as_bool() == Some(true) {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(format!("waitFor timed out after {timeout_ms}ms"));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

// ---------------------------------------------------------------------------
// Frame capture

/// Captures the current webview contents. Must NOT be called from the main
/// thread: the WebKit completion handler is delivered on the main run loop,
/// so blocking main here would deadlock.
fn capture_image(
    app: &AppHandle,
    tab_id: &str,
    max_width: Option<f64>,
    rect: Option<(f64, f64, f64, f64)>,
    format: platform::ImageFormat,
) -> Result<(Vec<u8>, u32, u32), String> {
    let webview = webview_of(app, tab_id)?;
    let (tx, rx) = mpsc::channel::<Result<(Vec<u8>, u32, u32), String>>();
    app.run_on_main_thread(move || {
        platform::take_snapshot(&webview, max_width, rect, format, tx);
    })
    .map_err(|error| error.to_string())?;
    rx.recv_timeout(Duration::from_secs(10))
        .map_err(|_| "snapshot capture timed out".to_string())?
}

// ---------------------------------------------------------------------------
// Platform layer (objc2 dynamic messaging on macOS; other platforms are M4)

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::mpsc;

    use block2::RcBlock;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    #[derive(Clone, Copy)]
    pub enum ImageFormat {
        Png,
        Jpeg,
    }

    impl ImageFormat {
        // NSBitmapImageFileType: JPEG = 3, PNG = 4.
        fn file_type(self) -> usize {
            match self {
                ImageFormat::Png => 4,
                ImageFormat::Jpeg => 3,
            }
        }
    }

    unsafe fn ns_string(value: &str) -> *mut AnyObject {
        let c = std::ffi::CString::new(value).unwrap_or_default();
        msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
    }

    /// Runs `operation` against the raw WKWebView pointer. Must be called on
    /// the main thread, where `with_webview` executes inline.
    fn with_wk<T: Send + 'static>(
        webview: &tauri::webview::Webview,
        operation: impl FnOnce(*mut AnyObject) -> T + Send + 'static,
    ) -> Option<T> {
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        let slot_for_closure = slot.clone();
        let dispatched = webview.with_webview(move |platform_webview| {
            let wk = platform_webview.inner() as *mut AnyObject;
            *slot_for_closure.lock().unwrap() = Some(operation(wk));
        });
        if dispatched.is_err() {
            return None;
        }
        let value = slot.lock().unwrap().take();
        value
    }

    pub fn nav_flags(webview: &tauri::webview::Webview) -> Option<(bool, bool)> {
        with_wk(webview, |wk| unsafe {
            let back: bool = msg_send![wk, canGoBack];
            let forward: bool = msg_send![wk, canGoForward];
            (back, forward)
        })
    }

    pub fn nav_action(webview: &tauri::webview::Webview, action: &'static str) -> Result<(), String> {
        with_wk(webview, move |wk| unsafe {
            let _: *mut AnyObject = match action {
                "back" => msg_send![wk, goBack],
                "forward" => msg_send![wk, goForward],
                "reload" => msg_send![wk, reload],
                "reload-from-origin" => msg_send![wk, reloadFromOrigin],
                _ => return,
            };
        })
        .ok_or_else(|| "webview is gone".to_string())
    }

    pub fn set_page_zoom(webview: &tauri::webview::Webview, zoom: f64) {
        let _ = with_wk(webview, move |wk| unsafe {
            let _: () = msg_send![wk, setPageZoom: zoom];
        });
    }

    pub fn set_color_scheme(webview: &tauri::webview::Webview, scheme: &str) {
        let name = match scheme {
            "light" => Some("NSAppearanceNameAqua"),
            "dark" => Some("NSAppearanceNameDarkAqua"),
            _ => None,
        };
        let _ = with_wk(webview, move |wk| unsafe {
            let appearance: *mut AnyObject = match name {
                Some(name) => {
                    let ns_name = ns_string(name);
                    msg_send![class!(NSAppearance), appearanceNamed: ns_name]
                }
                None => std::ptr::null_mut(),
            };
            let _: () = msg_send![wk, setAppearance: appearance];
        });
    }

    // CGRect by-value messaging for WKSnapshotConfiguration.rect; objc2 only
    // needs the encodings to match the Objective-C runtime's.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGSize {
        pub width: f64,
        pub height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGRect {
        pub origin: CGPoint,
        pub size: CGSize,
    }
    unsafe impl objc2::Encode for CGPoint {
        const ENCODING: objc2::Encoding =
            objc2::Encoding::Struct("CGPoint", &[objc2::Encoding::Double, objc2::Encoding::Double]);
    }
    unsafe impl objc2::Encode for CGSize {
        const ENCODING: objc2::Encoding =
            objc2::Encoding::Struct("CGSize", &[objc2::Encoding::Double, objc2::Encoding::Double]);
    }
    unsafe impl objc2::Encode for CGRect {
        const ENCODING: objc2::Encoding =
            objc2::Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
    }

    /// Kicks off `takeSnapshotWithConfiguration:`; the completion handler
    /// re-encodes through NSBitmapImageRep and reports over `tx`. Must run on
    /// the main thread; never blocks it. `rect` is in the web view's
    /// coordinate system (view points).
    pub fn take_snapshot(
        webview: &tauri::webview::Webview,
        max_width: Option<f64>,
        rect: Option<(f64, f64, f64, f64)>,
        format: ImageFormat,
        tx: mpsc::Sender<Result<(Vec<u8>, u32, u32), String>>,
    ) {
        let tx_for_error = tx.clone();
        let dispatched = with_wk(webview, move |wk| unsafe {
            let configuration: *mut AnyObject = msg_send![class!(WKSnapshotConfiguration), new];
            if let Some(width) = max_width {
                let number: *mut AnyObject = msg_send![class!(NSNumber), numberWithDouble: width];
                let _: () = msg_send![configuration, setSnapshotWidth: number];
            }
            if let Some((x, y, width, height)) = rect {
                let cg_rect = CGRect {
                    origin: CGPoint { x, y },
                    size: CGSize { width, height },
                };
                let _: () = msg_send![configuration, setRect: cg_rect];
            }
            let tx_for_block = tx.clone();
            let completion = RcBlock::new(move |image: *mut AnyObject, _error: *mut AnyObject| {
                if image.is_null() {
                    let _ = tx_for_block.send(Err("snapshot returned no image".to_string()));
                    return;
                }
                let _ = tx_for_block.send(encode_image(image, format));
            });
            let _: () = msg_send![
                wk,
                takeSnapshotWithConfiguration: configuration,
                completionHandler: &*completion
            ];
            let _: () = msg_send![configuration, release];
        });
        if dispatched.is_none() {
            let _ = tx_for_error.send(Err("webview is gone".to_string()));
        }
    }

    unsafe fn encode_image(
        image: *mut AnyObject,
        format: ImageFormat,
    ) -> Result<(Vec<u8>, u32, u32), String> {
        let tiff: *mut AnyObject = msg_send![image, TIFFRepresentation];
        if tiff.is_null() {
            return Err("snapshot image has no TIFF representation".to_string());
        }
        let rep: *mut AnyObject = msg_send![class!(NSBitmapImageRep), imageRepWithData: tiff];
        if rep.is_null() {
            return Err("failed to build a bitmap rep from the snapshot".to_string());
        }
        let properties: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
        let data: *mut AnyObject = msg_send![
            rep,
            representationUsingType: format.file_type(),
            properties: properties
        ];
        if data.is_null() {
            return Err("failed to encode the snapshot image".to_string());
        }
        let length: usize = msg_send![data, length];
        let bytes: *const u8 = msg_send![data, bytes];
        let encoded = std::slice::from_raw_parts(bytes, length).to_vec();
        let width: isize = msg_send![rep, pixelsWide];
        let height: isize = msg_send![rep, pixelsHigh];
        Ok((encoded, width.max(0) as u32, height.max(0) as u32))
    }

    /// Clears data from the shared default WKWebsiteDataStore. Must run on
    /// the main thread; completion reports over `tx`.
    pub fn clear_website_data(types: &[&str], tx: mpsc::Sender<Result<(), String>>) {
        unsafe {
            let array: *mut AnyObject = msg_send![class!(NSMutableArray), array];
            for data_type in types {
                let ns_type = ns_string(data_type);
                let _: () = msg_send![array, addObject: ns_type];
            }
            let set: *mut AnyObject = msg_send![class!(NSSet), setWithArray: array];
            let store: *mut AnyObject = msg_send![class!(WKWebsiteDataStore), defaultDataStore];
            let since: *mut AnyObject = msg_send![class!(NSDate), distantPast];
            let completion = RcBlock::new(move || {
                let _ = tx.send(Ok(()));
            });
            let _: () = msg_send![
                store,
                removeDataOfTypes: set,
                modifiedSince: since,
                completionHandler: &*completion
            ];
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use std::sync::mpsc;

    #[derive(Clone, Copy)]
    pub enum ImageFormat {
        Png,
        Jpeg,
    }

    pub fn nav_flags(_webview: &tauri::webview::Webview) -> Option<(bool, bool)> {
        None
    }

    pub fn nav_action(
        _webview: &tauri::webview::Webview,
        _action: &'static str,
    ) -> Result<(), String> {
        Err("preview navigation controls are macOS-only in M2".to_string())
    }

    pub fn set_page_zoom(_webview: &tauri::webview::Webview, _zoom: f64) {}

    pub fn set_color_scheme(_webview: &tauri::webview::Webview, _scheme: &str) {}

    pub fn take_snapshot(
        _webview: &tauri::webview::Webview,
        _max_width: Option<f64>,
        _rect: Option<(f64, f64, f64, f64)>,
        _format: ImageFormat,
        tx: mpsc::Sender<Result<(Vec<u8>, u32, u32), String>>,
    ) {
        let _ = tx.send(Err("preview capture is macOS-only in M2".to_string()));
    }

    pub fn clear_website_data(_types: &[&str], tx: mpsc::Sender<Result<(), String>>) {
        let _ = tx.send(Err("clearing website data is macOS-only in M2".to_string()));
    }
}
