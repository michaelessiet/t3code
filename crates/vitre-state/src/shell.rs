//! Port of `packages/client-runtime/src/state/shellReducer.ts`.

use vitre_contracts::{OrchestrationShellSnapshot, OrchestrationShellStreamEvent};

/// Reduce a single shell stream event into the snapshot in place. Returns
/// `true` when the event was applied (and `snapshot_sequence` advanced), or
/// `false` when it was a stale duplicate (`sequence <= snapshot_sequence`) or
/// an unrecognized event kind (forward-compatible).
///
/// Upserts replace an existing entry at its current position (no re-sort);
/// new entries are appended — presentation order is the caller's concern.
pub fn apply_shell_stream_event(
    snapshot: &mut OrchestrationShellSnapshot,
    event: &OrchestrationShellStreamEvent,
) -> bool {
    let sequence = match event {
        OrchestrationShellStreamEvent::ProjectUpserted { sequence, .. }
        | OrchestrationShellStreamEvent::ProjectRemoved { sequence, .. }
        | OrchestrationShellStreamEvent::ThreadUpserted { sequence, .. }
        | OrchestrationShellStreamEvent::ThreadRemoved { sequence, .. } => *sequence,
        OrchestrationShellStreamEvent::Unknown(_) => return false,
    };
    if sequence.0 <= snapshot.snapshot_sequence.0 {
        return false;
    }

    match event {
        OrchestrationShellStreamEvent::ProjectUpserted { project, .. } => {
            match snapshot.projects.iter_mut().find(|p| p.id == project.id) {
                Some(existing) => *existing = project.clone(),
                None => snapshot.projects.push(project.clone()),
            }
        }
        OrchestrationShellStreamEvent::ProjectRemoved { project_id, .. } => {
            snapshot.projects.retain(|p| p.id != *project_id);
        }
        OrchestrationShellStreamEvent::ThreadUpserted { thread, .. } => {
            match snapshot.threads.iter_mut().find(|t| t.id == thread.id) {
                Some(existing) => *existing = thread.clone(),
                None => snapshot.threads.push(thread.clone()),
            }
        }
        OrchestrationShellStreamEvent::ThreadRemoved { thread_id, .. } => {
            snapshot.threads.retain(|t| t.id != *thread_id);
        }
        OrchestrationShellStreamEvent::Unknown(_) => unreachable!(),
    }
    snapshot.snapshot_sequence = sequence;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot_with(
        projects: serde_json::Value,
        threads: serde_json::Value,
    ) -> OrchestrationShellSnapshot {
        serde_json::from_value(json!({
            "projects": projects,
            "threads": threads,
            "snapshotSequence": 10,
            "updatedAt": "2026-04-01T00:00:00.000Z",
        }))
        .expect("snapshot fixture decodes")
    }

    fn project(id: &str, title: &str) -> serde_json::Value {
        json!({
            "id": id,
            "title": title,
            "workspaceRoot": "/repo",
            "defaultModelSelection": null,
            "scripts": [],
            "createdAt": "2026-04-01T00:00:00.000Z",
            "updatedAt": "2026-04-01T00:00:00.000Z",
        })
    }

    fn thread_shell(id: &str, title: &str) -> serde_json::Value {
        json!({
            "id": id,
            "projectId": "project-1",
            "title": title,
            "modelSelection": { "model": "gpt-5.4" },
            "runtimeMode": "full-access",
            "branch": null,
            "worktreePath": null,
            "latestTurn": null,
            "latestUserMessageAt": null,
            "hasActionableProposedPlan": false,
            "hasPendingApprovals": false,
            "hasPendingUserInput": false,
            "session": null,
            "createdAt": "2026-04-01T00:00:00.000Z",
            "updatedAt": "2026-04-01T00:00:00.000Z",
        })
    }

    fn event(value: serde_json::Value) -> OrchestrationShellStreamEvent {
        serde_json::from_value(value).expect("event fixture decodes")
    }

    #[test]
    fn ignores_stale_sequences() {
        let mut snapshot = snapshot_with(json!([]), json!([]));
        let applied = apply_shell_stream_event(
            &mut snapshot,
            &event(
                json!({ "kind": "project-upserted", "sequence": 10, "project": project("project-1", "T3") }),
            ),
        );
        assert!(!applied);
        assert!(snapshot.projects.is_empty());
        assert_eq!(snapshot.snapshot_sequence.0, 10);
    }

    #[test]
    fn project_upserted_appends_new_and_advances_sequence() {
        let mut snapshot = snapshot_with(json!([]), json!([]));
        let applied = apply_shell_stream_event(
            &mut snapshot,
            &event(
                json!({ "kind": "project-upserted", "sequence": 11, "project": project("project-1", "T3") }),
            ),
        );
        assert!(applied);
        assert_eq!(snapshot.projects.len(), 1);
        assert_eq!(snapshot.snapshot_sequence.0, 11);
    }

    #[test]
    fn project_upserted_replaces_in_place_without_reordering() {
        let mut snapshot = snapshot_with(
            json!([project("project-1", "One"), project("project-2", "Two")]),
            json!([]),
        );
        let applied = apply_shell_stream_event(
            &mut snapshot,
            &event(
                json!({ "kind": "project-upserted", "sequence": 11, "project": project("project-1", "Renamed") }),
            ),
        );
        assert!(applied);
        assert_eq!(snapshot.projects.len(), 2);
        assert_eq!(snapshot.projects[0].id.0, "project-1");
        assert_eq!(snapshot.projects[0].title.0, "Renamed");
        assert_eq!(snapshot.projects[1].id.0, "project-2");
    }

    #[test]
    fn project_removed_filters_by_id() {
        let mut snapshot = snapshot_with(
            json!([project("project-1", "One"), project("project-2", "Two")]),
            json!([]),
        );
        let applied = apply_shell_stream_event(
            &mut snapshot,
            &event(json!({ "kind": "project-removed", "sequence": 11, "projectId": "project-1" })),
        );
        assert!(applied);
        assert_eq!(snapshot.projects.len(), 1);
        assert_eq!(snapshot.projects[0].id.0, "project-2");
    }

    #[test]
    fn thread_upserted_appends_and_replaces() {
        let mut snapshot = snapshot_with(json!([]), json!([thread_shell("thread-1", "One")]));
        assert!(apply_shell_stream_event(
            &mut snapshot,
            &event(
                json!({ "kind": "thread-upserted", "sequence": 11, "thread": thread_shell("thread-2", "Two") })
            ),
        ));
        assert_eq!(snapshot.threads.len(), 2);
        assert!(apply_shell_stream_event(
            &mut snapshot,
            &event(
                json!({ "kind": "thread-upserted", "sequence": 12, "thread": thread_shell("thread-1", "Renamed") })
            ),
        ));
        assert_eq!(snapshot.threads.len(), 2);
        assert_eq!(snapshot.threads[0].title.0, "Renamed");
        assert_eq!(snapshot.snapshot_sequence.0, 12);
    }

    #[test]
    fn thread_removed_filters_by_id() {
        let mut snapshot = snapshot_with(json!([]), json!([thread_shell("thread-1", "One")]));
        assert!(apply_shell_stream_event(
            &mut snapshot,
            &event(json!({ "kind": "thread-removed", "sequence": 11, "threadId": "thread-1" })),
        ));
        assert!(snapshot.threads.is_empty());
    }

    #[test]
    fn unknown_event_kind_is_ignored() {
        let mut snapshot = snapshot_with(json!([]), json!([]));
        let unknown = event(json!({ "kind": "future-thing", "sequence": 99, "widget": true }));
        assert!(matches!(unknown, OrchestrationShellStreamEvent::Unknown(_)));
        assert!(!apply_shell_stream_event(&mut snapshot, &unknown));
        assert_eq!(snapshot.snapshot_sequence.0, 10);
    }
}
