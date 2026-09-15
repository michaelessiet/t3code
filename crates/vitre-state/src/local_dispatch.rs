//! Optimistic turn dispatch state shared by the native chat surface.
//!
//! This is the data half of Electron's `useLocalDispatchState` plus its
//! optimistic-message merge. A send takes a snapshot of the last server state;
//! the local busy state survives the RPC receipt and clears only when the
//! projection proves that the server observed the command. This matters for
//! steering: a message can join the current running turn without changing any
//! turn timestamp, so the latest projected user-message id is also an ack.

use std::collections::HashSet;

use vitre_contracts::{
    MessageId, OrchestrationMessage, OrchestrationMessageRole, OrchestrationSessionStatus,
    OrchestrationThread, TrimmedNonEmptyString, TurnId,
};

#[derive(Debug, Clone, PartialEq)]
pub struct LocalDispatchSnapshot {
    pub started_at: TrimmedNonEmptyString,
    pub latest_user_message_id: Option<MessageId>,
    pub latest_turn_turn_id: Option<TurnId>,
    pub latest_turn_requested_at: Option<TrimmedNonEmptyString>,
    pub latest_turn_started_at: Option<TrimmedNonEmptyString>,
    pub latest_turn_completed_at: Option<TrimmedNonEmptyString>,
    pub session_status: Option<OrchestrationSessionStatus>,
    pub session_updated_at: Option<TrimmedNonEmptyString>,
}

impl LocalDispatchSnapshot {
    pub fn capture(thread: &OrchestrationThread, started_at: TrimmedNonEmptyString) -> Self {
        let latest_turn = thread.latest_turn.as_ref();
        let session = thread.session.as_ref();
        Self {
            started_at,
            latest_user_message_id: latest_user_message_id(&thread.messages).cloned(),
            latest_turn_turn_id: latest_turn.map(|turn| turn.turn_id.clone()),
            latest_turn_requested_at: latest_turn.map(|turn| turn.requested_at.clone()),
            latest_turn_started_at: latest_turn.and_then(|turn| turn.started_at.clone()),
            latest_turn_completed_at: latest_turn.and_then(|turn| turn.completed_at.clone()),
            session_status: session.map(|session| session.status.clone()),
            session_updated_at: session.map(|session| session.updated_at.clone()),
        }
    }

    /// Electron's `hasServerAcknowledgedLocalDispatch`.
    pub fn is_acknowledged(
        &self,
        thread: &OrchestrationThread,
        has_pending_approval: bool,
        has_pending_user_input: bool,
        has_thread_error: bool,
    ) -> bool {
        if has_pending_approval || has_pending_user_input || has_thread_error {
            return true;
        }

        let latest_turn = thread.latest_turn.as_ref();
        let session = thread.session.as_ref();
        let latest_user_message_changed =
            self.latest_user_message_id.as_ref() != latest_user_message_id(&thread.messages);
        let latest_turn_changed = self.latest_turn_turn_id.as_ref()
            != latest_turn.map(|turn| &turn.turn_id)
            || self.latest_turn_requested_at.as_ref() != latest_turn.map(|turn| &turn.requested_at)
            || self.latest_turn_started_at.as_ref()
                != latest_turn.and_then(|turn| turn.started_at.as_ref())
            || self.latest_turn_completed_at.as_ref()
                != latest_turn.and_then(|turn| turn.completed_at.as_ref());

        if session.is_some_and(|session| session.status == OrchestrationSessionStatus::Running) {
            if latest_user_message_changed {
                return true;
            }
            if !latest_turn_changed {
                return false;
            }
            let Some(latest_turn) = latest_turn.filter(|turn| turn.started_at.is_some()) else {
                return false;
            };
            if session
                .and_then(|session| session.active_turn_id.as_ref())
                .is_some_and(|active| active != &latest_turn.turn_id)
            {
                return false;
            }
            return true;
        }

        latest_turn_changed
            || self.session_status.as_ref() != session.map(|session| &session.status)
            || self.session_updated_at.as_ref() != session.map(|session| &session.updated_at)
    }
}

fn latest_user_message_id(messages: &[OrchestrationMessage]) -> Option<&MessageId> {
    messages
        .iter()
        .rev()
        .find(|message| message.role == OrchestrationMessageRole::User)
        .map(|message| &message.id)
}

/// Server messages plus local user messages whose ids have not appeared in
/// the projection yet. The id chosen for the command is therefore also the
/// handoff key, with no timing heuristic or duplicate bubble.
pub fn merge_optimistic_messages(
    server_messages: &[OrchestrationMessage],
    optimistic_messages: &[OrchestrationMessage],
) -> Vec<OrchestrationMessage> {
    if optimistic_messages.is_empty() {
        return server_messages.to_vec();
    }
    let server_ids: HashSet<&MessageId> =
        server_messages.iter().map(|message| &message.id).collect();
    let mut messages = Vec::with_capacity(server_messages.len() + optimistic_messages.len());
    messages.extend_from_slice(server_messages);
    messages.extend(
        optimistic_messages
            .iter()
            .filter(|message| !server_ids.contains(&message.id))
            .cloned(),
    );
    messages
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn thread(value: serde_json::Value) -> OrchestrationThread {
        serde_json::from_value(value).expect("valid thread fixture")
    }

    fn base_thread() -> OrchestrationThread {
        thread(json!({
            "activities": [],
            "additionalRoots": [],
            "archivedAt": null,
            "branch": "main",
            "checkpoints": [],
            "createdAt": "2026-09-12T00:00:00.000Z",
            "deletedAt": null,
            "id": "thread-1",
            "interactionMode": "default",
            "latestTurn": {
                "assistantMessageId": "assistant-1",
                "completedAt": "2026-09-12T00:00:30.000Z",
                "requestedAt": "2026-09-12T00:00:00.000Z",
                "startedAt": "2026-09-12T00:00:01.000Z",
                "state": "completed",
                "turnId": "turn-1"
            },
            "messages": [{
                "attachments": [],
                "createdAt": "2026-09-12T00:00:00.000Z",
                "id": "message-1",
                "role": "user",
                "streaming": false,
                "text": "hello",
                "turnId": "turn-1",
                "updatedAt": "2026-09-12T00:00:00.000Z"
            }],
            "modelSelection": { "model": "gpt-5" },
            "projectId": "project-1",
            "proposedPlans": [],
            "resolvedAdditionalRoots": [],
            "runtimeMode": "full-access",
            "session": {
                "activeTurnId": null,
                "lastError": null,
                "providerInstanceId": null,
                "providerName": "Codex",
                "runtimeMode": "full-access",
                "status": "ready",
                "threadId": "thread-1",
                "updatedAt": "2026-09-12T00:00:30.000Z"
            },
            "settledAt": null,
            "settledOverride": null,
            "snoozedAt": null,
            "snoozedUntil": null,
            "title": "Thread",
            "updatedAt": "2026-09-12T00:00:30.000Z",
            "worktreePath": null
        }))
    }

    fn capture(thread: &OrchestrationThread) -> LocalDispatchSnapshot {
        LocalDispatchSnapshot::capture(thread, TrimmedNonEmptyString("now".into()))
    }

    #[test]
    fn unchanged_server_state_is_not_an_ack() {
        let thread = base_thread();
        assert!(!capture(&thread).is_acknowledged(&thread, false, false, false));
    }

    #[test]
    fn a_new_matching_running_turn_is_an_ack() {
        let before = base_thread();
        let snapshot = capture(&before);
        let mut after = before.clone();
        let latest = after.latest_turn.as_mut().unwrap();
        latest.turn_id = TurnId("turn-2".into());
        latest.requested_at = TrimmedNonEmptyString("2026-09-12T00:01:00.000Z".into());
        latest.started_at = Some(TrimmedNonEmptyString("2026-09-12T00:01:01.000Z".into()));
        latest.completed_at = None;
        after.session.as_mut().unwrap().status = OrchestrationSessionStatus::Running;
        after.session.as_mut().unwrap().active_turn_id = Some(TurnId("turn-other".into()));
        assert!(!snapshot.is_acknowledged(&after, false, false, false));
        after.session.as_mut().unwrap().active_turn_id = Some(TurnId("turn-2".into()));
        assert!(snapshot.is_acknowledged(&after, false, false, false));
    }

    #[test]
    fn a_projected_steering_message_is_an_ack_without_timestamp_changes() {
        let mut before = base_thread();
        before.latest_turn.as_mut().unwrap().completed_at = None;
        before.session.as_mut().unwrap().status = OrchestrationSessionStatus::Running;
        before.session.as_mut().unwrap().active_turn_id = Some(TurnId("turn-1".into()));
        let snapshot = capture(&before);
        let mut after = before.clone();
        let mut message = after.messages[0].clone();
        message.id = MessageId("message-steer".into());
        message.text = TrimmedNonEmptyString("one more thing".into());
        after.messages.push(message);
        assert!(snapshot.is_acknowledged(&after, false, false, false));
    }

    #[test]
    fn pending_interaction_and_errors_ack_immediately() {
        let thread = base_thread();
        let snapshot = capture(&thread);
        assert!(snapshot.is_acknowledged(&thread, true, false, false));
        assert!(snapshot.is_acknowledged(&thread, false, true, false));
        assert!(snapshot.is_acknowledged(&thread, false, false, true));
    }

    #[test]
    fn optimistic_messages_hand_off_by_id() {
        let server = base_thread().messages;
        let mut pending = server[0].clone();
        pending.id = MessageId("pending".into());
        pending.text = TrimmedNonEmptyString("instant".into());
        assert_eq!(
            merge_optimistic_messages(&server, &[pending.clone()]).len(),
            2
        );

        let mut acknowledged = server.clone();
        acknowledged.push(pending);
        assert_eq!(
            merge_optimistic_messages(&acknowledged, &acknowledged[1..]).len(),
            2
        );
    }

    #[test]
    fn snapshot_keeps_the_last_user_message_not_the_last_message() {
        let mut thread = base_thread();
        let mut assistant = thread.messages[0].clone();
        assistant.id = MessageId("assistant".into());
        assistant.role = OrchestrationMessageRole::Assistant;
        thread.messages.push(assistant);
        assert_eq!(
            capture(&thread).latest_user_message_id.unwrap().0,
            "message-1"
        );
    }
}
