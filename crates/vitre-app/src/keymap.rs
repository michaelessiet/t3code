//! Runtime keybindings supplied by the sidecar.
//!
//! The server resolves defaults plus the user's `keybindings.json` into a
//! normalized shortcut and a parsed `when` AST. We translate that AST
//! directly to GPUI predicates. Updating adds targeted `Unbind` entries for
//! the prior generation before adding the new one; this preserves component
//! and Vim bindings instead of clearing GPUI's entire application keymap.

use std::rc::Rc;

use gpui::{
    Action, App, DummyKeyboardMapper, Global, KeyBinding, KeyBindingContextPredicate, Unbind,
};
use vitre_contracts::{
    KeybindingCommand, KeybindingShortcut, KeybindingWhenNode, ResolvedKeybindingsConfig,
};

use crate::{chat, files};

#[derive(Clone)]
struct InstalledBinding {
    key: String,
    action_name: String,
    predicate: Option<Rc<KeyBindingContextPredicate>>,
}

#[derive(Default)]
struct RuntimeKeymap {
    installed: Vec<InstalledBinding>,
}

impl Global for RuntimeKeymap {}

/// Replace only Vitre's server-owned bindings. GPUI component defaults and
/// the modal editor keymap remain registered and untouched.
pub fn install(config: &ResolvedKeybindingsConfig, cx: &mut App) {
    if !cx.has_global::<RuntimeKeymap>() {
        cx.set_global(RuntimeKeymap::default());
    }
    let previous = cx.global::<RuntimeKeymap>().installed.clone();
    let mut bindings = Vec::new();
    for old in previous {
        if let Ok(binding) = KeyBinding::load(
            &old.key,
            Box::new(Unbind(old.action_name.into())),
            old.predicate,
            false,
            None,
            &DummyKeyboardMapper,
        ) {
            bindings.push(binding);
        }
    }

    let mut installed = Vec::new();
    for rule in config {
        let Some(action) = action_for_command(&rule.command) else {
            continue;
        };
        let key = shortcut_string(&rule.shortcut);
        let mut predicate = match rule.when_ast.as_ref().and_then(|node| node.as_ref()) {
            Some(node) => {
                let Some(predicate) = predicate_from_ast(node) else {
                    eprintln!(
                        "[vitre] ignoring server keybinding with an unsupported `when` expression"
                    );
                    continue;
                };
                Some(Rc::new(predicate))
            }
            None => None,
        };
        // The stock T3 diff shortcut predates native multi-cursor editing.
        // Reserve Cmd/Ctrl-D inside the editor, retaining the stock behavior
        // everywhere else. Explicit editor-specific/custom chords are kept.
        let stock_diff = rule.command == KeybindingCommand::DiffToggle
            && rule.shortcut.key.0 == "d"
            && rule.shortcut.mod_key
            && !rule.shortcut.alt_key
            && !rule.shortcut.shift_key
            && !rule.shortcut.ctrl_key
            && !rule.shortcut.meta_key
            && matches!(rule.when_ast.as_ref().and_then(|v| v.as_ref()), Some(KeybindingWhenNode::Not { node }) if matches!(node.as_ref(), KeybindingWhenNode::Identifier { name } if name == "terminalFocus"));
        if stock_diff {
            predicate = Some(Rc::new(KeyBindingContextPredicate::And(
                Box::new(predicate.as_deref().unwrap().clone()),
                Box::new(KeyBindingContextPredicate::Not(Box::new(
                    KeyBindingContextPredicate::Identifier("Editor".into()),
                ))),
            )));
        }
        let action_name = action.name().to_string();
        match KeyBinding::load(
            &key,
            action,
            predicate.clone(),
            false,
            None,
            &DummyKeyboardMapper,
        ) {
            Ok(binding) => {
                bindings.push(binding);
                installed.push(InstalledBinding {
                    key,
                    action_name,
                    predicate,
                });
            }
            Err(error) => eprintln!("[vitre] ignoring invalid server keybinding: {error}"),
        }
    }
    cx.bind_keys(bindings);
    cx.global_mut::<RuntimeKeymap>().installed = installed;
}

fn action_for_command(command: &KeybindingCommand) -> Option<Box<dyn Action>> {
    Some(match command {
        KeybindingCommand::SidebarToggle => Box::new(chat::SidebarToggle),
        KeybindingCommand::TerminalToggle => Box::new(chat::TerminalToggle),
        KeybindingCommand::TerminalSplit => Box::new(chat::TerminalSplit),
        KeybindingCommand::TerminalSplitVertical => Box::new(chat::TerminalSplitVertical),
        KeybindingCommand::TerminalNew => Box::new(chat::TerminalNew),
        KeybindingCommand::TerminalClose => Box::new(chat::TerminalCloseActive),
        KeybindingCommand::RightPanelToggle => Box::new(chat::RightPanelToggle),
        KeybindingCommand::RightPanelCloseSurface => Box::new(chat::RightPanelCloseSurface),
        KeybindingCommand::RightPanelNextSurface => Box::new(chat::RightPanelNextSurface),
        KeybindingCommand::RightPanelPreviousSurface => Box::new(chat::RightPanelPreviousSurface),
        KeybindingCommand::DiffToggle => Box::new(chat::DiffToggle),
        KeybindingCommand::EditorShowCompletions => Box::new(files::ShowCompletions),
        KeybindingCommand::FileSave => Box::new(files::SaveFile),
        KeybindingCommand::FileTreeToggleFocus => Box::new(files::TreeToggleFocus),
        KeybindingCommand::FileTreeSearch => Box::new(files::TreeSearch),
        KeybindingCommand::FileTreeNewFile => Box::new(files::TreeNewFile),
        KeybindingCommand::FileTreeNewDirectory => Box::new(files::TreeNewDirectory),
        KeybindingCommand::FileTreeRename => Box::new(files::TreeRename),
        KeybindingCommand::QuickSearchOpen => Box::new(chat::QuickSearchOpen),
        KeybindingCommand::SearchToggle => Box::new(chat::SearchToggle),
        KeybindingCommand::GraphToggle => Box::new(chat::GraphToggle),
        KeybindingCommand::GraphBuild => Box::new(chat::GraphBuild),
        KeybindingCommand::WorkspaceRootsManage => Box::new(chat::WorkspaceRootsManage),
        KeybindingCommand::QuickSearchContent => Box::new(chat::QuickSearchContent),
        KeybindingCommand::PreviewToggle => Box::new(chat::PreviewToggle),
        KeybindingCommand::PreviewRefresh => Box::new(chat::PreviewRefresh),
        KeybindingCommand::PreviewFocusUrl => Box::new(chat::PreviewFocusUrl),
        KeybindingCommand::PreviewZoomIn => Box::new(chat::PreviewZoomIn),
        KeybindingCommand::PreviewZoomOut => Box::new(chat::PreviewZoomOut),
        KeybindingCommand::PreviewResetZoom => Box::new(chat::PreviewResetZoom),
        KeybindingCommand::CommandPaletteToggle => Box::new(chat::CommandPaletteToggle),
        KeybindingCommand::ModelPickerToggle => Box::new(chat::ModelPickerToggle),
        KeybindingCommand::EditorOpenFavorite => Box::new(chat::EditorOpenFavorite),
        KeybindingCommand::ModelPickerJump1 => Box::new(chat::ModelPickerJump1),
        KeybindingCommand::ModelPickerJump2 => Box::new(chat::ModelPickerJump2),
        KeybindingCommand::ModelPickerJump3 => Box::new(chat::ModelPickerJump3),
        KeybindingCommand::ModelPickerJump4 => Box::new(chat::ModelPickerJump4),
        KeybindingCommand::ModelPickerJump5 => Box::new(chat::ModelPickerJump5),
        KeybindingCommand::ModelPickerJump6 => Box::new(chat::ModelPickerJump6),
        KeybindingCommand::ModelPickerJump7 => Box::new(chat::ModelPickerJump7),
        KeybindingCommand::ModelPickerJump8 => Box::new(chat::ModelPickerJump8),
        KeybindingCommand::ModelPickerJump9 => Box::new(chat::ModelPickerJump9),
        KeybindingCommand::ChatNew | KeybindingCommand::ChatNewLocal => Box::new(chat::NewThread),
        KeybindingCommand::ThreadPrevious => Box::new(chat::ThreadPrevious),
        KeybindingCommand::ThreadNext => Box::new(chat::ThreadNext),
        KeybindingCommand::ThreadJump1 => Box::new(chat::ThreadJump1),
        KeybindingCommand::ThreadJump2 => Box::new(chat::ThreadJump2),
        KeybindingCommand::ThreadJump3 => Box::new(chat::ThreadJump3),
        KeybindingCommand::ThreadJump4 => Box::new(chat::ThreadJump4),
        KeybindingCommand::ThreadJump5 => Box::new(chat::ThreadJump5),
        KeybindingCommand::ThreadJump6 => Box::new(chat::ThreadJump6),
        KeybindingCommand::ThreadJump7 => Box::new(chat::ThreadJump7),
        KeybindingCommand::ThreadJump8 => Box::new(chat::ThreadJump8),
        KeybindingCommand::ThreadJump9 => Box::new(chat::ThreadJump9),
        // These commands need their owning M5/M8 surfaces before binding
        // them. The server entry remains visible in Settings; it simply does
        // not steal the chord from a working GPUI control meanwhile.
        _ => return None,
    })
}

fn shortcut_string(shortcut: &KeybindingShortcut) -> String {
    let mut modifiers = Vec::with_capacity(4);
    if shortcut.mod_key {
        modifiers.push(if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        });
    }
    if shortcut.meta_key && !(shortcut.mod_key && cfg!(target_os = "macos")) {
        modifiers.push("cmd");
    }
    if shortcut.ctrl_key && (!shortcut.mod_key || cfg!(target_os = "macos")) {
        modifiers.push("ctrl");
    }
    if shortcut.alt_key {
        modifiers.push("alt");
    }
    if shortcut.shift_key {
        modifiers.push("shift");
    }
    modifiers.push(shortcut.key.0.as_str());
    modifiers.join("-")
}

fn predicate_from_ast(node: &KeybindingWhenNode) -> Option<KeyBindingContextPredicate> {
    use KeyBindingContextPredicate as Gpui;
    use KeybindingWhenNode as Wire;
    Some(match node {
        Wire::Identifier { name } => Gpui::Identifier(context_name(name)?.into()),
        Wire::Not { node } => Gpui::Not(Box::new(predicate_from_ast(node)?)),
        Wire::And { left, right } => Gpui::And(
            Box::new(predicate_from_ast(left)?),
            Box::new(predicate_from_ast(right)?),
        ),
        Wire::Or { left, right } => Gpui::Or(
            Box::new(predicate_from_ast(left)?),
            Box::new(predicate_from_ast(right)?),
        ),
        Wire::Unknown(_) => return None,
    })
}

fn context_name(name: &str) -> Option<&str> {
    Some(match name {
        "terminalFocus" => "Terminal",
        "previewFocus" => "Preview",
        "rightPanelFocus" => "RightPanel",
        "fileTreeFocus" => "FileTree",
        "editorFocus" => "Editor",
        "modelPickerOpen" => "ModelPicker",
        "macPlatform" => "MacPlatform",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitre_contracts::KeybindingValue;

    #[gpui::test]
    fn rebind_removes_old_chord_and_preserves_component_keys(cx: &mut gpui::TestAppContext) {
        use gpui::{Focusable as _, prelude::*};
        use gpui_component::input::{Input, InputState};
        use std::cell::Cell;
        struct Probe {
            input: gpui::Entity<InputState>,
            count: Rc<Cell<usize>>,
        }
        impl gpui::Render for Probe {
            fn render(
                &mut self,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> impl gpui::IntoElement {
                let count = self.count.clone();
                gpui::div()
                    .size_full()
                    .on_action(move |_: &chat::SidebarToggle, _, _| count.set(count.get() + 1))
                    .child(Input::new(&self.input))
            }
        }
        let count = Rc::new(Cell::new(0));
        let observed = count.clone();
        let rule = |key: &str| {
            serde_json::from_value::<ResolvedKeybindingsConfig>(serde_json::json!([{"command":"sidebar.toggle","shortcut":{"key":key,"modKey":true,"metaKey":false,"ctrlKey":false,"shiftKey":false,"altKey":false}}])).unwrap()
        };
        cx.update(|cx| {
            gpui_component::init(cx);
            install(&rule("b"), cx);
        });
        let (probe, cx) = cx.add_window_view(|window, cx| Probe {
            input: cx.new(|cx| InputState::new(window, cx)),
            count: observed,
        });
        let input = probe.read_with(cx, |p, _| p.input.clone());
        cx.update(|w, cx| input.read(cx).focus_handle(cx).focus(w, cx));
        cx.simulate_keystrokes(&crate::modified("b"));
        assert_eq!(count.get(), 1);
        cx.update(|_, cx| install(&rule("j"), cx));
        cx.simulate_keystrokes(&crate::modified("b"));
        assert_eq!(count.get(), 1, "old shortcut must no longer dispatch");
        cx.simulate_keystrokes(&crate::modified("j"));
        assert_eq!(count.get(), 2);
        cx.simulate_keystrokes("a b backspace");
        assert_eq!(
            input.read_with(cx, |i, _| i.value()),
            "a",
            "component bindings survive keymap replacement"
        );
        cx.update(|_, cx| install(&vec![], cx));
        cx.simulate_keystrokes(&crate::modified("j"));
        assert_eq!(count.get(), 2);
    }

    #[test]
    fn shortcut_uses_the_host_platform_modifier_without_duplication() {
        let shortcut = KeybindingShortcut {
            alt_key: true,
            ctrl_key: false,
            key: KeybindingValue("b".into()),
            meta_key: false,
            mod_key: true,
            shift_key: true,
        };
        let expected = if cfg!(target_os = "macos") {
            "cmd-alt-shift-b"
        } else {
            "ctrl-alt-shift-b"
        };
        assert_eq!(shortcut_string(&shortcut), expected);
    }

    #[test]
    fn when_ast_maps_to_native_focus_contexts() {
        let ast = KeybindingWhenNode::Or {
            left: Box::new(KeybindingWhenNode::Not {
                node: Box::new(KeybindingWhenNode::Identifier {
                    name: "terminalFocus".into(),
                }),
            }),
            right: Box::new(KeybindingWhenNode::Identifier {
                name: "macPlatform".into(),
            }),
        };
        assert_eq!(
            predicate_from_ast(&ast).expect("known AST").to_string(),
            "!Terminal || MacPlatform"
        );
    }
}
