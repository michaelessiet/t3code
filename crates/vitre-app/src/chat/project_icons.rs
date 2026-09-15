//! Project favicons from the same signed-asset resolver as the web app.
//! GPUI has no HTTP client installed, so decode bounded thumbnails off-thread
//! and hand it render images, not remote URLs. Missing/broken artwork is quiet.
use super::*;
use anyhow::{Context as _, ensure};
use gpui::{App, DevicePixels, RenderImage, SvgRenderer, SvgSize};
use std::time::{Duration, Instant};

#[cfg(debug_assertions)]
#[path = "icons_verification.rs"]
mod verification;

const MAX_BYTES: usize = 256 * 1024;
const CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Default)]
pub(super) struct ProjectIcons {
    origin: String,
    epoch: u64,
    entries: HashMap<String, CachedIcon>,
    loading: HashSet<String>,
}

struct CachedIcon {
    image: Option<Arc<RenderImage>>,
    retry_at: Instant,
}

impl ChatApp {
    pub(super) fn project_icon(&self, name: &str, cwd: &str, cx: &App) -> AnyElement {
        let image = self
            .project_icons
            .entries
            .get(cwd)
            .and_then(|entry| entry.image.clone());
        crate::icons::project_badge(name, cwd, image, cx)
    }

    pub(super) fn sync_project_icons(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(session) = client.sessions().borrow().clone() else {
            return;
        };
        let base = session.target.base_url;
        if self.project_icons.origin != base {
            self.project_icons.origin.clone_from(&base);
            self.project_icons.epoch = self.project_icons.epoch.wrapping_add(1);
            self.project_icons.entries.clear();
            self.project_icons.loading.clear();
        }
        let Some(snapshot) = &self.shell.snapshot else {
            return;
        };
        let roots: HashSet<_> = snapshot
            .projects
            .iter()
            .map(|p| p.workspace_root.0.clone())
            .collect();
        self.project_icons
            .entries
            .retain(|cwd, _| roots.contains(cwd));
        let pending: Vec<_> = roots
            .into_iter()
            .filter(|cwd| {
                !self.project_icons.loading.contains(cwd)
                    && self
                        .project_icons
                        .entries
                        .get(cwd)
                        .is_none_or(|entry| entry.retry_at <= Instant::now())
            })
            .take(4_usize.saturating_sub(self.project_icons.loading.len()))
            .collect();
        for cwd in pending {
            self.project_icons.loading.insert(cwd.clone());
            let epoch = self.project_icons.epoch;
            let client = client.clone();
            let base = base.clone();
            let root = cwd.clone();
            let task = Tokio::spawn_result(cx, async move {
                tokio::time::timeout(Duration::from_secs(15), fetch_icon(client, &base, &root))
                    .await?
            });
            cx.spawn(async move |this, cx| {
                let image = task.await.ok().flatten();
                let _ = this.update(cx, |app, cx| {
                    if app.project_icons.epoch != epoch {
                        return;
                    }
                    app.project_icons.loading.remove(&cwd);
                    app.project_icons.entries.insert(
                        cwd,
                        CachedIcon {
                            image,
                            retry_at: Instant::now() + CACHE_TTL,
                        },
                    );
                    cx.notify();
                });
            })
            .detach();
        }
    }
}

fn asset_url(base: &str, relative: &str) -> anyhow::Result<reqwest::Url> {
    let base = reqwest::Url::parse(base)?;
    let url = base.join(relative)?;
    ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.origin() == base.origin()
            && url.username().is_empty()
            && url.password().is_none(),
        "Invalid asset origin"
    );
    Ok(url)
}

async fn fetch_icon(
    client: Arc<EnvironmentClient>,
    base: &str,
    cwd: &str,
) -> anyhow::Result<Option<Arc<RenderImage>>> {
    let signed = client
        .call::<AssetsCreateUrl>(&AssetCreateUrlInput {
            resource: AssetResource::ProjectFavicon { cwd: tnes(cwd) },
        })
        .await
        .map_err(|_| anyhow::anyhow!("Project icon unavailable"))?;
    let url = asset_url(base, &signed.relative_url.0)?;
    if url.path().rsplit('/').next() == Some("project-favicon-missing") {
        return Ok(None);
    }
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8))
        .build()?;
    let mut response = http.get(url).send().await?.error_for_status()?;
    ensure!(response.status().is_success(), "Asset redirect rejected");
    ensure!(
        response
            .content_length()
            .is_none_or(|size| size <= MAX_BYTES as u64),
        "Icon exceeds size limit"
    );
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= MAX_BYTES,
            "Icon exceeds size limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    // Parsing/rasterization must not occupy a Tokio IO worker or the UI thread.
    tokio::task::spawn_blocking(move || decode_icon(&bytes, &mime).map(Some)).await?
}

fn validate_svg(bytes: &[u8]) -> anyhow::Result<()> {
    let text = std::str::from_utf8(bytes)?;
    let document = roxmltree::Document::parse(text)?; // DTD/entities disabled.
    ensure!(
        document.root_element().tag_name().name() == "svg",
        "Not an SVG"
    );
    ensure!(document.descendants().count() <= 4096, "Overly complex SVG");
    for node in document.descendants().filter(|n| n.is_element()) {
        ensure!(
            !matches!(node.tag_name().name(), "script" | "foreignObject" | "image"),
            "Unsupported SVG content"
        );
        for attr in node.attributes() {
            ensure!(
                attr.name() != "href" || attr.value().starts_with('#'),
                "External SVG resource"
            );
        }
    }
    // usvg does not execute scripts/CSS, but keep input self-contained as well.
    ensure!(
        !text.to_ascii_lowercase().contains("@import"),
        "External SVG style"
    );
    Ok(())
}

fn decode_icon(bytes: &[u8], mime: &str) -> anyhow::Result<Arc<RenderImage>> {
    ensure!(bytes.len() <= MAX_BYTES, "Icon exceeds size limit");
    if mime.split(';').next().unwrap_or_default().trim() == "image/svg+xml" {
        validate_svg(bytes)?;
        let renderer = SvgRenderer::new(Arc::new(()));
        let parsed = renderer.parse_svg(bytes)?;
        return renderer
            .render_parsed(
                &parsed,
                // Width-constrained rendering preserves non-square project
                // marks; GPUI also caps the long edge at its texture limit.
                SvgSize::Size(gpui::size(DevicePixels(64), DevicePixels(64))),
            )
            .context("Rasterize icon");
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);
    limits.max_alloc = Some(8 * 1024 * 1024);
    reader.limits(limits);
    let thumbnail = reader.decode()?.thumbnail(64, 64);
    let mut encoded = std::io::Cursor::new(Vec::new());
    thumbnail.write_to(&mut encoded, image::ImageFormat::Png)?;
    gpui::Image::from_bytes(gpui::ImageFormat::Png, encoded.into_inner())
        .to_image_data(SvgRenderer::new(Arc::new(())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_asset_stays_on_environment_origin() {
        assert!(asset_url("http://127.0.0.1:3774", "/api/assets/signed?token=test").is_ok());
        for url in [
            "https://example.com/icon",
            "//evil.test/icon",
            "http://127.0.0.1:4000/icon",
            "data:image/svg+xml,test",
            "http://user@127.0.0.1:3774/icon",
        ] {
            assert!(asset_url("http://127.0.0.1:3774", url).is_err(), "{url}");
        }
    }

    #[test]
    fn svg_favicons_cannot_load_external_resources() {
        assert!(
            validate_svg(br##"<svg xmlns="http://www.w3.org/2000/svg"><use href="#mark"/></svg>"##)
                .is_ok()
        );
        for svg in [
            r#"<svg><use href="file:///tmp/private.svg"/></svg>"#,
            r#"<svg><image href="https://example.com/image.png"/></svg>"#,
            r#"<svg><script/></svg>"#,
            r#"<!DOCTYPE svg [<!ENTITY secret SYSTEM "file:///tmp/private">]><svg>&secret;</svg>"#,
        ] {
            assert!(validate_svg(svg.as_bytes()).is_err());
        }
    }

    #[test]
    fn decodes_bounded_svg_and_raster_thumbnails() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path fill="#89aaff" d="M0 0h16v16H0z"/></svg>"##;
        assert_eq!(
            decode_icon(svg, "image/svg+xml; charset=utf-8")
                .unwrap()
                .size(0),
            gpui::size(DevicePixels(64), DevicePixels(64))
        );
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(128, 64)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(
            decode_icon(bytes.get_ref(), "image/png").unwrap().size(0),
            gpui::size(DevicePixels(64), DevicePixels(32))
        );
        assert!(decode_icon(b"broken image", "image/png").is_err());
        let mut oversized = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(1025, 1)
            .write_to(&mut oversized, image::ImageFormat::Png)
            .unwrap();
        assert!(decode_icon(oversized.get_ref(), "image/png").is_err());
        assert!(decode_icon(&vec![0; MAX_BYTES + 1], "image/svg+xml").is_err());
        let wide = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 16"><path d="M0 0h32v16H0z"/></svg>"##;
        assert_eq!(
            decode_icon(wide, "image/svg+xml").unwrap().size(0),
            gpui::size(DevicePixels(64), DevicePixels(32))
        );
    }
}
