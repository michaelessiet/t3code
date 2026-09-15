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
//! ## The caret
//!
//! Vim's caret sits *on* a character rather than between two, so outside
//! insert mode the engine reports which character it covers
//! ([`VimEngine::caret_cell`]) and the editor paints a solid block there —
//! codemirror-vim's `cm-fat-cursor`.
//!
//! That is separate from what the editor paints as *selected*
//! ([`VimEngine::editor_selection`]), which is empty in normal mode: painting
//! the caret's character as a selection too would sink the block into the
//! muted selection colour, which is exactly what makes a modal caret
//! impossible to find inside a visual selection.
//! [`VimEngine::selection`] is the third, operator-facing range — what `d`,
//! `y` or `c` would act on.
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
//! - No replace mode: `R` is unbound, so [`VimMode::Replace`] is unreachable.
//!   codemirror-vim draws its caret there as a bottom bar (`hCoeff` 0.2), the
//!   way [`VimEngine::has_pending_keys`] drives the half-height block for a
//!   half-typed command.

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
