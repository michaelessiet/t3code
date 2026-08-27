//! HTTP orchestration snapshot loaders — ports of
//! `packages/client-runtime/src/state/shellSnapshotHttp.ts` and
//! `threadSnapshotHttp.ts`. Snapshots load over HTTP (gzip-compressible,
//! keeps multi-KB bodies off the socket) instead of arriving as the WS
//! subscription's first frame; the projections resume from their sequence.
//!
//! Error policy mirrors the TS loaders at the call site, not here: any
//! failure means "fall back to the socket-embedded snapshot", so callers
//! should log `Err` and continue with `None`. A 404 for a thread is expected
//! (the socket subscription is the source of truth for existence) and is
//! returned as `Ok(None)` so callers can skip the warning.

use std::time::Duration;

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use vitre_contracts::{OrchestrationShellSnapshot, OrchestrationThreadDetailSnapshot, ThreadId};

use crate::error::RpcError;
use crate::http::EnvironmentHttp;

/// Bounded so a pathologically slow endpoint cannot block the (cheaper)
/// socket fallback for long (`DEFAULT_*_SNAPSHOT_TIMEOUT_MS` in the TS
/// loaders).
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(6);

impl EnvironmentHttp {
    /// `GET /api/orchestration/shell`: projects + thread shells.
    pub async fn shell_snapshot(
        &self,
        access_token: &str,
    ) -> Result<OrchestrationShellSnapshot, RpcError> {
        let response = self
            .client()
            .get(format!("{}/api/orchestration/shell", self.base_url))
            .bearer_auth(access_token)
            .timeout(SNAPSHOT_TIMEOUT)
            .send()
            .await?;
        Ok(crate::http::check(response).await?.json().await?)
    }

    /// `GET /api/orchestration/threads/{threadId}`: one thread's detail
    /// snapshot. `Ok(None)` on 404 — a genuinely missing thread is expected
    /// and the socket subscription will surface the deletion.
    pub async fn thread_snapshot(
        &self,
        access_token: &str,
        thread_id: &ThreadId,
    ) -> Result<Option<OrchestrationThreadDetailSnapshot>, RpcError> {
        let response = self
            .client()
            .get(format!(
                "{}/api/orchestration/threads/{}",
                self.base_url,
                utf8_percent_encode(&thread_id.0, NON_ALPHANUMERIC)
            ))
            .bearer_auth(access_token)
            .timeout(SNAPSHOT_TIMEOUT)
            .send()
            .await?;
        if response.status().as_u16() == 404 {
            return Ok(None);
        }
        Ok(Some(crate::http::check(response).await?.json().await?))
    }
}
