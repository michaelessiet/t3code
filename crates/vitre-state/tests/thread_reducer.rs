//! Mirror of `packages/client-runtime/src/state/threadReducer.test.ts`, plus
//! extra pins for the wire-Option semantics the TS type system hides
//! (absent vs `null` on optional/nullable payload fields).

use serde_json::json;
use vitre_contracts::{
    OrchestrationCheckpointStatus, OrchestrationEvent, OrchestrationLatestTurnState,
    OrchestrationSessionStatus, OrchestrationThread, OrchestrationThreadSettledOverride,
    ProviderInteractionMode, RuntimeMode, TrimmedNonEmptyString,
};
use vitre_state::thread::{ThreadDetailReducerResult, apply_thread_detail_event};

fn base_thread_json() -> serde_json::Value {
    json!({
        "id": "thread-1",
        "projectId": "project-1",
        "title": "Test Thread",
        "modelSelection": { "instanceId": "codex", "model": "gpt-5.4" },
        "additionalRoots": [],
        "runtimeMode": "full-access",
        "interactionMode": "default",
        "branch": null,
        "worktreePath": null,
        "latestTurn": null,
        "createdAt": "2026-04-01T00:00:00.000Z",
        "updatedAt": "2026-04-01T00:00:00.000Z",
        "archivedAt": null,
        "settledOverride": null,
        "settledAt": null,
        "deletedAt": null,
        "messages": [],
        "proposedPlans": [],
        "activities": [],
        "checkpoints": [],
        "session": null,
    })
}

fn thread_from(value: serde_json::Value) -> OrchestrationThread {
    serde_json::from_value(value).expect("thread fixture decodes")
}

fn base_thread() -> OrchestrationThread {
    thread_from(base_thread_json())
}

fn event(
    kind: &str,
    sequence: i64,
    occurred_at: &str,
    aggregate_kind: &str,
    aggregate_id: &str,
    payload: serde_json::Value,
) -> OrchestrationEvent {
    serde_json::from_value(json!({
        "type": kind,
        "eventId": "event-1",
        "commandId": null,
        "causationEventId": null,
        "correlationId": null,
        "metadata": {},
        "sequence": sequence,
        "occurredAt": occurred_at,
        "aggregateKind": aggregate_kind,
        "aggregateId": aggregate_id,
        "payload": payload,
    }))
    .expect("event fixture decodes")
}

fn thread_event(
    kind: &str,
    sequence: i64,
    occurred_at: &str,
    payload: serde_json::Value,
) -> OrchestrationEvent {
    event(kind, sequence, occurred_at, "thread", "thread-1", payload)
}

fn updated(result: ThreadDetailReducerResult) -> OrchestrationThread {
    match result {
        ThreadDetailReducerResult::Updated(thread) => *thread,
        other => panic!("expected Updated, got {other:?}"),
    }
}

fn tnes(text: &str) -> TrimmedNonEmptyString {
    TrimmedNonEmptyString(text.to_owned())
}

fn message_json(
    id: &str,
    role: &str,
    text: &str,
    turn_id: serde_json::Value,
    streaming: bool,
    at: &str,
) -> serde_json::Value {
    json!({
        "id": id,
        "role": role,
        "text": text,
        "turnId": turn_id,
        "streaming": streaming,
        "createdAt": at,
        "updatedAt": at,
    })
}

fn session_json(
    status: &str,
    provider: &str,
    active_turn_id: serde_json::Value,
    at: &str,
) -> serde_json::Value {
    json!({
        "threadId": "thread-1",
        "status": status,
        "providerName": provider,
        "runtimeMode": "full-access",
        "activeTurnId": active_turn_id,
        "lastError": null,
        "updatedAt": at,
    })
}

fn checkpoint_json(
    turn_id: &str,
    turn_count: i64,
    reference: &str,
    status: &str,
    message_id: &str,
    at: &str,
) -> serde_json::Value {
    json!({
        "turnId": turn_id,
        "checkpointTurnCount": turn_count,
        "checkpointRef": reference,
        "status": status,
        "files": [],
        "assistantMessageId": message_id,
        "completedAt": at,
    })
}

// ── project events ───────────────────────────────────────────────────

#[test]
fn returns_unchanged_for_project_created() {
    let result = apply_thread_detail_event(
        &base_thread(),
        &event(
            "project.created",
            1,
            "2026-04-01T01:00:00.000Z",
            "project",
            "project-1",
            json!({
                "projectId": "project-1",
                "title": "T3 Code",
                "workspaceRoot": "/repo",
                "repositoryIdentity": null,
                "defaultModelSelection": null,
                "scripts": [],
                "createdAt": "2026-04-01T01:00:00.000Z",
                "updatedAt": "2026-04-01T01:00:00.000Z",
                "deletedAt": null,
            }),
        ),
    );
    assert_eq!(result, ThreadDetailReducerResult::Unchanged);
}

// ── thread.created ───────────────────────────────────────────────────

#[test]
fn creates_a_fresh_thread() {
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &event(
            "thread.created",
            1,
            "2026-04-01T01:00:00.000Z",
            "thread",
            "thread-2",
            json!({
                "threadId": "thread-2",
                "projectId": "project-1",
                "title": "New Thread",
                "modelSelection": { "instanceId": "codex", "model": "gpt-5.4" },
                "additionalRoots": [],
                "runtimeMode": "full-access",
                "interactionMode": "default",
                "branch": "main",
                "worktreePath": null,
                "createdAt": "2026-04-01T01:00:00.000Z",
                "updatedAt": "2026-04-01T01:00:00.000Z",
            }),
        ),
    ));

    assert_eq!(thread.id.0, "thread-2");
    assert_eq!(thread.title.0, "New Thread");
    assert_eq!(thread.branch, Some(tnes("main")));
    assert!(thread.messages.is_empty());
    assert!(thread.session.is_none());
    assert!(thread.latest_turn.is_none());
    assert_eq!(thread.archived_at, Some(None));
    assert_eq!(thread.proposed_plans, Some(Some(vec![])));
}

#[test]
fn created_applies_decoding_defaults_for_absent_modes() {
    // runtimeMode / interactionMode use `withDecodingDefault` in the TS
    // contracts; an old event without them must decode to the same defaults.
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &event(
            "thread.created",
            1,
            "2026-04-01T01:00:00.000Z",
            "thread",
            "thread-2",
            json!({
                "threadId": "thread-2",
                "projectId": "project-1",
                "title": "New Thread",
                "modelSelection": { "model": "gpt-5.4" },
                "branch": null,
                "worktreePath": null,
                "createdAt": "2026-04-01T01:00:00.000Z",
                "updatedAt": "2026-04-01T01:00:00.000Z",
            }),
        ),
    ));

    assert_eq!(thread.runtime_mode, RuntimeMode::FullAccess);
    assert_eq!(
        thread.interaction_mode,
        Some(Some(ProviderInteractionMode::Default))
    );
}

// ── thread.deleted ───────────────────────────────────────────────────

#[test]
fn returns_deleted_signal() {
    let result = apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.deleted",
            2,
            "2026-04-01T02:00:00.000Z",
            json!({ "threadId": "thread-1", "deletedAt": "2026-04-01T02:00:00.000Z" }),
        ),
    );
    assert_eq!(result, ThreadDetailReducerResult::Deleted);
}

// ── thread.archived / thread.unarchived ──────────────────────────────

#[test]
fn sets_archived_at() {
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.archived",
            3,
            "2026-04-01T03:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "archivedAt": "2026-04-01T03:00:00.000Z",
                "updatedAt": "2026-04-01T03:00:00.000Z",
            }),
        ),
    ));
    assert_eq!(
        thread.archived_at,
        Some(Some(Some(tnes("2026-04-01T03:00:00.000Z"))))
    );
    assert_eq!(thread.updated_at, tnes("2026-04-01T03:00:00.000Z"));
}

#[test]
fn clears_archived_at() {
    let mut source = base_thread_json();
    source["archivedAt"] = json!("2026-04-01T03:00:00.000Z");
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.unarchived",
            4,
            "2026-04-01T04:00:00.000Z",
            json!({ "threadId": "thread-1", "updatedAt": "2026-04-01T04:00:00.000Z" }),
        ),
    ));
    assert_eq!(thread.archived_at, Some(None));
}

// ── thread.settled / thread.unsettled ────────────────────────────────

#[test]
fn sets_the_settled_override_and_timestamp() {
    let settled_at = "2026-04-01T05:00:00.000Z";
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.settled",
            5,
            settled_at,
            json!({ "threadId": "thread-1", "settledAt": settled_at, "updatedAt": settled_at }),
        ),
    ));
    assert_eq!(
        thread.settled_override,
        Some(Some(Some(OrchestrationThreadSettledOverride::Settled)))
    );
    assert_eq!(thread.settled_at, Some(Some(Some(tnes(settled_at)))));
}

#[test]
fn unsettles_for_user_with_active_override() {
    let mut source = base_thread_json();
    source["settledOverride"] = json!("settled");
    source["settledAt"] = json!("2026-04-01T05:00:00.000Z");
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.unsettled",
            6,
            "2026-04-01T06:00:00.000Z",
            json!({ "threadId": "thread-1", "reason": "user", "updatedAt": "2026-04-01T06:00:00.000Z" }),
        ),
    ));
    assert_eq!(
        thread.settled_override,
        Some(Some(Some(OrchestrationThreadSettledOverride::Active)))
    );
    assert_eq!(thread.settled_at, Some(None));
}

#[test]
fn unsettles_for_activity_with_null_override() {
    let mut source = base_thread_json();
    source["settledOverride"] = json!("settled");
    source["settledAt"] = json!("2026-04-01T05:00:00.000Z");
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.unsettled",
            6,
            "2026-04-01T06:00:00.000Z",
            json!({ "threadId": "thread-1", "reason": "activity", "updatedAt": "2026-04-01T06:00:00.000Z" }),
        ),
    ));
    assert_eq!(thread.settled_override, Some(None));
    assert_eq!(thread.settled_at, Some(None));
}

// ── thread.snoozed / thread.unsnoozed ────────────────────────────────

#[test]
fn sets_and_clears_snooze() {
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.snoozed",
            5,
            "2026-04-01T05:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "snoozedUntil": "2026-04-02T05:00:00.000Z",
                "snoozedAt": "2026-04-01T05:00:00.000Z",
                "updatedAt": "2026-04-01T05:00:00.000Z",
            }),
        ),
    ));
    assert_eq!(
        thread.snoozed_until,
        Some(Some(Some(tnes("2026-04-02T05:00:00.000Z"))))
    );
    assert_eq!(
        thread.snoozed_at,
        Some(Some(Some(tnes("2026-04-01T05:00:00.000Z"))))
    );

    let thread = updated(apply_thread_detail_event(
        &thread,
        &thread_event(
            "thread.unsnoozed",
            6,
            "2026-04-01T06:00:00.000Z",
            json!({ "threadId": "thread-1", "reason": "user", "updatedAt": "2026-04-01T06:00:00.000Z" }),
        ),
    ));
    assert_eq!(thread.snoozed_until, Some(None));
    assert_eq!(thread.snoozed_at, Some(None));
}

// ── thread.meta-updated ──────────────────────────────────────────────

#[test]
fn patches_title_and_branch() {
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.meta-updated",
            5,
            "2026-04-01T05:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "title": "Updated Title",
                "branch": "feature/demo",
                "updatedAt": "2026-04-01T05:00:00.000Z",
            }),
        ),
    ));
    assert_eq!(thread.title.0, "Updated Title");
    assert_eq!(thread.branch, Some(tnes("feature/demo")));
    // Model selection unchanged since it wasn't in the payload.
    assert_eq!(thread.model_selection, base_thread().model_selection);
}

#[test]
fn meta_updated_clears_branch_on_explicit_null() {
    // `branch` is `optional(NullOr(...))` — wire `null` decodes to TS `null`
    // (meaningful: clear), unlike plain `optional` fields.
    let mut source = base_thread_json();
    source["branch"] = json!("main");
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.meta-updated",
            5,
            "2026-04-01T05:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "branch": null,
                "updatedAt": "2026-04-01T05:00:00.000Z",
            }),
        ),
    ));
    assert_eq!(thread.branch, None);
    assert_eq!(thread.title.0, "Test Thread");
}

#[test]
fn meta_updated_skips_title_on_wire_null() {
    // `title` is plain `optional(...)` — wire `null` decodes to TS
    // `undefined`, so a null title must NOT clobber the existing one.
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.meta-updated",
            5,
            "2026-04-01T05:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "title": null,
                "updatedAt": "2026-04-01T05:00:00.000Z",
            }),
        ),
    ));
    assert_eq!(thread.title.0, "Test Thread");
}

// ── thread.runtime-mode-set / thread.interaction-mode-set ────────────

#[test]
fn sets_runtime_and_interaction_modes() {
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.runtime-mode-set",
            5,
            "2026-04-01T05:00:00.000Z",
            json!({ "threadId": "thread-1", "runtimeMode": "approval-required", "updatedAt": "2026-04-01T05:00:00.000Z" }),
        ),
    ));
    assert_eq!(thread.runtime_mode, RuntimeMode::ApprovalRequired);

    let thread = updated(apply_thread_detail_event(
        &thread,
        &thread_event(
            "thread.interaction-mode-set",
            6,
            "2026-04-01T06:00:00.000Z",
            json!({ "threadId": "thread-1", "interactionMode": "plan", "updatedAt": "2026-04-01T06:00:00.000Z" }),
        ),
    ));
    assert_eq!(
        thread.interaction_mode,
        Some(Some(ProviderInteractionMode::Plan))
    );
}

// ── thread.turn-interrupt-requested ──────────────────────────────────

#[test]
fn interrupt_marks_matching_latest_turn() {
    let mut source = base_thread_json();
    source["latestTurn"] = json!({
        "turnId": "turn-1",
        "state": "running",
        "requestedAt": "2026-04-01T06:59:00.000Z",
        "startedAt": "2026-04-01T06:59:00.000Z",
        "completedAt": null,
        "assistantMessageId": null,
    });
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.turn-interrupt-requested",
            7,
            "2026-04-01T07:00:00.000Z",
            json!({ "threadId": "thread-1", "turnId": "turn-1", "createdAt": "2026-04-01T07:00:00.000Z" }),
        ),
    ));
    let latest = thread.latest_turn.expect("latest turn present");
    assert_eq!(latest.state, OrchestrationLatestTurnState::Interrupted);
    assert_eq!(latest.started_at, Some(tnes("2026-04-01T06:59:00.000Z")));
    assert_eq!(latest.completed_at, Some(tnes("2026-04-01T07:00:00.000Z")));
}

#[test]
fn interrupt_without_turn_id_or_mismatched_turn_is_unchanged() {
    let no_turn = apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.turn-interrupt-requested",
            7,
            "2026-04-01T07:00:00.000Z",
            json!({ "threadId": "thread-1", "createdAt": "2026-04-01T07:00:00.000Z" }),
        ),
    );
    assert_eq!(no_turn, ThreadDetailReducerResult::Unchanged);

    let mut source = base_thread_json();
    source["latestTurn"] = json!({
        "turnId": "turn-1",
        "state": "running",
        "requestedAt": "2026-04-01T06:59:00.000Z",
        "startedAt": null,
        "completedAt": null,
        "assistantMessageId": null,
    });
    let mismatched = apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.turn-interrupt-requested",
            7,
            "2026-04-01T07:00:00.000Z",
            json!({ "threadId": "thread-1", "turnId": "turn-2", "createdAt": "2026-04-01T07:00:00.000Z" }),
        ),
    );
    assert_eq!(mismatched, ThreadDetailReducerResult::Unchanged);
}

// ── thread.message-sent ──────────────────────────────────────────────

#[test]
fn appends_a_new_message() {
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.message-sent",
            6,
            "2026-04-01T06:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "messageId": "msg-1",
                "role": "user",
                "text": "Hello, world!",
                "turnId": null,
                "streaming": false,
                "createdAt": "2026-04-01T06:00:00.000Z",
                "updatedAt": "2026-04-01T06:00:00.000Z",
            }),
        ),
    ));
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.messages[0].text.0, "Hello, world!");
    assert!(thread.latest_turn.is_none());
}

#[test]
fn appends_text_for_streaming_messages() {
    let mut source = base_thread_json();
    source["messages"] = json!([message_json(
        "msg-2",
        "assistant",
        "Hello",
        json!("turn-1"),
        true,
        "2026-04-01T06:00:00.000Z",
    )]);
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.message-sent",
            7,
            "2026-04-01T06:01:00.000Z",
            json!({
                "threadId": "thread-1",
                "messageId": "msg-2",
                "role": "assistant",
                "text": ", world!",
                "turnId": "turn-1",
                "streaming": true,
                "createdAt": "2026-04-01T06:00:00.000Z",
                "updatedAt": "2026-04-01T06:01:00.000Z",
            }),
        ),
    ));
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.messages[0].text.0, "Hello, world!");
    assert!(thread.messages[0].streaming);
    // Streaming chunks must not touch updatedAt.
    assert_eq!(
        thread.messages[0].updated_at,
        tnes("2026-04-01T06:00:00.000Z")
    );
}

#[test]
fn updates_latest_turn_for_assistant_messages_with_a_turn() {
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.message-sent",
            8,
            "2026-04-01T07:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "messageId": "msg-3",
                "role": "assistant",
                "text": "Done.",
                "turnId": "turn-1",
                "streaming": false,
                "createdAt": "2026-04-01T07:00:00.000Z",
                "updatedAt": "2026-04-01T07:00:00.000Z",
            }),
        ),
    ));
    let latest = thread.latest_turn.expect("latest turn present");
    assert_eq!(latest.turn_id.0, "turn-1");
    assert_eq!(latest.state, OrchestrationLatestTurnState::Completed);
    assert_eq!(
        latest.assistant_message_id.map(|id| id.0),
        Some("msg-3".to_owned())
    );
}

#[test]
fn keeps_latest_turn_running_for_interim_assistant_messages_while_session_runs_the_turn() {
    let mut source = base_thread_json();
    source["session"] = session_json(
        "running",
        "claude",
        json!("turn-1"),
        "2026-04-01T06:59:00.000Z",
    );
    source["latestTurn"] = json!({
        "turnId": "turn-1",
        "state": "running",
        "requestedAt": "2026-04-01T06:59:00.000Z",
        "startedAt": "2026-04-01T06:59:00.000Z",
        "completedAt": null,
        "assistantMessageId": null,
    });
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.message-sent",
            8,
            "2026-04-01T07:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "messageId": "msg-3",
                "role": "assistant",
                "text": "Interim commentary between tool calls.",
                "turnId": "turn-1",
                "streaming": false,
                "createdAt": "2026-04-01T07:00:00.000Z",
                "updatedAt": "2026-04-01T07:00:00.000Z",
            }),
        ),
    ));
    let latest = thread.latest_turn.expect("latest turn present");
    assert_eq!(latest.state, OrchestrationLatestTurnState::Running);
    assert_eq!(latest.completed_at, None);
}

// ── thread.session-set ───────────────────────────────────────────────

#[test]
fn settles_a_running_latest_turn_when_the_session_leaves_the_running_status() {
    let mut source = base_thread_json();
    source["latestTurn"] = json!({
        "turnId": "turn-1",
        "state": "running",
        "requestedAt": "2026-04-01T07:00:00.000Z",
        "startedAt": "2026-04-01T07:00:00.000Z",
        "completedAt": null,
        "assistantMessageId": "msg-3",
    });
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.session-set",
            9,
            "2026-04-01T08:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "session": session_json("ready", "claude", json!(null), "2026-04-01T08:00:00.000Z"),
            }),
        ),
    ));
    let latest = thread.latest_turn.expect("latest turn present");
    assert_eq!(latest.state, OrchestrationLatestTurnState::Completed);
    assert_eq!(latest.completed_at, Some(tnes("2026-04-01T08:00:00.000Z")));
    assert_eq!(
        latest.assistant_message_id.map(|id| id.0),
        Some("msg-3".to_owned())
    );
}

#[test]
fn updates_session_and_latest_turn_for_a_running_session() {
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.session-set",
            9,
            "2026-04-01T08:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "session": session_json("running", "codex", json!("turn-1"), "2026-04-01T08:00:00.000Z"),
            }),
        ),
    ));
    assert_eq!(
        thread.session.as_ref().map(|s| s.status.clone()),
        Some(OrchestrationSessionStatus::Running)
    );
    let latest = thread.latest_turn.expect("latest turn present");
    assert_eq!(latest.turn_id.0, "turn-1");
    assert_eq!(latest.state, OrchestrationLatestTurnState::Running);
}

// ── thread.session-stop-requested ────────────────────────────────────

#[test]
fn marks_session_as_stopped() {
    let mut source = base_thread_json();
    source["session"] = session_json(
        "running",
        "codex",
        json!("turn-1"),
        "2026-04-01T08:00:00.000Z",
    );
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.session-stop-requested",
            10,
            "2026-04-01T09:00:00.000Z",
            json!({ "threadId": "thread-1", "createdAt": "2026-04-01T09:00:00.000Z" }),
        ),
    ));
    let session = thread.session.expect("session present");
    assert_eq!(session.status, OrchestrationSessionStatus::Stopped);
    assert_eq!(session.active_turn_id, None);
    assert_eq!(session.updated_at, tnes("2026-04-01T09:00:00.000Z"));
}

#[test]
fn returns_unchanged_when_no_session_exists() {
    let result = apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.session-stop-requested",
            10,
            "2026-04-01T09:00:00.000Z",
            json!({ "threadId": "thread-1", "createdAt": "2026-04-01T09:00:00.000Z" }),
        ),
    );
    assert_eq!(result, ThreadDetailReducerResult::Unchanged);
}

// ── thread.proposed-plan-upserted ────────────────────────────────────

#[test]
fn adds_a_proposed_plan() {
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.proposed-plan-upserted",
            11,
            "2026-04-01T10:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "proposedPlan": {
                    "id": "plan-1",
                    "turnId": "turn-1",
                    "planMarkdown": "## Plan\n- Do stuff",
                    "implementedAt": null,
                    "implementationThreadId": null,
                    "createdAt": "2026-04-01T10:00:00.000Z",
                    "updatedAt": "2026-04-01T10:00:00.000Z",
                },
            }),
        ),
    ));
    let plans = thread.proposed_plans.flatten().expect("plans present");
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].id, "plan-1");
}

// ── thread.activity-appended ─────────────────────────────────────────

#[test]
fn adds_an_activity() {
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.activity-appended",
            12,
            "2026-04-01T11:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "activity": {
                    "id": "activity-1",
                    "tone": "tool",
                    "kind": "file-edit",
                    "summary": "Edited src/index.ts",
                    "payload": {},
                    "turnId": "turn-1",
                    "createdAt": "2026-04-01T11:00:00.000Z",
                },
            }),
        ),
    ));
    assert_eq!(thread.activities.len(), 1);
    assert_eq!(thread.activities[0].kind.0, "file-edit");
}

#[test]
fn preserves_the_complete_activity_history_when_live_events_arrive() {
    let existing: Vec<serde_json::Value> = (0..129)
        .map(|index| {
            json!({
                "id": format!("activity-{index}"),
                "tone": "tool",
                "kind": "command",
                "summary": format!("Ran command {index}"),
                "payload": {},
                "turnId": "turn-1",
                "sequence": index,
                "createdAt": "2026-04-01T11:00:00.000Z",
            })
        })
        .collect();
    let mut source = base_thread_json();
    source["activities"] = json!(existing);
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.activity-appended",
            130,
            "2026-04-01T11:01:00.000Z",
            json!({
                "threadId": "thread-1",
                "activity": {
                    "id": "activity-129",
                    "tone": "tool",
                    "kind": "command",
                    "summary": "Ran command 129",
                    "payload": {},
                    "turnId": "turn-1",
                    "sequence": 129,
                    "createdAt": "2026-04-01T11:01:00.000Z",
                },
            }),
        ),
    ));
    assert_eq!(thread.activities.len(), 130);
    assert_eq!(thread.activities[0].id.0, "activity-0");
}

// ── thread.turn-diff-completed ───────────────────────────────────────

#[test]
fn adds_a_checkpoint_and_updates_latest_turn() {
    let thread = updated(apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.turn-diff-completed",
            13,
            "2026-04-01T12:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "checkpointTurnCount": 1,
                "checkpointRef": "ref-1",
                "status": "ready",
                "files": [],
                "assistantMessageId": "msg-3",
                "completedAt": "2026-04-01T12:00:00.000Z",
            }),
        ),
    ));
    assert_eq!(thread.checkpoints.len(), 1);
    let latest = thread.latest_turn.expect("latest turn present");
    assert_eq!(latest.turn_id.0, "turn-1");
    assert_eq!(latest.state, OrchestrationLatestTurnState::Completed);
}

#[test]
fn does_not_overwrite_a_ready_checkpoint_with_a_missing_one() {
    let mut source = base_thread_json();
    source["checkpoints"] = json!([checkpoint_json(
        "turn-1",
        1,
        "ref-1",
        "ready",
        "msg-2",
        "2026-04-01T02:00:00.000Z",
    )]);
    let result = apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.turn-diff-completed",
            14,
            "2026-04-01T12:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "checkpointTurnCount": 1,
                "checkpointRef": "ref-1",
                "status": "missing",
                "files": [],
                "assistantMessageId": null,
                "completedAt": "2026-04-01T12:00:00.000Z",
            }),
        ),
    );
    assert_eq!(result, ThreadDetailReducerResult::Unchanged);
}

#[test]
fn records_checkpoint_without_settling_a_turn_the_session_still_runs() {
    let mut source = base_thread_json();
    source["session"] = session_json(
        "running",
        "claude",
        json!("turn-1"),
        "2026-04-01T11:59:00.000Z",
    );
    source["latestTurn"] = json!({
        "turnId": "turn-1",
        "state": "running",
        "requestedAt": "2026-04-01T11:59:00.000Z",
        "startedAt": "2026-04-01T11:59:00.000Z",
        "completedAt": null,
        "assistantMessageId": null,
    });
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.turn-diff-completed",
            14,
            "2026-04-01T12:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "checkpointTurnCount": 1,
                "checkpointRef": "ref-1",
                "status": "ready",
                "files": [],
                "assistantMessageId": null,
                "completedAt": "2026-04-01T12:00:00.000Z",
            }),
        ),
    ));
    assert_eq!(thread.checkpoints.len(), 1);
    let latest = thread.latest_turn.expect("latest turn present");
    assert_eq!(latest.state, OrchestrationLatestTurnState::Running);
    assert_eq!(latest.completed_at, None);
}

// ── thread.reverted ──────────────────────────────────────────────────

#[test]
fn filters_entities_to_retained_turns() {
    let mut source = base_thread_json();
    source["messages"] = json!([
        message_json(
            "msg-1",
            "user",
            "First",
            json!(null),
            false,
            "2026-04-01T01:00:00.000Z"
        ),
        message_json(
            "msg-2",
            "assistant",
            "Response 1",
            json!("turn-1"),
            false,
            "2026-04-01T02:00:00.000Z"
        ),
        message_json(
            "msg-3",
            "assistant",
            "Response 2",
            json!("turn-2"),
            false,
            "2026-04-01T03:00:00.000Z"
        ),
    ]);
    source["checkpoints"] = json!([
        checkpoint_json(
            "turn-1",
            1,
            "ref-1",
            "ready",
            "msg-2",
            "2026-04-01T02:00:00.000Z"
        ),
        checkpoint_json(
            "turn-2",
            2,
            "ref-2",
            "ready",
            "msg-3",
            "2026-04-01T03:00:00.000Z"
        ),
    ]);
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.reverted",
            14,
            "2026-04-01T04:00:00.000Z",
            json!({ "threadId": "thread-1", "turnCount": 1 }),
        ),
    ));
    // The turn-2 checkpoint is filtered out (turnCount 2 > revert target 1).
    assert_eq!(thread.checkpoints.len(), 1);
    assert_eq!(thread.checkpoints[0].turn_id.0, "turn-1");
    // msg-3 (turn-2) is filtered; msg-1 (no turn) and msg-2 (turn-1) remain.
    assert_eq!(thread.messages.len(), 2);
    let latest = thread.latest_turn.expect("latest turn present");
    assert_eq!(latest.turn_id.0, "turn-1");
    assert_eq!(latest.completed_at, Some(tnes("2026-04-01T02:00:00.000Z")));
}

#[test]
fn revert_below_all_checkpoints_clears_latest_turn() {
    let mut source = base_thread_json();
    source["checkpoints"] = json!([checkpoint_json(
        "turn-1",
        1,
        "ref-1",
        "ready",
        "msg-2",
        "2026-04-01T02:00:00.000Z",
    )]);
    let thread = updated(apply_thread_detail_event(
        &thread_from(source),
        &thread_event(
            "thread.reverted",
            14,
            "2026-04-01T04:00:00.000Z",
            json!({ "threadId": "thread-1", "turnCount": 0 }),
        ),
    ));
    assert!(thread.checkpoints.is_empty());
    assert!(thread.latest_turn.is_none());
}

// ── no-op / forward-compat events ────────────────────────────────────

#[test]
fn returns_unchanged_for_approval_response_requested() {
    let result = apply_thread_detail_event(
        &base_thread(),
        &thread_event(
            "thread.approval-response-requested",
            15,
            "2026-04-01T13:00:00.000Z",
            json!({
                "threadId": "thread-1",
                "requestId": "req-1",
                "decision": "approve",
                "createdAt": "2026-04-01T13:00:00.000Z",
            }),
        ),
    );
    assert_eq!(result, ThreadDetailReducerResult::Unchanged);
}

#[test]
fn returns_unchanged_for_unknown_event_types() {
    let event: OrchestrationEvent = serde_json::from_value(json!({
        "type": "thread.event-from-the-future",
        "sequence": 99,
        "occurredAt": "2026-04-01T13:00:00.000Z",
        "payload": { "threadId": "thread-1" },
    }))
    .expect("unknown event decodes");
    assert!(matches!(event, OrchestrationEvent::Unknown(_)));
    assert_eq!(
        apply_thread_detail_event(&base_thread(), &event),
        ThreadDetailReducerResult::Unchanged
    );
}

#[test]
fn checkpoint_status_enum_matches_wire_literals() {
    // Guard the status strings the reducer branches on.
    assert_eq!(
        serde_json::from_value::<OrchestrationCheckpointStatus>(json!("missing")).unwrap(),
        OrchestrationCheckpointStatus::Missing
    );
}
