//! The settings surface.
//!
//! Electron's settings is a *route*, not a dialog: the sidebar swaps its body
//! for a section nav, the content area renders the page, and Escape (or the
//! nav's Back row) returns to where you were
//! (`apps/web/src/components/settings/SettingsSidebarNav.tsx`). Vitre does the
//! same thing at the workspace level — [`crate::chat::ChatApp`] renders this
//! view *instead of* the sidebar/chat/dock split — so no chat state is torn
//! down while you are in here.
//!
//! The rows themselves come from gpui-component's `setting` framework
//! (`Settings > SettingPage > SettingGroup > SettingItem > SettingField`),
//! which supplies the nav, the search box, the row chrome and the per-page
//! reset button. Its fields take plain `Fn(&App) -> T` / `Fn(T, &mut App)`
//! closures, which is why the client half of the settings lives in a global
//! ([`ClientSettings`]) and the server half is reached through a weak handle
//! to this view.
//!
//! ## Which half owns what
//!
//! `packages/contracts/src/settings.ts` splits settings in two, and this
//! screen keeps the split visible:
//!
//! - **Client** — local to this install, written straight to
//!   `<home>/client-settings.json`, applied immediately.
//! - **Server** — owned by the sidecar's `settings.json` and shared with every
//!   other client attached to it (the Electron app included). Read with
//!   `server.getSettings`, written with `server.updateSettings`, and — exactly
//!   as Electron does it — *not* applied optimistically: the switch moves when
//!   the server's new settings come back.

use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, Focusable, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, WeakEntity, Window, actions, div, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::setting::{
    NumberFieldOptions, RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage,
    Settings,
};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable as _, StyledExt as _, Theme, ThemeMode, h_flex, v_flex,
};
use vitre_client::EnvironmentClient;
use vitre_contracts::generated::{
    AutoCompactThresholdTokens2, ServerSettings, ServerSettingsPatch, ServerUpdateSettingsPayload,
    methods::{ServerGetSettings, ServerUpdateSettings},
};

use crate::client_settings::{
    ClientSettings, MAX_AUTO_SAVE_DELAY_MS, MIN_AUTO_SAVE_DELAY_MS, ThemeSetting,
};

actions!(
    vitre,
    [
        /// Electron's `Cmd+,` application-menu item (`open-settings`), which
        /// navigates to `/settings`. There is no rebindable keybinding command
        /// for it on that side either.
        SettingsOpen,
        /// Escape while the settings surface holds focus — Electron's
        /// `navigateBackWithinApp` on the settings route.
        SettingsClose,
    ]
);

/// Contract defaults (`settings.ts`), used whenever the server has never
/// written the key — its settings.json is sparse, so an absent key means
/// "default", not "off".
const DEFAULT_ASSISTANT_STREAMING: bool = true;
const DEFAULT_PROVIDER_UPDATE_CHECKS: bool = true;
const DEFAULT_AUTO_COMPACT: bool = true;
const DEFAULT_AUTO_COMPACT_THRESHOLD: i64 = 200_000;
/// `settings.ts` hard-bounds the threshold because the Claude SDK silently
/// drops values outside this range.
const MIN_AUTO_COMPACT_THRESHOLD: i64 = 100_000;
const MAX_AUTO_COMPACT_THRESHOLD: i64 = 1_000_000;

/// The server half, which arrives over RPC and can fail.
enum ServerState {
    Loading,
    Loaded(Box<ServerSettings>),
    Failed(SharedString),
}

pub struct SettingsPanel {
    client: Option<Arc<EnvironmentClient>>,
    server: ServerState,
    /// The last write that failed, shown above the rows it belongs to. A read
    /// failure replaces the rows instead (there is nothing to show).
    write_error: Option<SharedString>,
    /// The read loop, dropped with the surface.
    reader: Option<gpui::Task<()>>,
    focus_handle: gpui::FocusHandle,
}

impl SettingsPanel {
    pub fn new(client: Option<Arc<EnvironmentClient>>, cx: &mut Context<Self>) -> Self {
        let connected = client.is_some();
        let mut panel = Self {
            client,
            server: if connected {
                ServerState::Loading
            } else {
                ServerState::Failed("Not connected to an environment yet.".into())
            },
            write_error: None,
            reader: None,
            focus_handle: cx.focus_handle(),
        };
        panel.load(cx);
        panel
    }

    /// The environment arrived while this surface was already open — the
    /// shell hands it over rather than leaving the server rows dead until the
    /// user closes and reopens settings.
    pub fn attach_client(&mut self, client: Arc<EnvironmentClient>, cx: &mut Context<Self>) {
        if self.client.is_some() {
            return;
        }
        self.client = Some(client);
        self.server = ServerState::Loading;
        self.load(cx);
        cx.notify();
    }

    /// Read the sidecar's settings, and keep reading them.
    ///
    /// `call` fails outright when no session is up, and this surface can be
    /// opened while the sidecar is still booting — so the loop waits for a
    /// session first, then reads again whenever one is replaced: a reconnect
    /// may well have been someone else's client writing the same file.
    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.reader = Some(cx.spawn(async move |this, cx| {
            let mut sessions = client.sessions();
            loop {
                if sessions.borrow_and_update().is_some() {
                    let result = client
                        .call::<ServerGetSettings>(&serde_json::json!({}))
                        .await;
                    let applied = this.update(cx, |panel, cx| {
                        panel.server = match result {
                            Ok(settings) => ServerState::Loaded(Box::new(settings)),
                            Err(error) => ServerState::Failed(error.user_message().into()),
                        };
                        cx.notify();
                    });
                    if applied.is_err() {
                        return;
                    }
                }
                if sessions.changed().await.is_err() {
                    return;
                }
            }
        }));
    }

    fn server_settings(&self) -> Option<&ServerSettings> {
        match &self.server {
            ServerState::Loaded(settings) => Some(settings),
            _ => None,
        }
    }

    /// Send one key to the sidecar.
    ///
    /// The patch is built as JSON rather than by naming 15 `None`s: the
    /// generated `ServerSettingsPatch` has no `Default`, every field is
    /// `#[serde(default)]`, and the server deep-merges what it receives
    /// (`packages/shared/src/serverSettings.ts::applyServerSettingsPatch`), so
    /// a one-key object is exactly the wire shape Electron sends.
    fn patch(&mut self, patch: serde_json::Value, cx: &mut Context<Self>) {
        let patch: ServerSettingsPatch = match serde_json::from_value(patch) {
            Ok(patch) => patch,
            Err(error) => {
                self.write_error = Some(format!("could not build the update: {error}").into());
                cx.notify();
                return;
            }
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        self.write_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client
                .call::<ServerUpdateSettings>(&ServerUpdateSettingsPayload { patch })
                .await;
            let _ = this.update(cx, |panel, cx| {
                match result {
                    // The server answers with the whole settings object, so
                    // this is also how the row learns its new value — there is
                    // no optimistic write, exactly as in Electron.
                    Ok(settings) => panel.server = ServerState::Loaded(Box::new(settings)),
                    Err(error) => panel.write_error = Some(error.user_message().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Read a server boolean, falling back to the contract default.
    fn server_bool(
        weak: &WeakEntity<Self>,
        cx: &App,
        pick: impl Fn(&ServerSettings) -> Option<Option<bool>>,
        default: bool,
    ) -> bool {
        weak.upgrade()
            .and_then(|panel| {
                panel
                    .read(cx)
                    .server_settings()
                    .and_then(|settings| pick(settings).flatten())
            })
            .unwrap_or(default)
    }

    fn appearance_group() -> SettingGroup {
        SettingGroup::new().title("Appearance").item(
            SettingItem::new(
                "Theme",
                SettingField::dropdown(
                    ThemeSetting::ALL
                        .into_iter()
                        .map(|theme| (theme.as_str().into(), theme.label().into()))
                        .collect(),
                    |cx: &App| ClientSettings::theme(cx).as_str().into(),
                    |value: SharedString, cx: &mut App| {
                        let Some(theme) = ThemeSetting::from_str(value.as_ref()) else {
                            return;
                        };
                        ClientSettings::update(cx, |settings| settings.theme = theme);
                        apply_theme(theme, None, cx);
                    },
                )
                .default_value(ThemeSetting::System.as_str()),
            )
            .description("Match the system appearance, or pin Vitre to light or dark.")
            .keywords(["appearance", "dark", "light"]),
        )
    }

    fn editor_group() -> SettingGroup {
        SettingGroup::new()
            .title("Editor")
            .item(
                SettingItem::new(
                    "Vim mode",
                    SettingField::switch(
                        |cx: &App| ClientSettings::vim_mode(cx),
                        |value: bool, cx: &mut App| {
                            ClientSettings::update(cx, |settings| settings.vim_mode = value);
                        },
                    )
                    .default_value(false),
                )
                .description("Modal editing in the file editor, with the same motions as Electron.")
                .keywords(["vim", "modal", "keys"]),
            )
            .item(
                SettingItem::new(
                    "Word wrap",
                    SettingField::switch(
                        |cx: &App| ClientSettings::word_wrap(cx),
                        |value: bool, cx: &mut App| {
                            ClientSettings::update(cx, |settings| settings.word_wrap = value);
                        },
                    )
                    .default_value(true),
                )
                .description("Wrap long lines in the file editor instead of scrolling sideways.")
                .keywords(["wrap", "editor"]),
            )
            .item(
                SettingItem::new(
                    "Auto save",
                    SettingField::switch(
                        |cx: &App| ClientSettings::get(cx).auto_save_enabled,
                        |value: bool, cx: &mut App| {
                            ClientSettings::update(cx, |settings| {
                                settings.auto_save_enabled = value
                            });
                        },
                    )
                    .default_value(true),
                )
                .description("Persist edits automatically. With this off, ⌘S and `:w` still save.")
                .keywords(["save", "autosave"]),
            )
            .item(
                SettingItem::new(
                    "Auto save delay",
                    SettingField::number_input(
                        NumberFieldOptions {
                            min: MIN_AUTO_SAVE_DELAY_MS.into(),
                            max: MAX_AUTO_SAVE_DELAY_MS.into(),
                            step: 100.,
                        },
                        |cx: &App| ClientSettings::get(cx).auto_save_delay_ms.into(),
                        |value: f64, cx: &mut App| {
                            let delay = value
                                .round()
                                .clamp(MIN_AUTO_SAVE_DELAY_MS.into(), MAX_AUTO_SAVE_DELAY_MS.into())
                                as u32;
                            ClientSettings::update(cx, |settings| {
                                settings.auto_save_delay_ms = delay
                            });
                        },
                    )
                    .default_value(500.)
                    .w(px(150.)),
                )
                .description("Milliseconds of quiet before an edited buffer is written.")
                .keywords(["save", "delay", "debounce"]),
            )
            .item(
                SettingItem::new(
                    "File conflict warning",
                    SettingField::switch(
                        |cx: &App| ClientSettings::show_file_conflict_warning(cx),
                        |value: bool, cx: &mut App| {
                            ClientSettings::update(cx, |settings| {
                                settings.show_file_conflict_warning = value
                            });
                        },
                    )
                    .default_value(true),
                )
                .description(
                    "Show the banner when the open file changed on disk under your edits. \
                     Detection keeps running either way.",
                )
                .keywords(["conflict", "disk", "reload"]),
            )
    }

    fn threads_group() -> SettingGroup {
        SettingGroup::new().title("Threads").item(
            SettingItem::new(
                "Delete confirmation",
                SettingField::switch(
                    |cx: &App| ClientSettings::confirm_thread_delete(cx),
                    |value: bool, cx: &mut App| {
                        ClientSettings::update(cx, |settings| {
                            settings.confirm_thread_delete = value
                        });
                    },
                )
                .default_value(true),
            )
            .description("Ask before deleting a thread.")
            .keywords(["thread", "delete", "confirm"]),
        )
    }

    /// The rows the sidecar owns. Shared with every other client attached to
    /// it — changing one here changes it in the Electron app too.
    ///
    /// `ready` is false until the first read lands: the rows would otherwise
    /// show contract defaults as if they were the server's answer, and a click
    /// would write a value the user never saw.
    fn assistant_group(weak: &WeakEntity<Self>, ready: bool) -> SettingGroup {
        let streaming = weak.clone();
        let streaming_set = weak.clone();
        let compact = weak.clone();
        let compact_set = weak.clone();
        let threshold = weak.clone();
        let threshold_set = weak.clone();
        let updates = weak.clone();
        let updates_set = weak.clone();
        let status = weak.clone();

        SettingGroup::new()
            .title("Assistant")
            .description("Stored by the sidecar and shared with every client attached to it.")
            .item(SettingItem::render(move |_: &RenderOptions, _, cx| {
                render_server_status(&status, cx)
            }))
            .item(
                SettingItem::new(
                    "Assistant output",
                    SettingField::switch(
                        move |cx: &App| {
                            Self::server_bool(
                                &streaming,
                                cx,
                                |settings| settings.enable_assistant_streaming,
                                DEFAULT_ASSISTANT_STREAMING,
                            )
                        },
                        move |value: bool, cx: &mut App| {
                            if let Some(panel) = streaming_set.upgrade() {
                                panel.update(cx, |panel, cx| {
                                    panel.patch(
                                        serde_json::json!({ "enableAssistantStreaming": value }),
                                        cx,
                                    );
                                });
                            }
                        },
                    )
                    .default_value(DEFAULT_ASSISTANT_STREAMING),
                )
                .description("Stream tokens as they arrive instead of showing each message whole.")
                .disabled(!ready)
                .keywords(["stream", "assistant"]),
            )
            .item(
                SettingItem::new(
                    "Auto-compact context",
                    SettingField::switch(
                        move |cx: &App| {
                            Self::server_bool(
                                &compact,
                                cx,
                                |settings| settings.auto_compact_enabled,
                                DEFAULT_AUTO_COMPACT,
                            )
                        },
                        move |value: bool, cx: &mut App| {
                            if let Some(panel) = compact_set.upgrade() {
                                panel.update(cx, |panel, cx| {
                                    panel.patch(
                                        serde_json::json!({ "autoCompactEnabled": value }),
                                        cx,
                                    );
                                });
                            }
                        },
                    )
                    .default_value(DEFAULT_AUTO_COMPACT),
                )
                .description("Summarise a long thread automatically instead of failing on context.")
                .disabled(!ready)
                .keywords(["compact", "context"]),
            )
            .item(
                SettingItem::new(
                    "Auto-compact threshold",
                    SettingField::number_input(
                        NumberFieldOptions {
                            min: MIN_AUTO_COMPACT_THRESHOLD as f64,
                            max: MAX_AUTO_COMPACT_THRESHOLD as f64,
                            step: 10_000.,
                        },
                        move |cx: &App| {
                            threshold
                                .upgrade()
                                .and_then(|panel| {
                                    panel.read(cx).server_settings().and_then(|settings| {
                                        settings
                                            .auto_compact_threshold_tokens
                                            .flatten()
                                            .map(|tokens| tokens.0 as f64)
                                    })
                                })
                                .unwrap_or(DEFAULT_AUTO_COMPACT_THRESHOLD as f64)
                        },
                        move |value: f64, cx: &mut App| {
                            let tokens = (value.round() as i64)
                                .clamp(MIN_AUTO_COMPACT_THRESHOLD, MAX_AUTO_COMPACT_THRESHOLD);
                            if let Some(panel) = threshold_set.upgrade() {
                                panel.update(cx, |panel, cx| {
                                    panel.patch(
                                        serde_json::json!({
                                            "autoCompactThresholdTokens":
                                                AutoCompactThresholdTokens2(tokens),
                                        }),
                                        cx,
                                    );
                                });
                            }
                        },
                    )
                    .default_value(DEFAULT_AUTO_COMPACT_THRESHOLD as f64)
                    // Six digits plus the stepper buttons; the default width
                    // clips at 200000.
                    .w(px(150.)),
                )
                .description("Tokens of context to allow before compacting.")
                .disabled(!ready)
                .keywords(["compact", "tokens", "context"]),
            )
            .item(
                SettingItem::new(
                    "Provider update checks",
                    SettingField::switch(
                        move |cx: &App| {
                            Self::server_bool(
                                &updates,
                                cx,
                                |settings| settings.enable_provider_update_checks,
                                DEFAULT_PROVIDER_UPDATE_CHECKS,
                            )
                        },
                        move |value: bool, cx: &mut App| {
                            if let Some(panel) = updates_set.upgrade() {
                                panel.update(cx, |panel, cx| {
                                    panel.patch(
                                        serde_json::json!({ "enableProviderUpdateChecks": value }),
                                        cx,
                                    );
                                });
                            }
                        },
                    )
                    .default_value(DEFAULT_PROVIDER_UPDATE_CHECKS),
                )
                .description("Let the sidecar check whether the provider CLIs have updates.")
                .disabled(!ready)
                .keywords(["provider", "update"]),
            )
    }

    /// The sections Electron has and Vitre does not, named rather than
    /// silently missing — every one is a row in `docs/vitre-parity.md`.
    fn absent_page() -> SettingPage {
        let pending = [
            (
                "Providers",
                "Provider instances, the add-provider wizard, custom models and accent colours.",
            ),
            (
                "Keybindings",
                "The chord recorder and when-expression editor. Vitre's chords are still fixed.",
            ),
            (
                "Language servers",
                "Per-language server status and configuration.",
            ),
            ("Source control", "Repository discovery, clone and publish."),
            (
                "Connections",
                "Network exposure, pairing links and authorized clients — waits on remote environments.",
            ),
            ("Knowledge graph", "Graph indexing and rebuild controls."),
            (
                "Diagnostics",
                "Process metrics, resource history and traces.",
            ),
            ("Beta", "Feature flags."),
            (
                "Archived threads",
                "Browse, restore and delete archived threads.",
            ),
        ];
        SettingPage::new("Not yet in Vitre")
            .icon(IconName::Info)
            .resettable(false)
            .description(
                "Electron has these; Vitre does not yet. They are tracked as rows in \
                 docs/vitre-parity.md.",
            )
            .group(
                SettingGroup::new().items(pending.into_iter().map(|(title, description)| {
                    SettingItem::render(move |_: &RenderOptions, _, cx| {
                        v_flex()
                            .gap_0p5()
                            .child(div().text_sm().child(title))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(description),
                            )
                            .into_any_element()
                    })
                })),
            )
    }

    fn pages(&self, cx: &Context<Self>) -> Vec<SettingPage> {
        let weak = cx.entity().downgrade();
        vec![
            SettingPage::new("General")
                .icon(IconName::Settings)
                .description("Preferences for this copy of Vitre, and for the sidecar it talks to.")
                .group(Self::appearance_group())
                .group(Self::editor_group())
                .group(Self::threads_group())
                .group(Self::assistant_group(
                    &weak,
                    self.server_settings().is_some(),
                )),
            Self::absent_page(),
        ]
    }
}

/// Apply a theme choice to the live window.
///
/// `Theme::change` reads the `light_theme`/`dark_theme` slots, which
/// `main.rs` filled from `themes/vitre.json`, so this switches between the two
/// Vitre palettes rather than the fork's defaults.
pub fn apply_theme(theme: ThemeSetting, window: Option<&mut Window>, cx: &mut App) {
    // `Theme::change` only repaints the window it is handed, so a change made
    // from a setting field (which has no `Window`) has to refresh every window
    // itself.
    let refresh_all = window.is_none();
    match theme {
        ThemeSetting::System => Theme::sync_system_appearance(window, cx),
        ThemeSetting::Light => Theme::change(ThemeMode::Light, window, cx),
        ThemeSetting::Dark => Theme::change(ThemeMode::Dark, window, cx),
    }
    if refresh_all {
        cx.refresh_windows();
    }
}

/// What the group's status line says, if anything: a failed write outranks
/// the read state (it is the newer news), a failed read replaces the values,
/// and a healthy loaded state says nothing at all.
fn render_server_status_message(
    server: &ServerState,
    write_error: &Option<SharedString>,
) -> Option<(SharedString, bool)> {
    match (server, write_error) {
        (_, Some(error)) => Some((error.clone(), true)),
        (ServerState::Failed(error), _) => Some((error.clone(), true)),
        (ServerState::Loading, _) => Some((SharedString::from("Loading sidecar settings…"), false)),
        (ServerState::Loaded(_), None) => None,
    }
}

fn render_server_status(weak: &WeakEntity<SettingsPanel>, cx: &App) -> AnyElement {
    let Some(panel) = weak.upgrade() else {
        return div().into_any_element();
    };
    let panel = panel.read(cx);
    let Some((message, is_error)) = render_server_status_message(&panel.server, &panel.write_error)
    else {
        return div().into_any_element();
    };
    div()
        .text_xs()
        .text_color(if is_error {
            cx.theme().danger
        } else {
            cx.theme().muted_foreground
        })
        .child(message)
        .into_any_element()
}

impl Focusable for SettingsPanel {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .track_focus(&self.focus_handle)
            .debug_selector(|| "settings-surface".into())
            .key_context("Settings")
            .on_action(cx.listener(|_, _: &SettingsClose, _, cx| cx.emit(SettingsClosed)))
            .bg(cx.theme().background)
            // Clear the hiddenInset traffic lights, as the workspace this
            // replaces does.
            .pt(px(44.))
            .child(
                h_flex()
                    .h(px(40.))
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("settings-back")
                            .ghost()
                            .xsmall()
                            .icon(IconName::ArrowLeft)
                            .label("Back")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(SettingsClosed))),
                    )
                    .child(div().text_sm().font_semibold().child("Settings")),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(Settings::new("vitre-settings").pages(self.pages(cx))),
            )
    }
}

/// Emitted when the Back button is pressed; the shell listens and returns to
/// the workspace.
pub struct SettingsClosed;

impl gpui::EventEmitter<SettingsClosed> for SettingsPanel {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    /// With no environment attached there is nothing to read, and the rows the
    /// sidecar owns must not present contract defaults as the server's answer:
    /// clicking one would write a value the user never saw.
    #[gpui::test]
    fn the_server_rows_stay_disabled_until_the_sidecar_answers(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (panel, cx) = cx.add_window_view(|_, cx| SettingsPanel::new(None, cx));

        let (ready, message) = cx.update(|_, cx| {
            let panel = panel.read(cx);
            (
                panel.server_settings().is_some(),
                render_server_status_message(&panel.server, &panel.write_error),
            )
        });
        assert!(!ready, "no client means no server settings");
        let (message, is_error) = message.expect("an unreachable sidecar is reported, not hidden");
        assert!(is_error);
        assert!(message.contains("Not connected"));
    }

    /// Escape is the way out (Electron's settings route listens for it), and
    /// it is scoped to this surface's key context — so it only answers while
    /// the surface actually holds focus.
    #[gpui::test]
    fn escape_asks_the_shell_to_leave(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            cx.bind_keys([gpui::KeyBinding::new(
                "escape",
                SettingsClose,
                Some("Settings"),
            )])
        });
        let (panel, cx) = cx.add_window_view(|_, cx| SettingsPanel::new(None, cx));

        let closed = std::rc::Rc::new(std::cell::Cell::new(false));
        let flag = closed.clone();
        let subscription =
            cx.update(|_, cx| cx.subscribe(&panel, move |_, _: &SettingsClosed, _| flag.set(true)));
        cx.update(|window, cx| {
            window.focus(&panel.focus_handle(cx), cx);
            window.draw(cx).clear(cx);
        });

        cx.simulate_keystrokes("escape");

        assert!(closed.get(), "escape emitted the close event");
        drop(subscription);
    }

    /// The fork dispatches a row to a widget by the field's *runtime* type
    /// (`SettingItem::render_field`), and an unsupported pairing is an
    /// `unimplemented!()` at paint time, not a compile error. So this draws the
    /// whole surface: every row has to survive being rendered.
    #[gpui::test]
    fn every_row_survives_being_rendered(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_panel, cx) = cx.add_window_view(|_, cx| SettingsPanel::new(None, cx));

        cx.update(|window, cx| window.draw(cx).clear(cx));

        assert!(
            cx.debug_bounds("settings-surface").is_some(),
            "the settings surface painted"
        );
    }

    /// The patches are built as JSON, so a typo in a key would silently send
    /// an empty object. Each one has to land on the field it names.
    #[test]
    fn every_patch_key_lands_on_its_generated_field() {
        let streaming: ServerSettingsPatch =
            serde_json::from_value(serde_json::json!({ "enableAssistantStreaming": false }))
                .expect("decodes");
        assert_eq!(streaming.enable_assistant_streaming, Some(false));

        let compact: ServerSettingsPatch =
            serde_json::from_value(serde_json::json!({ "autoCompactEnabled": false }))
                .expect("decodes");
        assert_eq!(compact.auto_compact_enabled, Some(false));

        let threshold: ServerSettingsPatch = serde_json::from_value(serde_json::json!({
            "autoCompactThresholdTokens": AutoCompactThresholdTokens2(150_000),
        }))
        .expect("decodes");
        assert_eq!(
            threshold.auto_compact_threshold_tokens,
            Some(AutoCompactThresholdTokens2(150_000))
        );

        let updates: ServerSettingsPatch =
            serde_json::from_value(serde_json::json!({ "enableProviderUpdateChecks": true }))
                .expect("decodes");
        assert_eq!(updates.enable_provider_update_checks, Some(true));
    }

    /// The server deep-merges, so a patch must carry *only* the key it
    /// changes — anything else would overwrite a setting the user never
    /// touched (and, for provider instances, with redacted secrets).
    #[test]
    fn a_patch_serializes_to_exactly_one_key() {
        let patch: ServerSettingsPatch =
            serde_json::from_value(serde_json::json!({ "autoCompactEnabled": true }))
                .expect("decodes");
        let encoded = serde_json::to_value(&patch).expect("encodes");
        assert_eq!(encoded, serde_json::json!({ "autoCompactEnabled": true }));
    }
}
