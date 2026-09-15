//! S3+S4 spike: gpui-component `Input` (S4 — IME/CJK composition harness)
//! stacked above a wry WKWebView child (S3 — native webview tracking a gpui
//! flex rect through resizes).
//!
//!   cargo run -p vitre-app --example spike_ui
//!
//! Manual pass bars:
//! - S4: focus the input, switch to a Japanese/Chinese/Korean IME, and type —
//!   composition (underlined preedit, candidate window at the caret) must work.
//! - S3: resize the window and drag it between displays — the webview must
//!   track its flex cell without lag, and typing in the page must work.

use gpui::{
    Context, Entity, SharedString, Subscription, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, size,
};
use gpui_component::{
    ActiveTheme as _, Root, h_flex,
    input::{Input, InputEvent, InputState},
    v_flex,
};
use gpui_wry::WebView;

struct SpikeUi {
    input_state: Entity<InputState>,
    echo: SharedString,
    webview: Entity<WebView>,
    _subscriptions: Vec<Subscription>,
}

impl SpikeUi {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("S4: type here with a CJK IME (日本語 / 中文 / 한국어)")
        });

        let webview = cx.new(|cx| {
            use raw_window_handle::HasWindowHandle;
            let window_handle = window.window_handle().expect("window handle");
            let webview = wry::WebViewBuilder::new()
                .with_devtools(cfg!(debug_assertions))
                .build_as_child(&window_handle)
                .expect("build wry child webview");
            WebView::new(webview, window, cx)
        });
        webview.update(cx, |view, _| view.load_url("https://gpui.rs"));

        let subscriptions = vec![cx.subscribe_in(&input_state, window, {
            let input_state = input_state.clone();
            move |this: &mut Self, _, event: &InputEvent, _window, cx| {
                if let InputEvent::Change = event {
                    this.echo = format!("echo: {}", input_state.read(cx).value()).into();
                    cx.notify();
                }
            }
        })];

        Self {
            input_state,
            echo: SharedString::default(),
            webview,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for SpikeUi {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .p_3()
                    .gap_3()
                    .items_center()
                    .child(div().flex_1().child(Input::new(&self.input_state)))
                    .child(div().text_sm().child(self.echo.clone())),
            )
            .child(div().flex_1().p_3().child(self.webview.clone()))
    }
}

fn main() {
    let app = gpui_platform::application().with_assets(gpui_component_assets::Assets);
    app.run(move |cx| {
        gpui_component::init(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1000.), px(700.)), cx)),
            ..Default::default()
        };
        cx.spawn(async move |cx| {
            cx.open_window(options, |window, cx| {
                let view = cx.new(|cx| SpikeUi::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open window");
        })
        .detach();
    });
}
