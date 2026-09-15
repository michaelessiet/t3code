//! Work-log and chat-timeline derivations over raw thread activities.
//!
//! Port of the Electron chat timeline's activity pipeline:
//! - `apps/web/src/session-logic.ts` — `deriveWorkLogEntries` and its
//!   extraction/collapse helpers, the tool status predicates, and
//!   `formatDuration` / `deriveTimelineEntries`.
//! - `apps/web/src/components/chat/MessagesTimeline.logic.ts` —
//!   `deriveMessagesTimelineRows` (turn folds + work-run grouping).
//! - `apps/web/src/components/chat/MessagesTimeline.tsx` — row rendering
//!   helpers (icon name, preview, heading, expanded body).
//!
//! Deviations from the TS sources (all deliberate):
//! - Proposed-plan timeline entries are skipped entirely — Vitre has no plan
//!   rows yet, so `derive_timeline_rows` merges only messages and work
//!   entries.
//! - The web work log widens the activity tone with a synthetic `"thinking"`
//!   value for `task.progress` rows. The generated tone enum has no such
//!   variant, so the port stores it as
//!   `OrchestrationThreadActivityTone::Unknown("thinking")`.
//! - `build_tool_call_expanded_body` pretty-prints MCP tool data with
//!   `serde_json`, which orders object keys alphabetically where
//!   `JSON.stringify` preserves insertion order (the payloads already
//!   alphabetize when decoded into `serde_json::Value`).
//! - `Number.prototype.toLocaleString()` is approximated with en-US thousands
//!   grouping and at most three fraction digits.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};
use vitre_contracts::generated::{
    OrchestrationLatestTurn, OrchestrationLatestTurnState, OrchestrationMessage,
    OrchestrationMessageRole, OrchestrationThreadActivity, OrchestrationThreadActivityTone,
};

use crate::sidebar::parse_timestamp_ms;

/// `MAX_VISIBLE_WORK_LOG_ENTRIES` (MessagesTimeline.logic.ts).
pub const MAX_VISIBLE_WORK_LOG_ENTRIES: usize = 1;

/// `TOOL_LIFECYCLE_ITEM_TYPES` (packages/contracts/src/providerRuntime.ts).
const TOOL_LIFECYCLE_ITEM_TYPES: [&str; 7] = [
    "command_execution",
    "file_change",
    "mcp_tool_call",
    "dynamic_tool_call",
    "collab_agent_tool_call",
    "web_search",
    "image_view",
];

/// `isToolLifecycleItemType` (packages/contracts/src/providerRuntime.ts).
fn is_tool_lifecycle_item_type(value: &str) -> bool {
    TOOL_LIFECYCLE_ITEM_TYPES.contains(&value)
}

/// `WorkLogEntry` (session-logic.ts), flattened for Rust: `kind` carries the
/// TS `sourceActivityKind`, `collapse_key` is `""` when the TS field would be
/// `undefined`, and `changed_files` is empty instead of absent.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkLogEntry {
    pub id: String,
    pub created_at: String,
    pub turn_id: Option<String>,
    pub label: String,
    pub tone: OrchestrationThreadActivityTone,
    pub kind: String,
    pub detail: Option<String>,
    pub command: Option<String>,
    pub raw_command: Option<String>,
    pub tool_title: Option<String>,
    pub item_type: Option<String>,
    pub request_kind: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_lifecycle_status: Option<String>,
    pub collapse_key: String,
    pub changed_files: Vec<String>,
    pub tool_data: Option<serde_json::Value>,
}

/// `MessagesTimelineRow` (MessagesTimeline.logic.ts), reduced to the indices
/// the GPUI layer renders from. Proposed-plan rows are omitted (see module
/// doc).
#[derive(Debug, Clone, PartialEq)]
pub enum DerivedTimelineRow {
    /// Index into the `messages` slice passed to `derive_timeline_rows`.
    Message {
        message_index: usize,
    },
    /// Index into the `work_entries` slice.
    Work {
        entry_index: usize,
    },
    WorkToggle {
        group_id: String,
        hidden_count: usize,
        expanded: bool,
        only_tool_entries: bool,
    },
    TurnFold {
        turn_id: String,
        label: String,
        expanded: bool,
    },
    Working,
}

/// Inputs of `deriveMessagesTimelineRows` (MessagesTimeline.logic.ts) that
/// affect row structure.
pub struct TimelineDeriveInput<'a> {
    pub messages: &'a [OrchestrationMessage],
    pub work_entries: &'a [WorkLogEntry],
    pub latest_turn: Option<&'a OrchestrationLatestTurn>,
    /// session.activeTurnId when session.status == Running.
    pub running_turn_id: Option<&'a str>,
    pub expanded_turn_ids: &'a HashSet<String>,
    pub expanded_work_group_ids: &'a HashSet<String>,
    pub is_working: bool,
}

// ---------------------------------------------------------------------------
// Tone helpers
// ---------------------------------------------------------------------------

/// The synthetic `"thinking"` work-log tone (see module doc).
fn thinking_tone() -> OrchestrationThreadActivityTone {
    OrchestrationThreadActivityTone::Unknown("thinking".to_string())
}

fn is_thinking_tone(tone: &OrchestrationThreadActivityTone) -> bool {
    matches!(tone, OrchestrationThreadActivityTone::Unknown(value) if value == "thinking")
}

// ---------------------------------------------------------------------------
// JSON payload helpers (session-logic.ts `asRecord` / `asTrimmedString` /
// `asNumber`)
// ---------------------------------------------------------------------------

fn as_record(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value.and_then(Value::as_object)
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn as_trimmed_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .and_then(trimmed_non_empty)
        .map(str::to_string)
}

fn as_finite_number(value: &Value) -> Option<f64> {
    value.as_f64().filter(|number| number.is_finite())
}

// ---------------------------------------------------------------------------
// Activity ordering (session-logic.ts `compareActivitiesByOrder` /
// `compareActivityLifecycleRank`)
// ---------------------------------------------------------------------------

/// `compareActivitiesByOrder`: sequence first (present sorts after absent),
/// then createdAt, then lifecycle rank, then id.
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

/// `compareActivityLifecycleRank`: `.started` = 0, `.progress`/`.updated`
/// (and anything unrecognized) = 1, `.completed`/`.resolved` = 2.
fn lifecycle_rank(kind: &str) -> u8 {
    if kind.ends_with(".started") || kind == "tool.started" {
        0
    } else if kind.ends_with(".completed") || kind.ends_with(".resolved") {
        2
    } else {
        1
    }
}

// ---------------------------------------------------------------------------
// Work-log derivation (session-logic.ts `deriveWorkLogEntries`)
// ---------------------------------------------------------------------------

/// `DerivedWorkLogEntry` (session-logic.ts): the working shape during
/// collapse; `collapse_key` stays optional until the final mapping.
struct DerivedEntry {
    id: String,
    created_at: String,
    turn_id: Option<String>,
    label: String,
    tone: OrchestrationThreadActivityTone,
    activity_kind: String,
    detail: Option<String>,
    command: Option<String>,
    raw_command: Option<String>,
    changed_files: Vec<String>,
    tool_title: Option<String>,
    tool_data: Option<Value>,
    item_type: Option<String>,
    request_kind: Option<String>,
    tool_call_id: Option<String>,
    tool_lifecycle_status: Option<String>,
    collapse_key: Option<String>,
}

impl DerivedEntry {
    fn into_work_log_entry(self) -> WorkLogEntry {
        WorkLogEntry {
            id: self.id,
            created_at: self.created_at,
            turn_id: self.turn_id,
            label: self.label,
            tone: self.tone,
            kind: self.activity_kind,
            detail: self.detail,
            command: self.command,
            raw_command: self.raw_command,
            tool_title: self.tool_title,
            item_type: self.item_type,
            request_kind: self.request_kind,
            tool_call_id: self.tool_call_id,
            tool_lifecycle_status: self.tool_lifecycle_status,
            collapse_key: self.collapse_key.unwrap_or_default(),
            changed_files: self.changed_files,
            tool_data: self.tool_data,
        }
    }
}

/// `deriveWorkLogEntries` (session-logic.ts): sort, filter lifecycle noise,
/// map to entries, then collapse tool lifecycle pairs.
pub fn derive_work_log_entries(activities: &[OrchestrationThreadActivity]) -> Vec<WorkLogEntry> {
    let mut ordered: Vec<&OrchestrationThreadActivity> = activities.iter().collect();
    ordered.sort_by(|a, b| compare_activities_by_order(a, b));

    let mut entries: Vec<DerivedEntry> = Vec::new();
    for activity in ordered {
        let kind = activity.kind.0.as_str();
        if kind == "tool.started" || kind == "task.started" || kind == "context-window.updated" {
            continue;
        }
        if activity.summary.0 == "Checkpoint captured" {
            continue;
        }
        if is_plan_boundary_tool_activity(activity) {
            continue;
        }
        entries.push(to_derived_work_log_entry(activity));
    }
    collapse_derived_work_log_entries(entries)
        .into_iter()
        .map(DerivedEntry::into_work_log_entry)
        .collect()
}

/// `isPlanBoundaryToolActivity` (session-logic.ts): tool.updated /
/// tool.completed whose payload detail starts with `ExitPlanMode:`.
fn is_plan_boundary_tool_activity(activity: &OrchestrationThreadActivity) -> bool {
    let kind = activity.kind.0.as_str();
    if kind != "tool.updated" && kind != "tool.completed" {
        return false;
    }
    activity
        .payload
        .as_object()
        .and_then(|payload| payload.get("detail"))
        .and_then(Value::as_str)
        .is_some_and(|detail| detail.starts_with("ExitPlanMode:"))
}

/// `extractWorkLogToolLifecycleStatus` (session-logic.ts).
fn extract_work_log_tool_lifecycle_status(payload: Option<&Map<String, Value>>) -> Option<String> {
    let status = payload?.get("status")?.as_str()?;
    matches!(
        status,
        "inProgress" | "completed" | "failed" | "declined" | "stopped"
    )
    .then(|| status.to_string())
}

/// `toDerivedWorkLogEntry` (session-logic.ts).
fn to_derived_work_log_entry(activity: &OrchestrationThreadActivity) -> DerivedEntry {
    let payload = activity.payload.as_object();
    let (command, raw_command) = extract_tool_command(payload);
    let changed_files = extract_changed_files(payload);
    let title = extract_tool_title(payload);
    let kind = activity.kind.0.as_str();
    let is_task_activity = kind == "task.progress" || kind == "task.completed";
    let task_summary: Option<&str> = if is_task_activity {
        payload
            .and_then(|p| p.get("summary"))
            .and_then(Value::as_str)
            .filter(|summary| !summary.is_empty())
    } else {
        None
    };
    let task_detail_as_label: Option<&str> = if is_task_activity && task_summary.is_none() {
        payload
            .and_then(|p| p.get("detail"))
            .and_then(Value::as_str)
            .filter(|detail| !detail.is_empty())
    } else {
        None
    };
    let task_label = task_summary.or(task_detail_as_label);
    let detail: Option<String> = if is_task_activity {
        if task_detail_as_label.is_none() {
            payload
                .and_then(|p| p.get("detail"))
                .and_then(Value::as_str)
                .filter(|detail| !detail.is_empty())
                .and_then(strip_trailing_exit_code)
        } else {
            None
        }
    } else {
        let heading = title.as_deref().unwrap_or(activity.summary.0.as_str());
        extract_tool_detail(payload, heading)
    };
    let tool_call_id = if is_task_activity {
        None
    } else {
        extract_tool_call_id(payload)
    };
    let item_type = extract_work_log_item_type(payload);
    let request_kind = extract_work_log_request_kind(payload);
    let tool_data: Option<Value> = if item_type.as_deref() == Some("mcp_tool_call") {
        as_record(payload.and_then(|p| p.get("data")))
            .and_then(|data| data.get("item"))
            .cloned()
    } else {
        None
    };
    let mut tool_lifecycle_status = extract_work_log_tool_lifecycle_status(payload);
    if tool_lifecycle_status.is_none() && kind == "tool.completed" {
        tool_lifecycle_status = Some("completed".to_string());
    }
    let tone = if kind == "task.progress" {
        thinking_tone()
    } else if matches!(activity.tone, OrchestrationThreadActivityTone::Approval) {
        OrchestrationThreadActivityTone::Info
    } else {
        activity.tone.clone()
    };
    let mut entry = DerivedEntry {
        id: activity.id.0.clone(),
        created_at: activity.created_at.0.clone(),
        turn_id: activity.turn_id.as_ref().map(|turn| turn.0.clone()),
        label: task_label
            .map(str::to_string)
            .unwrap_or_else(|| activity.summary.0.clone()),
        tone,
        activity_kind: kind.to_string(),
        detail,
        command,
        raw_command,
        changed_files,
        tool_title: title,
        tool_data,
        item_type,
        request_kind,
        tool_call_id,
        tool_lifecycle_status,
        collapse_key: None,
    };
    entry.collapse_key = derive_tool_lifecycle_collapse_key(&entry);
    entry
}

/// `collapseDerivedWorkLogEntries` (session-logic.ts): fold adjacent tool
/// lifecycle entries into a single row.
fn collapse_derived_work_log_entries(entries: Vec<DerivedEntry>) -> Vec<DerivedEntry> {
    let mut collapsed: Vec<DerivedEntry> = Vec::new();
    for entry in entries {
        if let Some(previous) = collapsed.last()
            && should_collapse_tool_lifecycle_entries(previous, &entry)
        {
            let previous = collapsed.pop().expect("checked non-empty");
            collapsed.push(merge_derived_work_log_entries(previous, entry));
            continue;
        }
        collapsed.push(entry);
    }
    collapsed
}

/// `shouldCollapseToolLifecycleEntries` (session-logic.ts). A previous
/// `tool.completed` never absorbs more entries.
fn should_collapse_tool_lifecycle_entries(previous: &DerivedEntry, next: &DerivedEntry) -> bool {
    if previous.activity_kind != "tool.updated" && previous.activity_kind != "tool.completed" {
        return false;
    }
    if next.activity_kind != "tool.updated" && next.activity_kind != "tool.completed" {
        return false;
    }
    if previous.activity_kind == "tool.completed" {
        return false;
    }
    if previous.collapse_key.is_some() && previous.collapse_key == next.collapse_key {
        return true;
    }
    previous.tool_call_id.is_some()
        && next.tool_call_id.is_none()
        && previous.item_type == next.item_type
        && normalize_compact_tool_label(previous.tool_title.as_deref().unwrap_or(&previous.label))
            == normalize_compact_tool_label(next.tool_title.as_deref().unwrap_or(&next.label))
}

/// `mergeDerivedWorkLogEntries` (session-logic.ts): the NEWER activity wins
/// id/createdAt/label/tone/kind; per-field `next ?? previous` fallbacks for
/// the rest.
fn merge_derived_work_log_entries(previous: DerivedEntry, next: DerivedEntry) -> DerivedEntry {
    let changed_files = merge_changed_files(&previous.changed_files, &next.changed_files);
    DerivedEntry {
        id: next.id,
        created_at: next.created_at,
        turn_id: next.turn_id,
        label: next.label,
        tone: next.tone,
        activity_kind: next.activity_kind,
        detail: next.detail.or(previous.detail),
        command: next.command.or(previous.command),
        raw_command: next.raw_command.or(previous.raw_command),
        changed_files,
        tool_title: next.tool_title.or(previous.tool_title),
        tool_data: next.tool_data.or(previous.tool_data),
        item_type: next.item_type.or(previous.item_type),
        request_kind: next.request_kind.or(previous.request_kind),
        tool_call_id: next.tool_call_id.or(previous.tool_call_id),
        tool_lifecycle_status: next
            .tool_lifecycle_status
            .or(previous.tool_lifecycle_status),
        collapse_key: next.collapse_key.or(previous.collapse_key),
    }
}

/// `mergeChangedFiles` (session-logic.ts): concat then dedupe, keeping first
/// occurrence order.
fn merge_changed_files(previous: &[String], next: &[String]) -> Vec<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut merged: Vec<String> = Vec::new();
    for file in previous.iter().chain(next.iter()) {
        if seen.insert(file.as_str()) {
            merged.push(file.clone());
        }
    }
    merged
}

/// `deriveToolLifecycleCollapseKey` (session-logic.ts).
fn derive_tool_lifecycle_collapse_key(entry: &DerivedEntry) -> Option<String> {
    if entry.activity_kind != "tool.updated" && entry.activity_kind != "tool.completed" {
        return None;
    }
    if let Some(tool_call_id) = &entry.tool_call_id {
        return Some(format!("tool:{tool_call_id}"));
    }
    let normalized_label =
        normalize_compact_tool_label(entry.tool_title.as_deref().unwrap_or(&entry.label));
    let detail = entry.detail.as_deref().map(str::trim).unwrap_or("");
    let item_type = entry.item_type.as_deref().unwrap_or("");
    if normalized_label.is_empty() && detail.is_empty() && item_type.is_empty() {
        return None;
    }
    Some(format!("{item_type}\u{1f}{normalized_label}\u{1f}{detail}"))
}

/// `normalizeCompactToolLabel` (session-logic.ts):
/// `value.replace(/\s+(?:complete|completed)\s*$/i, "").trim()` — strips one
/// trailing "complete"/"completed" word that is preceded by whitespace.
pub fn normalize_compact_tool_label(value: &str) -> String {
    let trimmed_end = value.trim_end();
    for word in ["completed", "complete"] {
        let len = trimmed_end.len();
        if len > word.len()
            && trimmed_end.is_char_boundary(len - word.len())
            && trimmed_end[len - word.len()..].eq_ignore_ascii_case(word)
        {
            let head = &trimmed_end[..len - word.len()];
            if head.ends_with(char::is_whitespace) {
                return head.trim().to_string();
            }
        }
    }
    value.trim().to_string()
}

// ---------------------------------------------------------------------------
// Command extraction (session-logic.ts `extractToolCommand` and helpers)
// ---------------------------------------------------------------------------

/// `trimMatchingOuterQuotes` (session-logic.ts). A quoted-but-empty value
/// falls back to the still-quoted trimmed input, as in the TS.
fn trim_matching_outer_quotes(value: &str) -> &str {
    let trimmed = value.trim();
    let quoted = (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        || (trimmed.starts_with('"') && trimmed.ends_with('"'));
    if quoted && trimmed.len() >= 2 {
        let unquoted = trimmed[1..trimmed.len() - 1].trim();
        if !unquoted.is_empty() {
            return unquoted;
        }
    }
    trimmed
}

/// `executableBasename` (session-logic.ts).
fn executable_basename(value: &str) -> Option<String> {
    let trimmed = trim_matching_outer_quotes(value);
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.replace('\\', "/");
    let last = normalized.split('/').next_back().unwrap_or("").trim();
    if last.is_empty() {
        None
    } else {
        Some(last.to_lowercase())
    }
}

/// `splitExecutableAndRest` (session-logic.ts).
fn split_executable_and_rest(value: &str) -> Option<(String, String)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let first = trimmed.chars().next().expect("non-empty");
    if first == '"' || first == '\'' {
        let close_index = 1 + trimmed[1..].find(first)?;
        return Some((
            trimmed[..=close_index].to_string(),
            trimmed[close_index + 1..].trim().to_string(),
        ));
    }
    match trimmed.find(char::is_whitespace) {
        None => Some((trimmed.to_string(), String::new())),
        Some(index) => Some((
            trimmed[..index].to_string(),
            trimmed[index..].trim().to_string(),
        )),
    }
}

/// `SHELL_WRAPPER_SPECS` (session-logic.ts).
#[derive(Clone, Copy)]
enum ShellWrapperKind {
    /// `pwsh` / `powershell`: `/(?:^|\s)-command\s+/i`
    PowerShell,
    /// `cmd`: `/(?:^|\s)\/c\s+/i`
    Cmd,
    /// `bash` / `sh` / `zsh`: `/(?:^|\s)-(?:l)?c\s+/i`
    PosixShell,
}

fn find_shell_wrapper_spec(shell: &str) -> Option<ShellWrapperKind> {
    match shell {
        "pwsh" | "pwsh.exe" | "powershell" | "powershell.exe" => Some(ShellWrapperKind::PowerShell),
        "cmd" | "cmd.exe" => Some(ShellWrapperKind::Cmd),
        "bash" | "sh" | "zsh" => Some(ShellWrapperKind::PosixShell),
        _ => None,
    }
}

fn wrapper_flag_len(rest: &str, kind: ShellWrapperKind) -> Option<usize> {
    let flags: &[&str] = match kind {
        ShellWrapperKind::PowerShell => &["-command"],
        ShellWrapperKind::Cmd => &["/c"],
        // Greedy `(?:l)?`: prefer "-lc" over "-c" at the same position.
        ShellWrapperKind::PosixShell => &["-lc", "-c"],
    };
    flags.iter().find_map(|flag| {
        (rest.len() >= flag.len()
            && rest.is_char_boundary(flag.len())
            && rest[..flag.len()].eq_ignore_ascii_case(flag))
        .then_some(flag.len())
    })
}

/// Hand-rolled `wrapperFlagPattern` matcher: leftmost `(?:^|\s)FLAG\s+`
/// (case-insensitive), returning the byte index after the trailing
/// whitespace run — i.e. `match.index + match[0].length` in the TS.
fn match_wrapper_flag(value: &str, kind: ShellWrapperKind) -> Option<usize> {
    let mut previous_char: Option<char> = None;
    for (index, ch) in value.char_indices() {
        let boundary_ok = previous_char.is_none_or(char::is_whitespace);
        previous_char = Some(ch);
        if !boundary_ok {
            continue;
        }
        let rest = &value[index..];
        let Some(flag_len) = wrapper_flag_len(rest, kind) else {
            continue;
        };
        let after_flag = &rest[flag_len..];
        let ws_len = after_flag
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(after_flag.len());
        if ws_len == 0 {
            continue;
        }
        return Some(index + flag_len + ws_len);
    }
    None
}

/// `unwrapCommandRemainder` (session-logic.ts).
fn unwrap_command_remainder(value: &str, kind: ShellWrapperKind) -> Option<String> {
    let after = match_wrapper_flag(value, kind)?;
    let command = value[after..].trim();
    if command.is_empty() {
        return None;
    }
    let unwrapped = trim_matching_outer_quotes(command);
    if unwrapped.is_empty() {
        None
    } else {
        Some(unwrapped.to_string())
    }
}

/// `unwrapKnownShellCommandWrapper` (session-logic.ts).
fn unwrap_known_shell_command_wrapper(value: &str) -> String {
    let Some((executable, rest)) = split_executable_and_rest(value) else {
        return value.to_string();
    };
    if rest.is_empty() {
        return value.to_string();
    }
    let Some(shell) = executable_basename(&executable) else {
        return value.to_string();
    };
    let Some(spec) = find_shell_wrapper_spec(&shell) else {
        return value.to_string();
    };
    unwrap_command_remainder(&rest, spec).unwrap_or_else(|| value.to_string())
}

/// `formatCommandArrayPart` (session-logic.ts): quote parts containing
/// whitespace, `"`, `'`, or a backtick.
fn format_command_array_part(part: &str) -> String {
    if part
        .chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\'' || c == '`')
    {
        format!("\"{}\"", part.replace('"', "\\\""))
    } else {
        part.to_string()
    }
}

/// `formatCommandValue` (session-logic.ts).
fn format_command_value(value: &Value) -> Option<String> {
    if let Some(direct) = as_trimmed_string(Some(value)) {
        return Some(direct);
    }
    let entries = value.as_array()?;
    let parts: Vec<String> = entries
        .iter()
        .filter_map(|entry| as_trimmed_string(Some(entry)))
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(
        parts
            .iter()
            .map(|part| format_command_array_part(part))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// `normalizeCommandValue` (session-logic.ts).
fn normalize_command_value(value: &Value) -> Option<String> {
    format_command_value(value).map(|formatted| unwrap_known_shell_command_wrapper(&formatted))
}

/// `toRawToolCommand` (session-logic.ts): the pre-unwrap command when the
/// shell-wrapper normalization changed it.
fn to_raw_tool_command(value: &Value, normalized: &str) -> Option<String> {
    let formatted = format_command_value(value)?;
    if formatted == normalized {
        None
    } else {
        Some(formatted)
    }
}

/// `extractToolCommand` (session-logic.ts). Returns `(command, raw_command)`.
fn extract_tool_command(payload: Option<&Map<String, Value>>) -> (Option<String>, Option<String>) {
    let data = as_record(payload.and_then(|p| p.get("data")));
    let item = as_record(data.and_then(|d| d.get("item")));
    let item_result = as_record(item.and_then(|i| i.get("result")));
    let item_input = as_record(item.and_then(|i| i.get("input")));
    let item_type = as_trimmed_string(payload.and_then(|p| p.get("itemType")));
    let detail = as_trimmed_string(payload.and_then(|p| p.get("detail")));
    let detail_candidate: Option<Value> = if item_type.as_deref() == Some("command_execution") {
        detail
            .as_deref()
            .and_then(strip_trailing_exit_code)
            .map(Value::String)
    } else {
        None
    };

    let candidates: [Option<&Value>; 5] = [
        item.and_then(|i| i.get("command")),
        item_input.and_then(|i| i.get("command")),
        item_result.and_then(|i| i.get("command")),
        data.and_then(|d| d.get("command")),
        detail_candidate.as_ref(),
    ];
    for candidate in candidates.into_iter().flatten() {
        if let Some(command) = normalize_command_value(candidate) {
            let raw_command = to_raw_tool_command(candidate, &command);
            return (Some(command), raw_command);
        }
    }
    (None, None)
}

/// `extractToolTitle` (session-logic.ts).
fn extract_tool_title(payload: Option<&Map<String, Value>>) -> Option<String> {
    as_trimmed_string(payload.and_then(|p| p.get("title")))
}

/// `extractToolCallId` (session-logic.ts).
fn extract_tool_call_id(payload: Option<&Map<String, Value>>) -> Option<String> {
    as_trimmed_string(
        as_record(payload.and_then(|p| p.get("data"))).and_then(|data| data.get("toolCallId")),
    )
}

// ---------------------------------------------------------------------------
// Detail extraction (session-logic.ts `extractToolDetail` and helpers)
// ---------------------------------------------------------------------------

/// `normalizeInlinePreview` (session-logic.ts): collapse whitespace runs to a
/// single space and trim.
fn normalize_inline_preview(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `truncateInlinePreview` (session-logic.ts), maxLength 84. Counts chars
/// where the TS counts UTF-16 units (differs only for surrogate pairs).
fn truncate_inline_preview(value: &str) -> String {
    const MAX_LENGTH: usize = 84;
    if value.chars().count() <= MAX_LENGTH {
        return value.to_string();
    }
    let cut: String = value.chars().take(MAX_LENGTH - 1).collect();
    format!("{}…", cut.trim_end())
}

/// `normalizePreviewForComparison` (session-logic.ts).
fn normalize_preview_for_comparison(value: Option<&str>) -> Option<String> {
    let normalized = value.and_then(trimmed_non_empty)?;
    Some(normalize_compact_tool_label(&normalize_inline_preview(normalized)).to_lowercase())
}

/// `Number.prototype.toLocaleString()` approximation: en-US thousands
/// grouping, at most three fraction digits.
fn format_locale_number(value: f64) -> String {
    let negative = value < 0.0;
    let abs = value.abs();
    let mut integer_part = abs.trunc() as u64;
    let mut fraction_thousandths = (abs.fract() * 1000.0).round() as u64;
    if fraction_thousandths >= 1000 {
        integer_part += 1;
        fraction_thousandths = 0;
    }
    let digits = integer_part.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.as_bytes().iter().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(*digit as char);
    }
    let mut result = String::new();
    if negative && (integer_part > 0 || fraction_thousandths > 0) {
        result.push('-');
    }
    result.push_str(&grouped);
    if fraction_thousandths > 0 {
        let fraction = format!("{fraction_thousandths:03}");
        result.push('.');
        result.push_str(fraction.trim_end_matches('0'));
    }
    result
}

/// `summarizeToolTextOutput` (session-logic.ts).
fn summarize_tool_text_output(value: &str) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw_line in value.split('\n') {
        let raw_line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let line = normalize_inline_preview(raw_line);
        if !line.is_empty() {
            lines.push(line);
        }
    }
    if let Some(first_line) = lines.iter().find(|line| line.as_str() != "```") {
        return Some(truncate_inline_preview(first_line));
    }
    if lines.len() > 1 {
        return Some(format!(
            "{} lines",
            format_locale_number(lines.len() as f64)
        ));
    }
    None
}

/// `summarizeToolRawOutput` (session-logic.ts).
fn summarize_tool_raw_output(payload: Option<&Map<String, Value>>) -> Option<String> {
    let data = as_record(payload.and_then(|p| p.get("data")));
    let raw_output = as_record(data.and_then(|d| d.get("rawOutput")))?;

    if let Some(total_files) = raw_output.get("totalFiles").and_then(as_finite_number) {
        let suffix = if raw_output.get("truncated") == Some(&Value::Bool(true)) {
            "+"
        } else {
            ""
        };
        let plural = if total_files == 1.0 { "" } else { "s" };
        return Some(format!(
            "{} file{plural}{suffix}",
            format_locale_number(total_files)
        ));
    }

    if let Some(content) = as_trimmed_string(raw_output.get("content")) {
        return summarize_tool_text_output(&content);
    }

    if let Some(stdout) = as_trimmed_string(raw_output.get("stdout")) {
        return summarize_tool_text_output(&stdout);
    }

    None
}

/// `isCommandToolDetail` (session-logic.ts).
fn is_command_tool_detail(payload: Option<&Map<String, Value>>, heading: &str) -> bool {
    let data = as_record(payload.and_then(|p| p.get("data")));
    let kind = as_trimmed_string(data.and_then(|d| d.get("kind"))).map(|k| k.to_lowercase());
    // TS `payload?.title ?? heading`: fall back to the heading only when
    // `title` is absent or null.
    let title_source: Option<String> = match payload.and_then(|p| p.get("title")) {
        None | Some(Value::Null) => trimmed_non_empty(heading).map(str::to_string),
        Some(value) => as_trimmed_string(Some(value)),
    };
    let title = title_source.map(|t| t.to_lowercase());
    extract_work_log_item_type(payload).as_deref() == Some("command_execution")
        || kind.as_deref() == Some("execute")
        || title.as_deref() == Some("terminal")
        || title.as_deref() == Some("ran command")
}

/// `extractToolDetail` (session-logic.ts).
fn extract_tool_detail(payload: Option<&Map<String, Value>>, heading: &str) -> Option<String> {
    let raw_detail = as_trimmed_string(payload.and_then(|p| p.get("detail")));
    let detail = raw_detail.as_deref().and_then(strip_trailing_exit_code);
    let normalized_heading = normalize_preview_for_comparison(Some(heading));
    let normalized_detail = normalize_preview_for_comparison(detail.as_deref());

    if let Some(detail_text) = &detail
        && normalized_heading != normalized_detail
    {
        return Some(detail_text.clone());
    }

    if is_command_tool_detail(payload, heading) {
        return None;
    }

    if let Some(raw_output_summary) = summarize_tool_raw_output(payload) {
        let normalized_summary = normalize_preview_for_comparison(Some(&raw_output_summary));
        if normalized_summary != normalized_heading {
            return Some(raw_output_summary);
        }
    }

    None
}

/// `stripTrailingExitCode` (session-logic.ts), output side only: removes one
/// trailing `<exited with exit code N>` tag (case-insensitive) and trims.
/// Returns `None` when nothing but the tag (or whitespace) remains.
fn strip_trailing_exit_code(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let output = match trailing_exit_code_start(trimmed) {
        Some(start) => trimmed[..start].trim(),
        None => trimmed,
    };
    if output.is_empty() {
        None
    } else {
        Some(output.to_string())
    }
}

/// Byte offset where a trailing `<exited with exit code \d+>` tag begins, if
/// present at the very end of the (pre-trimmed) string.
fn trailing_exit_code_start(trimmed: &str) -> Option<usize> {
    const LITERAL: &str = "<exited with exit code ";
    let bytes = trimmed.as_bytes();
    if bytes.last() != Some(&b'>') {
        return None;
    }
    let digits_end = bytes.len() - 1;
    let mut index = digits_end;
    while index > 0 && bytes[index - 1].is_ascii_digit() {
        index -= 1;
    }
    if index == digits_end {
        return None;
    }
    let start = index.checked_sub(LITERAL.len())?;
    if trimmed.is_char_boundary(start) && trimmed[start..index].eq_ignore_ascii_case(LITERAL) {
        Some(start)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Item type / request kind / changed files (session-logic.ts)
// ---------------------------------------------------------------------------

/// `extractWorkLogItemType` (session-logic.ts): exact (untrimmed) membership
/// in the tool lifecycle item types.
fn extract_work_log_item_type(payload: Option<&Map<String, Value>>) -> Option<String> {
    let item_type = payload?.get("itemType")?.as_str()?;
    is_tool_lifecycle_item_type(item_type).then(|| item_type.to_string())
}

/// `requestKindFromRequestType` (session-logic.ts).
fn request_kind_from_request_type(request_type: Option<&Value>) -> Option<String> {
    match request_type.and_then(Value::as_str) {
        Some("command_execution_approval")
        | Some("exec_command_approval")
        | Some("dynamic_tool_call") => Some("command".to_string()),
        Some("file_read_approval") => Some("file-read".to_string()),
        Some("file_change_approval") | Some("apply_patch_approval") => {
            Some("file-change".to_string())
        }
        _ => None,
    }
}

/// `extractWorkLogRequestKind` (session-logic.ts).
fn extract_work_log_request_kind(payload: Option<&Map<String, Value>>) -> Option<String> {
    let payload = payload?;
    if let Some(kind) = payload.get("requestKind").and_then(Value::as_str)
        && matches!(kind, "command" | "file-read" | "file-change")
    {
        return Some(kind.to_string());
    }
    request_kind_from_request_type(payload.get("requestType"))
}

/// `pushChangedFile` (session-logic.ts).
fn push_changed_file(target: &mut Vec<String>, seen: &mut HashSet<String>, value: Option<&Value>) {
    let Some(normalized) = as_trimmed_string(value) else {
        return;
    };
    if seen.contains(&normalized) {
        return;
    }
    seen.insert(normalized.clone());
    target.push(normalized);
}

/// `collectChangedFiles` (session-logic.ts): bounded walk (depth 4, 12 files)
/// over well-known path-ish keys.
fn collect_changed_files(
    value: &Value,
    target: &mut Vec<String>,
    seen: &mut HashSet<String>,
    depth: u32,
) {
    if depth > 4 || target.len() >= 12 {
        return;
    }
    if let Some(entries) = value.as_array() {
        for entry in entries {
            collect_changed_files(entry, target, seen, depth + 1);
            if target.len() >= 12 {
                return;
            }
        }
        return;
    }

    let Some(record) = value.as_object() else {
        return;
    };

    for key in [
        "path",
        "filePath",
        "relativePath",
        "filename",
        "newPath",
        "oldPath",
    ] {
        push_changed_file(target, seen, record.get(key));
    }

    for nested_key in [
        "item",
        "result",
        "input",
        "data",
        "changes",
        "files",
        "edits",
        "patch",
        "patches",
        "operations",
    ] {
        let Some(nested) = record.get(nested_key) else {
            continue;
        };
        collect_changed_files(nested, target, seen, depth + 1);
        if target.len() >= 12 {
            return;
        }
    }
}

/// `extractChangedFiles` (session-logic.ts).
fn extract_changed_files(payload: Option<&Map<String, Value>>) -> Vec<String> {
    let mut target: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    if let Some(data) = payload.and_then(|p| p.get("data")) {
        collect_changed_files(data, &mut target, &mut seen, 0);
    }
    target
}

// ---------------------------------------------------------------------------
// Tool status predicates (session-logic.ts)
// ---------------------------------------------------------------------------

/// `workLogEntryIsToolLike` (session-logic.ts).
pub fn work_log_entry_is_tool_like(entry: &WorkLogEntry) -> bool {
    if matches!(
        entry.tone,
        OrchestrationThreadActivityTone::Tool | OrchestrationThreadActivityTone::Error
    ) || is_thinking_tone(&entry.tone)
    {
        return true;
    }
    if entry
        .command
        .as_deref()
        .is_some_and(|command| !command.trim().is_empty())
    {
        return true;
    }
    if entry.request_kind.is_some() {
        return true;
    }
    entry
        .item_type
        .as_deref()
        .is_some_and(is_tool_lifecycle_item_type)
}

/// `toolDetailTextLooksLikeFailure` (session-logic.ts): failure heuristics
/// over tool output text. The three TS regexes are hand-rolled below.
fn tool_detail_text_looks_like_failure(text: &str) -> bool {
    let lower = text.to_lowercase();
    if lower.contains("file not found") {
        return true;
    }
    if lower.contains("no files found") {
        return true;
    }
    if lower.contains("enoent")
        || lower.contains("no such file or directory")
        || lower.contains("no such file")
    {
        return true;
    }
    if lower.contains("cannot find path") && lower.contains("because it does not exist") {
        return true;
    }
    if lower.contains("commandnotfoundexception") {
        return true;
    }
    if lower.contains("is not recognized as the name of a cmdlet") {
        return true;
    }
    if lower.contains("is not recognized") && lower.contains("the term '") {
        return true;
    }
    if lower.contains("a parameter cannot be found that matches parameter name") {
        return true;
    }
    if lower.contains("command not found") {
        return true;
    }
    if has_exited_tag_with_nonzero_code(&lower) {
        return true;
    }
    if has_exit_with_exit_code_nonzero(&lower) {
        return true;
    }
    has_exit_code_separator_nonzero(&lower)
}

/// `/<exited with exit code\s+[1-9]\d*\s*>/i`: literal prefix, ≥1 whitespace,
/// a nonzero number, optional whitespace, `>`.
fn has_exited_tag_with_nonzero_code(lower: &str) -> bool {
    const NEEDLE: &str = "<exited with exit code";
    lower.match_indices(NEEDLE).any(|(index, _)| {
        let mut chars = lower[index + NEEDLE.len()..].chars().peekable();
        let mut saw_whitespace = false;
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
            saw_whitespace = true;
        }
        if !saw_whitespace {
            return false;
        }
        if !chars.peek().is_some_and(|c| ('1'..='9').contains(c)) {
            return false;
        }
        while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
            chars.next();
        }
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        chars.next() == Some('>')
    })
}

/// `/exit(?:ed)? with exit code\s+[1-9]\d*/i`: either literal, ≥1 whitespace,
/// then a nonzero leading digit.
fn has_exit_with_exit_code_nonzero(lower: &str) -> bool {
    ["exited with exit code", "exit with exit code"]
        .iter()
        .any(|needle| {
            lower.match_indices(needle).any(|(index, _)| {
                let mut chars = lower[index + needle.len()..].chars().peekable();
                let mut saw_whitespace = false;
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                    saw_whitespace = true;
                }
                saw_whitespace && chars.peek().is_some_and(|c| ('1'..='9').contains(c))
            })
        })
}

/// `/exit code\s*[:\s]\s*[1-9]\d*\b/i`: "exit code", a separator that is
/// either ≥1 whitespace or an optionally-whitespace-padded colon, a nonzero
/// number, then a word boundary (JS `\w` = `[A-Za-z0-9_]`).
fn has_exit_code_separator_nonzero(lower: &str) -> bool {
    lower.match_indices("exit code").any(|(index, _)| {
        let mut chars = lower[index + "exit code".len()..].chars().peekable();
        let mut leading_whitespace = 0usize;
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
            leading_whitespace += 1;
        }
        if chars.peek() == Some(&':') {
            chars.next();
            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
        } else if leading_whitespace == 0 {
            return false;
        }
        if !chars.peek().is_some_and(|c| ('1'..='9').contains(c)) {
            return false;
        }
        while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
            chars.next();
        }
        match chars.peek() {
            None => true,
            Some(c) => !(c.is_ascii_alphanumeric() || *c == '_'),
        }
    })
}

/// `workEntryIndicatesToolFailure` (session-logic.ts).
pub fn work_entry_indicates_tool_failure(entry: &WorkLogEntry) -> bool {
    if matches!(entry.tone, OrchestrationThreadActivityTone::Error) {
        return true;
    }
    if matches!(
        entry.tool_lifecycle_status.as_deref(),
        Some("failed") | Some("declined")
    ) {
        return true;
    }
    if !work_log_entry_is_tool_like(entry) {
        return false;
    }
    let mut parts: Vec<&str> = Vec::new();
    if let Some(detail) = entry.detail.as_deref().filter(|d| !d.is_empty()) {
        parts.push(detail);
    }
    if let Some(command) = entry.command.as_deref().filter(|c| !c.is_empty()) {
        parts.push(command);
    }
    if parts.is_empty() {
        return false;
    }
    tool_detail_text_looks_like_failure(&parts.join("\n"))
}

/// `workEntryIndicatesToolSuccess` (session-logic.ts).
pub fn work_entry_indicates_tool_success(entry: &WorkLogEntry) -> bool {
    if !work_log_entry_is_tool_like(entry) {
        return false;
    }
    if work_entry_indicates_tool_failure(entry) {
        return false;
    }
    if is_thinking_tone(&entry.tone) {
        return false;
    }
    !matches!(
        entry.tool_lifecycle_status.as_deref(),
        Some("failed") | Some("declined") | Some("inProgress") | Some("stopped")
    )
}

/// `workEntryIndicatesToolNeutralStatus` (session-logic.ts).
pub fn work_entry_indicates_tool_neutral_status(entry: &WorkLogEntry) -> bool {
    work_log_entry_is_tool_like(entry)
        && !work_entry_indicates_tool_failure(entry)
        && !work_entry_indicates_tool_success(entry)
}

/// `formatDuration` (session-logic.ts).
pub fn format_duration(ms: i64) -> String {
    if ms < 0 {
        return "0ms".to_string();
    }
    if ms < 1_000 {
        return format!("{}ms", ms.max(1));
    }
    if ms < 10_000 {
        let tenths = ((ms as f64) / 100.0).round() as i64;
        // 9.95s+ rounds up to the next bucket — render "10s", not "10.0s".
        if tenths >= 100 {
            return "10s".to_string();
        }
        return format!("{}.{}s", tenths / 10, tenths % 10);
    }
    if ms < 60_000 {
        return format!("{}s", ((ms as f64) / 1_000.0).round() as i64);
    }
    let minutes = ms / 60_000;
    let seconds = (((ms % 60_000) as f64) / 1_000.0).round() as i64;
    if seconds == 0 {
        return format!("{minutes}m");
    }
    if seconds == 60 {
        return format!("{}m", minutes + 1);
    }
    format!("{minutes}m {seconds}s")
}

// ---------------------------------------------------------------------------
// Timeline rows (MessagesTimeline.logic.ts `deriveMessagesTimelineRows`)
// ---------------------------------------------------------------------------

/// `TimelineEntry` (session-logic.ts) without proposed plans, carrying the
/// caller-slice index for the row output.
enum TlEntry<'a> {
    Message {
        index: usize,
        message: &'a OrchestrationMessage,
    },
    Work {
        index: usize,
        entry: &'a WorkLogEntry,
    },
}

impl<'a> TlEntry<'a> {
    fn id(&self) -> &'a str {
        match self {
            TlEntry::Message { message, .. } => message.id.0.as_str(),
            TlEntry::Work { entry, .. } => entry.id.as_str(),
        }
    }

    fn created_at(&self) -> &'a str {
        match self {
            TlEntry::Message { message, .. } => message.created_at.0.as_str(),
            TlEntry::Work { entry, .. } => entry.created_at.as_str(),
        }
    }
}

/// `deriveTimelineEntries` (session-logic.ts): messages then work entries,
/// stably sorted by createdAt (plain string compare, like the TS
/// `localeCompare` over fixed-precision RFC3339 strings).
fn build_timeline_entries<'a>(
    messages: &'a [OrchestrationMessage],
    work_entries: &'a [WorkLogEntry],
) -> Vec<TlEntry<'a>> {
    let mut entries: Vec<TlEntry<'a>> = Vec::with_capacity(messages.len() + work_entries.len());
    for (index, message) in messages.iter().enumerate() {
        entries.push(TlEntry::Message { index, message });
    }
    for (index, entry) in work_entries.iter().enumerate() {
        entries.push(TlEntry::Work { index, entry });
    }
    entries.sort_by(|a, b| a.created_at().cmp(b.created_at()));
    entries
}

/// `deriveTerminalAssistantMessageIds` (MessagesTimeline.logic.ts): the last
/// assistant message per response key (turn id, or a per-user-message
/// counter for unkeyed responses).
fn derive_terminal_assistant_message_ids(entries: &[TlEntry]) -> HashSet<String> {
    let mut last_by_response_key: HashMap<String, String> = HashMap::new();
    let mut null_turn_response_index: u64 = 0;

    for entry in entries {
        let TlEntry::Message { message, .. } = entry else {
            continue;
        };
        match &message.role {
            OrchestrationMessageRole::User => {
                null_turn_response_index += 1;
            }
            OrchestrationMessageRole::Assistant => {
                let response_key = match &message.turn_id {
                    Some(turn_id) => format!("turn:{}", turn_id.0),
                    None => format!("unkeyed:{null_turn_response_index}"),
                };
                last_by_response_key.insert(response_key, message.id.0.clone());
            }
            _ => {}
        }
    }

    last_by_response_key.into_values().collect()
}

/// `deriveUnsettledTurnId` (MessagesTimeline.logic.ts): the running turn is
/// authoritative; otherwise the latest turn counts as unsettled until it has
/// a completion and is no longer running.
fn derive_unsettled_turn_id(
    latest_turn: Option<&OrchestrationLatestTurn>,
    running_turn_id: Option<&str>,
) -> Option<String> {
    if let Some(running) = running_turn_id {
        return Some(running.to_string());
    }
    let latest = latest_turn?;
    let is_settled = latest.completed_at.is_some()
        && !matches!(latest.state, OrchestrationLatestTurnState::Running);
    if is_settled {
        None
    } else {
        Some(latest.turn_id.0.clone())
    }
}

/// `computeElapsedMs` (MessagesTimeline.logic.ts).
fn compute_elapsed_ms(start_iso: &str, end_iso: &str) -> Option<i64> {
    let start = parse_timestamp_ms(start_iso)?;
    let end = parse_timestamp_ms(end_iso)?;
    Some((end - start).max(0))
}

/// `maxIsoTimestamp` (MessagesTimeline.logic.ts).
fn max_iso_timestamp<'a>(a: Option<&'a str>, b: Option<&'a str>) -> Option<&'a str> {
    let Some(a) = a else { return b };
    let Some(b) = b else { return Some(a) };
    let Some(a_ms) = parse_timestamp_ms(a) else {
        return Some(b);
    };
    let Some(b_ms) = parse_timestamp_ms(b) else {
        return Some(a);
    };
    Some(if b_ms > a_ms { b } else { a })
}

/// `TurnFold` (MessagesTimeline.logic.ts), keyed externally by its anchor
/// entry id.
struct TurnFold {
    turn_id: String,
    hidden_entry_ids: HashSet<String>,
    label: String,
}

/// `deriveTurnFolds` (MessagesTimeline.logic.ts): settled turns fold their
/// commentary and tool activity behind a "Worked for ..." row anchored at
/// the turn's first foldable entry; the terminal assistant message stays
/// visible below the fold.
fn derive_turn_folds(
    entries: &[TlEntry],
    terminal_assistant_message_ids: &HashSet<String>,
    latest_turn: Option<&OrchestrationLatestTurn>,
    unsettled_turn_id: Option<&str>,
) -> HashMap<String, TurnFold> {
    struct TurnGroup {
        entry_indices: Vec<usize>,
        terminal_entry_index: Option<usize>,
        has_streaming_message: bool,
        /// The user message that kicked the turn off; each user boundary
        /// starts at most one turn.
        start_boundary: Option<String>,
    }

    let mut groups: Vec<(String, TurnGroup)> = Vec::new();
    let mut group_index_by_turn: HashMap<String, usize> = HashMap::new();
    let mut pending_user_boundary: Option<String> = None;

    for (index, entry) in entries.iter().enumerate() {
        if let TlEntry::Message { message, .. } = entry
            && matches!(message.role, OrchestrationMessageRole::User)
        {
            pending_user_boundary = Some(message.created_at.0.clone());
            continue;
        }
        let turn_id: Option<&str> = match entry {
            TlEntry::Message { message, .. }
                if matches!(message.role, OrchestrationMessageRole::Assistant) =>
            {
                message.turn_id.as_ref().map(|turn| turn.0.as_str())
            }
            TlEntry::Work { entry, .. } => entry.turn_id.as_deref(),
            _ => None,
        };
        let Some(turn_id) = turn_id else {
            continue;
        };
        let group_index = match group_index_by_turn.get(turn_id) {
            Some(existing) => *existing,
            None => {
                groups.push((
                    turn_id.to_string(),
                    TurnGroup {
                        entry_indices: Vec::new(),
                        terminal_entry_index: None,
                        has_streaming_message: false,
                        start_boundary: pending_user_boundary.take(),
                    },
                ));
                group_index_by_turn.insert(turn_id.to_string(), groups.len() - 1);
                groups.len() - 1
            }
        };
        let group = &mut groups[group_index].1;
        group.entry_indices.push(index);
        if let TlEntry::Message { message, .. } = entry {
            if terminal_assistant_message_ids.contains(message.id.0.as_str()) {
                group.terminal_entry_index = Some(index);
            }
            if message.streaming {
                group.has_streaming_message = true;
            }
        }
    }

    let mut folds_by_anchor_entry_id: HashMap<String, TurnFold> = HashMap::new();
    for (turn_id, group) in &groups {
        if unsettled_turn_id == Some(turn_id.as_str()) {
            continue;
        }
        if group.has_streaming_message {
            continue;
        }
        let terminal_id: Option<&str> = group.terminal_entry_index.map(|i| entries[i].id());
        let mut hidden_entry_ids: HashSet<String> = HashSet::new();
        for &entry_index in &group.entry_indices {
            let entry_id = entries[entry_index].id();
            if Some(entry_id) != terminal_id {
                hidden_entry_ids.insert(entry_id.to_string());
            }
        }
        if hidden_entry_ids.is_empty() {
            continue;
        }

        let (Some(&first_index), Some(&last_index)) =
            (group.entry_indices.first(), group.entry_indices.last())
        else {
            continue;
        };
        let first = &entries[first_index];
        let last = &entries[last_index];

        let is_latest_interrupted_turn = latest_turn.is_some_and(|latest| {
            latest.turn_id.0 == *turn_id
                && matches!(latest.state, OrchestrationLatestTurnState::Interrupted)
        });
        // A turn cut short by a steer leaves trailing work entries behind its
        // terminal message — take whichever ended last.
        let last_entry_end: &str = match last {
            TlEntry::Message { message, .. } => message.updated_at.0.as_str(),
            TlEntry::Work { entry, .. } => entry.created_at.as_str(),
        };
        let latest_turn_timing = latest_turn
            .filter(|latest| latest.turn_id.0 == *turn_id)
            .and_then(|latest| latest.started_at.as_ref().zip(latest.completed_at.as_ref()));
        let elapsed_ms = match latest_turn_timing {
            Some((started_at, completed_at)) => compute_elapsed_ms(&started_at.0, &completed_at.0),
            None => {
                let start: &str = group
                    .start_boundary
                    .as_deref()
                    .unwrap_or_else(|| first.created_at());
                let terminal_updated_at: Option<&str> =
                    group.terminal_entry_index.and_then(|i| match &entries[i] {
                        TlEntry::Message { message, .. } => Some(message.updated_at.0.as_str()),
                        TlEntry::Work { .. } => None,
                    });
                let end = max_iso_timestamp(terminal_updated_at, Some(last_entry_end))
                    .unwrap_or(last_entry_end);
                compute_elapsed_ms(start, end)
            }
        };
        let duration = elapsed_ms.map(format_duration);
        let label = if is_latest_interrupted_turn {
            match &duration {
                Some(duration) => format!("You stopped after {duration}"),
                None => "You stopped this response".to_string(),
            }
        } else {
            match &duration {
                Some(duration) => format!("Worked for {duration}"),
                None => "Worked".to_string(),
            }
        };

        folds_by_anchor_entry_id.insert(
            first.id().to_string(),
            TurnFold {
                turn_id: turn_id.clone(),
                hidden_entry_ids,
                label,
            },
        );
    }
    folds_by_anchor_entry_id
}

/// `deriveMessagesTimelineRows` (MessagesTimeline.logic.ts): turn folds plus
/// consecutive-work-run grouping with `MAX_VISIBLE_WORK_LOG_ENTRIES`, and a
/// trailing working row.
pub fn derive_timeline_rows(input: &TimelineDeriveInput) -> Vec<DerivedTimelineRow> {
    let entries = build_timeline_entries(input.messages, input.work_entries);
    let terminal_assistant_message_ids = derive_terminal_assistant_message_ids(&entries);
    let unsettled_turn_id = derive_unsettled_turn_id(input.latest_turn, input.running_turn_id);
    let folds_by_anchor_entry_id = derive_turn_folds(
        &entries,
        &terminal_assistant_message_ids,
        input.latest_turn,
        unsettled_turn_id.as_deref(),
    );

    let mut collapsed_entry_ids: HashSet<&str> = HashSet::new();
    for fold in folds_by_anchor_entry_id.values() {
        if !input.expanded_turn_ids.contains(&fold.turn_id) {
            for entry_id in &fold.hidden_entry_ids {
                collapsed_entry_ids.insert(entry_id.as_str());
            }
        }
    }

    let mut rows: Vec<DerivedTimelineRow> = Vec::new();
    let mut index = 0;
    while index < entries.len() {
        let entry = &entries[index];

        if let Some(fold) = folds_by_anchor_entry_id.get(entry.id()) {
            rows.push(DerivedTimelineRow::TurnFold {
                turn_id: fold.turn_id.clone(),
                label: fold.label.clone(),
                expanded: input.expanded_turn_ids.contains(&fold.turn_id),
            });
        }

        if collapsed_entry_ids.contains(entry.id()) {
            index += 1;
            continue;
        }

        match entry {
            TlEntry::Work {
                index: entry_index,
                entry: work,
            } => {
                let mut grouped: Vec<(usize, &WorkLogEntry)> = vec![(*entry_index, work)];
                let mut cursor = index + 1;
                while cursor < entries.len() {
                    let next = &entries[cursor];
                    let TlEntry::Work {
                        index: next_index,
                        entry: next_work,
                    } = next
                    else {
                        break;
                    };
                    if collapsed_entry_ids.contains(next.id())
                        || folds_by_anchor_entry_id.contains_key(next.id())
                    {
                        break;
                    }
                    grouped.push((*next_index, next_work));
                    cursor += 1;
                }

                let visible: Vec<(usize, &WorkLogEntry)> = grouped
                    .iter()
                    .copied()
                    .filter(|(_, entry)| !work_entry_indicates_tool_neutral_status(entry))
                    .collect();
                if !visible.is_empty() {
                    if visible.len() <= MAX_VISIBLE_WORK_LOG_ENTRIES {
                        for (visible_index, _) in &visible {
                            rows.push(DerivedTimelineRow::Work {
                                entry_index: *visible_index,
                            });
                        }
                    } else {
                        let group_id = format!("work-group:{}", entry.id());
                        let expanded = input.expanded_work_group_ids.contains(&group_id);
                        let hidden = &visible[..visible.len() - MAX_VISIBLE_WORK_LOG_ENTRIES];
                        let shown = &visible[visible.len() - MAX_VISIBLE_WORK_LOG_ENTRIES..];
                        if expanded {
                            for (hidden_index, _) in hidden {
                                rows.push(DerivedTimelineRow::Work {
                                    entry_index: *hidden_index,
                                });
                            }
                        }
                        for (shown_index, _) in shown {
                            rows.push(DerivedTimelineRow::Work {
                                entry_index: *shown_index,
                            });
                        }
                        rows.push(DerivedTimelineRow::WorkToggle {
                            group_id,
                            hidden_count: hidden.len(),
                            expanded,
                            only_tool_entries: visible
                                .iter()
                                .all(|(_, entry)| work_log_entry_is_tool_like(entry)),
                        });
                    }
                }
                index = cursor;
                continue;
            }
            TlEntry::Message {
                index: message_index,
                ..
            } => {
                rows.push(DerivedTimelineRow::Message {
                    message_index: *message_index,
                });
            }
        }
        index += 1;
    }

    if input.is_working {
        rows.push(DerivedTimelineRow::Working);
    }

    rows
}

// ---------------------------------------------------------------------------
// Render helpers (MessagesTimeline.tsx)
// ---------------------------------------------------------------------------

/// `capitalizePhrase` (MessagesTimeline.tsx).
fn capitalize_phrase(value: &str) -> String {
    let trimmed = value.trim();
    let mut chars = trimmed.chars();
    match chars.next() {
        None => value.to_string(),
        Some(first) => {
            let mut result: String = first.to_uppercase().collect();
            result.push_str(chars.as_str());
            result
        }
    }
}

/// `toolWorkEntryHeading` (MessagesTimeline.tsx). The TS checks
/// `!workEntry.toolTitle`, so an empty-string title falls back to the label.
pub fn work_entry_heading(entry: &WorkLogEntry) -> String {
    let source = entry
        .tool_title
        .as_deref()
        .filter(|title| !title.is_empty())
        .unwrap_or(&entry.label);
    capitalize_phrase(&normalize_compact_tool_label(source))
}

/// `workEntryPreview` (MessagesTimeline.tsx), without a workspace root (the
/// caller-side path shortening applies in `build_tool_call_expanded_body`).
pub fn work_entry_preview(entry: &WorkLogEntry) -> Option<String> {
    if let Some(command) = entry.command.as_deref().filter(|c| !c.is_empty()) {
        return Some(command.to_string());
    }
    if let Some(detail) = entry.detail.as_deref().filter(|d| !d.is_empty()) {
        return Some(detail.to_string());
    }
    let first_path = entry.changed_files.first()?;
    if first_path.is_empty() {
        return None;
    }
    let display_path = format_workspace_relative_path(first_path, None);
    if entry.changed_files.len() == 1 {
        Some(display_path)
    } else {
        Some(format!(
            "{display_path} +{} more",
            entry.changed_files.len() - 1
        ))
    }
}

/// `workEntryRawCommand` (MessagesTimeline.tsx).
fn work_entry_raw_command(entry: &WorkLogEntry) -> Option<String> {
    let raw_command = entry.raw_command.as_deref()?.trim();
    if raw_command.is_empty() {
        return None;
    }
    let command = entry.command.as_deref().filter(|c| !c.is_empty())?;
    if raw_command == command.trim() {
        None
    } else {
        Some(raw_command.to_string())
    }
}

/// `buildToolCallExpandedBody` (MessagesTimeline.tsx).
pub fn build_tool_call_expanded_body(
    entry: &WorkLogEntry,
    workspace_root: Option<&str>,
) -> Option<String> {
    let mut blocks: Vec<String> = Vec::new();
    if entry.item_type.as_deref() == Some("mcp_tool_call")
        && let Some(tool_data) = &entry.tool_data
    {
        let json =
            serde_json::to_string_pretty(tool_data).unwrap_or_else(|_| tool_data.to_string());
        blocks.push(format!("MCP call\n{json}"));
    }
    let raw_command = work_entry_raw_command(entry);
    if let Some(raw_command) = raw_command
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        blocks.push(raw_command.to_string());
    } else if let Some(command) = entry
        .command
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        blocks.push(command.to_string());
    }
    if let Some(detail) = entry
        .detail
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        blocks.push(detail.to_string());
    }
    if !entry.changed_files.is_empty() {
        blocks.push(
            entry
                .changed_files
                .iter()
                .map(|file_path| format_workspace_relative_path(file_path, workspace_root))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    if blocks.is_empty() {
        None
    } else {
        Some(blocks.join("\n\n"))
    }
}

/// `workEntryIconName` + `workToneIcon` (MessagesTimeline.tsx): the lucide
/// icon name for a work row.
pub fn work_entry_icon_name(entry: &WorkLogEntry) -> &'static str {
    if entry.kind == "user-input.requested" || entry.kind == "user-input.resolved" {
        return "message-circle";
    }
    match entry.request_kind.as_deref() {
        Some("command") => return "terminal",
        Some("file-read") => return "eye",
        Some("file-change") => return "square-pen",
        _ => {}
    }
    let item_type = entry.item_type.as_deref();
    let has_command = entry.command.as_deref().is_some_and(|c| !c.is_empty());
    if item_type == Some("command_execution") || has_command {
        return "terminal";
    }
    if item_type == Some("file_change") || !entry.changed_files.is_empty() {
        return "square-pen";
    }
    match item_type {
        Some("web_search") => return "globe",
        Some("image_view") => return "eye",
        Some("mcp_tool_call") => return "wrench",
        Some("dynamic_tool_call") | Some("collab_agent_tool_call") => return "hammer",
        _ => {}
    }
    if matches!(entry.tone, OrchestrationThreadActivityTone::Error) {
        return "circle-alert";
    }
    if is_thinking_tone(&entry.tone) {
        return "bot";
    }
    if matches!(entry.tone, OrchestrationThreadActivityTone::Info) {
        return "check";
    }
    "zap"
}

// ---------------------------------------------------------------------------
// Workspace-relative paths (apps/web/src/filePathDisplay.ts +
// apps/web/src/terminal-links.ts `splitPathAndPosition`)
// ---------------------------------------------------------------------------

struct PathPosition<'a> {
    path: &'a str,
    line: Option<&'a str>,
    column: Option<&'a str>,
    end_line: Option<&'a str>,
}

/// Trailing `:(\d+)` split helper for `splitPathAndPosition`.
fn split_trailing_colon_digits(value: &str) -> Option<(&str, &str)> {
    let bytes = value.as_bytes();
    let mut index = bytes.len();
    while index > 0 && bytes[index - 1].is_ascii_digit() {
        index -= 1;
    }
    if index == bytes.len() || index == 0 || bytes[index - 1] != b':' {
        return None;
    }
    Some((&value[..index - 1], &value[index..]))
}

/// Trailing `:(\d+)[-–](\d+)` range for `splitPathAndPosition`.
fn split_trailing_line_range(value: &str) -> Option<(&str, &str, &str)> {
    let bytes = value.as_bytes();
    let mut index = bytes.len();
    while index > 0 && bytes[index - 1].is_ascii_digit() {
        index -= 1;
    }
    if index == bytes.len() {
        return None;
    }
    let end_line = &value[index..];
    let head = &value[..index];
    let separator_start = if head.ends_with('-') {
        index - 1
    } else if head.ends_with('–') {
        index - '–'.len_utf8()
    } else {
        return None;
    };
    let (path, line) = split_trailing_colon_digits(&value[..separator_start])?;
    // split_trailing_colon_digits already guarantees ≥1 digit and the colon.
    Some((path, line, end_line))
}

/// `splitPathAndPosition` (terminal-links.ts).
fn split_path_and_position(value: &str) -> PathPosition<'_> {
    if let Some((path, line, end_line)) = split_trailing_line_range(value) {
        return PathPosition {
            path,
            line: Some(line),
            column: None,
            end_line: Some(end_line),
        };
    }
    let Some((rest, column)) = split_trailing_colon_digits(value) else {
        return PathPosition {
            path: value,
            line: None,
            column: None,
            end_line: None,
        };
    };
    if let Some((path, line)) = split_trailing_colon_digits(rest) {
        PathPosition {
            path,
            line: Some(line),
            column: Some(column),
            end_line: None,
        }
    } else {
        PathPosition {
            path: rest,
            line: Some(column),
            column: None,
            end_line: None,
        }
    }
}

/// `canonicalizeWindowsDrivePath` (filePathDisplay.ts): `/C:/…` → `C:/…`.
fn canonicalize_windows_drive_path(path: &str) -> &str {
    let bytes = path.as_bytes();
    if bytes.len() >= 4
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && bytes[3] == b'/'
    {
        &path[1..]
    } else {
        path
    }
}

/// `basenameOfPath` (filePathDisplay.ts).
fn basename_of_path(path: &str) -> &str {
    match path.rfind(['/', '\\']) {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

/// `stripRelativePrefixes` (filePathDisplay.ts): `^\.\/+` then `^\/+`.
fn strip_relative_prefixes(path: &str) -> &str {
    let after_dot = match path.strip_prefix('.') {
        Some(rest) if rest.starts_with('/') => rest,
        _ => path,
    };
    after_dot.trim_start_matches('/')
}

/// `formatWorkspaceRelativePath` (filePathDisplay.ts).
fn format_workspace_relative_path(
    path_with_position: &str,
    workspace_root: Option<&str>,
) -> String {
    let position = split_path_and_position(path_with_position);
    let normalized_path =
        canonicalize_windows_drive_path(&position.path.replace('\\', "/")).to_string();

    let mut display_path = normalized_path.clone();
    // TS `if (workspaceRoot)`: an empty root is falsy and applies no labeling.
    if let Some(workspace_root) = workspace_root.filter(|root| !root.is_empty()) {
        let trimmed_root = workspace_root
            .trim_end_matches(['/', '\\'])
            .replace('\\', "/");
        let normalized_root = canonicalize_windows_drive_path(&trimmed_root).to_string();
        let workspace_label = basename_of_path(&normalized_root).to_string();
        let path_for_compare = normalized_path.to_lowercase();
        let workspace_for_compare = normalized_root.to_lowercase();
        let workspace_with_separator = format!("{workspace_for_compare}/");
        let workspace_label_with_separator = format!("{}/", workspace_label.to_lowercase());

        if path_for_compare == workspace_for_compare {
            display_path = workspace_label.clone();
        } else if path_for_compare.starts_with(&workspace_with_separator) {
            // The TS slices the original-case path by the root's length; guard
            // the byte offset in case lowercasing changed lengths.
            let cut = normalized_root.len() + 1;
            if normalized_path.len() >= cut && normalized_path.is_char_boundary(cut) {
                display_path = format!("{workspace_label}/{}", &normalized_path[cut..]);
            }
        } else if !normalized_path.starts_with('/') {
            let relative_path = strip_relative_prefixes(&normalized_path);
            display_path = if path_for_compare.starts_with(&workspace_label_with_separator) {
                normalized_path.clone()
            } else {
                format!("{workspace_label}/{relative_path}")
            };
        }
    }

    match (position.line, position.end_line, position.column) {
        (None, _, _) => display_path,
        (Some(line), Some(end_line), _) => format!("{display_path}:{line}-{end_line}"),
        (Some(line), None, Some(column)) => format!("{display_path}:{line}:{column}"),
        (Some(line), None, None) => format!("{display_path}:{line}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vitre_contracts::{EventId, MessageId, NonNegativeInt, TrimmedNonEmptyString, TurnId};

    const T0: &str = "2026-01-01T00:00:00.000Z";
    const T1: &str = "2026-01-01T00:00:01.000Z";
    const T2: &str = "2026-01-01T00:00:02.000Z";
    const T3: &str = "2026-01-01T00:00:03.000Z";
    const T4: &str = "2026-01-01T00:00:04.000Z";
    const T5: &str = "2026-01-01T00:00:05.000Z";
    const T6: &str = "2026-01-01T00:00:06.000Z";
    const T7: &str = "2026-01-01T00:00:07.000Z";

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

    fn message(
        id: &str,
        role: OrchestrationMessageRole,
        created_at: &str,
        turn_id: Option<&str>,
    ) -> OrchestrationMessage {
        OrchestrationMessage {
            attachments: None,
            created_at: TrimmedNonEmptyString(created_at.into()),
            id: MessageId(id.into()),
            role,
            streaming: false,
            text: TrimmedNonEmptyString("text".into()),
            turn_id: turn_id.map(|turn| TurnId(turn.into())),
            updated_at: TrimmedNonEmptyString(created_at.into()),
        }
    }

    fn latest_turn(
        turn_id: &str,
        state: OrchestrationLatestTurnState,
        started_at: Option<&str>,
        completed_at: Option<&str>,
    ) -> OrchestrationLatestTurn {
        OrchestrationLatestTurn {
            assistant_message_id: None,
            completed_at: completed_at.map(|at| TrimmedNonEmptyString(at.into())),
            requested_at: TrimmedNonEmptyString(T0.into()),
            source_proposed_plan: None,
            started_at: started_at.map(|at| TrimmedNonEmptyString(at.into())),
            state,
            turn_id: TurnId(turn_id.into()),
        }
    }

    fn work_entry(id: &str, created_at: &str) -> WorkLogEntry {
        WorkLogEntry {
            id: id.into(),
            created_at: created_at.into(),
            turn_id: None,
            label: "Ran command".into(),
            tone: OrchestrationThreadActivityTone::Tool,
            kind: "tool.completed".into(),
            detail: None,
            command: None,
            raw_command: None,
            tool_title: None,
            item_type: None,
            request_kind: None,
            tool_call_id: None,
            tool_lifecycle_status: None,
            collapse_key: String::new(),
            changed_files: Vec::new(),
            tool_data: None,
        }
    }

    fn derive_rows_with(
        messages: &[OrchestrationMessage],
        work_entries: &[WorkLogEntry],
        latest_turn: Option<&OrchestrationLatestTurn>,
        running_turn_id: Option<&str>,
        expanded_turn_ids: &[&str],
        expanded_work_group_ids: &[&str],
        is_working: bool,
    ) -> Vec<DerivedTimelineRow> {
        let expanded_turn_ids: HashSet<String> = expanded_turn_ids
            .iter()
            .map(|id| (*id).to_string())
            .collect();
        let expanded_work_group_ids: HashSet<String> = expanded_work_group_ids
            .iter()
            .map(|id| (*id).to_string())
            .collect();
        derive_timeline_rows(&TimelineDeriveInput {
            messages,
            work_entries,
            latest_turn,
            running_turn_id,
            expanded_turn_ids: &expanded_turn_ids,
            expanded_work_group_ids: &expanded_work_group_ids,
            is_working,
        })
    }

    // -----------------------------------------------------------------------
    // deriveWorkLogEntries: filtering
    // -----------------------------------------------------------------------

    #[test]
    fn work_log_drops_lifecycle_noise_and_checkpoints() {
        let mut checkpoint = activity("a4", "turn.checkpoint", T3, json!({}));
        checkpoint.summary = TrimmedNonEmptyString("Checkpoint captured".into());
        let activities = vec![
            activity("a1", "tool.started", T0, json!({})),
            activity("a2", "task.started", T1, json!({})),
            activity("a3", "context-window.updated", T2, json!({})),
            checkpoint,
            activity("a5", "turn.completed", T4, json!({})),
        ];
        let entries = derive_work_log_entries(&activities);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "a5");
    }

    #[test]
    fn work_log_drops_exit_plan_mode_boundary_tool_activities() {
        let activities = vec![
            activity(
                "a1",
                "tool.updated",
                T0,
                json!({"detail": "ExitPlanMode: plan ready"}),
            ),
            activity(
                "a2",
                "tool.completed",
                T1,
                json!({"detail": "ExitPlanMode: plan ready"}),
            ),
            // Non-tool kinds keep an ExitPlanMode-shaped detail.
            activity(
                "a3",
                "task.progress",
                T2,
                json!({"detail": "ExitPlanMode: kept"}),
            ),
        ];
        let entries = derive_work_log_entries(&activities);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "a3");
        assert_eq!(entries[0].label, "ExitPlanMode: kept");
    }

    // -----------------------------------------------------------------------
    // Activity ordering
    // -----------------------------------------------------------------------

    #[test]
    fn ordering_sequence_dominates_created_at() {
        let mut low = activity("a1", "turn.completed", T5, json!({}));
        low.sequence = Some(Some(NonNegativeInt(3)));
        let mut high = activity("a2", "turn.completed", T0, json!({}));
        high.sequence = Some(Some(NonNegativeInt(5)));
        assert_eq!(
            compare_activities_by_order(&low, &high),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_activities_by_order(&high, &low),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn ordering_unsequenced_sorts_before_sequenced() {
        let unsequenced = activity("a1", "turn.completed", T5, json!({}));
        let mut sequenced = activity("a2", "turn.completed", T0, json!({}));
        sequenced.sequence = Some(Some(NonNegativeInt(1)));
        assert_eq!(
            compare_activities_by_order(&unsequenced, &sequenced),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_activities_by_order(&sequenced, &unsequenced),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn ordering_same_timestamp_uses_lifecycle_rank_then_id() {
        // Same createdAt: `.updated` (rank 1) precedes `.completed` (rank 2)
        // despite the larger id.
        let updated = activity("z9", "note.updated", T1, json!({}));
        let completed = activity("a1", "note.completed", T1, json!({}));
        assert_eq!(
            compare_activities_by_order(&updated, &completed),
            std::cmp::Ordering::Less
        );

        // Same createdAt + rank: id breaks the tie.
        let first = activity("a1", "note.updated", T1, json!({}));
        let second = activity("b2", "note.updated", T1, json!({}));
        assert_eq!(
            compare_activities_by_order(&first, &second),
            std::cmp::Ordering::Less
        );

        // End-to-end through derive_work_log_entries.
        let activities = vec![
            activity("a3", "note.completed", T1, json!({})),
            activity("a1", "note.updated", T1, json!({})),
            activity("a2", "turn.completed", T0, json!({})),
        ];
        let ids: Vec<String> = derive_work_log_entries(&activities)
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        assert_eq!(ids, vec!["a2", "a1", "a3"]);
    }

    // -----------------------------------------------------------------------
    // deriveWorkLogEntries: tool lifecycle collapse
    // -----------------------------------------------------------------------

    #[test]
    fn merges_adjacent_tool_lifecycle_entries_by_tool_call_id() {
        let activities = vec![
            activity(
                "a1",
                "tool.updated",
                T1,
                json!({"data": {"toolCallId": "c1"}, "detail": "running step"}),
            ),
            activity(
                "a2",
                "tool.completed",
                T2,
                json!({"data": {"toolCallId": "c1"}}),
            ),
        ];
        let entries = derive_work_log_entries(&activities);
        assert_eq!(entries.len(), 1);
        let merged = &entries[0];
        // The newer activity wins id/createdAt/kind …
        assert_eq!(merged.id, "a2");
        assert_eq!(merged.created_at, T2);
        assert_eq!(merged.kind, "tool.completed");
        // … with next ?? previous fallbacks for the rest.
        assert_eq!(merged.detail.as_deref(), Some("running step"));
        assert_eq!(merged.tool_call_id.as_deref(), Some("c1"));
        assert_eq!(merged.tool_lifecycle_status.as_deref(), Some("completed"));
        assert_eq!(merged.collapse_key, "tool:c1");
    }

    #[test]
    fn completed_entry_never_absorbs_follower() {
        let activities = vec![
            activity(
                "a1",
                "tool.completed",
                T1,
                json!({"data": {"toolCallId": "c1"}}),
            ),
            activity(
                "a2",
                "tool.completed",
                T2,
                json!({"data": {"toolCallId": "c1"}}),
            ),
        ];
        assert_eq!(derive_work_log_entries(&activities).len(), 2);
    }

    #[test]
    fn non_adjacent_tool_entries_do_not_merge() {
        let activities = vec![
            activity(
                "a1",
                "tool.updated",
                T1,
                json!({"data": {"toolCallId": "c1"}}),
            ),
            activity("a2", "turn.note", T2, json!({})),
            activity(
                "a3",
                "tool.completed",
                T3,
                json!({"data": {"toolCallId": "c1"}}),
            ),
        ];
        let ids: Vec<String> = derive_work_log_entries(&activities)
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        assert_eq!(ids, vec!["a1", "a2", "a3"]);
    }

    #[test]
    fn merges_by_item_type_label_detail_collapse_key() {
        // No toolCallId: the key is itemType + normalized label + detail, and
        // normalization strips the trailing "completed" on the follower.
        let activities = vec![
            activity(
                "a1",
                "tool.updated",
                T1,
                json!({"itemType": "web_search", "title": "Searching docs"}),
            ),
            activity(
                "a2",
                "tool.completed",
                T2,
                json!({"itemType": "web_search", "title": "Searching docs completed"}),
            ),
        ];
        let entries = derive_work_log_entries(&activities);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "a2");
        assert_eq!(
            entries[0].tool_title.as_deref(),
            Some("Searching docs completed")
        );
        assert_eq!(entries[0].item_type.as_deref(), Some("web_search"));
        assert_eq!(
            entries[0].tool_lifecycle_status.as_deref(),
            Some("completed")
        );
    }

    #[test]
    fn merges_keyed_previous_with_unkeyed_follower_on_matching_label() {
        // previous has a toolCallId, follower doesn't: matching itemType +
        // normalized label still collapses, keeping the previous toolCallId.
        let activities = vec![
            activity(
                "a1",
                "tool.updated",
                T1,
                json!({"title": "Fetch data", "data": {"toolCallId": "c9"}}),
            ),
            activity(
                "a2",
                "tool.completed",
                T2,
                json!({"title": "Fetch data complete"}),
            ),
        ];
        let entries = derive_work_log_entries(&activities);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "a2");
        assert_eq!(entries[0].tool_call_id.as_deref(), Some("c9"));
    }

    #[test]
    fn merge_dedupes_changed_files_keeping_first_occurrence_order() {
        let activities = vec![
            activity(
                "a1",
                "tool.updated",
                T1,
                json!({"data": {"toolCallId": "c2", "files": [{"path": "a.rs"}, {"path": "b.rs"}]}}),
            ),
            activity(
                "a2",
                "tool.completed",
                T2,
                json!({"data": {"toolCallId": "c2", "files": [{"path": "b.rs"}, {"path": "c.rs"}]}}),
            ),
        ];
        let entries = derive_work_log_entries(&activities);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].changed_files, vec!["a.rs", "b.rs", "c.rs"]);
    }

    // -----------------------------------------------------------------------
    // deriveWorkLogEntries: entry mapping
    // -----------------------------------------------------------------------

    #[test]
    fn task_progress_maps_summary_label_thinking_tone_and_detail() {
        let mut act = activity(
            "a1",
            "task.progress",
            T0,
            json!({
                "summary": "Analyzing repo",
                "detail": "step one <exited with exit code 0>",
                "data": {"toolCallId": "tc-1"},
            }),
        );
        act.summary = TrimmedNonEmptyString("Working".into());
        let entries = derive_work_log_entries(&[act]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "Analyzing repo");
        assert_eq!(entries[0].tone, thinking_tone());
        assert_eq!(entries[0].detail.as_deref(), Some("step one"));
        // Task activities never carry a toolCallId.
        assert_eq!(entries[0].tool_call_id, None);
    }

    #[test]
    fn task_detail_becomes_label_when_summary_missing() {
        let entries = derive_work_log_entries(&[activity(
            "a1",
            "task.completed",
            T0,
            json!({"detail": "All done"}),
        )]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "All done");
        assert_eq!(entries[0].detail, None);
        // Only task.progress gets the synthetic thinking tone.
        assert_eq!(entries[0].tone, OrchestrationThreadActivityTone::Tool);
    }

    #[test]
    fn approval_tone_maps_to_info_and_request_kind_from_request_type() {
        let mut act = activity(
            "a1",
            "approval.requested",
            T0,
            json!({"requestType": "exec_command_approval"}),
        );
        act.tone = OrchestrationThreadActivityTone::Approval;
        let entries = derive_work_log_entries(&[act]);
        assert_eq!(entries[0].tone, OrchestrationThreadActivityTone::Info);
        assert_eq!(entries[0].request_kind.as_deref(), Some("command"));
    }

    #[test]
    fn tool_lifecycle_status_extraction_and_completed_default() {
        let completed = derive_work_log_entries(&[activity("a1", "tool.completed", T0, json!({}))]);
        assert_eq!(
            completed[0].tool_lifecycle_status.as_deref(),
            Some("completed")
        );

        let updated = derive_work_log_entries(&[activity(
            "a1",
            "tool.updated",
            T0,
            json!({"status": "inProgress"}),
        )]);
        assert_eq!(
            updated[0].tool_lifecycle_status.as_deref(),
            Some("inProgress")
        );

        let bogus = derive_work_log_entries(&[activity(
            "a1",
            "tool.updated",
            T0,
            json!({"status": "bogus"}),
        )]);
        assert_eq!(bogus[0].tool_lifecycle_status, None);
    }

    #[test]
    fn command_unwraps_shell_wrapper_and_keeps_raw_command() {
        let entries = derive_work_log_entries(&[activity(
            "a1",
            "tool.updated",
            T0,
            json!({"data": {"item": {"command": "bash -lc \"cargo test --all\""}}}),
        )]);
        assert_eq!(entries[0].command.as_deref(), Some("cargo test --all"));
        assert_eq!(
            entries[0].raw_command.as_deref(),
            Some("bash -lc \"cargo test --all\"")
        );
    }

    #[test]
    fn array_command_parts_join_with_quoting() {
        let entries = derive_work_log_entries(&[activity(
            "a1",
            "tool.updated",
            T0,
            json!({"data": {"command": ["git", "commit", "-m", "hello world"]}}),
        )]);
        assert_eq!(
            entries[0].command.as_deref(),
            Some("git commit -m \"hello world\"")
        );
        // No shell wrapper was removed, so there is no separate raw command.
        assert_eq!(entries[0].raw_command, None);
    }

    #[test]
    fn command_execution_detail_supplies_command_and_strips_exit_code() {
        let entries = derive_work_log_entries(&[activity(
            "a1",
            "tool.updated",
            T0,
            json!({"itemType": "command_execution", "detail": "ls -la <exited with exit code 0>"}),
        )]);
        assert_eq!(entries[0].command.as_deref(), Some("ls -la"));
        assert_eq!(entries[0].detail.as_deref(), Some("ls -la"));
        assert_eq!(entries[0].item_type.as_deref(), Some("command_execution"));
    }

    #[test]
    fn detail_matching_heading_is_suppressed() {
        let entries = derive_work_log_entries(&[activity(
            "a1",
            "tool.updated",
            T0,
            json!({"title": "Read File", "detail": "read file"}),
        )]);
        assert_eq!(entries[0].tool_title.as_deref(), Some("Read File"));
        assert_eq!(entries[0].detail, None);
    }

    #[test]
    fn raw_output_summaries_fill_missing_detail() {
        let files = derive_work_log_entries(&[activity(
            "a1",
            "tool.updated",
            T0,
            json!({"title": "Search", "data": {"rawOutput": {"totalFiles": 1234, "truncated": true}}}),
        )]);
        assert_eq!(files[0].detail.as_deref(), Some("1,234 files+"));

        let single = derive_work_log_entries(&[activity(
            "a1",
            "tool.updated",
            T0,
            json!({"title": "Search", "data": {"rawOutput": {"totalFiles": 1}}}),
        )]);
        assert_eq!(single[0].detail.as_deref(), Some("1 file"));

        let content = derive_work_log_entries(&[activity(
            "a1",
            "tool.updated",
            T0,
            json!({"title": "Read", "data": {"rawOutput": {"content": "```\nfirst  line here\nsecond"}}}),
        )]);
        assert_eq!(content[0].detail.as_deref(), Some("first line here"));

        // A summary that normalizes to the heading is suppressed.
        let matching = derive_work_log_entries(&[activity(
            "a1",
            "tool.updated",
            T0,
            json!({"title": "3 files", "data": {"rawOutput": {"totalFiles": 3}}}),
        )]);
        assert_eq!(matching[0].detail, None);

        // Long first lines are truncated to 84 chars with an ellipsis.
        let long_line = "a".repeat(100);
        let truncated = derive_work_log_entries(&[activity(
            "a1",
            "tool.updated",
            T0,
            json!({"title": "Read", "data": {"rawOutput": {"content": long_line}}}),
        )]);
        assert_eq!(
            truncated[0].detail.as_deref(),
            Some(format!("{}…", "a".repeat(83)).as_str())
        );
    }

    #[test]
    fn changed_files_collected_from_nested_payload() {
        let entries = derive_work_log_entries(&[activity(
            "a1",
            "tool.updated",
            T0,
            json!({"data": {"item": {
                "changes": [{"path": "a.rs"}, {"filePath": "b.rs"}],
                "result": {"files": [{"path": "a.rs"}, {"newPath": "c.rs"}]},
            }}}),
        )]);
        // `result` is walked before `changes` (fixed nested-key order), and
        // duplicates keep their first occurrence.
        assert_eq!(entries[0].changed_files, vec!["a.rs", "c.rs", "b.rs"]);
    }

    // -----------------------------------------------------------------------
    // normalizeCompactToolLabel
    // -----------------------------------------------------------------------

    #[test]
    fn normalize_compact_tool_label_strips_trailing_completion_words() {
        assert_eq!(
            normalize_compact_tool_label("Read file complete"),
            "Read file"
        );
        assert_eq!(
            normalize_compact_tool_label("Read file COMPLETED"),
            "Read file"
        );
        assert_eq!(
            normalize_compact_tool_label("Read file completed   "),
            "Read file"
        );
        assert_eq!(
            normalize_compact_tool_label("  Search   completed"),
            "Search"
        );
        // Only one trailing word is stripped.
        assert_eq!(
            normalize_compact_tool_label("Search complete completed"),
            "Search complete"
        );
        // The word alone (no preceding whitespace) is preserved.
        assert_eq!(normalize_compact_tool_label("complete"), "complete");
        assert_eq!(normalize_compact_tool_label("completed"), "completed");
        assert_eq!(normalize_compact_tool_label("Xcompleted"), "Xcompleted");
        assert_eq!(normalize_compact_tool_label("  padded  "), "padded");
    }

    // -----------------------------------------------------------------------
    // formatDuration
    // -----------------------------------------------------------------------

    #[test]
    fn format_duration_matches_web_buckets() {
        assert_eq!(format_duration(-5), "0ms");
        assert_eq!(format_duration(0), "1ms");
        assert_eq!(format_duration(999), "999ms");
        assert_eq!(format_duration(1_000), "1.0s");
        assert_eq!(format_duration(3_440), "3.4s");
        assert_eq!(format_duration(9_940), "9.9s");
        // 9.95s+ rounds into the next bucket.
        assert_eq!(format_duration(9_950), "10s");
        assert_eq!(format_duration(10_000), "10s");
        assert_eq!(format_duration(59_499), "59s");
        assert_eq!(format_duration(59_500), "60s");
        assert_eq!(format_duration(60_000), "1m");
        assert_eq!(format_duration(61_000), "1m 1s");
        assert_eq!(format_duration(90_000), "1m 30s");
        // 1m 59.5s rounds the second component to 60 → next minute.
        assert_eq!(format_duration(119_500), "2m");
        assert_eq!(format_duration(150_000), "2m 30s");
    }

    // -----------------------------------------------------------------------
    // Tool status predicates
    // -----------------------------------------------------------------------

    #[test]
    fn tool_like_detection_matches_web() {
        let tool = work_entry("w", T0);
        assert!(work_log_entry_is_tool_like(&tool));

        let mut thinking = work_entry("w", T0);
        thinking.tone = thinking_tone();
        assert!(work_log_entry_is_tool_like(&thinking));

        let mut error = work_entry("w", T0);
        error.tone = OrchestrationThreadActivityTone::Error;
        assert!(work_log_entry_is_tool_like(&error));

        let mut info = work_entry("w", T0);
        info.tone = OrchestrationThreadActivityTone::Info;
        assert!(!work_log_entry_is_tool_like(&info));

        let mut with_command = work_entry("w", T0);
        with_command.tone = OrchestrationThreadActivityTone::Info;
        with_command.command = Some("ls".into());
        assert!(work_log_entry_is_tool_like(&with_command));

        let mut blank_command = work_entry("w", T0);
        blank_command.tone = OrchestrationThreadActivityTone::Info;
        blank_command.command = Some("   ".into());
        assert!(!work_log_entry_is_tool_like(&blank_command));

        let mut with_request = work_entry("w", T0);
        with_request.tone = OrchestrationThreadActivityTone::Info;
        with_request.request_kind = Some("command".into());
        assert!(work_log_entry_is_tool_like(&with_request));

        let mut with_item_type = work_entry("w", T0);
        with_item_type.tone = OrchestrationThreadActivityTone::Info;
        with_item_type.item_type = Some("web_search".into());
        assert!(work_log_entry_is_tool_like(&with_item_type));

        let mut bogus_item_type = work_entry("w", T0);
        bogus_item_type.tone = OrchestrationThreadActivityTone::Info;
        bogus_item_type.item_type = Some("bogus".into());
        assert!(!work_log_entry_is_tool_like(&bogus_item_type));
    }

    #[test]
    fn failure_predicate_covers_tone_status_and_output_heuristics() {
        let mut error_tone = work_entry("w", T0);
        error_tone.tone = OrchestrationThreadActivityTone::Error;
        assert!(work_entry_indicates_tool_failure(&error_tone));

        // Lifecycle failed/declined trip the predicate even for non-tool rows.
        let mut failed = work_entry("w", T0);
        failed.tone = OrchestrationThreadActivityTone::Info;
        failed.tool_lifecycle_status = Some("failed".into());
        assert!(work_entry_indicates_tool_failure(&failed));

        let mut declined = work_entry("w", T0);
        declined.tool_lifecycle_status = Some("declined".into());
        assert!(work_entry_indicates_tool_failure(&declined));

        let failure_details = [
            "npm ERR! command not found",
            "ENOENT: no such file or directory",
            "<exited with exit code 2>",
            "finished with exit code: 3",
            "process exited with exit code 5",
        ];
        for detail in failure_details {
            let mut entry = work_entry("w", T0);
            entry.detail = Some(detail.into());
            assert!(
                work_entry_indicates_tool_failure(&entry),
                "detail: {detail}"
            );
        }

        let benign_details = ["<exited with exit code 0>", "exit code: 0", "exit codes 12"];
        for detail in benign_details {
            let mut entry = work_entry("w", T0);
            entry.detail = Some(detail.into());
            assert!(
                !work_entry_indicates_tool_failure(&entry),
                "detail: {detail}"
            );
        }

        // Command text participates in the failure blob.
        let mut command_failure = work_entry("w", T0);
        command_failure.command = Some("cat missing.txt".into());
        command_failure.detail = Some("No such file or directory".into());
        assert!(work_entry_indicates_tool_failure(&command_failure));

        // A non-tool row without failure status never fails on text alone.
        let mut non_tool = work_entry("w", T0);
        non_tool.tone = OrchestrationThreadActivityTone::Info;
        non_tool.detail = Some("command not found".into());
        assert!(!work_entry_indicates_tool_failure(&non_tool));
    }

    #[test]
    fn success_predicate_requires_settled_non_failed_tool_row() {
        let completed = {
            let mut entry = work_entry("w", T0);
            entry.tool_lifecycle_status = Some("completed".into());
            entry
        };
        assert!(work_entry_indicates_tool_success(&completed));

        // No lifecycle status still counts as success for a tool row.
        assert!(work_entry_indicates_tool_success(&work_entry("w", T0)));

        let mut thinking = work_entry("w", T0);
        thinking.tone = thinking_tone();
        assert!(!work_entry_indicates_tool_success(&thinking));

        for status in ["inProgress", "stopped", "failed", "declined"] {
            let mut entry = work_entry("w", T0);
            entry.tool_lifecycle_status = Some(status.into());
            assert!(
                !work_entry_indicates_tool_success(&entry),
                "status: {status}"
            );
        }

        let mut failure_text = work_entry("w", T0);
        failure_text.detail = Some("command not found".into());
        assert!(!work_entry_indicates_tool_success(&failure_text));
    }

    #[test]
    fn neutral_predicate_is_tool_like_without_success_or_failure() {
        let mut in_progress = work_entry("w", T0);
        in_progress.tool_lifecycle_status = Some("inProgress".into());
        assert!(work_entry_indicates_tool_neutral_status(&in_progress));

        let mut thinking = work_entry("w", T0);
        thinking.tone = thinking_tone();
        assert!(work_entry_indicates_tool_neutral_status(&thinking));

        // Plain tool row counts as success, so it is not neutral.
        assert!(!work_entry_indicates_tool_neutral_status(&work_entry(
            "w", T0
        )));

        // Non-tool rows are never neutral.
        let mut info = work_entry("w", T0);
        info.tone = OrchestrationThreadActivityTone::Info;
        assert!(!work_entry_indicates_tool_neutral_status(&info));
    }

    // -----------------------------------------------------------------------
    // Timeline rows: turn folding
    // -----------------------------------------------------------------------

    /// user (T0) → work w1 in T1 (T2) → assistant a1 in T1 (T3) → terminal
    /// assistant a2 in T1 (T4).
    fn fold_scenario() -> (Vec<OrchestrationMessage>, Vec<WorkLogEntry>) {
        let messages = vec![
            message("m1", OrchestrationMessageRole::User, T0, None),
            message("a1", OrchestrationMessageRole::Assistant, T3, Some("T1")),
            message("a2", OrchestrationMessageRole::Assistant, T4, Some("T1")),
        ];
        let mut w1 = work_entry("w1", T2);
        w1.turn_id = Some("T1".into());
        (messages, vec![w1])
    }

    #[test]
    fn settled_turn_folds_behind_worked_row() {
        let (messages, work) = fold_scenario();
        let latest = latest_turn(
            "T1",
            OrchestrationLatestTurnState::Completed,
            Some(T1),
            Some(T6),
        );
        let rows = derive_rows_with(&messages, &work, Some(&latest), None, &[], &[], false);
        assert_eq!(
            rows,
            vec![
                DerivedTimelineRow::Message { message_index: 0 },
                DerivedTimelineRow::TurnFold {
                    turn_id: "T1".into(),
                    label: "Worked for 5.0s".into(),
                    expanded: false,
                },
                // w1 and the commentary assistant message a1 fold away; the
                // terminal assistant message stays visible.
                DerivedTimelineRow::Message { message_index: 2 },
            ]
        );
    }

    #[test]
    fn interrupted_turn_uses_stopped_label() {
        let (messages, work) = fold_scenario();
        let latest = latest_turn(
            "T1",
            OrchestrationLatestTurnState::Interrupted,
            Some(T1),
            Some(T6),
        );
        let rows = derive_rows_with(&messages, &work, Some(&latest), None, &[], &[], false);
        assert!(rows.contains(&DerivedTimelineRow::TurnFold {
            turn_id: "T1".into(),
            label: "You stopped after 5.0s".into(),
            expanded: false,
        }));
    }

    #[test]
    fn fold_duration_falls_back_to_user_boundary_and_terminal_updated_at() {
        let (mut messages, work) = fold_scenario();
        // Terminal assistant message finished at T7; the turn started at the
        // user boundary T0.
        messages[2].updated_at = TrimmedNonEmptyString(T7.into());
        let rows = derive_rows_with(&messages, &work, None, None, &[], &[], false);
        assert!(rows.contains(&DerivedTimelineRow::TurnFold {
            turn_id: "T1".into(),
            label: "Worked for 7.0s".into(),
            expanded: false,
        }));
    }

    #[test]
    fn expanded_turn_keeps_entries_visible() {
        let (messages, work) = fold_scenario();
        let latest = latest_turn(
            "T1",
            OrchestrationLatestTurnState::Completed,
            Some(T1),
            Some(T6),
        );
        let rows = derive_rows_with(&messages, &work, Some(&latest), None, &["T1"], &[], false);
        assert_eq!(
            rows,
            vec![
                DerivedTimelineRow::Message { message_index: 0 },
                DerivedTimelineRow::TurnFold {
                    turn_id: "T1".into(),
                    label: "Worked for 5.0s".into(),
                    expanded: true,
                },
                DerivedTimelineRow::Work { entry_index: 0 },
                DerivedTimelineRow::Message { message_index: 1 },
                DerivedTimelineRow::Message { message_index: 2 },
            ]
        );
    }

    #[test]
    fn unsettled_latest_turn_never_folds() {
        let (messages, work) = fold_scenario();
        let unfolded = vec![
            DerivedTimelineRow::Message { message_index: 0 },
            DerivedTimelineRow::Work { entry_index: 0 },
            DerivedTimelineRow::Message { message_index: 1 },
            DerivedTimelineRow::Message { message_index: 2 },
        ];

        // Still running (no completion).
        let running = latest_turn("T1", OrchestrationLatestTurnState::Running, Some(T1), None);
        assert_eq!(
            derive_rows_with(&messages, &work, Some(&running), None, &[], &[], false),
            unfolded
        );

        // The session's running turn is authoritative even when latestTurn
        // claims completion.
        let completed = latest_turn(
            "T1",
            OrchestrationLatestTurnState::Completed,
            Some(T1),
            Some(T6),
        );
        assert_eq!(
            derive_rows_with(
                &messages,
                &work,
                Some(&completed),
                Some("T1"),
                &[],
                &[],
                false
            ),
            unfolded
        );
    }

    #[test]
    fn streaming_message_blocks_fold() {
        let (mut messages, work) = fold_scenario();
        messages[2].streaming = true;
        let latest = latest_turn(
            "T1",
            OrchestrationLatestTurnState::Completed,
            Some(T1),
            Some(T6),
        );
        let rows = derive_rows_with(&messages, &work, Some(&latest), None, &[], &[], false);
        assert!(
            !rows
                .iter()
                .any(|row| matches!(row, DerivedTimelineRow::TurnFold { .. }))
        );
    }

    // -----------------------------------------------------------------------
    // Timeline rows: work-run grouping
    // -----------------------------------------------------------------------

    #[test]
    fn single_visible_work_entry_renders_plain_row() {
        let work = vec![work_entry("w1", T1)];
        let rows = derive_rows_with(&[], &work, None, None, &[], &[], false);
        assert_eq!(rows, vec![DerivedTimelineRow::Work { entry_index: 0 }]);
    }

    #[test]
    fn work_run_collapses_to_last_visible_entry_plus_toggle() {
        let work = vec![
            work_entry("w1", T1),
            work_entry("w2", T2),
            work_entry("w3", T3),
            work_entry("w4", T4),
        ];
        let rows = derive_rows_with(&[], &work, None, None, &[], &[], false);
        assert_eq!(
            rows,
            vec![
                DerivedTimelineRow::Work { entry_index: 3 },
                DerivedTimelineRow::WorkToggle {
                    group_id: "work-group:w1".into(),
                    hidden_count: 3,
                    expanded: false,
                    only_tool_entries: true,
                },
            ]
        );
    }

    #[test]
    fn expanded_work_group_shows_hidden_entries_before_visible_one() {
        let work = vec![
            work_entry("w1", T1),
            work_entry("w2", T2),
            work_entry("w3", T3),
        ];
        let rows = derive_rows_with(&[], &work, None, None, &[], &["work-group:w1"], false);
        assert_eq!(
            rows,
            vec![
                DerivedTimelineRow::Work { entry_index: 0 },
                DerivedTimelineRow::Work { entry_index: 1 },
                DerivedTimelineRow::Work { entry_index: 2 },
                DerivedTimelineRow::WorkToggle {
                    group_id: "work-group:w1".into(),
                    hidden_count: 2,
                    expanded: true,
                    only_tool_entries: true,
                },
            ]
        );
    }

    #[test]
    fn neutral_entries_are_invisible_in_work_runs() {
        let mut neutral = work_entry("w1", T1);
        neutral.tool_lifecycle_status = Some("inProgress".into());
        let work = vec![neutral.clone(), work_entry("w2", T2), work_entry("w3", T3)];
        let rows = derive_rows_with(&[], &work, None, None, &[], &[], false);
        assert_eq!(
            rows,
            vec![
                DerivedTimelineRow::Work { entry_index: 2 },
                DerivedTimelineRow::WorkToggle {
                    // The group is still keyed by the run's first entry.
                    group_id: "work-group:w1".into(),
                    hidden_count: 1,
                    expanded: false,
                    only_tool_entries: true,
                },
            ]
        );

        // A run of only-neutral entries renders nothing.
        let mut also_neutral = neutral;
        also_neutral.id = "w9".into();
        also_neutral.created_at = T2.into();
        let all_neutral = vec![
            {
                let mut entry = work_entry("w1", T1);
                entry.tool_lifecycle_status = Some("inProgress".into());
                entry
            },
            also_neutral,
        ];
        assert!(derive_rows_with(&[], &all_neutral, None, None, &[], &[], false).is_empty());
    }

    #[test]
    fn work_toggle_reports_non_tool_entries() {
        let mut non_tool = work_entry("w2", T2);
        non_tool.tone = OrchestrationThreadActivityTone::Info;
        non_tool.kind = "thread.created".into();
        let work = vec![work_entry("w1", T1), non_tool];
        let rows = derive_rows_with(&[], &work, None, None, &[], &[], false);
        assert_eq!(
            rows,
            vec![
                DerivedTimelineRow::Work { entry_index: 1 },
                DerivedTimelineRow::WorkToggle {
                    group_id: "work-group:w1".into(),
                    hidden_count: 1,
                    expanded: false,
                    only_tool_entries: false,
                },
            ]
        );
    }

    #[test]
    fn working_row_appended_when_working() {
        assert_eq!(
            derive_rows_with(&[], &[], None, None, &[], &[], true),
            vec![DerivedTimelineRow::Working]
        );

        let messages = vec![message("m1", OrchestrationMessageRole::User, T0, None)];
        assert_eq!(
            derive_rows_with(&messages, &[], None, None, &[], &[], true),
            vec![
                DerivedTimelineRow::Message { message_index: 0 },
                DerivedTimelineRow::Working,
            ]
        );
    }

    // -----------------------------------------------------------------------
    // Render helpers
    // -----------------------------------------------------------------------

    #[test]
    fn work_entry_heading_normalizes_and_capitalizes() {
        let mut titled = work_entry("w", T0);
        titled.tool_title = Some("read the file completed".into());
        assert_eq!(work_entry_heading(&titled), "Read the file");

        let mut untitled = work_entry("w", T0);
        untitled.label = "searched the docs complete".into();
        assert_eq!(work_entry_heading(&untitled), "Searched the docs");

        // TS `!workEntry.toolTitle`: an empty title falls back to the label.
        let mut empty_title = work_entry("w", T0);
        empty_title.tool_title = Some(String::new());
        empty_title.label = "ran command".into();
        assert_eq!(work_entry_heading(&empty_title), "Ran command");
    }

    #[test]
    fn work_entry_preview_prefers_command_then_detail_then_files() {
        let mut entry = work_entry("w", T0);
        entry.command = Some("cargo test".into());
        entry.detail = Some("output".into());
        entry.changed_files = vec!["src/a.rs".into()];
        assert_eq!(work_entry_preview(&entry).as_deref(), Some("cargo test"));

        entry.command = None;
        assert_eq!(work_entry_preview(&entry).as_deref(), Some("output"));

        entry.detail = None;
        assert_eq!(work_entry_preview(&entry).as_deref(), Some("src/a.rs"));

        // Backslashes normalize and extra files render a "+N more" suffix.
        entry.changed_files = vec!["src\\main.rs:10:5".into(), "b.rs".into(), "c.rs".into()];
        assert_eq!(
            work_entry_preview(&entry).as_deref(),
            Some("src/main.rs:10:5 +2 more")
        );

        entry.changed_files = Vec::new();
        assert_eq!(work_entry_preview(&entry), None);
    }

    #[test]
    fn expanded_body_stacks_mcp_command_detail_and_files() {
        let mut entry = work_entry("w", T0);
        entry.item_type = Some("mcp_tool_call".into());
        entry.tool_data = Some(json!({"name": "search", "args": {"q": "x"}}));
        entry.raw_command = Some("bash -lc 'cargo test'".into());
        entry.command = Some("cargo test".into());
        entry.detail = Some("ok".into());
        entry.changed_files = vec!["/repo/src/lib.rs".into(), "docs/readme.md".into()];

        let json = serde_json::to_string_pretty(&json!({"name": "search", "args": {"q": "x"}}))
            .expect("pretty json");
        let expected = format!(
            "MCP call\n{json}\n\nbash -lc 'cargo test'\n\nok\n\nrepo/src/lib.rs\nrepo/docs/readme.md"
        );
        assert_eq!(
            build_tool_call_expanded_body(&entry, Some("/repo")).as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn expanded_body_falls_back_to_command_when_raw_matches() {
        let mut entry = work_entry("w", T0);
        entry.raw_command = Some("cargo test".into());
        entry.command = Some("cargo test".into());
        assert_eq!(
            build_tool_call_expanded_body(&entry, None).as_deref(),
            Some("cargo test")
        );

        assert_eq!(
            build_tool_call_expanded_body(&work_entry("w", T0), None),
            None
        );
    }

    #[test]
    fn workspace_relative_paths_format_like_web() {
        let body = |files: &[&str], root: Option<&str>| {
            let mut entry = work_entry("w", T0);
            entry.changed_files = files.iter().map(|file| (*file).to_string()).collect();
            build_tool_call_expanded_body(&entry, root).expect("body")
        };

        // Windows drive canonicalization without a root.
        assert_eq!(body(&["/C:/repo/src/a.rs"], None), "C:/repo/src/a.rs");
        // Line/column and range suffixes survive formatting.
        assert_eq!(
            body(&["src/a.rs:10-20", "src/b.rs:3:7"], None),
            "src/a.rs:10-20\nsrc/b.rs:3:7"
        );
        // Case-insensitive root prefix (trailing slash trimmed).
        assert_eq!(body(&["/Repo/src/a.rs"], Some("/repo/")), "repo/src/a.rs");
        // Path equal to the root becomes the label.
        assert_eq!(body(&["/repo"], Some("/repo")), "repo");
        // Relative paths already under the label pass through; others gain it.
        assert_eq!(body(&["repo/src/b.rs"], Some("/x/repo")), "repo/src/b.rs");
        assert_eq!(
            body(&["docs/readme.md"], Some("/x/repo")),
            "repo/docs/readme.md"
        );
        assert_eq!(
            body(&["./docs/readme.md"], Some("/x/repo")),
            "repo/docs/readme.md"
        );
        // Position suffix combined with a root.
        assert_eq!(body(&["/repo/a.rs:12"], Some("/repo")), "repo/a.rs:12");
    }

    #[test]
    fn work_entry_icon_name_matches_web_precedence() {
        let mut user_input = work_entry("w", T0);
        user_input.kind = "user-input.requested".into();
        assert_eq!(work_entry_icon_name(&user_input), "message-circle");

        let mut file_read = work_entry("w", T0);
        file_read.request_kind = Some("file-read".into());
        assert_eq!(work_entry_icon_name(&file_read), "eye");

        let mut command = work_entry("w", T0);
        command.command = Some("ls".into());
        assert_eq!(work_entry_icon_name(&command), "terminal");

        let mut file_change = work_entry("w", T0);
        file_change.changed_files = vec!["a.rs".into()];
        assert_eq!(work_entry_icon_name(&file_change), "square-pen");

        let mut mcp = work_entry("w", T0);
        mcp.item_type = Some("mcp_tool_call".into());
        assert_eq!(work_entry_icon_name(&mcp), "wrench");

        let mut error = work_entry("w", T0);
        error.tone = OrchestrationThreadActivityTone::Error;
        assert_eq!(work_entry_icon_name(&error), "circle-alert");

        let mut thinking = work_entry("w", T0);
        thinking.tone = thinking_tone();
        assert_eq!(work_entry_icon_name(&thinking), "bot");

        let mut info = work_entry("w", T0);
        info.tone = OrchestrationThreadActivityTone::Info;
        assert_eq!(work_entry_icon_name(&info), "check");

        assert_eq!(work_entry_icon_name(&work_entry("w", T0)), "zap");
    }
}
