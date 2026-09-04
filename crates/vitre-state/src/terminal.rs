//! Terminal client state: the pure reducers behind the terminal drawer.
//!
//! Ports `packages/client-runtime/src/state/terminalSession.ts`
//! (`applyTerminalAttachStreamEvent` over `TerminalBufferState`) and the
//! metadata fold from `packages/client-runtime/src/state/terminal.ts`
//! (`applyTerminalMetadataStreamEvent`), plus `getTerminalLabel` from
//! `packages/shared/src/terminalLabels.ts`.
//!
//! One deliberate simplification: Electron tracks `bufferBytes` beside the
//! JS (UTF-16) string and trims with two UTF-8-boundary-aware algorithms.
//! Rust strings *are* UTF-8, so `buffer.len()` is the byte count and both
//! trims reduce to "drop leading bytes to the next char boundary"
//! ([`trim_buffer_to_cap`]). Lone surrogates cannot exist here, so the
//! U+FFFD width special-case has no equivalent.

use vitre_contracts::{TerminalAttachStreamEvent, TerminalMetadataStreamEvent, TerminalSummary};

/// `DEFAULT_MAX_TERMINAL_BUFFER_BYTES` — 512 KiB of UTF-8.
pub const MAX_TERMINAL_BUFFER_BYTES: usize = 512 * 1024;

/// Client-side session status: the wire's four states plus the client-only
/// `Closed` seed/terminal state (Electron's `TerminalBufferState["status"]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalClientStatus {
    Starting,
    Running,
    Exited,
    Error,
    #[default]
    Closed,
}

impl TerminalClientStatus {
    pub fn from_wire(status: &vitre_contracts::TerminalSessionStatus) -> Self {
        use vitre_contracts::TerminalSessionStatus as Wire;
        match status {
            Wire::Starting => Self::Starting,
            Wire::Running => Self::Running,
            Wire::Exited => Self::Exited,
            Wire::Error => Self::Error,
            // Unknown future statuses render as errors rather than lying
            // about liveness.
            Wire::Unknown(_) => Self::Error,
        }
    }
}

/// The scanned attach-stream state (`EMPTY_TERMINAL_BUFFER_STATE` seed).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TerminalBufferState {
    pub buffer: String,
    pub status: TerminalClientStatus,
    pub error: Option<String>,
    pub updated_at: Option<String>,
    /// NOT monotonic: `snapshot`/`restarted` always set exactly 1 (gotcha —
    /// consumers must treat an unchanged version as "nothing to apply").
    pub version: u64,
}

/// Fold one attach-stream event (`applyTerminalAttachStreamEvent`).
pub fn apply_attach_event(state: &mut TerminalBufferState, event: &TerminalAttachStreamEvent) {
    match event {
        TerminalAttachStreamEvent::Snapshot { snapshot }
        | TerminalAttachStreamEvent::Restarted { snapshot, .. } => {
            let mut buffer = snapshot.history.0.clone();
            trim_buffer_to_cap(&mut buffer);
            state.buffer = buffer;
            state.status = TerminalClientStatus::from_wire(&snapshot.status);
            state.error = None;
            state.updated_at = Some(snapshot.updated_at.0.clone());
            state.version = 1;
        }
        TerminalAttachStreamEvent::Output { data, .. } => {
            state.buffer.push_str(&data.0);
            trim_buffer_to_cap(&mut state.buffer);
            if state.status == TerminalClientStatus::Closed {
                state.status = TerminalClientStatus::Running;
            }
            state.error = None;
            state.version += 1;
        }
        TerminalAttachStreamEvent::Cleared { .. } => {
            state.buffer.clear();
            state.error = None;
            state.version += 1;
        }
        TerminalAttachStreamEvent::Exited { .. } => {
            state.status = TerminalClientStatus::Exited;
            state.error = None;
            state.version += 1;
        }
        TerminalAttachStreamEvent::Closed { .. } => {
            state.status = TerminalClientStatus::Closed;
            state.error = None;
            state.version += 1;
        }
        TerminalAttachStreamEvent::Error { message, .. } => {
            state.status = TerminalClientStatus::Error;
            state.error = Some(message.clone());
            state.version += 1;
        }
        // Activity is consumed via the metadata stream only.
        TerminalAttachStreamEvent::Activity { .. } | TerminalAttachStreamEvent::Unknown(_) => {}
    }
}

/// Front-trim `buffer` to [`MAX_TERMINAL_BUFFER_BYTES`], advancing to a char
/// boundary so the result stays valid UTF-8.
fn trim_buffer_to_cap(buffer: &mut String) {
    if buffer.len() <= MAX_TERMINAL_BUFFER_BYTES {
        return;
    }
    let mut start = buffer.len() - MAX_TERMINAL_BUFFER_BYTES;
    while !buffer.is_char_boundary(start) {
        start += 1;
    }
    buffer.drain(..start);
}

/// Fold one metadata-stream event over the summary list
/// (`applyTerminalMetadataStreamEvent`). Snapshot order is the server's MRU
/// sort and upserts append — this list is NOT tab order; sort by id for
/// stable derived lists ([`compare_terminal_ids`]).
pub fn apply_metadata_event(
    terminals: &mut Vec<TerminalSummary>,
    event: &TerminalMetadataStreamEvent,
) {
    match event {
        TerminalMetadataStreamEvent::Snapshot { terminals: next } => {
            *terminals = next.clone();
        }
        TerminalMetadataStreamEvent::Upsert { terminal } => {
            terminals.retain(|existing| {
                existing.thread_id != terminal.thread_id
                    || existing.terminal_id != terminal.terminal_id
            });
            terminals.push(terminal.clone());
        }
        TerminalMetadataStreamEvent::Remove {
            terminal_id,
            thread_id,
        } => {
            terminals.retain(|existing| {
                existing.thread_id != *thread_id || existing.terminal_id != *terminal_id
            });
        }
        TerminalMetadataStreamEvent::Unknown(_) => {}
    }
}

/// `combineTerminalSessionState`'s status rule: the attach status once the
/// stream has produced anything (`version > 0`), else the metadata summary's,
/// else the attach seed (`Closed`).
pub fn combined_status(
    buffer: &TerminalBufferState,
    summary: Option<&TerminalSummary>,
) -> TerminalClientStatus {
    if buffer.version > 0 {
        return buffer.status;
    }
    summary
        .map(|summary| TerminalClientStatus::from_wire(&summary.status))
        .unwrap_or(buffer.status)
}

/// `getTerminalLabel`: `term-3` / `Terminal-3` → `Terminal 3`, anything else
/// verbatim. The server label (trimmed, non-empty) always wins over this.
pub fn terminal_fallback_label(terminal_id: &str) -> String {
    let lower = terminal_id.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("terminal-")
        .or_else(|| lower.strip_prefix("term-"));
    match rest {
        Some(digits) if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => {
            format!("Terminal {digits}")
        }
        _ => terminal_id.to_string(),
    }
}

/// Numeric-aware terminal-id order (`term-2` < `term-10`), Electron's
/// `localeCompare(..., { numeric: true })` as used by
/// `useKnownTerminalSessions` and the drawer's server-ordered ids.
pub fn compare_terminal_ids(a: &str, b: &str) -> std::cmp::Ordering {
    crate::turn_diff_tree::natural_cmp(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitre_contracts::{TerminalSessionSnapshot, TerminalSessionStatus, TrimmedNonEmptyString};

    fn snapshot(history: &str, status: TerminalSessionStatus) -> TerminalSessionSnapshot {
        TerminalSessionSnapshot {
            cwd: "/tmp".into(),
            exit_code: None,
            exit_signal: None,
            history: TrimmedNonEmptyString(history.into()),
            label: "Terminal 1".into(),
            pid: Some(1),
            sequence: None,
            status,
            terminal_id: "term-1".into(),
            thread_id: "t".into(),
            updated_at: TrimmedNonEmptyString("2026-09-03T00:00:00.000Z".into()),
            worktree_path: None,
        }
    }

    fn output(data: &str) -> TerminalAttachStreamEvent {
        TerminalAttachStreamEvent::Output {
            data: TrimmedNonEmptyString(data.into()),
            sequence: None,
            terminal_id: "term-1".into(),
            thread_id: "t".into(),
        }
    }

    #[test]
    fn snapshot_always_resets_version_to_one() {
        let mut state = TerminalBufferState::default();
        apply_attach_event(
            &mut state,
            &TerminalAttachStreamEvent::Snapshot {
                snapshot: snapshot("hello", TerminalSessionStatus::Running),
            },
        );
        assert_eq!(state.version, 1);
        assert_eq!(state.buffer, "hello");
        assert_eq!(state.status, TerminalClientStatus::Running);
        for _ in 0..5 {
            apply_attach_event(&mut state, &output("x"));
        }
        assert_eq!(state.version, 6);
        apply_attach_event(
            &mut state,
            &TerminalAttachStreamEvent::Snapshot {
                snapshot: snapshot("fresh", TerminalSessionStatus::Running),
            },
        );
        assert_eq!(state.version, 1, "reconnect snapshots pin version to 1");
        assert_eq!(state.buffer, "fresh");
    }

    #[test]
    fn output_flips_closed_to_running_and_clears_error() {
        let mut state = TerminalBufferState {
            error: Some("boom".into()),
            ..Default::default()
        };
        apply_attach_event(&mut state, &output("hi"));
        assert_eq!(state.status, TerminalClientStatus::Running);
        assert_eq!(state.error, None);
        apply_attach_event(
            &mut state,
            &TerminalAttachStreamEvent::Exited {
                exit_code: Some(0),
                exit_signal: None,
                sequence: None,
                terminal_id: "term-1".into(),
                thread_id: "t".into(),
            },
        );
        assert_eq!(state.status, TerminalClientStatus::Exited);
        assert_eq!(state.buffer, "hi", "exit keeps the buffer");
        // Exited (not Closed) stays: only Closed flips to Running on output.
        apply_attach_event(&mut state, &output("post"));
        assert_eq!(state.status, TerminalClientStatus::Exited);
    }

    #[test]
    fn trimming_lands_on_char_boundaries() {
        let mut state = TerminalBufferState::default();
        // 3-byte chars that do not divide the cap evenly. Trimming keeps the
        // buffer at the cap after every event, so drive by append count.
        let chunk = "あ".repeat(1000);
        for _ in 0..=MAX_TERMINAL_BUFFER_BYTES / chunk.len() {
            apply_attach_event(&mut state, &output(&chunk));
        }
        assert!(state.buffer.len() <= MAX_TERMINAL_BUFFER_BYTES);
        assert!(
            state.buffer.len() > MAX_TERMINAL_BUFFER_BYTES - 4,
            "trim happened and kept the buffer at the cap"
        );
        assert!(state.buffer.chars().all(|c| c == 'あ'));

        // Snapshot trim takes the same path.
        let long = "é".repeat(MAX_TERMINAL_BUFFER_BYTES);
        apply_attach_event(
            &mut state,
            &TerminalAttachStreamEvent::Snapshot {
                snapshot: snapshot(&long, TerminalSessionStatus::Running),
            },
        );
        assert!(state.buffer.len() <= MAX_TERMINAL_BUFFER_BYTES);
        assert!(state.buffer.chars().all(|c| c == 'é'));
    }

    #[test]
    fn metadata_upsert_replaces_and_appends() {
        let base = snapshot("", TerminalSessionStatus::Running);
        let summary = |id: &str, label: &str| TerminalSummary {
            cwd: base.cwd.clone(),
            exit_code: None,
            exit_signal: None,
            has_running_subprocess: false,
            label: label.into(),
            pid: None,
            status: TerminalSessionStatus::Running,
            terminal_id: id.into(),
            thread_id: "t".into(),
            updated_at: base.updated_at.clone(),
            worktree_path: None,
        };
        let mut terminals = Vec::new();
        apply_metadata_event(
            &mut terminals,
            &TerminalMetadataStreamEvent::Snapshot {
                terminals: vec![summary("term-1", "zsh"), summary("term-2", "vim")],
            },
        );
        apply_metadata_event(
            &mut terminals,
            &TerminalMetadataStreamEvent::Upsert {
                terminal: summary("term-1", "htop"),
            },
        );
        assert_eq!(terminals.len(), 2);
        assert_eq!(terminals[1].terminal_id, "term-1", "upsert appends");
        assert_eq!(terminals[1].label, "htop");
        apply_metadata_event(
            &mut terminals,
            &TerminalMetadataStreamEvent::Remove {
                terminal_id: "term-2".into(),
                thread_id: "t".into(),
            },
        );
        assert_eq!(terminals.len(), 1);
    }

    #[test]
    fn combined_status_prefers_attach_once_versioned() {
        let mut buffer = TerminalBufferState::default();
        let base = snapshot("", TerminalSessionStatus::Exited);
        let summary = TerminalSummary {
            cwd: base.cwd,
            exit_code: None,
            exit_signal: None,
            has_running_subprocess: false,
            label: base.label,
            pid: None,
            status: TerminalSessionStatus::Exited,
            terminal_id: base.terminal_id,
            thread_id: base.thread_id,
            updated_at: base.updated_at,
            worktree_path: None,
        };
        assert_eq!(
            combined_status(&buffer, Some(&summary)),
            TerminalClientStatus::Exited,
            "summary wins before the stream produces anything"
        );
        assert_eq!(combined_status(&buffer, None), TerminalClientStatus::Closed);
        apply_attach_event(&mut buffer, &output("x"));
        assert_eq!(
            combined_status(&buffer, Some(&summary)),
            TerminalClientStatus::Running,
            "attach status wins once version > 0"
        );
    }

    #[test]
    fn fallback_labels_follow_electron() {
        assert_eq!(terminal_fallback_label("term-3"), "Terminal 3");
        assert_eq!(terminal_fallback_label("Terminal-12"), "Terminal 12");
        assert_eq!(terminal_fallback_label("term-"), "term-");
        assert_eq!(terminal_fallback_label("zsh"), "zsh");
    }

    #[test]
    fn terminal_id_order_is_numeric_aware() {
        let mut ids = vec!["term-10", "term-2", "term-1"];
        ids.sort_by(|a, b| compare_terminal_ids(a, b));
        assert_eq!(ids, vec!["term-1", "term-2", "term-10"]);
    }
}
