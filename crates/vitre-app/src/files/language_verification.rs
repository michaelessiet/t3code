//! Disposable native React/LSP verification (debug builds only).
use super::*;
use gpui::Task;
use gpui_component::{WindowExt as _, input::CompletionProvider as _};
use lsp_types::{CompletionContext, CompletionResponse, CompletionTriggerKind};

impl FilesPanel {
    pub(crate) fn verification_language_fixtures() -> &'static [(&'static str, &'static str)] {
        &[
            (
                "src/quickfix.ts",
                "const score = 1;\nscore = 2;\nexport {};\n",
            ),
            (
                "tsconfig.json",
                "{\"compilerOptions\":{\"strict\":true,\"jsx\":\"preserve\",\"allowJs\":true,\"checkJs\":true,\"target\":\"ES2022\",\"moduleResolution\":\"node\"}}\n",
            ),
            (
                "src/jsx.d.ts",
                "declare namespace JSX { interface Element {} interface IntrinsicElements { main: {className?: string}; h1: {}; button: {}; } }\n",
            ),
            (
                "src/language.ts",
                "export function greet(name: string, count: number): string {\n  return name.repeat(count);\n}\n",
            ),
            (
                "src/components/Button.tsx",
                "export function Button(props: { label: string; disabled?: boolean }) {\n  return <button>{props.label}</button>;\n}\n",
            ),
            (
                "src/components/Workspace.tsx",
                "import { Button } from './Button';\nimport { greet } from '../language';\n\n// React language-service fixture\nexport function Workspace() {\n  const description = 'Ready';\n  const title = greet('World', 2);\n  return <main className=\"workspace\">\n    <h1>{description}{title}</h1>\n    <Button label={42} />\n  </main>;\n}\n",
            ),
            (
                "src/components/Playground.jsx",
                "import { Button } from './Button';\nexport function Playground() {\n  return <main><Button label=\"Ready\" /></main>;\n}\n",
            ),
        ]
    }

    pub(crate) fn verify_languages(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        self.editor.update(cx, |state, cx| state.focus(window, cx));
        cx.spawn_in(window, async move |this, cx| {
            let executor = cx.background_executor().clone();
            let run = async {
                for (path, prop_marker) in [("src/components/Workspace.tsx", "<Button "), ("src/components/Playground.jsx", "<Button ")] {
                    this.update_in(cx, |panel, window, cx| panel.open_file(path.into(), window, cx)).map_err(|e| e.to_string())?;
                    loop {
                        cx.background_executor().timer(Duration::from_millis(80)).await;
                        if this.update(cx, |panel, _| panel.lsp.current_document().as_deref() == Some(path)).map_err(|e| e.to_string())? { break; }
                    }
                    let task = this.update_in(cx, |panel, window, cx| {
                        let text = panel.editor.read(cx).text().clone();
                        let offset = text.to_string().find(prop_marker).unwrap() + prop_marker.len();
                        panel.lsp.completions(&text, offset, CompletionContext { trigger_kind: CompletionTriggerKind::INVOKED, trigger_character: None }, window, cx)
                    }).map_err(|e| e.to_string())?;
                    let result = task.await.map_err(|e| e.to_string())?;
                    let items = match result { CompletionResponse::Array(items) => items, CompletionResponse::List(list) => list.items };
                    for label in ["label", "disabled"] {
                        if !items.iter().any(|item| item.label.trim_end_matches('?') == label) { return Err(format!("{path}: missing JSX prop {label}, got {:?}", items.iter().map(|item| &item.label).collect::<Vec<_>>())); }
                    }
                    eprintln!("[vitre-language-test] {path}: typed React prop completion PASS");
                    if path.ends_with("Workspace.tsx") {
                        this.update_in(cx, |p, window, cx| {
                            let offset = p.editor.read(cx).text().to_string().find(prop_marker).unwrap() + prop_marker.len();
                            p.editor.update(cx, |s, cx| { s.set_selected_range(offset..offset, cx); s.focus(window, cx); });
                        }).map_err(|e| e.to_string())?;
                        crate::chat::ui_verification::key("ctrl-space", cx).await?;
                        loop {
                            cx.background_executor().timer(Duration::from_millis(80)).await;
                            if this.update(cx, |p, cx| p.editor.read(cx).completion_menu_state().open).map_err(|e| e.to_string())? { break; }
                        }
                        crate::chat::ui_verification::capture("languages-completion", cx).await?;
                        crate::chat::ui_verification::key("escape", cx).await?;
                    }
                }
                this.update_in(cx, |panel, window, cx| panel.open_file("src/components/Workspace.tsx".into(), window, cx)).map_err(|e| e.to_string())?;
                loop {
                    cx.background_executor().timer(Duration::from_millis(80)).await;
                    if this.update(cx, |p, _| p.lsp.current_document().as_deref() == Some("src/components/Workspace.tsx")).map_err(|e| e.to_string())? { break; }
                }
                // Ensure the streaming diagnostic reaches the actual native buffer.
                loop {
                    let received = this.update(cx, |p, _| p.diagnostics.get("src/components/Workspace.tsx")
                        .is_some_and(|items| items.iter().any(|d| d.message.0.contains("not assignable")))).map_err(|e| e.to_string())?;
                    if received { break; }
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                }
                eprintln!("[vitre-language-test] TSX prop type-error streaming diagnostics PASS");
                let task = this.update_in(cx, |p, window, cx| {
                    let text = p.editor.read(cx).text().clone();
                    let offset = text.to_string().find("greet('").unwrap();
                    p.lsp.definitions(&text, offset, window, cx)
                }).map_err(|e| e.to_string())?;
                let definitions = task.await.map_err(|e| e.to_string())?;
                if !definitions.iter().any(|d| d.target_uri.as_str().ends_with("/src/language.ts")) { return Err("Cross-file definition did not reach language.ts".into()); }
                this.update_in(cx, |p, window, cx| {
                    let offset = p.editor.read(cx).text().to_string().find("greet('").unwrap() + "greet(".len();
                    p.editor.update(cx, |state, cx| { state.set_selected_range(offset..offset, cx); state.focus(window, cx); });
                }).map_err(|e| e.to_string())?;
                crate::chat::ui_verification::key(if cfg!(target_os = "macos") { "cmd-shift-space" } else { "ctrl-shift-space" }, cx).await?;
                loop {
                    cx.background_executor().timer(Duration::from_millis(80)).await;
                    if this.update(cx, |p, _| p.verification_signature().is_some_and(|s| s.contains("Parameter 1"))).map_err(|e| e.to_string())? { break; }
                }
                eprintln!("[vitre-language-test] cross-file definition and rendered signature help PASS");
                crate::chat::ui_verification::capture("languages-signature", cx).await?;
                if !this.update(cx, |p, _| p.verification_signature().is_some()).map_err(|e| e.to_string())? { return Err("Mouse movement dismissed parameter hints".into()); }
                crate::chat::ui_verification::key("escape", cx).await?;
                if this.update(cx, |p, _| p.verification_signature().is_some()).map_err(|e| e.to_string())? { return Err("Escape did not dismiss parameter hints".into()); }
                this.update_in(cx, |p, _, cx| {
                    let offset = p.editor.read(cx).text().to_string().find("greet('").unwrap();
                    p.editor.update(cx, |state, cx| state.set_selected_range(offset..offset, cx));
                }).map_err(|e| e.to_string())?;
                crate::chat::ui_verification::key("shift-f12", cx).await?;
                loop {
                    cx.background_executor().timer(Duration::from_millis(80)).await;
                    if cx.update(|window, cx| window.has_active_dialog(cx)).map_err(|e| e.to_string())? { break; }
                }
                crate::chat::ui_verification::capture("languages-references", cx).await?;
                cx.update(|window, cx| window.close_dialog(cx)).map_err(|e| e.to_string())?;
                let hover = this.update_in(cx, |p, window, cx| {
                    let text = p.editor.read(cx).text().to_string();
                    let value = text.find("'Ready'").unwrap();
                    p.editor.update(cx, |state, cx| state.edit_range(value..value+7, "99", window, cx));
                    let text = p.editor.read(cx).text().clone();
                    let offset = text.to_string().find("description").unwrap();
                    p.lsp.hover(&text, offset, window, cx)
                }).map_err(|e| e.to_string())?;
                let hover = hover.await.map_err(|e| e.to_string())?.ok_or("No hover after unsaved edit")?;
                if !format!("{:?}", hover.contents).contains("99") { return Err(format!("Hover used stale text: {:?}", hover.contents)); }
                this.update_in(cx, |p, window, cx| {
                    p.editor.update(cx, |s, cx| s.edit_range(0..0, "const formattingProbe={a:1,b:2};\n", window, cx));
                    p.format_document(window, cx);
                    // Change the document before the queued formatting response.
                    p.editor.update(cx, |s, cx| s.edit_range(0..0, "// keep this newer edit\n", window, cx));
                }).map_err(|e| e.to_string())?;
                loop {
                    cx.background_executor().timer(Duration::from_millis(80)).await;
                    if this.update(cx, |p, _| p.status.as_ref().is_some_and(|s| s.contains("Formatting cancelled"))).map_err(|e| e.to_string())? { break; }
                }
                this.update_in(cx, |p, window, cx| p.format_document(window, cx)).map_err(|e| e.to_string())?;
                loop {
                    cx.background_executor().timer(Duration::from_millis(80)).await;
                    if this.update(cx, |p, cx| p.editor.read(cx).text().to_string().contains("formattingProbe = { a: 1, b: 2 }")).map_err(|e| e.to_string())? { break; }
                }
                if !this.update(cx, |p, cx| p.editor.read(cx).text().to_string().starts_with("// keep this newer edit")).map_err(|e| e.to_string())? { return Err("Formatting lost a newer edit".into()); }
                eprintln!("[vitre-language-test] PASS: TSX/JSX React prop completions, live diagnostics, cross-file definitions, rendered parameter hints, references dialog, unsaved-buffer hover, formatting and stale-format protection");
                Ok(())
            };
            tokio::select! { result = run => result, _ = executor.timer(Duration::from_secs(70)) => Err("Native language verification timed out".into()) }
        })
    }
}
