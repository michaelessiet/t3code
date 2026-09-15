//! Root selection is scoped to a thread; every file tab retains its root.
use super::*;
use vitre_contracts::WorkspaceRootRef;

impl ChatApp {
    pub(super) fn save_workspace_selection(&mut self) {
        self.right_panel.active_roots = self.active_roots.clone();
        self.right_panel.save();
    }
    pub(super) fn copy_to_thread_picker(
        &mut self,
        source: crate::files::FileContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let destinations: Arc<Vec<_>> = Arc::new(
            self.shell_threads()
                .into_iter()
                .filter_map(|thread| {
                    let cwd = thread
                        .worktree_path
                        .as_ref()
                        .map(|p| p.0.clone())
                        .or_else(|| self.project_root(&thread.project_id))?;
                    (cwd != source.cwd).then(|| (thread.title.0.clone(), cwd))
                })
                .collect(),
        );
        let owner = cx.weak_entity();
        window.open_dialog(cx,move |dialog,window,_| {
            let source=source.clone();let owner=owner.clone();let rows=destinations.clone();
            dialog.title(format!("Copy {} to a conversation",source.path)).w(window.rem_size()*36.)
                .child(div().text_sm().child("Copies the saved file or folder into that conversation’s checkout. Existing files are never overwritten."))
                .when(rows.is_empty(),|d|d.child("No other workspace is available yet."))
                .child(gpui::uniform_list("copy-thread-destinations",rows.len(),move |range,_,cx| {
                    let muted=cx.theme().muted_foreground;
                    range.map(|i|{let (title,cwd)=&rows[i];let cwd=cwd.clone();let source=source.clone();let owner=owner.clone();
                        Button::new(("copy-destination",i)).ghost().h_12().w_full().tooltip(cwd.clone())
                            .child(v_flex().w_full().min_w_0().child(div().text_sm().truncate().child(title.clone())).child(div().text_xs().text_color(muted).truncate().child(cwd.clone())))
                            .on_click(move |_,w,cx| {w.close_dialog(cx);let _=owner.update(cx,|app,cx|app.copy_workspace_entry(source.clone(),cwd.clone(),cx));})
                    }).collect::<Vec<_>>()
                }).h(gpui::rems(20.)))
        });
    }
    pub(super) fn copy_workspace_entry(
        &mut self,
        source: crate::files::FileContext,
        cwd: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let input = vitre_contracts::ProjectCopyEntryInput {
            from_cwd: tnes(source.cwd),
            from_relative_path: tnes(&source.path),
            to_cwd: tnes(cwd),
            to_relative_path: tnes(source.path),
            overwrite: Some(Some(false)),
        };
        cx.spawn(async move |this, cx| {
            let result = client
                .call::<vitre_contracts::methods::ProjectsCopyEntry>(&input)
                .await;
            let _ = this.update(cx, |app, cx| {
                if result.is_ok() {
                    for panel in app
                        .files
                        .iter()
                        .chain(app.files_cache.iter().map(|(_, p)| p))
                        .cloned()
                        .collect::<Vec<_>>()
                    {
                        if panel.read(cx).cwd() == input.to_cwd.0 {
                            panel.update(cx, |p, cx| p.refresh_after_copy(cx));
                        }
                    }
                }
                app.runtime_notice = Some(
                    match result {
                        Ok(_) => "Copied into the destination workspace.".into(),
                        Err(e) => e.user_message(),
                    }
                    .into(),
                );
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn workspace_roots(&self) -> Vec<String> {
        let mut roots = self.open_project_root().into_iter().collect::<Vec<_>>();
        if let Some(thread) = self.thread.as_ref().and_then(|t| self.shell_thread(&t.id)) {
            for root in thread
                .resolved_additional_roots
                .as_ref()
                .and_then(|r| r.as_ref())
                .into_iter()
                .flatten()
            {
                if let Some(path) = root.path.as_ref().and_then(|p| p.as_ref())
                    && !roots.contains(&path.0)
                {
                    roots.push(path.0.clone());
                }
            }
        }
        roots
    }
    pub(super) fn selected_workspace_root(&self) -> Option<String> {
        let id = &self.thread.as_ref()?.id.0;
        self.active_roots
            .get(id)
            .filter(|root| self.workspace_roots().contains(root))
            .cloned()
    }
    pub(super) fn root_for_file_tab(&self) -> Option<String> {
        self.selected_workspace_root()
            .filter(|r| Some(r) != self.open_project_root().as_ref())
    }
    fn additional_root_refs(&self) -> Vec<WorkspaceRootRef> {
        self.thread
            .as_ref()
            .and_then(|t| self.shell_thread(&t.id))
            .and_then(|t| t.additional_roots.as_ref())
            .and_then(|r| r.as_ref())
            .map(|roots| {
                roots
                    .iter()
                    .filter_map(|r| {
                        serde_json::to_value(r)
                            .ok()
                            .and_then(|v| serde_json::from_value(v).ok())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub(super) fn update_workspace_roots(
        &mut self,
        roots: Vec<WorkspaceRootRef>,
        cx: &mut Context<Self>,
    ) {
        let (Some(client), Some(thread)) = (self.client.clone(), self.thread.as_ref()) else {
            return;
        };
        let id = thread.id.clone();
        if self.workspace_updates.contains(&id) {
            return;
        }
        let removed: Vec<String> = self
            .shell_thread(&id)
            .and_then(|t| t.resolved_additional_roots.as_ref())
            .and_then(|r| r.as_ref())
            .into_iter()
            .flatten()
            .filter(|r| !roots.contains(&r.r#ref))
            .filter_map(|r| {
                r.path
                    .as_ref()
                    .and_then(|p| p.as_ref())
                    .map(|p| p.0.clone())
            })
            .collect();
        if self
            .files
            .iter()
            .chain(self.files_cache.iter().map(|(_, f)| f))
            .any(|f| {
                let panel = f.read(cx);
                removed.iter().any(|r| r == panel.cwd())
                    && panel.active_file_status().is_some_and(|(_, dirty)| dirty)
            })
        {
            self.runtime_notice = Some("Save changes in this folder before detaching it.".into());
            cx.notify();
            return;
        }
        let key = self.dock_thread_key();
        self.workspace_updates.insert(id.clone());
        let command = ClientOrchestrationCommand::ThreadMetaUpdate {
            additional_roots: Some(Some(roots)),
            branch: None,
            command_id: CommandId(fresh_id("workspace-roots")),
            expected_branch: None,
            model_selection: None,
            thread_id: thread.id.clone(),
            title: None,
            r#type: Default::default(),
            worktree_path: None,
        };
        cx.spawn(async move |this, cx| {
            let result=client.dispatch(&command).await;
            let _=this.update(cx,|app,cx| {
                app.workspace_updates.remove(&id);
                match result {
                    Err(error)=>app.runtime_notice=Some(error.user_message().into()),
                    Ok(_)=> {
                        if let Some(key)=key {
                            let tabs:Vec<_>=app.right_panel.map.thread(&key).surfaces.iter().filter(|s|matches!(s,vitre_state::right_panel::RightPanelSurface::File{root_path:Some(root),..} if removed.contains(root))).map(|s|s.id().to_string()).collect();
                            for tab in tabs {app.right_panel.map.close_surface(&key,&tab);}
                            app.right_panel.save();
                        }
                        if app.active_roots.get(&id.0).is_some_and(|r|removed.contains(r)) {app.active_roots.remove(&id.0);app.save_workspace_selection();}
                        app.sync_git_status(cx);app.sync_branch_toolbar(cx);
                    }
                }cx.notify();
            });
        }).detach();
    }
    pub(super) fn attach_workspace_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let thread_id = self.thread.as_ref().map(|t| t.id.clone());
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some("Attach workspace folders".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let _ = this.update(cx, |app, cx| {
                if app.thread.as_ref().map(|t| &t.id) != thread_id.as_ref() {
                    return;
                }
                let mut roots = app.additional_root_refs();
                for path in paths {
                    let root = WorkspaceRootRef::Path {
                        path: tnes(path.to_string_lossy()),
                    };
                    if !roots.contains(&root) {
                        roots.push(root);
                    }
                }
                app.update_workspace_roots(roots, cx);
            });
        })
        .detach();
    }
    pub(super) fn render_workspace_roots(&self, cx: &mut Context<Self>) -> AnyElement {
        let roots = self.workspace_roots();
        let selected = self.search_root();
        let label = format!(
            "{} {}",
            roots.len(),
            if roots.len() == 1 { "root" } else { "roots" }
        );
        let owner = cx.entity().downgrade();
        Button::new("workspace-roots")
            .ghost()
            .small()
            .icon(IconName::Folder)
            .label(label)
            .tooltip("Workspace folders and active repository")
            .disabled(
                self.thread
                    .as_ref()
                    .is_some_and(|t| self.workspace_updates.contains(&t.id)),
            )
            .dropdown_menu(move |menu, _, _| {
                let mut menu = menu;
                for root in &roots {
                    let path = root.clone();
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(format!(
                            "{}{}",
                            if selected.as_ref() == Some(root) {
                                "✓ "
                            } else {
                                ""
                            },
                            root
                        ))
                        .on_click(move |_, window, cx| {
                            let _ = owner.update(cx, |app, cx| {
                                if let Some(t) = &app.thread {
                                    app.active_roots.insert(t.id.0.clone(), path.clone());
                                }
                                app.save_workspace_selection();
                                app.sync_git_status(cx);
                                app.sync_branch_toolbar(cx);
                                app.dock_open_files_surface(window, cx);
                            });
                        }),
                    );
                }
                let attach_owner = owner.clone();
                menu = menu
                    .separator()
                    .item(
                        PopupMenuItem::new("Attach folder…").on_click(move |_, window, cx| {
                            let _ = attach_owner
                                .update(cx, |app, cx| app.attach_workspace_folder(window, cx));
                        }),
                    );
                for root in roots.iter().skip(1) {
                    let path = root.clone();
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(format!("Detach {root}")).on_click(
                        move |_, _, cx| {
                            let _ = owner.update(cx, |app, cx| {
                                let resolved = app
                                    .thread
                                    .as_ref()
                                    .and_then(|t| app.shell_thread(&t.id))
                                    .and_then(|t| t.resolved_additional_roots.as_ref())
                                    .and_then(|r| r.as_ref());
                                let target = resolved
                                    .and_then(|roots| {
                                        roots.iter().find(|r| {
                                            r.path
                                                .as_ref()
                                                .and_then(|p| p.as_ref())
                                                .is_some_and(|p| p.0 == path)
                                        })
                                    })
                                    .map(|r| r.r#ref.clone());
                                let mut refs = app.additional_root_refs();
                                refs.retain(|r| Some(r) != target.as_ref());
                                app.update_workspace_roots(refs, cx);
                            });
                        },
                    ));
                }
                menu
            })
            .into_any_element()
    }
}
