//! Real GPUI mouse dispatch, including macOS Control-click and nested targets.
use gpui::{
    Context, IntoElement, MouseButton, Render, TestAppContext, Window, div, point, prelude::*, px,
};
use gpui_component::menu::ContextMenuExt as _;
use std::{cell::Cell, rc::Rc};

#[gpui::test]
fn inline_input_owns_secondary_click_inside_a_project_menu(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    for control in [false, cfg!(target_os = "macos")] {
        struct Inline {
            input: gpui::Entity<gpui_base::input::InputState>,
            ancestor: Rc<Cell<usize>>,
        }
        impl Render for Inline {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let ancestor = self.ancestor.clone();
                div()
                    .size_full()
                    .child(div().w(px(200.)).h(px(40.)).child(self.input.clone()))
                    .context_menu(move |menu, _, _| {
                        ancestor.set(ancestor.get() + 1);
                        menu.item(gpui_component::menu::PopupMenuItem::new("Project action"))
                    })
            }
        }
        let native = Rc::new(Cell::new(0));
        let ancestor = Rc::new(Cell::new(0));
        let native_handler = native.clone();
        let ancestor_handler = ancestor.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let input = cx.new(|cx| {
                let mut state =
                    gpui_base::input::InputState::new(window, cx).default_value("Rename me");
                state.on_context_menu(Rc::new(move |_, _, _, _, _| {
                    native_handler.set(native_handler.get() + 1)
                }));
                state
            });
            Inline {
                input,
                ancestor: ancestor_handler,
            }
        });
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let button = if control {
            MouseButton::Left
        } else {
            MouseButton::Right
        };
        let modifiers = gpui::Modifiers {
            control,
            ..Default::default()
        };
        cx.simulate_mouse_down(point(px(20.), px(12.)), button, modifiers);
        cx.simulate_mouse_up(point(px(20.), px(12.)), button, modifiers);
        cx.run_until_parked();
        assert_eq!(
            native.get(),
            1,
            "native text menu must win (control={control})"
        );
        assert_eq!(
            ancestor.get(),
            0,
            "project menu must not open inside an input"
        );
    }
}

#[gpui::test]
fn chat_markdown_context_menu_copies_the_source(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    struct Markdown;
    impl Render for Markdown {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(super::assistant_markdown(
                "context-markdown".into(),
                "Hello **Rust** and `GPUI`.".into(),
            ))
        }
    }
    let (_, cx) = cx.add_window_view(|_, _| Markdown);
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
    let bounds = cx
        .debug_bounds("markdown-context-context-markdown")
        .expect("Markdown hit target is rendered");
    assert!(
        bounds.size.height > px(0.),
        "Markdown must have a visible hit target: {bounds:?}"
    );
    // Skip disabled Copy selection both on initial selection and when wrapping.
    for keys in ["down enter", "down down enter", "up enter"] {
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into()))
        });
        cx.simulate_mouse_down(bounds.center(), MouseButton::Right, Default::default());
        cx.simulate_mouse_up(bounds.center(), MouseButton::Right, Default::default());
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        cx.simulate_keystrokes(keys);
        cx.run_until_parked();
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text())),
            Some("Hello **Rust** and `GPUI`.".into()),
            "menu navigation: {keys}"
        );
        cx.update(|w, cx| w.draw(cx).clear(cx));
    }
}

#[gpui::test]
fn secondary_click_is_owned_by_innermost_target(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    for control_click in [false, cfg!(target_os = "macos")] {
        struct Nested {
            invoked: Rc<Cell<usize>>,
            selected: Rc<Cell<usize>>,
        }
        impl Render for Nested {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let inner = self.invoked.clone();
                let outer = self.invoked.clone();
                let selected = self.selected.clone();
                div()
                    .id("parent")
                    .size_full()
                    .child(
                        div()
                            .id("thread")
                            .w(px(200.))
                            .h(px(100.))
                            .on_click(move |_, _, _| selected.set(selected.get() + 1))
                            .context_menu(move |menu, _, _| {
                                let inner = inner.clone();
                                menu.item(
                                    gpui_component::menu::PopupMenuItem::new("Thread action")
                                        .on_click(move |_, _, _| inner.set(inner.get() + 1)),
                                )
                            }),
                    )
                    .context_menu(move |menu, _, _| {
                        let outer = outer.clone();
                        menu.item(
                            gpui_component::menu::PopupMenuItem::new("Project action")
                                .on_click(move |_, _, _| outer.set(outer.get() + 100)),
                        )
                    })
            }
        }
        let invoked = Rc::new(Cell::new(0));
        let selected = Rc::new(Cell::new(0));
        let (_, cx) = cx.add_window_view({
            let invoked = invoked.clone();
            let selected = selected.clone();
            move |_, _| Nested { invoked, selected }
        });
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        let button = if control_click {
            MouseButton::Left
        } else {
            MouseButton::Right
        };
        let modifiers = gpui::Modifiers {
            control: control_click,
            ..Default::default()
        };
        cx.simulate_mouse_down(point(px(30.), px(30.)), button, modifiers);
        cx.simulate_mouse_up(point(px(30.), px(30.)), button, modifiers);
        cx.run_until_parked();
        cx.update(|w, cx| w.draw(cx).clear(cx));
        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();
        assert_eq!(
            invoked.get(),
            1,
            "only the innermost menu may handle the click"
        );
        assert_eq!(
            selected.get(),
            0,
            "secondary-click must not activate the row"
        );
    }
}
