//! A single environment-scoped automation host routes every request to its
//! exact thread/tab. User consent is controlled in Settings and revocable.
use super::{ChatApp, preview::PreviewPanel};
use base64::Engine as _;
use gpui::{Context, Entity, Window, prelude::*};
use serde_json::{Value, json};
use vitre_contracts::{methods::*, *};
use vitre_rpc::TypedStreamEvent;

impl ChatApp {
    pub(super) fn sync_preview_automation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !crate::client_settings::ClientSettings::get(cx).preview_automation_enabled {
            self.preview_automation_task = None;
            return;
        }
        if self.preview_automation_task.is_some() {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        self.preview_automation_task=Some(cx.spawn_in(window,async move |this,cx| {
            let mut sessions=client.sessions();
            loop {
                let handle=sessions.borrow_and_update().clone();
                let Some(handle)=handle else{if sessions.changed().await.is_err(){return;}continue;};
                let client_id=PreviewAutomationClientId(format!("vitre-{}",std::process::id()));
                let host=PreviewAutomationHost{client_id:client_id.clone(),environment_id:handle.config.environment.environment_id.clone(),supported_operations:Some(Some(vec![PreviewAutomationOperation::Status,PreviewAutomationOperation::Open,PreviewAutomationOperation::Navigate,PreviewAutomationOperation::Snapshot,PreviewAutomationOperation::Click,PreviewAutomationOperation::Type,PreviewAutomationOperation::Press,PreviewAutomationOperation::Scroll,PreviewAutomationOperation::Evaluate,PreviewAutomationOperation::WaitFor,PreviewAutomationOperation::Resize,PreviewAutomationOperation::SetColorScheme,PreviewAutomationOperation::RecordingStart,PreviewAutomationOperation::RecordingStop]))};
                let Ok(mut stream)=handle.session.subscribe_typed::<PreviewAutomationConnect>(&host)else{return};
                let mut connection=None;
                let mut last_focus=None;
                loop {
                    let event=tokio::select! {
                        changed=sessions.changed()=>{if changed.is_err(){return;}break;},
                        _=cx.background_executor().timer(std::time::Duration::from_secs(2))=>{
                            if let Some(connection_id)=connection.clone(){
                                let focused=this.update_in(cx,|_,window,_|window.is_window_active()).unwrap_or(false);
                                if last_focus!=Some(focused){let _=handle.session.call_typed::<PreviewAutomationFocusHost>(&PreviewAutomationHostFocus{client_id:client_id.clone(),connection_id,environment_id:host.environment_id.clone(),focused}).await;last_focus=Some(focused);}
                            }continue;
                        },
                        event=stream.next()=>event,
                    };
                    let Some(TypedStreamEvent::Values(events))=event else{break;};
                    for event in events {
                        let (connection_id,request)=match event {
                            PreviewAutomationStreamEvent::Connected{connection_id}=>{
                                connection=Some(connection_id.clone());
                                let focused=this.update_in(cx,|_,window,_|window.is_window_active()).unwrap_or(false);last_focus=Some(focused);
                                let input=PreviewAutomationHostFocus{client_id:client_id.clone(),connection_id,environment_id:host.environment_id.clone(),focused};
                                let _=handle.session.call_typed::<PreviewAutomationFocusHost>(&input).await;continue;
                            }
                            PreviewAutomationStreamEvent::Request{connection_id,request}=>(connection_id,request),
                            _=>continue,
                        };
                        let executor=cx.background_executor().clone();
                        let timeout=executor.timer(std::time::Duration::from_millis(request.timeout_ms.clamp(1,60000) as u64));
                        let run=async {
                            let list=client.call::<PreviewList>(&PreviewListInput{thread_id:request.thread_id.clone()}).await.map_err(|e|e.user_message())?;
                            let explicit=request.tab_id.as_ref().and_then(|v|v.as_ref());
                            let existing=if let Some(id)=explicit {list.sessions.into_iter().find(|s|&s.tab_id==id)}else{list.sessions.into_iter().next()};
                            if explicit.is_some() && existing.is_none(){return Err("The requested browser tab no longer exists.".to_string());}
                            let snapshot=match request.operation {
                                PreviewAutomationOperation::Open if existing.is_none() || request.input["reuseExistingTab"].as_bool()==Some(false)=>{
                                    let payload=PreviewOpenInput{thread_id:request.thread_id.clone(),url:request.input["url"].as_str().map(|url|Some(TrimmedNonEmptyString(url.to_string())))};
                                    client.call::<PreviewOpen>(&payload).await.map_err(|e|e.user_message())?
                                }
                                PreviewAutomationOperation::Status if existing.is_none()=>return Ok(json!({"available":false,"visible":false,"tabId":null,"url":null,"title":null,"loading":false})),
                                _=>existing.ok_or("Open a browser tab first")?,
                            };
                            let tab_id=snapshot.tab_id.clone();
                            let reveal=request.input["open"].as_bool().or(request.input["show"].as_bool()).unwrap_or(request.operation==PreviewAutomationOperation::Open);
                            let panel=this.update_in(cx,|app,window,cx|app.ensure_automation_panel(snapshot,reveal,window,cx)).map_err(|e|e.to_string())??;
                            match request.operation {
                                PreviewAutomationOperation::Open|PreviewAutomationOperation::Navigate=>{
                                    if let Some(url)=navigation_url(&request.input)?{
                                        let snapshot=client.call::<PreviewNavigate>(&PreviewNavigateInput{thread_id:request.thread_id.clone(),tab_id:tab_id.clone(),url:TrimmedNonEmptyString(url),resolved_title:None}).await.map_err(|e|e.user_message())?;
                                        panel.update(cx,|panel,cx|panel.load_automation_snapshot(snapshot,cx));
                                    }
                                    // Give the native guest time to complete navigation; state comes
                                    // from WebKit's callbacks, not the server's optimistic snapshot.
                                    if request.input["readiness"].as_str()!=Some("none") {
                                        loop {
                                            let status=panel.update(cx,|p,cx|p.automation_status(cx));
                                            if let Some(error)=status["error"].as_str(){return Err(error.to_string());}
                                            if status["loading"].as_bool()!=Some(true){break;}
                                            if request.input["readiness"].as_str()==Some("domContentLoaded"){
                                                let rx=panel.update(cx,|p,cx|p.execute_page("evaluate",json!({"expression":"document.readyState !== 'loading'"}),cx));
                                                let ready=tokio::select!{ready=rx=>ready.ok().and_then(Result::ok).is_some_and(|v|v==true),_=executor.timer(std::time::Duration::from_millis(100))=>false};if ready{break;}
                                            }
                                            executor.timer(std::time::Duration::from_millis(50)).await;
                                        }
                                    }
                                    Ok(panel.update(cx,|p,cx|p.automation_status(cx)))
                                }
                                PreviewAutomationOperation::Status=>Ok(panel.update(cx,|p,cx|p.automation_status(cx))),
                                PreviewAutomationOperation::Resize=>{
                                    let viewport=resize_input(&request.input)?;
                                    let snapshot=client.call::<PreviewResize>(&PreviewResizeInput{thread_id:request.thread_id.clone(),tab_id:tab_id.clone(),viewport:viewport.clone()}).await.map_err(|e|e.user_message())?;
                                    panel.update(cx,|p,cx|p.apply_automation_metadata(snapshot,cx));
                                    let expected=serde_json::to_value(&viewport).unwrap();
                                    let dimensions=loop{
                                        executor.timer(std::time::Duration::from_millis(100)).await;
                                        let rx=panel.update(cx,|p,cx|p.execute_page("evaluate",json!({"expression":"({width:innerWidth,height:innerHeight})"}),cx));
                                        let dimensions=rx.await.map_err(|_|"Browser closed during resize")??;
                                        if expected["_tag"]=="fill" || ((dimensions["width"].as_i64().unwrap_or(0)-expected["width"].as_i64().unwrap_or(0)).abs()<=2 && (dimensions["height"].as_i64().unwrap_or(0)-expected["height"].as_i64().unwrap_or(0)).abs()<=2){break dimensions;}
                                    };
                                    Ok(json!({"tabId":tab_id,"setting":viewport,"viewport":dimensions}))
                                }
                                PreviewAutomationOperation::SetColorScheme=>{
                                    let scheme=match request.input["colorScheme"].as_str(){Some("dark")=>"dark",Some("light")=>"light",_=>"system"};
                                    panel.update(cx,|p,cx|p.set_color_scheme(scheme,cx));
                                    Ok(json!({"tabId":tab_id,"colorScheme":scheme}))
                                }
                                PreviewAutomationOperation::RecordingStart=>panel.update(cx,|p,cx|p.start_recording(cx)),
                                PreviewAutomationOperation::RecordingStop=>{
                                    let finished=panel.update(cx,|p,cx|p.stop_recording(cx))?;
                                    finished.await.map_err(|_|"Recording encoder stopped".to_string())?
                                }
                                ref operation=>{
                                    let operation=serde_json::to_value(operation).unwrap().as_str().unwrap().to_string();
                                    let rx=panel.update(cx,|p,cx|p.execute_page(&operation,request.input.clone(),cx));
                                    let mut result=rx.await.map_err(|_|"Browser closed during the operation")??;
                                    if operation=="snapshot" {
                                        let rx=panel.update(cx,|p,cx|p.native_snapshot(cx)).ok_or("Browser host unavailable")?;
                                        let bytes=rx.await.map_err(|_|"Snapshot canceled")??;
                                        let (width,height)=png_dimensions(&bytes).ok_or("Invalid snapshot PNG")?;
                                        result["screenshot"]=json!({"mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(bytes),"width":width,"height":height});
                                    }
                                    Ok(result)
                                }
                            }
                        };
                        let result=tokio::select! {result=run=>result,_=timeout=>Err("Browser operation timed out".to_string())};
                        let response=PreviewAutomationResponse{client_id:client_id.clone(),connection_id,request_id:request.request_id,ok:result.is_ok(),result:result.as_ref().ok().cloned().map(Some),error:result.err().map(|message|Some(PreviewAutomationResponseError{tag:TrimmedNonEmptyString("PreviewAutomationExecutionError".into()),message:TrimmedNonEmptyString(message),detail:None}))};
                        let _=handle.session.call_typed::<PreviewAutomationRespond>(&response).await;
                    }
                    if stream.ack().is_err(){break;}
                }
                cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
            }
        }));
    }

    pub(super) fn ensure_automation_panel(
        &mut self,
        snapshot: PreviewSessionSnapshot,
        reveal: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Entity<PreviewPanel>, String> {
        let client = self.client.clone().expect("connected automation host");
        let environment = client
            .sessions()
            .borrow()
            .as_ref()
            .unwrap()
            .config
            .environment
            .environment_id
            .0
            .clone();
        let thread = ThreadId(snapshot.thread_id.0.clone());
        let thread_key = format!("{environment}:{}", thread.0);
        let key = format!("{thread_key}|browser:{}", snapshot.tab_id.0);
        if !self.reserve_preview_guest(&key, cx) {
            return Err(
                "Browser guest limit reached. Stop a recording or close the mini player first."
                    .into(),
            );
        }
        let panel = if let Some(panel) = self.preview_panels.get(&key) {
            panel.clone()
        } else {
            let panel = cx.new(|cx| {
                PreviewPanel::new(
                    client,
                    thread.clone(),
                    Some(snapshot.tab_id.clone()),
                    Some(snapshot.clone()),
                    window,
                    cx,
                )
            });
            self.register_preview_events(&panel, key.clone(), thread_key.clone(), window, cx);
            self.preview_panels.insert(key, panel.clone());
            panel
        };
        if reveal {
            self.select_thread(thread, cx);
            self.right_panel
                .map
                .open_browser(&thread_key, Some(&snapshot.tab_id.0));
            self.right_panel.save();
        }
        cx.notify();
        Ok(panel)
    }
}

fn navigation_url(input: &Value) -> Result<Option<String>, String> {
    if let Some(url) = input["url"].as_str().or(input["target"]["url"].as_str()) {
        return super::preview::normalize_url(url)
            .map(Some)
            .ok_or("Invalid browser URL".into());
    }
    if input["target"]["kind"] == "environment-port" {
        let port = input["target"]["port"]
            .as_u64()
            .filter(|p| *p > 0 && *p < 65536)
            .ok_or("Invalid environment port")?;
        let protocol = input["target"]["protocol"].as_str().unwrap_or("http");
        if !["http", "https"].contains(&protocol) {
            return Err("Invalid protocol".into());
        }
        let path = input["target"]["path"].as_str().unwrap_or("/");
        return Ok(Some(format!(
            "{protocol}://127.0.0.1:{port}/{}",
            path.trim_start_matches('/')
        )));
    }
    Ok(None)
}

fn resize_input(input: &Value) -> Result<PreviewViewportSetting, String> {
    let has_dimensions = input.get("width").is_some() || input.get("height").is_some();
    let mut value = match input["mode"].as_str() {
        Some("fill")
            if !has_dimensions
                && input.get("preset").is_none()
                && input.get("orientation").is_none() =>
        {
            json!({"_tag":"fill"})
        }
        Some("freeform")
            if input["width"].is_i64()
                && input["height"].is_i64()
                && input.get("preset").is_none()
                && input.get("orientation").is_none() =>
        {
            json!({"_tag":"freeform","width":input["width"],"height":input["height"]})
        }
        Some("preset") if !has_dimensions => super::preview::preset_viewport(
            input["preset"]
                .as_str()
                .ok_or("Preset mode requires a preset")?,
        )
        .ok_or("Unknown device preset")?,
        _ => {
            return Err(
                "Invalid viewport mode or incompatible preset/dimensions/orientation".into(),
            );
        }
    };
    if let Some(orientation) = input["orientation"].as_str() {
        if !["portrait", "landscape"].contains(&orientation) {
            return Err("Unknown orientation".into());
        }
        let w = value["width"]
            .as_i64()
            .ok_or("Orientation requires a preset")?;
        let h = value["height"]
            .as_i64()
            .ok_or("Orientation requires a preset")?;
        if (orientation == "landscape" && w < h) || (orientation == "portrait" && w > h) {
            value["width"] = json!(h);
            value["height"] = json!(w);
        }
    }
    if let (Some(w), Some(h)) = (value["width"].as_i64(), value["height"].as_i64())
        && (!(240..=3840).contains(&w) || !(240..=3840).contains(&h) || w * h > 3840 * 2160)
    {
        return Err("Invalid viewport dimensions".into());
    }
    serde_json::from_value(value).map_err(|e| e.to_string())
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    Some((
        u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        u32::from_be_bytes(bytes[20..24].try_into().ok()?),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_matches_contract_modes_and_bounds() {
        assert!(resize_input(&json!({"mode":"fill"})).is_ok());
        let landscape = serde_json::to_value(
            resize_input(&json!({"mode":"preset","preset":"pixel-7","orientation":"landscape"}))
                .unwrap(),
        )
        .unwrap();
        assert!(landscape["width"].as_i64() > landscape["height"].as_i64());
        assert!(resize_input(&json!({"mode":"freeform","width":240,"height":3840})).is_ok());
        for bad in [
            json!({}),
            json!({"mode":"fill","width":800,"height":600}),
            json!({"mode":"freeform","width":239,"height":600}),
            json!({"mode":"freeform","width":3840,"height":3840}),
            json!({"mode":"freeform","width":800.5,"height":600}),
            json!({"mode":"preset","preset":"invented"}),
            json!({"mode":"preset","preset":"pixel-7","orientation":"up"}),
            json!({"mode":"freeform","width":800,"height":600,"orientation":"landscape"}),
        ] {
            assert!(resize_input(&bad).is_err(), "accepted {bad}");
        }
    }
    #[test]
    fn navigation_targets_validate_protocol_and_local_port() {
        assert_eq!(
            navigation_url(
                &json!({"target":{"kind":"environment-port","port":43199,"path":"/page?q=1"}})
            )
            .unwrap()
            .as_deref(),
            Some("http://127.0.0.1:43199/page?q=1")
        );
        assert!(navigation_url(&json!({"url":"javascript:alert(1)"})).is_err());
        assert!(
            navigation_url(&json!({"target":{"kind":"environment-port","port":65536}})).is_err()
        );
        assert!(
            navigation_url(
                &json!({"target":{"kind":"environment-port","port":80,"protocol":"file"}})
            )
            .is_err()
        );
    }
}
