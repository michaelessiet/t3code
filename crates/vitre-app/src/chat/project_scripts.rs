//! The header's project scripts ("actions") control.
//!
//! Ports `ProjectScriptsControl.tsx` + the `runProjectScript`/
//! `saveProjectScript`/`updateProjectScript`/`deleteProjectScript` flows from
//! `ChatView.tsx` and the t3.json import offer from
//! `useT3ProjectFileScripts.ts`. Scripts persist through
//! `project.meta.update` (full list replacement) and flow back via the shell's
//! `ProjectUpserted` events; the run flow opens the drawer, reuses the active
//! terminal unless it has a running subprocess (then allocates a new one at
//! 120x30), and writes `{command}\r`.
//!
//! Deviations (matrix-noted):
//! - No keybinding capture/labels: the dialog omits Electron's Keybinding
//!   field and menu rows show no shortcut, pending a dynamic keymap.
//! - The per-script edit gear is always visible in the menu (Electron reveals
//!   it on hover over the shortcut label).
//! - `previewUrl`/`autoOpenPreview` persist round-trip but nothing opens a
//!   preview on run (preview subsystem is M4).
//! - The run button's label has no container-query collapse to icon-only.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{Context, Entity, SharedString, WeakEntity, Window, div, prelude::*, px};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, StyledExt as _,
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    switch::Switch,
    v_flex,
};
use vitre_contracts::methods::{ProjectsReadFile, TerminalOpen, TerminalWrite};
use vitre_contracts::{
    ClientOrchestrationCommand, CommandId, OrchestrationProjectShell, ProjectReadFileInput,
    ProjectScript, ProjectScriptIcon, TerminalOpenInput, TerminalWriteInput,
};
use vitre_state::project_scripts::{
    ProjectScriptInput, T3_PROJECT_FILE_NAME, T3FileScript, build_project_script,
    import_input_for_file_script, importable_file_scripts, next_project_script_id,
    preferred_project_script, scripts_after_add, scripts_after_delete, scripts_after_update,
};
use vitre_state::right_panel::next_terminal_id;
use vitre_state::terminal_ui::DEFAULT_THREAD_TERMINAL_ID;

use crate::assets::VitreIcon;

use super::{ChatApp, fresh_id, tnes};

/// `SCRIPT_TERMINAL_COLS`/`SCRIPT_TERMINAL_ROWS`: the size a script-created
/// terminal opens at (the first canvas layout re-fits it).
const SCRIPT_TERMINAL_COLS: i64 = 120;
const SCRIPT_TERMINAL_ROWS: i64 = 30;

/// All scripts-control state hanging off [`ChatApp`].
#[derive(Default)]
pub(super) struct ScriptsState {
    dialog: Option<ScriptDialog>,
    /// Scripts from the project's checked-in t3.json, offered for import.
    file_scripts: Vec<T3FileScript>,
    /// Workspace root `file_scripts` was fetched for.
    file_scripts_root: Option<String>,
    file_scripts_generation: u64,
}

/// The open Add/Edit Action dialog (`ProjectScriptsControl`'s form state).
pub(super) struct ScriptDialog {
    editing_script_id: Option<String>,
    name: Entity<InputState>,
    command: Entity<TextareaState>,
    preview_url: Entity<InputState>,
    icon: ProjectScriptIcon,
    icon_picker_open: bool,
    run_on_worktree_create: bool,
    auto_open_preview: bool,
    validation_error: Option<SharedString>,
    /// Shared with the dialog builder closure, which runs inside
    /// `ChatApp::render` and therefore cannot read the chat entity.
    saving: Rc<Cell<bool>>,
}

/// `ScriptIcon`: the lucide icon for each `ProjectScriptIcon` literal
/// (unknown values fall back to play, like Electron's final return).
fn script_icon(icon: &ProjectScriptIcon) -> Icon {
    match icon {
        ProjectScriptIcon::Test => Icon::new(VitreIcon::FlaskConical),
        ProjectScriptIcon::Lint => Icon::new(VitreIcon::ListChecks),
        ProjectScriptIcon::Configure => Icon::new(VitreIcon::Wrench),
        ProjectScriptIcon::Build => Icon::new(VitreIcon::Hammer),
        ProjectScriptIcon::Debug => Icon::new(VitreIcon::Bug),
        ProjectScriptIcon::Play | ProjectScriptIcon::Unknown(_) => Icon::new(IconName::Play),
    }
}

/// `SCRIPT_ICONS`: the picker's six choices, in Electron's order.
const SCRIPT_ICONS: &[(ProjectScriptIcon, &str)] = &[
    (ProjectScriptIcon::Play, "Play"),
    (ProjectScriptIcon::Test, "Test"),
    (ProjectScriptIcon::Lint, "Lint"),
    (ProjectScriptIcon::Configure, "Configure"),
    (ProjectScriptIcon::Build, "Build"),
    (ProjectScriptIcon::Debug, "Debug"),
];

impl ChatApp {
    /// The open thread's project (Electron's `activeProject`).
    fn active_project(&self) -> Option<&OrchestrationProjectShell> {
        let open = self.thread.as_ref()?;
        let thread = self.shell_thread(&open.id)?;
        self.shell
            .snapshot
            .as_ref()?
            .projects
            .iter()
            .find(|project| project.id == thread.project_id)
    }

    /// `useT3ProjectFileScripts`: fetch `t3.json` at the project's workspace
    /// root once per root. Missing, truncated, or invalid files resolve to no
    /// import offers.
    fn ensure_file_scripts(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active_project() else {
            return;
        };
        let root = project.workspace_root.0.clone();
        if self.scripts.file_scripts_root.as_deref() == Some(root.as_str()) {
            return;
        }
        self.scripts.file_scripts_root = Some(root.clone());
        self.scripts.file_scripts.clear();
        self.scripts.file_scripts_generation += 1;
        let generation = self.scripts.file_scripts_generation;
        let Some(client) = self.client.clone() else {
            return;
        };
        let payload = ProjectReadFileInput {
            cwd: tnes(&root),
            relative_path: tnes(T3_PROJECT_FILE_NAME),
        };
        cx.spawn(async move |this, cx| {
            let scripts = match client.call::<ProjectsReadFile>(&payload).await {
                Ok(result) if !result.truncated => {
                    vitre_state::project_scripts::parse_t3_project_file_scripts(&result.contents.0)
                }
                _ => Vec::new(),
            };
            let _ = this.update(cx, |this, cx| {
                if this.scripts.file_scripts_generation != generation {
                    return;
                }
                this.scripts.file_scripts = scripts;
                cx.notify();
            });
        })
        .detach();
    }

    // ------------------------------------------------------------- rendering

    /// The header control: run button + menu when scripts exist, an import
    /// menu when only t3.json offers exist, a bare "Add action" otherwise.
    pub(super) fn render_project_scripts(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let project = self.active_project()?;
        let project_id = project.id.0.clone();
        let scripts: Vec<ProjectScript> = project.scripts.clone();
        self.ensure_file_scripts(cx);
        let preferred_id = self.changed_files.last_invoked_script(&project_id);
        let primary = preferred_project_script(&scripts, preferred_id.as_deref()).cloned();
        let importable: Vec<T3FileScript> =
            importable_file_scripts(&self.scripts.file_scripts, &scripts)
                .into_iter()
                .cloned()
                .collect();
        let chat = cx.entity().downgrade();

        if let Some(primary) = primary {
            let run_script = primary.clone();
            let run_button = Button::new("project-script-run")
                .outline()
                .xsmall()
                .icon(script_icon(&primary.icon).with_size(px(14.)))
                .label(SharedString::from(primary.name.0.clone()))
                .tooltip(SharedString::from(format!("Run {}", primary.name.0)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.run_project_script(run_script.clone(), window, cx);
                }));
            let menu_trigger = Button::new("project-script-menu")
                .outline()
                .xsmall()
                .icon(Icon::new(IconName::ChevronDown).with_size(px(16.)))
                .tooltip("Script actions")
                .dropdown_menu({
                    let chat = chat.clone();
                    move |mut menu, _window, _cx| {
                        for script in &scripts {
                            menu = menu.item(script_menu_item(&chat, script));
                        }
                        menu = import_menu_items(menu, &chat, &importable, true);
                        add_action_item(menu, &chat)
                    }
                });
            return Some(
                h_flex()
                    .gap_1()
                    .flex_shrink_0()
                    .items_center()
                    .child(run_button)
                    .child(menu_trigger)
                    .into_any_element(),
            );
        }

        if !importable.is_empty() {
            let menu_trigger = Button::new("project-script-menu")
                .outline()
                .xsmall()
                .icon(Icon::new(IconName::Plus).with_size(px(14.)))
                .label("Add action")
                .dropdown_menu({
                    let chat = chat.clone();
                    move |menu, _window, _cx| {
                        let menu = import_menu_items(menu, &chat, &importable, false);
                        add_action_item(menu, &chat)
                    }
                });
            return Some(menu_trigger.into_any_element());
        }

        Some(
            Button::new("project-script-add")
                .outline()
                .xsmall()
                .icon(Icon::new(IconName::Plus).with_size(px(14.)))
                .label("Add action")
                .tooltip("Add action")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_script_dialog(None, window, cx);
                }))
                .into_any_element(),
        )
    }

    // ---------------------------------------------------------------- dialog

    /// Opens the Add/Edit Action dialog (`openAddDialog`/`openEditDialog`).
    fn open_script_dialog(
        &mut self,
        editing: Option<&ProjectScript>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_script_dialog_with(
            editing.map(|script| script.id.0.clone()),
            editing
                .map(|script| script.name.0.clone())
                .unwrap_or_default(),
            editing
                .map(|script| script.command.0.clone())
                .unwrap_or_default(),
            editing
                .map(|script| script.icon.clone())
                .unwrap_or(ProjectScriptIcon::Play),
            editing.is_some_and(|script| script.run_on_worktree_create),
            editing
                .and_then(|script| script.preview_url.clone().flatten())
                .map(|url| url.0)
                .unwrap_or_default(),
            editing
                .and_then(|script| script.auto_open_preview.flatten())
                .unwrap_or(false),
            None,
            window,
            cx,
        );
    }

    /// Shared open path; the import-failure flow re-enters with prefilled
    /// values and a validation error, like Electron's `importFileScript`.
    #[allow(clippy::too_many_arguments)]
    fn open_script_dialog_with(
        &mut self,
        editing_script_id: Option<String>,
        name: String,
        command: String,
        icon: ProjectScriptIcon,
        run_on_worktree_create: bool,
        preview_url: String,
        auto_open_preview: bool,
        validation_error: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_editing = editing_script_id.is_some();
        let name_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Test")
                .default_value(name)
        });
        let command_state = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("bun test")
                .default_value(command)
        });
        let preview_url_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("http://localhost:5173")
                .default_value(preview_url)
        });
        let saving = Rc::new(Cell::new(false));
        self.scripts.dialog = Some(ScriptDialog {
            editing_script_id,
            name: name_state,
            command: command_state,
            preview_url: preview_url_state,
            icon,
            icon_picker_open: false,
            run_on_worktree_create,
            auto_open_preview,
            validation_error,
            saving: saving.clone(),
        });

        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let content_owner = owner.clone();
            let close_owner = owner.clone();
            let cancel_owner = owner.clone();
            let save_owner = owner.clone();
            let delete_owner = owner.clone();
            // Dialog builders run inside `ChatApp::render` (the chat view
            // renders the dialog layer), so this closure must not read the
            // chat entity — the saving flag arrives through a shared cell.
            let saving = saving.get();
            dialog
                .title(if is_editing {
                    "Edit Action"
                } else {
                    "Add Action"
                })
                .w(px(512.))
                .on_close(move |_, _, cx| {
                    let _ = close_owner.update(cx, |this, _| this.scripts.dialog = None);
                })
                .content(move |content, _window, cx| {
                    let Some(chat) = content_owner.upgrade() else {
                        return content;
                    };
                    let body = chat.update(cx, |this, cx| this.render_script_dialog_body(cx));
                    content.child(body)
                })
                .footer(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .when(is_editing, |this| {
                            this.child(
                                Button::new("script-delete")
                                    .danger()
                                    .outline()
                                    .small()
                                    .label("Delete")
                                    .on_click(move |_, window, cx| {
                                        let _ = delete_owner.update(cx, |this, cx| {
                                            this.confirm_delete_script(window, cx);
                                        });
                                    }),
                            )
                            .child(div().flex_1())
                        })
                        .when(!is_editing, |this| this.justify_end())
                        .child(
                            Button::new("script-cancel")
                                .outline()
                                .small()
                                .label("Cancel")
                                .on_click(move |_, window, cx| {
                                    let _ = cancel_owner
                                        .update(cx, |this, _| this.scripts.dialog = None);
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("script-save")
                                .primary()
                                .small()
                                .label(if is_editing {
                                    "Save changes"
                                } else {
                                    "Save action"
                                })
                                .disabled(saving)
                                .on_click(move |_, window, cx| {
                                    let _ = save_owner.update(cx, |this, cx| {
                                        this.submit_script_dialog(window, cx);
                                    });
                                }),
                        ),
                )
        });
    }

    /// The dialog form: icon picker + name, command, preview URL, the two
    /// switch rows, and the validation line. (Electron's Keybinding field is
    /// deferred — no dynamic keymap yet.)
    fn render_script_dialog_body(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(dialog) = self.scripts.dialog.as_ref() else {
            return div().into_any_element();
        };
        let theme = cx.theme().clone();
        let icon = dialog.icon.clone();
        let picker_open = dialog.icon_picker_open;
        let run_on_worktree_create = dialog.run_on_worktree_create;
        let auto_open_preview = dialog.auto_open_preview;
        let preview_url_empty = dialog.preview_url.read(cx).value().trim().is_empty();
        let validation_error = dialog.validation_error.clone();
        let name_state = dialog.name.clone();
        let command_state = dialog.command.clone();
        let preview_url_state = dialog.preview_url.clone();

        let mut picker_button = div().relative().child(
            Button::new("script-icon-picker")
                .outline()
                .size(px(36.))
                .icon(script_icon(&icon).with_size(px(18.)))
                .tooltip("Choose icon")
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(dialog) = this.scripts.dialog.as_mut() {
                        dialog.icon_picker_open = !dialog.icon_picker_open;
                        cx.notify();
                    }
                })),
        );
        if picker_open {
            let mut grid = v_flex().gap_2();
            for row in SCRIPT_ICONS.chunks(3) {
                let mut cells = h_flex().gap_2();
                for (entry_icon, label) in row {
                    let selected = *entry_icon == icon;
                    let entry_icon = entry_icon.clone();
                    cells = cells.child(
                        div()
                            .id(SharedString::from(format!("script-icon-{label}")))
                            .cursor_pointer()
                            .v_flex()
                            .items_center()
                            .gap_2()
                            .rounded(px(6.))
                            .border_1()
                            .px_2()
                            .py_2()
                            .text_xs()
                            .w(px(72.))
                            .map(|this| {
                                if selected {
                                    this.border_color(theme.primary.opacity(0.7))
                                        .bg(theme.primary.opacity(0.1))
                                } else {
                                    this.border_color(theme.border.opacity(0.7))
                                        .hover(|style| style.bg(theme.accent.opacity(0.6)))
                                }
                            })
                            .child(script_icon(&entry_icon).with_size(px(16.)))
                            .child(SharedString::from(*label))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(dialog) = this.scripts.dialog.as_mut() {
                                    dialog.icon = entry_icon.clone();
                                    dialog.icon_picker_open = false;
                                    cx.notify();
                                }
                            })),
                    );
                }
                grid = grid.child(cells);
            }
            // Deferred so the popup paints above the fields below it.
            picker_button = picker_button.child(
                gpui::deferred(
                    div()
                        .occlude()
                        .absolute()
                        .top(px(40.))
                        .left_0()
                        .p_2()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.popover)
                        .shadow_md()
                        .child(grid),
                )
                .with_priority(gpui_component::POPUP_PRIORITY),
            );
        }

        v_flex()
            .gap_4()
            .child(div().text_sm().text_color(theme.muted_foreground).child(
                "Actions are project-scoped commands you can run from the top bar or keybindings.",
            ))
            .child(
                v_flex()
                    .gap_1p5()
                    .child(div().text_xs().font_medium().child("Name"))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(picker_button)
                            .child(div().flex_1().child(Input::new(&name_state))),
                    ),
            )
            .child(
                v_flex()
                    .gap_1p5()
                    .child(div().text_xs().font_medium().child("Command"))
                    .child(Textarea::new(&command_state)),
            )
            .child(
                v_flex()
                    .gap_1p5()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .child("Preview URL (optional)"),
                    )
                    .child(Input::new(&preview_url_state))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Open this URL in the in-app preview when this action runs."),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme.border.opacity(0.7))
                    .px_3()
                    .py_2()
                    .text_sm()
                    .child("Run automatically on worktree creation")
                    .child(
                        Switch::new("script-run-on-worktree")
                            .checked(run_on_worktree_create)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                if let Some(dialog) = this.scripts.dialog.as_mut() {
                                    dialog.run_on_worktree_create = *checked;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme.border.opacity(0.7))
                    .px_3()
                    .py_2()
                    .text_sm()
                    .when(preview_url_empty, |this| this.opacity(0.6))
                    .child("Open preview automatically when this action runs")
                    .child(
                        Switch::new("script-auto-open-preview")
                            .checked(auto_open_preview)
                            .disabled(preview_url_empty)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                if let Some(dialog) = this.scripts.dialog.as_mut() {
                                    dialog.auto_open_preview = *checked;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .when_some(validation_error, |this, error| {
                this.child(div().text_sm().text_color(theme.danger).child(error))
            })
            .into_any_element()
    }

    /// `submitAddScript`: validate, build, persist, close (or surface the
    /// failure inline and stay open).
    fn submit_script_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.scripts.dialog.as_ref() else {
            return;
        };
        if dialog.saving.get() {
            return;
        }
        let name = dialog.name.read(cx).value().trim().to_string();
        let command = dialog.command.read(cx).value().trim().to_string();
        let preview_url = dialog.preview_url.read(cx).value().trim().to_string();
        if name.is_empty() {
            self.set_script_dialog_error(Some("Name is required.".into()), cx);
            return;
        }
        if command.is_empty() {
            self.set_script_dialog_error(Some("Command is required.".into()), cx);
            return;
        }
        let input = ProjectScriptInput {
            name,
            command,
            icon: dialog.icon.clone(),
            run_on_worktree_create: dialog.run_on_worktree_create,
            preview_url: (!preview_url.is_empty()).then_some(preview_url.clone()),
            auto_open_preview: if preview_url.is_empty() {
                false
            } else {
                dialog.auto_open_preview
            },
        };
        let editing_script_id = dialog.editing_script_id.clone();

        // Electron's `saveProjectScript`/`updateProjectScript` no-op without a
        // project ("success"), which just closes the dialog.
        let Some(project) = self.active_project() else {
            self.scripts.dialog = None;
            _window.close_dialog(cx);
            return;
        };
        let existing = project.scripts.clone();
        let project_id = project.id.clone();
        let next_scripts = match &editing_script_id {
            Some(script_id) => {
                if !existing.iter().any(|script| script.id.0 == *script_id) {
                    self.set_script_dialog_error(Some("Script not found.".into()), cx);
                    return;
                }
                let updated = build_project_script(script_id, &input);
                scripts_after_update(&existing, script_id, updated)
            }
            None => {
                let next_id = next_project_script_id(
                    &input.name,
                    existing.iter().map(|script| script.id.0.as_str()),
                );
                scripts_after_add(&existing, build_project_script(&next_id, &input))
            }
        };

        if let Some(dialog) = self.scripts.dialog.as_mut() {
            dialog.saving.set(true);
            dialog.validation_error = None;
        }
        cx.notify();
        let Some(client) = self.client.clone() else {
            return;
        };
        let command = ClientOrchestrationCommand::ProjectMetaUpdate {
            additional_roots: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            default_model_selection: None,
            project_id,
            scripts: Some(Some(next_scripts)),
            title: None,
            r#type: Default::default(),
            workspace_root: None,
        };
        cx.spawn(async move |this, cx| {
            let result = client.dispatch(&command).await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(_) => {
                    if this.scripts.dialog.take().is_some() {
                        window.close_dialog(cx);
                    }
                }
                Err(error) => {
                    if let Some(dialog) = this.scripts.dialog.as_mut() {
                        dialog.saving.set(false);
                        dialog.validation_error = Some(SharedString::from(format!("{error:?}")));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn set_script_dialog_error(&mut self, error: Option<SharedString>, cx: &mut Context<Self>) {
        if let Some(dialog) = self.scripts.dialog.as_mut() {
            dialog.validation_error = error;
            cx.notify();
        }
    }

    /// The stacked delete confirmation (`Delete action "{name}"?`). Confirming
    /// closes both dialogs and fires the delete, toasting the outcome.
    fn confirm_delete_script(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.scripts.dialog.as_ref() else {
            return;
        };
        let Some(script_id) = dialog.editing_script_id.clone() else {
            return;
        };
        // Electron titles the confirm with the *form's* current name value.
        let name = dialog.name.read(cx).value().trim().to_string();
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let cancel_owner = owner.clone();
            let confirm_owner = owner.clone();
            let script_id = script_id.clone();
            let name = name.clone();
            dialog
                .title(SharedString::from(format!("Delete action \"{name}\"?")))
                .w(px(420.))
                .content(move |content, _, cx| {
                    content.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("This action cannot be undone."),
                    )
                })
                .footer(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .justify_end()
                        .child(
                            Button::new("script-delete-cancel")
                                .outline()
                                .small()
                                .label("Cancel")
                                .on_click(move |_, window, cx| {
                                    let _ = cancel_owner.update(cx, |_, _| {});
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("script-delete-confirm")
                                .danger()
                                .small()
                                .label("Delete action")
                                .on_click(move |_, window, cx| {
                                    // Pop the confirm, then the edit dialog.
                                    window.close_dialog(cx);
                                    window.close_dialog(cx);
                                    let script_id = script_id.clone();
                                    let _ = confirm_owner.update(cx, |this, cx| {
                                        this.scripts.dialog = None;
                                        this.delete_project_script(&script_id, cx);
                                    });
                                }),
                        ),
                )
        });
    }

    /// `deleteProjectScript`: filter + persist, success/failure toasts.
    fn delete_project_script(&mut self, script_id: &str, cx: &mut Context<Self>) {
        let Some(project) = self.active_project() else {
            return;
        };
        let deleted_name = project
            .scripts
            .iter()
            .find(|script| script.id.0 == script_id)
            .map(|script| script.name.0.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let next_scripts = scripts_after_delete(&project.scripts, script_id);
        let project_id = project.id.clone();
        let Some(client) = self.client.clone() else {
            return;
        };
        let command = ClientOrchestrationCommand::ProjectMetaUpdate {
            additional_roots: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            default_model_selection: None,
            project_id,
            scripts: Some(Some(next_scripts)),
            title: None,
            r#type: Default::default(),
            workspace_root: None,
        };
        cx.spawn(async move |this, cx| {
            let result = client.dispatch(&command).await;
            let _ = this.update_in(cx, |_, window, cx| match result {
                Ok(_) => window.push_notification(
                    Notification::success(SharedString::from(format!(
                        "Deleted action \"{deleted_name}\""
                    ))),
                    cx,
                ),
                Err(error) => window.push_notification(
                    Notification::error(SharedString::from(format!("{error:?}")))
                        .title("Could not delete action"),
                    cx,
                ),
            });
        })
        .detach();
    }

    /// `importFileScript`: add with defaults; a failure re-opens the add
    /// dialog prefilled so the user can adjust and retry.
    fn import_file_script(&mut self, file_script: &T3FileScript, cx: &mut Context<Self>) {
        let input = import_input_for_file_script(file_script);
        let Some(project) = self.active_project() else {
            return;
        };
        let existing = project.scripts.clone();
        let project_id = project.id.clone();
        let next_id = next_project_script_id(
            &input.name,
            existing.iter().map(|script| script.id.0.as_str()),
        );
        let next_scripts = scripts_after_add(&existing, build_project_script(&next_id, &input));
        let Some(client) = self.client.clone() else {
            return;
        };
        let command = ClientOrchestrationCommand::ProjectMetaUpdate {
            additional_roots: None,
            command_id: CommandId(fresh_id("vitre-cmd")),
            default_model_selection: None,
            project_id,
            scripts: Some(Some(next_scripts)),
            title: None,
            r#type: Default::default(),
            workspace_root: None,
        };
        cx.spawn(async move |this, cx| {
            if client.dispatch(&command).await.is_ok() {
                return;
            }
            let _ = this.update_in(cx, |this, window, cx| {
                this.open_script_dialog_with(
                    None,
                    input.name.clone(),
                    input.command.clone(),
                    input.icon.clone(),
                    input.run_on_worktree_create,
                    input.preview_url.clone().unwrap_or_default(),
                    input.auto_open_preview,
                    Some("Failed to import action.".into()),
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    // ------------------------------------------------------------------- run

    /// `runProjectScript`: remember the choice, open the drawer, reuse the
    /// active terminal unless it's busy (then a new 120x30 one), await
    /// `terminal.open`, then write `{command}\r`.
    pub(super) fn run_project_script(
        &mut self,
        script: ProjectScript,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project() else {
            return;
        };
        let project_id = project.id.0.clone();
        self.changed_files
            .remember_last_invoked_script(&project_id, &script.id.0);
        let Some(defaults) = self.thread_launch_defaults() else {
            return;
        };
        let Some(key) = self.terminal_thread_key() else {
            return;
        };

        // Electron: `activeTerminalId || activeKnownTerminalIds[0] || DEFAULT`.
        let state = self.terminal_ui.map.thread(&key);
        let base_terminal_id = if !state.active_terminal_id.is_empty() {
            state.active_terminal_id.clone()
        } else {
            self.terminal_metadata
                .iter()
                .find(|summary| summary.thread_id == defaults.thread_id)
                .map(|summary| summary.terminal_id.clone())
                .unwrap_or_else(|| DEFAULT_THREAD_TERMINAL_ID.to_string())
        };
        let base_busy = self.terminal_metadata.iter().any(|summary| {
            summary.thread_id == defaults.thread_id
                && summary.terminal_id == base_terminal_id
                && summary.has_running_subprocess
        });
        let create_new = base_busy;

        if self.terminal_ui.map.set_terminal_open(&key, true) {
            self.terminal_ui.save();
        }
        let target_terminal_id = if create_new {
            let existing = self.all_known_terminal_ids(&key, &defaults.thread_id);
            let id = next_terminal_id(existing.iter().map(String::as_str));
            if self.terminal_ui.map.new_terminal(&key, &id) {
                self.terminal_ui.save();
            }
            id
        } else {
            if self
                .terminal_ui
                .map
                .set_active_terminal(&key, &base_terminal_id)
            {
                self.terminal_ui.save();
            }
            base_terminal_id
        };
        self.focus_active_terminal(&key, cx);
        cx.notify();

        let Some(client) = self.client.clone() else {
            return;
        };
        let env: serde_json::Map<String, serde_json::Value> = defaults
            .env
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        let open_payload = TerminalOpenInput {
            cols: create_new.then_some(Some(SCRIPT_TERMINAL_COLS)),
            cwd: tnes(&defaults.cwd),
            env: Some(Some(env)),
            rows: create_new.then_some(Some(SCRIPT_TERMINAL_ROWS)),
            terminal_id: tnes(&target_terminal_id),
            thread_id: tnes(&defaults.thread_id),
            worktree_path: Some(Some(defaults.worktree.as_deref().map(tnes))),
        };
        let write_payload = TerminalWriteInput {
            data: format!("{}\r", script.command.0),
            terminal_id: tnes(&target_terminal_id),
            thread_id: tnes(&defaults.thread_id),
        };
        let script_name = script.name.0.clone();
        cx.spawn(async move |this, cx| {
            if let Err(error) = client.call::<TerminalOpen>(&open_payload).await {
                eprintln!("[vitre] project script open failed: {error:?}");
                let _ = this.update(cx, |this, cx| {
                    this.last_error =
                        Some(format!("Failed to run script \"{script_name}\".").into());
                    cx.notify();
                });
                return;
            }
            if let Err(error) = client.call::<TerminalWrite>(&write_payload).await {
                eprintln!("[vitre] project script write failed: {error:?}");
                let _ = this.update(cx, |this, cx| {
                    this.last_error =
                        Some(format!("Failed to run script \"{script_name}\".").into());
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

// -------------------------------------------------------------- menu builders

/// One script row: icon + name (`{name} (setup)` for the worktree-create
/// script) + an edit gear. Clicking the row runs; the gear opens Edit.
fn script_menu_item(chat: &WeakEntity<ChatApp>, script: &ProjectScript) -> PopupMenuItem {
    let label = if script.run_on_worktree_create {
        format!("{} (setup)", script.name.0)
    } else {
        script.name.0.clone()
    };
    let row_script = script.clone();
    let gear_script = script.clone();
    let gear_chat = chat.clone();
    let run_chat = chat.clone();
    let icon = script.icon.clone();
    let gear_id = SharedString::from(format!("script-edit-{}", script.id.0));
    PopupMenuItem::element(move |_, _| {
        let gear_chat = gear_chat.clone();
        let gear_script = gear_script.clone();
        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(script_icon(&icon).with_size(px(16.)))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .child(SharedString::from(label.clone())),
            )
            .child(
                Button::new(gear_id.clone())
                    .ghost()
                    .xsmall()
                    .icon(Icon::new(IconName::Settings).with_size(px(14.)))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        let _ = gear_chat.update(cx, |this, cx| {
                            this.open_script_dialog(Some(&gear_script), window, cx);
                        });
                    }),
            )
    })
    .on_click(move |_, window, cx| {
        let _ = run_chat.update(cx, |this, cx| {
            this.run_project_script(row_script.clone(), window, cx);
        });
    })
}

/// The "From t3.json" import section (preceded by a separator when script
/// rows are above it, exactly like Electron's `importMenuItems`).
fn import_menu_items(
    mut menu: gpui_component::menu::PopupMenu,
    chat: &WeakEntity<ChatApp>,
    importable: &[T3FileScript],
    after_scripts: bool,
) -> gpui_component::menu::PopupMenu {
    if importable.is_empty() {
        return menu;
    }
    if after_scripts {
        menu = menu.item(PopupMenuItem::separator());
    }
    menu = menu.item(PopupMenuItem::label("From t3.json"));
    for file_script in importable {
        let icon = file_script.icon.clone().unwrap_or(ProjectScriptIcon::Play);
        let name = file_script.name.clone();
        let import_chat = chat.clone();
        let import_script = file_script.clone();
        menu = menu.item(
            PopupMenuItem::element(move |_, _| {
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(script_icon(&icon).with_size(px(16.)))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .child(SharedString::from(name.clone())),
                    )
                    .child(Icon::new(VitreIcon::Download).with_size(px(14.)))
            })
            .on_click(move |_, _, cx| {
                let _ = import_chat.update(cx, |this, cx| {
                    this.import_file_script(&import_script, cx);
                });
            }),
        );
    }
    menu
}

fn add_action_item(
    menu: gpui_component::menu::PopupMenu,
    chat: &WeakEntity<ChatApp>,
) -> gpui_component::menu::PopupMenu {
    let chat = chat.clone();
    menu.item(
        PopupMenuItem::new("Add action")
            .icon(Icon::new(IconName::Plus))
            .on_click(move |_, window, cx| {
                let _ = chat.update(cx, |this, cx| {
                    this.open_script_dialog(None, window, cx);
                });
            }),
    )
}
