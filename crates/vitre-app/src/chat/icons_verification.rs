//! Reproducible native icon/layout pass. Never touches the user's live home.
use super::*;

const FIXTURES: &[(&str, &str)] = &[
    (
        "README.md",
        "# Vitre studio\n\nA calmer space to build great things.\n\nInline code may cross a source line: `first\nsecond`.\n",
    ),
    (
        "package.json",
        "{\n  \"name\": \"vitre-studio\",\n  \"private\": true\n}\n",
    ),
    (
        "Cargo.toml",
        "[package]\nname = \"vitre-studio\"\nversion = \"0.1.0\"\n",
    ),
    ("tsconfig.json", "{\"compilerOptions\":{\"strict\":true}}\n"),
    ("pnpm-lock.yaml", "lockfileVersion: '9.0'\n"),
    (".gitignore", "node_modules/\ntarget/\n"),
    (".github/workflows/check.yml", "name: Checks\n"),
    (
        "src/components/Workspace.tsx",
        "import { useState } from 'react';\n\nexport function Workspace() {\n  const [mode, setMode] = useState('build');\n\n  return (\n    <main className=\"workspace\">\n      <h1>A little room to think.</h1>\n      <button onClick={() => setMode('plan')}>\n        {mode === 'build' ? 'Make a plan' : 'Ready to build'}\n      </button>\n    </main>\n  );\n}\n",
    ),
    (
        "src/components/workspace-toolbar-with-a-long-name.tsx",
        "export const title = 'Workspace';\n",
    ),
    ("src/main.rs", "fn main() { println!(\"Hello, Vitre\"); }\n"),
    ("src/theme.scss", "$accent: #8ba4ff;\n"),
    ("src/types.ts", "export type Mode = 'plan' | 'build';\n"),
    (
        "tests/workspace.test.ts",
        "// Workspace interaction tests\n",
    ),
    ("AGENTS.md", "# Project conventions\n"),
    ("unknown.custom", "A file without a recognized extension.\n"),
];

impl ChatApp {
    pub(in crate::chat) fn verify_icons_if_requested(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let Ok(mode) = std::env::var("VITRE_VERIFY_ICONS") else {
            return;
        };
        let home = crate::vitre_home();
        if !home
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("vitre-polish-"))
        {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        if self.shell.snapshot.is_none() || STARTED.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let run = async {
                let mut selected = None;
                for (title, icon) in [("Vitre studio", Some(br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="8" fill="#6478ed"/><path d="m8 9 8 16 8-16h-5l-3 7-3-7z" fill="white"/></svg>"##.as_slice())), ("Design system", None), ("API playground", Some(b"invalid icon".as_slice()))] {
                    let project = fresh_id("vitre-project");
                    let cwd = home.join("workspace").join(&project).to_string_lossy().into_owned();
                    let thread = ThreadId(fresh_id("icon-thread"));
                    // Create a unique fixture directory before publishing the project;
                    // the favicon cache must see the finished fixture on its first read.
                    let seed_root = cwd.clone();
                    let language_fixtures = mode == "languages";
                    let seed = Tokio::spawn_result(cx, async move {
                        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                            std::fs::create_dir_all(Path::new(&seed_root).parent().unwrap())?;
                            std::fs::create_dir(&seed_root)?; // Fail if anything already exists.
                            if let Some(icon) = icon { std::fs::write(Path::new(&seed_root).join("favicon.svg"), icon)?; }
                            for dir in ["src/components", "tests", ".github/workflows"] { std::fs::create_dir_all(Path::new(&seed_root).join(dir))?; }
                            for (path, text) in FIXTURES { std::fs::write(Path::new(&seed_root).join(path), text)?; }
                            if language_fixtures {
                                for (path, text) in crate::files::FilesPanel::verification_language_fixtures() {
                                    std::fs::write(Path::new(&seed_root).join(path), text)?;
                                }
                            }
                            let mut preview = image::RgbaImage::new(640,360);
                            for (x,y,pixel) in preview.enumerate_pixels_mut() {*pixel=image::Rgba([(x*120/640+50) as u8,(y*100/360+60) as u8,190,255]);}
                            preview.save(Path::new(&seed_root).join("preview.png"))?;
                            Ok(())
                        }).await?
                    });
                    seed.await.map_err(|e|e.to_string())?;
                    for payload in [
                        serde_json::json!({"type":"project.create","commandId":fresh_id("cmd"),"projectId":project,"title":title,"workspaceRoot":cwd,"createdAt":now_iso()}),
                        serde_json::json!({"type":"thread.create","commandId":fresh_id("cmd"),"projectId":project,"threadId":thread,"title":if selected.is_none(){"Refine the workspace"}else{"Start a new conversation"},"modelSelection":{"provider":"codex","model":"gpt-5"},"runtimeMode":"full-access","interactionMode":"default","branch":null,"worktreePath":null,"createdAt":now_iso()})
                    ] {
                        let command = serde_json::from_value(payload).map_err(|e| e.to_string())?;
                        client.dispatch(&command).await.map_err(|e|e.user_message())?;
                    }
                    if selected.is_none() { selected = Some((thread, cwd)); }
                }
                let (thread, cwd) = selected.unwrap();
                loop { executor.timer(Duration::from_millis(80)).await; if this.update(cx,|a,_|a.shell_thread(&thread).is_some()).map_err(|e|e.to_string())? {break;} }
                this.update_in(cx,|a,w,cx| {
                    a.select_thread(thread.clone(),cx);
                    gpui_component::Theme::change(if mode == "light" {gpui_component::ThemeMode::Light}else{gpui_component::ThemeMode::Dark},Some(w),cx);
                    if mode == "narrow" { w.resize(gpui::size(px(1100.),px(800.))); }
                }).map_err(|e|e.to_string())?;
                loop {executor.timer(Duration::from_millis(80)).await; if this.update(cx,|a,_|a.thread.as_ref().is_some_and(|t|t.state.view.is_some()) && a.project_icons.entries.get(&cwd).is_some_and(|e|e.image.is_some())).map_err(|e|e.to_string())? {break;} }
                for path in ["README.md", "package.json", "Cargo.toml", "src/components/Workspace.tsx"] {
                    this.update_in(cx, |a,w,cx|a.dock_open_file(path.into(),Some(1),w,cx)).map_err(|e|e.to_string())?;
                    loop { executor.timer(Duration::from_millis(80)).await; if this.update(cx,|a,cx|a.files.as_ref().and_then(|f|f.read(cx).active_file_status()).is_some_and(|(p,_)|p==path)).map_err(|e|e.to_string())? {break;} }
                }
                executor.timer(Duration::from_millis(500)).await;
                cx.update(|w,cx| {
                    let position = gpui::point(px(70.),px(60.));
                    w.dispatch_event(gpui::PlatformInput::MouseDown(gpui::MouseDownEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1, first_mouse: false }),cx);
                    w.dispatch_event(gpui::PlatformInput::MouseUp(gpui::MouseUpEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1 }),cx);
                }).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(250)).await;
                if !this.update(cx,|a,_|a.quick_search.is_some()).map_err(|e|e.to_string())? {return Err("Sidebar Search hit target did not open the overlay".into());}
                this.update_in(cx,|a,w,cx|a.toggle_quick_search(QuickSearchMode::Open,w,cx)).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(150)).await;
                cx.update(|w,cx| {
                    let position = gpui::point(w.viewport_size().width - px(18.),px(22.));
                    w.dispatch_event(gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {button:gpui::MouseButton::Left,position,modifiers:Default::default(),click_count:1,first_mouse:false}),cx);
                    w.dispatch_event(gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {button:gpui::MouseButton::Left,position,modifiers:Default::default(),click_count:1}),cx);
                }).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(150)).await;
                if this.update(cx,|a,_|a.dock_open()).map_err(|e|e.to_string())? { return Err("Hide panel hit target did not close the dock".into()); }
                this.update_in(cx,|a,w,cx|a.toggle_right_panel(w,cx)).map_err(|e|e.to_string())?;
                let counts = this.update(cx, |a,_| (a.project_icons.entries.values().filter(|e|e.image.is_some()).count(),a.project_icons.entries.values().filter(|e|e.image.is_none()).count())).map_err(|e|e.to_string())?;
                if counts.0 == 0 || counts.1 < 2 {return Err(format!("Unexpected favicon/fallback counts: {counts:?}"));}
                if mode == "ui" {
                    this.update_in(cx, |app, window, cx| app.verify_surface_ui(window, cx)).map_err(|e| e.to_string())?;
                }
                if mode == "phase5" {
                    this.update_in(cx, |app, window, cx| app.verify_phase5(window, cx)).map_err(|e|e.to_string())?;
                }
                if mode == "tabs" {
                    this.update_in(cx, |app, window, cx| app.verify_dock_tabs(window, cx)).map_err(|e|e.to_string())?;
                }
                if mode == "menus" {
                    this.update_in(cx, |app, window, cx| app.verify_context_menus(window, cx)).map_err(|e|e.to_string())?;
                }
                if mode == "languages" {
                    let task = this.update_in(cx, |app, window, cx| app.files.as_ref().unwrap().update(cx, |p, cx| p.verify_languages(window, cx))).map_err(|e| e.to_string())?;
                    task.await?;
                    super::ui_verification::capture("languages-react", cx).await?;
                    let task = this.update_in(cx, |app, window, cx| app.files.as_ref().unwrap().update(cx, |p, cx| p.verify_refactor(window, cx))).map_err(|e| e.to_string())?;
                    task.await?;
                    let task = this.update_in(cx, |app, window, cx| app.files.as_ref().unwrap().update(cx, |p, cx| p.verify_debugger(window, cx))).map_err(|e| e.to_string())?;
                    task.await?;
                }
                Ok::<_,String>(())
            };
            let result = tokio::select! { r = run => r, _ = executor.timer(Duration::from_secs(if mode == "languages" { 180 } else { 40 })) => Err("Icon verification timed out".into()) };
            match result {Ok(())=>eprintln!("[vitre-icons-test] PASS: signed SVG favicon, missing/broken fallbacks, nested file tree, four file tabs, Search and Hide panel mouse hit targets, dock restoration ({mode})"),Err(e)=>eprintln!("[vitre-icons-test] FAIL: {e}")}
        }).detach();
    }
}
