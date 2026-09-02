//! The project header's context menu: Rename, Group into…, Copy Path, Remove.
//!
//! Electron builds exactly these four, each targeting one *physical* project:
//! a lone project gets them inline, a grouped repository gets a submenu per
//! member (`Sidebar.tsx`, `handleProjectButtonContextMenu`). Rename and
//! "Group into…" open dialogs; Remove confirms, and refuses a non-empty
//! project until you insist.

use gpui::{ClipboardItem, Context, Entity, SharedString, WeakEntity, Window, div, prelude::*, px};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, StyledExt as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::DialogButtonProps,
    input::{Input, InputState},
    menu::{DropdownMenu as _, PopupMenu, PopupMenuItem},
    notification::Notification,
    v_flex,
};
use vitre_contracts::{ClientOrchestrationCommand, CommandId, ProjectId, TrimmedNonEmptyString};
use vitre_state::project_grouping::ProjectGroupingMode;

use super::{ChatApp, fresh_id};

/// One physical project a context-menu action targets. Grouped rows hand a
/// different member to each submenu, so every action carries its own copy.
#[derive(Clone)]
pub(super) struct ProjectMember {
    pub physical_key: String,
    pub project_id: ProjectId,
    pub title: SharedString,
    pub workspace_root: SharedString,
}

impl ProjectMember {
    /// `formatProjectMemberActionLabel`: a lone project's menu item is just
    /// its title; a grouped row's submenu items are workspace roots, the only
    /// thing that tells two worktrees apart.
    fn action_label(&self, grouped_count: usize) -> SharedString {
        if grouped_count <= 1 {
            self.title.clone()
        } else {
            self.workspace_root.clone()
        }
    }
}

/// The open "Rename project" dialog.
pub(super) struct ProjectRenameDialog {
    member: ProjectMember,
    title: Entity<InputState>,
}

/// The open "Project grouping" dialog. `selection` is `None` for "use the
/// global default", mirroring Electron's `"inherit"` sentinel.
pub(super) struct ProjectGroupingDialog {
    member: ProjectMember,
    selection: Option<ProjectGroupingMode>,
}

/// Build the four context-menu entries for a project header.
pub(super) fn project_context_menu(
    menu: PopupMenu,
    chat: &WeakEntity<ChatApp>,
    members: &[ProjectMember],
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let menu = targeted_item(menu, "Rename", members, window, cx, {
        let chat = chat.clone();
        move |member, window, cx| {
            let _ = chat.update(cx, |this, cx| this.open_project_rename(member, window, cx));
        }
    });
    let menu = targeted_item(menu, "Group into...", members, window, cx, {
        let chat = chat.clone();
        move |member, window, cx| {
            let _ = chat.update(cx, |this, cx| {
                this.open_project_grouping(member, window, cx)
            });
        }
    });
    let menu = targeted_item(menu, "Copy Path", members, window, cx, {
        let chat = chat.clone();
        move |member, _, cx| {
            let _ = chat.update(cx, |this, cx| this.copy_project_path(member, cx));
        }
    });
    targeted_item(menu, "Remove", members, window, cx, {
        let chat = chat.clone();
        move |member, window, cx| {
            let _ = chat.update(cx, |this, cx| {
                this.confirm_remove_project(member, window, cx)
            });
        }
    })
}

/// `buildTargetedItem`: inline for a single member, a submenu of members
/// otherwise.
fn targeted_item(
    menu: PopupMenu,
    label: &'static str,
    members: &[ProjectMember],
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
    run: impl Fn(&ProjectMember, &mut Window, &mut gpui::App) + Clone + 'static,
) -> PopupMenu {
    if let [member] = members {
        let member = member.clone();
        return menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
            run(&member, window, cx);
        }));
    }
    let members = members.to_vec();
    let grouped_count = members.len();
    menu.submenu(label, window, cx, move |mut menu, _, _| {
        for member in &members {
            let member = member.clone();
            let run = run.clone();
            menu = menu.item(
                PopupMenuItem::new(member.action_label(grouped_count)).on_click(
                    move |_, window, cx| {
                        run(&member, window, cx);
                    },
                ),
            );
        }
        menu
    })
}

impl ChatApp {
    fn copy_project_path(&mut self, member: &ProjectMember, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(member.workspace_root.to_string()));
    }

    // ---------------------------------------------------------------- rename

    fn open_project_rename(
        &mut self,
        member: &ProjectMember,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Project title")
                .default_value(member.title.clone())
        });
        self.project_rename = Some(ProjectRenameDialog {
            member: member.clone(),
            title,
        });
        let description: SharedString =
            format!("Update the title for {}.", member.workspace_root).into();
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let content_owner = owner.clone();
            let ok_owner = owner.clone();
            let close_owner = owner.clone();
            let description = description.clone();
            dialog
                .title("Rename project")
                .w(px(512.))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Save")
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .on_ok(move |_, window, cx| {
                    let _ = ok_owner.update(cx, |this, cx| this.submit_project_rename(window, cx));
                    true
                })
                .on_close(move |_, _, cx| {
                    let _ = close_owner.update(cx, |this, _| this.project_rename = None);
                })
                .content(move |content, _, cx| {
                    let Some(state) = content_owner.upgrade().and_then(|chat| {
                        chat.read(cx)
                            .project_rename
                            .as_ref()
                            .map(|d| d.title.clone())
                    }) else {
                        return content;
                    };
                    content.child(
                        v_flex()
                            .gap_4()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(description.clone()),
                            )
                            .child(
                                v_flex()
                                    .gap_1p5()
                                    .child(div().text_xs().font_medium().child("Project title"))
                                    .child(Input::new(&state)),
                            ),
                    )
                })
        });
    }

    /// `submitProjectRename`: an empty or unchanged title just closes.
    fn submit_project_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.project_rename.take() else {
            return;
        };
        let title = dialog.title.read(cx).value().trim().to_string();
        if title.is_empty() || title == dialog.member.title.as_ref() {
            return;
        }
        self.dispatch_project_command(
            ClientOrchestrationCommand::ProjectMetaUpdate {
                additional_roots: None,
                command_id: CommandId(fresh_id("vitre-cmd")),
                default_model_selection: None,
                project_id: dialog.member.project_id.clone(),
                scripts: None,
                title: Some(Some(TrimmedNonEmptyString(title))),
                r#type: Default::default(),
                workspace_root: None,
            },
            format!("Failed to rename \"{}\"", dialog.member.title),
            window,
            cx,
        );
    }

    // -------------------------------------------------------------- grouping

    fn open_project_grouping(
        &mut self,
        member: &ProjectMember,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_grouping = Some(ProjectGroupingDialog {
            member: member.clone(),
            selection: self.sidebar.project_grouping_override(&member.physical_key),
        });
        let description: SharedString = format!(
            "Choose how {} should be grouped in the sidebar.",
            member.workspace_root
        )
        .into();
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let content_owner = owner.clone();
            let ok_owner = owner.clone();
            let close_owner = owner.clone();
            let description = description.clone();
            dialog
                .title("Project grouping")
                .w(px(512.))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Save")
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .on_ok(move |_, _, cx| {
                    let _ = ok_owner.update(cx, |this, cx| this.save_project_grouping(cx));
                    true
                })
                .on_close(move |_, _, cx| {
                    let _ = close_owner.update(cx, |this, _| this.project_grouping = None);
                })
                .content(move |content, _, cx| {
                    let Some(chat) = content_owner.upgrade() else {
                        return content;
                    };
                    content.child(
                        v_flex()
                            .gap_4()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(description.clone()),
                            )
                            .child(chat.update(cx, |chat, cx| chat.render_grouping_choice(cx))),
                    )
                })
        });
    }

    /// The grouping-rule picker plus the chosen rule's description.
    ///
    /// Electron uses a `Select`; gpui-component's wants a searchable-list
    /// delegate for what is really a four-item radio, so this offers the same
    /// choices behind the menu button the rest of the app uses.
    fn render_grouping_choice(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        let global_mode = self.sidebar.grouping.mode;
        let selection = self
            .project_grouping
            .as_ref()
            .and_then(|dialog| dialog.selection);
        let trigger_label: SharedString = match selection {
            Some(mode) => mode.label().into(),
            None => format!("Use global default ({})", global_mode.label()).into(),
        };
        // The description tracks the *effective* rule, so "inherit" explains
        // what the global default will do.
        let description = selection.unwrap_or(global_mode).description();
        let owner = cx.entity().downgrade();

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_1p5()
                    .child(div().text_xs().font_medium().child("Grouping rule"))
                    .child(
                        Button::new("project-grouping-rule")
                            .outline()
                            .w_full()
                            .label(trigger_label)
                            .icon(Icon::new(IconName::ChevronDown).size_3p5())
                            .dropdown_menu(move |mut menu, _, _| {
                                let choices = std::iter::once(None)
                                    .chain(ProjectGroupingMode::ALL.into_iter().map(Some));
                                for choice in choices {
                                    let label: SharedString = match choice {
                                        Some(mode) => mode.label().into(),
                                        None => "Use global default".into(),
                                    };
                                    let owner = owner.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(label)
                                            .checked(choice == selection)
                                            .on_click(move |_, _, cx| {
                                                let _ = owner.update(cx, |this, cx| {
                                                    if let Some(dialog) = &mut this.project_grouping
                                                    {
                                                        dialog.selection = choice;
                                                    }
                                                    cx.notify();
                                                });
                                            }),
                                    );
                                }
                                menu
                            }),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(description),
            )
    }

    fn save_project_grouping(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.project_grouping.take() else {
            return;
        };
        self.sidebar
            .set_project_grouping_override(&dialog.member.physical_key, dialog.selection);
        self.rebuild_sidebar();
        cx.notify();
    }

    // ---------------------------------------------------------------- remove

    /// `handleRemoveProject`: a project holding threads gets a warning first
    /// and only proceeds under `force`; an empty one goes straight to the
    /// confirm.
    fn confirm_remove_project(
        &mut self,
        member: &ProjectMember,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let thread_count = self.project_thread_count(&member.project_id);
        if thread_count > 0 {
            self.warn_project_not_empty(member, window, cx);
            return;
        }
        self.ask_and_remove_project(member.clone(), false, 0, window, cx);
    }

    /// The "Project is not empty" warning, with its destructive escape hatch.
    fn warn_project_not_empty(
        &mut self,
        member: &ProjectMember,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = cx.entity().downgrade();
        let member = member.clone();
        window.push_notification(
            Notification::warning("Delete all threads in this project before removing it.")
                .title("Project is not empty")
                .action(move |_, _, cx| {
                    let owner = owner.clone();
                    let member = member.clone();
                    Button::new("remove-project-anyway")
                        .danger()
                        .label("Delete anyway")
                        .on_click(cx.listener(move |toast, _, window, cx| {
                            // The toast does not close itself once it carries
                            // an action, and the confirm has to come up over a
                            // dismissed one.
                            toast.dismiss(window, cx);
                            let _ = owner.update(cx, |this, cx| {
                                // Re-count now: threads may have gone away
                                // while the warning sat there.
                                let count = this.project_thread_count(&member.project_id);
                                this.ask_and_remove_project(
                                    member.clone(),
                                    true,
                                    count,
                                    window,
                                    cx,
                                );
                            });
                        }))
                }),
            cx,
        );
    }

    /// The final confirm, whose wording spells out exactly what is destroyed.
    fn ask_and_remove_project(
        &mut self,
        member: ProjectMember,
        force: bool,
        thread_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title: SharedString = if thread_count > 0 {
            format!(
                "Remove project \"{}\" and delete its {thread_count} thread{}?",
                member.title,
                if thread_count == 1 { "" } else { "s" }
            )
            .into()
        } else {
            format!("Remove project \"{}\"?", member.title).into()
        };
        let mut lines = vec![format!("Path: {}", member.workspace_root)];
        if thread_count > 0 {
            lines.push("This permanently clears conversation history for those threads.".into());
        }
        lines.push("This removes only this project entry.".into());
        if thread_count > 0 {
            lines.push("This action cannot be undone.".into());
        }
        let description: SharedString = lines.join("\n").into();

        let owner = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let owner = owner.clone();
            let member = member.clone();
            alert
                .confirm()
                .title(title.clone())
                .description(description.clone())
                .button_props(DialogButtonProps::default().ok_text("Remove"))
                .on_ok(move |_, window, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.remove_project(&member, force, window, cx);
                    });
                    true
                })
        });
    }

    fn remove_project(
        &mut self,
        member: &ProjectMember,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_project_command(
            ClientOrchestrationCommand::ProjectDelete {
                command_id: CommandId(fresh_id("vitre-cmd")),
                force: force.then_some(Some(true)),
                project_id: member.project_id.clone(),
                r#type: Default::default(),
            },
            format!("Failed to remove \"{}\"", member.title),
            window,
            cx,
        );
    }

    /// Unarchived threads belonging to one physical project.
    fn project_thread_count(&self, project_id: &ProjectId) -> usize {
        let Some(snapshot) = &self.shell.snapshot else {
            return 0;
        };
        snapshot
            .threads
            .iter()
            .filter(|thread| {
                thread.project_id == *project_id && !vitre_state::sidebar::is_archived(thread)
            })
            .count()
    }

    /// Fire a project command, surfacing a failure as a toast the way Electron
    /// does rather than through the inline chat error line.
    fn dispatch_project_command(
        &mut self,
        command: ClientOrchestrationCommand,
        failure_title: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let Err(error) = client.dispatch(&command).await else {
                return;
            };
            let _ = this.update_in(cx, |_, window, cx| {
                window.push_notification(
                    Notification::error(SharedString::from(format!("{error:?}")))
                        .title(SharedString::from(failure_title)),
                    cx,
                );
            });
        })
        .detach();
    }
}
