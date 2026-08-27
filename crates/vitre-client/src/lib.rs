//! Environment client: the IO half of the sync machinery. Combines the
//! sidecar supervisor (`vitre-sidecar`), the RPC session supervisor
//! (`vitre-rpc`), and the pure projections (`vitre-state`) into long-running
//! tokio tasks that publish render-ready state over watch channels — the
//! Rust counterpart of `packages/client-runtime/src/state/{shell,threads}.ts`
//! with the UI (gpui) kept out of this crate entirely.
//!
//! Durable-subscription semantics (`subscribeDynamic` in the TS client):
//! a lost transport is not retried at the request level — the loop waits for
//! the supervisor to publish the next session, then re-issues the
//! subscription with the projection's `afterSequence` resume cursor.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use vitre_contracts::methods::{
    OrchestrationDispatchCommand, OrchestrationDispatchCommandErrorX, OrchestrationSubscribeShell,
    OrchestrationSubscribeThread,
};
use vitre_contracts::support::RpcMethod;
use vitre_contracts::{
    ClientOrchestrationCommand, DispatchResult, OrchestrationShellSnapshot, OrchestrationThread,
    ThreadId,
};
use vitre_rpc::supervisor::PrepareFn;
use vitre_rpc::{
    EnvironmentHttp, EnvironmentSupervisor, PreparedTarget, RpcError, SessionHandle, TypedError,
    TypedStreamEvent, TypedSubscription, wait_for_session,
};
use vitre_sidecar::SupervisorStatus;
use vitre_state::{ShellApplyOutcome, ShellProjection, ThreadApplyOutcome, ThreadProjection};

/// Where a durable subscription is in its replay→live lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncPhase {
    /// No session, or the subscription is between sessions.
    #[default]
    Disconnected,
    /// Subscribed; replaying events up to the completion marker.
    Synchronizing,
    /// Caught up to live (or the server does not support the marker).
    Live,
}

#[derive(Debug, Clone, Default)]
pub struct ShellState {
    pub snapshot: Option<Arc<OrchestrationShellSnapshot>>,
    pub phase: SyncPhase,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadState {
    /// The overlaid render view ([`ThreadProjection::view`]).
    pub view: Option<Arc<OrchestrationThread>>,
    pub phase: SyncPhase,
    pub deleted: bool,
}

/// A stream completion that was not a transport loss: the server ended the
/// subscription (typed failure or normal exit). Resubscribing immediately
/// would hot-loop, so the loops sleep this long first (the TS client's
/// `retryExpectedFailureAfter` base).
const RESUBSCRIBE_AFTER_COMPLETION: Duration = Duration::from_secs(2);

pub struct EnvironmentClient {
    supervisor: Arc<EnvironmentSupervisor>,
    shell_rx: watch::Receiver<ShellState>,
    /// Sync tasks spawn through this handle so [`Self::open_thread`] works
    /// from non-runtime threads (e.g. the gpui main thread).
    runtime: tokio::runtime::Handle,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl EnvironmentClient {
    /// Start against a supervised local sidecar. Credentials re-resolve from
    /// the sidecar's status channel on every connection attempt, so restarts
    /// (new port and/or bootstrap token) reconnect automatically.
    ///
    /// Must be called within a tokio runtime context (panics otherwise); the
    /// returned client itself may then be used from any thread.
    pub fn start(sidecar_status: watch::Receiver<SupervisorStatus>) -> Self {
        let runtime = tokio::runtime::Handle::current();
        let prepare: PrepareFn = Box::new({
            let status = sidecar_status.clone();
            move || {
                let mut status = status.clone();
                Box::pin(async move {
                    let info = loop {
                        let ready = match &*status.borrow_and_update() {
                            SupervisorStatus::Ready { info } => Some(info.clone()),
                            _ => None,
                        };
                        if let Some(info) = ready {
                            break info;
                        }
                        status.changed().await.map_err(|_| {
                            RpcError::Transport("sidecar supervisor shut down".into())
                        })?;
                    };
                    let http = EnvironmentHttp::new(info.http_base_url());
                    let token = http.exchange_bootstrap_token(&info.bootstrap_token).await?;
                    Ok(PreparedTarget {
                        base_url: info.http_base_url(),
                        access_token: token.access_token,
                    })
                })
            }
        });
        let supervisor = Arc::new(EnvironmentSupervisor::spawn(prepare));

        // Sidecar status transitions (crash, restart) invalidate the live
        // session immediately instead of waiting for keepalive to notice.
        let nudge = runtime.spawn({
            let supervisor = supervisor.clone();
            let mut status = sidecar_status;
            async move {
                loop {
                    if status.changed().await.is_err() {
                        return;
                    }
                    supervisor.retry_now();
                }
            }
        });

        let (shell_tx, shell_rx) = watch::channel(ShellState::default());
        let shell = runtime.spawn(run_shell(supervisor.sessions(), shell_tx));

        Self {
            supervisor,
            shell_rx,
            runtime,
            tasks: vec![nudge, shell],
        }
    }

    /// Current-session channel (`None` while disconnected/backing off).
    pub fn sessions(&self) -> watch::Receiver<Option<SessionHandle>> {
        self.supervisor.sessions()
    }

    /// The environment shell (projects + thread shells), kept synchronized.
    pub fn shell(&self) -> watch::Receiver<ShellState> {
        self.shell_rx.clone()
    }

    /// Open a durable subscription to one thread. Dropping the handle ends it.
    pub fn open_thread(&self, thread_id: ThreadId) -> ThreadHandle {
        let (tx, rx) = watch::channel(ThreadState::default());
        let task = self
            .runtime
            .spawn(run_thread(self.supervisor.sessions(), thread_id, tx));
        ThreadHandle { state: rx, task }
    }

    /// Dispatch an orchestration command on the current session.
    pub async fn dispatch(
        &self,
        command: &ClientOrchestrationCommand,
    ) -> Result<DispatchResult, TypedError<OrchestrationDispatchCommandErrorX>> {
        let handle = self.supervisor.sessions().borrow().clone();
        let Some(handle) = handle else {
            return Err(TypedError::Rpc(RpcError::ConnectionClosed));
        };
        handle
            .session
            .call_typed::<OrchestrationDispatchCommand>(command)
            .await
    }

    /// Call any typed (non-stream) RPC method on the current session.
    pub async fn call<M: RpcMethod>(
        &self,
        payload: &M::Payload,
    ) -> Result<M::Success, TypedError<M::Error>> {
        let handle = self.supervisor.sessions().borrow().clone();
        let Some(handle) = handle else {
            return Err(TypedError::Rpc(RpcError::ConnectionClosed));
        };
        handle.session.call_typed::<M>(payload).await
    }
}

impl Drop for EnvironmentClient {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

pub struct ThreadHandle {
    state: watch::Receiver<ThreadState>,
    task: tokio::task::JoinHandle<()>,
}

impl ThreadHandle {
    pub fn state(&self) -> watch::Receiver<ThreadState> {
        self.state.clone()
    }
}

impl Drop for ThreadHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Why one subscription attempt ended.
enum SubscriptionEnd {
    /// Transport loss or session replacement: wait for the next session.
    SessionLost,
    /// The server completed the stream (typed failure or normal exit).
    Completed,
}

/// Drive one subscription until it ends, applying every value through
/// `apply`. Selects on the session channel so a replaced/dropped session
/// unblocks the loop even though the dead subscription would never yield
/// (its router entry is gone once the reader task exits).
async fn drain<M, F>(
    subscription: &mut TypedSubscription<M>,
    sessions: &mut watch::Receiver<Option<SessionHandle>>,
    generation: u64,
    mut apply: F,
) -> SubscriptionEnd
where
    M: RpcMethod,
    F: FnMut(M::Success),
{
    loop {
        tokio::select! {
            event = subscription.next() => match event {
                Some(TypedStreamEvent::Values(values)) => {
                    for value in values {
                        apply(value);
                    }
                    if subscription.ack().is_err() {
                        return SubscriptionEnd::SessionLost;
                    }
                }
                Some(TypedStreamEvent::Completed(result)) => {
                    return match result {
                        Err(error) if error.is_transport() => SubscriptionEnd::SessionLost,
                        Err(error) => {
                            eprintln!("[vitre-client] {} completed with error: {error:?}", M::TAG);
                            SubscriptionEnd::Completed
                        }
                        Ok(()) => SubscriptionEnd::Completed,
                    };
                }
                None => return SubscriptionEnd::SessionLost,
            },
            changed = sessions.changed() => {
                if changed.is_err() {
                    return SubscriptionEnd::SessionLost;
                }
                let replaced = sessions
                    .borrow()
                    .as_ref()
                    .is_none_or(|handle| handle.generation != generation);
                if replaced {
                    return SubscriptionEnd::SessionLost;
                }
            }
        }
    }
}

/// The shell sync loop (`shell.ts` `makeEnvironmentShellState`): on every
/// (re)subscription, re-fetch the HTTP snapshot and resume from its sequence.
async fn run_shell(
    mut sessions: watch::Receiver<Option<SessionHandle>>,
    state_tx: watch::Sender<ShellState>,
) {
    let mut projection = ShellProjection::new();
    loop {
        let Some(handle) = wait_for_session(&mut sessions).await else {
            return;
        };
        let http = EnvironmentHttp::new(handle.target.base_url.clone());
        match http.shell_snapshot(&handle.target.access_token).await {
            Ok(snapshot) => {
                projection.apply_snapshot(snapshot);
            }
            // Fall back to the socket-embedded snapshot (no afterSequence →
            // the server sends a fresh one as the first stream item).
            Err(error) => {
                eprintln!("[vitre-client] shell snapshot over HTTP failed: {error}");
            }
        }
        projection.begin_subscription(handle.config.shell_resume_completion_marker == Some(true));
        publish_shell(&state_tx, &projection);

        let mut subscription = match handle
            .session
            .subscribe_typed::<OrchestrationSubscribeShell>(&projection.resume_input())
        {
            Ok(subscription) => subscription,
            // Send failed: the published session is already dead. Wait for
            // the supervisor to replace it instead of spinning on it.
            Err(_) => {
                if sessions.changed().await.is_err() {
                    return;
                }
                continue;
            }
        };
        let end = drain(
            &mut subscription,
            &mut sessions,
            handle.generation,
            |item| {
                if projection.apply_item(&item) != ShellApplyOutcome::Unchanged {
                    publish_shell(&state_tx, &projection);
                }
            },
        )
        .await;
        state_tx.send_modify(|state| state.phase = SyncPhase::Disconnected);
        if matches!(end, SubscriptionEnd::Completed) {
            tokio::time::sleep(RESUBSCRIBE_AFTER_COMPLETION).await;
        }
    }
}

fn publish_shell(state_tx: &watch::Sender<ShellState>, projection: &ShellProjection) {
    let _ = state_tx.send(ShellState {
        snapshot: projection.snapshot().cloned().map(Arc::new),
        phase: if projection.awaiting_completion() {
            SyncPhase::Synchronizing
        } else {
            SyncPhase::Live
        },
    });
}

/// The thread sync loop (`threads.ts` subscribe flow, minus the disk cache):
/// seed once over HTTP, then resume via `afterSequence` on every session.
async fn run_thread(
    mut sessions: watch::Receiver<Option<SessionHandle>>,
    thread_id: ThreadId,
    state_tx: watch::Sender<ThreadState>,
) {
    let mut projection = ThreadProjection::new();
    loop {
        let Some(handle) = wait_for_session(&mut sessions).await else {
            return;
        };
        if projection.persisted().is_none() && !projection.is_deleted() {
            let http = EnvironmentHttp::new(handle.target.base_url.clone());
            match http
                .thread_snapshot(&handle.target.access_token, &thread_id)
                .await
            {
                Ok(Some(snapshot)) => projection.seed(&snapshot),
                // 404: the socket subscription is the source of truth for
                // existence; it sends a snapshot (or the deletion) itself.
                Ok(None) => {}
                Err(error) => {
                    eprintln!("[vitre-client] thread snapshot over HTTP failed: {error}");
                }
            }
        }
        projection.begin_subscription(handle.config.thread_resume_completion_marker == Some(true));
        publish_thread(&state_tx, &projection);

        let mut subscription = match handle
            .session
            .subscribe_typed::<OrchestrationSubscribeThread>(&projection.resume_input(&thread_id))
        {
            Ok(subscription) => subscription,
            // Send failed: the published session is already dead. Wait for
            // the supervisor to replace it instead of spinning on it.
            Err(_) => {
                if sessions.changed().await.is_err() {
                    return;
                }
                continue;
            }
        };
        let end = drain(
            &mut subscription,
            &mut sessions,
            handle.generation,
            |item| {
                if projection.apply_item(&item) != ThreadApplyOutcome::Unchanged {
                    publish_thread(&state_tx, &projection);
                }
            },
        )
        .await;
        state_tx.send_modify(|state| state.phase = SyncPhase::Disconnected);
        if matches!(end, SubscriptionEnd::Completed) {
            tokio::time::sleep(RESUBSCRIBE_AFTER_COMPLETION).await;
        }
    }
}

fn publish_thread(state_tx: &watch::Sender<ThreadState>, projection: &ThreadProjection) {
    let _ = state_tx.send(ThreadState {
        view: projection.view().map(Arc::new),
        phase: if projection.awaiting_completion() {
            SyncPhase::Synchronizing
        } else {
            SyncPhase::Live
        },
        deleted: projection.is_deleted(),
    });
}
