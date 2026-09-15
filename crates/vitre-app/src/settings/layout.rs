//! T3's settings route chrome. Controls retain the native setting framework;
//! the application's navigation, widths and row hierarchy match the web UI.
use super::*;
use gpui::{
    AppContext as _, Entity, IntoElement, RenderOnce, Subscription, prelude::FluentBuilder as _,
    rems,
};
use gpui_component::{
    Icon,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
};

#[derive(Default)]
pub(super) struct SettingsLayout {
    pub selected: String,
    search: Option<Entity<InputState>>,
    search_subscription: Option<Subscription>,
}

impl SettingsPanel {
    pub(super) fn about_group(&self, cx: &Context<Self>) -> SettingGroup {
        let owner = cx.entity().downgrade();
        SettingGroup::new()
            .title("About")
            .item(SettingItem::render(move |_, _, cx| {
                let diagnostics = owner.clone();
                let parity = owner.clone();
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Vitre development build"),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("settings-diagnostics")
                                    .small()
                                    .ghost()
                                    .label("Diagnostics")
                                    .on_click(move |_, _, cx| {
                                        let _ = diagnostics.update(cx, |p, cx| {
                                            p.layout.selected = "Diagnostics".into();
                                            p.refresh_operations(cx);
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                Button::new("settings-parity")
                                    .small()
                                    .ghost()
                                    .label("Feature parity status")
                                    .on_click(move |_, _, cx| {
                                        let _ = parity.update(cx, |p, cx| {
                                            p.layout.selected = "Not yet in Vitre".into();
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .into_any_element()
            }))
    }
    #[cfg(debug_assertions)]
    pub(crate) fn verification_select(&mut self, page: &str, cx: &mut Context<Self>) {
        self.layout.selected = page.into();
        if page == "Knowledge Graph" {
            self.refresh_graph_runtime(cx);
        }
        if matches!(page, "Archived threads" | "Source control") {
            self.refresh_operations(cx);
        }
    }
    pub(super) fn render_layout(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.layout.search.is_none() {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search settings…"));
            self.layout.search_subscription = Some(cx.subscribe(&input, |_, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }));
            self.layout.search = Some(input);
        }
        let search = self.layout.search.as_ref().unwrap();
        let query = search.read(cx).value().trim().to_lowercase();
        let pages = self.pages(cx);
        let selected = if self.layout.selected.is_empty() {
            "General"
        } else {
            &self.layout.selected
        };
        let index = pages
            .iter()
            .position(|p| p.label().as_ref() == selected)
            .unwrap_or(0);
        let mut nav = v_flex().flex_1().gap_1().px_2().py_3();
        // Keep the established T3 navigation order (diagnostics is an About link).
        for (label, page, icon) in [
            ("General", "General", IconName::Settings),
            ("Keybindings", "Keybindings", IconName::Settings),
            ("Providers", "Providers", IconName::Bot),
            (
                "Language Servers",
                "Language servers",
                IconName::SquareTerminal,
            ),
            ("Knowledge Graph", "Knowledge Graph", IconName::Network),
            ("Source Control", "Source control", IconName::GitBranch),
            ("Connections", "Connections", IconName::Link),
            ("Beta", "Beta", IconName::Info),
            ("Archive", "Archived threads", IconName::Archive),
        ] {
            if !pages.iter().any(|p| p.label().as_ref() == page) {
                continue;
            }
            nav = nav.child(
                Button::new(page)
                    .ghost()
                    .small()
                    .w_full()
                    .h_8()
                    .justify_start()
                    .icon(match page {
                        "General" => Icon::new(crate::assets::VitreIcon::Settings2),
                        "Keybindings" => Icon::new(crate::assets::VitreIcon::Keyboard),
                        "Language servers" => Icon::new(crate::assets::VitreIcon::Braces),
                        "Beta" => Icon::new(crate::assets::VitreIcon::FlaskConical),
                        _ => Icon::new(icon),
                    })
                    .label(label)
                    .when(selected == page, |b| b.bg(cx.theme().list_active))
                    .on_click(cx.listener(move |p, _, window, cx| {
                        p.layout.selected = page.into();
                        if page == "Knowledge Graph" {
                            p.refresh_graph_runtime(cx);
                        }
                        if matches!(page, "Archived threads" | "Source control") {
                            p.refresh_operations(cx);
                        }
                        if let Some(search) = &p.layout.search {
                            search.update(cx, |i, cx| i.set_value("", window, cx));
                        }
                        cx.notify();
                    })),
            );
        }
        let content = SettingsContent {
            pages,
            index,
            query,
        };
        h_flex()
            .size_full()
            .items_stretch()
            .child(
                v_flex()
                    .w(rems(16.))
                    .flex_none()
                    .border_r_1()
                    .border_color(cx.theme().border.opacity(0.5))
                    .bg(crate::glass::sidebar(cx))
                    .child(
                        h_flex()
                            .h(rems(3.25))
                            .pl(px(100.))
                            .gap_3()
                            .child(Icon::new(IconName::PanelLeft).size_4())
                            .child(div().text_sm().font_semibold().child("T3 Code")),
                    )
                    .child(nav)
                    .child(
                        Button::new("settings-back")
                            .ghost()
                            .small()
                            .h_9()
                            .m_2()
                            .justify_start()
                            .icon(IconName::ArrowLeft)
                            .label("Back")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(SettingsClosed))),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(
                        h_flex()
                            .h(rems(3.25))
                            .flex_none()
                            .px_5()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Settings"),
                            )
                            .child(
                                div().w(rems(12.)).child(
                                    Input::new(search)
                                        .small()
                                        .prefix(Icon::new(IconName::Search).size_3p5()),
                                ),
                            ),
                    )
                    .child(div().flex_1().min_h_0().child(content)),
            )
            .into_any_element()
    }
}

// Render after SettingsPanel releases its entity borrow: field getter closures
// are allowed to read that panel's server snapshot.
#[derive(IntoElement)]
struct SettingsContent {
    pages: Vec<SettingPage>,
    index: usize,
    query: String,
}

impl RenderOnce for SettingsContent {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut body = v_flex().w_full().max_w(rems(56.)).mx_auto().gap_10();
        let mut count = 0;
        for (page_ix, page) in self.pages.into_iter().enumerate() {
            if self.query.is_empty() && page_ix != self.index {
                continue;
            }
            let page_match = page.label().to_lowercase().contains(&self.query);
            let mut groups = v_flex().gap_8();
            let mut matches = 0;
            for (group_ix, group) in page.sections().iter().enumerate() {
                let mut rows = v_flex().gap_1();
                let mut group_matches = 0;
                for (item_ix, item) in group.entries().iter().enumerate() {
                    let matches_query = match item {
                        SettingItem::Item {
                            title, keywords, ..
                        } => {
                            title.to_lowercase().contains(&self.query)
                                || keywords
                                    .iter()
                                    .any(|k| k.to_lowercase().contains(&self.query))
                        }
                        SettingItem::Element { keywords, .. } => keywords
                            .iter()
                            .any(|k| k.to_lowercase().contains(&self.query)),
                    };
                    if !self.query.is_empty() && !page_match && !matches_query {
                        continue;
                    }
                    let options = RenderOptions::new()
                        .with_page_ix(page_ix)
                        .with_group_ix(group_ix)
                        .with_item_ix(item_ix)
                        .with_size(gpui_component::Size::Small);
                    rows = rows.child(render_row(item, options, window, cx));
                    group_matches += 1;
                }
                if group_matches == 0 {
                    continue;
                }
                groups = groups.child(
                    v_flex()
                        .gap_2()
                        .children(
                            group
                                .heading()
                                .filter(|t| t.as_ref() != page.label().as_ref())
                                .map(|title| {
                                    div().px_4().text_sm().font_semibold().child(title.clone())
                                }),
                        )
                        .child(rows),
                );
                matches += group_matches;
            }
            if matches > 0 {
                let reset_items = page
                    .sections()
                    .iter()
                    .flat_map(|g| g.entries())
                    .cloned()
                    .collect::<Vec<_>>();
                let dirty = page.label().as_ref() == "General"
                    && reset_items.iter().any(|item| match item {
                        SettingItem::Item {
                            field, disabled, ..
                        } => !*disabled && field.is_resettable(cx),
                        SettingItem::Element {
                            reset_handler: Some((dirty, _)),
                            disabled,
                            ..
                        } => !*disabled && dirty(cx),
                        _ => false,
                    });
                body = body.child(
                    v_flex()
                        .gap_3()
                        .when(page.label().as_ref() != "Providers", |body| {
                            body.child(
                                h_flex()
                                    .px_4()
                                    .w_full()
                                    .justify_between()
                                    .child(
                                        div().text_lg().font_semibold().child(page.label().clone()),
                                    )
                                    .when(dirty, |row| {
                                        row.child(
                                            Button::new("reset-general-settings")
                                                .ghost()
                                                .small()
                                                .label("Restore defaults")
                                                .on_click(move |_, window, cx| {
                                                    for item in &reset_items {
                                                        match item {
                                                            SettingItem::Item {
                                                                field,
                                                                disabled,
                                                                ..
                                                            } if !disabled => {
                                                                field.reset(window, cx)
                                                            }
                                                            SettingItem::Element {
                                                                reset_handler: Some((_, reset)),
                                                                disabled,
                                                                ..
                                                            } if !disabled => reset(window, cx),
                                                            _ => {}
                                                        }
                                                    }
                                                }),
                                        )
                                    }),
                            )
                        })
                        .child(groups),
                );
                count += matches;
            }
        }
        if count == 0 {
            body = body.child(
                div()
                    .px_4()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No settings found."),
            );
        }
        div()
            .id("settings-content-scroll")
            .size_full()
            .overflow_y_scrollbar()
            .px_8()
            .pt_10()
            .pb_10()
            .child(body)
    }
}

fn render_row(
    item: &SettingItem,
    options: RenderOptions,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    match item {
        SettingItem::Item {
            title,
            description,
            disabled,
            field,
            ..
        } => {
            let reset_field = field.clone();
            let control = SettingItem::render_field(
                field.clone(),
                options.with_disabled(*disabled),
                window,
                cx,
            )
            .into_any_element();
            h_flex()
                .id(title.clone())
                .w_full()
                .px_4()
                .py_3()
                .gap_8()
                .rounded_xl()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(div().text_sm().font_medium().child(title.clone()))
                                .when(field.is_resettable(cx), |row| {
                                    row.child(
                                        Button::new("reset-setting")
                                            .ghost()
                                            .xsmall()
                                            .icon(IconName::Undo2)
                                            .tooltip("Restore default")
                                            .on_click(move |_, window, cx| {
                                                reset_field.reset(window, cx)
                                            }),
                                    )
                                }),
                        )
                        .children(description.clone().map(|d| {
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground.opacity(0.8))
                                .child(d)
                        })),
                )
                .child(
                    h_flex()
                        .w(rems(12.))
                        .flex_none()
                        .justify_end()
                        .child(control),
                )
                .into_any_element()
        }
        SettingItem::Element {
            disabled, render, ..
        } => h_flex()
            .id(SharedString::from(format!(
                "setting-{}-{}-{}",
                options.page_ix(),
                options.group_ix(),
                options.item_ix()
            )))
            .w_full()
            .px_4()
            .py_2()
            .child(render(&options.with_disabled(*disabled), window, cx))
            .into_any_element(),
    }
}
