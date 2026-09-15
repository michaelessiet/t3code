//! Preview-first workspace edits. Every write is revision-guarded; resource
//! operations and opaque server commands are intentionally never executed.
use super::*;
use gpui_component::{Disableable as _, WindowExt as _, dialog::DialogButtonProps};
use ropey::Rope;
use vitre_contracts::methods::{LspCodeActions, LspRename, LspResolveCodeAction};

#[cfg(debug_assertions)]
mod verification;
use vitre_contracts::{
    LspCodeAction, LspCodeActionsInput, LspFileEdits, LspRenameInput, LspResolveCodeActionInput,
    LspTextEdit,
};

#[derive(Clone)]
struct PreparedFile {
    path: String,
    before: String,
    after: String,
    revision: TrimmedNonEmptyString,
}

/// Editing must reject invalid ranges, not silently clamp them as navigation
/// does. In particular a UTF-16 position inside a surrogate is not writable.
fn apply_edits(contents: &str, edits: &[LspTextEdit]) -> anyhow::Result<String> {
    let text = Rope::from(contents);
    let mut converted = Vec::new();
    for edit in edits {
        let convert = |p: &vitre_contracts::LspPosition| -> anyhow::Result<usize> {
            let pos = WirePosition {
                line: u32::try_from(p.line.0)?,
                character: u32::try_from(p.character.0)?,
            };
            let offset = wire_to_offset(&text, pos);
            anyhow::ensure!(
                positions::offset_to_wire(&text, offset) == pos,
                "Language server returned an invalid edit position"
            );
            Ok(offset)
        };
        let start = convert(&edit.range.start)?;
        let end = convert(&edit.range.end)?;
        anyhow::ensure!(start <= end, "Language server returned a reversed edit");
        converted.push((start..end, edit.new_text.0.as_str()));
    }
    converted.sort_by_key(|(range, _)| (range.start, range.end));
    for pair in converted.windows(2) {
        anyhow::ensure!(
            pair[0].0.end <= pair[1].0.start && pair[0].0 != pair[1].0,
            "Language server returned overlapping edits"
        );
    }
    let mut result = contents.to_string();
    for (range, replacement) in converted.into_iter().rev() {
        result.replace_range(range, replacement);
    }
    Ok(result)
}

impl FilesPanel {
    fn refactor_ready(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.refactor_busy {
            return false;
        }
        let Some(open) = &self.open else {
            return false;
        };
        if open.truncated || open.buffer.is_dirty() {
            window.push_notification("Save this file before requesting a workspace refactor.", cx);
            return false;
        }
        true
    }

    pub(super) fn rename_symbol(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.refactor_ready(window, cx) {
            return;
        }
        let Some(path) = self.lsp.current_document() else {
            return;
        };
        let source = self.editor.read(cx).text().clone();
        let position = self
            .lsp
            .position_payload(
                &path,
                positions::offset_to_wire(&source, self.editor.read(cx).cursor()),
            )
            .position;
        let generation = self.open_generation;
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("New symbol name"));
        let focus = input.read(cx).focus_handle(cx);
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let submit = input.clone();
            let owner = owner.clone();
            let source = source.clone();
            let path = path.clone();
            let position = position.clone();
            dialog
                .title("Rename symbol")
                .w(px(420.))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Preview changes")
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .child(Input::new(&input))
                .on_ok(move |_, window, cx| {
                    let name = submit.read(cx).value().trim().to_string();
                    if name.is_empty() {
                        return false;
                    }
                    let _ = owner.update(cx, |p, cx| {
                        if p.open_generation != generation || p.editor.read(cx).text() != &source {
                            window.push_notification(
                                "The document changed. Request rename again.",
                                cx,
                            );
                            return;
                        }
                        let payload = LspRenameInput {
                            cwd: tnes(&p.cwd),
                            relative_path: tnes(&path),
                            position: position.clone(),
                            new_name: tnes(name.clone()),
                        };
                        let client = p.client.clone();
                        let flush = p.lsp.flush_document(&source, cx);
                        let source = source.clone();
                        cx.spawn_in(window, async move |this, cx| {
                            flush.await;
                            let result = client.call::<LspRename>(&payload).await;
                            let _ = this.update_in(cx, |p, window, cx| match result {
                                Ok(result)
                                    if p.open_generation == generation
                                        && p.editor.read(cx).text() == &source =>
                                {
                                    p.prepare_refactor(
                                        format!("Rename to {name}"),
                                        result.files,
                                        window,
                                        cx,
                                    )
                                }
                                Ok(_) => window.push_notification(
                                    "The document changed. Request rename again.",
                                    cx,
                                ),
                                Err(error) => window.push_notification(error.user_message(), cx),
                            });
                        })
                        .detach();
                    });
                    true
                })
        });
        window.on_next_frame(move |window, cx| focus.focus(window, cx));
    }

    pub(super) fn show_code_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.refactor_ready(window, cx) {
            return;
        }
        let Some(path) = self.lsp.current_document() else {
            return;
        };
        let editor = self.editor.read(cx);
        let source = editor.text().clone();
        let selection = editor.selected_range();
        let generation = self.open_generation;
        let start = self
            .lsp
            .position_payload(&path, positions::offset_to_wire(&source, selection.start))
            .position;
        let end = self
            .lsp
            .position_payload(&path, positions::offset_to_wire(&source, selection.end))
            .position;
        let payload = LspCodeActionsInput {
            cwd: tnes(&self.cwd),
            relative_path: tnes(&path),
            range: vitre_contracts::LspRange { start, end },
        };
        let client = self.client.clone();
        let flush = self.lsp.flush_document(&source, cx);
        cx.spawn_in(window, async move |this, cx| {
            flush.await;
            let result = client.call::<LspCodeActions>(&payload).await;
            let _ = this.update_in(cx, |p, window, cx| {
                if p.open_generation != generation || p.editor.read(cx).text() != &source { return; }
                match result {
                    Ok(mut result) if !result.actions.is_empty() => {
                        result.actions.sort_by_key(|a| (
                            a.disabled_reason.as_ref().and_then(|v| v.as_ref()).is_some(),
                            !a.preferred,
                            !a.kind.as_ref().and_then(|v| v.as_ref()).is_some_and(|kind| kind.0.starts_with("quickfix")),
                        ));
                        let owner = cx.weak_entity();
                        window.open_dialog(cx, move |dialog, _, cx| {
                            dialog.title("Refactor / Quick Fix").w(px(600.)).child(
                                v_flex().id("code-actions").max_h(px(440.)).overflow_y_scrollbar().gap_1()
                                    .children(result.actions.iter().enumerate().map(|(i, action)| {
                                        let action = action.clone();
                                        let owner = owner.clone();
                                        let source = source.clone();
                                        let path = path.clone();
                                        let disabled = action.disabled_reason.clone().flatten();
                                        v_flex().gap_1().child(Button::new(("code-action", i)).ghost().justify_start().label(action.title.0.clone()).disabled(disabled.is_some()).on_click(move |_, window, cx| {
                                            window.close_dialog(cx);
                                            let _ = owner.update(cx, |p, cx| {
                                                if p.open_generation == generation && p.editor.read(cx).text() == &source {
                                                    p.resolve_code_action(&path, action.clone(), window, cx);
                                                } else { window.push_notification("The document changed. Request actions again.", cx); }
                                            });
                                        })).children(disabled.map(|reason| div().px_3().text_xs().text_color(cx.theme().muted_foreground).child(reason.0)))
                                    }))
                            )
                        });
                    }
                    Ok(_) => window.push_notification("No code actions at this selection.", cx),
                    Err(error) => window.push_notification(error.user_message(), cx),
                }
            });
        }).detach();
    }

    fn resolve_code_action(
        &mut self,
        path: &str,
        action: LspCodeAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let payload = LspResolveCodeActionInput {
            cwd: tnes(&self.cwd),
            relative_path: tnes(path),
            resolve_data: action.resolve_data,
        };
        let client = self.client.clone();
        let generation = self.open_generation;
        let source = self.editor.read(cx).text().clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = client.call::<LspResolveCodeAction>(&payload).await;
            let _ = this.update_in(cx, |p, window, cx| {
                if p.open_generation != generation || p.editor.read(cx).text() != &source {
                    return;
                }
                match result {
                    Ok(action) => {
                        if let Some(reason) = action.disabled_reason.flatten() {
                            window.push_notification(reason.0, cx);
                        } else {
                            p.prepare_refactor(action.title.0, action.files, window, cx);
                        }
                    }
                    Err(error) => window.push_notification(error.user_message(), cx),
                }
            });
        })
        .detach();
    }

    fn prepare_refactor(
        &mut self,
        title: String,
        files: Vec<LspFileEdits>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let client = self.client.clone();
        let cwd = self.cwd.clone();
        let source = self.editor.read(cx).text().clone();
        let generation = self.open_generation;
        let source_path = self.open.as_ref().map(|file| file.relative_path.clone());
        cx.spawn_in(window, async move |this, cx| {
            let result: anyhow::Result<Vec<PreparedFile>> = async {
                anyhow::ensure!(!files.is_empty(), "No previewable text edits were returned");
                anyhow::ensure!(
                    files.len() <= 128,
                    "Refactor exceeds the safety limit of 128 files"
                );
                // Merge duplicate file entries before checking overlap.
                let mut grouped = std::collections::BTreeMap::<String, Vec<LspTextEdit>>::new();
                for file in files {
                    grouped
                        .entry(file.relative_path.0)
                        .or_default()
                        .extend(file.edits);
                }
                let mut prepared = Vec::new();
                for (path, edits) in grouped {
                    let file = client
                        .call::<ProjectsReadFile>(&ProjectReadFileInput {
                            cwd: tnes(&cwd),
                            relative_path: tnes(&path),
                        })
                        .await?;
                    anyhow::ensure!(
                        !file.truncated,
                        "{path}: file is too large for a safe refactor"
                    );
                    if source_path.as_deref() == Some(&path) {
                        anyhow::ensure!(
                            source == file.contents.0,
                            "{path} changed on disk. Reload it before refactoring"
                        );
                    }
                    let revision = file
                        .revision
                        .flatten()
                        .ok_or_else(|| anyhow::anyhow!("{path}: no revision guard available"))?;
                    let after = apply_edits(&file.contents.0, &edits)?;
                    if after != file.contents.0 {
                        prepared.push(PreparedFile {
                            path,
                            before: file.contents.0,
                            after,
                            revision,
                        });
                    }
                }
                anyhow::ensure!(!prepared.is_empty(), "No changes to apply");
                Ok(prepared)
            }
            .await;
            let _ = this.update_in(cx, |p, window, cx| {
                if p.open_generation != generation || p.editor.read(cx).text() != &source {
                    window
                        .push_notification("The document changed. Request the refactor again.", cx);
                    return;
                }
                match result {
                    Ok(files) => p.preview_refactor(title, files, window, cx),
                    Err(error) => window.push_notification(error.to_string(), cx),
                }
            });
        })
        .detach();
    }

    fn preview_refactor(
        &mut self,
        title: String,
        files: Vec<PreparedFile>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previews = files
            .iter()
            .map(|file| {
                let editor = cx.new(|cx| {
                    EditorState::new(window, cx)
                        .language(language_for_path(&file.path))
                        .line_number(true)
                });
                editor.update(cx, |s, cx| s.set_value(file.after.clone(), window, cx));
                let before = cx.new(|cx| {
                    EditorState::new(window, cx)
                        .language(language_for_path(&file.path))
                        .line_number(true)
                });
                before.update(cx, |s, cx| s.set_value(file.before.clone(), window, cx));
                (file.path.clone(), before, editor)
            })
            .collect::<Vec<_>>();
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, cx| {
            let owner = owner.clone();
            let files = files.clone();
            dialog.title(title.clone()).w(px(820.))
                .button_props(DialogButtonProps::default().ok_text("Apply changes").cancel_text("Cancel").show_cancel(true))
                .child(v_flex().gap_3()
                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child(format!("Result preview · {} file(s). Changes are saved to disk with revision checks.", files.len())))
                    .child(v_flex().id("refactor-preview").max_h(px(480.)).overflow_y_scrollbar().gap_4().children(previews.iter().map(|(path, before, editor)| {
                        v_flex().gap_2().child(div().text_sm().child(path.clone())).child(h_flex().h(px(240.)).gap_2().children([("Before", before), ("After", editor)].map(|(label, state)| {
                            v_flex().flex_1().min_w_0().h_full().gap_1().child(div().text_xs().child(label)).child(div().flex_1().min_h_0().border_1().border_color(cx.theme().border).rounded_md().overflow_hidden().child(Editor::new(state).readonly(true).size_full()))
                        })))
                    }))))
                .on_ok(move |_, window, cx| {
                    owner.update(cx, |p, cx| {
                        if !p.refactor_ready(window, cx) { return false; }
                        if let Some(open) = &p.open && let Some(file) = files.iter().find(|f| f.path == open.relative_path) && *p.editor.read(cx).text() != file.before {
                            window.push_notification("The buffer changed. Request the refactor again.", cx); return false;
                        }
                        p.apply_refactor(files.clone(), window, cx);
                        true
                    }).unwrap_or(true)
                })
        });
    }

    fn apply_refactor(
        &mut self,
        files: Vec<PreparedFile>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refactor_busy = true;
        self.editor.update(cx, |s, cx| s.set_readonly(true, cx));
        cx.notify();
        let client = self.client.clone();
        let cwd = self.cwd.clone();
        cx.spawn_in(window, async move |this, cx| {
            let mut applied = 0usize;
            let result: anyhow::Result<()> = async {
                // Preflight ALL revisions before the first write. Each write
                // also checks its own revision against concurrent disk edits.
                for file in &files {
                    let current = client.call::<ProjectsReadFile>(&ProjectReadFileInput { cwd: tnes(&cwd), relative_path: tnes(&file.path) }).await?;
                    anyhow::ensure!(current.revision.flatten().as_ref() == Some(&file.revision), "{} changed since preview; no changes applied", file.path);
                }
                for file in &files {
                    client.call::<ProjectsWriteFile>(&ProjectWriteFileInput {
                        cwd: tnes(&cwd), relative_path: tnes(&file.path), contents: tnes(&file.after), base_revision: Some(Some(file.revision.clone())),
                    }).await?;
                    applied += 1;
                }
                Ok(())
            }.await;
            let _ = this.update_in(cx, |p, window, cx| {
                p.refactor_busy = false;
                let readonly = p.open.as_ref().is_some_and(|o| o.truncated);
                p.editor.update(cx, |s, cx| s.set_readonly(readonly, cx));
                p.check_open_file_disk(window, cx);
                match result {
                    Ok(()) => window.push_notification(format!("Applied changes to {applied} file(s)."), cx),
                    Err(error) => window.push_notification(format!("Refactor stopped after {applied}/{} files: {error}. Review changed files before retrying.", files.len()), cx),
                }
                cx.notify();
            });
        }).detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn edit(start: u32, end: u32, value: &str) -> LspTextEdit {
        serde_json::from_value(serde_json::json!({"range":{"start":{"line":0,"character":start},"end":{"line":0,"character":end}},"newText":value})).unwrap()
    }
    #[test]
    fn applies_unordered_unicode_edits_without_shifting_later_ranges() {
        assert_eq!(
            apply_edits("😀 foo foo", &[edit(7, 10, "bar"), edit(3, 6, "baz")]).unwrap(),
            "😀 baz bar"
        );
    }
    #[test]
    fn refuses_overlap_and_invalid_utf16_instead_of_clamping() {
        assert!(apply_edits("abcdef", &[edit(0, 3, "x"), edit(2, 4, "y")]).is_err());
        assert!(apply_edits("😀 x", &[edit(1, 2, "y")]).is_err());
        assert!(apply_edits("x", &[edit(5, 6, "y")]).is_err());
        assert!(apply_edits("abc", &[edit(2, 1, "y")]).is_err());
    }
}
