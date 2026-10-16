#![cfg(feature = "devtools")]

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::{
    App, AppContext as _, Context, Empty, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, MouseUpEvent, ParentElement as _, Pixels, PlatformInput, Point, Render,
    StatefulInteractiveElement as _, Styled as _, TestAppContext, Window, div, img, point, px, rgb,
};
use gpui_kit::{ObjectFit, StyledImage as _};

struct OverlayRoot;

impl Render for OverlayRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(div().id("picker-target").size(px(100.)).bg(rgb(0x333333)))
            .child(div().absolute().inset_0())
    }
}

#[gpui_kit::test]
fn picker_selects_content_under_empty_window_overlay(cx: &mut TestAppContext) {
    let selected = Rc::new(RefCell::new(None::<String>));
    let selection = selected.clone();
    cx.update(|cx| {
        cx.set_inspector_renderer(Box::new(move |inspector, _, _| {
            *selection.borrow_mut() = inspector
                .active_element_id()
                .map(|id| id.path.global_id.to_string());
            Empty.into_any_element()
        }));
    });

    let window = cx.add_window(|_, _| OverlayRoot);
    cx.update_window(window.into(), |_, window, cx| {
        window.toggle_inspector(cx);
        window.draw(cx).clear(cx);
        window.draw(cx).clear(cx);
        let position = point(px(20.), px(20.));
        window.simulate_mouse_move(position, cx);
        click(window, position, cx);
        window.draw(cx).clear(cx);

        assert!(!window.is_inspector_picking(cx));
        assert!(
            selected
                .borrow()
                .as_ref()
                .is_some_and(|id| id.contains("picker-target"))
        );
    })
    .expect("inspect content");
}

#[gpui_kit::test]
fn close_button_receives_first_click_while_picker_is_active(cx: &mut TestAppContext) {
    let closed = Rc::new(Cell::new(false));
    let close_state = closed.clone();
    cx.update(|cx| {
        cx.set_inspector_renderer(Box::new(move |_, _, cx| {
            let close_state = close_state.clone();
            div()
                .size_full()
                .child(
                    div()
                        .id("close-inspector")
                        .size(px(80.))
                        .on_click(cx.listener(move |_, _, window, cx| {
                            close_state.set(true);
                            window.toggle_inspector(cx);
                        })),
                )
                .into_any_element()
        }));
    });

    let window = cx.add_window(|_, _| OverlayRoot);
    cx.update_window(window.into(), |_, window, cx| {
        window.toggle_inspector(cx);
        window.draw(cx).clear(cx);
        window.draw(cx).clear(cx);
        assert!(window.is_inspector_picking(cx));

        let panel_width = gpui_kit::rems(30.)
            .to_pixels(window.rem_size())
            .min(window.viewport_size().width);
        let position = point(
            window.viewport_size().width - panel_width + px(20.),
            px(20.),
        );
        window.simulate_mouse_move(position, cx);
        click(window, position, cx);

        assert!(closed.get(), "the close handler runs on the first click");
        assert!(!window.is_inspector_picking(cx));
    })
    .expect("close inspector from panel");
}

fn click(window: &mut Window, position: Point<Pixels>, cx: &mut App) {
    window.dispatch_event(
        PlatformInput::MouseDown(MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            modifiers: Default::default(),
            first_mouse: false,
        }),
        cx,
    );
    window.dispatch_event(
        PlatformInput::MouseUp(MouseUpEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            modifiers: Default::default(),
        }),
        cx,
    );
}

struct AccountHeadRoot {
    head: std::sync::Arc<gpui_kit::RenderImage>,
}

impl Render for AccountHeadRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let head = self.head.clone();
        div()
            .v_flex()
            .w(px(252.))
            .h(px(400.))
            .px_3()
            .py_5()
            .gap_6()
            .child(div().h(px(1.)).w_full().bg(rgb(0x444444)))
            .child(
                gpui_kit::component::button::Button::new("account-switcher")
                    .ghost()
                    .w_full()
                    .h_11()
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(32.))
                                    .h(px(32.))
                                    .flex_shrink_0()
                                    .rounded(px(8.))
                                    .overflow_hidden()
                                    .bg(rgb(0x222222))
                                    .child(
                                        img(head)
                                            .size_full()
                                            .object_fit(ObjectFit::Cover)
                                            .debug_selector(|| "account-head-img".to_string()),
                                    ),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().text_size(px(13.)).truncate().child("Player"))
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .truncate()
                                            .child("Offline mode (no account)"),
                                    ),
                            ),
                    )
                    .dropdown_menu(move |menu, _, _| {
                        menu.label("Аккаунт".to_owned()).item(
                            gpui_kit::component::menu::PopupMenuItem::new(
                                "Офлайн-режим (без аккаунта)",
                            )
                            .checked(true)
                            .disabled(true),
                        )
                    }),
            )
    }
}

#[gpui_kit::test]
fn account_head_image_keeps_square_bounds(cx: &mut TestAppContext) {
    let head = std::sync::Arc::new(gpui_kit::RenderImage::new([image::Frame::new(
        image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 180, 144, 255])),
    )]));
    cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::component::set_locale("ru-RU");
        gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
    });

    let (_, cx) = cx.add_window_view(|_, _| AccountHeadRoot { head });
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    let bounds = cx
        .debug_bounds("account-head-img")
        .expect("head image bounds");
    assert_eq!(bounds.size.width, px(32.));
    assert_eq!(bounds.size.height, px(32.));
}
