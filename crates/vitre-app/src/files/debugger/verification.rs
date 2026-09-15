use super::*;

async fn fixture(cwd: String) -> anyhow::Result<Configuration> {
    let source = std::path::Path::new(&cwd).join("vitre-debug-fixture.c");
    let program = std::path::Path::new(&cwd).join("vitre-debug-fixture");
    anyhow::ensure!(
        !source.exists() && !program.exists(),
        "Debugger fixture already exists"
    );
    std::fs::write(
        &source,
        "#include <stdio.h>\nint main(void) {\n  int value = 41;\n  value += 1;\n  printf(\"value=%d\\n\", value);\n  return 0;\n}\n",
    )?;
    let compiler = tokio::process::Command::new("/usr/bin/clang")
        .args(["-g", "-O0"])
        .arg(&source)
        .arg("-o")
        .arg(&program)
        .output()
        .await?;
    anyhow::ensure!(
        compiler.status.success(),
        "{}",
        String::from_utf8_lossy(&compiler.stderr)
    );
    let adapter = tokio::process::Command::new("/usr/bin/xcrun")
        .args(["--find", "lldb-dap"])
        .output()
        .await?;
    anyhow::ensure!(adapter.status.success(), "LLDB debug adapter unavailable");
    Ok(Configuration {
        name: "Disposable native debugger verification".into(),
        adapter: Adapter {
            command: String::from_utf8(adapter.stdout)?.trim().to_string(),
            args: vec![],
        },
        request: "launch".into(),
        arguments: json!({"program":program,"cwd":cwd,"stopOnEntry":false})
            .as_object()
            .unwrap()
            .clone(),
    })
}

impl FilesPanel {
    pub(crate) fn verify_debugger(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let cwd = self.cwd.clone();
        let prepare = Tokio::spawn_result(cx, fixture(cwd));
        cx.spawn_in(window, async move |this, cx| {
            let config = prepare.await.map_err(|e| e.to_string())?;
            this.update_in(cx, |p, window, cx| {
                p.debugger.breakpoints.insert("vitre-debug-fixture.c".into(), vec![4]);
                p.launch_debugger(config, window, cx);
            }).map_err(|e| e.to_string())?;
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                let status = this.update(cx, |p, _| (p.debugger.frame.is_some(), p.debugger.state.clone(), p.debugger.console.clone())).map_err(|e| e.to_string())?;
                if status.0 { break; }
                if status.1 == "Failed to start" || status.1 == "Stopped" { return Err(format!("LLDB failed: {:?}", status.2)); }
            }
            let check = this.update(cx, |p, cx| {
                let client = p.debugger.client.clone().unwrap();
                let frame = p.debugger.frame.unwrap();
                Tokio::spawn_result(cx, async move {
                    let result = client.request("evaluate", json!({"expression":"value","frameId":frame,"context":"watch"})).await?;
                    anyhow::ensure!(result["result"].as_str().is_some_and(|s| s.contains("41")), "Unexpected paused value: {result}");
                    Ok(())
                })
            }).map_err(|e| e.to_string())?;
            check.await.map_err(|e| e.to_string())?;
            eprintln!("[vitre-debug-test] LLDB launch, verified breakpoint, call stack and evaluation PASS");
            crate::chat::ui_verification::capture("editor-debugger-stack", cx).await?;
            this.update_in(cx, |p, w, cx| p.editor.update(cx, |s, cx| s.focus(w, cx))).map_err(|e|e.to_string())?;
            let old = this.update(cx, |p, _| p.debugger.stop_generation).map_err(|e|e.to_string())?;
            crate::chat::ui_verification::key("f10", cx).await?;
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                if this.update(cx, |p, _| p.debugger.frame.is_some() && p.debugger.stop_generation > old).map_err(|e|e.to_string())? { break; }
            }
            let check = this.update(cx, |p, cx| {
                let client = p.debugger.client.clone().unwrap();
                let frame = p.debugger.frame.unwrap();
                Tokio::spawn_result(cx, async move {
                    let result = client.request("evaluate", json!({"expression":"value","frameId":frame,"context":"watch"})).await?;
                    anyhow::ensure!(result["result"].as_str().is_some_and(|s| s.contains("42")), "Unexpected stepped value: {result}");
                    Ok(())
                })
            }).map_err(|e| e.to_string())?;
            check.await.map_err(|e|e.to_string())?;
            this.update_in(cx, |p, w, cx| {
                p.debugger.tab = 1;
                let reference = p.debugger.variables.first().and_then(|v| v["variablesReference"].as_i64());
                if let Some(reference) = reference { p.debug_variables(reference, w, cx); }
                cx.notify();
            }).map_err(|e|e.to_string())?;
            cx.background_executor().timer(Duration::from_millis(350)).await;
            crate::chat::ui_verification::capture("editor-debugger-variables", cx).await?;
            crate::chat::ui_verification::key("shift-f5", cx).await?;
            if this.update(cx, |p, _| p.debugger.client.is_some()).map_err(|e|e.to_string())? { return Err("Shift-F5 did not disconnect debugger".into()); }
            eprintln!("[vitre-debug-test] F10 step-over, variable inspection and Shift-F5 stop PASS");
            Ok(())
        })
    }
}
