//! Vim modal editing for the code editor.
//!
//! Electron gets its vim mode from `@replit/codemirror-vim`, wired up in
//! `apps/web/src/components/files/codemirror/CodeMirrorFileEditor.tsx` behind
//! the `vimMode` client setting (default off). There is no equivalent crate
//! for the gpui-component editor and Zed's `vim` crate is GPL, so this is a
//! from-scratch engine.
//!
//! ## Shape
//!
//! The engine is pure: it owns the mode, the pending command, the registers
//! and the caret, and it turns one key at a time into a list of
//! [`VimEffect`]s that the caller applies to the editor. It never touches
//! gpui, so the whole command surface is unit-testable against a `&str`.
//!
//! Offsets are byte offsets into the document, which is what the fork's
//! `InputState` speaks.
//!
//! ## Normal-mode caret
//!
//! Vim's caret sits *on* a character rather than between two, so in normal
//! and visual mode the engine reports the caret as a one-character selection
//! ([`VimEngine::selection`]). That is the same trick CodeMirror's vim mode
//! uses for its "fat cursor", and it makes the editor paint a block.
//!
//! ## Deliberate divergences from Electron's `@replit/codemirror-vim`
//!
//! - Search and `:s` use Rust regex syntax (with `\<`/`\>` translated to
//!   `\b`), not vim regex; a pattern that fails to compile is matched
//!   literally instead of erroring.
//! - Backwards visual selections paint with the caret at the high end,
//!   because the fork's `set_selected_range` always clears
//!   `selection_reversed`.
//! - No macros (`q`/`@`), no `:g`, no `<C-v>` block mode, no `:set`.

mod engine;
mod motion;
mod object;

#[cfg(test)]
mod tests;

pub use engine::{
    VimConfig, VimDocument, VimEffect, VimEngine, VimKey, VimMode, VimResponse, VimStatus,
};
pub use motion::FindKind;
pub use object::TextObject;
