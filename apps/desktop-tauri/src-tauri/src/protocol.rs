//! `t3code-tauri://app/` custom-protocol serving, mirroring the Electron
//! app's ElectronProtocol.ts: proxy every renderer request to the target
//! origin (the Vite dev server in dev, the backend's static client serving in
//! prod) and stamp a Content-Security-Policy on the response. This gives the
//! renderer a stable origin independent of the backend port.

use std::sync::OnceLock;
use std::time::Duration;

pub const SCHEME: &str = "t3code-tauri";
pub const HOST: &str = "app";

static TARGET: OnceLock<reqwest::Url> = OnceLock::new();
static CSP: OnceLock<String> = OnceLock::new();
static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// Mirror of `makeDesktopContentSecurityPolicy` (ElectronProtocol.ts:67-95),
/// minus the Clerk origin — cloud auth is not part of milestone 1.
fn make_csp(scheme: &str) -> String {
    [
        "default-src 'self'".to_string(),
        "script-src 'self' 'unsafe-inline' https://challenges.cloudflare.com".to_string(),
        // The renderer connects directly to local backends and user-configured
        // environments whose origins aren't known here; restrict by scheme.
        "connect-src 'self' http: https: ws: wss:".to_string(),
        format!("img-src 'self' {scheme}: blob: data: http: https:"),
        "style-src 'self' 'unsafe-inline'".to_string(),
        format!("font-src 'self' {scheme}: data:"),
        "worker-src 'self' blob:".to_string(),
        "frame-src 'self' https://challenges.cloudflare.com".to_string(),
        "form-action 'self'".to_string(),
    ]
    .join("; ")
}

pub fn set_target(origin: &str) {
    let url = reqwest::Url::parse(origin).expect("proxy target origin parses");
    let _ = TARGET.set(url);
    let _ = CSP.set(make_csp(SCHEME));
}

/// Request headers never forwarded upstream (same list as proxyRequest in
/// ElectronProtocol.ts). Dropping accept-encoding keeps upstream responses
/// unencoded so we can re-frame them without a decompression step.
const STRIPPED_REQUEST_HEADERS: [&str; 7] = [
    "host",
    "origin",
    "referer",
    "connection",
    "content-length",
    "accept-encoding",
    "upgrade-insecure-requests",
];

const STRIPPED_RESPONSE_HEADERS: [&str; 4] = [
    "transfer-encoding",
    "content-length",
    "connection",
    "content-security-policy",
];

const GET_RETRY_DELAYS_MS: [u64; 3] = [0, 50, 150];

pub fn handler<R: tauri::Runtime>(
    _ctx: tauri::UriSchemeContext<'_, R>,
    request: tauri::http::Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    tauri::async_runtime::spawn(async move {
        let response = match proxy(request).await {
            Ok(response) => response,
            Err(message) => tauri::http::Response::builder()
                .status(502)
                .header("content-type", "text/plain")
                .body(message.into_bytes())
                .expect("static error response builds"),
        };
        responder.respond(response);
    });
}

async fn proxy(
    request: tauri::http::Request<Vec<u8>>,
) -> Result<tauri::http::Response<Vec<u8>>, String> {
    let uri = request.uri();
    if uri.host() != Some(HOST) {
        return tauri::http::Response::builder()
            .status(404)
            .body(Vec::new())
            .map_err(|error| error.to_string());
    }
    let target = TARGET
        .get()
        .ok_or("desktop protocol target is not configured yet")?;

    let mut url = target.clone();
    url.set_path(uri.path());
    url.set_query(uri.query());

    let client = CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest client builds")
    });

    let method = reqwest::Method::from_bytes(request.method().as_str().as_bytes())
        .map_err(|error| error.to_string())?;
    let idempotent = matches!(method, reqwest::Method::GET | reqwest::Method::HEAD);

    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in request.headers() {
        let lower = name.as_str().to_ascii_lowercase();
        if STRIPPED_REQUEST_HEADERS.contains(&lower.as_str()) || lower.starts_with("sec-fetch-") {
            continue;
        }
        let Ok(header_name) = reqwest::header::HeaderName::from_bytes(lower.as_bytes()) else {
            continue;
        };
        let Ok(header_value) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) else {
            continue;
        };
        headers.append(header_name, header_value);
    }

    let send = |attempt_headers: reqwest::header::HeaderMap| {
        let mut upstream = client
            .request(method.clone(), url.clone())
            .headers(attempt_headers);
        if !idempotent {
            upstream = upstream.body(request.body().clone());
        }
        upstream.send()
    };

    // Same transient-retry shape as fetchWithTransientRetry for reads; writes
    // go out exactly once.
    let mut response = None;
    let mut last_error = String::from("unreachable");
    let delays: &[u64] = if idempotent {
        &GET_RETRY_DELAYS_MS
    } else {
        &GET_RETRY_DELAYS_MS[..1]
    };
    for delay_ms in delays {
        if *delay_ms > 0 {
            tokio_sleep(Duration::from_millis(*delay_ms)).await;
        }
        match send(headers.clone()).await {
            Ok(upstream_response) => {
                response = Some(upstream_response);
                break;
            }
            Err(error) => {
                last_error = error.to_string();
            }
        }
    }
    let response = response.ok_or(last_error)?;

    let mut builder = tauri::http::Response::builder().status(response.status().as_u16());
    for (name, value) in response.headers() {
        if STRIPPED_RESPONSE_HEADERS.contains(&name.as_str()) {
            continue;
        }
        builder = builder.header(name.as_str(), value.as_bytes());
    }
    builder = builder.header(
        "Content-Security-Policy",
        CSP.get().map(String::as_str).unwrap_or_default(),
    );

    let body = response.bytes().await.map_err(|error| error.to_string())?;
    builder
        .body(body.to_vec())
        .map_err(|error| error.to_string())
}

async fn tokio_sleep(duration: Duration) {
    // tauri::async_runtime is tokio, but the `time` feature isn't guaranteed
    // through our feature set; a spawn_blocking sleep is dependency-free.
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(duration)).await;
}
