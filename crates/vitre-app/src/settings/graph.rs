//! Knowledge Graph preferences use the same sidecar keys as the web client.
use super::*;
use gpui_component::Disableable as _;
use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct GraphSettingsUi {
    runtime: Option<vitre_contracts::GraphRuntimeStatus>,
    status: Option<String>,
    busy: bool,
}

fn value(owner: &WeakEntity<SettingsPanel>, key: &str, default: Value, cx: &App) -> Value {
    owner
        .upgrade()
        .and_then(|p| serde_json::to_value(p.read(cx).server_settings()).ok())
        .and_then(|s| s["knowledgeGraph"].get(key).cloned())
        .unwrap_or(default)
}

impl SettingsPanel {
    pub(super) fn refresh_graph_runtime(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client
                .call::<vitre_contracts::methods::GraphRuntimeStatus>(&json!({}))
                .await;
            let _ = this.update(cx, |p, cx| {
                match result {
                    Ok(runtime) => p.graph.runtime = Some(runtime),
                    Err(e) => p.graph.status = Some(e.user_message()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn install_graph_runtime(&mut self, cx: &mut Context<Self>) {
        if self.graph.busy {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.graph.busy = true;
        self.graph.status = Some("Checking for an existing install…".into());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = async {
                let mut stream = client
                    .subscribe::<vitre_contracts::methods::GraphInstallRuntime>(
                        &vitre_contracts::GraphInstallRuntimeInput {
                            interpreter_path: None,
                        },
                    )
                    .map_err(|e| e.to_string())?;
                while let Some(event) = stream.next().await {
                    match event {
                        vitre_rpc::TypedStreamEvent::Values(events) => {
                            stream.ack().map_err(|e| e.to_string())?;
                            for event in events {
                                match event {
                                    vitre_contracts::GraphInstallEvent::Progress {
                                        detail,
                                        stage,
                                    } => {
                                        let _ = this.update(cx, |p, cx| {
                                            p.graph.status =
                                                Some(detail.map(|d| d.0).unwrap_or_else(|| {
                                                    enum_wire_name(&stage).replace('_', " ")
                                                }));
                                            cx.notify();
                                        });
                                    }
                                    vitre_contracts::GraphInstallEvent::Complete { .. } => {
                                        return Ok::<_, String>(());
                                    }
                                    _ => {}
                                }
                            }
                        }
                        vitre_rpc::TypedStreamEvent::Completed(result) => {
                            result.map_err(|e| e.user_message())?;
                            return Ok(());
                        }
                    }
                }
                Err("Graph installer disconnected before completion.".into())
            }
            .await;
            let _ = this.update(cx, |p, cx| {
                p.graph.busy = false;
                p.graph.status = Some(match result {
                    Ok(()) => "Graph tools installed.".into(),
                    Err(e) => e,
                });
                p.refresh_graph_runtime(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn graph_page(&self, cx: &Context<Self>) -> SettingPage {
        let owner = cx.entity().downgrade();
        let ready = self.server_settings().is_some();
        let mut group = SettingGroup::new();
        for (key, label, description) in [
            (
                "enabled",
                "Knowledge graph",
                "Build a code knowledge graph for your projects.",
            ),
            (
                "autoRebuild",
                "Auto-rebuild",
                "Rebuild when workspace files change.",
            ),
        ] {
            let get = owner.clone();
            let set = owner.clone();
            group = group.item(
                SettingItem::new(
                    label,
                    SettingField::switch(
                        move |cx: &App| {
                            value(&get, key, json!(false), cx)
                                .as_bool()
                                .unwrap_or(false)
                        },
                        move |v, cx: &mut App| {
                            let _ = set
                                .update(cx, |p, cx| p.patch(json!({"knowledgeGraph":{key:v}}), cx));
                        },
                    )
                    .default_value(false),
                )
                .description(description)
                .disabled(!ready),
            );
        }
        let runtime = self.graph.runtime.clone();
        let status = self.graph.status.clone();
        let busy = self.graph.busy;
        let refresh = owner.clone();
        let enabled = serde_json::to_value(self.server_settings())
            .ok()
            .and_then(|s| s["knowledgeGraph"]["enabled"].as_bool())
            .unwrap_or(false);
        group = group.item(SettingItem::render(move |_, _, cx| {
            let refresh = refresh.clone();
            let install = refresh.clone();
            let state = runtime
                .as_ref()
                .map(|r| enum_wire_name(&r.state))
                .unwrap_or("checking".into());
            let mut controls = h_flex().gap_2().child(
                Button::new("refresh-graph-runtime")
                    .ghost()
                    .small()
                    .icon(crate::assets::VitreIcon::RefreshCw)
                    .tooltip("Refresh graph runtime")
                    .on_click(move |_, _, cx| {
                        let _ = refresh.update(cx, |p, cx| p.refresh_graph_runtime(cx));
                    }),
            );
            if enabled && state != "ready" {
                controls = controls.child(
                    Button::new("install-graphify")
                        .small()
                        .label(if busy {
                            "Installing…"
                        } else {
                            "Install graphify"
                        })
                        .disabled(busy)
                        .on_click(move |_, _, cx| {
                            let _ = install.update(cx, |p, cx| p.install_graph_runtime(cx));
                        }),
                );
            }
            h_flex()
                .w_full()
                .gap_4()
                .child(
                    v_flex()
                        .flex_1()
                        .gap_1()
                        .child(div().text_sm().font_medium().child(format!(
                            "Runtime · {}",
                            match state.as_str() {
                                "ready" => "Ready",
                                "missing" => "Not installed",
                                "disabled" => "Disabled",
                                "failed" => "Failed",
                                "installing" => "Installing",
                                _ => "Checking…",
                            }
                        )))
                        .children(runtime.as_ref().and_then(|r| r.detail.as_ref()).map(|d| {
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(d.0.clone())
                        }))
                        .children(status.clone().map(|s| {
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(s)
                        })),
                )
                .child(controls)
                .into_any_element()
        }));
        let get = owner.clone();
        let set = owner.clone();
        group = group.item(
            SettingItem::new(
                "graphify path",
                SettingField::input(
                    move |cx: &App| {
                        value(&get, "graphifyPath", json!(""), cx)
                            .as_str()
                            .unwrap_or_default()
                            .to_string()
                            .into()
                    },
                    move |v: SharedString, cx: &mut App| {
                        let _ = set.update(cx, |p, cx| {
                            p.patch(json!({"knowledgeGraph":{"graphifyPath":v.as_ref()}}), cx)
                        });
                    },
                )
                .default_value(SharedString::default()),
            )
            .description(
                "Absolute path to graphify or its Python interpreter. Leave empty to auto-detect.",
            )
            .disabled(!ready),
        );
        let mut storage = SettingGroup::new().title("Storage");
        for (key, label, default, description) in [
            (
                "retentionDays",
                "Retention days",
                60.,
                "Remove graphs this many days after last opening them. 0 disables age eviction.",
            ),
            (
                "maxStoreMegabytes",
                "Maximum storage (MB)",
                2048.,
                "Size budget for the graph store. 0 disables size eviction.",
            ),
        ] {
            let get = owner.clone();
            let set = owner.clone();
            storage = storage.item(
                SettingItem::new(
                    label,
                    SettingField::number_input(
                        NumberFieldOptions {
                            min: 0.,
                            max: 1_000_000.,
                            step: 1.,
                        },
                        move |cx: &App| {
                            value(&get, key, json!(default), cx)
                                .as_f64()
                                .unwrap_or(default)
                        },
                        move |v, cx: &mut App| {
                            let _ = set.update(cx, |p, cx| {
                                p.patch(json!({"knowledgeGraph":{key:v.round() as u64}}), cx)
                            });
                        },
                    )
                    .default_value(default),
                )
                .description(description)
                .disabled(!ready),
            );
        }
        SettingPage::new("Knowledge Graph")
            .group(group)
            .group(storage)
    }
}
