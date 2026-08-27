//! Session supervisor: keeps one live [`RpcSession`] against an environment,
//! reconnecting with backoff. The connection half of
//! `packages/client-runtime/src/connection/supervisor.ts` for the local
//! driver — the multi-connection catalog and remote drivers come with M5.
//!
//! There is deliberately no transport-level retry of individual requests
//! (mirroring the TS client): a lost session simply publishes `None`, and
//! durable subscribers re-issue their subscriptions with `afterSequence`
//! resume cursors when the next session appears (`subscribeDynamic`
//! semantics — see the projections in `vitre-state`).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Notify, watch};
use vitre_contracts::ServerConfig;
use vitre_contracts::methods::ServerGetConfig;

use crate::error::RpcError;
use crate::http::EnvironmentHttp;
use crate::session::RpcSession;
use crate::typed::TypedError;

/// Consecutive-failure backoff schedule (`RETRY_DELAYS_MS` in supervisor.ts).
const RETRY_DELAYS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(16),
];
/// A connection that survived this long resets the backoff streak
/// (`BACKOFF_RESET_AFTER_MS`).
const BACKOFF_RESET_AFTER: Duration = Duration::from_secs(30);
/// Keepalive cadence (the TS session pings every 5s)…
const PING_INTERVAL: Duration = Duration::from_secs(5);
/// …with the probe timeout from supervisor.ts (`CONNECTION_PROBE_TIMEOUT`).
const PING_TIMEOUT: Duration = Duration::from_secs(15);

/// Where to connect and how to authenticate, re-resolved on every attempt so
/// a restarted sidecar (new port and/or bootstrap token) is picked up
/// automatically.
#[derive(Debug, Clone)]
pub struct PreparedTarget {
    /// e.g. `http://127.0.0.1:3773` — always 127.0.0.1, never localhost.
    pub base_url: String,
    pub access_token: String,
}

type PrepareFuture = Pin<Box<dyn Future<Output = Result<PreparedTarget, RpcError>> + Send>>;
/// Async factory invoked before each connection attempt.
pub type PrepareFn = Box<dyn FnMut() -> PrepareFuture + Send>;

/// One established session, published to consumers via a watch channel.
#[derive(Clone)]
pub struct SessionHandle {
    /// Increments per (re)connect; consumers use it to notice replacement.
    pub generation: u64,
    pub session: Arc<RpcSession>,
    /// The target the session was built from — same credentials work for the
    /// HTTP snapshot endpoints ([`EnvironmentHttp::shell_snapshot`]).
    pub target: PreparedTarget,
    /// The initial-sync `server.getConfig` (the TS client performs it before
    /// any subscription); carries the resume-completion-marker capability
    /// flags the durable subscriptions key off.
    pub config: Arc<ServerConfig>,
}

pub struct EnvironmentSupervisor {
    session_rx: watch::Receiver<Option<SessionHandle>>,
    retry_now: Arc<Notify>,
    task: tokio::task::JoinHandle<()>,
}

impl EnvironmentSupervisor {
    /// Spawn the supervision loop. It runs until the returned handle drops.
    pub fn spawn(prepare: PrepareFn) -> Self {
        let (session_tx, session_rx) = watch::channel(None);
        let retry_now = Arc::new(Notify::new());
        let task = tokio::spawn(run(prepare, session_tx, retry_now.clone()));
        Self {
            session_rx,
            retry_now,
            task,
        }
    }

    /// Current-session channel: `None` while disconnected/backing off.
    pub fn sessions(&self) -> watch::Receiver<Option<SessionHandle>> {
        self.session_rx.clone()
    }

    /// Skip any pending backoff and re-prepare immediately. Also tears down a
    /// live session — call it when the sidecar restarts under the supervisor.
    pub fn retry_now(&self) {
        self.retry_now.notify_waiters();
    }
}

impl Drop for EnvironmentSupervisor {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Wait until the channel holds a session, returning `None` only if the
/// supervisor is gone.
pub async fn wait_for_session(
    rx: &mut watch::Receiver<Option<SessionHandle>>,
) -> Option<SessionHandle> {
    loop {
        if let Some(handle) = rx.borrow_and_update().clone() {
            return Some(handle);
        }
        rx.changed().await.ok()?;
    }
}

async fn run(
    mut prepare: PrepareFn,
    session_tx: watch::Sender<Option<SessionHandle>>,
    retry_now: Arc<Notify>,
) {
    let mut failure_streak: usize = 0;
    let mut generation: u64 = 0;
    loop {
        match connect_once(&mut prepare).await {
            Ok((session, target, config)) => {
                generation += 1;
                let connected_at = Instant::now();
                let session = Arc::new(session);
                let _ = session_tx.send(Some(SessionHandle {
                    generation,
                    session: session.clone(),
                    target,
                    config: Arc::new(config),
                }));
                let teardown = keepalive(&session, &retry_now).await;
                let _ = session_tx.send(None);
                if connected_at.elapsed() >= BACKOFF_RESET_AFTER {
                    failure_streak = 0;
                } else {
                    failure_streak += 1;
                }
                if teardown == Teardown::RetryRequested {
                    // An explicit retry (e.g. sidecar restarted) skips the
                    // backoff sleep entirely.
                    failure_streak = 0;
                    continue;
                }
            }
            Err(error) => {
                eprintln!("[vitre-rpc] connection attempt failed: {error}");
                failure_streak += 1;
            }
        }
        // failure_streak == 0 here means the session outlived the reset
        // window; reconnect after the base delay rather than instantly.
        let delay = RETRY_DELAYS[failure_streak.saturating_sub(1).min(RETRY_DELAYS.len() - 1)];
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = retry_now.notified() => {}
        }
    }
}

async fn connect_once(
    prepare: &mut PrepareFn,
) -> Result<(RpcSession, PreparedTarget, ServerConfig), RpcError> {
    let target = prepare().await?;
    let http = EnvironmentHttp::new(target.base_url.clone());
    let ticket = http.websocket_ticket(&target.access_token).await?;
    let session = RpcSession::connect(&http.ws_url(&ticket)).await?;
    let config = session
        .call_typed::<ServerGetConfig>(&serde_json::Value::Object(Default::default()))
        .await
        .map_err(|error| match error {
            TypedError::Rpc(error) => error,
            TypedError::Failed(typed) => {
                RpcError::Transport(format!("server.getConfig failed: {typed:?}"))
            }
        })?;
    Ok((session, target, config))
}

#[derive(Debug, PartialEq, Eq)]
enum Teardown {
    Died,
    RetryRequested,
}

/// Ping until the session dies or a retry is requested.
async fn keepalive(session: &RpcSession, retry_now: &Notify) -> Teardown {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(PING_INTERVAL) => {}
            _ = retry_now.notified() => return Teardown::RetryRequested,
        }
        if let Err(error) = session.ping(PING_TIMEOUT).await {
            eprintln!("[vitre-rpc] keepalive failed, reconnecting: {error}");
            return Teardown::Died;
        }
    }
}
