//! Reproducible native-host smoke test for development builds. Run with an
//! isolated VITRE_HOME, VITRE_OPEN_THREAD and VITRE_VERIFY_PREVIEW_URL pointing
//! to scripts/vitre/preview-fixture.html served over loopback HTTP.
use super::ChatApp;
use gpui::{Context, Window};
use serde_json::json;
use vitre_contracts::{PreviewOpenInput, TrimmedNonEmptyString, methods::PreviewOpen};

impl ChatApp {
    pub(super) fn verify_preview_if_requested(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let Ok(url) = std::env::var("VITRE_VERIFY_PREVIEW_URL") else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(thread) = self.thread.as_ref().map(|t| t.id.clone()) else {
            return;
        };
        if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let executor = cx.background_executor().clone();
        cx.spawn_in(window,async move |this,cx| {
            let test=async {
                let snapshot=client.call::<PreviewOpen>(&PreviewOpenInput{thread_id:thread,url:Some(Some(TrimmedNonEmptyString(url.clone())))}).await.map_err(|e|e.user_message())?;
                let panel=this.update_in(cx,|app,window,cx|app.ensure_automation_panel(snapshot,true,window,cx)).map_err(|e|e.to_string())??;
                loop {
                    executor.timer(std::time::Duration::from_millis(100)).await;
                    let status=panel.update(cx,|panel,cx|panel.automation_status(cx));
                    if status["url"].as_str().is_some_and(|s|s.starts_with(&url)) && status["loading"]==false {break;}
                }
                let rx=panel.update(cx,|panel,cx|panel.execute_page("snapshot",json!({}),cx));
                let snapshot=rx.await.map_err(|e|e.to_string())??;
                if !snapshot["visibleText"].as_str().unwrap_or("").contains("Vitre browser fixture"){return Err(format!("Unexpected page snapshot: {snapshot}"));}
                let rx=panel.update(cx,|panel,cx|panel.execute_page("type",json!({"locator":"role=textbox[name='Message']","text":"native verified","clear":true}),cx));
                rx.await.map_err(|e|e.to_string())??;
                let rx=panel.update(cx,|p,cx|p.execute_page("press",json!({"key":"!"}),cx));rx.await.map_err(|e|e.to_string())??;
                let rx=panel.update(cx,|p,cx|p.execute_page("press",json!({"key":"Backspace"}),cx));rx.await.map_err(|e|e.to_string())??;
                let rx=panel.update(cx,|panel,cx|panel.execute_page("click",json!({"locator":"role=button[name='Apply']"}),cx));
                rx.await.map_err(|e|e.to_string())??;
                let rx=panel.update(cx,|panel,cx|panel.execute_page("evaluate",json!({"expression":"document.querySelector('#result').textContent"}),cx));
                if rx.await.map_err(|e|e.to_string())?? != "native verified" {return Err("Native typing/click did not update the fixture".into());}
                let rx=panel.update(cx,|panel,cx|panel.native_snapshot(cx)).ok_or("No native host")?;
                let png=rx.await.map_err(|e|e.to_string())??;
                if png.len()<1000{return Err("Snapshot is empty".into());}
                let path=crate::vitre_home().join("preview-verified.png");
                executor.spawn(async move {std::fs::write(path,png)}).await.map_err(|e|e.to_string())?;
                panel.update(cx,|p,cx|p.resize_viewport(serde_json::from_value(json!({"_tag":"freeform","width":375,"height":667})).unwrap(),cx));
                loop {
                    executor.timer(std::time::Duration::from_millis(100)).await;
                    let rx=panel.update(cx,|p,cx|p.execute_page("evaluate",json!({"expression":"({width:innerWidth,height:innerHeight})"}),cx));
                    let dimensions=rx.await.map_err(|e|e.to_string())??;
                    if (dimensions["width"].as_i64().unwrap_or(0)-375).abs()<=2 && (dimensions["height"].as_i64().unwrap_or(0)-667).abs()<=2{break;}
                }
                panel.update(cx,|p,cx|p.set_color_scheme("dark",cx));
                executor.timer(std::time::Duration::from_millis(200)).await;
                let rx=panel.update(cx,|p,cx|p.execute_page("evaluate",json!({"expression":"matchMedia('(prefers-color-scheme: dark)').matches"}),cx));
                if rx.await.map_err(|e|e.to_string())??!=true{return Err("Native color-scheme override failed".into());}
                panel.update(cx,|p,cx|p.start_recording(cx))?;
                executor.timer(std::time::Duration::from_millis(1200)).await;
                let rx=panel.update(cx,|p,cx|p.stop_recording(cx))?;
                let artifact=rx.await.map_err(|e|e.to_string())??;
                if artifact["sizeBytes"].as_u64().unwrap_or(0)<1000{return Err("Recording is empty".into());}
                panel.update(cx,|_,cx|cx.emit(super::preview::PreviewPanelEvent::ToggleMiniPlayer));
                executor.timer(std::time::Duration::from_millis(400)).await;
                this.update(cx,|app,_|if app.preview_mini.is_none()||app.dock_open(){Err("Mini-player failed to detach from dock")}else{Ok(())}).map_err(|e|e.to_string())??;
                if panel.update(cx,|p,cx|p.automation_status(cx))["visible"]!=true{return Err("Mini-player guest is hidden".into());}
                let rx=panel.update(cx,|p,cx|p.execute_page("type",json!({"locator":"role=textbox[name='Message']","text":"mini verified","clear":true}),cx));rx.await.map_err(|e|e.to_string())??;
                this.update_in(cx,|app,w,cx|app.sync_active_preview_surface(w,cx)).map_err(|e|e.to_string())?;
                let rx=panel.update(cx,|p,cx|p.execute_page("press",json!({"key":"!"}),cx));rx.await.map_err(|e|e.to_string())??;
                let rx=panel.update(cx,|p,cx|p.execute_page("evaluate",json!({"expression":"document.querySelector('input').value"}),cx));
                if rx.await.map_err(|e|e.to_string())??!="mini verified!"{return Err("Mini-player lost native keyboard focus on redraw".into());}
                panel.update(cx,|_,cx|cx.emit(super::preview::PreviewPanelEvent::ToggleMiniPlayer));
                let previous=this.update(cx,|_,cx|{let old=crate::client_settings::ClientSettings::get(cx).sidebar_v2_enabled;crate::client_settings::ClientSettings::update(cx,|s|s.sidebar_v2_enabled=true);old}).map_err(|e|e.to_string())?;
                executor.timer(std::time::Duration::from_millis(200)).await;
                this.update(cx,|_,cx|crate::client_settings::ClientSettings::update(cx,|s|s.sidebar_v2_enabled=previous)).map_err(|e|e.to_string())?;
                let settings=this.update_in(cx,|app,window,cx|{app.toggle_settings(window,cx);app.settings.clone().unwrap()}).map_err(|e|e.to_string())?;
                loop{executor.timer(std::time::Duration::from_millis(100)).await;if settings.update(cx,|p,_|p.verification_ready()){break;}}
                let rx=this.update_in(cx,|_,window,cx|settings.update(cx,|p,cx|p.verify_provider_form(window,cx))).map_err(|e|e.to_string())?;
                rx.await.map_err(|e|e.to_string())??;
                Ok::<_,String>(())
            };
            let result=tokio::select! {result=test=>result,_=executor.timer(std::time::Duration::from_secs(45))=>Err("Native preview verification timed out".into())};
            match result {Ok(())=>eprintln!("[vitre-preview-test] PASS: native snapshot, selectors, typing, native keys, click, PNG capture, device resize, dark appearance, GIF recording, mini player, provider form save/delete"),Err(error)=>eprintln!("[vitre-preview-test] FAIL: {error}")}
        }).detach();
    }
}
