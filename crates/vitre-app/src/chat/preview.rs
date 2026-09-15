//! Native browser preview hosted in the right-panel dock.
//!
//! The page is a wry child view. GPUI owns the browser chrome and
//! `gpui-wry` keeps the native child clipped to the panel's live layout rect.
//! Session metadata still uses the sidecar's `preview.*` RPCs, so agents and
//! attached clients refer to the same tab.

use std::sync::Arc;

use gpui::{
    Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Subscription, Window, canvas, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
    v_flex,
};
use gpui_wry::WebView;
use vitre_client::EnvironmentClient;
use vitre_contracts::methods::{
    PreviewList, PreviewNavigate, PreviewOpen, PreviewReportStatus, PreviewResize,
    SubscribeDiscoveredLocalServers, SubscribePreviewEvents,
};
use vitre_contracts::{
    DiscoveredLocalServer, PreviewEvent, PreviewListInput, PreviewNavStatus, PreviewNavigateInput,
    PreviewOpenInput, PreviewReportStatusInput, PreviewResizeInput, PreviewSessionSnapshot,
    PreviewTabId, PreviewViewportSetting, ThreadId, TrimmedNonEmptyString,
};
use vitre_rpc::TypedStreamEvent;

const BLANK_PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><style>
html,body{height:100%;margin:0;background:#171717;color:#aaa;font:13px system-ui}
body{display:grid;place-items:center}p{opacity:.7}
</style></head><body><p>Enter a URL or choose a local server above.</p></body></html>"#;

const PAGE_BRIDGE: &str = r#"
(() => {
  if (window.__vitrePreviewBridge) return;
  window.__vitrePreviewBridge = true;
  const report = () => window.ipc.postMessage(JSON.stringify({
    kind: "page", url: location.href, title: document.title || location.hostname
  }));
  addEventListener("DOMContentLoaded", report);
  addEventListener("load", report);
  addEventListener("popstate", report);
  addEventListener("hashchange", report);
  for (const method of ["pushState", "replaceState"]) {
    const original = history[method];
    history[method] = function(...args) { const result = original.apply(this, args); report(); return result; };
  }
  const observe = () => { if (document.head) new MutationObserver(report).observe(document.head, {childList:true,subtree:true}); };
  if (document.readyState === "loading") addEventListener("DOMContentLoaded", observe, {once:true}); else observe();
})();
"#;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace=vitre,no_json)]
struct SetViewport {
    value: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace=vitre,no_json)]
struct SetAppearance {
    value: String,
}

#[derive(Debug)]
enum HostEvent {
    Loading(String),
    Loaded(String),
    Page { url: String, title: String },
    Message(serde_json::Value),
}

#[derive(Debug, serde::Deserialize)]
struct PageMessage {
    kind: String,
    url: String,
    title: String,
}

#[derive(Debug, Clone)]
pub(super) enum PreviewPanelEvent {
    SessionOpened(PreviewTabId),
    SessionClosed,
    Picked(String),
    PickedImage(Vec<u8>),
    ToggleMiniPlayer,
}

pub(super) struct PreviewPanel {
    client: Arc<EnvironmentClient>,
    thread_id: ThreadId,
    tab_id: Option<PreviewTabId>,
    address: Entity<InputState>,
    webview: Option<Entity<WebView>>,
    focus_handle: FocusHandle,
    url: SharedString,
    title: SharedString,
    loading: bool,
    load_started: Option<std::time::Instant>,
    recovery_attempted: bool,
    can_go_back: bool,
    can_go_forward: bool,
    zoom: f64,
    local_servers: Vec<DiscoveredLocalServer>,
    error: Option<SharedString>,
    viewport: PreviewViewportSetting,
    color_scheme: &'static str,
    viewport_scale: f64,
    viewport_frame: Option<(f32, f32)>,
    pending_evaluations: std::collections::HashMap<
        String,
        tokio::sync::oneshot::Sender<Result<serde_json::Value, String>>,
    >,
    recording: Option<super::preview_recording::Recording>,
    tasks: Vec<gpui::Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl PreviewPanel {
    pub(super) fn new(
        client: Arc<EnvironmentClient>,
        thread_id: ThreadId,
        tab_id: Option<PreviewTabId>,
        initial_snapshot: Option<PreviewSessionSnapshot>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let address = cx.new(|cx| InputState::new(window, cx).placeholder("URL or localhost:port"));
        let (host_tx, mut host_rx) = tokio::sync::mpsc::unbounded_channel();

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let webview = {
            use raw_window_handle::HasWindowHandle as _;
            let ipc_tx = host_tx.clone();
            let nav_tx = host_tx.clone();
            let load_tx = host_tx;
            let builder = wry::WebViewBuilder::new()
                .with_devtools(cfg!(debug_assertions))
                .with_initialization_script(PAGE_BRIDGE)
                .with_initialization_script(include_str!("../../assets/preview-runtime.js"))
                .with_initialization_script(include_str!("../../assets/preview-host.js"))
                .with_ipc_handler(move |request| {
                    if request.body().len() > 4_000_000 {
                        return;
                    }
                    if let Ok(message) = serde_json::from_str::<PageMessage>(request.body())
                        && message.kind == "page"
                    {
                        let _ = ipc_tx.send(HostEvent::Page {
                            url: message.url,
                            title: message.title,
                        });
                    } else if let Ok(message) =
                        serde_json::from_str::<serde_json::Value>(request.body())
                    {
                        let _ = ipc_tx.send(HostEvent::Message(message));
                    }
                })
                .with_navigation_handler(move |url| {
                    if !url.starts_with("http://")
                        && !url.starts_with("https://")
                        && !url.starts_with("about:")
                    {
                        return false;
                    }
                    let _ = nav_tx.send(HostEvent::Loading(url));
                    true
                })
                .with_on_page_load_handler(move |event, url| {
                    if matches!(event, wry::PageLoadEvent::Finished) {
                        let _ = load_tx.send(HostEvent::Loaded(url));
                    }
                })
                .with_html(BLANK_PAGE);
            let handle = window.window_handle().expect("Vitre window handle");
            match builder.build_as_child(&handle) {
                Ok(raw) => Some(cx.new(|cx| {
                    let mut view = WebView::new(raw, window, cx);
                    view.hide();
                    // Background automation needs a real CSS viewport before
                    // this guest has ever participated in GPUI layout.
                    let _ = view.raw().set_bounds(wry::Rect {
                        position: wry::dpi::LogicalPosition::new(0., 0.).into(),
                        size: wry::dpi::LogicalSize::new(1024., 768.).into(),
                    });
                    view
                })),
                Err(error) => {
                    eprintln!("[vitre] browser preview host failed: {error}");
                    None
                }
            }
        };

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let webview = None;

        let host_task = cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = host_rx.recv().await {
                if this
                    .update_in(cx, |panel, window, cx| {
                        panel.apply_host_event(event, window, cx)
                    })
                    .is_err()
                {
                    return;
                }
            }
        });

        let address_for_submit = address.clone();
        let subscriptions = vec![cx.subscribe_in(
            &address,
            window,
            move |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    let value = address_for_submit.read(cx).value().to_string();
                    this.navigate(value, window, cx);
                }
            },
        )];

        let mut panel = Self {
            client,
            thread_id,
            tab_id,
            address,
            webview,
            focus_handle: cx.focus_handle(),
            url: SharedString::default(),
            title: "Browser".into(),
            loading: false,
            load_started: None,
            recovery_attempted: false,
            can_go_back: false,
            can_go_forward: false,
            zoom: 1.0,
            local_servers: Vec::new(),
            error: None,
            viewport: serde_json::from_value(serde_json::json!({"_tag":"fill"}))
                .expect("fill viewport"),
            color_scheme: "system",
            viewport_scale: 1.,
            viewport_frame: None,
            pending_evaluations: std::collections::HashMap::new(),
            recording: None,
            tasks: vec![host_task],
            _subscriptions: subscriptions,
        };
        if let Some(snapshot) = initial_snapshot {
            panel.apply_snapshot(snapshot, true, cx);
        } else {
            panel.load_session(cx);
        }
        panel.watch_local_servers(cx);
        panel.watch_preview_events(cx);
        panel.watch_health(cx);
        panel
    }

    fn watch_health(&mut self, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        let task=cx.spawn(async move|this,cx|loop{
            executor.timer(std::time::Duration::from_secs(10)).await;
            let Ok(rx)=this.update(cx,|p,cx|{
                if p.url.is_empty() || !p.webview.as_ref().is_some_and(|v|v.read(cx).visible()){return None;}
                if p.loading && p.load_started.is_some_and(|at|at.elapsed()<std::time::Duration::from_secs(30)){return None;}
                Some(p.execute_page("evaluate",serde_json::json!({"expression":"true"}),cx))
            })else{return};
            let Some(rx)=rx else{continue};
            let result=tokio::select!{result=rx=>result,_=executor.timer(std::time::Duration::from_secs(5))=>{
                let _=this.update(cx,|p,cx|{
                    p.loading=false;
                    if !p.recovery_attempted {
                        p.recovery_attempted=true;
                        p.error=Some("The browser stopped responding. Attempting recovery…".into());
                        if let Some(view)=&p.webview{let _=view.read(cx).raw().reload();}
                    }else{p.error=Some("Browser recovery failed. Reload or close and reopen this tab.".into());}
                    cx.notify();
                });continue;
            }};
            if let Ok(Ok(_))=result{let _=this.update(cx,|p,cx|{
                if p.loading && p.load_started.is_some_and(|at|at.elapsed()>std::time::Duration::from_secs(30)){
                    p.loading=false;p.error=Some("Navigation took too long. Check the address or reload the tab.".into());cx.notify();
                }
            });}
        });
        self.tasks.push(task);
    }

    pub(super) fn show(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.webview.clone() {
            view.update(cx, |view, _| {
                if !view.visible() {
                    view.show();
                }
            });
        }
    }

    pub(super) fn hide(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.webview.clone() {
            view.update(cx, |view, _| {
                if view.visible() {
                    view.hide();
                }
            });
        }
    }

    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        self.recovery_attempted = false;
        #[cfg(target_os = "macos")]
        if self.loading
            && let Some(view) = self.webview.as_ref()
        {
            use wry::WebViewExtMacOS as _;
            // The entity is updated on GPUI's main thread; wry owns the WKWebView.
            unsafe { view.read(cx).raw().webview().stopLoading() };
            self.loading = false;
            self.report_status(cx);
            cx.notify();
            return;
        }
        if let Some(view) = self.webview.clone()
            && let Err(error) = view.read(cx).raw().reload()
        {
            self.error = Some(format!("Refresh failed: {error}").into());
            cx.notify();
        }
    }

    pub(super) fn focus_url(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.address.read(cx).focus_handle(cx).focus(window, cx);
        self.address
            .update(cx, |address, cx| address.select_all(window, cx));
    }

    pub(super) fn change_zoom(&mut self, delta: i32, cx: &mut Context<Self>) {
        const LEVELS: &[f64] = &[0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];
        let current = LEVELS
            .iter()
            .position(|value| (*value - self.zoom).abs() < 0.001)
            .unwrap_or(5) as i32;
        let next = (current + delta).clamp(0, LEVELS.len() as i32 - 1) as usize;
        self.set_zoom(LEVELS[next], cx);
    }

    pub(super) fn set_zoom(&mut self, zoom: f64, cx: &mut Context<Self>) {
        self.zoom = zoom;
        if let Some(view) = self.webview.clone() {
            let _ = view.read(cx).raw().zoom(zoom * self.viewport_scale);
        }
        cx.notify();
    }

    fn navigate(&mut self, raw: String, _window: &mut Window, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let Some(url) = normalize_url(&raw) else {
            self.error = Some("Enter a valid http(s) URL.".into());
            cx.notify();
            return;
        };
        let client = self.client.clone();
        let thread_id = self.thread_id.clone();
        let tab_id = self.tab_id.clone();
        self.loading = true;
        self.load_started = Some(std::time::Instant::now());
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result: Result<PreviewSessionSnapshot, String> = match tab_id {
                Some(tab_id) => client
                    .call::<PreviewNavigate>(&PreviewNavigateInput {
                        thread_id,
                        tab_id,
                        url: TrimmedNonEmptyString(url),
                        resolved_title: None,
                    })
                    .await
                    .map_err(|error| error.user_message()),
                None => client
                    .call::<PreviewOpen>(&PreviewOpenInput {
                        thread_id,
                        url: Some(Some(TrimmedNonEmptyString(url))),
                    })
                    .await
                    .map_err(|error| error.user_message()),
            };
            let _ = this.update(cx, |panel, cx| match result {
                Ok(snapshot) => panel.apply_snapshot(snapshot, true, cx),
                Err(error) => {
                    panel.loading = false;
                    panel.error = Some(error.into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn load_session(&mut self, cx: &mut Context<Self>) {
        let Some(tab_id) = self.tab_id.clone() else {
            return;
        };
        let client = self.client.clone();
        let thread_id = self.thread_id.clone();
        cx.spawn(async move |this, cx| {
            let result = client
                .call::<PreviewList>(&PreviewListInput { thread_id })
                .await;
            let _ = this.update(cx, |panel, cx| match result {
                Ok(list) => {
                    if let Some(snapshot) = list.sessions.into_iter().find(|s| s.tab_id == tab_id) {
                        panel.apply_snapshot(snapshot, true, cx);
                    }
                }
                Err(error) => {
                    panel.error = Some(error.user_message().into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn watch_local_servers(&mut self, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let task = cx.spawn(async move |this, cx| {
            let mut sessions = client.sessions();
            loop {
                let Some(handle) = sessions.borrow_and_update().clone() else {
                    if sessions.changed().await.is_err() {
                        return;
                    }
                    continue;
                };
                let Ok(mut sub) = handle
                    .session
                    .subscribe_typed::<SubscribeDiscoveredLocalServers>(&serde_json::json!({}))
                else {
                    if sessions.changed().await.is_err() {
                        return;
                    }
                    continue;
                };
                loop {
                    tokio::select! {
                        event = sub.next() => match event {
                            Some(TypedStreamEvent::Values(values)) => {
                                for value in values {
                                    if this.update(cx, |panel, cx| {
                                        panel.local_servers = value.servers;
                                        cx.notify();
                                    }).is_err() { return; }
                                }
                                if sub.ack().is_err() { break; }
                            }
                            _ => break,
                        },
                        changed = sessions.changed() => {
                            if changed.is_err() { return; }
                            break;
                        }
                    }
                }
            }
        });
        self.tasks.push(task);
    }

    fn watch_preview_events(&mut self, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let task = cx.spawn(async move |this,cx| {
            let mut sessions=client.sessions();
            loop {
                let handle=sessions.borrow_and_update().clone();
                let Some(handle)=handle else {
                    if sessions.changed().await.is_err(){return;}
                    continue;
                };
                let Ok(mut stream)=handle.session.subscribe_typed::<SubscribePreviewEvents>(&serde_json::json!({})) else {return;};
                loop {
                    tokio::select! {
                        changed=sessions.changed()=>{if changed.is_err(){return;} break;},
                        event=stream.next()=>match event {
                            Some(TypedStreamEvent::Values(values))=> {
                                for event in values {
                                    if this.update(cx,|panel,cx| {
                                        match event {
                                            PreviewEvent::Navigated{snapshot,..} | PreviewEvent::Resized{snapshot,..}
                                            if panel.tab_id.as_ref()==Some(&snapshot.tab_id) && panel.thread_id.0==snapshot.thread_id.0 => {
                                                let changed=nav_parts(&snapshot.nav_status).is_some_and(|(url,_,_)| url != panel.url.as_ref());
                                                panel.apply_snapshot(snapshot,changed,cx);
                                            }
                                            PreviewEvent::Closed{tab_id,thread_id,..} if panel.tab_id.as_ref()==Some(&tab_id) && panel.thread_id.0==thread_id.0 => {
                                                panel.tab_id=None;panel.url="".into();panel.loading=false;
                                                panel.can_go_back=false;panel.can_go_forward=false;
                                                panel.error=Some("This browser session was closed. Enter a URL to open a new session.".into());
                                                if let Some(view)=panel.webview.as_ref(){let _=view.read(cx).raw().load_html(BLANK_PAGE);}
                                                cx.emit(PreviewPanelEvent::SessionClosed);
                                                cx.notify();
                                            }
                                            PreviewEvent::Failed{tab_id,description,..} if panel.tab_id.as_ref()==Some(&tab_id)=>{
                                                panel.loading=false;panel.error=Some(description.0.into());cx.notify();
                                            }
                                            _=>{}
                                        }
                                    }).is_err(){return;}
                                }
                                if stream.ack().is_err(){break;}
                            }
                            _=>break,
                        }
                    }
                }
                cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
            }
        });
        self.tasks.push(task);
    }

    pub(super) fn resize_viewport(
        &mut self,
        viewport: PreviewViewportSetting,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_id) = self.tab_id.clone() else {
            self.viewport = viewport;
            cx.notify();
            return;
        };
        let client = self.client.clone();
        let input = PreviewResizeInput {
            thread_id: self.thread_id.clone(),
            tab_id,
            viewport,
        };
        cx.spawn(async move |this, cx| {
            let result = client.call::<PreviewResize>(&input).await;
            let _ = this.update(cx, |panel, cx| match result {
                Ok(snapshot) => panel.apply_snapshot(snapshot, false, cx),
                Err(error) => {
                    panel.error = Some(error.user_message().into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn set_color_scheme(&mut self, scheme: &'static str, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        if let Some(view) = self.webview.as_ref() {
            use objc2_app_kit::{NSAppearance, NSAppearanceCustomization as _};
            use objc2_foundation::NSString;
            use wry::WebViewExtMacOS as _;
            let name = match scheme {
                "dark" => Some("NSAppearanceNameDarkAqua"),
                "light" => Some("NSAppearanceNameAqua"),
                _ => None,
            };
            let appearance =
                name.and_then(|name| NSAppearance::appearanceNamed(&NSString::from_str(name)));
            view.read(cx)
                .raw()
                .webview()
                .setAppearance(appearance.as_deref());
        }
        self.color_scheme = scheme;
        cx.notify();
    }

    fn custom_viewport(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (w, h) = viewport_size(&self.viewport).unwrap_or((1024, 768));
        let width = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(w.to_string())
                .placeholder("Width (CSS pixels)")
        });
        let height = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(h.to_string())
                .placeholder("Height (CSS pixels)")
        });
        let owner = cx.entity().downgrade();
        self.hide(cx);
        window.open_dialog(cx, move |dialog, _, _| {
            let width = width.clone();
            let height = height.clone();
            let owner = owner.clone();
            dialog
                .title("Custom viewport")
                .child(
                    v_flex()
                        .gap_2()
                        .child("Width and height: 240–3840 CSS pixels; maximum area 3840 × 2160.")
                        .child(Input::new(&width))
                        .child(Input::new(&height)),
                )
                .on_ok(move |_, _, cx| {
                    let Some((w, h)) =
                        parse_dimensions(&width.read(cx).value(), &height.read(cx).value())
                    else {
                        return false;
                    };
                    let _ = owner.update(cx, |p, cx| {
                        p.resize_viewport(
                            serde_json::from_value(
                                serde_json::json!({"_tag":"freeform","width":w,"height":h}),
                            )
                            .unwrap(),
                            cx,
                        )
                    });
                    true
                })
        });
    }

    pub(super) fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    fn screenshot(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.webview.as_ref() else {
            return;
        };
        let result = super::preview_capture::snapshot(view.read(cx).raw());
        let path = crate::vitre_home().join("preview-artifacts").join(format!(
            "preview-{}.png",
            chrono::Utc::now().timestamp_micros()
        ));
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this,cx| {
            let result=async {
                let png=tokio::select! {result=result=>result.map_err(|_|"Webview closed".to_string())??,_=executor.timer(std::time::Duration::from_secs(15))=>return Err("Snapshot timed out".to_string())};
                executor.spawn(async move {
                    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e|e.to_string())?;
                    std::fs::write(&path,png).map_err(|e|e.to_string())?;
                    Ok::<_,String>(path)
                }).await
            }.await;
            let _=this.update(cx,|panel,cx| {match result {Ok(path)=>cx.reveal_path(&path),Err(error)=>panel.error=Some(error.into())}cx.notify();});
        }).detach();
    }

    fn apply_snapshot(
        &mut self,
        snapshot: PreviewSessionSnapshot,
        navigate: bool,
        cx: &mut Context<Self>,
    ) {
        let was_new = self.tab_id.is_none();
        if let Some(Some(viewport)) = snapshot.viewport.clone() {
            self.viewport = viewport;
        }
        if let Some(view) = &self.webview
            && !view.read(cx).visible()
        {
            let (w, h) = viewport_size(&self.viewport).unwrap_or((1024, 768));
            self.viewport_scale = 1.;
            let _ = view.read(cx).raw().zoom(self.zoom);
            let _ = view.read(cx).raw().set_bounds(wry::Rect {
                position: wry::dpi::LogicalPosition::new(0., 0.).into(),
                size: wry::dpi::LogicalSize::new(w as f64 * self.zoom, h as f64 * self.zoom).into(),
            });
        }
        self.tab_id = Some(snapshot.tab_id.clone());
        // A newly constructed native guest has no restored back/forward list.
        // The server flags describe the old guest, not this one's history.
        if navigate && let Some((url, title, _)) = nav_parts(&snapshot.nav_status) {
            self.url = url.clone().into();
            self.title = title.into();
            if let Some(view) = self.webview.clone() {
                self.loading = true;
                self.load_started = Some(std::time::Instant::now());
                self.error = None;
                view.update(cx, |view, _| view.load_url(&url));
            }
        }
        if was_new {
            cx.emit(PreviewPanelEvent::SessionOpened(snapshot.tab_id));
        }
        cx.notify();
    }

    fn apply_host_event(&mut self, event: HostEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            HostEvent::Message(message) => {
                match message["kind"].as_str() {
                    Some("automation") => {
                        if let Some(tx) = message["id"]
                            .as_str()
                            .and_then(|id| self.pending_evaluations.remove(id))
                        {
                            let result = if message["ok"].as_bool() == Some(true) {
                                Ok(message["result"].clone())
                            } else {
                                Err(message["error"]
                                    .as_str()
                                    .unwrap_or("Page operation failed")
                                    .to_string())
                            };
                            let _ = tx.send(result);
                        }
                    }
                    Some("picked") => {
                        let text = format!(
                            "Preview element from {}\nURL: {}\nSelector: {}\nText: {}\nHTML: {}\nStyles: {}",
                            message["title"].as_str().unwrap_or("Preview"),
                            message["url"].as_str().unwrap_or(""),
                            message["selector"].as_str().unwrap_or(""),
                            message["text"].as_str().unwrap_or(""),
                            message["html"].as_str().unwrap_or(""),
                            message["styles"]
                        );
                        cx.emit(PreviewPanelEvent::Picked(text));
                        if let Some(rx) = self.native_snapshot(cx) {
                            let executor = cx.background_executor().clone();
                            cx.spawn(async move|this,cx|{
                                let png=tokio::select!{png=rx=>png,_=executor.timer(std::time::Duration::from_secs(10))=>return};
                                if let Ok(Ok(png))=png{let _=this.update(cx,|_,cx|cx.emit(PreviewPanelEvent::PickedImage(png)));}
                            }).detach();
                        }
                    }
                    _ => {}
                }
                return;
            }
            HostEvent::Loading(url) => {
                if url != "about:blank" {
                    self.url = url.into();
                    self.loading = true;
                    self.load_started = Some(std::time::Instant::now());
                    self.error = None;
                }
            }
            HostEvent::Loaded(url) => {
                if url == "about:blank" && !self.url.is_empty() {
                    return;
                }
                if url != "about:blank" {
                    self.url = url.into();
                }
                self.loading = false;
                self.load_started = None;
                if let Some(view) = self.webview.clone() {
                    let _ = view.read(cx).raw().evaluate_script(PAGE_BRIDGE);
                }
            }
            HostEvent::Page { url, title } => {
                if url != "about:blank" {
                    self.url = url.into();
                    self.title = title.into();
                }
            }
        }
        #[cfg(target_os = "macos")]
        if let Some(view) = self.webview.as_ref() {
            use wry::WebViewExtMacOS as _;
            let native = view.read(cx).raw().webview();
            // Read native history rather than guessing from URL changes (which
            // breaks redirects, replaceState, and same-document navigation).
            self.can_go_back = unsafe { native.canGoBack() };
            self.can_go_forward = unsafe { native.canGoForward() };
        }
        if !self.address.read(cx).focus_handle(cx).is_focused(window) {
            let value = self.url.to_string();
            self.address
                .update(cx, |input, cx| input.set_value(value, window, cx));
        }
        self.report_status(cx);
        cx.notify();
    }

    pub(super) fn execute_page(
        &mut self,
        operation: &str,
        input: serde_json::Value,
        cx: &mut Context<Self>,
    ) -> tokio::sync::oneshot::Receiver<Result<serde_json::Value, String>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        #[cfg(target_os = "macos")]
        if operation == "press" {
            let result = self
                .webview
                .as_ref()
                .ok_or("Browser unavailable".to_string())
                .and_then(|view| {
                    if !view.read(cx).visible(){return Err("Native keyboard input requires a visible tab. Open the tab before pressing keys.".into());}
                    let modifiers = input["modifiers"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|s| s.as_str().map(str::to_string))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    super::preview_capture::press(
                        view.read(cx).raw(),
                        input["key"].as_str().unwrap_or(""),
                        &modifiers,
                    )
                    .map(|_| serde_json::Value::Null)
                });
            if let Err(error) = result {
                let _ = tx.send(Err(error));
                return rx;
            }
            // Key delivery crosses into WebKit's content process. Do not send
            // the next action before its default editing operation completes.
            return self.execute_page("evaluate",serde_json::json!({"expression":"new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(null))))"}),cx);
        }
        // Discard canceled/timed-out requests before adding another one.
        self.pending_evaluations.retain(|_, tx| !tx.is_closed());
        let Some(view) = self.webview.as_ref() else {
            let _ = tx.send(Err("Browser host unavailable".into()));
            return rx;
        };
        let id = super::fresh_id("preview-eval");
        let script = format!(
            "globalThis.__vitreHost.run({},{},{})",
            serde_json::json!(id),
            serde_json::json!(operation),
            input
        );
        self.pending_evaluations.insert(id.clone(), tx);
        if let Err(error) = view.read(cx).raw().evaluate_script(&script)
            && let Some(tx) = self.pending_evaluations.remove(&id)
        {
            let _ = tx.send(Err(error.to_string()));
        }
        rx
    }

    pub(super) fn native_snapshot(
        &self,
        cx: &Context<Self>,
    ) -> Option<tokio::sync::oneshot::Receiver<Result<Vec<u8>, String>>> {
        self.webview
            .as_ref()
            .map(|view| super::preview_capture::snapshot(view.read(cx).raw()))
    }

    pub(super) fn start_recording(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        if self.recording.is_some() {
            return Err("This tab is already recording".into());
        }
        let tab_id = self
            .tab_id
            .as_ref()
            .ok_or("Open a page before recording")?
            .0
            .clone();
        let recording = super::preview_recording::Recording::start(tab_id.clone(), cx);
        let status =
            serde_json::json!({"tabId":tab_id,"recording":true,"startedAt":recording.started_at});
        self.recording = Some(recording);
        cx.notify();
        Ok(status)
    }

    pub(super) fn stop_recording(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<serde_json::Value, String>>, String> {
        let recording = self.recording.take().ok_or("This tab is not recording")?;
        cx.notify();
        Ok(recording.stop())
    }

    fn toggle_recording(&mut self, cx: &mut Context<Self>) {
        if self.recording.is_none() {
            if let Err(error) = self.start_recording(cx) {
                self.error = Some(error.into());
                cx.notify();
            }
            return;
        }
        let Ok(finished) = self.stop_recording(cx) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = finished
                .await
                .unwrap_or_else(|_| Err("Recording was interrupted".into()));
            let _ = this.update(cx, |panel, cx| {
                match result {
                    Ok(artifact) => {
                        if let Some(path) = artifact["path"].as_str() {
                            cx.reveal_path(std::path::Path::new(path));
                        }
                    }
                    Err(error) => panel.error = Some(error.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn automation_status(&self, cx: &Context<Self>) -> serde_json::Value {
        serde_json::json!({"available":self.webview.is_some(),"visible":self.webview.as_ref().is_some_and(|v|v.read(cx).visible()),"tabId":self.tab_id,"url":self.url.as_ref(),"title":self.title.as_ref(),"loading":self.loading,"error":self.error.as_ref().map(|e|e.as_ref()),"viewportSetting":self.viewport})
    }

    pub(super) fn load_automation_snapshot(
        &mut self,
        snapshot: PreviewSessionSnapshot,
        cx: &mut Context<Self>,
    ) {
        self.apply_snapshot(snapshot, true, cx);
    }
    pub(super) fn apply_automation_metadata(
        &mut self,
        snapshot: PreviewSessionSnapshot,
        cx: &mut Context<Self>,
    ) {
        self.apply_snapshot(snapshot, false, cx);
    }

    fn report_status(&self, cx: &mut Context<Self>) {
        let Some(tab_id) = self.tab_id.clone() else {
            return;
        };
        let url = self.url.to_string();
        if url.is_empty() {
            return;
        }
        let client = self.client.clone();
        let input = PreviewReportStatusInput {
            thread_id: self.thread_id.clone(),
            tab_id,
            nav_status: if self.loading {
                PreviewNavStatus::Loading {
                    url: TrimmedNonEmptyString(url),
                    title: self.title.to_string(),
                }
            } else {
                PreviewNavStatus::Success {
                    url: TrimmedNonEmptyString(url),
                    title: self.title.to_string(),
                }
            },
            can_go_back: self.can_go_back,
            can_go_forward: self.can_go_forward,
        };
        cx.spawn(async move |_, _| {
            let _ = client.call::<PreviewReportStatus>(&input).await;
        })
        .detach();
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.webview.clone() {
            view.update(cx, |view, _| {
                let _ = view.back();
            });
        }
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.webview.clone() {
            let _ = view.read(cx).raw().evaluate_script("history.forward()");
        }
    }
}

impl Focusable for PreviewPanel {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl gpui::EventEmitter<PreviewPanelEvent> for PreviewPanel {}

impl Render for PreviewPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let url = self.url.clone();
        let local_servers = self.local_servers.clone();
        let has_page = !url.is_empty();
        let zoom = format!("{}%", (self.zoom * 100.).round());
        let viewport = viewport_size(&self.viewport);
        let owner = cx.entity().downgrade();
        let layout_owner = owner.clone();
        let viewport_frame = self.viewport_frame;
        v_flex()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus_handle)
            .key_context("Preview")
            .on_action(cx.listener(|p,action:&SetViewport,_,cx|{if let Ok(v)=serde_json::from_value(action.value.clone()){p.resize_viewport(v,cx);}}))
            .on_action(cx.listener(|p,action:&SetAppearance,_,cx|p.set_color_scheme(match action.value.as_str(){"dark"=>"dark","light"=>"light",_=>"system"},cx)))
            .child(
                h_flex()
                    .h(px(40.))
                    .px_2()
                    .gap_1()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(Button::new("preview-back").label("←").ghost().xsmall().disabled(!self.can_go_back).on_click(cx.listener(|this, _, _, cx| this.go_back(cx))))
                    .child(Button::new("preview-forward").label("→").ghost().xsmall().disabled(!self.can_go_forward).on_click(cx.listener(|this, _, _, cx| this.go_forward(cx))))
                    .child(Button::new("preview-refresh").label(if self.loading { "×" } else { "↻" }).ghost().xsmall().disabled(!has_page).on_click(cx.listener(|this, _, _, cx| this.refresh(cx))))
                    .child(div().flex_1().min_w_0().child(Input::new(&self.address)))
                    .child(Button::new("preview-external").icon(IconName::ExternalLink).ghost().xsmall().disabled(!has_page).tooltip("Open in default browser").on_click(cx.listener(move |_, _, _, cx| { if !url.is_empty() { cx.open_url(&url); } })))
                    .child(Button::new("preview-zoom-out").label("−").ghost().xsmall().on_click(cx.listener(|this, _, _, cx| this.change_zoom(-1, cx))))
                    .child(div().w(px(40.)).text_xs().text_center().text_color(cx.theme().muted_foreground).child(zoom))
                    .child(Button::new("preview-zoom-in").label("+").ghost().xsmall().on_click(cx.listener(|this, _, _, cx| this.change_zoom(1, cx)))),
            )
            .children(self.error.clone().map(|error| div().px_3().py_1().text_xs().text_color(cx.theme().danger).child(error)))
            .child(h_flex().h(px(32.)).flex_none().gap_2().px_2().items_center().overflow_x_scrollbar()
                .child(Button::new("preview-device").label(viewport.map(|(w,h)|format!("{w} × {h}")).unwrap_or_else(||"Responsive".into())).ghost().xsmall().on_click(cx.listener(|p,event:&gpui::ClickEvent,window,cx|{
                    p.focus_handle.focus(window,cx);
                    let mut menu=gpui_component::native_menu::NativeMenu::new().menu("Responsive",Box::new(SetViewport{value:serde_json::json!({"_tag":"fill"})}));
                    for &(id,label,w,h) in DEVICE_PRESETS{menu=menu.menu(label,Box::new(SetViewport{value:serde_json::json!({"_tag":"preset","presetId":id,"width":w,"height":h})}));}
                    menu.show(event.position(),window,cx);
                })))
                .child(Button::new("preview-custom").label("Size…").ghost().xsmall().on_click(cx.listener(|p,_,w,cx|p.custom_viewport(w,cx))))
                .child(Button::new("preview-rotate").label("Rotate").ghost().xsmall().disabled(viewport.is_none()).on_click(cx.listener(|panel,_,_,cx| {
                    if let Some((w,h))=viewport_size(&panel.viewport){panel.resize_viewport(serde_json::from_value(serde_json::json!({"_tag":"freeform","width":h,"height":w})).unwrap(),cx);}
                })))
                .child(Button::new("preview-appearance").label(self.color_scheme).ghost().xsmall().on_click(cx.listener(|p,event:&gpui::ClickEvent,window,cx|{
                    p.focus_handle.focus(window,cx);let mut menu=gpui_component::native_menu::NativeMenu::new();
                    for scheme in ["system","light","dark"]{menu=menu.menu_with_check(scheme,p.color_scheme==scheme,Box::new(SetAppearance{value:scheme.into()}));}
                    menu.show(event.position(),window,cx);
                })))
                .child(Button::new("preview-screenshot").label("Screenshot").ghost().xsmall().disabled(!has_page).on_click(cx.listener(|panel,_,_,cx|panel.screenshot(cx))))
                .child(Button::new("preview-record").label(if self.recording.is_some(){"Stop recording"}else{"Record GIF"}).ghost().xsmall().disabled(!has_page).on_click(cx.listener(|panel,_,_,cx|panel.toggle_recording(cx))))
                .child(Button::new("preview-mini").label("Mini / dock").ghost().xsmall().on_click(cx.listener(|_,_,_,cx|cx.emit(PreviewPanelEvent::ToggleMiniPlayer))))
                .child(Button::new("preview-pick").label("Pick element").ghost().xsmall().disabled(!has_page).on_click(cx.listener(|panel,_,_,cx| {
                    if let Some(view)=panel.webview.as_ref(){let _=view.read(cx).raw().evaluate_script("globalThis.__vitreHost.pick()");}
                }))))
            .children((!has_page && !local_servers.is_empty()).then(|| {
                let mut row = h_flex().gap_1().px_2().py_2().overflow_x_scrollbar();
                for server in local_servers {
                    let target = server.url.0.clone();
                    row = row.child(Button::new(SharedString::from(format!("preview-local-{}", server.port))).label(format!("{}:{}", server.host.0, server.port)).small().on_click(cx.listener(move |this, _, window, cx| this.navigate(target.clone(), window, cx))));
                }
                row
            }))
            .child(match self.webview.clone() {
                Some(view) => div().flex().flex_1().min_h_0().overflow_hidden().items_center().justify_center()
                    .relative()
                    .child(canvas(move |bounds,_,cx| {
                        let _=layout_owner.update(cx,|panel,cx| {
                            let (frame,scale)=fit_viewport(viewport_size(&panel.viewport),f32::from(bounds.size.width),f32::from(bounds.size.height));
                            if panel.viewport_frame!=frame || (panel.viewport_scale-scale).abs()>0.001 {
                                panel.viewport_frame=frame;panel.viewport_scale=scale;
                                if let Some(view)=panel.webview.as_ref(){let _=view.read(cx).raw().zoom(panel.zoom*scale);}
                                cx.notify();
                            }
                        });
                    },|_,_,_,_|{}).absolute().top_0().left_0().size_full())
                    .child(div().flex().size_full().when_some(viewport_frame,|el,(w,h)|el.w(px(w)).h(px(h)).flex_none()).child(view)).into_any_element(),
                None => v_flex().flex_1().items_center().justify_center().text_sm().text_color(cx.theme().muted_foreground).child("Embedded preview is unavailable on this platform. Open the URL in your default browser.").into_any_element(),
            })
    }
}

const DEVICE_PRESETS: &[(&str, &str, i64, i64)] = &[
    ("iphone-se", "iPhone SE", 375, 667),
    ("iphone-xr", "iPhone XR", 414, 896),
    ("iphone-12-pro", "iPhone 12 Pro", 390, 844),
    ("iphone-14-pro-max", "iPhone 14 Pro Max", 430, 932),
    ("pixel-7", "Pixel 7", 412, 915),
    ("samsung-galaxy-s8-plus", "Samsung Galaxy S8+", 360, 740),
    (
        "samsung-galaxy-s20-ultra",
        "Samsung Galaxy S20 Ultra",
        412,
        915,
    ),
    ("ipad-mini", "iPad Mini", 768, 1024),
    ("ipad-air", "iPad Air", 820, 1180),
    ("ipad-pro", "iPad Pro", 1024, 1366),
    ("surface-pro-7", "Surface Pro 7", 912, 1368),
    ("surface-duo", "Surface Duo", 540, 720),
    ("galaxy-z-fold-5", "Galaxy Z Fold 5", 344, 882),
    ("asus-zenbook-fold", "Asus Zenbook Fold", 853, 1280),
    ("samsung-galaxy-a51-71", "Samsung Galaxy A51/71", 412, 914),
    ("nest-hub", "Nest Hub", 1024, 600),
    ("nest-hub-max", "Nest Hub Max", 1280, 800),
];

pub(super) fn preset_viewport(id: &str) -> Option<serde_json::Value> {
    DEVICE_PRESETS.iter().find(|p|p.0==id).map(|&(id,_,width,height)|serde_json::json!({"_tag":"preset","presetId":id,"width":width,"height":height}))
}

fn parse_dimensions(width: &str, height: &str) -> Option<(i64, i64)> {
    let w = width.trim().parse().ok()?;
    let h = height.trim().parse().ok()?;
    ((240..=3840).contains(&w) && (240..=3840).contains(&h) && w * h <= 3840 * 2160)
        .then_some((w, h))
}

fn viewport_size(viewport: &PreviewViewportSetting) -> Option<(i64, i64)> {
    match viewport {
        PreviewViewportSetting::PreviewViewportSize(v) => Some((v.width, v.height)),
        PreviewViewportSetting::PreviewViewportSize1(v) => Some((v.width, v.height)),
        _ => None,
    }
}

fn fit_viewport(
    viewport: Option<(i64, i64)>,
    width: f32,
    height: f32,
) -> (Option<(f32, f32)>, f64) {
    let Some((w, h)) = viewport else {
        return (None, 1.);
    };
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    let scale = (width.max(1.) / w).min(height.max(1.) / h).min(1.);
    (Some((w * scale, h * scale)), scale as f64)
}

fn nav_parts(status: &PreviewNavStatus) -> Option<(String, String, bool)> {
    match status {
        PreviewNavStatus::Loading { url, title } => Some((url.0.clone(), title.clone(), true)),
        PreviewNavStatus::Success { url, title } => Some((url.0.clone(), title.clone(), false)),
        PreviewNavStatus::LoadFailed { url, title, .. } => {
            Some((url.0.clone(), title.clone(), false))
        }
        PreviewNavStatus::Idle {} | PreviewNavStatus::Unknown(_) => None,
    }
}

pub(super) fn normalize_url(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() || value.len() > 2048 {
        return None;
    }
    // A colon without an HTTP scheme is only valid for a numeric host port
    // (or bracketed IPv6), never an executable/custom URL scheme.
    if !value.starts_with("http://") && !value.starts_with("https://") {
        let authority = value.split(['/', '?', '#']).next()?;
        if !authority.ends_with(']')
            && let Some((_, port)) = authority.rsplit_once(':')
        {
            port.parse::<u16>().ok().filter(|port| *port > 0)?;
        }
    }
    let normalized = if value.starts_with("http://") || value.starts_with("https://") {
        value.to_string()
    } else if value.starts_with("localhost")
        || value.starts_with("127.0.0.1")
        || value.starts_with("[::1]")
    {
        format!("http://{value}")
    } else {
        format!("https://{value}")
    };
    normalized
        .parse::<wry::http::Uri>()
        .ok()
        .map(|_| normalized)
}

#[cfg(test)]
mod tests {
    use super::normalize_url;

    #[test]
    fn normalizes_loopback_and_public_hosts() {
        assert_eq!(
            normalize_url("localhost:5173/foo").as_deref(),
            Some("http://localhost:5173/foo")
        );
        assert_eq!(normalize_url("t3.chat").as_deref(), Some("https://t3.chat"));
        assert_eq!(
            normalize_url("https://example.com").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(normalize_url(" "), None);
    }
}
