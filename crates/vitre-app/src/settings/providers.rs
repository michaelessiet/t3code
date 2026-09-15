//! Compact provider rows and field-level edits, shared with the sidecar's
//! legacy/default instances. Never replace an instance with a partial form.
use super::*;
use gpui::{AppContext as _, Entity, RenderOnce, rems};
use gpui_component::{
    Disableable as _, WindowExt as _,
    input::{Input, InputEvent, InputState},
    switch::Switch,
};
use serde_json::{Value, json};
use std::collections::{HashSet, VecDeque};
use vitre_contracts::{
    ServerProvider, ServerProviderAuthStatus, ServerProviderUpdateInput,
    ServerRefreshProvidersPayload, methods::ServerRefreshProviders,
};

#[derive(Default)]
pub(super) struct ProviderUi {
    expanded: HashSet<String>,
    revealed: HashSet<String>,
    edits: VecDeque<ProviderEdit>,
    writing: bool,
    refreshing: bool,
}

#[derive(Clone)]
struct ProviderEdit {
    id: String,
    driver: String,
    path: Vec<String>,
    value: Value,
}

fn effective_instance(settings: &Value, id: &str, driver: &str) -> Value {
    if let Some(instance) = settings["providerInstances"].get(id) {
        return instance.clone();
    }
    let mut config = settings["providers"][driver].clone();
    if !config.is_object() {
        config = json!({});
    }
    let enabled = config
        .as_object_mut()
        .unwrap()
        .remove("enabled")
        .unwrap_or(json!(driver != "cursor"));
    json!({"driver":driver,"enabled":enabled,"config":config})
}

fn edit_patch(settings: &Value, edit: &ProviderEdit) -> Value {
    let mut instances = settings["providerInstances"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut instance = effective_instance(settings, &edit.id, &edit.driver);
    if edit.path.is_empty() {
        instances.remove(&edit.id);
    } else {
        let mut at = &mut instance;
        for key in &edit.path[..edit.path.len() - 1] {
            if !at[key].is_object() {
                at[key] = json!({});
            }
            at = &mut at[key];
        }
        let key = edit.path.last().unwrap();
        if edit.value.is_null() {
            at.as_object_mut().unwrap().remove(key);
        } else {
            at[key] = edit.value.clone();
        }
        instances.insert(edit.id.clone(), instance);
    }
    let mut patch = json!({"providerInstances":instances});
    if edit.path.is_empty() && edit.id == edit.driver && !config_fields(&edit.driver).is_empty() {
        let mut defaults = json!({"enabled":edit.driver != "cursor","customModels":[]});
        for (key, _, placeholder) in config_fields(&edit.driver) {
            defaults[key] = json!(if key == "binaryPath" { placeholder } else { "" });
        }
        patch["providers"] = json!({edit.driver.clone():defaults});
    }
    if (edit.path.is_empty() || (edit.path == ["enabled"] && edit.value == false))
        && settings["textGenerationModelSelection"]["provider"].as_str() == Some(&edit.id)
    {
        patch["textGenerationModelSelection"] = Value::Null;
    }
    patch
}

fn provider_name(provider: &ServerProvider) -> String {
    provider
        .display_name
        .as_ref()
        .and_then(Option::as_ref)
        .map(|n| n.0.clone())
        .unwrap_or_else(|| {
            match enum_wire_name(&provider.driver).as_str() {
                "codex" => "Codex",
                "claudeAgent" => "Claude",
                "cursor" => "Cursor",
                "grok" => "Grok",
                "opencode" => "OpenCode",
                _ => &provider.instance_id.0,
            }
            .into()
        })
}

fn provider_summary(provider: &ServerProvider, reveal: bool) -> String {
    let name = provider_name(provider);
    if !provider.enabled {
        return format!("Disabled – {name} is disabled in T3 Code settings.");
    }
    let message = provider
        .message
        .as_ref()
        .and_then(Option::as_ref)
        .map(|s| s.0.as_str());
    if !provider.installed {
        return message
            .unwrap_or("Not found – CLI is not available on PATH.")
            .into();
    }
    match provider.auth.status {
        ServerProviderAuthStatus::Authenticated => {
            let mut summary = "Authenticated".to_string();
            if let Some(email) = provider.auth.email.as_ref().and_then(Option::as_ref) {
                summary.push_str(" as ");
                summary.push_str(if reveal {
                    &email.0
                } else {
                    "••••••••"
                });
            }
            if let Some(label) = provider.auth.label.as_ref().and_then(Option::as_ref) {
                summary.push_str(" · ");
                summary.push_str(&label.0);
            }
            summary
        }
        ServerProviderAuthStatus::Unauthenticated => message
            .unwrap_or("Not authenticated – sign in with this provider's CLI.")
            .into(),
        _ => message.map(str::to_owned).unwrap_or_else(|| {
            match enum_wire_name(&provider.status).as_str() {
                "error" => "Unavailable".into(),
                "warning" => "Needs attention".into(),
                _ => "Available – authentication status unknown".into(),
            }
        }),
    }
}

/// Contract annotation order, labels and placeholders (settings.ts).
fn config_fields(driver: &str) -> Vec<(&'static str, &'static str, &'static str)> {
    if !["codex", "claudeAgent", "cursor", "grok", "opencode"].contains(&driver) {
        return Vec::new();
    }
    let mut fields = vec![(
        "binaryPath",
        "Binary path",
        match driver {
            "claudeAgent" => "claude",
            "cursor" => "cursor-agent",
            "grok" => "grok",
            "opencode" => "opencode",
            _ => "codex",
        },
    )];
    match driver {
        "codex" => fields.extend([
            ("homePath", "CODEX_HOME path", "~/.codex"),
            ("shadowHomePath", "Shadow home path", "~/.codex-t3/personal"),
            ("launchArgs", "Launch arguments", ""),
        ]),
        "claudeAgent" => fields.extend([
            ("homePath", "CLAUDE_CONFIG_DIR path", "~/.claude"),
            ("launchArgs", "Launch arguments", "e.g. --chrome"),
        ]),
        "cursor" => fields.push(("apiEndpoint", "API endpoint", "https://…")),
        "opencode" => fields.extend([
            ("serverUrl", "Server URL", "http://127.0.0.1:4096"),
            ("serverPassword", "Server password", "Optional"),
        ]),
        _ => {}
    }
    fields
}

impl SettingsPanel {
    #[cfg(debug_assertions)]
    pub(crate) fn verification_provider_expanded(&self, id: &str) -> bool {
        self.providers.expanded.contains(id)
    }
    fn queue_provider_edit(&mut self, edit: ProviderEdit, cx: &mut Context<Self>) {
        self.providers.edits.push_back(edit);
        self.write_next_provider_edit(cx);
    }

    fn write_next_provider_edit(&mut self, cx: &mut Context<Self>) {
        if self.providers.writing {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(edit) = self.providers.edits.pop_front() else {
            return;
        };
        self.providers.writing = true;
        self.write_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = async {
                let settings = client
                    .call::<ServerGetSettings>(&json!({}))
                    .await
                    .map_err(|e| e.user_message())?;
                let patch = serde_json::from_value(edit_patch(
                    &serde_json::to_value(settings).unwrap(),
                    &edit,
                ))
                .map_err(|e| e.to_string())?;
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
            let _ = this.update(cx, |p, cx| {
                p.providers.writing = false;
                let succeeded = result.is_ok();
                match result {
                    Ok(settings) => p.server = ServerState::Loaded(Box::new(settings)),
                    Err(error) => p.write_error = Some(error.into()),
                }
                if let Some(config) = config {
                    p.config = Some(Box::new(config));
                }
                if succeeded {
                    p.write_next_provider_edit(cx);
                } else {
                    p.providers.edits.clear();
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh_providers(&mut self, cx: &mut Context<Self>) {
        if self.providers.refreshing {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.providers.refreshing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client
                .call::<ServerRefreshProviders>(&ServerRefreshProvidersPayload {
                    instance_id: None,
                })
                .await;
            let _ = this.update(cx, |p, cx| {
                p.providers.refreshing = false;
                match result {
                    Ok(result) => {
                        if let Some(config) = p.config.as_mut() {
                            config.providers = result.providers;
                        }
                    }
                    Err(error) => p.write_error = Some(error.user_message().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn providers_page(&self, cx: &Context<Self>) -> SettingPage {
        let Some(config) = &self.config else {
            return loading_page("Providers", "Loading provider status…");
        };
        let owner = cx.entity().downgrade();
        let refreshing = self.providers.refreshing;
        let checked = config
            .providers
            .iter()
            .map(|p| p.checked_at.as_str())
            .max()
            .unwrap_or_default()
            .to_string();
        let error = self.write_error.clone();
        let status = self.operations_status();
        let mut group = SettingGroup::new().item(SettingItem::render(move |_, _, cx| {
            let add = owner.clone();
            let refresh = owner.clone();
            v_flex()
                .w_full()
                .gap_2()
                .child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .child(div().flex_1().text_lg().font_semibold().child("Providers"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(if refreshing {
                                    "Checking…".to_string()
                                } else {
                                    checked_label(&checked)
                                }),
                        )
                        .child(
                            Button::new("add-provider")
                                .ghost()
                                .xsmall()
                                .icon(IconName::Plus)
                                .tooltip("Add provider")
                                .on_click(move |_, w, cx| {
                                    let _ = add.update(cx, |p, cx| {
                                        p.open_form(forms::FormKind::Provider(String::new()), w, cx)
                                    });
                                }),
                        )
                        .child(
                            Button::new("refresh-providers")
                                .ghost()
                                .xsmall()
                                .icon(crate::assets::VitreIcon::RefreshCw)
                                .tooltip("Refresh providers")
                                .disabled(refreshing)
                                .on_click(move |_, _, cx| {
                                    let _ = refresh.update(cx, |p, cx| p.refresh_providers(cx));
                                }),
                        ),
                )
                .children(
                    error
                        .clone()
                        .map(|e| div().text_sm().text_color(cx.theme().danger).child(e)),
                )
                .children(status.clone().map(|s| {
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(s)
                }))
                .into_any_element()
        }));
        let settings = serde_json::to_value(self.server_settings()).unwrap_or(Value::Null);
        let mut providers = config.providers.clone();
        providers.sort_by_key(|p| {
            let driver = enum_wire_name(&p.driver);
            (
                ["codex", "claudeAgent", "cursor", "grok", "opencode"]
                    .iter()
                    .position(|d| *d == driver)
                    .unwrap_or(5),
                p.instance_id.0 != driver,
                p.instance_id.0.clone(),
            )
        });
        for provider in providers {
            let owner = cx.entity().downgrade();
            let expanded = self.providers.expanded.contains(&provider.instance_id.0);
            let reveal = self.providers.revealed.contains(&provider.instance_id.0);
            let busy = self.providers.writing;
            let driver = enum_wire_name(&provider.driver);
            let value = effective_instance(&settings, &provider.instance_id.0, &driver);
            let custom = provider.instance_id.0 != driver;
            let dirty = settings["providerInstances"]
                .get(&provider.instance_id.0)
                .is_some();
            let name = provider_name(&provider);
            let keywords = [
                driver.clone(),
                format!("{driver} models environment binary authentication enabled"),
            ];
            group = group.item(SettingItem::render(move |_, _, cx| {
                let id = provider.instance_id.0.clone();
                let mut title = h_flex().gap_2().child(div().relative().child(crate::icons::provider_icon(&driver, cx).size_4())
                    .child(div().absolute().left(-rems(0.2)).top(-rems(0.2)).size(rems(0.4)).rounded_full()
                        .bg(if !provider.enabled || !provider.installed { cx.theme().warning } else if enum_wire_name(&provider.status) == "error" { cx.theme().danger } else { cx.theme().success })))
                    .child(div().text_sm().font_semibold().child(name.clone()));
                if custom { title = title.child(div().text_xs().text_color(cx.theme().muted_foreground).child(id.clone())); }
                if matches!(driver.as_str(), "cursor" | "grok") { title = title.child(div().text_xs().rounded_sm().px_1().bg(cx.theme().warning.opacity(0.15)).text_color(cx.theme().warning).child("Early Access")); }
                if let Some(version) = &provider.version { title = title.child(div().text_xs().font_family("monospace").text_color(cx.theme().muted_foreground).child(format!("v{}",version.0))); }
                if let Some(advisory) = provider.version_advisory.as_ref().filter(|a| enum_wire_name(&a.status) == "behind_latest") {
                    let update = owner.clone(); let input = ServerProviderUpdateInput { instance_id: Some(provider.instance_id.clone()), provider: provider.driver.clone() };
                    title = title.child(Button::new("update-provider").ghost().xsmall().icon(IconName::ArrowUp).tooltip(advisory.message.as_ref().map(|m| m.0.clone()).unwrap_or("Update CLI".into()))
                        .disabled(!advisory.can_update.flatten().unwrap_or(false)).on_click(move |_, _, cx| { let _ = update.update(cx, |p, cx| p.update_provider(input.clone(), cx)); }));
                }
                if dirty {
                    let reset = owner.clone(); let edit = ProviderEdit { id: id.clone(), driver: driver.clone(), path: vec![], value: Value::Null };
                    title = title.child(Button::new("reset-provider").ghost().xsmall().icon(if custom { IconName::Delete } else { IconName::Undo2 }).tooltip(if custom { "Remove provider instance" } else { "Restore defaults" })
                        .disabled(busy).on_click(move |_, window, cx| {
                            if !custom { let _ = reset.update(cx, |p, cx| p.queue_provider_edit(edit.clone(), cx)); return; }
                            let owner=reset.clone();let edit=edit.clone();
                            window.open_dialog(cx,move|dialog,_,_| {
                                let owner=owner.clone();let edit=edit.clone();
                                dialog.title(format!("Remove provider {}?",edit.id))
                                    .child("This removes this provider instance and its stored credentials. Existing conversations are kept.")
                                    .button_props(gpui_component::dialog::DialogButtonProps::default().ok_text("Remove").cancel_text("Cancel").show_cancel(true).ok_variant(gpui_component::button::ButtonVariant::Danger))
                                    .on_ok(move|_,_,cx|{let _=owner.update(cx,|p,cx|p.queue_provider_edit(edit.clone(),cx));true})
                            });
                        }));
                }
                let toggle = owner.clone(); let toggle_id = id.clone();
                let enable = owner.clone(); let edit = ProviderEdit { id: id.clone(), driver: driver.clone(), path: vec!["enabled".into()], value: json!(!provider.enabled) };
                let mut summary = h_flex().gap_2().child(div().text_sm().text_color(cx.theme().muted_foreground.opacity(0.85)).child(provider_summary(&provider, reveal)));
                if provider.auth.email.as_ref().and_then(Option::as_ref).is_some() {
                    let reveal_owner = owner.clone(); let reveal_id = id.clone();
                    summary = summary.child(Button::new("reveal-auth").ghost().xsmall().icon(IconName::Eye).tooltip(if reveal { "Hide account" } else { "Reveal account" })
                        .on_click(move |_, _, cx| { let _ = reveal_owner.update(cx, |p, cx| { if !p.providers.revealed.insert(reveal_id.clone()) { p.providers.revealed.remove(&reveal_id); } cx.notify(); }); }));
                }
                let header = h_flex().w_full().gap_4().py_3()
                    .child(v_flex().flex_1().min_w_0().gap_1().child(title).child(summary))
                    .child(Button::new("expand-provider").ghost().xsmall().icon(if expanded { IconName::ChevronUp } else { IconName::ChevronDown }).tooltip(if expanded { "Collapse provider" } else { "Configure provider" })
                        .on_click(move |_, _, cx| { let _ = toggle.update(cx, |p, cx| { if !p.providers.expanded.insert(toggle_id.clone()) { p.providers.expanded.remove(&toggle_id); } cx.notify(); }); }))
                    .child(Switch::new("enable-provider").small().checked(provider.enabled).disabled(busy).on_click(move |_, _, cx| { let _ = enable.update(cx, |p, cx| p.queue_provider_edit(edit.clone(), cx)); }));
                let mut card = v_flex().id(SharedString::from(format!("provider-{id}"))).w_full().gap_2().child(header);
                {
                    let mut fields = v_flex().w_full().gap_4().pl_6().pb_4();
                    for (key, label, placeholder, config) in std::iter::once(("displayName","Display name","Provider name",false))
                        .chain(std::iter::once(("accentColor","Accent color","#RRGGBB",false)))
                        .chain(config_fields(&driver).into_iter().map(|(k,l,p)|(k,l,p,true))) {
                        let path = if config { vec!["config".to_string(),key.into()] } else { vec![key.into()] };
                        let current = if config { &value["config"][key] } else { &value[key] };
                        fields = fields.child(ProviderTextField { owner: owner.clone(), edit: ProviderEdit { id:id.clone(),driver:driver.clone(),path,value:Value::Null }, value:current.as_str().unwrap_or_default().into(), label, placeholder, password:key=="serverPassword", disabled:busy || !expanded });
                    }
                    let configure = owner.clone(); let configure_id = id.clone();
                    fields = fields.child(h_flex().child(Button::new("provider-environment").small().outline().disabled(!expanded).label("Environment variables and custom models")
                        .on_click(move |_, w, cx| { let _ = configure.update(cx, |p, cx| p.open_form(forms::FormKind::Provider(configure_id.clone()), w, cx)); })));
                    if driver == "claudeAgent" && !provider.installed {
                        let install = owner.clone();
                        fields = fields.child(h_flex().child(Button::new("install-claude").small().disabled(!expanded).label("Download Claude CLI")
                            .on_click(move |_, _, cx| { let _ = install.update(cx, |p, cx| p.install_claude(cx)); })));
                    }
                    card = card.child(gpui_component::animated_height::AnimatedHeight::new("provider-details",fields).expanded(expanded));
                }
                card.into_any_element()
            }).keywords(keywords));
        }
        SettingPage::new("Providers").resettable(false).group(group)
    }
}

fn checked_label(at: &str) -> String {
    let Ok(at) = chrono::DateTime::parse_from_rfc3339(at) else {
        return "Not checked yet".into();
    };
    let minutes = (chrono::Utc::now() - at.with_timezone(&chrono::Utc)).num_minutes();
    if minutes <= 0 {
        "Checked just now".into()
    } else {
        format!("Checked {minutes}m ago")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_reset_is_scoped_and_clears_the_removed_model_selection() {
        let settings = json!({"providerInstances":{"codex":{"driver":"codex"},"personal":{"driver":"codex"}},"textGenerationModelSelection":{"provider":"codex","model":"test"}});
        let patch = edit_patch(
            &settings,
            &ProviderEdit {
                id: "codex".into(),
                driver: "codex".into(),
                path: vec![],
                value: Value::Null,
            },
        );
        assert_eq!(
            patch["providerInstances"]["personal"],
            settings["providerInstances"]["personal"]
        );
        assert!(patch["providerInstances"].get("codex").is_none());
        assert_eq!(patch["providers"]["codex"]["binaryPath"], "codex");
        assert_eq!(patch["providers"]["codex"]["homePath"], "");
        assert_eq!(patch["textGenerationModelSelection"], Value::Null);
        let unknown = edit_patch(
            &settings,
            &ProviderEdit {
                id: "future".into(),
                driver: "future".into(),
                path: vec![],
                value: Value::Null,
            },
        );
        assert!(unknown.get("providers").is_none());
        assert!(config_fields("future").is_empty());
    }
    #[test]
    fn changing_enabled_preserves_legacy_configuration_and_other_instances() {
        let settings = json!({"providers":{"codex":{"enabled":true,"homePath":"/custom","launchArgs":"--sandbox"}},"providerInstances":{"custom":{"driver":"codex","enabled":false,"environment":[{"name":"TOKEN","sensitive":true,"valueRedacted":true}]}},"textGenerationModelSelection":{"provider":"codex","model":"gpt-5"}});
        let patch = edit_patch(
            &settings,
            &ProviderEdit {
                id: "codex".into(),
                driver: "codex".into(),
                path: vec!["enabled".into()],
                value: json!(false),
            },
        );
        assert_eq!(
            patch["providerInstances"]["codex"]["config"]["homePath"],
            "/custom"
        );
        assert_eq!(
            patch["providerInstances"]["custom"],
            settings["providerInstances"]["custom"]
        );
        assert_eq!(patch["textGenerationModelSelection"], Value::Null);
        assert!(
            !patch["providerInstances"]["codex"]["config"]
                .as_object()
                .unwrap()
                .contains_key("enabled")
        );
    }
    #[test]
    fn field_edits_preserve_credentials_and_unknown_configuration() {
        let settings = json!({"providerInstances":{"work":{"driver":"opencode","enabled":true,"environment":[{"name":"API_KEY","sensitive":true,"valueRedacted":true}],"config":{"serverPassword":"secret","futureOption":42,"binaryPath":"old"}}}});
        let patch = edit_patch(
            &settings,
            &ProviderEdit {
                id: "work".into(),
                driver: "opencode".into(),
                path: vec!["config".into(), "binaryPath".into()],
                value: Value::Null,
            },
        );
        assert_eq!(
            patch["providerInstances"]["work"]["environment"],
            settings["providerInstances"]["work"]["environment"]
        );
        assert_eq!(
            patch["providerInstances"]["work"]["config"]["futureOption"],
            42
        );
        assert_eq!(
            patch["providerInstances"]["work"]["config"]["serverPassword"],
            "secret"
        );
        assert!(
            !patch["providerInstances"]["work"]["config"]
                .as_object()
                .unwrap()
                .contains_key("binaryPath")
        );
    }
    #[test]
    fn provider_config_fields_match_contract_order() {
        assert_eq!(
            config_fields("codex")
                .iter()
                .map(|f| f.0)
                .collect::<Vec<_>>(),
            ["binaryPath", "homePath", "shadowHomePath", "launchArgs"]
        );
        assert_eq!(
            config_fields("opencode")
                .iter()
                .map(|f| f.0)
                .collect::<Vec<_>>(),
            ["binaryPath", "serverUrl", "serverPassword"]
        );
    }
}

#[derive(IntoElement)]
struct ProviderTextField {
    owner: WeakEntity<SettingsPanel>,
    edit: ProviderEdit,
    value: String,
    label: &'static str,
    placeholder: &'static str,
    password: bool,
    disabled: bool,
}
struct FieldState {
    input: Entity<InputState>,
    saved: String,
    _subscription: gpui::Subscription,
}
impl RenderOnce for ProviderTextField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let value = self.value.clone();
        let state = window.use_keyed_state(
            SharedString::from(format!(
                "provider-field-{}-{}",
                self.edit.id,
                self.edit.path.join("/")
            )),
            cx,
            |window, cx| {
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(value.clone())
                        .placeholder(self.placeholder)
                        .masked(self.password)
                });
                let subscription =
                    cx.subscribe(&input, move |state: &mut FieldState, input, event, cx| {
                        if !matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                            return;
                        }
                        let value = input.read(cx).value().trim().to_string();
                        if value == state.saved {
                            return;
                        }
                        let mut edit = self.edit.clone();
                        edit.value = if value.is_empty() {
                            Value::Null
                        } else {
                            json!(value)
                        };
                        let _ = self
                            .owner
                            .update(cx, |p, cx| p.queue_provider_edit(edit, cx));
                    });
                FieldState {
                    input,
                    saved: value,
                    _subscription: subscription,
                }
            },
        );
        state.update(cx, |s, cx| {
            if !s.input.read(cx).focus_handle(cx).is_focused(window) && s.saved != self.value {
                s.input
                    .update(cx, |i, cx| i.set_value(self.value.clone(), window, cx));
            }
            s.saved = self.value.clone();
        });
        v_flex()
            .gap_1p5()
            .child(div().text_xs().font_medium().child(self.label))
            .child(
                Input::new(&state.read(cx).input)
                    .small()
                    .disabled(self.disabled),
            )
    }
}
