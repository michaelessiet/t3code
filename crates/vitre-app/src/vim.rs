//! Vim mode for the code editor: the keymap and the glue
//! that drives [`vitre_state::vim`] against the fork's `EditorState`.
//!
//! Electron gets this from `@replit/codemirror-vim`, switched on by the
//! `vimMode` client setting (default off,
//! `packages/contracts/src/settings.ts:144`) and configured in
//! `apps/web/src/components/files/codemirror/CodeMirrorFileEditor.tsx`, which
//! adds three mappings on top of the stock command set: `gh` → LSP hover,
//! `gd`/`<C-]>` → LSP definition, and `:w` → the host's save coordinator.
//! Those three live in [`vitre_state::vim`] as effects; this module turns them
//! into calls on the panel.
//!
//! ## How keys reach the engine
//!
//! gpui dispatches action bindings *before* low-level key listeners
//! (`window.rs::dispatch_key_event`), so a plain `on_key_down` on an ancestor
//! could never win `escape` or `enter` back from the input's own bindings.
//! Instead the editor's dispatch node carries an extra key context
//! (`InputState::set_extra_key_context`, the one change this feature needs in
//! the fork) and every vim key is a real binding against it. Bindings at equal
//! depth are ranked by registration order, and Vitre binds after
//! `gpui_component::init`, so vim wins exactly the keys it claims and nothing
//! else.
//!
//! Two context flags do the arbitration:
//!
//! - `vim` — vim is on. Only `escape` binds here, because escape has to work
//!   in insert mode too.
//! - `vim_command` — vim is on *and* not inserting. Everything else binds
//!   here, so insert mode leaves the input's own keymap and the platform text
//!   path (IME composition included) completely untouched.

use std::rc::Rc;

use gpui::{Action, App, KeyBinding, KeyContext, SharedString, actions};
use vitre_state::vim::{VimEngine, VimKey, VimMode};

/// One vim keystroke, carried as the vim-notation name (`"d"`, `"A"`, `"$"`,
/// `"escape"`, `"ctrl-r"`) so the handler never has to re-derive it from the
/// binding.
#[derive(Clone, Debug, PartialEq, Eq, Action)]
#[action(namespace = vitre, no_json)]
pub struct VimKeystroke {
    pub key: SharedString,
}

actions!(
    vitre,
    [
        /// Turn vim mode on or off — Electron's Settings ▸ General switch
        /// (`SettingsPanels.tsx:829`), which Vitre now has too; the
        /// action stays for the command palette and any user chord.
        ToggleVimMode,
    ]
);

/// Key context marker: vim is enabled.
const CONTEXT_VIM: &str = "vim";
/// Key context marker: vim is enabled and the engine is not inserting.
const CONTEXT_COMMAND: &str = "vim_command";

/// The extra key context the editor's dispatch node should carry for `mode`,
/// or `None` when vim is off.
pub fn key_context(mode: Option<VimMode>) -> Option<KeyContext> {
    let mode = mode?;
    let mut context = KeyContext::default();
    context.add(CONTEXT_VIM);
    if !mode.is_inserting() {
        context.add(CONTEXT_COMMAND);
    }
    Some(context)
}

pub fn init(cx: &mut App) {
    cx.bind_keys(bindings());
}

/// Punctuation typed without shift. gpui reports these verbatim as the
/// keystroke key, so the binding string and the vim key agree.
const PLAIN_PUNCTUATION: &[&str] = &["`", "-", "=", "[", "]", "\\", ";", "'", ",", ".", "/"];

/// Punctuation typed with shift. macOS clears the shift modifier and reports
/// the shifted character as the key (`gpui_macos::events`), so these also
/// bind verbatim.
const SHIFTED_PUNCTUATION: &[&str] = &[
    "~", "!", "@", "#", "$", "%", "^", "&", "*", "(", ")", "_", "+", "{", "}", "|", ":", "\"", "<",
    ">", "?",
];

/// Chords the engine understands: half-page and page scrolls, redo, the
/// classic tag jump, and the scroll-a-line pair.
const CTRL_KEYS: &[&str] = &[
    "ctrl-r", "ctrl-d", "ctrl-u", "ctrl-f", "ctrl-b", "ctrl-]", "ctrl-e", "ctrl-y", "ctrl-v",
];

/// Named keys the engine maps onto motions and edits (`enter` is `+`,
/// `backspace` is `h`).
const NAMED_KEYS: &[&str] = &["enter", "backspace", "tab", "left", "right", "up", "down"];

fn bindings() -> Vec<KeyBinding> {
    let command = Some(CONTEXT_COMMAND);
    let mut bindings = Vec::new();
    let mut bind = |key: String, context: Option<&str>| {
        let action = VimKeystroke {
            key: SharedString::from(key.clone()),
        };
        bindings.push(KeyBinding::new(&key, action, context));
    };

    for c in 'a'..='z' {
        bind(c.to_string(), command);
        bind(c.to_ascii_uppercase().to_string(), command);
    }
    for c in '0'..='9' {
        bind(c.to_string(), command);
    }
    for key in PLAIN_PUNCTUATION
        .iter()
        .chain(SHIFTED_PUNCTUATION)
        .chain(CTRL_KEYS)
        .chain(NAMED_KEYS)
    {
        bind((*key).to_string(), command);
    }
    bind("space".to_string(), command);
    // Escape has to leave insert mode, so it is the one key bound for every
    // vim mode rather than only the command modes.
    bind("escape".to_string(), Some(CONTEXT_VIM));

    bindings
}

/// Decode a [`VimKeystroke`] payload into an engine key.
pub fn engine_key(key: &str) -> Option<VimKey> {
    if let Some(chord) = key.strip_prefix("ctrl-") {
        let mut chars = chord.chars();
        let c = chars.next()?;
        return chars.next().is_none().then_some(VimKey::Ctrl(c));
    }
    Some(match key {
        "escape" => VimKey::Escape,
        "enter" => VimKey::Enter,
        "backspace" => VimKey::Backspace,
        "tab" => VimKey::Tab,
        "left" => VimKey::Left,
        "right" => VimKey::Right,
        "up" => VimKey::Up,
        "down" => VimKey::Down,
        "space" => VimKey::Char(' '),
        other => {
            let mut chars = other.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            VimKey::Char(c)
        }
    })
}

/// Per-editor vim state: the engine plus the two caches the glue needs to
/// avoid re-deriving them on every keystroke.
pub struct VimSession {
    pub engine: VimEngine,
    /// The buffer as the engine last saw it. `InputState` stores a rope and
    /// the motions want a `&str`, so this is materialised once per edit
    /// instead of once per key.
    text: Option<Rc<str>>,
    /// Where the current insert session began. The editor — not the engine —
    /// owns text input while inserting, so the text `.` should replay is
    /// recovered by slicing the buffer when insert mode ends.
    insert_anchor: Option<usize>,
}

impl Default for VimSession {
    fn default() -> Self {
        Self::new()
    }
}

impl VimSession {
    pub fn new() -> Self {
        Self {
            engine: VimEngine::default(),
            text: None,
            insert_anchor: None,
        }
    }

    /// Drop the cached buffer after an edit landed.
    pub fn invalidate(&mut self) {
        self.text = None;
    }

    /// The buffer text, materialising it from `rope` on a cache miss.
    pub fn text(&mut self, rope: &ropey::Rope) -> Rc<str> {
        self.text
            .get_or_insert_with(|| Rc::from(rope.to_string()))
            .clone()
    }

    /// Remember where an insert session started, so [`Self::take_inserted`]
    /// can recover what was typed.
    pub fn begin_insert(&mut self, offset: usize) {
        self.insert_anchor = Some(offset);
    }

    /// The text typed since [`Self::begin_insert`], or `""` when the caret
    /// moved somewhere the anchor no longer describes (an arrow key, a click)
    /// and the slice would be a lie.
    pub fn take_inserted(&mut self, text: &str, cursor: usize) -> String {
        let Some(anchor) = self.insert_anchor.take() else {
            return String::new();
        };
        if anchor > cursor || cursor > text.len() || !text.is_char_boundary(anchor) {
            return String::new();
        }
        text[anchor..cursor].to_string()
    }

    /// Start over on a fresh buffer: normal mode, caret at the top, no
    /// half-typed command and no insert session in flight.
    pub fn reset(&mut self) {
        self.engine.reset("");
        self.text = None;
        self.insert_anchor = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key the keymap claims must decode back into an engine key —
    /// otherwise the binding swallows the keystroke and nothing happens.
    #[test]
    fn every_bound_key_decodes() {
        let mut keys: Vec<String> = Vec::new();
        for c in 'a'..='z' {
            keys.push(c.to_string());
            keys.push(c.to_ascii_uppercase().to_string());
        }
        for c in '0'..='9' {
            keys.push(c.to_string());
        }
        keys.extend(
            PLAIN_PUNCTUATION
                .iter()
                .chain(SHIFTED_PUNCTUATION)
                .chain(CTRL_KEYS)
                .chain(NAMED_KEYS)
                .map(|key| (*key).to_string()),
        );
        keys.push("space".into());
        keys.push("escape".into());

        for key in keys {
            assert!(
                engine_key(&key).is_some(),
                "bound key {key:?} does not decode"
            );
        }
    }

    #[test]
    fn engine_key_decodes_chords_and_named_keys() {
        assert_eq!(engine_key("d"), Some(VimKey::Char('d')));
        assert_eq!(engine_key("D"), Some(VimKey::Char('D')));
        assert_eq!(engine_key("$"), Some(VimKey::Char('$')));
        assert_eq!(engine_key("space"), Some(VimKey::Char(' ')));
        assert_eq!(engine_key("ctrl-r"), Some(VimKey::Ctrl('r')));
        assert_eq!(engine_key("ctrl-]"), Some(VimKey::Ctrl(']')));
        assert_eq!(engine_key("escape"), Some(VimKey::Escape));
        assert_eq!(engine_key("enter"), Some(VimKey::Enter));
        assert_eq!(engine_key("backspace"), Some(VimKey::Backspace));
        assert_eq!(engine_key("left"), Some(VimKey::Left));
        assert_eq!(engine_key(""), None);
        assert_eq!(engine_key("f13"), None);
    }

    /// The keymap must never bind the same chord twice in one context: the
    /// loser would shadow silently. Building the list also proves every
    /// binding string parses, since `KeyBinding::new` panics if it does not.
    #[test]
    fn bindings_are_unique_single_keystrokes() {
        let bindings = bindings();
        let mut seen = std::collections::HashSet::new();
        for binding in &bindings {
            assert_eq!(
                binding.keystrokes().len(),
                1,
                "vim keys are single keystrokes"
            );
            let key = format!("{:?}", binding.keystrokes()[0]);
            assert!(seen.insert(key.clone()), "duplicate vim binding: {key}");
        }
        // 26 lowercase + 26 uppercase + 10 digits + the tables + space + escape.
        let expected = 62
            + PLAIN_PUNCTUATION.len()
            + SHIFTED_PUNCTUATION.len()
            + CTRL_KEYS.len()
            + NAMED_KEYS.len()
            + 2;
        assert_eq!(bindings.len(), expected);
    }

    #[test]
    fn insert_mode_gives_the_keys_back_to_the_editor() {
        let normal = key_context(Some(VimMode::Normal)).expect("vim is on");
        assert!(normal.contains(CONTEXT_VIM));
        assert!(normal.contains(CONTEXT_COMMAND));

        let visual = key_context(Some(VimMode::VisualLine)).expect("vim is on");
        assert!(visual.contains(CONTEXT_COMMAND));

        // Only `escape` binds against `vim`, so dropping `vim_command` hands
        // every other key back to the input's own keymap and the platform
        // text path.
        let insert = key_context(Some(VimMode::Insert)).expect("vim is on");
        assert!(insert.contains(CONTEXT_VIM));
        assert!(!insert.contains(CONTEXT_COMMAND));

        assert!(key_context(None).is_none());
    }

    /// The claim this whole design rests on: gpui matches action bindings
    /// before it lets text reach the input, and it breaks same-depth ties by
    /// registration order — so vim bindings registered after
    /// `gpui_component::init` win the keys they claim, including `escape`,
    /// which the input already binds.
    ///
    /// Driven through `Window::dispatch_keystroke`, which is the real
    /// pipeline: interceptors, bindings, key listeners, then the platform
    /// text fallback for anything nothing consumed.
    #[gpui::test]
    fn vim_bindings_outrank_the_inputs_own_keymap(cx: &mut gpui::TestAppContext) {
        use gpui::{
            AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
            Render, Styled as _,
        };
        use gpui_component::input::{Editor, EditorState};
        use std::cell::RefCell;

        struct KeymapProbe {
            state: Entity<EditorState>,
            seen: Rc<RefCell<Vec<String>>>,
        }

        impl Render for KeymapProbe {
            fn render(
                &mut self,
                _window: &mut gpui::Window,
                _cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                let seen = self.seen.clone();
                gpui::div()
                    .size_full()
                    .on_action(move |action: &VimKeystroke, _, _| {
                        seen.borrow_mut().push(action.key.to_string());
                    })
                    .child(Editor::new(&self.state).appearance(false).size_full())
            }
        }

        let seen = Rc::new(RefCell::new(Vec::new()));
        cx.update(|cx| {
            gpui_component::init(cx);
            // The ordering the whole feature depends on.
            cx.bind_keys(bindings());
        });
        let probe_seen = seen.clone();
        let (probe, cx) = cx.add_window_view(|window, cx| KeymapProbe {
            state: cx.new(|cx| EditorState::new(window, cx)),
            seen: probe_seen,
        });
        let state = probe.read_with(cx, |probe, _| probe.state.clone());
        cx.update(|window, cx| {
            state.update(cx, |state, cx| {
                state.set_value("abc", window, cx);
                state.focus(window, cx);
            });
        });

        // --- vim off: the editor keeps every key -------------------------
        cx.simulate_keystrokes("d escape");
        assert!(
            seen.borrow().is_empty(),
            "without the vim key context no vim binding may fire"
        );
        assert_eq!(state.read_with(cx, |state, _| state.value()), "dabc");

        // --- normal mode: vim takes the command keys ----------------------
        cx.update(|window, cx| {
            state.update(cx, |state, cx| {
                state.set_value("abc", window, cx);
                state.set_extra_key_context(key_context(Some(VimMode::Normal)), cx);
            });
        });
        cx.simulate_keystrokes("d w escape $ ctrl-r");
        assert_eq!(
            *seen.borrow(),
            vec!["d", "w", "escape", "$", "ctrl-r"],
            "every command key must reach the engine"
        );
        assert_eq!(
            state.read_with(cx, |state, _| state.value()),
            "abc",
            "a consumed command key must not also be typed into the buffer"
        );

        // --- insert mode: only escape stays claimed -----------------------
        seen.borrow_mut().clear();
        cx.update(|_, cx| {
            state.update(cx, |state, cx| {
                state.set_extra_key_context(key_context(Some(VimMode::Insert)), cx);
            });
        });
        cx.simulate_keystrokes("d w escape");
        assert_eq!(
            *seen.borrow(),
            vec!["escape"],
            "insert mode hands every key but escape back to the editor"
        );
        assert_eq!(
            state.read_with(cx, |state, _| state.value()),
            "dwabc",
            "insert-mode typing must reach the platform text path"
        );
    }
}
