//! Node sidecar lifecycle for Vitre: spawn with the stdin bootstrap envelope,
//! readiness polling, and a restart-with-backoff supervisor. Ported from the
//! Tauri experiment's `backend.rs`
//! (`feat/desktop-tauri-m2:apps/desktop-tauri/src-tauri/src/backend.rs`).

mod spawn;
mod supervisor;

pub use spawn::{BackendInfo, Sidecar, SidecarConfig};
pub use supervisor::{Supervisor, SupervisorStatus};
