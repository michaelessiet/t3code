//! Native language-server commands shared by keyboard and context menu.
use super::*;
use gpui_component::{ThemeStyled as _, WindowExt as _};
use vitre_contracts::methods::{LspReferences, LspSignatureHelp};

#[derive(Default)]
pub(super) struct LanguageTools {
    signature_generation: u64,
    signature_active: bool,
    signature: Option<(usize, SharedString)>,
}

impl FilesPanel {
    pub(super) fn cancel_signature_help(&mut self, cx: &mut Context<Self>) {
        self.language_tools.signature_generation += 1;
        self.language_tools.signature_active = false;
        self.language_tools.signature = None;
        self.editor
            .update(cx, |state, cx| state.clear_hover_state(cx));
    }

    pub(super) fn update_signature_help(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.editor.read(cx);
        let text = editor.text();
        let offset = editor.cursor().min(text.len());
        let last = text
            .slice(text.floor_char_boundary(offset.saturating_sub(1))..offset)
            .chars()
            .next();
        self.language_tools.signature_generation += 1;
        self.language_tools.signature = None;
        if last == Some(')') {
            self.language_tools.signature_active = false;
            self.editor.update(cx, |s, cx| s.clear_hover_state(cx));
        } else if self.language_tools.signature_active || matches!(last, Some('(' | ',')) {
            self.show_signature_help(false, window, cx);
        }
    }

    pub(super) fn show_signature_help(
        &mut self,
        manual: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self.editor.read(cx);
        if !editor.focus_handle(cx).is_focused(window) {
            return;
        }
        let Some(path) = self.lsp.current_document() else {
            return;
        };
        let text = editor.text().clone();
        let offset = editor.cursor();
        let generation = self.open_generation;
        self.language_tools.signature_generation += 1;
        let request = self.language_tools.signature_generation;
        let payload = self
            .lsp
            .position_payload(&path, positions::offset_to_wire(&text, offset));
        let flush = self.lsp.flush_document(&text, cx);
        let client = self.client.clone();
        cx.spawn_in(window, async move |this, cx| {
            if !manual {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
            }
            let current = this
                .update(cx, |p, cx| {
                    p.open_generation == generation
                        && p.language_tools.signature_generation == request
                        && p.editor.read(cx).text() == &text
                })
                .unwrap_or(false);
            if !current {
                return;
            }
            flush.await;
            let result = client.call::<LspSignatureHelp>(&payload).await;
            let _ = this.update_in(cx, |panel, window, cx| {
                if panel.open_generation != generation
                    || panel.language_tools.signature_generation != request
                    || panel.editor.read(cx).text() != &text
                    || panel.editor.read(cx).cursor() != offset
                    || !panel.editor.read(cx).focus_handle(cx).is_focused(window)
                {
                    return;
                }
                match result {
                    Ok(Some(help)) => {
                        let Some(signature) = help
                            .signatures
                            .get(help.active_signature.0.max(0) as usize)
                            .or_else(|| help.signatures.first())
                        else {
                            return;
                        };
                        let mut markdown = format!(
                            "```{}\n{}\n```",
                            language_for_path(&path),
                            signature.label.0
                        );
                        if let Some(parameter) = signature
                            .parameters
                            .get(help.active_parameter.0.max(0) as usize)
                        {
                            markdown.push_str(&format!(
                                "\n\nParameter {}: `{}`",
                                help.active_parameter.0 + 1,
                                parameter.label.0
                            ));
                            if let Some(Some(doc)) = &parameter.documentation {
                                markdown.push_str(&format!("\n\n{}", doc.0));
                            }
                        }
                        if let Some(Some(doc)) = &signature.documentation {
                            markdown.push_str(&format!("\n\n{}", doc.0));
                        }
                        if help.signatures.len() > 1 {
                            markdown.push_str(&format!(
                                "\n\nOverload {} of {}",
                                help.active_signature.0 + 1,
                                help.signatures.len()
                            ));
                        }
                        panel.language_tools.signature_active = true;
                        panel.language_tools.signature = Some((offset, markdown.into()));
                    }
                    Ok(None) => {
                        panel.language_tools.signature_active = false;
                        panel.language_tools.signature = None;
                        if manual {
                            window.push_notification("No signature at this position.", cx);
                        }
                    }
                    Err(error) if manual => {
                        window.push_notification(error.user_message(), cx);
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_signature_help(
        &self,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let (offset, markdown) = self.language_tools.signature.as_ref()?;
        let editor = self.editor.read(cx);
        if !editor.focus_handle(cx).is_focused(window) || editor.cursor() != *offset {
            return None;
        }
        let bounds = editor.range_to_bounds(&(*offset..*offset))?;
        // Independent of mouse-hover state: moving the pointer must not
        // dismiss parameter hints while the user is typing arguments.
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(bounds.origin - gpui::point(px(0.), px(8.)))
                    .anchor(gpui::Anchor::BottomLeft)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .id("language-signature")
                            .occlude()
                            .w(px(460.).min(window.viewport_size().width - px(16.)))
                            .popover_style(cx)
                            .rounded_lg()
                            .overflow_hidden()
                            .shadow_lg()
                            .p_3()
                            .text_xs()
                            .child(
                                div()
                                    .id("language-signature-scroll")
                                    .max_h(px(260.))
                                    .overflow_y_scrollbar()
                                    .child(gpui_component::text::TextView::markdown(
                                        "signature-content",
                                        markdown.clone(),
                                    )),
                            ),
                    ),
            )
            .into_any_element(),
        )
    }

    #[cfg(debug_assertions)]
    pub(super) fn verification_signature(&self) -> Option<&str> {
        self.language_tools
            .signature
            .as_ref()
            .map(|(_, text)| text.as_ref())
    }

    pub(super) fn find_references(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.editor.read(cx);
        if !editor.focus_handle(cx).is_focused(window) {
            return;
        }
        let Some(path) = self.lsp.current_document() else {
            return;
        };
        let text = editor.text().clone();
        let payload = self
            .lsp
            .position_payload(&path, positions::offset_to_wire(&text, editor.cursor()));
        let flush = self.lsp.flush_document(&text, cx);
        let generation = self.open_generation;
        let client = self.client.clone();
        cx.spawn_in(window, async move |this, cx| {
            flush.await;
            let result = client.call::<LspReferences>(&payload).await;
            let _ = this.update_in(cx, |panel, window, cx| {
                if panel.open_generation != generation || panel.editor.read(cx).text() != &text {
                    return;
                }
                let result = match result {
                    Ok(result) => result,
                    Err(error) => {
                        window.push_notification(error.user_message(), cx);
                        return;
                    }
                };
                let locations: Vec<_> = result
                    .locations
                    .into_iter()
                    .filter_map(|location| {
                        Some((
                            location.relative_path.flatten()?.0,
                            bridge::wire_position(&location.range.start),
                        ))
                    })
                    .collect();
                if locations.is_empty() {
                    window.push_notification("No references found in this workspace.", cx);
                    return;
                }
                let locations = Rc::new(locations);
                let entity = cx.entity().downgrade();
                window.open_dialog(cx, move |dialog, _, _| {
                    let locations = locations.clone();
                    let count = locations.len();
                    let entity = entity.clone();
                    dialog
                        .title(format!("{} references", locations.len()))
                        .w(px(680.))
                        .child(
                            uniform_list(
                                "language-references",
                                locations.len(),
                                move |range, _, _| {
                                    range
                                        .map(|index| {
                                            let (path, position) = locations[index].clone();
                                            let target = path.clone();
                                            let entity = entity.clone();
                                            Button::new(("reference", index))
                                                .ghost()
                                                .w_full()
                                                .justify_start()
                                                .label(format!(
                                                    "{path}:{}:{}",
                                                    position.line + 1,
                                                    position.character + 1
                                                ))
                                                .on_click(move |_, window, cx| {
                                                    window.close_dialog(cx);
                                                    let _ = entity.update(cx, |panel, cx| {
                                                        panel.reveal(
                                                            target.clone(),
                                                            Some(RevealTarget::at(position)),
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                })
                                                .into_any_element()
                                        })
                                        .collect()
                                },
                            )
                            .h(px((count.min(10) * 36) as f32))
                            .w_full(),
                        )
                });
            });
        })
        .detach();
    }
}
