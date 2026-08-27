//! Pure shell-projection state machine: the data half of
//! `packages/client-runtime/src/state/shell.ts` (`makeEnvironmentShellState`).
//!
//! The IO layer re-fetches the HTTP shell snapshot on every (re)subscription
//! and feeds it through [`ShellProjection::apply_snapshot`]; the resulting
//! `resume_input` then carries that snapshot's sequence as `afterSequence`.

use vitre_contracts::{
    NonNegativeInt, OrchestrationShellSnapshot, OrchestrationShellStreamItem,
    OrchestrationSubscribeShellInput,
};

use crate::shell::apply_shell_stream_event;

/// What one stream item did to the projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellApplyOutcome {
    /// Completion marker: replay caught up to live.
    Synchronized,
    /// The snapshot view refreshed (possibly by a stale-but-acknowledged
    /// event, mirroring the TS layer's unconditional re-render + persist
    /// offer); consider persisting.
    Applied,
    /// Nothing observable changed (no snapshot yet).
    Unchanged,
}

#[derive(Debug, Default)]
pub struct ShellProjection {
    snapshot: Option<OrchestrationShellSnapshot>,
    awaiting_completion: bool,
}

impl ShellProjection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Option<&OrchestrationShellSnapshot> {
        self.snapshot.as_ref()
    }

    pub fn awaiting_completion(&self) -> bool {
        self.awaiting_completion
    }

    /// Called when (re)issuing the subscription, before [`Self::resume_input`].
    pub fn begin_subscription(&mut self, supports_completion_marker: bool) {
        self.awaiting_completion = supports_completion_marker;
    }

    /// Subscription payload: resumes from the current snapshot's sequence when
    /// one is present (normally the fresh HTTP snapshot just applied).
    pub fn resume_input(&self) -> OrchestrationSubscribeShellInput {
        OrchestrationSubscribeShellInput {
            after_sequence: self
                .snapshot
                .as_ref()
                .map(|snapshot| NonNegativeInt(snapshot.snapshot_sequence.0)),
            request_completion_marker: self.awaiting_completion.then_some(true),
        }
    }

    /// Seed or replace the snapshot (HTTP loader or a full stream snapshot).
    pub fn apply_snapshot(&mut self, snapshot: OrchestrationShellSnapshot) -> ShellApplyOutcome {
        self.snapshot = Some(snapshot);
        ShellApplyOutcome::Applied
    }

    pub fn apply_item(&mut self, item: &OrchestrationShellStreamItem) -> ShellApplyOutcome {
        match item {
            OrchestrationShellStreamItem::Synchronized { .. } => {
                self.awaiting_completion = false;
                ShellApplyOutcome::Synchronized
            }
            OrchestrationShellStreamItem::Snapshot { snapshot, .. } => {
                self.apply_snapshot(snapshot.clone())
            }
            OrchestrationShellStreamItem::OrchestrationShellStreamEvent(event) => {
                match self.snapshot.as_mut() {
                    None => ShellApplyOutcome::Unchanged,
                    Some(snapshot) => {
                        apply_shell_stream_event(snapshot, event);
                        // Mirrors the TS layer: even a stale event re-renders
                        // and re-offers persistence (harmless; sliding queue).
                        ShellApplyOutcome::Applied
                    }
                }
            }
            OrchestrationShellStreamItem::Unknown(_) => match self.snapshot {
                Some(_) => ShellApplyOutcome::Applied,
                None => ShellApplyOutcome::Unchanged,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(sequence: i64) -> OrchestrationShellSnapshot {
        serde_json::from_value(json!({
            "projects": [],
            "threads": [],
            "snapshotSequence": sequence,
            "updatedAt": "2026-04-01T00:00:00.000Z",
        }))
        .expect("snapshot decodes")
    }

    fn item(value: serde_json::Value) -> OrchestrationShellStreamItem {
        serde_json::from_value(value).expect("stream item decodes")
    }

    #[test]
    fn events_before_a_snapshot_are_ignored() {
        let mut projection = ShellProjection::new();
        let outcome = projection.apply_item(&item(json!({
            "kind": "project-removed", "sequence": 5, "projectId": "project-1",
        })));
        assert_eq!(outcome, ShellApplyOutcome::Unchanged);
        assert!(projection.snapshot().is_none());
    }

    #[test]
    fn snapshot_then_event_advances_the_projection() {
        let mut projection = ShellProjection::new();
        projection.begin_subscription(true);
        assert_eq!(
            projection.apply_snapshot(snapshot(10)),
            ShellApplyOutcome::Applied
        );

        let input = projection.resume_input();
        assert_eq!(input.after_sequence.map(|n| n.0), Some(10));
        assert_eq!(input.request_completion_marker, Some(true));

        let outcome = projection.apply_item(&item(json!({
            "kind": "thread-removed", "sequence": 11, "threadId": "thread-1",
        })));
        assert_eq!(outcome, ShellApplyOutcome::Applied);
        assert_eq!(projection.snapshot().unwrap().snapshot_sequence.0, 11);

        assert_eq!(
            projection.apply_item(&item(json!({ "kind": "synchronized" }))),
            ShellApplyOutcome::Synchronized
        );
        assert!(!projection.awaiting_completion());
    }

    #[test]
    fn stale_events_still_report_applied_but_do_not_mutate() {
        let mut projection = ShellProjection::new();
        projection.apply_snapshot(snapshot(10));
        let outcome = projection.apply_item(&item(json!({
            "kind": "thread-removed", "sequence": 10, "threadId": "thread-1",
        })));
        assert_eq!(outcome, ShellApplyOutcome::Applied);
        assert_eq!(projection.snapshot().unwrap().snapshot_sequence.0, 10);
    }

    #[test]
    fn stream_snapshot_replaces_the_projection() {
        let mut projection = ShellProjection::new();
        projection.apply_snapshot(snapshot(10));
        let outcome = projection.apply_item(&item(json!({
            "kind": "snapshot",
            "snapshot": {
                "projects": [],
                "threads": [],
                "snapshotSequence": 42,
                "updatedAt": "2026-04-01T01:00:00.000Z",
            },
        })));
        assert_eq!(outcome, ShellApplyOutcome::Applied);
        assert_eq!(projection.snapshot().unwrap().snapshot_sequence.0, 42);
    }
}
