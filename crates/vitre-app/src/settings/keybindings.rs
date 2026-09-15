//! Keybinding edits are validated by the sidecar and replace the exact old rule.
use super::*;
use gpui::{AppContext as _, Entity};
use gpui_component::{
    Disableable as _, WindowExt as _,
    input::{Input, InputState},
};
use vitre_contracts::{
    ResolvedKeybindingRule, ServerRemoveKeybindingInput, ServerUpsertKeybindingInput,
    methods::{ServerRemoveKeybinding, ServerUpsertKeybinding},
};

pub(super) struct KeybindingEditor {
    command: Entity<InputState>,
    key: Entity<InputState>,
    when: Entity<InputState>,
    previous: Option<ServerRemoveKeybindingInput>,
    busy: bool,
    recording: bool,
    error: Option<SharedString>,
    subscriptions: Vec<gpui::Subscription>,
}

impl SettingsPanel {
    pub(super) fn edit_keybinding(
        &mut self,
        rule: Option<ResolvedKeybindingRule>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = rule.as_ref().map(|rule| ServerRemoveKeybindingInput {
            command: rule.command.clone(),
            key: vitre_contracts::KeybindingValue(wire_shortcut(&rule.shortcut)),
            when: rule
                .when_ast
                .as_ref()
                .and_then(|a| a.as_ref())
                .map(|a| Some(vitre_contracts::KeybindingWhen(format_when(a)))),
        });
        let input = |value: String,
                     placeholder: &'static str,
                     window: &mut Window,
                     cx: &mut Context<Self>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(value)
                    .placeholder(placeholder)
            })
        };
        self.keybinding_editor = Some(KeybindingEditor {
            command: input(
                rule.as_ref()
                    .map(|r| enum_wire_name(&r.command))
                    .unwrap_or_default(),
                "Command, e.g. preview.toggle",
                window,
                cx,
            ),
            key: input(
                previous
                    .as_ref()
                    .map(|r| r.key.0.clone())
                    .unwrap_or_default(),
                "Shortcut, e.g. mod+shift+j",
                window,
                cx,
            ),
            when: input(
                previous
                    .as_ref()
                    .and_then(|r| r.when.as_ref())
                    .and_then(|v| v.as_ref())
                    .map(|v| v.0.clone())
                    .unwrap_or_default(),
                "Optional condition, e.g. !terminalFocus",
                window,
                cx,
            ),
            previous,
            busy: false,
            recording: false,
            error: None,
            subscriptions: Vec::new(),
        });
        // Input entities notify themselves, not the surrounding dialog's
        // conflict summary. Observe edits (including paste) explicitly.
        let fields = {
            let e = self.keybinding_editor.as_ref().unwrap();
            [e.key.clone(), e.when.clone()]
        };
        let subscriptions = fields
            .iter()
            .map(|field| {
                cx.subscribe(field, |_, _, _: &gpui_component::input::InputEvent, cx| {
                    cx.notify()
                })
            })
            .collect();
        self.keybinding_editor.as_mut().unwrap().subscriptions = subscriptions;
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            dialog
                .title("Keyboard shortcut")
                .w(px(560.))
                .content(move |content, _, cx| {
                    let Some(panel) = owner.upgrade() else {
                        return content;
                    };
                    let Some(editor) = panel.read(cx).keybinding_editor.as_ref() else {
                        return content;
                    };
                    let save = owner.clone();
                    let remove = owner.clone();
                    let record = owner.clone();
                    let capture = owner.clone();
                    let key=editor.key.read(cx).value();let when=editor.when.read(cx).value();
                    let conflicts=panel.read(cx).config.as_ref().map(|c|c.keybindings.iter().filter(|r|{
                        let existing_when=r.when_ast.as_ref().and_then(|a|a.as_ref()).map(format_when).unwrap_or_default();
                        wire_shortcut(&r.shortcut)==key.as_ref() && (when.is_empty()||existing_when.is_empty()||existing_when==when.as_ref()) && editor.previous.as_ref().is_none_or(|p|p.command!=r.command || p.key.0!=wire_shortcut(&r.shortcut))
                    }).map(|r|enum_wire_name(&r.command)).collect::<Vec<_>>()).unwrap_or_default();
                    content.child(
                        v_flex()
                            .capture_key_down(move |event: &gpui::KeyDownEvent, window, cx| {
                                let _ = capture.update(cx, |panel, cx| {
                                    let Some(editor) = panel.keybinding_editor.as_mut() else {
                                        return;
                                    };
                                    if !editor.recording {
                                        return;
                                    }
                                    cx.stop_propagation();
                                    window.prevent_default();
                                    if event.keystroke.key == "escape" {
                                        editor.recording = false;
                                        cx.notify();
                                        return;
                                    }
                                    if ["shift", "control", "alt", "command", "fn"]
                                        .contains(&event.keystroke.key.as_str())
                                    {
                                        return;
                                    }
                                    let m = event.keystroke.modifiers;
                                    let mut parts = Vec::new();
                                    if m.platform {
                                        parts.push(if cfg!(target_os = "macos") {
                                            "mod"
                                        } else {
                                            "meta"
                                        });
                                    }
                                    if m.control {
                                        parts.push(if cfg!(target_os = "macos") {
                                            "ctrl"
                                        } else {
                                            "mod"
                                        });
                                    }
                                    if m.alt {
                                        parts.push("alt");
                                    }
                                    if m.shift {
                                        parts.push("shift");
                                    }
                                    parts.push(&event.keystroke.key);
                                    editor.key.update(cx, |input, cx| {
                                        input.set_value(parts.join("+"), window, cx)
                                    });
                                    editor.recording = false;
                                    cx.notify();
                                });
                            })
                            .gap_3()
                            .child("Command")
                            .child(Input::new(&editor.command))
                            .child("Shortcut")
                            .child(Input::new(&editor.key))
                            .child(
                                Button::new("record-shortcut")
                                    .label(if editor.recording {
                                        "Press shortcut… Escape cancels"
                                    } else {
                                        "Record shortcut"
                                    })
                                    .on_click(move |_, window, cx| {
                                        let _ = record.update(cx, |panel, cx| {
                                            if let Some(editor) = panel.keybinding_editor.as_mut() {
                                                editor.recording = true;
                                                editor
                                                    .key
                                                    .read(cx)
                                                    .focus_handle(cx)
                                                    .focus(window, cx);
                                                cx.notify();
                                            }
                                        });
                                    }),
                            )
                            .child("When")
                            .child(Input::new(&editor.when))
                            .children((!conflicts.is_empty()).then(||div().text_sm().text_color(cx.theme().warning).child(format!("Also used by: {}. Saving may override those commands in the same context.",conflicts.join(", ")))))
                            .children(
                                editor.error.clone().map(|e| {
                                    div().text_sm().text_color(cx.theme().danger).child(e)
                                }),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("save-keybinding")
                                            .label("Save")
                                            .disabled(editor.busy)
                                            .on_click(move |_, window, cx| {
                                                let _ = save.update(cx, |panel, cx| {
                                                    panel.save_keybinding(false, window, cx)
                                                });
                                            }),
                                    )
                                    .child(
                                        Button::new("remove-keybinding")
                                            .label("Remove override / restore default")
                                            .disabled(editor.busy || editor.previous.is_none())
                                            .on_click(move |_, window, cx| {
                                                let _ = remove.update(cx, |panel, cx| {
                                                    panel.save_keybinding(true, window, cx)
                                                });
                                            }),
                                    ),
                            ),
                    )
                })
        });
    }

    fn save_keybinding(&mut self, remove: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.keybinding_editor.as_mut() else {
            return;
        };
        if editor.busy {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let payload = serde_json::json!({
            "command": editor.command.read(cx).value().trim(),
            "key": editor.key.read(cx).value().trim(),
            "when": if editor.when.read(cx).value().trim().is_empty() {None} else {Some(editor.when.read(cx).value().trim().to_string())},
            "replace":editor.previous,
        });
        let input = match serde_json::from_value::<ServerUpsertKeybindingInput>(payload) {
            Ok(input) => input,
            Err(error) => {
                editor.error = Some(error.to_string().into());
                cx.notify();
                return;
            }
        };
        let previous = editor.previous.clone();
        editor.busy = true;
        editor.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = if remove {
                match previous {
                    Some(input) => client
                        .call::<ServerRemoveKeybinding>(&input)
                        .await
                        .map_err(|e| e.user_message()),
                    None => return,
                }
            } else {
                client
                    .call::<ServerUpsertKeybinding>(&input)
                    .await
                    .map_err(|e| e.user_message())
            };
            let _ = this.update_in(cx, |panel, window, cx| {
                match result {
                    Ok(result) => {
                        crate::keymap::install(&result.keybindings, cx);
                        if let Some(config) = panel.config.as_mut() {
                            config.keybindings = result.keybindings;
                            config.issues = result.issues;
                        }
                        panel.keybinding_editor = None;
                        window.close_dialog(cx);
                    }
                    Err(error) => {
                        if let Some(editor) = panel.keybinding_editor.as_mut() {
                            editor.busy = false;
                            editor.error = Some(error.into());
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn wire_shortcut(shortcut: &KeybindingShortcut) -> String {
    let mut keys = Vec::new();
    if shortcut.mod_key {
        keys.push("mod");
    }
    if shortcut.meta_key {
        keys.push("meta");
    }
    if shortcut.ctrl_key {
        keys.push("ctrl");
    }
    if shortcut.alt_key {
        keys.push("alt");
    }
    if shortcut.shift_key {
        keys.push("shift");
    }
    keys.push(&shortcut.key.0);
    keys.join("+")
}
