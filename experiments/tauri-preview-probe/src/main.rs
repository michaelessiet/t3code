//! Viability probe for running T3 Code's preview subsystem on Tauri 2 /
//! WKWebView instead of Electron/Chromium.
//!
//! The preview subsystem's Chromium dependencies are: CDP Runtime.evaluate
//! (InjectedScript automation with results), DOM snapshot + element
//! targeting, navigation control/events, ~12fps frame capture (recording),
//! and CDP Network-domain observation. This app exercises a WKWebView
//! equivalent for each and prints a PROBE REPORT to stdout:
//!
//! 1. eval-with-result — `Webview::eval` is fire-and-forget, so results
//!    return through a `probe://` custom protocol the injected runtime
//!    fetches (WKURLSchemeHandler intercepts fetch; CORS headers added).
//! 2. DOM snapshot — injected runtime serializes interactive elements.
//! 3. navigation — `Webview::navigate` + `on_page_load` events.
//! 4. frame capture — `WKWebView takeSnapshotWithConfiguration` via objc2
//!    dynamic messaging, driven at a 12fps target for 5s.
//! 5. network observation — no CDP equivalent exists; the injected runtime
//!    monkeypatches fetch/XHR (documented as partial: no response bodies,
//!    no subresource/navigation requests).
//!
//! Guests load from a built-in localhost HTTP server so the probe is
//! self-contained (animated canvas + periodic fetch traffic + clickables).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{LogicalPosition, LogicalSize, Manager};

const GUEST_SERVER_PORT: u16 = 43117;
const GUEST_COUNT: usize = 3;
const SNAPSHOT_TARGET_FPS: u32 = 12;
const SNAPSHOT_WINDOW_SECS: u64 = 5;

const PROBE_RUNTIME_JS: &str = r#"
(() => {
  if (window.__probeInstalled) return;
  window.__probeInstalled = true;
  const post = (payload) => {
    try {
      return fetch("probe://ipc/message", {
        method: "POST",
        body: JSON.stringify({ ...payload, href: location.href }),
      }).then(() => true).catch((error) => {
        window.__probeLastPostError = String(error);
        return false;
      });
    } catch (error) {
      window.__probeLastPostError = String(error);
      return Promise.resolve(false);
    }
  };
  const originalFetch = window.fetch.bind(window);
  window.fetch = (...args) => {
    const url = String(args[0] instanceof Request ? args[0].url : args[0]);
    if (!url.startsWith("probe://")) void post({ kind: "net", api: "fetch", url });
    return originalFetch(...args);
  };
  const originalOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (method, url, ...rest) {
    void post({ kind: "net", api: "xhr", method: String(method), url: String(url) });
    return originalOpen.call(this, method, url, ...rest);
  };
  window.__probeEval = (id, code) => {
    const payload = { kind: "eval", id };
    try {
      payload.result = String(eval(code));
    } catch (error) {
      payload.error = String(error);
    }
    void post(payload);
  };
  window.__probeSnapshotDom = (id) => {
    const elements = [...document.querySelectorAll("a, button, input, [onclick]")]
      .slice(0, 50)
      .map((element) => {
        const rect = element.getBoundingClientRect();
        return {
          tag: element.tagName.toLowerCase(),
          text: (element.textContent ?? "").trim().slice(0, 60),
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
        };
      });
    void post({ kind: "dom", id, title: document.title, elements });
  };
  window.addEventListener("load", () => void post({ kind: "guest-load" }));
})();
"#;

fn guest_page(label: &str) -> String {
    format!(
        r##"<!doctype html>
<html><head><title>guest {label}</title></head>
<body style="margin:0;font-family:sans-serif">
<button id="probe-btn" onclick="document.title='clicked'">click target</button>
<a href="/two">second page</a>
<canvas id="c" width="640" height="360"></canvas>
<script>
  const ctx = document.getElementById("c").getContext("2d");
  let t = 0;
  (function draw() {{
    t += 1;
    ctx.fillStyle = "hsl(" + (t % 360) + ",70%,50%)";
    ctx.fillRect(0, 0, 640, 360);
    ctx.fillStyle = "#fff";
    ctx.font = "32px sans-serif";
    ctx.fillText("{label} frame " + t, 20, 60);
    requestAnimationFrame(draw);
  }})();
  setInterval(() => fetch("/ping").catch(() => null), 500);
</script>
</body></html>"##
    )
}

/// Minimal single-threaded HTTP server for guest pages; the probe must not
/// depend on external network availability.
fn spawn_guest_server() {
    std::thread::spawn(|| {
        let listener = TcpListener::bind(("127.0.0.1", GUEST_SERVER_PORT))
            .expect("guest server port should be free");
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    return;
                }
                let path = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                        break;
                    }
                }
                let (status, content_type, body) = match path.as_str() {
                    "/ping" => ("200 OK", "text/plain", "pong".to_string()),
                    "/two" => ("200 OK", "text/html", guest_page("page-two")),
                    _ => ("200 OK", "text/html", guest_page(&path)),
                };
                let mut stream = reader.into_inner();
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len(),
                );
            });
        }
    });
}

#[derive(Default)]
struct ProbeInbox {
    messages: Vec<serde_json::Value>,
}

fn wait_for_message(
    inbox: &Arc<Mutex<ProbeInbox>>,
    timeout: Duration,
    predicate: impl Fn(&serde_json::Value) -> bool,
) -> Option<serde_json::Value> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(found) = inbox
            .lock()
            .unwrap()
            .messages
            .iter()
            .find(|message| predicate(message))
        {
            return Some(found.clone());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

fn count_messages(
    inbox: &Arc<Mutex<ProbeInbox>>,
    predicate: impl Fn(&serde_json::Value) -> bool,
) -> usize {
    inbox
        .lock()
        .unwrap()
        .messages
        .iter()
        .filter(|message| predicate(message))
        .count()
}

/// One `takeSnapshotWithConfiguration:` call via dynamic objc messaging.
/// Bumps `frames` and records the TIFF byte length once WebKit delivers the
/// image. Must run on the main thread.
#[cfg(target_os = "macos")]
fn take_snapshot(
    webview: &tauri::webview::Webview,
    frames: Arc<AtomicUsize>,
    last_bytes: Arc<AtomicUsize>,
) {
    use block2::RcBlock;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    let _ = webview.with_webview(move |platform_webview| unsafe {
        let wk_webview = platform_webview.inner() as *mut AnyObject;
        let configuration: *mut AnyObject = msg_send![class!(WKSnapshotConfiguration), new];
        let completion = RcBlock::new(move |image: *mut AnyObject, _error: *mut AnyObject| {
            if image.is_null() {
                return;
            }
            let tiff: *mut AnyObject = msg_send![image, TIFFRepresentation];
            if !tiff.is_null() {
                let length: usize = msg_send![tiff, length];
                last_bytes.store(length, Ordering::SeqCst);
            }
            frames.fetch_add(1, Ordering::SeqCst);
        });
        let _: () = msg_send![
            wk_webview,
            takeSnapshotWithConfiguration: configuration,
            completionHandler: &*completion
        ];
        let _: () = msg_send![configuration, release];
    });
}

#[cfg(not(target_os = "macos"))]
fn take_snapshot(
    _webview: &tauri::webview::Webview,
    _frames: Arc<AtomicUsize>,
    _last_bytes: Arc<AtomicUsize>,
) {
}

fn own_memory_report() -> String {
    let own_pid = std::process::id();
    let own = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &own_pid.to_string()])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|rss| rss.trim().parse::<u64>().ok())
        .unwrap_or(0);
    format!("app process RSS: {}MB (WebContent processes are external XPC services — see driver script for the before/after delta)", own / 1024)
}

fn main() {
    spawn_guest_server();

    let inbox: Arc<Mutex<ProbeInbox>> = Arc::new(Mutex::new(ProbeInbox::default()));
    let protocol_inbox = inbox.clone();

    tauri::Builder::default()
        .register_uri_scheme_protocol("probe", move |_context, request| {
            let body = String::from_utf8_lossy(request.body()).to_string();
            if let Ok(message) = serde_json::from_str::<serde_json::Value>(&body) {
                protocol_inbox.lock().unwrap().messages.push(message);
            }
            // CORS header so guest pages on http://127.0.0.1 may fetch us.
            tauri::http::Response::builder()
                .status(200)
                .header("Access-Control-Allow-Origin", "*")
                .header("Content-Type", "text/plain")
                .body(b"ok".to_vec())
                .expect("static response should build")
        })
        .setup(move |app| {
            let window = tauri::window::WindowBuilder::new(app, "main")
                .title("tauri-preview-probe")
                .inner_size(1340.0, 460.0)
                .build()?;

            let mut nav_log: Vec<String> = Vec::new();
            let nav_events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(nav_log.drain(..).collect()));
            for index in 0..GUEST_COUNT {
                let label = format!("guest-{index}");
                let url: tauri::Url =
                    format!("http://127.0.0.1:{GUEST_SERVER_PORT}/{label}").parse()?;
                let events = nav_events.clone();
                let event_label = label.clone();
                let builder = tauri::webview::WebviewBuilder::new(&label, tauri::WebviewUrl::External(url))
                    .initialization_script(PROBE_RUNTIME_JS)
                    .on_page_load(move |_webview, payload| {
                        if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                            events
                                .lock()
                                .unwrap()
                                .push(format!("{event_label}: {}", payload.url()));
                        }
                    });
                window.add_child(
                    builder,
                    LogicalPosition::new(10.0 + (index as f64) * 445.0, 10.0),
                    LogicalSize::new(435.0, 435.0),
                )?;
            }

            let app_handle = app.handle().clone();
            let orchestrator_inbox = inbox.clone();
            std::thread::spawn(move || {
                let report = run_probe(&app_handle, &orchestrator_inbox, &nav_events);
                println!("{report}");
                std::thread::sleep(Duration::from_secs(15));
                app_handle.exit(0);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("probe app should run");
}

fn run_probe(
    app: &tauri::AppHandle,
    inbox: &Arc<Mutex<ProbeInbox>>,
    nav_events: &Arc<Mutex<Vec<String>>>,
) -> String {
    let mut report = String::from("\n===== PROBE REPORT =====\n");

    // 0. Guests load + injected runtime reachable over probe://.
    let loads = (0..GUEST_COUNT)
        .filter(|_| {
            wait_for_message(inbox, Duration::from_secs(15), |message| {
                message["kind"] == "guest-load"
            })
            .is_some()
        })
        .count();
    let load_count = count_messages(inbox, |message| message["kind"] == "guest-load");
    report.push_str(&format!(
        "[{}] guest load + custom-protocol IPC: {load_count}/{GUEST_COUNT} guests posted load events (waited: {loads})\n",
        if load_count >= 1 { "PASS" } else { "FAIL" },
    ));

    let guest = app.get_webview("guest-0");

    // 1. eval with result.
    if let Some(guest) = &guest {
        let _ = guest.eval(r#"__probeEval(1, "document.title + ' / ' + (6 * 7)")"#);
        let result = wait_for_message(inbox, Duration::from_secs(10), |message| {
            message["kind"] == "eval" && message["id"] == 1
        });
        report.push_str(&match result {
            Some(message) => format!(
                "[PASS] eval with result: {}\n",
                message["result"].as_str().unwrap_or("<non-string>"),
            ),
            None => "[FAIL] eval with result: no eval response received\n".to_string(),
        });
    } else {
        report.push_str("[FAIL] eval with result: guest-0 webview missing\n");
    }

    // 2. DOM snapshot + element targeting.
    if let Some(guest) = &guest {
        let _ = guest.eval("__probeSnapshotDom(2)");
        let snapshot = wait_for_message(inbox, Duration::from_secs(10), |message| {
            message["kind"] == "dom" && message["id"] == 2
        });
        report.push_str(&match snapshot {
            Some(message) => {
                let elements = message["elements"].as_array().map(Vec::len).unwrap_or(0);
                format!(
                    "[{}] dom snapshot: {elements} interactive elements with layout rects (title: {})\n",
                    if elements >= 2 { "PASS" } else { "FAIL" },
                    message["title"].as_str().unwrap_or("?"),
                )
            }
            None => "[FAIL] dom snapshot: no response\n".to_string(),
        });
    }

    // 3. Navigation control + events.
    if let Some(guest) = app.get_webview("guest-1") {
        let before = nav_events.lock().unwrap().len();
        let url: Result<tauri::Url, _> =
            format!("http://127.0.0.1:{GUEST_SERVER_PORT}/two").parse();
        let navigated = url.is_ok_and(|url| guest.navigate(url).is_ok());
        let mut observed = false;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if nav_events
                .lock()
                .unwrap()
                .iter()
                .skip(before)
                .any(|entry| entry.contains("/two"))
            {
                observed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        report.push_str(&format!(
            "[{}] navigation control + page-load events: navigate() ok={navigated}, finished event observed={observed}\n",
            if navigated && observed { "PASS" } else { "FAIL" },
        ));
    }

    // 4. Frame capture at the recording cadence.
    if let Some(guest) = guest.clone() {
        let frames = Arc::new(AtomicUsize::new(0));
        let last_bytes = Arc::new(AtomicUsize::new(0));
        let started = Instant::now();
        let interval = Duration::from_millis(1000 / SNAPSHOT_TARGET_FPS as u64);
        while started.elapsed() < Duration::from_secs(SNAPSHOT_WINDOW_SECS) {
            let frames_for_call = frames.clone();
            let bytes_for_call = last_bytes.clone();
            let guest_for_call = guest.clone();
            let _ = app.run_on_main_thread(move || {
                take_snapshot(&guest_for_call, frames_for_call, bytes_for_call);
            });
            std::thread::sleep(interval);
        }
        std::thread::sleep(Duration::from_millis(500));
        let captured = frames.load(Ordering::SeqCst);
        let fps = captured as f64 / SNAPSHOT_WINDOW_SECS as f64;
        report.push_str(&format!(
            "[{}] frame capture (takeSnapshotWithConfiguration): {captured} frames in {SNAPSHOT_WINDOW_SECS}s = {fps:.1}fps (target {SNAPSHOT_TARGET_FPS}), last frame {}KB TIFF\n",
            if fps >= 10.0 { "PASS" } else { "FAIL" },
            last_bytes.load(Ordering::SeqCst) / 1024,
        ));
    }

    // 5. Network observation (JS monkeypatch — the only non-proxy option).
    let network_events = count_messages(inbox, |message| message["kind"] == "net");
    report.push_str(&format!(
        "[{}] network observation: {network_events} fetch/XHR events via injected monkeypatch (PARTIAL by design: no response bodies, no subresource or navigation requests — CDP Network domain has no WKWebView equivalent)\n",
        if network_events >= 3 { "PASS" } else { "FAIL" },
    ));

    report.push_str(&format!("[info] {}\n", own_memory_report()));
    report.push_str("===== END PROBE REPORT =====");
    report
}
