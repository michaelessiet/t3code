//! Port of `packages/client-runtime/src/state/threadReducer.ts`.

use std::cmp::Ordering;
use std::collections::HashSet;

use vitre_contracts::{
    NonNegativeInt, OrchestrationCheckpointStatus, OrchestrationCheckpointSummary,
    OrchestrationEvent, OrchestrationLatestTurn, OrchestrationLatestTurnState,
    OrchestrationMessage, OrchestrationMessageRole, OrchestrationProposedPlan,
    OrchestrationSessionStatus, OrchestrationThread, OrchestrationThreadActivity,
    OrchestrationThreadAdditionalRoots, OrchestrationThreadProposedPlans,
    OrchestrationThreadSettledOverride, ProviderInteractionMode, RuntimeMode,
    ThreadCreatedPayloadAdditionalRoots, ThreadId, ThreadUnsettledPayloadReason,
    TrimmedNonEmptyString, WorkspaceRootRef,
};

use crate::wire_opt::{defined2, defined3, null3, value3};

/// Result of applying one orchestration event to a thread-detail projection.
#[derive(Debug, Clone, PartialEq)]
pub enum ThreadDetailReducerResult {
    Updated(Box<OrchestrationThread>),
    Deleted,
    Unchanged,
}

/// The event-stream sequence number carried by every known event kind, or
/// `None` for forward-compat `Unknown` events. Callers enforce the
/// `sequence <= last_sequence` dedup guard (the reducer itself does not,
/// mirroring `threadReducer.ts`).
pub fn event_sequence(event: &OrchestrationEvent) -> Option<NonNegativeInt> {
    use OrchestrationEvent::*;
    match event {
        ProjectCreated { sequence, .. }
        | ProjectMetaUpdated { sequence, .. }
        | ProjectDeleted { sequence, .. }
        | ThreadCreated { sequence, .. }
        | ThreadDeleted { sequence, .. }
        | ThreadArchived { sequence, .. }
        | ThreadUnarchived { sequence, .. }
        | ThreadSettled { sequence, .. }
        | ThreadUnsettled { sequence, .. }
        | ThreadSnoozed { sequence, .. }
        | ThreadUnsnoozed { sequence, .. }
        | ThreadMetaUpdated { sequence, .. }
        | ThreadRuntimeModeSet { sequence, .. }
        | ThreadInteractionModeSet { sequence, .. }
        | ThreadMessageSent { sequence, .. }
        | ThreadTurnStartRequested { sequence, .. }
        | ThreadTurnInterruptRequested { sequence, .. }
        | ThreadApprovalResponseRequested { sequence, .. }
        | ThreadUserInputResponseRequested { sequence, .. }
        | ThreadCheckpointRevertRequested { sequence, .. }
        | ThreadReverted { sequence, .. }
        | ThreadSessionStopRequested { sequence, .. }
        | ThreadSessionSet { sequence, .. }
        | ThreadProposedPlanUpserted { sequence, .. }
        | ThreadTurnDiffCompleted { sequence, .. }
        | ThreadActivityAppended { sequence, .. } => Some(*sequence),
        Unknown(_) => None,
    }
}

/// The thread a thread-scoped event belongs to (`None` for project events and
/// `Unknown`). Used by the sync layer to route events to thread projections.
pub fn event_thread_id(event: &OrchestrationEvent) -> Option<&ThreadId> {
    use OrchestrationEvent::*;
    match event {
        ProjectCreated { .. } | ProjectMetaUpdated { .. } | ProjectDeleted { .. } | Unknown(_) => {
            None
        }
        ThreadCreated { payload, .. } => Some(&payload.thread_id),
        ThreadDeleted { payload, .. } => Some(&payload.thread_id),
        ThreadArchived { payload, .. } => Some(&payload.thread_id),
        ThreadUnarchived { payload, .. } => Some(&payload.thread_id),
        ThreadSettled { payload, .. } => Some(&payload.thread_id),
        ThreadUnsettled { payload, .. } => Some(&payload.thread_id),
        ThreadSnoozed { payload, .. } => Some(&payload.thread_id),
        ThreadUnsnoozed { payload, .. } => Some(&payload.thread_id),
        ThreadMetaUpdated { payload, .. } => Some(&payload.thread_id),
        ThreadRuntimeModeSet { payload, .. } => Some(&payload.thread_id),
        ThreadInteractionModeSet { payload, .. } => Some(&payload.thread_id),
        ThreadMessageSent { payload, .. } => Some(&payload.thread_id),
        ThreadTurnStartRequested { payload, .. } => Some(&payload.thread_id),
        ThreadTurnInterruptRequested { payload, .. } => Some(&payload.thread_id),
        ThreadApprovalResponseRequested { payload, .. } => Some(&payload.thread_id),
        ThreadUserInputResponseRequested { payload, .. } => Some(&payload.thread_id),
        ThreadCheckpointRevertRequested { payload, .. } => Some(&payload.thread_id),
        ThreadReverted { payload, .. } => Some(&payload.thread_id),
        ThreadSessionStopRequested { payload, .. } => Some(&payload.thread_id),
        ThreadSessionSet { payload, .. } => Some(&payload.thread_id),
        ThreadProposedPlanUpserted { payload, .. } => Some(&payload.thread_id),
        ThreadTurnDiffCompleted { payload, .. } => Some(&payload.thread_id),
        ThreadActivityAppended { payload, .. } => Some(&payload.thread_id),
    }
}

// ── Orders (stable sorts, mirroring the effect `Order` combinators) ─────────

fn proposed_plan_order(
    a: &OrchestrationThreadProposedPlans,
    b: &OrchestrationThreadProposedPlans,
) -> Ordering {
    a.created_at
        .0
        .cmp(&b.created_at.0)
        .then_with(|| a.id.cmp(&b.id))
}

fn checkpoint_order(
    a: &OrchestrationCheckpointSummary,
    b: &OrchestrationCheckpointSummary,
) -> Ordering {
    a.checkpoint_turn_count.0.cmp(&b.checkpoint_turn_count.0)
}

fn activity_order(a: &OrchestrationThreadActivity, b: &OrchestrationThreadActivity) -> Ordering {
    let seq =
        |x: &OrchestrationThreadActivity| defined2(&x.sequence).map(|n| n.0).unwrap_or(i64::MAX);
    seq(a)
        .cmp(&seq(b))
        .then_with(|| a.created_at.0.cmp(&b.created_at.0))
        .then_with(|| a.id.0.cmp(&b.id.0))
}

/// Apply a single orchestration event to an `OrchestrationThread`, returning
/// the updated thread, a deletion signal, or `Unchanged` when the event
/// doesn't affect this thread. Pure; unrecognized event kinds are ignored
/// (forward-compatible).
pub fn apply_thread_detail_event(
    thread: &OrchestrationThread,
    event: &OrchestrationEvent,
) -> ThreadDetailReducerResult {
    use ThreadDetailReducerResult::{Deleted, Unchanged, Updated};

    match event {
        // ── Project events (irrelevant to thread detail) ────────────────
        OrchestrationEvent::ProjectCreated { .. }
        | OrchestrationEvent::ProjectMetaUpdated { .. }
        | OrchestrationEvent::ProjectDeleted { .. } => Unchanged,

        // ── Thread lifecycle ────────────────────────────────────────────
        OrchestrationEvent::ThreadCreated { payload, .. } => {
            Updated(Box::new(OrchestrationThread {
                activities: vec![],
                additional_roots: payload.additional_roots.as_ref().map(|middle| {
                    middle
                        .as_ref()
                        .map(|roots| roots.iter().map(created_root_to_thread_root).collect())
                }),
                archived_at: null3(),
                branch: payload.branch.clone(),
                checkpoints: vec![],
                created_at: payload.created_at.clone(),
                deleted_at: None,
                id: payload.thread_id.clone(),
                interaction_mode: Some(Some(
                    defined2(&payload.interaction_mode)
                        .cloned()
                        .unwrap_or(ProviderInteractionMode::Default),
                )),
                latest_turn: None,
                messages: vec![],
                model_selection: payload.model_selection.clone(),
                project_id: payload.project_id.clone(),
                proposed_plans: Some(Some(vec![])),
                resolved_additional_roots: None,
                runtime_mode: defined2(&payload.runtime_mode)
                    .cloned()
                    .unwrap_or(RuntimeMode::FullAccess),
                session: None,
                settled_at: null3(),
                settled_override: null3(),
                snoozed_at: null3(),
                snoozed_until: null3(),
                title: payload.title.clone(),
                updated_at: payload.updated_at.clone(),
                worktree_path: payload.worktree_path.clone(),
            }))
        }

        OrchestrationEvent::ThreadDeleted { .. } => Deleted,

        OrchestrationEvent::ThreadArchived { payload, .. } => {
            let mut t = thread.clone();
            t.archived_at = value3(payload.archived_at.clone());
            t.updated_at = payload.updated_at.clone();
            Updated(Box::new(t))
        }

        OrchestrationEvent::ThreadUnarchived { payload, .. } => {
            let mut t = thread.clone();
            t.archived_at = null3();
            t.updated_at = payload.updated_at.clone();
            Updated(Box::new(t))
        }

        OrchestrationEvent::ThreadSettled { payload, .. } => {
            let mut t = thread.clone();
            t.settled_override = value3(OrchestrationThreadSettledOverride::Settled);
            t.settled_at = value3(payload.settled_at.clone());
            t.updated_at = payload.updated_at.clone();
            Updated(Box::new(t))
        }

        OrchestrationEvent::ThreadUnsettled { payload, .. } => {
            let mut t = thread.clone();
            t.settled_override = if payload.reason == ThreadUnsettledPayloadReason::User {
                value3(OrchestrationThreadSettledOverride::Active)
            } else {
                null3()
            };
            t.settled_at = null3();
            t.updated_at = payload.updated_at.clone();
            Updated(Box::new(t))
        }

        OrchestrationEvent::ThreadSnoozed { payload, .. } => {
            let mut t = thread.clone();
            t.snoozed_until = value3(payload.snoozed_until.clone());
            t.snoozed_at = value3(payload.snoozed_at.clone());
            t.updated_at = payload.updated_at.clone();
            Updated(Box::new(t))
        }

        OrchestrationEvent::ThreadUnsnoozed { payload, .. } => {
            let mut t = thread.clone();
            t.snoozed_until = null3();
            t.snoozed_at = null3();
            t.updated_at = payload.updated_at.clone();
            Updated(Box::new(t))
        }

        // ── Thread metadata ─────────────────────────────────────────────
        OrchestrationEvent::ThreadMetaUpdated { payload, .. } => {
            let mut t = thread.clone();
            if let Some(title) = defined2(&payload.title) {
                t.title = title.clone();
            }
            if let Some(selection) = defined2(&payload.model_selection) {
                t.model_selection = selection.clone();
            }
            if let Some(branch) = defined3(&payload.branch) {
                t.branch = branch.cloned();
            }
            if let Some(worktree_path) = defined3(&payload.worktree_path) {
                t.worktree_path = worktree_path.cloned();
            }
            if let Some(roots) = defined2(&payload.additional_roots) {
                t.additional_roots = Some(Some(
                    roots.iter().map(workspace_root_to_thread_root).collect(),
                ));
            }
            t.updated_at = payload.updated_at.clone();
            Updated(Box::new(t))
        }

        OrchestrationEvent::ThreadRuntimeModeSet { payload, .. } => {
            let mut t = thread.clone();
            t.runtime_mode = payload.runtime_mode.clone();
            t.updated_at = payload.updated_at.clone();
            Updated(Box::new(t))
        }

        OrchestrationEvent::ThreadInteractionModeSet { payload, .. } => {
            let mut t = thread.clone();
            t.interaction_mode = Some(Some(
                defined2(&payload.interaction_mode)
                    .cloned()
                    .unwrap_or(ProviderInteractionMode::Default),
            ));
            t.updated_at = payload.updated_at.clone();
            Updated(Box::new(t))
        }

        // ── Turn lifecycle ──────────────────────────────────────────────
        OrchestrationEvent::ThreadTurnStartRequested {
            payload,
            occurred_at,
            ..
        } => {
            let mut t = thread.clone();
            if let Some(selection) = defined2(&payload.model_selection) {
                t.model_selection = selection.clone();
            }
            t.runtime_mode = defined2(&payload.runtime_mode)
                .cloned()
                .unwrap_or(RuntimeMode::FullAccess);
            t.interaction_mode = Some(Some(
                defined2(&payload.interaction_mode)
                    .cloned()
                    .unwrap_or(ProviderInteractionMode::Default),
            ));
            t.updated_at = occurred_at.clone();
            Updated(Box::new(t))
        }

        OrchestrationEvent::ThreadTurnInterruptRequested {
            payload,
            occurred_at,
            ..
        } => {
            let Some(turn_id) = defined2(&payload.turn_id) else {
                return Unchanged;
            };
            let Some(latest_turn) = thread.latest_turn.as_ref() else {
                return Unchanged;
            };
            if latest_turn.turn_id != *turn_id {
                return Unchanged;
            }
            let mut t = thread.clone();
            t.latest_turn = Some(OrchestrationLatestTurn {
                state: OrchestrationLatestTurnState::Interrupted,
                started_at: latest_turn
                    .started_at
                    .clone()
                    .or_else(|| Some(payload.created_at.clone())),
                completed_at: latest_turn
                    .completed_at
                    .clone()
                    .or_else(|| Some(payload.created_at.clone())),
                ..latest_turn.clone()
            });
            t.updated_at = occurred_at.clone();
            Updated(Box::new(t))
        }

        // ── Messages ────────────────────────────────────────────────────
        OrchestrationEvent::ThreadMessageSent {
            payload,
            occurred_at,
            ..
        } => {
            let mut t = thread.clone();

            match t.messages.iter_mut().find(|m| m.id == payload.message_id) {
                Some(entry) => {
                    entry.text = if payload.streaming {
                        TrimmedNonEmptyString(format!("{}{}", entry.text.0, payload.text.0))
                    } else if !payload.text.0.is_empty() {
                        payload.text.clone()
                    } else {
                        entry.text.clone()
                    };
                    entry.streaming = payload.streaming;
                    entry.turn_id = payload.turn_id.clone();
                    if !payload.streaming {
                        entry.updated_at = payload.updated_at.clone();
                    }
                    if let Some(attachments) = defined2(&payload.attachments) {
                        entry.attachments = Some(Some(attachments.clone()));
                    }
                }
                None => t.messages.push(OrchestrationMessage {
                    attachments: defined2(&payload.attachments)
                        .map(|attachments| Some(attachments.clone())),
                    created_at: payload.created_at.clone(),
                    id: payload.message_id.clone(),
                    role: payload.role.clone(),
                    streaming: payload.streaming,
                    text: payload.text.clone(),
                    turn_id: payload.turn_id.clone(),
                    updated_at: payload.updated_at.clone(),
                }),
            }

            // A completed assistant message only settles the turn once the
            // session is no longer running it — providers may emit several
            // assistant messages per turn (commentary between tool calls).
            let turn_still_running = payload.turn_id.is_some()
                && thread.session.as_ref().is_some_and(|session| {
                    session.status == OrchestrationSessionStatus::Running
                        && session.active_turn_id == payload.turn_id
                });
            let settles_turn = !payload.streaming && !turn_still_running;

            if payload.role == OrchestrationMessageRole::Assistant
                && let Some(turn_id) = payload.turn_id.as_ref()
                && thread
                    .latest_turn
                    .as_ref()
                    .is_none_or(|latest| latest.turn_id == *turn_id)
            {
                let prev = thread.latest_turn.as_ref();
                let prev_matches = prev.is_some_and(|latest| latest.turn_id == *turn_id);
                t.latest_turn = Some(OrchestrationLatestTurn {
                    assistant_message_id: Some(payload.message_id.clone()),
                    completed_at: if settles_turn {
                        Some(payload.updated_at.clone())
                    } else if prev_matches {
                        prev.and_then(|latest| latest.completed_at.clone())
                    } else {
                        None
                    },
                    requested_at: if prev_matches {
                        prev.map(|latest| latest.requested_at.clone()).unwrap()
                    } else {
                        payload.created_at.clone()
                    },
                    source_proposed_plan: None,
                    started_at: if prev_matches {
                        prev.and_then(|latest| latest.started_at.clone())
                            .or_else(|| Some(payload.created_at.clone()))
                    } else {
                        Some(payload.created_at.clone())
                    },
                    state: if settles_turn {
                        match prev.map(|latest| &latest.state) {
                            Some(OrchestrationLatestTurnState::Interrupted) => {
                                OrchestrationLatestTurnState::Interrupted
                            }
                            Some(OrchestrationLatestTurnState::Error) => {
                                OrchestrationLatestTurnState::Error
                            }
                            _ => OrchestrationLatestTurnState::Completed,
                        }
                    } else {
                        OrchestrationLatestTurnState::Running
                    },
                    turn_id: turn_id.clone(),
                });
            }

            // Rebind checkpoint assistant message IDs for assistant messages.
            if payload.role == OrchestrationMessageRole::Assistant
                && let Some(turn_id) = payload.turn_id.as_ref()
            {
                for checkpoint in t.checkpoints.iter_mut() {
                    if checkpoint.turn_id == *turn_id {
                        checkpoint.assistant_message_id = Some(payload.message_id.clone());
                    }
                }
            }

            t.updated_at = occurred_at.clone();
            Updated(Box::new(t))
        }

        // ── Session ─────────────────────────────────────────────────────
        OrchestrationEvent::ThreadSessionSet {
            payload,
            occurred_at,
            ..
        } => {
            let session = &payload.session;
            // Leaving the "running" session status is the turn-end signal:
            // settle a still-running latest turn so its duration reflects the
            // whole turn.
            let settled_turn_state = settled_turn_state_for_session_status(&session.status);
            let latest_turn: Option<OrchestrationLatestTurn> = if session.status
                == OrchestrationSessionStatus::Running
                && session.active_turn_id.is_some()
            {
                let turn_id = session.active_turn_id.clone().unwrap();
                let prev = thread.latest_turn.as_ref();
                let prev_matches = prev.is_some_and(|latest| latest.turn_id == turn_id);
                Some(OrchestrationLatestTurn {
                    assistant_message_id: if prev_matches {
                        prev.and_then(|latest| latest.assistant_message_id.clone())
                    } else {
                        None
                    },
                    completed_at: None,
                    requested_at: if prev_matches {
                        prev.map(|latest| latest.requested_at.clone()).unwrap()
                    } else {
                        session.updated_at.clone()
                    },
                    source_proposed_plan: None,
                    started_at: if prev_matches {
                        prev.and_then(|latest| latest.started_at.clone())
                            .or_else(|| Some(session.updated_at.clone()))
                    } else {
                        Some(session.updated_at.clone())
                    },
                    state: OrchestrationLatestTurnState::Running,
                    turn_id,
                })
            } else if let (Some(latest), Some(settled_state)) =
                (thread.latest_turn.as_ref(), settled_turn_state)
                && latest.state == OrchestrationLatestTurnState::Running
            {
                // A running turn's completedAt can only hold a mid-turn
                // placeholder checkpoint timestamp — the session leaving
                // "running" is the authoritative turn end.
                let mut settled = latest.clone();
                settled.state = settled_state;
                settled.completed_at = Some(session.updated_at.clone());
                Some(settled)
            } else {
                thread.latest_turn.clone()
            };

            let mut t = thread.clone();
            t.session = Some(session.clone());
            t.latest_turn = latest_turn;
            t.updated_at = occurred_at.clone();
            Updated(Box::new(t))
        }

        OrchestrationEvent::ThreadSessionStopRequested {
            payload,
            occurred_at,
            ..
        } => match thread.session.as_ref() {
            None => Unchanged,
            Some(session) => {
                let mut stopped = session.clone();
                stopped.status = OrchestrationSessionStatus::Stopped;
                stopped.active_turn_id = None;
                stopped.updated_at = payload.created_at.clone();
                let mut t = thread.clone();
                t.session = Some(stopped);
                t.updated_at = occurred_at.clone();
                Updated(Box::new(t))
            }
        },

        // ── Proposed plans ──────────────────────────────────────────────
        OrchestrationEvent::ThreadProposedPlanUpserted {
            payload,
            occurred_at,
            ..
        } => {
            let proposed_plan = plan_to_thread_plan(&payload.proposed_plan);
            let mut plans: Vec<OrchestrationThreadProposedPlans> = defined2(&thread.proposed_plans)
                .cloned()
                .unwrap_or_default();
            plans.retain(|entry| entry.id != proposed_plan.id);
            plans.push(proposed_plan);
            plans.sort_by(proposed_plan_order);

            let mut t = thread.clone();
            t.proposed_plans = Some(Some(plans));
            t.updated_at = occurred_at.clone();
            Updated(Box::new(t))
        }

        // ── Checkpoints / turn diffs ────────────────────────────────────
        OrchestrationEvent::ThreadTurnDiffCompleted {
            payload,
            occurred_at,
            ..
        } => {
            let checkpoint = OrchestrationCheckpointSummary {
                assistant_message_id: payload.assistant_message_id.clone(),
                checkpoint_ref: payload.checkpoint_ref.clone(),
                checkpoint_turn_count: payload.checkpoint_turn_count,
                completed_at: payload.completed_at.clone(),
                files: payload.files.clone(),
                status: payload.status.clone(),
                turn_id: payload.turn_id.clone(),
            };

            let existing = thread
                .checkpoints
                .iter()
                .find(|entry| entry.turn_id == checkpoint.turn_id);
            // Don't overwrite a non-missing checkpoint with a missing one.
            if existing.is_some_and(|entry| entry.status != OrchestrationCheckpointStatus::Missing)
                && checkpoint.status == OrchestrationCheckpointStatus::Missing
            {
                return Unchanged;
            }

            let mut checkpoints: Vec<OrchestrationCheckpointSummary> = thread
                .checkpoints
                .iter()
                .filter(|entry| entry.turn_id != checkpoint.turn_id)
                .cloned()
                .collect();
            checkpoints.push(checkpoint);
            checkpoints.sort_by(checkpoint_order);

            // Mid-turn diff updates produce placeholder checkpoints; record
            // the checkpoint, but don't settle a turn its session is still
            // running.
            let diff_turn_still_running = thread.session.as_ref().is_some_and(|session| {
                session.status == OrchestrationSessionStatus::Running
                    && session.active_turn_id.as_ref() == Some(&payload.turn_id)
            });
            let latest_turn = if !diff_turn_still_running
                && thread
                    .latest_turn
                    .as_ref()
                    .is_none_or(|latest| latest.turn_id == payload.turn_id)
            {
                let prev = thread.latest_turn.as_ref();
                Some(OrchestrationLatestTurn {
                    assistant_message_id: payload.assistant_message_id.clone(),
                    completed_at: Some(payload.completed_at.clone()),
                    requested_at: prev
                        .map(|latest| latest.requested_at.clone())
                        .unwrap_or_else(|| payload.completed_at.clone()),
                    source_proposed_plan: None,
                    started_at: prev
                        .and_then(|latest| latest.started_at.clone())
                        .or_else(|| Some(payload.completed_at.clone())),
                    state: checkpoint_status_to_turn_state(&payload.status),
                    turn_id: payload.turn_id.clone(),
                })
            } else {
                thread.latest_turn.clone()
            };

            let mut t = thread.clone();
            t.checkpoints = checkpoints;
            t.latest_turn = latest_turn;
            t.updated_at = occurred_at.clone();
            Updated(Box::new(t))
        }

        // ── Revert ──────────────────────────────────────────────────────
        OrchestrationEvent::ThreadReverted {
            payload,
            occurred_at,
            ..
        } => {
            let mut checkpoints: Vec<OrchestrationCheckpointSummary> = thread
                .checkpoints
                .iter()
                .filter(|entry| entry.checkpoint_turn_count.0 <= payload.turn_count.0)
                .cloned()
                .collect();
            checkpoints.sort_by(checkpoint_order);

            let retained_turn_ids: HashSet<String> = checkpoints
                .iter()
                .map(|entry| entry.turn_id.0.clone())
                .collect();
            let messages = retain_messages_after_revert(&thread.messages, &retained_turn_ids);
            let proposed_plans: Vec<OrchestrationThreadProposedPlans> =
                defined2(&thread.proposed_plans)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|plan| {
                        plan.turn_id
                            .as_ref()
                            .is_none_or(|turn_id| retained_turn_ids.contains(turn_id))
                    })
                    .collect();
            let activities: Vec<OrchestrationThreadActivity> = thread
                .activities
                .iter()
                .filter(|activity| {
                    activity
                        .turn_id
                        .as_ref()
                        .is_none_or(|turn_id| retained_turn_ids.contains(&turn_id.0))
                })
                .cloned()
                .collect();
            let latest_checkpoint = checkpoints.last();

            let latest_turn = latest_checkpoint.map(|checkpoint| OrchestrationLatestTurn {
                assistant_message_id: checkpoint.assistant_message_id.clone(),
                completed_at: Some(checkpoint.completed_at.clone()),
                requested_at: checkpoint.completed_at.clone(),
                source_proposed_plan: None,
                started_at: Some(checkpoint.completed_at.clone()),
                state: checkpoint_status_to_turn_state(&checkpoint.status),
                turn_id: checkpoint.turn_id.clone(),
            });

            let mut t = thread.clone();
            t.checkpoints = checkpoints;
            t.messages = messages;
            t.proposed_plans = Some(Some(proposed_plans));
            t.activities = activities;
            t.latest_turn = latest_turn;
            t.updated_at = occurred_at.clone();
            Updated(Box::new(t))
        }

        // ── Activities ──────────────────────────────────────────────────
        OrchestrationEvent::ThreadActivityAppended {
            payload,
            occurred_at,
            ..
        } => {
            let mut activities: Vec<OrchestrationThreadActivity> = thread
                .activities
                .iter()
                .filter(|activity| activity.id != payload.activity.id)
                .cloned()
                .collect();
            activities.push(payload.activity.clone());
            activities.sort_by(activity_order);

            let mut t = thread.clone();
            t.activities = activities;
            t.updated_at = occurred_at.clone();
            Updated(Box::new(t))
        }

        // ── Events that don't mutate thread state directly ──────────────
        OrchestrationEvent::ThreadApprovalResponseRequested { .. }
        | OrchestrationEvent::ThreadUserInputResponseRequested { .. }
        | OrchestrationEvent::ThreadCheckpointRevertRequested { .. } => Unchanged,

        // Forward-compatible: ignore unrecognized event types.
        OrchestrationEvent::Unknown(_) => Unchanged,
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Turn state to settle a still-running latest turn with when its session
/// leaves the "running" status, or `None` while the session is (re)starting or
/// running and the turn must stay unsettled.
fn settled_turn_state_for_session_status(
    status: &OrchestrationSessionStatus,
) -> Option<OrchestrationLatestTurnState> {
    match status {
        OrchestrationSessionStatus::Idle | OrchestrationSessionStatus::Ready => {
            Some(OrchestrationLatestTurnState::Completed)
        }
        OrchestrationSessionStatus::Error => Some(OrchestrationLatestTurnState::Error),
        OrchestrationSessionStatus::Interrupted | OrchestrationSessionStatus::Stopped => {
            Some(OrchestrationLatestTurnState::Interrupted)
        }
        OrchestrationSessionStatus::Starting
        | OrchestrationSessionStatus::Running
        | OrchestrationSessionStatus::Unknown(_) => None,
    }
}

fn checkpoint_status_to_turn_state(
    status: &OrchestrationCheckpointStatus,
) -> OrchestrationLatestTurnState {
    match status {
        OrchestrationCheckpointStatus::Ready
        | OrchestrationCheckpointStatus::Missing
        | OrchestrationCheckpointStatus::Unknown(_) => OrchestrationLatestTurnState::Completed,
        OrchestrationCheckpointStatus::Error => OrchestrationLatestTurnState::Error,
    }
}

fn retain_messages_after_revert(
    messages: &[OrchestrationMessage],
    retained_turn_ids: &HashSet<String>,
) -> Vec<OrchestrationMessage> {
    // Keep messages that belong to a retained turn, plus system messages and
    // messages without a turn binding (pre-turn-0 user messages).
    messages
        .iter()
        .filter(|message| {
            message.role == OrchestrationMessageRole::System
                || message
                    .turn_id
                    .as_ref()
                    .is_none_or(|turn_id| retained_turn_ids.contains(&turn_id.0))
        })
        .cloned()
        .collect()
}

// The thread's inline element types are structurally identical to the shared
// payload types but were synthesized under distinct names by the contracts
// codegen; these convert between them.

fn created_root_to_thread_root(
    root: &ThreadCreatedPayloadAdditionalRoots,
) -> OrchestrationThreadAdditionalRoots {
    match root {
        ThreadCreatedPayloadAdditionalRoots::Project { project_id } => {
            OrchestrationThreadAdditionalRoots::Project {
                project_id: project_id.clone(),
            }
        }
        ThreadCreatedPayloadAdditionalRoots::Path { path } => {
            OrchestrationThreadAdditionalRoots::Path { path: path.clone() }
        }
        ThreadCreatedPayloadAdditionalRoots::Unknown(value) => {
            OrchestrationThreadAdditionalRoots::Unknown(value.clone())
        }
    }
}

fn workspace_root_to_thread_root(root: &WorkspaceRootRef) -> OrchestrationThreadAdditionalRoots {
    match root {
        WorkspaceRootRef::Project { project_id } => OrchestrationThreadAdditionalRoots::Project {
            project_id: project_id.0.clone(),
        },
        WorkspaceRootRef::Path { path } => OrchestrationThreadAdditionalRoots::Path {
            path: path.0.clone(),
        },
        WorkspaceRootRef::Unknown(value) => {
            OrchestrationThreadAdditionalRoots::Unknown(value.clone())
        }
    }
}

fn plan_to_thread_plan(plan: &OrchestrationProposedPlan) -> OrchestrationThreadProposedPlans {
    OrchestrationThreadProposedPlans {
        created_at: plan.created_at.clone(),
        id: plan.id.0.clone(),
        implementation_thread_id: plan.implementation_thread_id.clone(),
        implemented_at: plan.implemented_at.clone(),
        plan_markdown: plan.plan_markdown.0.clone(),
        turn_id: plan.turn_id.as_ref().map(|turn_id| turn_id.0.clone()),
        updated_at: plan.updated_at.clone(),
    }
}
