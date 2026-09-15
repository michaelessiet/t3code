//! Geometry shared by native navigation surfaces (logical pixels).
pub(crate) const ICON: f32 = 16.;
pub(crate) const TREE_ROW: f32 = 28.;
pub(crate) const TREE_INDENT: f32 = 16.;
pub(crate) const CHROME_HEIGHT: f32 = 44.;

#[cfg(test)]
mod tests {
    use gpui::{
        Context, Entity, Modifiers, MouseButton, Pixels, TestAppContext, Window, div, point,
        prelude::*, px,
    };
    use gpui_component::text::{TextView, TextViewState};

    struct DialogProbe;
    impl Render for DialogProbe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .children(gpui_component::Root::render_dialog_layer(window, cx))
        }
    }

    #[gpui::test]
    fn dialog_resizes_both_axes_and_honors_reduced_motion(cx: &mut TestAppContext) {
        use gpui_component::WindowExt as _;
        use std::{cell::Cell, rc::Rc, time::Duration};
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|w, cx| {
            let view = cx.new(|_| DialogProbe);
            gpui_component::Root::new(view, w, cx)
        });
        let dimensions = Rc::new(Cell::new((320., 120.)));
        cx.update(|w, cx| {
            let dimensions = dimensions.clone();
            w.open_dialog(cx, move |dialog, _, _| {
                let (width, height) = dimensions.get();
                dialog
                    .w(px(width))
                    .p_0()
                    .close_button(false)
                    .content(move |body, _, _| body.child(div().h(px(height))))
            });
        });
        let frames = |cx: &mut gpui::VisualTestContext, count| {
            for _ in 0..count {
                cx.executor().advance_clock(Duration::from_millis(20));
                cx.update(|w, cx| {
                    w.refresh();
                    w.draw(cx).clear(cx);
                });
            }
        };
        frames(cx, 40);
        let before = cx.debug_bounds("dialog-surface-0").unwrap();
        assert!(before.size.height > px(120.));
        dimensions.set((620., 260.));
        frames(cx, 4);
        let during = cx.debug_bounds("dialog-surface-0").unwrap();
        assert!(
            during.size.width > before.size.width && during.size.width < px(620.),
            "width should interpolate: {during:?}"
        );
        assert!(
            during.size.height > before.size.height && during.size.height < px(260.),
            "height should interpolate: {during:?}"
        );
        frames(cx, 40);
        let after = cx.debug_bounds("dialog-surface-0").unwrap();
        assert_eq!(after.size.width, px(620.));
        assert!(after.size.height >= px(260.));
        cx.update(|_, cx| cx.set_reduce_motion(true));
        dimensions.set((360., 90.));
        frames(cx, 3);
        let reduced = cx.debug_bounds("dialog-surface-0").unwrap();
        assert_eq!(reduced.size.width, px(360.));
        assert!(reduced.size.height < px(120.));
    }

    struct MarkdownProbe {
        text: Entity<TextViewState>,
        width: Pixels,
        source: bool,
    }
    impl Render for MarkdownProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                div()
                    .w(self.width)
                    .debug_selector(|| "markdown-column".into())
                    .child(
                        TextView::new(&self.text)
                            .selectable(true)
                            .text_size(px(13.))
                            .selection_format(if self.source {
                                gpui_component::text::SelectionFormat::Source
                            } else {
                                gpui_component::text::SelectionFormat::Plain
                            }),
                    ),
            )
        }
    }

    #[gpui::test]
    fn inline_code_is_padded_wrapped_and_copies_without_padding(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (host, cx) = cx.add_window_view(|w, cx| {
            let content = cx.new(|cx| MarkdownProbe {
                text: cx.new(|cx| TextViewState::markdown("Before `café 世界` after", cx)),
                width: px(420.),
                source: false,
            });
            gpui_component::Root::new(content, w, cx)
        });
        let root = host.read_with(cx, |h, _| {
            h.view().clone().downcast::<MarkdownProbe>().unwrap()
        });
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let chip = cx
            .debug_bounds("inline-code-1")
            .expect("code chip is rendered");
        assert!(chip.size.width > px(20.) && chip.size.height > px(16.));
        assert!(
            cx.debug_bounds("inline-code-content-1")
                .unwrap()
                .size
                .height
                <= chip.size.height - px(4.),
            "fitting code must not wrap or overflow its pill"
        );
        let column = cx.debug_bounds("markdown-column").unwrap();
        cx.simulate_mouse_down(
            point(column.left() + px(1.), chip.center().y),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.simulate_mouse_move(
            point(column.right() - px(1.), chip.bottom() + px(16.)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        cx.simulate_mouse_up(
            point(column.right() - px(1.), chip.bottom() + px(16.)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(
            root.read_with(cx, |r, cx| r.text.read(cx).selected_text())
                .trim(),
            "Before café 世界 after"
        );
        root.update(cx, |r, cx| {
            r.source = true;
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(
            root.read_with(cx, |r, cx| r.text.read(cx).selected_text())
                .trim(),
            "Before `café 世界` after"
        );
        root.update(cx, |r, cx| {
            r.width = px(50.);
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let wrapped = cx.debug_bounds("inline-code-1").expect("wrapped code chip");
        assert!(wrapped.size.width <= px(50.));
        assert!(
            wrapped.size.height > chip.size.height,
            "long inline code wraps within the column"
        );
    }

    #[gpui::test]
    fn multiline_inline_code_renders_without_violating_gpui_line_shaping(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let content = cx.new(|cx| MarkdownProbe {
                text: cx.new(|cx| TextViewState::markdown("Before `first\nsecond` after", cx)),
                width: px(420.),
                source: false,
            });
            gpui_component::Root::new(content, window, cx)
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let chip = cx
            .debug_bounds("inline-code-1")
            .expect("multiline source renders as an inline code chip");
        assert!(chip.size.width > px(20.) && chip.size.height > px(16.));
    }

    struct RevealProbe {
        state: Entity<gpui_component::ResizableState>,
        progress: f32,
    }
    impl Render for RevealProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            use gpui_component::{h_resizable, resizable_panel};
            div().w(px(800.)).h(px(400.)).child(
                h_resizable("reveal-test")
                    .with_state(&self.state)
                    .child(
                        resizable_panel()
                            .size(px(240.))
                            .size_range(px(208.)..px(480.))
                            .flex_none()
                            .reveal(self.progress)
                            .child(
                                div()
                                    .size_full()
                                    .debug_selector(|| "revealed-sidebar".into()),
                            ),
                    )
                    .child(
                        resizable_panel()
                            .child(div().size_full().debug_selector(|| "remaining-chat".into())),
                    ),
            )
        }
    }

    #[gpui::test]
    fn panel_reveal_keeps_content_and_saved_width(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (root, cx) = cx.add_window_view(|_, cx| RevealProbe {
            state: cx.new(|_| gpui_component::ResizableState::default()),
            progress: 1.,
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let full = cx.debug_bounds("revealed-sidebar").unwrap();
        let chat = cx.debug_bounds("remaining-chat").unwrap();
        root.update(cx, |r, cx| {
            r.progress = 0.5;
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let half = cx.debug_bounds("revealed-sidebar").unwrap();
        assert_eq!(
            half.size.width, full.size.width,
            "content must slide, not reflow"
        );
        assert!(cx.debug_bounds("remaining-chat").unwrap().left() < chat.left());
        root.update(cx, |r, cx| {
            r.progress = 1.;
            cx.notify();
        });
        cx.update(|w, cx| w.draw(cx).clear(cx));
        assert_eq!(
            cx.debug_bounds("revealed-sidebar").unwrap().size.width,
            full.size.width
        );
    }
}
