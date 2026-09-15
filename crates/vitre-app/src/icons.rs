//! Shared, theme-aware native iconography. File artwork stays multicolour:
//! GPUI's `Icon`/SVG mask would discard the Pierre/T3 palette.
mod file_types;

use gpui::{
    AnyElement, App, Hsla, ObjectFit, RenderImage, StyledImage as _, div, img, prelude::*, px, rgb,
};
use gpui_component::{ActiveTheme as _, Icon, IconName};
use std::sync::Arc;

pub(crate) fn assets() -> &'static [(&'static str, &'static [u8])] {
    file_types::ASSETS
}

/// Provider artwork shared with the web composer (Icons.tsx, MIT).
pub(crate) fn provider_icon(driver: &str, cx: &App) -> Icon {
    let icon = match driver {
        "codex" | "claudeAgent" | "cursor" | "grok" | "opencode" => {
            Icon::default().path(format!("icons/providers/{driver}.svg"))
        }
        _ => Icon::new(IconName::Bot),
    };
    icon.size(px(16.)).text_color(if driver == "claudeAgent" {
        rgb(0xd97757).into()
    } else {
        cx.theme().foreground
    })
}

fn lookup<'a>(table: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    table
        .iter()
        .find_map(|(key, icon)| (*key == name).then_some(*icon))
}

fn extension<'a>(table: &'a [(&str, &str)], name: &str) -> Option<&'a str> {
    // Multipart extensions are tried longest first, exactly as in the web UI.
    for (offset, _) in name.match_indices('.') {
        if let Some((_, icon)) = table.iter().find(|(key, _)| *key == &name[offset + 1..]) {
            return Some(icon);
        }
    }
    None
}

pub(crate) fn file_icon_name(path: &str) -> &'static str {
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase();
    lookup(file_types::CUSTOM_NAMES, &name)
        .or_else(|| extension(file_types::CUSTOM_EXTENSIONS, &name))
        .or_else(|| lookup(file_types::BUILTIN_NAMES, &name))
        .or_else(|| extension(file_types::BUILTIN_EXTENSIONS, &name))
        .unwrap_or("file-tree-builtin-default")
}

pub(crate) fn file_icon(path: &str, cx: &App) -> AnyElement {
    let theme = if cx.theme().is_dark() {
        "dark"
    } else {
        "light"
    };
    let ink = cx.theme().muted_foreground;
    img(format!("file-icons/{theme}/{}.svg", file_icon_name(path)))
        .size(px(crate::ui::ICON))
        .flex_shrink_0()
        .object_fit(ObjectFit::Contain)
        .with_fallback(move || {
            Icon::new(IconName::File)
                .size(px(crate::ui::ICON))
                .text_color(ink)
                .into_any_element()
        })
        .into_any_element()
}

pub(crate) fn folder_icon(expanded: bool, cx: &App) -> AnyElement {
    Icon::new(if expanded {
        IconName::FolderOpen
    } else {
        IconName::Folder
    })
    .size(px(crate::ui::ICON))
    .flex_shrink_0()
    .text_color(cx.theme().muted_foreground)
    .into_any_element()
}

fn project_colour(identity: &str, dark: bool) -> Hsla {
    let hash = identity.bytes().fold(2_166_136_261_u32, |hash, byte| {
        (hash ^ byte as u32).wrapping_mul(16_777_619)
    });
    let colours = if dark {
        [0x93b4ff, 0x92d4bf, 0xd0adf0, 0xe6bd8d, 0x94cddd, 0xeba7b6]
    } else {
        [0x335dba, 0x27745c, 0x784b9c, 0x8a5b29, 0x286d80, 0xa34260]
    };
    rgb(colours[hash as usize % colours.len()]).into()
}

fn project_initial(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".into())
}

pub(crate) fn project_badge(
    name: &str,
    identity: &str,
    image: Option<Arc<RenderImage>>,
    cx: &App,
) -> AnyElement {
    let colour = project_colour(identity, cx.theme().is_dark());
    let initial = project_initial(name);
    let fallback = move || {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(11.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(colour)
            .child(initial.clone())
            .into_any_element()
    };
    let content = match image {
        Some(image) => img(image)
            .size(px(16.))
            .object_fit(ObjectFit::Contain)
            .with_loading(fallback.clone())
            .with_fallback(fallback)
            .into_any_element(),
        None => fallback(),
    };
    div()
        .size(px(20.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.))
        .border_1()
        .border_color(colour.opacity(0.18))
        .bg(colour.opacity(0.09))
        .child(content)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_web_filename_and_extension_precedence() {
        for (name, expected) in file_types::WEB_CASES {
            assert_eq!(file_icon_name(name), *expected, "{name}");
            assert_eq!(
                file_icon_name(&format!("C:\\workspace\\{}", name.to_uppercase())),
                *expected,
                "Windows/case: {name}"
            );
        }
    }

    #[test]
    fn every_resolved_icon_has_both_theme_assets() {
        for (_, name) in file_types::WEB_CASES {
            for theme in ["dark", "light"] {
                let path = format!("file-icons/{theme}/{name}.svg");
                let bytes = assets()
                    .iter()
                    .find(|(p, _)| *p == path)
                    .unwrap_or_else(|| panic!("Missing embedded icon {path}"))
                    .1;
                let svg = std::str::from_utf8(bytes).unwrap();
                assert!(svg.contains("<svg ") && svg.contains("</svg>"));
                assert!(!svg.contains("var(") && !svg.contains("href="));
            }
        }
    }

    #[test]
    fn every_embedded_svg_rasterizes_at_retina_size() {
        let renderer = gpui::SvgRenderer::new(Arc::new(()));
        for (path, bytes) in assets() {
            let svg = renderer
                .parse_svg(bytes)
                .unwrap_or_else(|e| panic!("{path}: {e}"));
            let image = renderer
                .render_parsed(
                    &svg,
                    gpui::SvgSize::ExactSize(gpui::size(
                        gpui::DevicePixels(32),
                        gpui::DevicePixels(32),
                    )),
                )
                .unwrap_or_else(|e| panic!("{path}: {e}"));
            assert_eq!(image.size(0).width, gpui::DevicePixels(32));
        }
    }

    #[test]
    fn standalone_icons_keep_the_web_language_palette() {
        for (theme, token, ink) in [
            ("dark", "react", "#68cdf2"),
            ("light", "react", "#1ca1c7"),
            ("dark", "rust", "#ffa359"),
            ("light", "typescript", "#1a85d4"),
        ] {
            let path = format!("file-icons/{theme}/file-tree-builtin-{token}.svg");
            let (_, bytes) = assets().iter().find(|(p, _)| *p == path).unwrap();
            assert!(
                std::str::from_utf8(bytes)
                    .unwrap()
                    .contains(&format!("color=\"{ink}\"")),
                "{path}"
            );
        }
    }

    #[test]
    fn project_badges_handle_unicode_and_have_stable_identity() {
        assert_eq!(project_initial("  vitre"), "V");
        assert_eq!(project_initial("éclair"), "É");
        assert_eq!(project_initial(""), "?");
        assert_eq!(project_colour("/repo", true), project_colour("/repo", true));
        assert_ne!(
            project_colour("/repo", true),
            project_colour("/repo", false)
        );
    }
}
