use super::*;
use gpui::{ObjectFit, StyledImage as _};
use gpui_component::WindowExt as _;

#[derive(Clone)]
pub struct FileContext {
    pub cwd: String,
    pub path: String,
}
pub enum FilesEvent {
    Opened(String),
    AddToChat(FileContext),
    CopyToThread(FileContext),
}
#[derive(Clone)]
pub(super) struct FileClipboard(pub FileContext);
impl gpui::Global for FileClipboard {}
impl gpui::EventEmitter<FilesEvent> for FilesPanel {}
impl Render for FileContext {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .p_2()
            .gap_2()
            .rounded_md()
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .child(crate::icons::file_icon(&self.path, cx))
            .child(self.path.clone())
    }
}
pub fn mention(path: &str) -> String {
    if path
        .chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\\')
    {
        format!("@{}", serde_json::to_string(path).unwrap())
    } else {
        format!("@{path}")
    }
}

pub(super) struct MediaPreview {
    pub path: String,
    image: Option<Arc<gpui::RenderImage>>,
    detail: String,
}
impl MediaPreview {
    pub fn render(&self, cx: &Context<FilesPanel>) -> AnyElement {
        v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .p_4()
            .gap_3()
            .items_center()
            .justify_center()
            .children(self.image.clone().map(|image| {
                gpui::img(image)
                    .object_fit(ObjectFit::Contain)
                    .w_full()
                    .flex_1()
                    .min_h_0()
            }))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.detail.clone()),
            )
            .into_any_element()
    }
}
pub fn is_image(path: &str) -> bool {
    matches!(
        std::path::Path::new(path)
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "ico"
    )
}
pub(super) fn is_markdown(path: &str) -> bool {
    matches!(
        language_for_path(path).as_str(),
        "md" | "markdown" | "mdown" | "mkd"
    )
}
impl FilesPanel {
    pub fn refresh_after_copy(&mut self, cx: &mut Context<Self>) {
        self.listed_at = None;
        self.refresh_tree(cx);
    }
    pub(super) fn copy_focused(&self, cx: &mut Context<Self>) {
        if let Some(path) = &self.tree_focused {
            cx.set_global(FileClipboard(FileContext {
                cwd: self.cwd.clone(),
                path: path.clone(),
            }));
        }
    }
    pub(super) fn paste_focused(&mut self, cx: &mut Context<Self>) {
        let Some(source) = cx.try_global::<FileClipboard>().cloned() else {
            return;
        };
        let parent = self
            .tree_focused
            .as_ref()
            .map(|path| {
                if self
                    .visible_rows()
                    .iter()
                    .any(|r| r.path == *path && r.is_dir)
                {
                    path.clone()
                } else {
                    path.rsplit_once('/')
                        .map(|(p, _)| p.to_string())
                        .unwrap_or_default()
                }
            })
            .unwrap_or_default();
        self.paste_entry(source.0, parent, cx);
    }
    pub(super) fn paste_entry(
        &mut self,
        source: FileContext,
        parent: String,
        cx: &mut Context<Self>,
    ) {
        let name = source.path.rsplit('/').next().unwrap_or(&source.path);
        let path = if parent.is_empty() {
            name.to_string()
        } else {
            format!("{parent}/{name}")
        };
        let client = self.client.clone();
        let input = vitre_contracts::ProjectCopyEntryInput {
            from_cwd: tnes(source.cwd),
            from_relative_path: tnes(source.path),
            to_cwd: tnes(&self.cwd),
            to_relative_path: tnes(&path),
            overwrite: Some(Some(false)),
        };
        cx.spawn(async move |this, cx| {
            let result = client
                .call::<vitre_contracts::methods::ProjectsCopyEntry>(&input)
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.status = Some(
                    match result {
                        Ok(_) => format!("Copied {path}"),
                        Err(e) => format!("Could not copy: {}", e.user_message()),
                    }
                    .into(),
                );
                panel.refresh_after_copy(cx);
                cx.notify();
            });
        })
        .detach();
    }
    #[cfg(debug_assertions)]
    pub(crate) fn verify_filter(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        self.filter
            .update(cx, |q, cx| q.set_value(query, window, cx));
        self.filter_query = query.into();
        *self.tree_rows.borrow_mut() = None;
        self.visible_rows().len()
    }
    #[cfg(debug_assertions)]
    pub(crate) fn verify_markdown_preview(&mut self, cx: &mut Context<Self>) {
        assert!(
            self.open
                .as_ref()
                .is_some_and(|f| is_markdown(&f.relative_path))
        );
        self.markdown_preview = true;
        cx.notify();
    }
    #[cfg(debug_assertions)]
    pub(crate) fn verify_image_loaded(&self) -> Option<bool> {
        self.media.as_ref().map(|m| m.image.is_some())
    }
    #[cfg(debug_assertions)]
    pub(crate) fn verify_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.compare_disk(window, cx);
    }
    pub(super) fn open_image(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.open_generation;
        let cwd = self.cwd.clone();
        let file_path = path.clone();
        self.media = Some(MediaPreview {
            path: path.clone(),
            image: None,
            detail: "Loading image…".into(),
        });
        // This client currently owns a local sidecar. Resolve against its root,
        // reject escaping symlinks, and keep I/O and decoding off the UI thread.
        let task = cx.background_executor().spawn(async move {
            use std::io::Read as _;
            let result = (|| -> anyhow::Result<(Arc<gpui::RenderImage>, String)> {
                let root = std::fs::canonicalize(cwd)?;
                let absolute = std::fs::canonicalize(root.join(&file_path))?;
                anyhow::ensure!(absolute.starts_with(root), "Image is outside the workspace");
                let mut bytes = Vec::new();
                std::fs::File::open(absolute)?
                    .take(32 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)?;
                anyhow::ensure!(bytes.len() <= 32 * 1024 * 1024, "Image exceeds 32 MB");
                let mut reader =
                    image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
                let mut limits = image::Limits::default();
                limits.max_alloc = Some(128 * 1024 * 1024);
                limits.max_image_width = Some(16384);
                limits.max_image_height = Some(16384);
                reader.limits(limits);
                let decoded = reader.decode()?;
                let detail = format!(
                    "{} × {} · {} KB",
                    decoded.width(),
                    decoded.height(),
                    bytes.len().div_ceil(1024)
                );
                let mut png = std::io::Cursor::new(Vec::new());
                decoded
                    .thumbnail(4096, 4096)
                    .write_to(&mut png, image::ImageFormat::Png)?;
                let image = gpui::Image::from_bytes(gpui::ImageFormat::Png, png.into_inner())
                    .to_image_data(gpui::SvgRenderer::new(Arc::new(())))?;
                Ok((image, detail))
            })();
            result.map_err(|e| e.to_string())
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.open_generation != generation {
                    return;
                }
                if this
                    .open
                    .as_ref()
                    .is_some_and(|open| open.buffer.is_dirty())
                {
                    this.media = None;
                    this.status =
                        Some("Save or discard your changes before opening another file.".into());
                    if let Some(open) = &this.open {
                        cx.emit(FilesEvent::Opened(open.relative_path.clone()));
                    }
                    cx.notify();
                    return;
                }
                this.open = None;
                this.open_error = None;
                this.lsp.close_document(cx);
                this.media = Some(match result {
                    Ok((image, detail)) => MediaPreview {
                        path: path.clone(),
                        image: Some(image),
                        detail,
                    },
                    Err(error) => MediaPreview {
                        path: path.clone(),
                        image: None,
                        detail: format!("Could not preview image: {error}"),
                    },
                });
                cx.emit(FilesEvent::Opened(path));
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn compare_disk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        let path = open.relative_path.clone();
        let mine = self.editor.read(cx).value().to_string();
        let client = self.client.clone();
        let cwd = self.cwd.clone();
        cx.spawn_in(window, async move |_, cx| {
            let result = client
                .call::<ProjectsReadFile>(&ProjectReadFileInput {
                    cwd: tnes(cwd),
                    relative_path: tnes(&path),
                })
                .await;
            let _ = cx.update(|window, cx| match result {
                Err(error) => window.push_notification(
                    gpui_component::notification::Notification::error(error.user_message()),
                    cx,
                ),
                Ok(file) => {
                    let local = cx.new(|cx| {
                        let mut s = EditorState::new(window, cx).line_number(true);
                        s.set_highlighter(language_for_path(&path), cx);
                        s.set_value(mine, window, cx);
                        s
                    });
                    let disk = cx.new(|cx| {
                        let mut s = EditorState::new(window, cx).line_number(true);
                        s.set_highlighter(language_for_path(&path), cx);
                        s.set_value(file.contents.0, window, cx);
                        s
                    });
                    window.open_dialog(cx, move |dialog, window, _| {
                        dialog
                            .title(format!("Compare {path}"))
                            .w(window.rem_size() * 60.)
                            .child(
                                h_flex()
                                    .items_stretch()
                                    .gap_3()
                                    .h(gpui::rems(26.))
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .h_full()
                                            .min_h_0()
                                            .min_w_0()
                                            .gap_2()
                                            .child("Your unsaved version")
                                            .child(
                                                div().flex_1().min_h_0().child(
                                                    Editor::new(&local)
                                                        .readonly(true)
                                                        .appearance(false)
                                                        .size_full(),
                                                ),
                                            ),
                                    )
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .h_full()
                                            .min_h_0()
                                            .min_w_0()
                                            .gap_2()
                                            .child(if file.truncated {
                                                "On disk · partial preview"
                                            } else {
                                                "On disk"
                                            })
                                            .child(
                                                div().flex_1().min_h_0().child(
                                                    Editor::new(&disk)
                                                        .readonly(true)
                                                        .appearance(false)
                                                        .size_full(),
                                                ),
                                            ),
                                    ),
                            )
                    });
                }
            });
        })
        .detach();
    }
}
