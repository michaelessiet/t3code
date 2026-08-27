//! Restart-with-backoff supervision, run on a dedicated std thread for the
//! app's lifetime. Port and bootstrap token are picked once and survive
//! restarts, so clients can keep dialing the same address.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use tokio::sync::watch;

use crate::spawn::{
    BackendInfo, SidecarConfig, pick_port, random_hex_token, spawn_backend, wait_until_ready,
};

const READINESS_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub enum SupervisorStatus {
    Idle,
    /// Spawning the process / waiting for readiness. `attempt` counts from 1
    /// and resets never — it's the lifetime spawn count.
    Starting {
        attempt: u32,
    },
    Ready {
        info: BackendInfo,
    },
    /// The child died (or never became ready); sleeping before respawn.
    Backoff {
        attempt: u32,
        delay_ms: u64,
    },
    /// Unrecoverable (e.g. no free port) or deliberate shutdown.
    Stopped {
        reason: String,
    },
}

pub struct Supervisor {
    status_rx: watch::Receiver<SupervisorStatus>,
    shutting_down: Arc<AtomicBool>,
    child_pid: Arc<AtomicU32>,
}

impl Supervisor {
    pub fn start(config: SidecarConfig) -> Self {
        let (status_tx, status_rx) = watch::channel(SupervisorStatus::Idle);
        let shutting_down = Arc::new(AtomicBool::new(false));
        let child_pid = Arc::new(AtomicU32::new(0));

        let thread_flags = (shutting_down.clone(), child_pid.clone());
        std::thread::Builder::new()
            .name("vitre-sidecar-supervisor".into())
            .spawn(move || run_loop(config, status_tx, thread_flags.0, thread_flags.1))
            .expect("spawn supervisor thread");

        Self {
            status_rx,
            shutting_down,
            child_pid,
        }
    }

    /// Watch endpoint for status transitions. `Ready` re-fires after every
    /// successful restart.
    pub fn status(&self) -> watch::Receiver<SupervisorStatus> {
        self.status_rx.clone()
    }

    /// Stop supervising and SIGTERM the live child. Without this the Node
    /// server outlives the shell as an orphan holding the SQLite home open.
    pub fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let pid = self.child_pid.swap(0, Ordering::SeqCst);
        if pid != 0 {
            #[cfg(unix)]
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
    }
}

fn run_loop(
    config: SidecarConfig,
    status_tx: watch::Sender<SupervisorStatus>,
    shutting_down: Arc<AtomicBool>,
    child_pid: Arc<AtomicU32>,
) {
    let Some(port) = pick_port(config.fixed_port) else {
        let _ = status_tx.send(SupervisorStatus::Stopped {
            reason: "no free backend port from 3773 upward".into(),
        });
        return;
    };
    let bootstrap_token = random_hex_token();
    let info = BackendInfo {
        port,
        bootstrap_token: bootstrap_token.clone(),
    };

    let mut attempt: u32 = 0;
    let mut backoff_ms: u64 = 500;
    loop {
        if shutting_down.load(Ordering::SeqCst) {
            let _ = status_tx.send(SupervisorStatus::Stopped {
                reason: "shutdown".into(),
            });
            return;
        }
        attempt += 1;
        let _ = status_tx.send(SupervisorStatus::Starting { attempt });
        eprintln!("[vitre] starting backend on 127.0.0.1:{port} (attempt {attempt})");

        let mut child = match spawn_backend(&config, port, &bootstrap_token) {
            Ok(child) => child,
            Err(error) => {
                eprintln!("[vitre] failed to spawn backend: {error}");
                let _ = status_tx.send(SupervisorStatus::Backoff {
                    attempt,
                    delay_ms: backoff_ms,
                });
                std::thread::sleep(Duration::from_millis(backoff_ms));
                backoff_ms = (backoff_ms * 2).min(10_000);
                continue;
            }
        };
        child_pid.store(child.id(), Ordering::SeqCst);

        if wait_until_ready(port, &mut child, READINESS_TIMEOUT) {
            backoff_ms = 500;
            eprintln!("[vitre] backend ready at http://127.0.0.1:{port}");
            let _ = status_tx.send(SupervisorStatus::Ready { info: info.clone() });
        } else {
            eprintln!("[vitre] backend did not become ready within the timeout");
        }

        let status = child.wait();
        child_pid.store(0, Ordering::SeqCst);
        if shutting_down.load(Ordering::SeqCst) {
            let _ = status_tx.send(SupervisorStatus::Stopped {
                reason: "shutdown".into(),
            });
            return;
        }
        eprintln!("[vitre] backend exited ({status:?}); restarting in {backoff_ms}ms");
        let _ = status_tx.send(SupervisorStatus::Backoff {
            attempt,
            delay_ms: backoff_ms,
        });
        std::thread::sleep(Duration::from_millis(backoff_ms));
        backoff_ms = (backoff_ms * 2).min(10_000);
    }
}
