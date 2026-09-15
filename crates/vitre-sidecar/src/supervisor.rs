//! Restart-with-backoff supervision, run on a dedicated std thread for the
//! app's lifetime. Port and bootstrap token are picked once and survive
//! restarts, so clients can keep dialing the same address.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::watch;

use crate::spawn::{
    BackendInfo, SidecarConfig, pick_port, random_hex_token, spawn_backend, wait_until_ready,
};

const READINESS_TIMEOUT: Duration = Duration::from_secs(60);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(unix)]
static SIGNAL_CHILD_PGID: AtomicU32 = AtomicU32::new(0);
#[cfg(unix)]
static SIGNAL_FORWARDING_INSTALLED: AtomicBool = AtomicBool::new(false);

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
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Supervisor {
    pub fn start(config: SidecarConfig) -> Self {
        let (status_tx, status_rx) = watch::channel(SupervisorStatus::Idle);
        let shutting_down = Arc::new(AtomicBool::new(false));
        let child_pid = Arc::new(AtomicU32::new(0));

        let thread_flags = (shutting_down.clone(), child_pid.clone());
        #[cfg(unix)]
        install_signal_forwarding();

        let thread = std::thread::Builder::new()
            .name("vitre-sidecar-supervisor".into())
            .spawn(move || run_loop(config, status_tx, thread_flags.0, thread_flags.1))
            .expect("spawn supervisor thread");

        Self {
            status_rx,
            shutting_down,
            child_pid,
            thread: Mutex::new(Some(thread)),
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
        let pid = self.child_pid.load(Ordering::SeqCst);
        if pid != 0 {
            #[cfg(unix)]
            {
                signal_process_group(pid, libc::SIGTERM);
                let deadline = std::time::Instant::now() + SHUTDOWN_TIMEOUT;
                while self.child_pid.load(Ordering::SeqCst) != 0
                    && std::time::Instant::now() < deadline
                {
                    std::thread::sleep(Duration::from_millis(50));
                }
                if self.child_pid.load(Ordering::SeqCst) != 0 {
                    signal_process_group(pid, libc::SIGKILL);
                }
            }
        }
        if let Some(thread) = self.thread.lock().expect("supervisor thread lock").take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(unix)]
fn signal_process_group(pgid: u32, signal: i32) {
    if pgid != 0 {
        unsafe {
            libc::kill(-(pgid as i32), signal);
        }
    }
}

/// GPUI's `on_app_quit` is skipped when the process receives SIGTERM. Forward
/// terminal signals from an async-signal-safe handler before ending the app.
#[cfg(unix)]
fn install_signal_forwarding() {
    if SIGNAL_FORWARDING_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    unsafe {
        libc::signal(
            libc::SIGTERM,
            forward_termination_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            forward_termination_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGHUP,
            forward_termination_signal as *const () as libc::sighandler_t,
        );
    }
}

#[cfg(unix)]
extern "C" fn forward_termination_signal(signal: i32) {
    let pgid = SIGNAL_CHILD_PGID.load(Ordering::SeqCst);
    if pgid != 0 {
        unsafe {
            libc::kill(-(pgid as i32), signal);
        }
    }
    unsafe {
        libc::_exit(128 + signal);
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
                if sleep_backoff(backoff_ms, &shutting_down) {
                    backoff_ms = (backoff_ms * 2).min(10_000);
                    continue;
                }
                let _ = status_tx.send(SupervisorStatus::Stopped {
                    reason: "shutdown".into(),
                });
                return;
            }
        };
        child_pid.store(child.id(), Ordering::SeqCst);
        #[cfg(unix)]
        SIGNAL_CHILD_PGID.store(child.id(), Ordering::SeqCst);

        if wait_until_ready(port, &mut child, READINESS_TIMEOUT) {
            backoff_ms = 500;
            eprintln!("[vitre] backend ready at http://127.0.0.1:{port}");
            let _ = status_tx.send(SupervisorStatus::Ready { info: info.clone() });
        } else {
            eprintln!("[vitre] backend did not become ready within the timeout");
        }

        let status = child.wait();
        child_pid.store(0, Ordering::SeqCst);
        #[cfg(unix)]
        SIGNAL_CHILD_PGID.store(0, Ordering::SeqCst);
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
        if !sleep_backoff(backoff_ms, &shutting_down) {
            let _ = status_tx.send(SupervisorStatus::Stopped {
                reason: "shutdown".into(),
            });
            return;
        }
        backoff_ms = (backoff_ms * 2).min(10_000);
    }
}

fn sleep_backoff(delay_ms: u64, shutting_down: &AtomicBool) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_millis(delay_ms);
    while std::time::Instant::now() < deadline {
        if shutting_down.load(Ordering::SeqCst) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_stops_as_soon_as_shutdown_is_requested() {
        let shutting_down = AtomicBool::new(true);
        let started = std::time::Instant::now();
        assert!(!sleep_backoff(10_000, &shutting_down));
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[cfg(unix)]
    #[test]
    fn termination_reaches_the_sidecar_process_group() {
        use std::os::unix::process::CommandExt as _;
        use std::process::Command;

        let mut child = Command::new("sh")
            .args(["-c", "trap 'exit 0' TERM; while :; do sleep 1; done"])
            .process_group(0)
            .spawn()
            .expect("spawn process-group leader");
        signal_process_group(child.id(), libc::SIGTERM);

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if child.try_wait().expect("poll child").is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("process-group leader survived SIGTERM");
    }
}
