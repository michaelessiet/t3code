//! Secondary-click regression pass in the disposable native fixture.
use super::*;

async fn click(
    button: gpui::MouseButton,
    control: bool,
    x: f32,
    y: f32,
    cx: &mut gpui::AsyncWindowContext,
) -> Result<(), String> {
    cx.update(|w, cx| {
        let position = gpui::point(px(x), px(y));
        let modifiers = gpui::Modifiers {
            control,
            ..Default::default()
        };
        w.dispatch_event(
            gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers,
            }),
            cx,
        );
        w.dispatch_event(
            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                button,
                position,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        w.dispatch_event(
            gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                button,
                position,
                modifiers,
                click_count: 1,
            }),
            cx,
        );
    })
    .map_err(|e| e.to_string())
}

impl ChatApp {
    pub(super) fn verify_context_menus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this,cx| {
            let run = async {
                ui_verification::float_fixture(cx).await?;
                this.update_in(cx,|a,w,cx| { w.resize(gpui::size(px(1240.),px(900.))); if a.dock_open() { a.toggle_right_panel(w,cx); } }).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(900)).await;
                ui_verification::capture("menus-before",cx).await?;
                let selected = this.update(cx,|a,_| a.thread.as_ref().map(|t|t.id.clone())).map_err(|e|e.to_string())?;
                cx.update(|_,cx|cx.write_to_clipboard(gpui::ClipboardItem::new_string("menu-test-sentinel".into()))).map_err(|e|e.to_string())?;
                click(gpui::MouseButton::Right,false,120.,146.,cx).await?;
                executor.timer(Duration::from_millis(300)).await;
                ui_verification::capture("menus-secondary",cx).await?;
                if this.update(cx,|a,_|a.thread.as_ref().map(|t|t.id.clone())!=selected).map_err(|e|e.to_string())? { return Err("Secondary-click changed the selected thread".into()); }
                // The menu starts with no keyboard selection.
                for _ in 0..4 { ui_verification::key("down",cx).await?; }
                ui_verification::key("enter",cx).await?;
                executor.timer(Duration::from_millis(200)).await;
                let copied = cx.update(|_,cx| cx.read_from_clipboard().and_then(|c|c.text())).map_err(|e|e.to_string())?;
                let target = this.update(cx, |a,_| copied.as_ref().and_then(|id|a.shell_thread(&ThreadId(id.clone()))).cloned()).map_err(|e|e.to_string())?;
                if target.is_none() { return Err(format!("Secondary-click Copy Thread ID did not target a fixture thread: {copied:?}")); }
                click(gpui::MouseButton::Left,true,120.,146.,cx).await?;
                executor.timer(Duration::from_millis(300)).await;
                ui_verification::capture("menus-control-click",cx).await?;
                ui_verification::key("down",cx).await?;
                ui_verification::key("enter",cx).await?;
                executor.timer(Duration::from_millis(200)).await;
                let renaming = this.update(cx,|a,_|a.thread_rename.is_some()).map_err(|e|e.to_string())?;
                if !renaming { return Err("Control-click menu did not open Rename thread".into()); }
                ui_verification::key("escape",cx).await?;
                if this.update(cx,|a,_|a.thread_rename.is_some()).map_err(|e|e.to_string())? { return Err("Escape did not cancel inline rename".into()); }
                // Give the row a frame to replace the native text input with
                // the thread hit target before issuing the next mouse event.
                executor.timer(Duration::from_millis(160)).await;
                // Commit an inline rename from the same real context menu.
                click(gpui::MouseButton::Right,false,120.,146.,cx).await?;
                executor.timer(Duration::from_millis(160)).await;
                ui_verification::key("down",cx).await?;
                ui_verification::key("enter",cx).await?;
                executor.timer(Duration::from_millis(120)).await;
                this.update_in(cx,|a,w,cx| {
                    let input=a.verification_rename_input().ok_or("Rename input was not focused")?;
                    input.update(cx,|i,cx|i.set_value("Context menu regression",w,cx));
                    Ok::<_,String>(())
                }).map_err(|e|e.to_string())??;
                ui_verification::key("enter",cx).await?;
                let target=target.unwrap();
                loop {
                    executor.timer(Duration::from_millis(80)).await;
                    if this.update(cx,|a,_|a.shell_thread(&target.id).is_some_and(|t|t.title.0=="Context menu regression")).map_err(|e|e.to_string())? {break;}
                }
                // Project secondary-click uses its own actions, not a thread's.
                click(gpui::MouseButton::Right,false,120.,106.,cx).await?;
                executor.timer(Duration::from_millis(200)).await;
                ui_verification::capture("menus-project",cx).await?;
                ui_verification::key("escape",cx).await?;
                // Both sidebar implementations use the same thread actions.
                this.update(cx,|_,cx|ClientSettings::update(cx,|s|s.sidebar_v2_enabled=true)).map_err(|e|e.to_string())?;
                cx.update(|_,cx|cx.write_to_clipboard(gpui::ClipboardItem::new_string("menu-test-sentinel-v2".into()))).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(250)).await;
                click(gpui::MouseButton::Right,false,100.,110.,cx).await?;
                executor.timer(Duration::from_millis(250)).await;
                ui_verification::capture("menus-sidebar-v2",cx).await?;
                // The V2 first item is Settle, followed by Rename, Mark unread,
                // Copy Path and Copy Thread ID.
                for _ in 0..5 { ui_verification::key("down",cx).await?; }
                ui_verification::key("enter",cx).await?;
                executor.timer(Duration::from_millis(100)).await;
                let copied = cx.update(|_,cx|cx.read_from_clipboard().and_then(|c|c.text())).map_err(|e|e.to_string())?;
                if !this.update(cx,|a,_|copied.as_ref().is_some_and(|id|a.shell_thread(&ThreadId(id.clone())).is_some())).map_err(|e|e.to_string())? {return Err("V2 context menu did not copy a thread ID".into());}
                this.update(cx,|_,cx|ClientSettings::update(cx,|s|s.sidebar_v2_enabled=false)).map_err(|e|e.to_string())?;
                ui_verification::key("cmd-,",cx).await?;
                executor.timer(Duration::from_millis(600)).await;
                this.update(cx,|a,cx| { if let Some(settings)=&a.settings { settings.update(cx,|p,cx| { p.verification_select("Providers",cx); cx.notify(); }); } }).map_err(|e|e.to_string())?;
                executor.timer(Duration::from_millis(700)).await;
                ui_verification::capture("settings-providers",cx).await?;
                click(gpui::MouseButton::Left,false,1127.,184.,cx).await?;
                executor.timer(Duration::from_millis(400)).await;
                if !this.update(cx,|a,cx|a.settings.as_ref().unwrap().read(cx).verification_provider_expanded("codex")).map_err(|e|e.to_string())? {return Err("Provider chevron did not expand Codex".into());}
                ui_verification::capture("settings-provider-expanded",cx).await?;
                click(gpui::MouseButton::Left,false,1127.,184.,cx).await?;
                executor.timer(Duration::from_millis(350)).await;
                let saved=this.update_in(cx,|a,w,cx|a.settings.as_ref().unwrap().update(cx,|p,cx|p.verify_provider_form(w,cx))).map_err(|e|e.to_string())?;
                saved.await.map_err(|e|e.to_string())??;
                for section in ["General","Keybindings","Knowledge Graph","Language servers","Archived threads"] {
                    this.update(cx,|a,cx|a.settings.as_ref().unwrap().update(cx,|p,cx|{p.verification_select(section,cx);cx.notify();})).map_err(|e|e.to_string())?;
                    executor.timer(Duration::from_millis(200)).await;
                    ui_verification::capture(&format!("settings-{}",section.to_lowercase().replace(' ',"-")),cx).await?;
                }
                Ok::<_,String>(())
            };
            let result = tokio::select! { r=run=>r, _=executor.timer(Duration::from_secs(60))=>Err("Native menu verification timed out".into()) };
            match result { Ok(())=>eprintln!("[vitre-menu-test] PASS: secondary click and Control-click, both sidebars, target identity, selection preservation, inline rename commit/cancel, provider save/remove, settings pages"),Err(e)=>eprintln!("[vitre-menu-test] FAIL: {e}") }
        }).detach();
    }
}
