//! Pure state reducers over the generated contract types.
//!
//! These are line-for-line ports of the shared TypeScript reducers in
//! `packages/client-runtime/src/state/` (`shellReducer.ts`, `threadReducer.ts`)
//! and must keep the exact same semantics — both apps reduce the same
//! orchestration event stream. UI-specific mapping (attachment preview URLs,
//! model-slug normalisation, scoped fields) is the caller's responsibility,
//! exactly as in the TS layer.
//!
//! ## Wire-Option conventions
//!
//! The generated types are wire-faithful, so effect's optional/nullable field
//! flavours surface as nested `Option`s (see `wire_opt`):
//! - `Option<Option<T>>` (effect `optional(X)`): absent and wire-`null` BOTH
//!   decode to TS `undefined` — only `Some(Some(v))` carries a value.
//! - `Option<Option<Option<T>>>` (effect `optional(NullOr(X))` /
//!   `optionalKey(NullOr(X))`): `None` = TS `undefined`, `Some(None)` =
//!   wire-`null` = TS `null` (meaningful!), `Some(Some(Some(v)))` = value.
//!
//! Fields the TS contracts decode with `withDecodingDefault` (`runtimeMode` →
//! `"full-access"`, `interactionMode` → `"default"`) get the same defaults
//! applied here when the reducer reads them.

pub mod browse_path;
pub mod chat_ui;
pub mod diff_panel;
pub mod diff_patch;
pub mod file_buffer;
pub mod file_tree;
pub mod lsp_gating;
pub mod project_grouping;
pub mod right_panel;
pub mod session_logic;
pub mod shell;
pub mod shell_sync;
pub mod sidebar;
pub mod source_control;
pub mod terminal;
pub mod terminal_ui;
pub mod thread;
pub mod thread_sync;
pub mod turn_diff_tree;
pub mod vcs_tree_status;
pub mod wire_opt;

pub use shell::apply_shell_stream_event;
pub use shell_sync::{ShellApplyOutcome, ShellProjection};
pub use thread::{
    ThreadDetailReducerResult, apply_thread_detail_event, event_sequence, event_thread_id,
};
pub use thread_sync::{ThreadApplyOutcome, ThreadProjection};
