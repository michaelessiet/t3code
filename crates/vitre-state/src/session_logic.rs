//! Chat presentation derivations over raw thread activities.
//!
//! Port of the relevant parts of `apps/web/src/session-logic.ts`: pending
//! approvals, pending user-input requests, and the active (TodoWrite) plan
//! are NOT part of the thread projection — the reducer deliberately ignores
//! their request events — so both apps derive them from the activity log.

use serde_json::Value;
use vitre_contracts::{OrchestrationThreadActivity, TurnId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalRequestKind {
    Command,
    FileRead,
    FileChange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingApproval {
    pub request_id: String,
    pub request_kind: ApprovalRequestKind,
    pub created_at: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInputOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub options: Vec<UserInputOption>,
    pub multi_select: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingUserInput {
    pub request_id: String,
    pub created_at: String,
    pub questions: Vec<UserInputQuestion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStepStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    pub step: String,
    pub status: PlanStepStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivePlanState {
    pub created_at: String,
    pub turn_id: Option<TurnId>,
    pub explanation: Option<String>,
    pub steps: Vec<PlanStep>,
}

fn payload_object(
    activity: &OrchestrationThreadActivity,
) -> Option<&serde_json::Map<String, Value>> {
    activity.payload.as_object()
}

fn payload_str<'a>(payload: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

fn request_kind_from_request_type(request_type: Option<&Value>) -> Option<ApprovalRequestKind> {
    match request_type.and_then(Value::as_str) {
        Some("command_execution_approval")
        | Some("exec_command_approval")
        | Some("dynamic_tool_call") => Some(ApprovalRequestKind::Command),
        Some("file_read_approval") => Some(ApprovalRequestKind::FileRead),
        Some("file_change_approval") | Some("apply_patch_approval") => {
            Some(ApprovalRequestKind::FileChange)
        }
        _ => None,
    }
}

fn request_kind(payload: &serde_json::Map<String, Value>) -> Option<ApprovalRequestKind> {
    match payload_str(payload, "requestKind") {
        Some("command") => Some(ApprovalRequestKind::Command),
        Some("file-read") => Some(ApprovalRequestKind::FileRead),
        Some("file-change") => Some(ApprovalRequestKind::FileChange),
        _ => request_kind_from_request_type(payload.get("requestType")),
    }
}

/// Provider "respond failed" details that mean the request is gone and must
/// be dropped from the pending list.
fn is_stale_pending_request_failure_detail(detail: Option<&str>) -> bool {
    let Some(detail) = detail else {
        return false;
    };
    let normalized = detail.to_lowercase();
    [
        "stale pending approval request",
        "stale pending user-input request",
        "unknown pending approval request",
        "unknown pending permission request",
        "unknown pending user-input request",
        "unknown pending user input request",
        "unknown pending codex user input request",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

/// Activity ordering: sequence first (present beats absent), then createdAt,
/// then lifecycle rank (`.started` < progress/updated < completed/resolved),
/// then id.
fn compare_activities_by_order(
    left: &OrchestrationThreadActivity,
    right: &OrchestrationThreadActivity,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let sequence = |activity: &OrchestrationThreadActivity| -> Option<i64> {
        match &activity.sequence {
            Some(Some(sequence)) => Some(sequence.0),
            _ => None,
        }
    };
    match (sequence(left), sequence(right)) {
        (Some(l), Some(r)) if l != r => return l.cmp(&r),
        (Some(_), Some(_)) => {}
        (Some(_), None) => return Ordering::Greater,
        (None, Some(_)) => return Ordering::Less,
        (None, None) => {}
    }
    let created = left.created_at.0.cmp(&right.created_at.0);
    if created != Ordering::Equal {
        return created;
    }
    let rank = lifecycle_rank(&left.kind.0).cmp(&lifecycle_rank(&right.kind.0));
    if rank != Ordering::Equal {
        return rank;
    }
    left.id.0.cmp(&right.id.0)
}

fn lifecycle_rank(kind: &str) -> u8 {
    if kind.ends_with(".started") || kind == "tool.started" {
        0
    } else if kind.ends_with(".completed") || kind.ends_with(".resolved") {
        2
    } else {
        1
    }
}

fn ordered(activities: &[OrchestrationThreadActivity]) -> Vec<&OrchestrationThreadActivity> {
    let mut ordered: Vec<&OrchestrationThreadActivity> = activities.iter().collect();
    ordered.sort_by(|a, b| compare_activities_by_order(a, b));
    ordered
}

pub fn derive_pending_approvals(
    activities: &[OrchestrationThreadActivity],
) -> Vec<PendingApproval> {
    // Insertion-ordered map semantics like the TS Map.
    let mut open: Vec<PendingApproval> = Vec::new();
    for activity in ordered(activities) {
        let Some(payload) = payload_object(activity) else {
            continue;
        };
        let request_id = payload_str(payload, "requestId");
        let detail = payload_str(payload, "detail");
        match activity.kind.0.as_str() {
            "approval.requested" => {
                if let (Some(request_id), Some(kind)) = (request_id, request_kind(payload)) {
                    open.retain(|entry| entry.request_id != request_id);
                    open.push(PendingApproval {
                        request_id: request_id.to_string(),
                        request_kind: kind,
                        created_at: activity.created_at.0.clone(),
                        detail: detail.map(str::to_string),
                    });
                }
            }
            "approval.resolved" => {
                if let Some(request_id) = request_id {
                    open.retain(|entry| entry.request_id != request_id);
                }
            }
            "provider.approval.respond.failed" => {
                if let Some(request_id) = request_id
                    && is_stale_pending_request_failure_detail(detail)
                {
                    open.retain(|entry| entry.request_id != request_id);
                }
            }
            _ => {}
        }
    }
    open.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    open
}

fn parse_user_input_questions(payload: &serde_json::Map<String, Value>) -> Vec<UserInputQuestion> {
    let Some(questions) = payload.get("questions").and_then(Value::as_array) else {
        return Vec::new();
    };
    questions
        .iter()
        .filter_map(|entry| {
            let question = entry.as_object()?;
            let id = payload_str(question, "id")?;
            let header = payload_str(question, "header")?;
            let text = payload_str(question, "question")?;
            let options: Vec<UserInputOption> = question
                .get("options")?
                .as_array()?
                .iter()
                .filter_map(|option| {
                    let option = option.as_object()?;
                    Some(UserInputOption {
                        label: payload_str(option, "label")?.to_string(),
                        description: payload_str(option, "description")?.to_string(),
                    })
                })
                .collect();
            if options.is_empty() {
                return None;
            }
            Some(UserInputQuestion {
                id: id.to_string(),
                header: header.to_string(),
                question: text.to_string(),
                options,
                multi_select: question.get("multiSelect") == Some(&Value::Bool(true)),
            })
        })
        .collect()
}

pub fn derive_pending_user_inputs(
    activities: &[OrchestrationThreadActivity],
) -> Vec<PendingUserInput> {
    let mut open: Vec<PendingUserInput> = Vec::new();
    for activity in ordered(activities) {
        let Some(payload) = payload_object(activity) else {
            continue;
        };
        let request_id = payload_str(payload, "requestId");
        let detail = payload_str(payload, "detail");
        match activity.kind.0.as_str() {
            "user-input.requested" => {
                if let Some(request_id) = request_id {
                    let questions = parse_user_input_questions(payload);
                    if questions.is_empty() {
                        continue;
                    }
                    open.retain(|entry| entry.request_id != request_id);
                    open.push(PendingUserInput {
                        request_id: request_id.to_string(),
                        created_at: activity.created_at.0.clone(),
                        questions,
                    });
                }
            }
            "user-input.resolved" => {
                if let Some(request_id) = request_id {
                    open.retain(|entry| entry.request_id != request_id);
                }
            }
            "provider.user-input.respond.failed" => {
                if let Some(request_id) = request_id
                    && is_stale_pending_request_failure_detail(detail)
                {
                    open.retain(|entry| entry.request_id != request_id);
                }
            }
            _ => {}
        }
    }
    open.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    open
}

/// The latest `turn.plan.updated` plan — preferring the current turn's, so
/// TodoWrite tasks persist across follow-up messages.
pub fn derive_active_plan_state(
    activities: &[OrchestrationThreadActivity],
    latest_turn_id: Option<&TurnId>,
) -> Option<ActivePlanState> {
    let ordered = ordered(activities);
    let plans: Vec<&&OrchestrationThreadActivity> = ordered
        .iter()
        .filter(|activity| activity.kind.0 == "turn.plan.updated")
        .collect();
    let latest = latest_turn_id
        .and_then(|turn_id| {
            plans
                .iter()
                .rev()
                .find(|activity| activity.turn_id.as_ref() == Some(turn_id))
        })
        .or_else(|| plans.last())?;
    let payload = payload_object(latest)?;
    let raw_plan = payload.get("plan")?.as_array()?;
    let steps: Vec<PlanStep> = raw_plan
        .iter()
        .filter_map(|entry| {
            let record = entry.as_object()?;
            let step = payload_str(record, "step")?;
            let status = match payload_str(record, "status") {
                Some("completed") => PlanStepStatus::Completed,
                Some("inProgress") => PlanStepStatus::InProgress,
                _ => PlanStepStatus::Pending,
            };
            Some(PlanStep {
                step: step.to_string(),
                status,
            })
        })
        .collect();
    if steps.is_empty() {
        return None;
    }
    Some(ActivePlanState {
        created_at: latest.created_at.0.clone(),
        turn_id: latest.turn_id.clone(),
        explanation: payload
            .get("explanation")
            .and_then(Value::as_str)
            .map(str::to_string),
        steps,
    })
}

/// Latest context-window usage, from the newest well-formed
/// `context-window.updated` activity (`lib/contextWindow.ts` port — only the
/// fields the meter renders; the per-turn `last*`/breakdown numbers are not
/// displayed anywhere and are left out).
#[derive(Debug, Clone, PartialEq)]
pub struct ContextWindowSnapshot {
    pub used_tokens: f64,
    pub max_tokens: Option<f64>,
    /// `used/max`, capped at 100. `None` without a max.
    pub used_percentage: Option<f64>,
    pub total_processed_tokens: Option<f64>,
    pub compacts_automatically: bool,
    pub updated_at: String,
}

pub fn derive_latest_context_window_snapshot(
    activities: &[OrchestrationThreadActivity],
) -> Option<ContextWindowSnapshot> {
    activities
        .iter()
        .rev()
        .filter(|activity| activity.kind.0 == "context-window.updated")
        .find_map(|activity| {
            let payload = activity.payload.as_object()?;
            let finite = |key: &str| {
                payload
                    .get(key)
                    .and_then(Value::as_f64)
                    .filter(|v| v.is_finite())
            };
            let used_tokens = finite("usedTokens").filter(|used| *used >= 0.0)?;
            let max_tokens = finite("maxTokens");
            let used_percentage = max_tokens
                .filter(|max| *max > 0.0)
                .map(|max| (used_tokens / max * 100.0).min(100.0));
            Some(ContextWindowSnapshot {
                used_tokens,
                max_tokens,
                used_percentage,
                total_processed_tokens: finite("totalProcessedTokens"),
                compacts_automatically: payload
                    .get("compactsAutomatically")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                updated_at: activity.created_at.0.clone(),
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vitre_contracts::{EventId, OrchestrationThreadActivityTone, TrimmedNonEmptyString};

    fn activity(
        id: &str,
        kind: &str,
        created_at: &str,
        payload: Value,
    ) -> OrchestrationThreadActivity {
        OrchestrationThreadActivity {
            created_at: TrimmedNonEmptyString(created_at.into()),
            id: EventId(id.into()),
            kind: TrimmedNonEmptyString(kind.into()),
            payload,
            sequence: None,
            summary: TrimmedNonEmptyString("summary".into()),
            tone: OrchestrationThreadActivityTone::Tool,
            turn_id: None,
        }
    }

    #[test]
    fn approval_requested_then_resolved_is_not_pending() {
        let activities = vec![
            activity(
                "a1",
                "approval.requested",
                "2026-01-01T00:00:00.000Z",
                json!({"requestId": "r1", "requestKind": "command", "detail": "rm -rf /tmp/x"}),
            ),
            activity(
                "a2",
                "approval.resolved",
                "2026-01-01T00:00:01.000Z",
                json!({"requestId": "r1"}),
            ),
        ];
        assert!(derive_pending_approvals(&activities).is_empty());
    }

    #[test]
    fn open_approval_surfaces_with_kind_from_request_type() {
        let activities = vec![activity(
            "a1",
            "approval.requested",
            "2026-01-01T00:00:00.000Z",
            json!({"requestId": "r1", "requestType": "apply_patch_approval"}),
        )];
        let pending = derive_pending_approvals(&activities);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].request_kind, ApprovalRequestKind::FileChange);
        assert_eq!(pending[0].request_id, "r1");
    }

    #[test]
    fn stale_respond_failure_clears_pending_approval() {
        let activities = vec![
            activity(
                "a1",
                "approval.requested",
                "2026-01-01T00:00:00.000Z",
                json!({"requestId": "r1", "requestKind": "command"}),
            ),
            activity(
                "a2",
                "provider.approval.respond.failed",
                "2026-01-01T00:00:01.000Z",
                json!({"requestId": "r1", "detail": "Stale pending approval request r1"}),
            ),
        ];
        assert!(derive_pending_approvals(&activities).is_empty());
    }

    #[test]
    fn unrelated_respond_failure_keeps_pending_approval() {
        let activities = vec![
            activity(
                "a1",
                "approval.requested",
                "2026-01-01T00:00:00.000Z",
                json!({"requestId": "r1", "requestKind": "command"}),
            ),
            activity(
                "a2",
                "provider.approval.respond.failed",
                "2026-01-01T00:00:01.000Z",
                json!({"requestId": "r1", "detail": "network error"}),
            ),
        ];
        assert_eq!(derive_pending_approvals(&activities).len(), 1);
    }

    #[test]
    fn user_input_questions_parse_and_resolve() {
        let question = json!({
            "id": "q1",
            "header": "Choose",
            "question": "Pick one?",
            "options": [{"label": "A", "description": "first"}],
            "multiSelect": false,
        });
        let activities = vec![activity(
            "a1",
            "user-input.requested",
            "2026-01-01T00:00:00.000Z",
            json!({"requestId": "r1", "questions": [question]}),
        )];
        let pending = derive_pending_user_inputs(&activities);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].questions[0].header, "Choose");
        assert_eq!(pending[0].questions[0].options.len(), 1);

        let mut resolved = activities;
        resolved.push(activity(
            "a2",
            "user-input.resolved",
            "2026-01-01T00:00:01.000Z",
            json!({"requestId": "r1"}),
        ));
        assert!(derive_pending_user_inputs(&resolved).is_empty());
    }

    #[test]
    fn user_input_without_valid_questions_is_ignored() {
        let activities = vec![activity(
            "a1",
            "user-input.requested",
            "2026-01-01T00:00:00.000Z",
            json!({"requestId": "r1", "questions": [{"id": "q1"}]}),
        )];
        assert!(derive_pending_user_inputs(&activities).is_empty());
    }

    #[test]
    fn plan_prefers_latest_turn_then_falls_back() {
        let turn_a = TurnId("turn-a".into());
        let turn_b = TurnId("turn-b".into());
        let mut plan_a = activity(
            "a1",
            "turn.plan.updated",
            "2026-01-01T00:00:00.000Z",
            json!({"plan": [{"step": "one", "status": "completed"}]}),
        );
        plan_a.turn_id = Some(turn_a.clone());
        let mut plan_b = activity(
            "a2",
            "turn.plan.updated",
            "2026-01-01T00:00:01.000Z",
            json!({"plan": [{"step": "two", "status": "inProgress"}], "explanation": "why"}),
        );
        plan_b.turn_id = Some(turn_b.clone());
        let activities = vec![plan_a, plan_b];

        let for_a = derive_active_plan_state(&activities, Some(&turn_a)).unwrap();
        assert_eq!(for_a.steps[0].step, "one");
        assert_eq!(for_a.steps[0].status, PlanStepStatus::Completed);

        let fallback =
            derive_active_plan_state(&activities, Some(&TurnId("other".into()))).unwrap();
        assert_eq!(fallback.steps[0].step, "two");
        assert_eq!(fallback.steps[0].status, PlanStepStatus::InProgress);
        assert_eq!(fallback.explanation.as_deref(), Some("why"));
    }

    #[test]
    fn ordering_respects_sequence_created_at_and_lifecycle() {
        let mut early = activity("b", "tool.completed", "2026-01-01T00:00:00.000Z", json!({}));
        early.sequence = Some(Some(vitre_contracts::NonNegativeInt(1)));
        let mut late = activity("a", "tool.started", "2026-01-01T00:00:00.000Z", json!({}));
        late.sequence = Some(Some(vitre_contracts::NonNegativeInt(2)));
        // sequence dominates createdAt/lifecycle/id.
        assert_eq!(
            compare_activities_by_order(&early, &late),
            std::cmp::Ordering::Less
        );

        let started = activity("z", "tool.started", "2026-01-01T00:00:00.000Z", json!({}));
        let completed = activity("a", "tool.completed", "2026-01-01T00:00:00.000Z", json!({}));
        // same createdAt: started ranks before completed despite larger id.
        assert_eq!(
            compare_activities_by_order(&started, &completed),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn context_window_snapshot_prefers_latest_valid_activity() {
        let activities = vec![
            activity(
                "c1",
                "context-window.updated",
                "2026-01-01T00:00:00.000Z",
                json!({"usedTokens": 1000.0, "maxTokens": 200000.0}),
            ),
            activity(
                "c2",
                "context-window.updated",
                "2026-01-01T00:00:01.000Z",
                json!({
                    "usedTokens": 50000.0,
                    "maxTokens": 200000.0,
                    "totalProcessedTokens": 1200000.0,
                    "compactsAutomatically": true
                }),
            ),
            // Newest entry is malformed → falls back to the previous one.
            activity(
                "c3",
                "context-window.updated",
                "2026-01-01T00:00:02.000Z",
                json!({"usedTokens": -5.0}),
            ),
        ];
        let snapshot = derive_latest_context_window_snapshot(&activities).expect("snapshot");
        assert_eq!(snapshot.used_tokens, 50000.0);
        assert_eq!(snapshot.max_tokens, Some(200000.0));
        assert_eq!(snapshot.used_percentage, Some(25.0));
        assert_eq!(snapshot.total_processed_tokens, Some(1200000.0));
        assert!(snapshot.compacts_automatically);
        assert_eq!(snapshot.updated_at, "2026-01-01T00:00:01.000Z");
    }

    #[test]
    fn context_window_snapshot_without_max_has_no_percentage_and_caps_at_100() {
        let no_max = vec![activity(
            "c1",
            "context-window.updated",
            "2026-01-01T00:00:00.000Z",
            json!({"usedTokens": 123.0}),
        )];
        let snapshot = derive_latest_context_window_snapshot(&no_max).expect("snapshot");
        assert_eq!(snapshot.max_tokens, None);
        assert_eq!(snapshot.used_percentage, None);
        assert!(!snapshot.compacts_automatically);

        let over = vec![activity(
            "c2",
            "context-window.updated",
            "2026-01-01T00:00:00.000Z",
            json!({"usedTokens": 300.0, "maxTokens": 200.0}),
        )];
        let snapshot = derive_latest_context_window_snapshot(&over).expect("snapshot");
        assert_eq!(snapshot.used_percentage, Some(100.0));

        assert!(derive_latest_context_window_snapshot(&[]).is_none());
    }
}
