//! Pure thread-projection state machine: the data half of
//! `packages/client-runtime/src/state/threads.ts` (`makeEnvironmentThreadState`).
//!
//! Owns the un-overlaid persisted thread (the reducer's source of truth), the
//! ephemeral streaming-text overlays, and the `afterSequence` resume cursor.
//! IO concerns (subscription lifecycle, HTTP snapshot loading, cache
//! persistence scheduling, connection status) stay with the caller; every
//! reconciliation rule lives here so it is unit-testable.
//!
//! Invariants ported from the TS layer:
//! - Only the un-overlaid thread may be persisted — overlay text arrives again
//!   via persisted events after the cached sequence.
//! - Ephemeral deltas never advance the resume cursor and are only applied
//!   when contiguous with what is already rendered (`offset == persisted_len +
//!   overlay_len`, both in UTF-16 code units — the server counts JS string
//!   lengths).
//! - A persisted `thread.message-sent` carrying the same characters trims the
//!   flushed prefix from the overlay (streaming) or drops it (finalized).

use vitre_contracts::{
    MessageId, OrchestrationEvent, OrchestrationMessage, OrchestrationMessageRole,
    OrchestrationSessionStatus, OrchestrationSubscribeThreadInput, OrchestrationThread,
    OrchestrationThreadDetailSnapshot, OrchestrationThreadStreamItem, ThreadId,
    TrimmedNonEmptyString, TurnId,
};

use crate::thread::{ThreadDetailReducerResult, apply_thread_detail_event, event_sequence};

/// What one stream item did to the projection — tells the caller which
/// downstream effects to run (re-render, schedule persistence, mark live).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadApplyOutcome {
    /// Completion marker: replay caught up to live.
    Synchronized,
    /// The persisted thread changed; re-render and consider persisting.
    ThreadChanged,
    /// Only the ephemeral overlay changed; re-render the view.
    ViewChanged,
    /// The thread was deleted.
    Deleted,
    /// Nothing observable changed.
    Unchanged,
}

#[derive(Debug, Clone)]
struct Overlay {
    turn_id: Option<TurnId>,
    created_at: TrimmedNonEmptyString,
    text: String,
}

#[derive(Debug, Default)]
pub struct ThreadProjection {
    /// The reducer's source of truth; rendered with overlays via [`Self::view`].
    persisted: Option<OrchestrationThread>,
    deleted: bool,
    /// Insertion-ordered, keyed by message id (mirrors the TS `Map`).
    overlays: Vec<(MessageId, Overlay)>,
    last_sequence: i64,
    awaiting_completion: bool,
}

impl ThreadProjection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Warm start from a cached snapshot: resume via `afterSequence` instead
    /// of re-downloading the full thread body.
    pub fn seed(&mut self, snapshot: &OrchestrationThreadDetailSnapshot) {
        self.persisted = Some(snapshot.thread.clone());
        self.deleted = false;
        self.overlays.clear();
        self.last_sequence = snapshot.snapshot_sequence.0;
    }

    /// Called when (re)issuing the subscription, before [`Self::resume_input`].
    pub fn begin_subscription(&mut self, supports_completion_marker: bool) {
        self.awaiting_completion = supports_completion_marker;
    }

    /// Subscription payload with the resume cursor — `afterSequence` only when
    /// there is data to resume from, the completion marker only when the
    /// server advertised support (`begin_subscription`).
    pub fn resume_input(&self, thread_id: &ThreadId) -> OrchestrationSubscribeThreadInput {
        let can_resume = self.persisted.is_some() && !self.deleted;
        OrchestrationSubscribeThreadInput {
            after_sequence: can_resume
                .then_some(vitre_contracts::NonNegativeInt(self.last_sequence)),
            request_completion_marker: self.awaiting_completion.then_some(true),
            thread_id: thread_id.clone(),
        }
    }

    pub fn last_sequence(&self) -> i64 {
        self.last_sequence
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted
    }

    /// Whether a completion marker is still outstanding (view is
    /// "synchronizing" rather than "live" while true).
    pub fn awaiting_completion(&self) -> bool {
        self.awaiting_completion
    }

    /// The persisted (un-overlaid) thread. Only this may be written to a
    /// cache; the overlaid view would replay overlay text twice on resume.
    pub fn persisted(&self) -> Option<&OrchestrationThread> {
        self.persisted.as_ref()
    }

    /// Cache snapshot to persist, or `None` while the session is starting or
    /// running a turn (the server stays the source of truth mid-turn; this
    /// keeps cache encoding off the streaming path).
    pub fn persist_snapshot(&self) -> Option<OrchestrationThreadDetailSnapshot> {
        let thread = self.persisted.as_ref()?;
        let session_active = thread.session.as_ref().is_some_and(|session| {
            session.status == OrchestrationSessionStatus::Starting
                || session.status == OrchestrationSessionStatus::Running
        });
        if session_active {
            return None;
        }
        Some(OrchestrationThreadDetailSnapshot {
            snapshot_sequence: vitre_contracts::NonNegativeInt(self.last_sequence),
            thread: thread.clone(),
        })
    }

    /// The render view: the persisted thread with ephemeral overlays applied.
    pub fn view(&self) -> Option<OrchestrationThread> {
        let thread = self.persisted.as_ref()?;
        if self.deleted {
            return None;
        }
        if self.overlays.is_empty() {
            return Some(thread.clone());
        }
        let mut result = thread.clone();
        for (message_id, overlay) in &self.overlays {
            if overlay.text.is_empty() {
                continue;
            }
            match result
                .messages
                .iter_mut()
                .find(|entry| entry.id == *message_id)
            {
                Some(existing) => {
                    if !existing.streaming {
                        continue;
                    }
                    existing.text =
                        TrimmedNonEmptyString(format!("{}{}", existing.text.0, overlay.text));
                }
                None => result.messages.push(OrchestrationMessage {
                    attachments: None,
                    created_at: overlay.created_at.clone(),
                    id: message_id.clone(),
                    role: OrchestrationMessageRole::Assistant,
                    streaming: true,
                    text: TrimmedNonEmptyString(overlay.text.clone()),
                    turn_id: overlay.turn_id.clone(),
                    updated_at: overlay.created_at.clone(),
                }),
            }
        }
        Some(result)
    }

    pub fn apply_item(&mut self, item: &OrchestrationThreadStreamItem) -> ThreadApplyOutcome {
        match item {
            OrchestrationThreadStreamItem::Synchronized {} => {
                self.awaiting_completion = false;
                ThreadApplyOutcome::Synchronized
            }

            OrchestrationThreadStreamItem::Snapshot { snapshot } => {
                // The snapshot may already contain any overlaid text; drop
                // overlays rather than risk rendering them twice. The next
                // flush self-heals.
                self.overlays.clear();
                self.last_sequence = snapshot.snapshot_sequence.0;
                self.persisted = Some(snapshot.thread.clone());
                self.deleted = false;
                ThreadApplyOutcome::ThreadChanged
            }

            OrchestrationThreadStreamItem::EphemeralDelta {
                created_at,
                delta,
                message_id,
                offset,
                turn_id,
                ..
            } => {
                // Best-effort live text. Never advances last_sequence and is
                // only applied when contiguous with what we already render;
                // anything dropped here arrives again in the coalesced
                // persisted delta.
                let Some(persisted) = self.persisted.as_ref() else {
                    return ThreadApplyOutcome::Unchanged;
                };
                let persisted_message = persisted
                    .messages
                    .iter()
                    .find(|entry| entry.id == *message_id);
                if persisted_message.is_some_and(|message| !message.streaming) {
                    return ThreadApplyOutcome::Unchanged;
                }
                let persisted_length = persisted_message
                    .map(|message| utf16_len(&message.text.0))
                    .unwrap_or(0);
                let overlay = self
                    .overlays
                    .iter_mut()
                    .find(|(id, _)| id == message_id)
                    .map(|(_, overlay)| overlay);
                let overlay_length = overlay
                    .as_ref()
                    .map(|overlay| utf16_len(&overlay.text))
                    .unwrap_or(0);
                if offset.0 < 0 || offset.0 as usize != persisted_length + overlay_length {
                    return ThreadApplyOutcome::Unchanged;
                }
                match overlay {
                    Some(overlay) => {
                        overlay.turn_id = turn_id.clone();
                        overlay.text.push_str(&delta.0);
                    }
                    None => self.overlays.push((
                        message_id.clone(),
                        Overlay {
                            turn_id: turn_id.clone(),
                            created_at: created_at.clone(),
                            text: delta.0.clone(),
                        },
                    )),
                }
                ThreadApplyOutcome::ViewChanged
            }

            OrchestrationThreadStreamItem::Event { event } => self.apply_event(event),

            OrchestrationThreadStreamItem::Unknown(_) => ThreadApplyOutcome::Unchanged,
        }
    }

    fn apply_event(&mut self, event: &OrchestrationEvent) -> ThreadApplyOutcome {
        // Unknown event kinds still carry a sequence in their raw JSON; keep
        // the cursor moving so resumes do not replay them forever.
        let sequence = event_sequence(event).map(|n| n.0).or_else(|| match event {
            OrchestrationEvent::Unknown(value) => value.get("sequence").and_then(|s| s.as_i64()),
            _ => None,
        });
        let Some(sequence) = sequence else {
            return ThreadApplyOutcome::Unchanged;
        };
        if sequence <= self.last_sequence {
            return ThreadApplyOutcome::Unchanged;
        }
        self.last_sequence = sequence;

        // Reconcile overlays against the persisted delta that carries the
        // same characters: trim the flushed prefix from the overlay
        // (streaming) or drop it entirely (message finalized).
        if let OrchestrationEvent::ThreadMessageSent { payload, .. } = event
            && let Some(index) = self
                .overlays
                .iter()
                .position(|(id, _)| *id == payload.message_id)
        {
            if payload.streaming {
                let flushed_units = utf16_len(&payload.text.0);
                let remaining = skip_utf16_units(&self.overlays[index].1.text, flushed_units)
                    .unwrap_or("")
                    .to_owned();
                if remaining.is_empty() {
                    self.overlays.remove(index);
                } else {
                    self.overlays[index].1.text = remaining;
                }
            } else {
                self.overlays.remove(index);
            }
        }

        let Some(persisted) = self.persisted.as_ref() else {
            if matches!(event, OrchestrationEvent::ThreadDeleted { .. }) {
                self.mark_deleted();
                return ThreadApplyOutcome::Deleted;
            }
            return ThreadApplyOutcome::Unchanged;
        };
        match apply_thread_detail_event(persisted, event) {
            ThreadDetailReducerResult::Updated(thread) => {
                self.persisted = Some(*thread);
                ThreadApplyOutcome::ThreadChanged
            }
            ThreadDetailReducerResult::Deleted => {
                self.mark_deleted();
                ThreadApplyOutcome::Deleted
            }
            ThreadDetailReducerResult::Unchanged => ThreadApplyOutcome::Unchanged,
        }
    }

    fn mark_deleted(&mut self) {
        self.persisted = None;
        self.deleted = true;
        self.overlays.clear();
        self.awaiting_completion = false;
    }
}

/// JS `String.length` semantics: UTF-16 code units.
fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// Remainder of `text` after skipping `units` UTF-16 code units, or `None`
/// when the cut exceeds the text or lands inside a surrogate pair (treated by
/// callers as "fully flushed" — the overlay self-heals on the next delta).
fn skip_utf16_units(text: &str, units: usize) -> Option<&str> {
    if units == 0 {
        return Some(text);
    }
    let mut consumed = 0usize;
    for (byte_index, ch) in text.char_indices() {
        if consumed == units {
            return Some(&text[byte_index..]);
        }
        consumed += ch.len_utf16();
        if consumed > units {
            return None;
        }
    }
    (consumed == units).then_some("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(value: serde_json::Value) -> OrchestrationThreadStreamItem {
        serde_json::from_value(value).expect("stream item decodes")
    }

    fn snapshot_item(sequence: i64, messages: serde_json::Value) -> OrchestrationThreadStreamItem {
        item(json!({
            "kind": "snapshot",
            "snapshot": {
                "snapshotSequence": sequence,
                "thread": {
                    "id": "thread-1",
                    "projectId": "project-1",
                    "title": "Test Thread",
                    "modelSelection": { "model": "gpt-5.4" },
                    "runtimeMode": "full-access",
                    "branch": null,
                    "worktreePath": null,
                    "latestTurn": null,
                    "createdAt": "2026-04-01T00:00:00.000Z",
                    "updatedAt": "2026-04-01T00:00:00.000Z",
                    "deletedAt": null,
                    "messages": messages,
                    "proposedPlans": [],
                    "activities": [],
                    "checkpoints": [],
                    "session": null,
                },
            },
        }))
    }

    fn streaming_message(id: &str, text: &str) -> serde_json::Value {
        json!({
            "id": id,
            "role": "assistant",
            "text": text,
            "turnId": "turn-1",
            "streaming": true,
            "createdAt": "2026-04-01T00:00:00.000Z",
            "updatedAt": "2026-04-01T00:00:00.000Z",
        })
    }

    fn delta_item(message_id: &str, offset: i64, delta: &str) -> OrchestrationThreadStreamItem {
        item(json!({
            "kind": "ephemeral-delta",
            "threadId": "thread-1",
            "messageId": message_id,
            "turnId": "turn-1",
            "offset": offset,
            "delta": delta,
            "createdAt": "2026-04-01T00:01:00.000Z",
        }))
    }

    fn message_sent_item(
        sequence: i64,
        message_id: &str,
        text: &str,
        streaming: bool,
    ) -> OrchestrationThreadStreamItem {
        item(json!({
            "kind": "event",
            "event": {
                "type": "thread.message-sent",
                "eventId": "event-1",
                "commandId": null,
                "causationEventId": null,
                "correlationId": null,
                "metadata": {},
                "sequence": sequence,
                "occurredAt": "2026-04-01T00:02:00.000Z",
                "aggregateKind": "thread",
                "aggregateId": "thread-1",
                "payload": {
                    "threadId": "thread-1",
                    "messageId": message_id,
                    "role": "assistant",
                    "text": text,
                    "turnId": "turn-1",
                    "streaming": streaming,
                    "createdAt": "2026-04-01T00:02:00.000Z",
                    "updatedAt": "2026-04-01T00:02:00.000Z",
                },
            },
        }))
    }

    fn seeded(messages: serde_json::Value) -> ThreadProjection {
        let mut projection = ThreadProjection::new();
        assert_eq!(
            projection.apply_item(&snapshot_item(10, messages)),
            ThreadApplyOutcome::ThreadChanged
        );
        projection
    }

    fn view_text(projection: &ThreadProjection, message_id: &str) -> String {
        let view = projection.view().expect("view present");
        view.messages
            .iter()
            .find(|m| m.id.0 == message_id)
            .map(|m| m.text.0.clone())
            .unwrap_or_default()
    }

    #[test]
    fn delta_without_persisted_thread_is_dropped() {
        let mut projection = ThreadProjection::new();
        assert_eq!(
            projection.apply_item(&delta_item("msg-1", 0, "Hi")),
            ThreadApplyOutcome::Unchanged
        );
    }

    #[test]
    fn contiguous_delta_overlays_a_streaming_message() {
        let mut projection = seeded(json!([streaming_message("msg-1", "Hello")]));
        assert_eq!(
            projection.apply_item(&delta_item("msg-1", 5, ", wor")),
            ThreadApplyOutcome::ViewChanged
        );
        assert_eq!(view_text(&projection, "msg-1"), "Hello, wor");
        // The persisted thread stays un-overlaid.
        assert_eq!(projection.persisted().unwrap().messages[0].text.0, "Hello");
        // Second delta continues after persisted + overlay.
        assert_eq!(
            projection.apply_item(&delta_item("msg-1", 10, "ld!")),
            ThreadApplyOutcome::ViewChanged
        );
        assert_eq!(view_text(&projection, "msg-1"), "Hello, world!");
    }

    #[test]
    fn non_contiguous_delta_is_dropped() {
        let mut projection = seeded(json!([streaming_message("msg-1", "Hello")]));
        assert_eq!(
            projection.apply_item(&delta_item("msg-1", 7, "xx")),
            ThreadApplyOutcome::Unchanged
        );
        assert_eq!(view_text(&projection, "msg-1"), "Hello");
    }

    #[test]
    fn delta_on_finalized_message_is_dropped() {
        let mut projection = seeded(json!([{
            "id": "msg-1",
            "role": "assistant",
            "text": "Done.",
            "turnId": "turn-1",
            "streaming": false,
            "createdAt": "2026-04-01T00:00:00.000Z",
            "updatedAt": "2026-04-01T00:00:00.000Z",
        }]));
        assert_eq!(
            projection.apply_item(&delta_item("msg-1", 5, "more")),
            ThreadApplyOutcome::Unchanged
        );
    }

    #[test]
    fn delta_for_unknown_message_synthesizes_a_streaming_message() {
        let mut projection = seeded(json!([]));
        assert_eq!(
            projection.apply_item(&delta_item("msg-9", 0, "Fresh")),
            ThreadApplyOutcome::ViewChanged
        );
        let view = projection.view().expect("view present");
        let synthesized = view.messages.iter().find(|m| m.id.0 == "msg-9").unwrap();
        assert_eq!(synthesized.text.0, "Fresh");
        assert!(synthesized.streaming);
        assert_eq!(synthesized.role, OrchestrationMessageRole::Assistant);
        // Not part of the persisted thread.
        assert!(projection.persisted().unwrap().messages.is_empty());
    }

    #[test]
    fn deltas_use_utf16_offsets() {
        // "你好" is 2 UTF-16 units (6 UTF-8 bytes); "😀" is 2 UTF-16 units.
        let mut projection = seeded(json!([streaming_message("msg-1", "你好")]));
        assert_eq!(
            projection.apply_item(&delta_item("msg-1", 2, "😀")),
            ThreadApplyOutcome::ViewChanged
        );
        assert_eq!(view_text(&projection, "msg-1"), "你好😀");
        assert_eq!(
            projection.apply_item(&delta_item("msg-1", 4, "!")),
            ThreadApplyOutcome::ViewChanged
        );
        assert_eq!(view_text(&projection, "msg-1"), "你好😀!");
    }

    #[test]
    fn persisted_streaming_delta_trims_the_overlay_prefix() {
        let mut projection = seeded(json!([streaming_message("msg-1", "Hello")]));
        projection.apply_item(&delta_item("msg-1", 5, ", world!"));
        assert_eq!(view_text(&projection, "msg-1"), "Hello, world!");
        // The coalesced persisted delta flushes ", wor" — the overlay keeps "ld!".
        assert_eq!(
            projection.apply_item(&message_sent_item(11, "msg-1", ", wor", true)),
            ThreadApplyOutcome::ThreadChanged
        );
        assert_eq!(
            projection.persisted().unwrap().messages[0].text.0,
            "Hello, wor"
        );
        assert_eq!(view_text(&projection, "msg-1"), "Hello, world!");
    }

    #[test]
    fn finalized_message_drops_the_overlay() {
        let mut projection = seeded(json!([streaming_message("msg-1", "Hello")]));
        projection.apply_item(&delta_item("msg-1", 5, ", wor"));
        assert_eq!(
            projection.apply_item(&message_sent_item(11, "msg-1", "Hello, world!", false)),
            ThreadApplyOutcome::ThreadChanged
        );
        assert_eq!(view_text(&projection, "msg-1"), "Hello, world!");
        assert_eq!(
            projection.persisted().unwrap().messages[0].text.0,
            "Hello, world!"
        );
    }

    #[test]
    fn snapshot_clears_overlays() {
        let mut projection = seeded(json!([streaming_message("msg-1", "Hello")]));
        projection.apply_item(&delta_item("msg-1", 5, ", wor"));
        projection.apply_item(&snapshot_item(
            20,
            json!([streaming_message("msg-1", "Hello, world")]),
        ));
        assert_eq!(view_text(&projection, "msg-1"), "Hello, world");
        assert_eq!(projection.last_sequence(), 20);
    }

    #[test]
    fn stale_events_are_deduplicated() {
        let mut projection = seeded(json!([]));
        assert_eq!(
            projection.apply_item(&message_sent_item(10, "msg-1", "old", false)),
            ThreadApplyOutcome::Unchanged
        );
        assert!(projection.persisted().unwrap().messages.is_empty());
        assert_eq!(projection.last_sequence(), 10);
    }

    #[test]
    fn ephemeral_deltas_never_advance_the_cursor() {
        let mut projection = seeded(json!([streaming_message("msg-1", "Hello")]));
        projection.apply_item(&delta_item("msg-1", 5, ", wor"));
        assert_eq!(projection.last_sequence(), 10);
    }

    #[test]
    fn deleted_event_without_persisted_thread_marks_deleted() {
        let mut projection = seeded(json!([]));
        let deleted = item(json!({
            "kind": "event",
            "event": {
                "type": "thread.deleted",
                "eventId": "event-2",
                "commandId": null,
                "causationEventId": null,
                "correlationId": null,
                "metadata": {},
                "sequence": 11,
                "occurredAt": "2026-04-01T00:03:00.000Z",
                "aggregateKind": "thread",
                "aggregateId": "thread-1",
                "payload": { "threadId": "thread-1", "deletedAt": "2026-04-01T00:03:00.000Z" },
            },
        }));
        assert_eq!(projection.apply_item(&deleted), ThreadApplyOutcome::Deleted);
        assert!(projection.is_deleted());
        assert!(projection.view().is_none());
        // resume_input no longer resumes.
        let input = projection.resume_input(&ThreadId("thread-1".into()));
        assert!(input.after_sequence.is_none());
    }

    #[test]
    fn synchronized_clears_awaiting_completion() {
        let mut projection = seeded(json!([]));
        projection.begin_subscription(true);
        assert!(projection.awaiting_completion());
        let input = projection.resume_input(&ThreadId("thread-1".into()));
        assert_eq!(input.after_sequence.map(|n| n.0), Some(10));
        assert_eq!(input.request_completion_marker, Some(true));
        assert_eq!(
            projection.apply_item(&item(json!({ "kind": "synchronized" }))),
            ThreadApplyOutcome::Synchronized
        );
        assert!(!projection.awaiting_completion());
    }

    #[test]
    fn unknown_event_kinds_advance_the_cursor() {
        let mut projection = seeded(json!([]));
        let future = item(json!({
            "kind": "event",
            "event": {
                "type": "thread.event-from-the-future",
                "sequence": 15,
                "occurredAt": "2026-04-01T00:03:00.000Z",
                "payload": { "threadId": "thread-1" },
            },
        }));
        assert_eq!(
            projection.apply_item(&future),
            ThreadApplyOutcome::Unchanged
        );
        assert_eq!(projection.last_sequence(), 15);
    }

    #[test]
    fn persist_snapshot_is_gated_on_settled_sessions() {
        let mut projection = seeded(json!([]));
        assert!(projection.persist_snapshot().is_some());

        let running = item(json!({
            "kind": "event",
            "event": {
                "type": "thread.session-set",
                "eventId": "event-3",
                "commandId": null,
                "causationEventId": null,
                "correlationId": null,
                "metadata": {},
                "sequence": 12,
                "occurredAt": "2026-04-01T00:04:00.000Z",
                "aggregateKind": "thread",
                "aggregateId": "thread-1",
                "payload": {
                    "threadId": "thread-1",
                    "session": {
                        "threadId": "thread-1",
                        "status": "running",
                        "providerName": "claude",
                        "activeTurnId": "turn-1",
                        "lastError": null,
                        "updatedAt": "2026-04-01T00:04:00.000Z",
                    },
                },
            },
        }));
        assert_eq!(
            projection.apply_item(&running),
            ThreadApplyOutcome::ThreadChanged
        );
        assert!(projection.persist_snapshot().is_none());
    }

    #[test]
    fn utf16_helpers_handle_surrogate_pairs() {
        assert_eq!(utf16_len("你好"), 2);
        assert_eq!(utf16_len("😀"), 2);
        assert_eq!(skip_utf16_units("Hello", 2), Some("llo"));
        assert_eq!(skip_utf16_units("😀!", 2), Some("!"));
        // A cut inside a surrogate pair is unrepresentable.
        assert_eq!(skip_utf16_units("😀!", 1), None);
        assert_eq!(skip_utf16_units("ab", 2), Some(""));
        assert_eq!(skip_utf16_units("ab", 3), None);
    }
}
