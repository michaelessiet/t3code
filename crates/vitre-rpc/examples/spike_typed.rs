//! M1 live check: the typed RPC layer, HTTP snapshot loaders, and the
//! reconnect supervisor against a real sidecar — including a forced sidecar
//! restart with a `ShellProjection` resume (`subscribeDynamic` semantics).
//!
//! Run from the repo root (after `pnpm --filter t3 build:bundle`):
//!   cargo run -p vitre-rpc --example spike_typed
//! Env: T3_SERVER_ENTRY, VITRE_NODE, VITRE_SPIKE_HOME override the defaults.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use vitre_contracts::methods::{OrchestrationSubscribeShell, ServerGetConfig};
use vitre_rpc::supervisor::PrepareFn;
use vitre_rpc::{
    EnvironmentHttp, EnvironmentSupervisor, PreparedTarget, SessionHandle, TypedStreamEvent,
    wait_for_session,
};
use vitre_sidecar::{BackendInfo, Sidecar, SidecarConfig};
use vitre_state::{ShellApplyOutcome, ShellProjection};

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
                std::env::temp_dir().join(format!("vitre-spike-typed-{}", std::process::id()))
            }),
        fixed_port: None,
    };

    println!("[spike] home: {}", config.t3_home.display());
    let mut sidecar = Sidecar::spawn(&config)?;
    sidecar.wait_ready(Duration::from_secs(60))?;
    println!("[spike] 1/6 sidecar ready on 127.0.0.1:{}", sidecar.port);

    // The prepare closure re-reads this slot on every attempt, so a restarted
    // sidecar (new bootstrap token) is picked up without rebuilding anything.
    let backend: Arc<Mutex<BackendInfo>> = Arc::new(Mutex::new(sidecar.info()));
    let prepare: PrepareFn = Box::new({
        let backend = backend.clone();
        move || {
            let info = backend.lock().unwrap().clone();
            Box::pin(async move {
                let http = EnvironmentHttp::new(info.http_base_url());
                let token = http.exchange_bootstrap_token(&info.bootstrap_token).await?;
                Ok(PreparedTarget {
                    base_url: info.http_base_url(),
                    access_token: token.access_token,
                })
            })
        }
    });

    let supervisor = EnvironmentSupervisor::spawn(prepare);
    let mut sessions = supervisor.sessions();
    let handle = tokio::time::timeout(Duration::from_secs(30), wait_for_session(&mut sessions))
        .await?
        .expect("supervisor alive");
    println!(
        "[spike] 2/6 supervisor connected (generation {})",
        handle.generation
    );

    let config_value = handle
        .session
        .call_typed::<ServerGetConfig>(&json!({}))
        .await
        .map_err(|error| anyhow::anyhow!("typed getConfig failed: {error:?}"))?;
    println!(
        "[spike] 3/6 typed server.getConfig OK — cwd {}, {} editors, shellResumeCompletionMarker={:?}",
        config_value.cwd.0,
        config_value.available_editors.len(),
        config_value.shell_resume_completion_marker,
    );

    let mut projection = ShellProjection::new();
    let sequence = seed_and_synchronize(&handle, &mut projection).await?;
    println!(
        "[spike] 4/6 shell snapshot over HTTP + typed subscribe synchronized (sequence {sequence})"
    );

    // Force a full restart: same home + port, fresh bootstrap token. The old
    // session's keepalive dies; retry_now skips the backoff.
    let port = sidecar.port;
    sidecar.shutdown();
    let mut restarted = Sidecar::spawn(&SidecarConfig {
        fixed_port: Some(port),
        node_binary: config.node_binary.clone(),
        server_entry: config.server_entry.clone(),
        t3_home: config.t3_home.clone(),
    })?;
    restarted.wait_ready(Duration::from_secs(60))?;
    *backend.lock().unwrap() = restarted.info();
    supervisor.retry_now();
    println!("[spike] 5/6 sidecar restarted on port {port}");

    let handle = tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            match wait_for_session(&mut sessions).await {
                Some(next) if next.generation >= 2 => return Some(next),
                Some(_) => {
                    if sessions.changed().await.is_err() {
                        return None;
                    }
                }
                None => return None,
            }
        }
    })
    .await?
    .expect("supervisor alive");
    let sequence = seed_and_synchronize(&handle, &mut projection).await?;
    println!(
        "[spike] 6/6 reconnected (generation {}) and re-synchronized (sequence {sequence})",
        handle.generation
    );

    restarted.shutdown();
    println!("[spike] PASS — typed layer + supervisor + projection resume validated");
    Ok(())
}

/// The shell resume recipe from `shell.ts` `makeSubscribeInput`: fetch the
/// HTTP snapshot, apply it, then subscribe from its sequence and drain until
/// the completion marker.
async fn seed_and_synchronize(
    handle: &SessionHandle,
    projection: &mut ShellProjection,
) -> anyhow::Result<i64> {
    let http = EnvironmentHttp::new(handle.target.base_url.clone());
    let snapshot = http.shell_snapshot(&handle.target.access_token).await?;
    projection.apply_snapshot(snapshot);
    projection.begin_subscription(true);

    let mut subscription = handle
        .session
        .subscribe_typed::<OrchestrationSubscribeShell>(&projection.resume_input())?;
    loop {
        let event = tokio::time::timeout(Duration::from_secs(15), subscription.next())
            .await
            .map_err(|_| anyhow::anyhow!("no synchronized marker within 15s"))?
            .ok_or_else(|| anyhow::anyhow!("stream channel closed unexpectedly"))?;
        match event {
            TypedStreamEvent::Values(items) => {
                let mut synchronized = false;
                for item in &items {
                    synchronized |= projection.apply_item(item) == ShellApplyOutcome::Synchronized;
                }
                subscription.ack()?;
                if synchronized {
                    break;
                }
            }
            TypedStreamEvent::Completed(result) => {
                anyhow::bail!("stream exited before the synchronized marker: {result:?}");
            }
        }
    }
    subscription.interrupt()?;
    let snapshot = projection.snapshot().expect("snapshot applied");
    Ok(snapshot.snapshot_sequence.0)
}
