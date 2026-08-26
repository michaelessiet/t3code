use gpui::{
    App, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    rgb, size,
};

struct VitreShell {
    status: SharedString,
}

impl Render for VitreShell {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .size_full()
            .justify_center()
            .items_center()
            .bg(rgb(0x0a0a0a))
            .text_color(rgb(0xffffff))
            .child(div().text_xl().child("Vitre"))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x888888))
                    .child(self.status.clone()),
            )
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(640.), px(400.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| VitreShell {
                    status: "M0 scaffold — S2 spike".into(),
                })
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
