//! `vitre://app/` custom-protocol serving, mirroring the Electron
//! app's ElectronProtocol.ts: proxy every renderer request to the target
//! origin (the Vite dev server in dev, the backend's static client serving in
//! prod) and stamp a Content-Security-Policy on the response. This gives the
//! renderer a stable origin independent of the backend port.

use std::sync::OnceLock;
use std::time::Duration;

pub const SCHEME: &str = "vitre";
pub const HOST: &str = "app";

static TARGET: OnceLock<reqwest::Url> = OnceLock::new();
static CSP: OnceLock<String> = OnceLock::new();
static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// Mirror of `clerkFrontendApiHostnameFromPublishableKey`
/// (packages/shared/src/relayAuth.ts): the frontend-API hostname is base64
/// inside the publishable key, with a trailing `$`. The key reaches this
/// process through the repo env (dev-runner assigns loadRepoEnv() into the
/// environment; packaged builds must inject it the same way the web bundle
/// bakes VITE_CLERK_PUBLISHABLE_KEY).
fn clerk_frontend_api_hostname() -> Option<String> {
    use base64::Engine as _;

    let key = std::env::var("VITE_CLERK_PUBLISHABLE_KEY")
        .or_else(|_| std::env::var("T3CODE_CLERK_PUBLISHABLE_KEY"))
        .ok()
        // Packaged fallback: scripts/build-app.ts exports the repo .env key to
        // `cargo build`, mirroring how the web bundle bakes it at build time.
        .or_else(|| option_env!("VITE_CLERK_PUBLISHABLE_KEY").map(String::from))?;
    let key = key.trim();
    let encoded = key
        .strip_prefix("pk_test_")
        .or_else(|| key.strip_prefix("pk_live_"))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(encoded))
        .ok()?;
    let hostname = String::from_utf8(decoded).ok()?;
    let hostname = hostname.trim_end_matches('$');
    if hostname.is_empty() || hostname.contains('/') {
        return None;
    }
    Some(hostname.to_string())
}

/// Mirror of `makeDesktopContentSecurityPolicy` (ElectronProtocol.ts:67-95),
/// including the Clerk frontend-API origin in script-src when a publishable
/// key is configured (clerk-js is loaded from that host by the web provider).
fn make_csp(scheme: &str) -> String {
    let script_src = match clerk_frontend_api_hostname() {
        Some(hostname) => format!(
            "script-src 'self' 'unsafe-inline' https://{hostname} https://challenges.cloudflare.com"
        ),
        None => "script-src 'self' 'unsafe-inline' https://challenges.cloudflare.com".to_string(),
    };
    [
        "default-src 'self'".to_string(),
        script_src,
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
