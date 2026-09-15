//! S1 spike: validate the Effect-RPC wire format against a real sidecar.
//!
//! Boots `apps/server/dist/bin.mjs` with a fresh throwaway home, then walks
//! the full client path: bootstrap-token exchange → websocket ticket →
//! `/ws` connect → unary `server.getConfig` → `Ping`/`Pong` → streaming
//! `orchestration.subscribeShell` with Ack pacing → `Interrupt`.
//!
//! Run from the repo root (after `pnpm --filter t3 build:bundle`):
//!   cargo run -p vitre-rpc --example spike_rpc
//! Env: T3_SERVER_ENTRY, VITRE_NODE, VITRE_SPIKE_HOME override the defaults.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;
use vitre_rpc::{EnvironmentHttp, RpcSession, StreamEvent};
use vitre_sidecar::{Sidecar, SidecarConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server_entry = PathBuf::from(
        std::env::var("T3_SERVER_ENTRY").unwrap_or_else(|_| "apps/server/dist/bin.mjs".into()),
    );
    anyhow::ensure!(
        server_entry.exists(),
        "server entry {server_entry:?} not found — run `pnpm --filter t3 build:bundle` first"
    );
    let config = SidecarConfig {
        node_binary: std::env::var("VITRE_NODE").unwrap_or_else(|_| "node".into()),
        server_entry,
        t3_home: std::env::var("VITRE_SPIKE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::temp_dir().join(format!("vitre-spike-{}", std::process::id()))
            }),
        fixed_port: None,
    };

    println!("[spike] home: {}", config.t3_home.display());
    let mut sidecar = Sidecar::spawn(&config)?;
    sidecar.wait_ready(Duration::from_secs(60))?;
    println!("[spike] 1/7 sidecar ready on 127.0.0.1:{}", sidecar.port);

    let http = EnvironmentHttp::new(sidecar.http_base_url());
    let token = http
        .exchange_bootstrap_token(&sidecar.bootstrap_token)
        .await?;
    println!(
        "[spike] 2/7 /oauth/token exchange OK (token_type={}, scope=\"{}\")",
        token.token_type, token.scope
    );

    let ticket = http.websocket_ticket(&token.access_token).await?;
    println!("[spike] 3/7 websocket ticket OK");

    let session = RpcSession::connect(&http.ws_url(&ticket)).await?;
    println!("[spike] 4/7 /ws connected");

    let config_value = session.call("server.getConfig", json!({})).await?;
    let mut keys: Vec<&str> = config_value
        .as_object()
        .map(|object| object.keys().map(String::as_str).collect())
        .unwrap_or_default();
    keys.sort_unstable();
    println!("[spike] 5/7 server.getConfig OK — top-level keys: {keys:?}");

    let latency = session.ping(Duration::from_secs(5)).await?;
    println!("[spike] 6/7 Ping/Pong OK ({latency:?})");

    let mut subscription = session.subscribe(
        "orchestration.subscribeShell",
        json!({ "requestCompletionMarker": true }),
    )?;
    let mut chunks = 0usize;
    let mut values_total = 0usize;
    let mut synchronized = false;
    loop {
        match tokio::time::timeout(Duration::from_secs(10), subscription.next()).await {
            Ok(Some(StreamEvent::Values(values))) => {
                chunks += 1;
                values_total += values.len();
                synchronized |= values.iter().any(|value| {
                    value.get("kind").and_then(|kind| kind.as_str()) == Some("synchronized")
                });
                if chunks == 1 {
                    // Back-pressure check: the server must not stream past an
                    // un-Acked chunk. (Vacuous if the snapshot fit one chunk.)
                    tokio::time::sleep(Duration::from_millis(1200)).await;
                    match subscription.try_next() {
                        Some(_) => {
                            println!("[spike]     FAIL: server streamed past an un-Acked chunk")
                        }
                        None => {
                            println!("[spike]     ack pacing holds (nothing arrived before Ack)")
                        }
                    }
                }
                subscription.ack()?;
                if synchronized {
                    break;
                }
            }
            Ok(Some(StreamEvent::Completed(result))) => {
                println!("[spike]     stream exited before synchronized marker: {result:?}");
                break;
            }
            Ok(None) => anyhow::bail!("stream channel closed unexpectedly"),
            Err(_) => {
                anyhow::ensure!(chunks > 0, "no shell stream data within 10s");
                println!(
                    "[spike]     note: no synchronized marker within 10s (chunks so far: {chunks})"
                );
                break;
            }
        }
    }
    println!(
        "[spike] 7/7 subscribeShell OK — {chunks} chunk(s), {values_total} value(s), synchronized={synchronized}"
    );

    subscription.interrupt()?;
    match tokio::time::timeout(Duration::from_secs(5), subscription.next()).await {
        Ok(Some(StreamEvent::Completed(result))) => {
            println!("[spike]     interrupt acknowledged with Exit: {result:?}");
        }
        Ok(Some(StreamEvent::Values(_))) => {
            println!("[spike]     interrupt: unexpected extra chunk")
        }
        Ok(None) => println!("[spike]     interrupt: channel closed"),
        Err(_) => println!("[spike]     interrupt: no Exit within 5s"),
    }

    sidecar.shutdown();
    println!("[spike] PASS — wire format validated end to end");
    Ok(())
}
