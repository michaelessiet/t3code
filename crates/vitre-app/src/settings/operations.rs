//! Sidecar-backed maintenance pages. Mutations are explicit, never performed
//! by opening Settings; errors remain visible and refresh is always available.
use super::*;
use gpui::AppContext as _;
use gpui_component::input::{Input, InputState};
use gpui_component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_component::{Disableable as _, WindowExt as _, notification::Notification};
use serde_json::{Value, json};
use vitre_contracts::{methods::*, *};

#[derive(Default)]
pub(super) struct Operations {
    pub(super) lsp_status: Option<String>,
    archives: Vec<OrchestrationThreadShell>,
    diagnostics: Option<Value>,
    discovery: Option<Value>,
    status: Option<String>,
    busy: bool,
    install: Option<gpui::Task<()>>,
}

impl SettingsPanel {
    pub(super) fn refresh_lsp_status(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd.clone() else {
            self.operations.lsp_status =
                Some("Open a workspace thread first to inspect its language servers.".into());
            cx.notify();
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        self.operations.lsp_status = Some("Loading language server status…".into());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client
                .call::<methods::LspServerStatus>(&LspServerStatusPayload {
                    cwd: TrimmedNonEmptyString(cwd.clone()),
                })
                .await;
            let message = match result {
                Ok(status) => format!(
                    "{cwd}\n{}",
                    status
                        .servers
                        .into_iter()
                        .map(|s| format!("{}: {:?}", s.display_name.0, s.state))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                Err(e) => e.user_message(),
            };
            let _ = this.update(cx, |p, cx| {
                p.operations.lsp_status = Some(message);
                cx.notify();
            });
        })
        .detach();
    }
    fn repository_form(
        &mut self,
        operation: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fields = if operation == "clone" {
            vec![
                ("Remote URL or owner/repository", ""),
                ("Destination directory (absolute path)", ""),
                (
                    "Provider: github / gitlab / azure-devops / bitbucket",
                    "github",
                ),
            ]
        } else {
            vec![
                ("Local repository directory (absolute path)", ""),
                ("Repository name or owner/name", ""),
                (
                    "Provider: github / gitlab / azure-devops / bitbucket",
                    "github",
                ),
                ("Visibility: private / public", "private"),
            ]
        };
        let fields = fields
            .into_iter()
            .map(|(label, value)| {
                (
                    label,
                    cx.new(|cx| InputState::new(window, cx).default_value(value)),
                )
            })
            .collect::<Vec<_>>();
        let owner = cx.entity().downgrade();
        window.open_dialog(cx,move|dialog,_,_|{
            let fields=fields.clone();let owner=owner.clone();
            let mut body=v_flex().gap_2();for(label,input)in &fields{body=body.child(*label).child(Input::new(input));}
            dialog.title(if operation=="clone"{"Clone repository"}else{"Publish repository"}).child(body).child(if operation=="clone"{"Creates files in the destination. Existing directories are never overwritten."}else{"Creates a remote repository and pushes local commits. Choose visibility carefully."}).on_ok(move|_,_,cx|{
                let values=fields.iter().map(|(_,i)|i.read(cx).value().trim().to_string()).collect::<Vec<_>>();
                if values.iter().any(String::is_empty){return false;}
                let _=owner.update(cx,|p,cx|p.repository_operation(operation,values,cx));true
            })
        });
    }

    fn repository_operation(
        &mut self,
        operation: &'static str,
        values: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        if self.operations.busy {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.operations.busy = true;
        self.operations.status = Some(format!("{operation} in progress…"));
        cx.notify();
        cx.spawn(async move|this,cx|{
            let result=async {
                if operation=="clone"{
                    let mut payload=json!({"destinationPath":values[1],"provider":values[2]});payload[if values[0].contains(":"){"remoteUrl"}else{"repository"}]=json!(values[0]);
                    let input=serde_json::from_value(payload).map_err(|e|e.to_string())?;
                    client.call::<SourceControlCloneRepository>(&input).await.map(|v|format!("Cloned to {}. Use Add project to open it.",v.cwd.0)).map_err(|e|e.user_message())
                }else{
                    let input=serde_json::from_value(json!({"cwd":values[0],"repository":values[1],"provider":values[2],"visibility":values[3]})).map_err(|e|e.to_string())?;
                    client.call::<SourceControlPublishRepository>(&input).await.map(|v|format!("Published: {}",v.remote_url.0)).map_err(|e|e.user_message())
                }
            }.await;
            let _=this.update(cx,|p,cx|{p.operations.busy=false;p.operations.status=Some(result.unwrap_or_else(|e|e));cx.notify();});
        }).detach();
    }
    pub(super) fn update_provider(
        &mut self,
        provider: ServerProviderUpdateInput,
        cx: &mut Context<Self>,
    ) {
        if self.operations.busy {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.operations.busy = true;
        self.operations.status = Some("Updating provider…".into());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.call::<ServerUpdateProvider>(&provider).await;
            let _ = this.update(cx, |p, cx| {
                p.operations.busy = false;
                match result {
                    Ok(v) => {
                        if let Some(config) = p.config.as_mut() {
                            config.providers = v.providers;
                        }
                        p.operations.status = Some("Provider update completed.".into());
                    }
                    Err(e) => p.operations.status = Some(e.user_message()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn operations_status(&self) -> Option<String> {
        self.operations.status.clone()
    }
    pub(super) fn refresh_operations(&mut self, cx: &mut Context<Self>) {
        if self.operations.busy {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.operations.busy = true;
        self.operations.status = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let archives = client
                .call::<OrchestrationGetArchivedShellSnapshot>(&json!({}))
                .await;
            let processes = client.call::<ServerGetProcessDiagnostics>(&json!({})).await;
            let traces = client.call::<ServerGetTraceDiagnostics>(&json!({})).await;
            let history = client
                .call::<ServerGetProcessResourceHistory>(&ServerProcessResourceHistoryInput {
                    bucket_ms: NonNegativeInt(5000),
                    window_ms: NonNegativeInt(300000),
                })
                .await;
            let discovery = client.call::<ServerDiscoverSourceControl>(&json!({})).await;
            let _ = this.update(cx, |panel, cx| {
                let mut errors = Vec::new();
                match archives {
                    Ok(snapshot) => panel.operations.archives = snapshot.threads,
                    Err(e) => errors.push(e.user_message()),
                }
                let process = match processes {
                    Ok(v) => serde_json::to_value(v).unwrap_or(Value::Null),
                    Err(e) => {
                        errors.push(e.user_message());
                        Value::Null
                    }
                };
                let trace = match traces {
                    Ok(v) => serde_json::to_value(v).unwrap_or(Value::Null),
                    Err(e) => {
                        errors.push(e.user_message());
                        Value::Null
                    }
                };
                let history = match history {
                    Ok(v) => serde_json::to_value(v).unwrap_or(Value::Null),
                    Err(e) => {
                        errors.push(e.user_message());
                        Value::Null
                    }
                };
                panel.operations.diagnostics =
                    Some(json!({"processes":process,"traces":trace,"resourceHistory":history}));
                match discovery {
                    Ok(v) => panel.operations.discovery = serde_json::to_value(v).ok(),
                    Err(e) => errors.push(e.user_message()),
                }
                panel.operations.status = (!errors.is_empty()).then(|| errors.join("\n"));
                panel.operations.busy = false;
                cx.notify();
            });
        })
        .detach();
    }

    fn archive_action(
        &mut self,
        id: ThreadId,
        delete: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = cx.entity().downgrade();
        if delete {
            window.open_dialog(cx,move|dialog,_,_|{
                let owner=owner.clone();let id=id.clone();
                dialog.title("Delete archived thread?").child("This permanently removes the conversation. Its project files are not deleted.")
                    .on_ok(move|_,window,cx|{let _=owner.update(cx,|panel,cx|panel.dispatch_archive(id.clone(),true,window,cx));true})
            });
        } else {
            self.dispatch_archive(id, false, window, cx);
        }
    }

    fn dispatch_archive(
        &mut self,
        id: ThreadId,
        delete: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let command:ClientOrchestrationCommand=serde_json::from_value(json!({"type":if delete{"thread.delete"}else{"thread.unarchive"},"threadId":id,"commandId":format!("settings-{}-{}",std::process::id(),chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default())})).expect("archive command shape");
        cx.spawn_in(window, async move |this, cx| {
            let result = client.dispatch(&command).await;
            let _ = this.update_in(cx, |panel, window, cx| {
                match result {
                    Ok(_) => {
                        panel.operations.archives.retain(|t| t.id != id);
                        window.push_notification(
                            Notification::success(if delete {
                                "Thread deleted"
                            } else {
                                "Thread restored"
                            }),
                            cx,
                        );
                    }
                    Err(e) => {
                        window.push_notification(Notification::error(e.user_message()), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn install_claude(&mut self, cx: &mut Context<Self>) {
        if self.operations.install.is_some() {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.operations.status = Some("Installing the managed Claude binary…".into());
        self.operations.install = Some(cx.spawn(async move |this, cx| {
            let result = async {
                let mut stream = client
                    .subscribe::<ClaudeInstallBinary>(&json!({}))
                    .map_err(|e| e.to_string())?;
                while let Some(event) = stream.next().await {
                    match event {
                        vitre_rpc::TypedStreamEvent::Values(events) => {
                            for event in events {
                                let text = serde_json::to_string(&event).unwrap_or_default();
                                let _ = this.update(cx, |panel, cx| {
                                    panel.operations.status = Some(text);
                                    cx.notify();
                                });
                            }
                            stream.ack().map_err(|e| e.to_string())?;
                        }
                        vitre_rpc::TypedStreamEvent::Completed(result) => {
                            return result.map_err(|e| e.user_message());
                        }
                    }
                }
                Ok::<_, String>(())
            }
            .await;
            let _ = this.update(cx, |panel, cx| {
                panel.operations.install = None;
                if let Err(e) = result {
                    panel.operations.status = Some(e);
                }
                panel.load(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn operations_header(&self, cx: &Context<Self>) -> SettingGroup {
        let owner = cx.entity().downgrade();
        let busy = self.operations.busy;
        let status = self.operations.status.clone();
        SettingGroup::new().item(SettingItem::render(move |_: &RenderOptions, _, cx| {
            let owner = owner.clone();
            v_flex()
                .gap_2()
                .child(
                    Button::new("refresh-maintenance")
                        .label(if busy { "Loading…" } else { "Refresh" })
                        .disabled(busy)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |panel, cx| panel.refresh_operations(cx));
                        }),
                )
                .children(status.clone().map(|s| {
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(s)
                }))
                .into_any_element()
        }))
    }

    pub(super) fn archives_page(&self, cx: &Context<Self>) -> SettingPage {
        let mut group = self.operations_header(cx);
        if self.operations.archives.is_empty() {
            group = group.item(SettingItem::render(|_: &RenderOptions, _, _| {
                div()
                    .child("No archived threads loaded. Use Refresh to check the environment.")
                    .into_any_element()
            }));
        }
        for thread in &self.operations.archives {
            let owner = cx.entity().downgrade();
            let id = thread.id.clone();
            let title = thread.title.0.clone();
            group =
                group.item(SettingItem::render(move |_: &RenderOptions, _, _| {
                    let restore = owner.clone();
                    let remove = owner.clone();
                    let restore_id = id.clone();
                    let remove_id = id.clone();
                    let menu_owner = owner.clone();
                    let menu_id = id.clone();
                    h_flex()
                        .id(SharedString::from(format!("archived-thread-{}", id.0)))
                        .w_full()
                        .gap_2()
                        .child(div().flex_1().child(title.clone()))
                        .child(Button::new("restore").label("Restore").on_click(
                            move |_, window, cx| {
                                let _ = restore.update(cx, |p, cx| {
                                    p.archive_action(restore_id.clone(), false, window, cx)
                                });
                            },
                        ))
                        .child(Button::new("delete").label("Delete…").on_click(
                            move |_, window, cx| {
                                let _ = remove.update(cx, |p, cx| {
                                    p.archive_action(remove_id.clone(), true, window, cx)
                                });
                            },
                        ))
                        .context_menu(move |menu, _, _| {
                            let restore = menu_owner.clone();
                            let restore_id = menu_id.clone();
                            let remove = menu_owner.clone();
                            let remove_id = menu_id.clone();
                            menu.item(PopupMenuItem::new("Unarchive").on_click(
                                move |_, window, cx| {
                                    let _ = restore.update(cx, |p, cx| {
                                        p.archive_action(restore_id.clone(), false, window, cx)
                                    });
                                },
                            ))
                            .separator()
                            .item(
                                PopupMenuItem::new("Delete")
                                    .icon(crate::assets::VitreIcon::Trash2)
                                    .on_click(move |_, window, cx| {
                                        let _ = remove.update(cx, |p, cx| {
                                            p.archive_action(remove_id.clone(), true, window, cx)
                                        });
                                    }),
                            )
                        })
                        .into_any_element()
                }));
        }
        SettingPage::new("Archived threads")
            .resettable(false)
            .group(group)
    }

    pub(super) fn diagnostics_page(&self, cx: &Context<Self>) -> SettingPage {
        let mut processes = SettingGroup::new().title("Processes");
        if let Some(report) = &self.operations.diagnostics {
            for process in report["processes"]["processes"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let Some(pid) = process["pid"].as_i64() else {
                    continue;
                };
                let protected = Some(pid) == report["processes"]["serverPid"].as_i64();
                let label = format!(
                    "PID {pid} · {} · CPU {}% · {:.1} MiB",
                    process["command"].as_str().unwrap_or("Process"),
                    process["cpuPercent"],
                    process["rssBytes"].as_f64().unwrap_or(0.) / 1048576.
                );
                let owner = cx.entity().downgrade();
                processes=processes.item(SettingItem::render(move|_:&RenderOptions,_,_|{
                    let owner=owner.clone();h_flex().gap_2().child(div().flex_1().text_xs().child(label.clone())).child(Button::new("interrupt-process").label("Interrupt…").disabled(protected).on_click(move|_,window,cx|{
                        let owner=owner.clone();window.open_dialog(cx,move|dialog,_,_|{
                            let owner=owner.clone();dialog.title(format!("Interrupt process {pid}?")).child("Sends SIGINT to this environment-owned process. Running work may stop.").on_ok(move|_,_,cx|{let _=owner.update(cx,|p,cx|p.signal_process(pid,cx));true})
                        });
                    })).into_any_element()
                }));
            }
        }
        let report = self
            .operations
            .diagnostics
            .clone()
            .map(|v| serde_json::to_string_pretty(&v).unwrap_or_default())
            .unwrap_or_else(|| "Refresh to collect process metrics and recent traces.".into());
        SettingPage::new("Diagnostics")
            .resettable(false)
            .group(self.operations_header(cx))
            .group(processes)
            .group(
                SettingGroup::new().item(SettingItem::render(move |_: &RenderOptions, _, _| {
                    let copy = report.clone();
                    v_flex()
                        .gap_2()
                        .child(
                            Button::new("copy-diagnostics")
                                .label("Copy report")
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        copy.clone(),
                                    ))
                                }),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_family("monospace")
                                .child(report.clone()),
                        )
                        .into_any_element()
                })),
            )
    }

    fn signal_process(&mut self, pid: i64, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client
                .call::<ServerSignalProcess>(&ServerSignalProcessInput {
                    pid: PositiveInt(pid),
                    signal: ServerProcessSignal::SIGINT,
                })
                .await;
            let _ = this.update(cx, |p, cx| {
                p.operations.status = Some(match result {
                    Ok(v) => {
                        if v.signaled {
                            "Interrupt sent".into()
                        } else {
                            "The process was not signaled".into()
                        }
                    }
                    Err(e) => e.user_message(),
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn source_control_page(&self, cx: &Context<Self>) -> SettingPage {
        let owner = cx.entity().downgrade();
        let report = self
            .operations
            .discovery
            .clone()
            .map(|v| serde_json::to_string_pretty(&v).unwrap_or_default())
            .unwrap_or_else(|| {
                "Refresh to discover installed Git and source-control providers.".into()
            });
        SettingPage::new("Source control")
            .resettable(false)
            .group(
                SettingGroup::new().item(SettingItem::render(move |_: &RenderOptions, _, _| {
                    let clone = owner.clone();
                    let publish = owner.clone();
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("clone-repo")
                                .label("Clone repository…")
                                .on_click(move |_, window, cx| {
                                    let _ = clone
                                        .update(cx, |p, cx| p.repository_form("clone", window, cx));
                                }),
                        )
                        .child(
                            Button::new("publish-repo")
                                .label("Publish repository…")
                                .on_click(move |_, window, cx| {
                                    let _ = publish.update(cx, |p, cx| {
                                        p.repository_form("publish", window, cx)
                                    });
                                }),
                        )
                        .into_any_element()
                })),
            )
            .group(self.operations_header(cx))
            .group(
                SettingGroup::new().item(SettingItem::render(move |_: &RenderOptions, _, _| {
                    div()
                        .text_xs()
                        .font_family("monospace")
                        .child(report.clone())
                        .into_any_element()
                })),
            )
    }
}
