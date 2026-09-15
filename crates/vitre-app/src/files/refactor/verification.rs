use super::*;
use crate::chat::ui_verification::{capture, key};

impl FilesPanel {
    pub(crate) fn verify_refactor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<(), String>> {
        self.save_now(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                if this.update(cx, |p, _| p.open.as_ref().is_some_and(|f| !f.buffer.is_dirty())).map_err(|e| e.to_string())? { break; }
            }
            this.update_in(cx, |p, w, cx| p.open_file("src/language.ts".into(), w, cx)).map_err(|e|e.to_string())?;
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                if this.update(cx, |p, cx| p.lsp.current_document().as_deref() == Some("src/language.ts") && p.editor.read(cx).lsp().semantic_token_count() > 0).map_err(|e|e.to_string())? { break; }
            }
            eprintln!("[vitre-editor-test] native semantic-token render cache PASS");
            this.update_in(cx, |p, w, cx| {
                let offset = p.editor.read(cx).value().find("greet").unwrap();
                p.editor.update(cx, |s, cx| { s.set_selected_range(offset..offset, cx); s.focus(w, cx); });
            }).map_err(|e|e.to_string())?;
            key("f2", cx).await?;
            cx.background_executor().timer(Duration::from_millis(400)).await;
            if !cx.update(|w, cx| w.has_active_dialog(cx)).map_err(|e|e.to_string())? { return Err("F2 did not open Rename".into()); }
            cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("welcome".into()))).map_err(|e|e.to_string())?;
            key("cmd-v", cx).await?;
            cx.update(|w, cx| w.dispatch_action(Box::new(gpui_component::dialog::Confirm { secondary: false }), cx)).map_err(|e|e.to_string())?;
            cx.background_executor().timer(Duration::from_millis(600)).await;
            loop {
                if cx.update(|w, cx| w.has_active_dialog(cx)).map_err(|e|e.to_string())? { break; }
                cx.background_executor().timer(Duration::from_millis(80)).await;
            }
            capture("editor-refactor-preview", cx).await?;
            cx.update(|w, cx| w.dispatch_action(Box::new(gpui_component::dialog::Confirm { secondary: false }), cx)).map_err(|e|e.to_string())?;
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                if this.update(cx, |p, cx| !p.refactor_busy && p.editor.read(cx).value().contains("function welcome(")).map_err(|e|e.to_string())? { break; }
            }
            let (client, cwd) = this.update(cx, |p, _| (p.client.clone(), p.cwd.clone())).map_err(|e|e.to_string())?;
            let importer = client.call::<ProjectsReadFile>(&ProjectReadFileInput { cwd: tnes(&cwd), relative_path: tnes("src/components/Workspace.tsx") }).await.map_err(|e|e.to_string())?;
            if !importer.contents.0.contains("welcome('World'") || importer.contents.0.contains("greet") { return Err(format!("Rename did not update importer: {}", importer.contents.0)); }
            eprintln!("[vitre-editor-test] F2 rename, before/after preview, guarded apply and cross-file importer update PASS");

            // Test the preflight against a concurrent writer on the second
            // file: even the first file must remain untouched.
            let mut prepared = vec![];
            for path in ["src/language.ts", "src/components/Workspace.tsx"] {
                let file = client.call::<ProjectsReadFile>(&ProjectReadFileInput { cwd: tnes(&cwd), relative_path: tnes(path) }).await.map_err(|e|e.to_string())?;
                prepared.push(PreparedFile { path: path.into(), before: file.contents.0.clone(), after: format!("// forbidden stale refactor\n{}", file.contents.0), revision: file.revision.flatten().unwrap() });
            }
            let second = &prepared[1];
            client.call::<ProjectsWriteFile>(&ProjectWriteFileInput { cwd: tnes(&cwd), relative_path: tnes(&second.path), contents: tnes(format!("// external edit\n{}", second.before)), base_revision: Some(Some(second.revision.clone())) }).await.map_err(|e|e.to_string())?;
            let original = prepared[0].before.clone();
            this.update_in(cx, |p, w, cx| p.apply_refactor(prepared, w, cx)).map_err(|e|e.to_string())?;
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                if !this.update(cx, |p, _| p.refactor_busy).map_err(|e|e.to_string())? { break; }
            }
            let file = client.call::<ProjectsReadFile>(&ProjectReadFileInput { cwd: tnes(&cwd), relative_path: tnes("src/language.ts") }).await.map_err(|e|e.to_string())?;
            if file.contents.0 != original { return Err("Stale multi-file refactor wrote its first file before preflight".into()); }
            eprintln!("[vitre-editor-test] concurrent disk change rejects entire refactor preflight PASS");

            // Exercise real UI snippets and multi-cursor chords without
            // changing the saved fixture: restore with the normal undo path.
            this.update_in(cx, |p, w, cx| {
                p.editor.update(cx, |s, cx| {
                    let end = s.text().len(); s.set_selected_range(end..end, cx);
                    s.insert_snippet("\n<${1:div}>${2:content}</$1>$0", w, cx).unwrap(); s.focus(w, cx);
                });
            }).map_err(|e|e.to_string())?;
            capture("editor-snippet-linked", cx).await?;
            key("tab", cx).await?;
            let placeholder = this.update(cx, |p, cx| { let s = p.editor.read(cx); s.text().slice(s.selected_range()).to_string() }).map_err(|e|e.to_string())?;
            if placeholder != "content" { return Err(format!("Tab did not select next placeholder: {placeholder}")); }
            key("escape", cx).await?; key("cmd-z", cx).await?;
            this.update_in(cx, |p, w, cx| {
                let offset = p.editor.read(cx).value().find("name").unwrap();
                p.editor.update(cx, |s, cx| { s.set_selected_range(offset..offset+4, cx); s.focus(w, cx); });
            }).map_err(|e|e.to_string())?;
            key("cmd-d", cx).await?;
            let state = this.update(cx, |p, cx| { let s = p.editor.read(cx); (s.selection_count(), s.value().to_string(), s.selected_range()) }).map_err(|e|e.to_string())?;
            if state.0 != 2 { return Err(format!("Cmd-D did not add the next occurrence: {state:?}")); }
            capture("editor-multi-cursor", cx).await?;
            key("escape", cx).await?;
            key("cmd-alt-j", cx).await?;
            cx.background_executor().timer(Duration::from_millis(500)).await;
            if !cx.update(|w, cx| w.has_active_dialog(cx)).map_err(|e|e.to_string())? { return Err("Cmd-Option-J did not open the snippet picker".into()); }
            capture("editor-snippet-picker", cx).await?;
            cx.update(|w, cx| w.close_dialog(cx)).map_err(|e|e.to_string())?;
            eprintln!("[vitre-editor-test] linked snippet rendering, Tab, undo, Cmd-D and snippet picker PASS");
            this.update_in(cx, |p, w, cx| p.save_now(w, cx)).map_err(|e|e.to_string())?;
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                if this.update(cx, |p, _| p.open.as_ref().is_some_and(|f| !f.buffer.is_dirty())).map_err(|e|e.to_string())? { break; }
            }
            this.update_in(cx, |p, w, cx| p.open_file("src/quickfix.ts".into(), w, cx)).map_err(|e|e.to_string())?;
            eprintln!("[vitre-editor-test] waiting for quick-fix fixture diagnostics");
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                if this.update(cx, |p, _| p.lsp.current_document().as_deref() == Some("src/quickfix.ts")).map_err(|e|e.to_string())? { break; }
            }
            let flushed = this.update(cx, |p, cx| { let text = p.editor.read(cx).text().clone(); p.lsp.flush_document(&text, cx) }).map_err(|e|e.to_string())?;
            flushed.await;
            let hover_input = serde_json::from_value(serde_json::json!({"cwd":cwd,"relativePath":"src/quickfix.ts","position":{"line":0,"character":8}})).unwrap();
            let hover = client.call::<vitre_contracts::methods::LspHover>(&hover_input).await.map_err(|e|e.to_string())?;
            if !hover.is_some_and(|result| result.contents.0.contains("score")) { return Err("Language server lost the document after multiline snippet insertion/undo".into()); }
            let started = std::time::Instant::now();
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                if this.update(cx, |p, _| p.lsp.current_document().as_deref() == Some("src/quickfix.ts") && p.diagnostics.get("src/quickfix.ts").is_some_and(|ds| !ds.is_empty())).map_err(|e|e.to_string())? { break; }
                if started.elapsed() > Duration::from_secs(15) {
                    let status = this.update(cx, |p, _| format!("doc={:?}, open={:?}, status={:?}, error={:?}, diagnostic paths={:?}", p.lsp.current_document(), p.open.as_ref().map(|f| (&f.relative_path, f.buffer.is_dirty())), p.status, p.open_error.as_ref().map(|e| &e.relative_path), p.diagnostics.keys())).map_err(|e|e.to_string())?;
                    return Err(format!("Quick-fix fixture did not receive diagnostics: {status}"));
                }
            }
            this.update_in(cx, |p, w, cx| {
                p.editor.update(cx, |s, cx| { s.set_selected_range(17..17, cx); s.focus(w, cx); });
                p.show_code_actions(w, cx);
            }).map_err(|e|e.to_string())?;
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                if cx.update(|w, cx| w.has_active_dialog(cx)).map_err(|e|e.to_string())? { break; }
            }
            capture("editor-code-actions", cx).await?;
            cx.update(|w, cx| w.close_dialog(cx)).map_err(|e|e.to_string())?;
            let payload = serde_json::from_value(serde_json::json!({"cwd":cwd,"relativePath":"src/quickfix.ts","range":{"start":{"line":1,"character":0},"end":{"line":1,"character":5}}})).unwrap();
            let actions = client.call::<LspCodeActions>(&payload).await.map_err(|e|e.to_string())?;
            let action = actions.actions.into_iter().find(|a| a.title.0.contains("const") && a.disabled_reason.clone().flatten().is_none()).ok_or("No usable const-to-let quick fix")?;
            this.update_in(cx, |p, w, cx| p.resolve_code_action("src/quickfix.ts", action, w, cx)).map_err(|e|e.to_string())?;
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                if cx.update(|w, cx| w.has_active_dialog(cx)).map_err(|e|e.to_string())? { break; }
            }
            cx.update(|w, cx| w.dispatch_action(Box::new(gpui_component::dialog::Confirm { secondary: false }), cx)).map_err(|e|e.to_string())?;
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                if this.update(cx, |p, cx| !p.refactor_busy && p.editor.read(cx).value().starts_with("let score")).map_err(|e|e.to_string())? { break; }
            }
            eprintln!("[vitre-editor-test] native code-action picker, const-to-let resolution, preview and apply PASS");
            Ok(())
        })
    }
}
