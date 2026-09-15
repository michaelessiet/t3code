//! Local DAP debugging, shared by the toolbar and conventional F-keys.
mod dap;
#[cfg(debug_assertions)]
mod verification;
use super::*;
use gpui::{KeyBinding, Task};
use gpui_component::{Disableable as _, WindowExt as _, dialog::DialogButtonProps};
use gpui_tokio::Tokio;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Adapter {
    command: String,
    #[serde(default)]
    args: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Configuration {
    name: String,
    adapter: Adapter,
    #[serde(default = "launch_request")]
    request: String,
    #[serde(flatten)]
    arguments: serde_json::Map<String, Value>,
}
fn launch_request() -> String {
    "launch".into()
}

fn expand(value: &mut Value, cwd: &str) {
    match value {
        Value::String(s) => *s = s.replace("${workspaceFolder}", cwd),
        Value::Array(values) => {
            for value in values {
                expand(value, cwd);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                expand(value, cwd);
            }
        }
        _ => {}
    }
}

pub(super) struct Debugger {
    client: Option<dap::Client>,
    pub(super) visible: bool,
    state: String,
    generation: u64,
    stop_generation: u64,
    thread: Option<i64>,
    frame: Option<i64>,
    attached: bool,
    frames: Vec<Value>,
    variables: Vec<Value>,
    variable_history: Vec<Vec<Value>>,
    console: VecDeque<String>,
    breakpoints: BTreeMap<String, Vec<u32>>,
    verified_breakpoints: BTreeMap<String, Vec<u32>>,
    tab: usize,
    expression: Entity<InputState>,
    events: Task<()>,
}

impl Debugger {
    pub(super) fn new(window: &mut Window, cx: &mut Context<FilesPanel>) -> Self {
        let expression = cx.new(|cx| {
            let mut input =
                InputState::new(window, cx).placeholder("Evaluate expression while paused…");
            let mut context = gpui::KeyContext::default();
            context.add("DebugConsole");
            input.set_extra_key_context(Some(context), cx);
            input
        });
        Self {
            client: None,
            visible: false,
            state: "Not running".into(),
            generation: 0,
            stop_generation: 0,
            thread: None,
            frame: None,
            attached: false,
            frames: vec![],
            variables: vec![],
            variable_history: vec![],
            console: VecDeque::new(),
            breakpoints: BTreeMap::new(),
            verified_breakpoints: BTreeMap::new(),
            tab: 0,
            expression,
            events: Task::ready(()),
        }
    }
    fn log(&mut self, text: impl Into<String>) {
        let text = text.into();
        for line in text.lines() {
            self.console.push_back(line.chars().take(4096).collect());
            while self.console.len() > 500 {
                self.console.pop_front();
            }
        }
    }
}

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("f5", DebugStart, Some("Editor")),
        KeyBinding::new("shift-f5", DebugStop, Some("Editor")),
        KeyBinding::new("f6", DebugPause, Some("Editor")),
        KeyBinding::new("f9", ToggleBreakpoint, Some("Editor")),
        KeyBinding::new("f10", DebugNext, Some("Editor")),
        KeyBinding::new("f11", DebugStepIn, Some("Editor")),
        KeyBinding::new("shift-f11", DebugStepOut, Some("Editor")),
        KeyBinding::new("enter", DebugEvaluate, Some("DebugConsole")),
    ]);
}

async fn start_session(
    config: Configuration,
    cwd: String,
    breakpoints: BTreeMap<String, Vec<u32>>,
) -> anyhow::Result<(
    dap::Client,
    tokio::sync::broadcast::Receiver<Value>,
    BTreeMap<String, Vec<u32>>,
)> {
    anyhow::ensure!(
        config.request == "launch" || config.request == "attach",
        "Only launch and attach requests are supported"
    );
    anyhow::ensure!(
        std::path::Path::new(&cwd).is_dir(),
        "Debugging currently requires a local workspace"
    );
    let client = dap::Client::spawn(&config.adapter.command, &config.adapter.args, &cwd).await?;
    let mut handshake = client.subscribe();
    let events = client.subscribe();
    let capabilities = client.request("initialize", json!({"clientID":"vitre","clientName":"Vitre","adapterID":"vitre-configured","pathFormat":"path","linesStartAt1":true,"columnsStartAt1":true,"supportsVariableType":true,"supportsRunInTerminalRequest":false})).await?;
    let mut arguments = Value::Object(config.arguments);
    expand(&mut arguments, &cwd);
    let args = arguments.as_object_mut().unwrap();
    args.entry("cwd").or_insert(json!(cwd));
    // Launch responses may arrive only AFTER configurationDone. Keep the
    // request in flight while processing the initialized event.
    let launch_client = client.clone();
    let launch =
        tokio::spawn(async move { launch_client.request(&config.request, arguments).await });
    let configure: anyhow::Result<_> = async {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let event = handshake.recv().await?;
                if event["event"] == "initialized" {
                    return Ok::<_, anyhow::Error>(());
                }
            }
        })
        .await??;
        let mut verified = BTreeMap::new();
        for (path, lines) in breakpoints {
            let response = client
                .request("setBreakpoints", breakpoint_args(&cwd, &path, &lines))
                .await?;
            verified.insert(path, verified_lines(&response));
        }
        if capabilities["supportsConfigurationDoneRequest"] == true {
            client.request("configurationDone", json!({})).await?;
        }
        Ok(verified)
    }
    .await;
    let verified = match configure {
        Ok(v) => v,
        Err(e) => {
            launch.abort();
            return Err(e);
        }
    };
    launch.await??;
    Ok((client, events, verified))
}

fn breakpoint_args(cwd: &str, path: &str, lines: &[u32]) -> Value {
    json!({"source":{"path":std::path::Path::new(cwd).join(path)},"breakpoints":lines.iter().map(|line| json!({"line":line})).collect::<Vec<_>>(),"sourceModified":false})
}
fn verified_lines(response: &Value) -> Vec<u32> {
    response["breakpoints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|bp| bp["verified"] == true)
        .filter_map(|bp| bp["line"].as_u64().and_then(|v| u32::try_from(v).ok()))
        .collect()
}

impl FilesPanel {
    pub(super) fn debug_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.debugger.visible = true;
        if self.debugger.client.is_some() {
            if self.debugger.thread.is_some() {
                self.debug_command("continue", window, cx);
            }
            return;
        }
        if self.debugger.state == "Starting…" {
            return;
        }
        let cwd = self.cwd.clone();
        let load = cx.background_spawn(async move {
            let path = std::path::Path::new(&cwd).join(".vitre/launch.json");
            match std::fs::read_to_string(path) {
                Ok(text) => {
                    anyhow::ensure!(
                        text.len() <= 1024 * 1024,
                        "Debug configuration exceeds 1 MiB"
                    );
                    #[derive(Deserialize)]
                    struct File {
                        configurations: Vec<Configuration>,
                    }
                    Ok(serde_json::from_str::<File>(&text)?.configurations)
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
                Err(e) => Err(anyhow::Error::from(e)),
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let configs: anyhow::Result<Vec<Configuration>> = load.await;
            let _ = this.update_in(cx, |p, window, cx| match configs {
                Ok(configs) if !configs.is_empty() => {
                    let owner = cx.weak_entity();
                    window.open_dialog(cx, move |dialog, _, _| {
                        dialog.title("Start debugging").w(px(620.)).child(
                            v_flex()
                                .gap_3()
                                .child("Only run debug configurations from workspaces you trust.")
                                .children(configs.iter().enumerate().map(|(i, config)| {
                                    let config = config.clone();
                                    let owner = owner.clone();
                                    let description = format!(
                                        "{} {} · {} · {}",
                                        config.adapter.command,
                                        config.adapter.args.join(" "),
                                        config.request,
                                        config
                                            .arguments
                                            .get("program")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("attach target")
                                    );
                                    v_flex()
                                        .gap_1()
                                        .child(
                                            Button::new(("launch-config", i))
                                                .label(config.name.clone())
                                                .on_click(move |_, window, cx| {
                                                    window.close_dialog(cx);
                                                    let _ = owner.update(cx, |p, cx| {
                                                        p.launch_debugger(
                                                            config.clone(),
                                                            window,
                                                            cx,
                                                        )
                                                    });
                                                }),
                                        )
                                        .child(div().text_xs().child(description))
                                })),
                        )
                    });
                }
                Ok(_) => p.debug_launch_dialog(window, cx),
                Err(error) => {
                    window.push_notification(format!("Invalid .vitre/launch.json: {error}"), cx)
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn debug_launch_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let default_adapter = if cfg!(target_os = "macos") {
            "/Applications/Xcode.app/Contents/Developer/usr/bin/lldb-dap"
        } else {
            "lldb-dap"
        };
        let adapter = cx.new(|cx| InputState::new(window, cx).default_value(default_adapter));
        let program = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Absolute path to the compiled executable")
        });
        let owner = cx.weak_entity();
        let cwd = self.cwd.clone();
        window.open_dialog(cx, move |dialog, _, cx| {
            let owner = owner.clone(); let program_input = program.clone(); let adapter_input = adapter.clone(); let cwd = cwd.clone();
            dialog.title("Debug a local executable").w(px(600.))
                .button_props(DialogButtonProps::default().ok_text("Launch debugger").cancel_text("Cancel").show_cancel(true))
                .child(v_flex().gap_3().child("Debug adapter (stdio)").child(Input::new(&adapter)).child("Program").child(Input::new(&program))
                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Compile with debug symbols first. For Python, JavaScript or attach configurations, configure an installed adapter in .vitre/launch.json. Launching runs the program on this machine.")))
                .on_ok(move |_, window, cx| {
                    let adapter = adapter_input.read(cx).value().trim().to_string();
                    let program = program_input.read(cx).value().trim().to_string();
                    if adapter.is_empty() || program.is_empty() { return false; }
                    let arguments = json!({"program":program,"cwd":cwd,"stopOnEntry":true}).as_object().unwrap().clone();
                    let config = Configuration { name: "Local executable".into(), adapter: Adapter { command: adapter, args: vec![] }, request: "launch".into(), arguments };
                    let _ = owner.update(cx, |p, cx| p.launch_debugger(config, window, cx));
                    true
                })
        });
    }

    fn launch_debugger(
        &mut self,
        config: Configuration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.debugger.client.is_some() || self.debugger.state == "Starting…" {
            return;
        }
        self.debugger.generation += 1;
        let generation = self.debugger.generation;
        self.debugger.visible = true;
        self.debugger.state = "Starting…".into();
        self.debugger.attached = config.request == "attach";
        self.debugger.console.clear();
        self.debugger
            .log(format!("{} · {}", config.name, config.adapter.command));
        let cwd = self.cwd.clone();
        let breakpoints = self.debugger.breakpoints.clone();
        let launch = Tokio::spawn_result(cx, start_session(config, cwd, breakpoints));
        self.debugger.events = cx.spawn_in(window, async move |this, cx| {
            let (client, mut events, verified) = match launch.await {
                Ok(result) => result,
                Err(error) => {
                    let _ = this.update(cx, |p, cx| {
                        if p.debugger.generation == generation {
                            p.debugger.state = "Failed to start".into();
                            p.debugger.log(error.to_string());
                            p.debugger.tab = 2;
                            cx.notify();
                        }
                    });
                    return;
                }
            };
            let _ = this.update(cx, |p, cx| {
                if p.debugger.generation == generation {
                    p.debugger.client = Some(client);
                    p.debugger.verified_breakpoints = verified;
                    p.debugger.state = "Running".into();
                    cx.notify();
                }
            });
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        let _ = this.update(cx, |p, _| {
                            p.debugger
                                .log(format!("Debug output overflow: skipped {count} events"))
                        });
                        continue;
                    }
                    Err(_) => break,
                };
                let done = event["event"] == "vitreDisconnected" || event["event"] == "terminated";
                let _ = this.update_in(cx, |p, window, cx| {
                    if p.debugger.generation != generation {
                        return;
                    }
                    match event["event"].as_str() {
                        Some("stopped") => {
                            p.debugger.state = format!(
                                "Paused · {}",
                                event["body"]["reason"].as_str().unwrap_or("stopped")
                            );
                            p.debugger.thread = event["body"]["threadId"].as_i64();
                            p.debugger.stop_generation += 1;
                            p.debug_stack(window, cx);
                        }
                        Some("continued") => p.debug_running(cx),
                        Some("output") => p
                            .debugger
                            .log(event["body"]["output"].as_str().unwrap_or_default()),
                        Some("terminated" | "vitreDisconnected") => {
                            if let Some(message) = event["body"]["message"].as_str() {
                                p.debugger.log(message);
                            }
                            p.debugger.client = None;
                            p.debugger.thread = None;
                            p.debugger.frame = None;
                            p.debugger.frames.clear();
                            p.debugger.variables.clear();
                            p.debugger.state = "Stopped".into();
                            p.debugger.stop_generation += 1;
                            p.editor.update(cx, |s, cx| s.set_line_highlight(None, cx));
                        }
                        _ => {}
                    }
                    cx.notify();
                });
                if done {
                    break;
                }
            }
        });
        cx.notify();
    }

    fn debug_running(&mut self, cx: &mut Context<Self>) {
        self.debugger.thread = None;
        self.debugger.frame = None;
        self.debugger.frames.clear();
        self.debugger.variables.clear();
        self.debugger.state = "Running".into();
        self.debugger.stop_generation += 1;
        self.editor
            .update(cx, |s, cx| s.set_line_highlight(None, cx));
    }

    pub(super) fn debug_pause(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.debugger.thread.is_some() {
            return;
        }
        let Some(client) = self.debugger.client.clone() else {
            return;
        };
        let generation = self.debugger.generation;
        let task = Tokio::spawn_result(cx, async move {
            let threads = client.request("threads", json!({})).await?;
            let thread = threads["threads"][0]["id"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("No running thread available"))?;
            client.request("pause", json!({"threadId":thread})).await
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = task.await {
                let _ = this.update(cx, |p, cx| {
                    if p.debugger.generation == generation {
                        p.debugger.log(error.to_string());
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    pub(super) fn debug_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.debugger.generation += 1;
        self.debugger.events = Task::ready(());
        self.debug_running(cx);
        self.debugger.state = "Stopped".into();
        if let Some(client) = self.debugger.client.take() {
            let terminate = !self.debugger.attached;
            let stop = Tokio::spawn_result(cx, async move {
                tokio::time::timeout(
                    Duration::from_secs(3),
                    client.request("disconnect", json!({"terminateDebuggee":terminate})),
                )
                .await??;
                Ok(())
            });
            cx.spawn_in(window, async move |_, _| {
                let _ = stop.await;
            })
            .detach();
        }
        cx.notify();
    }

    pub(super) fn debug_command(
        &mut self,
        command: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.debugger.client.clone() else {
            return;
        };
        let Some(thread) = self.debugger.thread else {
            return;
        };
        let generation = self.debugger.generation;
        let stop_generation = self.debugger.stop_generation;
        let task = Tokio::spawn_result(cx, async move {
            client.request(command, json!({"threadId":thread})).await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |p, cx| {
                if p.debugger.generation != generation
                    || p.debugger.stop_generation != stop_generation
                {
                    return;
                }
                match result {
                    Ok(_) => p.debug_running(cx),
                    Err(e) => p.debugger.log(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn toggle_breakpoint(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let line =
            positions::offset_to_wire(self.editor.read(cx).text(), self.editor.read(cx).cursor())
                .line
                + 1;
        self.toggle_breakpoint_line(line, window, cx);
    }

    pub(super) fn sync_breakpoint_gutter(&self, cx: &mut Context<Self>) {
        let lines = self.open.as_ref().map(|open| {
            self.debugger
                .breakpoints
                .get(&open.relative_path)
                .into_iter()
                .flatten()
                .map(|line| {
                    let verified = self
                        .debugger
                        .verified_breakpoints
                        .get(&open.relative_path)
                        .is_some_and(|lines| lines.contains(line));
                    (line.saturating_sub(1) as usize, verified)
                })
                .collect()
        });
        self.editor.update(cx, |s, cx| s.set_breakpoints(lines, cx));
    }

    pub(super) fn toggle_breakpoint_line(
        &mut self,
        line: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = &self.open else {
            return;
        };
        let path = open.relative_path.clone();
        let lines = self.debugger.breakpoints.entry(path.clone()).or_default();
        if lines.contains(&line) {
            lines.retain(|l| *l != line);
        } else {
            lines.push(line);
            lines.sort_unstable();
        }
        self.debugger.visible = true;
        self.debugger.tab = 3;
        if let Some(client) = self.debugger.client.clone() {
            let arguments = breakpoint_args(&self.cwd, &path, lines);
            let generation = self.debugger.generation;
            let task = Tokio::spawn_result(cx, async move {
                client.request("setBreakpoints", arguments).await
            });
            cx.spawn_in(window, async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |p, cx| {
                    if p.debugger.generation != generation {
                        return;
                    }
                    match result {
                        Ok(response) => {
                            p.debugger
                                .verified_breakpoints
                                .insert(path, verified_lines(&response));
                        }
                        Err(e) => p.debugger.log(e.to_string()),
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn debug_stack(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.debugger.client.clone() else {
            return;
        };
        let thread = self.debugger.thread;
        let epoch = (self.debugger.generation, self.debugger.stop_generation);
        let task = Tokio::spawn_result(cx, async move {
            let thread = match thread {
                Some(id) => id,
                None => client.request("threads", json!({})).await?["threads"][0]["id"]
                    .as_i64()
                    .ok_or_else(|| anyhow::anyhow!("No stopped thread available"))?,
            };
            Ok((
                thread,
                client
                    .request(
                        "stackTrace",
                        json!({"threadId":thread,"startFrame":0,"levels":100}),
                    )
                    .await?,
            ))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |p, window, cx| {
                if (p.debugger.generation, p.debugger.stop_generation) != epoch {
                    return;
                }
                match result {
                    Ok((thread, response)) => {
                        p.debugger.thread = Some(thread);
                        p.debugger.frames = response["stackFrames"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default();
                        p.debugger.tab = 0;
                        if let Some(frame) = p.debugger.frames.first().cloned() {
                            p.debug_frame(frame, window, cx);
                        }
                    }
                    Err(e) => p.debugger.log(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn debug_frame(&mut self, frame: Value, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = frame["id"].as_i64() else {
            return;
        };
        self.debugger.frame = Some(id);
        self.debugger.variable_history.clear();
        if let Some(path) = frame["source"]["path"].as_str()
            && let Ok(relative) = std::path::Path::new(path).strip_prefix(&self.cwd)
        {
            let line = frame["line"].as_u64().unwrap_or(1).saturating_sub(1) as u32;
            self.reveal(
                relative.to_string_lossy().to_string(),
                Some(RevealTarget::at(WirePosition { line, character: 0 })),
                window,
                cx,
            );
        }
        let Some(client) = self.debugger.client.clone() else {
            return;
        };
        let epoch = (self.debugger.generation, self.debugger.stop_generation);
        let task = Tokio::spawn_result(cx, async move {
            client.request("scopes", json!({"frameId":id})).await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |p, cx| {
                if (p.debugger.generation, p.debugger.stop_generation) != epoch
                    || p.debugger.frame != Some(id)
                {
                    return;
                }
                match result {
                    Ok(response) => {
                        p.debugger.variables =
                            response["scopes"].as_array().cloned().unwrap_or_default()
                    }
                    Err(e) => p.debugger.log(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn debug_variables(&mut self, reference: i64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.debugger.client.clone() else {
            return;
        };
        let epoch = (
            self.debugger.generation,
            self.debugger.stop_generation,
            self.debugger.frame,
        );
        let task = Tokio::spawn_result(cx, async move {
            client
                .request("variables", json!({"variablesReference":reference}))
                .await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |p, cx| {
                if (
                    p.debugger.generation,
                    p.debugger.stop_generation,
                    p.debugger.frame,
                ) != epoch
                {
                    return;
                }
                match result {
                    Ok(response) => {
                        p.debugger
                            .variable_history
                            .push(std::mem::take(&mut p.debugger.variables));
                        p.debugger.variables = response["variables"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default();
                    }
                    Err(e) => p.debugger.log(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn debug_evaluate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.debugger.client.clone() else {
            return;
        };
        let Some(frame) = self.debugger.frame else {
            return;
        };
        let expression = self.debugger.expression.read(cx).value().to_string();
        if expression.trim().is_empty() {
            return;
        }
        self.debugger.log(format!("> {expression}"));
        self.debugger
            .expression
            .update(cx, |s, cx| s.set_value("", window, cx));
        let epoch = (self.debugger.generation, self.debugger.stop_generation);
        let task = Tokio::spawn_result(cx, async move {
            client
                .request(
                    "evaluate",
                    json!({"expression":expression,"frameId":frame,"context":"repl"}),
                )
                .await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |p, cx| {
                if (p.debugger.generation, p.debugger.stop_generation) != epoch {
                    return;
                }
                p.debugger.log(match result {
                    Ok(result) => result["result"].as_str().unwrap_or_default().to_string(),
                    Err(e) => e.to_string(),
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_debugger(&self, cx: &Context<Self>) -> AnyElement {
        let running = self.debugger.client.is_some();
        let paused = self.debugger.thread.is_some();
        v_flex()
            .h(px(290.))
            .flex_shrink_0()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(crate::glass::elevated(cx))
            .gap_1()
            .p_2()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .flex_1()
                            .child(format!("Debug · {}", self.debugger.state)),
                    )
                    .child(
                        Button::new("debug-start")
                            .ghost()
                            .compact()
                            .label(if paused {
                                "Continue · F5"
                            } else if running {
                                "Pause · F6"
                            } else {
                                "Start · F5"
                            })
                            .on_click(cx.listener(|p, _, w, cx| {
                                if p.debugger.client.is_some() && p.debugger.thread.is_none() {
                                    p.debug_pause(w, cx);
                                } else {
                                    p.debug_start(w, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("debug-next")
                            .ghost()
                            .compact()
                            .label("Over")
                            .tooltip("Step over · F10")
                            .disabled(!paused)
                            .on_click(cx.listener(|p, _, w, cx| p.debug_command("next", w, cx))),
                    )
                    .child(
                        Button::new("debug-in")
                            .ghost()
                            .compact()
                            .label("Into")
                            .tooltip("Step into · F11")
                            .disabled(!paused)
                            .on_click(cx.listener(|p, _, w, cx| p.debug_command("stepIn", w, cx))),
                    )
                    .child(
                        Button::new("debug-out")
                            .ghost()
                            .compact()
                            .label("Out")
                            .tooltip("Step out · Shift F11")
                            .disabled(!paused)
                            .on_click(cx.listener(|p, _, w, cx| p.debug_command("stepOut", w, cx))),
                    )
                    .child(
                        Button::new("debug-stop")
                            .ghost()
                            .compact()
                            .label("Stop")
                            .disabled(!running && self.debugger.state != "Starting…")
                            .on_click(cx.listener(|p, _, w, cx| p.debug_stop(w, cx))),
                    )
                    .child(
                        Button::new("debug-hide")
                            .ghost()
                            .compact()
                            .icon(IconName::Close)
                            .on_click(cx.listener(|p, _, _, cx| {
                                p.debugger.visible = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex().gap_1().children(
                    ["Call Stack", "Variables", "Console", "Breakpoints"]
                        .into_iter()
                        .enumerate()
                        .map(|(i, name)| {
                            Button::new(("debug-tab", i))
                                .ghost()
                                .compact()
                                .label(name)
                                .selected(self.debugger.tab == i)
                                .on_click(cx.listener(move |p, _, _, cx| {
                                    p.debugger.tab = i;
                                    cx.notify();
                                }))
                        }),
                ),
            )
            .child(
                v_flex()
                    .id("debug-content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .gap_1()
                    .children(match self.debugger.tab {
                        0 => self
                            .debugger
                            .frames
                            .iter()
                            .enumerate()
                            .map(|(i, frame)| {
                                let frame = frame.clone();
                                Button::new(("debug-frame", i))
                                    .ghost()
                                    .compact()
                                    .justify_start()
                                    .text_xs()
                                    .label(format!(
                                        "{} · {}:{}",
                                        frame["name"].as_str().unwrap_or("Frame"),
                                        frame["source"]["name"].as_str().unwrap_or(""),
                                        frame["line"]
                                    ))
                                    .on_click(cx.listener(move |p, _, w, cx| {
                                        p.debug_frame(frame.clone(), w, cx)
                                    }))
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>(),
                        1 => {
                            let mut rows = vec![];
                            if !self.debugger.variable_history.is_empty() {
                                rows.push(
                                    Button::new("variables-back")
                                        .ghost()
                                        .compact()
                                        .justify_start()
                                        .label("← Parent")
                                        .on_click(cx.listener(|p, _, _, cx| {
                                            if let Some(v) = p.debugger.variable_history.pop() {
                                                p.debugger.variables = v;
                                            }
                                            cx.notify();
                                        }))
                                        .into_any_element(),
                                );
                            }
                            rows.extend(self.debugger.variables.iter().enumerate().map(
                                |(i, variable)| {
                                    let reference =
                                        variable["variablesReference"].as_i64().unwrap_or(0);
                                    Button::new(("debug-variable", i))
                                        .ghost()
                                        .compact()
                                        .justify_start()
                                        .text_xs()
                                        .label(format!(
                                            "{}{}  {}",
                                            if reference > 0 { "› " } else { "" },
                                            variable["name"].as_str().unwrap_or(""),
                                            variable["value"].as_str().unwrap_or("")
                                        ))
                                        .on_click(cx.listener(move |p, _, w, cx| {
                                            if reference > 0 {
                                                p.debug_variables(reference, w, cx);
                                            }
                                        }))
                                        .into_any_element()
                                },
                            ));
                            rows
                        }
                        2 => self
                            .debugger
                            .console
                            .iter()
                            .map(|line| {
                                div()
                                    .text_xs()
                                    .font_family("monospace")
                                    .child(line.clone())
                                    .into_any_element()
                            })
                            .collect(),
                        _ => self
                            .debugger
                            .breakpoints
                            .iter()
                            .flat_map(|(path, lines)| lines.iter().map(move |line| (path, line)))
                            .enumerate()
                            .map(|(i, (path, line))| {
                                let verified = self
                                    .debugger
                                    .verified_breakpoints
                                    .get(path)
                                    .is_some_and(|lines| lines.contains(line));
                                let path = path.clone();
                                let line = *line;
                                Button::new(("breakpoint", i))
                                    .ghost()
                                    .compact()
                                    .justify_start()
                                    .text_xs()
                                    .label(format!(
                                        "{} {path}:{line}{}",
                                        if verified { "●" } else { "○" },
                                        if running && !verified {
                                            " · unverified"
                                        } else {
                                            ""
                                        }
                                    ))
                                    .on_click(cx.listener(move |p, _, w, cx| {
                                        p.reveal(
                                            path.clone(),
                                            Some(RevealTarget::at(WirePosition {
                                                line: line - 1,
                                                character: 0,
                                            })),
                                            w,
                                            cx,
                                        )
                                    }))
                                    .into_any_element()
                            })
                            .collect(),
                    }),
            )
            .when(self.debugger.tab == 2, |view| {
                view.child(Input::new(&self.debugger.expression).disabled(!paused))
            })
            .into_any_element()
    }
}
