//! Editable provider and language-server forms with conflict checks on save.
use super::*;
use gpui::{AppContext as _, Entity};
use gpui_component::{
    Disableable as _, WindowExt as _,
    input::{Input, InputState},
    scroll::ScrollableElement as _,
};
use serde_json::{Value, json};

#[derive(Clone)]
pub(super) enum FormKind {
    Provider(String),
    LanguageServer(String),
}
pub(super) struct SettingsForm {
    kind: FormKind,
    original: Option<Value>,
    base: Value,
    legacy_original: Option<(String, Value)>,
    fields: Vec<(&'static str, Entity<InputState>)>,
    busy: bool,
    enabled: bool,
    error: Option<SharedString>,
}

impl SettingsPanel {
    #[cfg(debug_assertions)]
    pub(crate) fn verification_ready(&self) -> bool {
        self.server_settings().is_some() && self.config.is_some()
    }

    #[cfg(debug_assertions)]
    pub(crate) fn verify_provider_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> tokio::sync::oneshot::Receiver<Result<(), String>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let id = format!("vitre-verification-{}", std::process::id());
        self.open_form(FormKind::Provider(id.clone()), window, cx);
        if let Some(form) = self.form.as_mut() {
            form.enabled = false;
            form.fields[1]
                .1
                .update(cx, |i, cx| i.set_value("codex", window, cx));
            form.fields[2]
                .1
                .update(cx, |i, cx| i.set_value("Native verification", window, cx));
        }
        self.submit_form(false, window, cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = async {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(100))
                        .await;
                    let done = this
                        .update(cx, |p, _| {
                            if let Some(form) = &p.form {
                                if let Some(error) = &form.error {
                                    Err(error.to_string())
                                } else {
                                    Ok(false)
                                }
                            } else {
                                Ok(true)
                            }
                        })
                        .map_err(|e| e.to_string())??;
                    if done {
                        break;
                    }
                }
                this.update_in(cx, |p, window, cx| {
                    let settings = serde_json::to_value(p.server_settings()).unwrap_or(Value::Null);
                    if settings["providerInstances"][&id]["enabled"] != false {
                        return Err("Provider form did not persist its disabled state".into());
                    }
                    p.open_form(FormKind::Provider(id.clone()), window, cx);
                    p.submit_form(true, window, cx);
                    Ok::<_, String>(())
                })
                .map_err(|e| e.to_string())??;
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(100))
                        .await;
                    let done = this
                        .update(cx, |p, _| {
                            if let Some(form) = &p.form {
                                if let Some(error) = &form.error {
                                    Err(error.to_string())
                                } else {
                                    Ok(false)
                                }
                            } else {
                                Ok(true)
                            }
                        })
                        .map_err(|e| e.to_string())??;
                    if done {
                        break;
                    }
                }
                Ok(())
            }
            .await;
            let _ = tx.send(result);
        })
        .detach();
        rx
    }

    pub(super) fn open_form(
        &mut self,
        kind: FormKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = serde_json::to_value(self.server_settings()).unwrap_or(Value::Null);
        let original = match &kind {
            FormKind::Provider(id) => settings
                .get("providerInstances")
                .and_then(|m| m.get(id))
                .cloned(),
            FormKind::LanguageServer(id) => settings["languageServers"]
                .as_array()
                .and_then(|a| a.iter().find(|s| s["serverId"].as_str() == Some(id)))
                .cloned(),
        };
        let mut existing = original.clone().unwrap_or(json!({}));
        let mut legacy_original = None;
        if original.is_none()
            && let FormKind::Provider(id) = &kind
            && let Some(provider) = self
                .config
                .as_ref()
                .and_then(|c| c.providers.iter().find(|p| &p.instance_id.0 == id))
        {
            existing["driver"] = serde_json::to_value(&provider.driver).unwrap();
            existing["enabled"] = json!(provider.enabled);
            let driver = enum_wire_name(&provider.driver);
            let legacy = settings["providers"][&driver].clone();
            existing["config"] = legacy.clone();
            legacy_original = Some((driver, legacy));
        }
        let text = |key: &str| existing[key].as_str().unwrap_or("").to_string();
        let (title, fields) = match &kind {
            FormKind::Provider(id) => (
                "Provider instance",
                vec![
                    ("Instance ID", id.clone()),
                    ("Driver", text("driver")),
                    ("Display name", text("displayName")),
                    ("Accent color (#RRGGBB)", text("accentColor")),
                    (
                        "Binary path",
                        existing["config"]["binaryPath"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                    ),
                    ("API key environment variable", String::new()),
                    ("API key (leave blank to keep existing)", String::new()),
                    (
                        "Custom models (space separated)",
                        existing["config"]["customModels"]
                            .as_array()
                            .map(|v| {
                                v.iter()
                                    .filter_map(Value::as_str)
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .unwrap_or_default(),
                    ),
                ],
            ),
            FormKind::LanguageServer(id) => (
                "Language server",
                vec![
                    ("Server ID", id.clone()),
                    ("Display name", text("displayName")),
                    ("Language ID", text("languageId")),
                    ("Command", text("command")),
                    (
                        "Extensions (space separated)",
                        existing["extensions"]
                            .as_array()
                            .map(|v| {
                                v.iter()
                                    .filter_map(Value::as_str)
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .unwrap_or_default(),
                    ),
                    (
                        "Arguments (JSON array of strings)",
                        existing["args"]
                            .as_array()
                            .map(|v| serde_json::to_string(v).unwrap_or_default())
                            .unwrap_or_else(|| "[]".into()),
                    ),
                ],
            ),
        };
        let fields = fields
            .into_iter()
            .map(|(label, value)| {
                let state = cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(value)
                        .masked(label.starts_with("API key ("))
                });
                (label, state)
            })
            .collect();
        self.form = Some(SettingsForm {
            kind,
            original,
            base: existing.clone(),
            legacy_original,
            fields,
            busy: false,
            enabled: existing["enabled"].as_bool().unwrap_or(true),
            error: None,
        });
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            dialog
                .title(title)
                .w(px(600.))
                .content(move |content, _, cx| {
                    let Some(panel) = owner.upgrade() else {
                        return content;
                    };
                    let Some(form) = panel.read(cx).form.as_ref() else {
                        return content;
                    };
                    let mut body = v_flex().gap_3().max_h(px(540.)).overflow_y_scrollbar();
                    if matches!(form.kind, FormKind::Provider(_)) {
                        let owner = owner.clone();
                        body = body.child(
                            gpui_component::switch::Switch::new("provider-enabled")
                                .label("Enabled")
                                .checked(form.enabled)
                                .on_click(move |checked: &bool, _, cx| {
                                    let _ = owner.update(cx, |p, cx| {
                                        if let Some(form) = p.form.as_mut() {
                                            form.enabled = *checked;
                                        }
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    for (label, input) in &form.fields {
                        body = body.child(v_flex().gap_1().child(*label).child(Input::new(input)));
                    }
                    let save = owner.clone();
                    let delete = owner.clone();
                    content.child(
                        body.children(
                            form.error
                                .clone()
                                .map(|e| div().text_sm().text_color(cx.theme().danger).child(e)),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Button::new("save-settings-form")
                                        .label("Save")
                                        .disabled(form.busy)
                                        .on_click(move |_, window, cx| {
                                            let _ = save.update(cx, |panel, cx| {
                                                panel.submit_form(false, window, cx)
                                            });
                                        }),
                                )
                                .child(
                                    Button::new("delete-settings-form")
                                        .label("Delete")
                                        .disabled(form.busy || form.original.is_none())
                                        .on_click(move |_, window, cx| {
                                            let _ = delete.update(cx, |panel, cx| {
                                                panel.submit_form(true, window, cx)
                                            });
                                        }),
                                ),
                        ),
                    )
                })
        });
    }

    fn submit_form(&mut self, delete: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if form.busy {
            return;
        }
        let values = form
            .fields
            .iter()
            .map(|(_, v)| v.read(cx).value().to_string())
            .collect::<Vec<_>>();
        let kind = form.kind.clone();
        let original = form.original.clone();
        let legacy_original = form.legacy_original.clone();
        let mut replacement = match if delete {
            Ok(json!({}))
        } else {
            build_replacement(&kind, &values, Some(&form.base))
        } {
            Ok(value) => value,
            Err(error) => {
                form.error = Some(error.into());
                cx.notify();
                return;
            }
        };
        if matches!(kind, FormKind::Provider(_)) {
            replacement["enabled"] = json!(form.enabled);
        }
        form.busy = true;
        form.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let current = client
                    .call::<ServerGetSettings>(&json!({}))
                    .await
                    .map_err(|e| e.user_message())?;
                let current = serde_json::to_value(current).map_err(|e| e.to_string())?;
                if let Some((driver, original)) = &legacy_original
                    && current["providers"][driver] != *original
                {
                    return Err("Provider defaults changed while this form was open. Reopen it before saving.".to_string());
                }
                let patch = merge_form(
                    &current,
                    &kind,
                    original.as_ref(),
                    &values[0],
                    if delete { None } else { Some(replacement) },
                )?;
                let patch: ServerSettingsPatch =
                    serde_json::from_value(patch).map_err(|e| e.to_string())?;
                client
                    .call::<ServerUpdateSettings>(&ServerUpdateSettingsPayload { patch })
                    .await
                    .map_err(|e| e.user_message())
            }
            .await;
            let config = if result.is_ok() {
                client.call::<ServerGetConfig>(&json!({})).await.ok()
            } else {
                None
            };
            let _ = this.update_in(cx, |panel, window, cx| {
                match result {
                    Ok(settings) => {
                        panel.server = ServerState::Loaded(Box::new(settings));
                        if let Some(config) = config {
                            panel.config = Some(Box::new(config));
                        }
                        panel.form = None;
                        window.close_dialog(cx);
                    }
                    Err(error) => {
                        if let Some(form) = panel.form.as_mut() {
                            form.busy = false;
                            form.error = Some(error.into());
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn build_replacement(
    kind: &FormKind,
    values: &[String],
    original: Option<&Value>,
) -> Result<Value, String> {
    if values[0].trim().is_empty() {
        return Err("An ID is required.".into());
    }
    match kind {
        FormKind::Provider(_) => {
            if !["codex", "claudeAgent", "cursor", "opencode", "grok"].contains(&values[1].trim()) {
                return Err(
                    "Choose a driver: codex, claudeAgent, cursor, opencode or grok.".into(),
                );
            }
            let mut value = original.cloned().unwrap_or(json!({"enabled":true}));
            value["driver"] = json!(values[1].trim());
            value["displayName"] = json!(values[2].trim());
            if values[2].trim().is_empty() {
                value.as_object_mut().unwrap().remove("displayName");
            }
            if values[3].trim().is_empty() {
                value.as_object_mut().unwrap().remove("accentColor");
            } else {
                value["accentColor"] = json!(values[3].trim());
            }
            if !values[4].trim().is_empty() {
                if !value["config"].is_object() {
                    value["config"] = json!({});
                }
                value["config"]["binaryPath"] = json!(values[4].trim());
            } else if let Some(config) = value["config"].as_object_mut() {
                config.remove("binaryPath");
            }
            if let Some(models) = values.get(7) {
                if !value["config"].is_object() {
                    value["config"] = json!({});
                }
                value["config"]["customModels"] =
                    json!(models.split_whitespace().collect::<Vec<_>>());
            }
            if !values[6].is_empty() {
                if values[5].trim().is_empty() {
                    return Err("Enter the environment variable name for this API key.".into());
                }
                let mut env = value["environment"].as_array().cloned().unwrap_or_default();
                env.retain(|v| v["name"].as_str() != Some(values[5].trim()));
                env.push(json!({"name":values[5].trim(),"value":values[6],"sensitive":true}));
                value["environment"] = json!(env);
            }
            Ok(value)
        }
        FormKind::LanguageServer(_) => {
            if values[2].trim().is_empty() || values[3].trim().is_empty() {
                return Err("Language ID and command are required.".into());
            }
            let extensions = values[4]
                .split([',', ' '])
                .filter(|s| !s.is_empty())
                .map(|s| format!(".{}", s.trim_start_matches('.').to_lowercase()))
                .collect::<Vec<_>>();
            if extensions.is_empty() {
                return Err("Enter at least one file extension.".into());
            }
            let args: Vec<String> = serde_json::from_str(&values[5])
                .map_err(|_| "Arguments must be a JSON array of strings, e.g. [\"--stdio\"]")?;
            Ok(
                json!({"serverId":values[0].trim(),"displayName":values[1].trim(),"languageId":values[2].trim(),"command":values[3].trim(),"extensions":extensions,"args":args}),
            )
        }
    }
}

fn merge_form(
    current: &Value,
    kind: &FormKind,
    original: Option<&Value>,
    new_id: &str,
    replacement: Option<Value>,
) -> Result<Value, String> {
    let new_id = new_id.trim();
    match kind {
        FormKind::Provider(old_id) => {
            let mut map = current["providerInstances"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            if map.get(old_id) != original {
                return Err(
                    "This provider changed elsewhere. Close and reopen the editor before saving."
                        .into(),
                );
            }
            if old_id != new_id && map.contains_key(new_id) {
                return Err("That provider ID already exists.".into());
            }
            map.remove(old_id);
            if let Some(value) = replacement {
                map.insert(new_id.into(), value);
            }
            Ok(json!({"providerInstances":map}))
        }
        FormKind::LanguageServer(old_id) => {
            let mut list = current["languageServers"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if list.iter().find(|v| v["serverId"].as_str() == Some(old_id)) != original {
                return Err("This language server changed elsewhere. Reopen the editor.".into());
            }
            if old_id != new_id && list.iter().any(|v| v["serverId"].as_str() == Some(new_id)) {
                return Err("That server ID already exists.".into());
            }
            list.retain(|v| v["serverId"].as_str() != Some(old_id));
            if let Some(value) = replacement {
                list.push(value);
            }
            Ok(json!({"languageServers":list}))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn promoting_a_legacy_provider_preserves_unexposed_configuration() {
        let legacy = json!({"driver":"codex","enabled":false,"config":{"homePath":"/tmp/personal","shadowHomePath":"/tmp/shadow","launchArgs":"--flag","futureOption":42}});
        let values =
            ["codex", "codex", "Personal", "", "codex", "", "", "model-a"].map(String::from);
        let replacement =
            build_replacement(&FormKind::Provider("codex".into()), &values, Some(&legacy)).unwrap();
        let merged = merge_form(
            &json!({"providerInstances":{}}),
            &FormKind::Provider("codex".into()),
            None,
            "codex",
            Some(replacement),
        )
        .unwrap();
        let instance = &merged["providerInstances"]["codex"];
        assert_eq!(instance["enabled"], false);
        for key in ["homePath", "shadowHomePath", "launchArgs", "futureOption"] {
            assert_eq!(instance["config"][key], legacy["config"][key]);
        }
        assert_eq!(instance["config"]["customModels"], json!(["model-a"]));
    }
    #[test]
    fn provider_blank_secret_preserves_existing_environment_and_unknown_config() {
        let original = json!({"driver":"codex","enabled":false,"environment":[{"name":"API_KEY","value":"redacted","sensitive":true}],"config":{"futureOption":42}});
        let values = [
            "personal",
            "codex",
            "Personal",
            "",
            "",
            "API_KEY",
            "",
            "model-a model-b",
        ]
        .map(String::from);
        let result = build_replacement(
            &FormKind::Provider("personal".into()),
            &values,
            Some(&original),
        )
        .unwrap();
        assert_eq!(result["environment"], original["environment"]);
        assert_eq!(result["enabled"], false);
        assert_eq!(result["config"]["futureOption"], 42);
        assert_eq!(
            result["config"]["customModels"],
            json!(["model-a", "model-b"])
        );
    }
    #[test]
    fn language_server_arguments_and_exact_merge_preserve_other_servers() {
        let values = [
            "local",
            "Local",
            "rust",
            "rust-analyzer",
            "rs",
            r#"["--log-file", "/tmp/path with spaces"]"#,
        ]
        .map(String::from);
        let replacement =
            build_replacement(&FormKind::LanguageServer("local".into()), &values, None).unwrap();
        assert_eq!(replacement["args"][1], "/tmp/path with spaces");
        assert_eq!(replacement["extensions"], json!([".rs"]));
        let current = json!({"languageServers":[{"serverId":"other","command":"keep"}]});
        let merged = merge_form(
            &current,
            &FormKind::LanguageServer("".into()),
            None,
            "local",
            Some(replacement),
        )
        .unwrap();
        assert_eq!(merged["languageServers"][0], current["languageServers"][0]);
        assert_eq!(merged["languageServers"].as_array().unwrap().len(), 2);
        assert!(
            merge_form(
                &current,
                &FormKind::LanguageServer("".into()),
                None,
                "other",
                Some(json!({}))
            )
            .is_err()
        );
    }
    #[test]
    fn provider_edit_preserves_other_instances_and_rejects_stale_edits() {
        let original = json!({"driver":"codex"});
        let settings = json!({"providerInstances":{"personal":original,"work":{"driver":"codex"}}});
        let result = merge_form(
            &settings,
            &FormKind::Provider("personal".into()),
            Some(&original),
            "personal",
            Some(json!({"driver":"codex","displayName":"Personal"})),
        )
        .unwrap();
        assert_eq!(
            result["providerInstances"]["work"],
            settings["providerInstances"]["work"]
        );
        assert!(
            merge_form(
                &settings,
                &FormKind::Provider("personal".into()),
                Some(&json!({})),
                "personal",
                None
            )
            .is_err()
        );
    }
}
