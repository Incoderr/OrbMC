//! A colour picker: a saturation/brightness square and a hue strip, both
//! draggable. Emits [`ColorPicked`] while dragging and once more, committed,
//! when the mouse is released.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::StyledExt;
use gpui_kit::*;

const SQUARE_WIDTH: f32 = 220.;
const PICKER_HEIGHT: f32 = 160.;
const STRIP_WIDTH: f32 = 16.;

/// Colour chosen in the picker, as `0xRRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorPicked {
    pub color: u32,
    /// The drag ended: the colour is final and worth saving.
    pub committed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragTarget {
    Square,
    Hue,
}

pub struct ColorPicker {
    /// Hue, saturation and value in `0.0..=1.0`.
    hsv: (f32, f32, f32),
    drag: Option<DragTarget>,
    square: Rc<Cell<Bounds<Pixels>>>,
    strip: Rc<Cell<Bounds<Pixels>>>,
}

impl EventEmitter<ColorPicked> for ColorPicker {}

impl ColorPicker {
    pub fn new(color: u32) -> Self {
        Self {
            hsv: rgb_to_hsv(color),
            drag: None,
            square: Rc::default(),
            strip: Rc::default(),
        }
    }

    /// Shows `color` without emitting anything.
    pub fn set_color(&mut self, color: u32, cx: &mut Context<Self>) {
        if self.drag.is_none() && hsv_to_rgb(self.hsv) != color {
            self.hsv = rgb_to_hsv(color);
            cx.notify();
        }
    }

    pub fn color(&self) -> u32 {
        hsv_to_rgb(self.hsv)
    }

    fn pick(&mut self, target: DragTarget, position: Point<Pixels>, cx: &mut Context<Self>) {
        let fraction = |bounds: Bounds<Pixels>, x: bool| {
            let (offset, size) = if x {
                (position.x - bounds.origin.x, bounds.size.width)
            } else {
                (position.y - bounds.origin.y, bounds.size.height)
            };
            if size <= px(0.) {
                return 0.;
            }
            (offset / size).clamp(0., 1.)
        };
        let (hue, saturation, value) = self.hsv;
        self.hsv = match target {
            DragTarget::Square => {
                let bounds = self.square.get();
                (hue, fraction(bounds, true), 1. - fraction(bounds, false))
            }
            DragTarget::Hue => (fraction(self.strip.get(), false), saturation, value),
        };
        cx.emit(ColorPicked {
            color: self.color(),
            committed: false,
        });
        cx.notify();
    }

    fn start_drag(&mut self, target: DragTarget, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.drag = Some(target);
        self.pick(target, position, cx);
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            cx.emit(ColorPicked {
                color: self.color(),
                committed: true,
            });
            cx.notify();
        }
    }
}

/// Records where an element was laid out, for mapping mouse positions.
fn track_bounds(slot: Rc<Cell<Bounds<Pixels>>>) -> impl IntoElement {
    canvas(move |bounds, _, _| slot.set(bounds), |_, _, _, _| {})
        .absolute()
        .size_full()
}

impl Render for ColorPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (hue, saturation, value) = self.hsv;
        let pure_hue = rgb(hsv_to_rgb((hue, 1., 1.)));
        let white = hsla(0., 0., 1., 1.);
        let black = hsla(0., 0., 0., 1.);
        let clear_white = hsla(0., 0., 1., 0.);
        let clear_black = hsla(0., 0., 0., 0.);

        let square = div()
            .id("color-picker-square")
            .relative()
            .w(px(SQUARE_WIDTH))
            .h(px(PICKER_HEIGHT))
            .rounded(px(8.))
            .overflow_hidden()
            .cursor_crosshair()
            .bg(pure_hue)
            .child(track_bounds(self.square.clone()))
            .child(div().absolute().size_full().bg(linear_gradient(
                90.,
                linear_color_stop(white, 0.),
                linear_color_stop(clear_white, 1.),
            )))
            .child(div().absolute().size_full().bg(linear_gradient(
                180.,
                linear_color_stop(clear_black, 0.),
                linear_color_stop(black, 1.),
            )))
            .child(
                div()
                    .absolute()
                    .left(px(saturation * SQUARE_WIDTH - 7.))
                    .top(px((1. - value) * PICKER_HEIGHT - 7.))
                    .size(px(14.))
                    .rounded_full()
                    .border_2()
                    .border_color(white)
                    .shadow_sm(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.start_drag(DragTarget::Square, event.position, cx)
                }),
            );

        // Six stops around the colour wheel, red to red.
        let stops = [
            0xff0000, 0xffff00, 0x00ff00, 0x00ffff, 0x0000ff, 0xff00ff, 0xff0000,
        ];
        let strip = div()
            .id("color-picker-hue")
            .relative()
            .w(px(STRIP_WIDTH))
            .h(px(PICKER_HEIGHT))
            .rounded(px(4.))
            .cursor_row_resize()
            .child(track_bounds(self.strip.clone()))
            .child(
                div()
                    .v_flex()
                    .size_full()
                    .rounded(px(4.))
                    .overflow_hidden()
                    .children(stops.windows(2).map(|pair| {
                        div().w_full().flex_1().bg(linear_gradient(
                            180.,
                            linear_color_stop(rgb(pair[0]), 0.),
                            linear_color_stop(rgb(pair[1]), 1.),
                        ))
                    })),
            )
            .child(
                div()
                    .absolute()
                    .left(px(-2.))
                    .top(px(hue * PICKER_HEIGHT - 3.))
                    .w(px(STRIP_WIDTH + 4.))
                    .h(px(6.))
                    .rounded(px(3.))
                    .border_2()
                    .border_color(white)
                    .shadow_sm(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.start_drag(DragTarget::Hue, event.position, cx)
                }),
            );

        // While dragging, follow the mouse anywhere in the window.
        let drag_listener = self.drag.map(|target| {
            let picker = cx.entity().downgrade();
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    let moving = picker.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble {
                            let _ =
                                moving.update(cx, |this, cx| this.pick(target, event.position, cx));
                        }
                    });
                    let releasing = picker.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble {
                            let _ = releasing.update(cx, |this, cx| this.end_drag(cx));
                        }
                    });
                },
            )
            .absolute()
            .size_0()
        });

        div()
            .relative()
            .h_flex()
            .gap_3()
            .child(square)
            .child(strip)
            .children(drag_listener)
    }
}

/// `(hue, saturation, value)` in `0.0..=1.0` to `0xRRGGBB`.
pub fn hsv_to_rgb((hue, saturation, value): (f32, f32, f32)) -> u32 {
    let sector = (hue.rem_euclid(1.) * 6.).min(5.999_99);
    let chroma = value * saturation;
    let secondary = chroma * (1. - (sector % 2. - 1.).abs());
    let (red, green, blue) = match sector as u32 {
        0 => (chroma, secondary, 0.),
        1 => (secondary, chroma, 0.),
        2 => (0., chroma, secondary),
        3 => (0., secondary, chroma),
        4 => (secondary, 0., chroma),
        _ => (chroma, 0., secondary),
    };
    let base = value - chroma;
    let channel = |value: f32| ((value + base).clamp(0., 1.) * 255.).round() as u32;
    (channel(red) << 16) | (channel(green) << 8) | channel(blue)
}

/// `0xRRGGBB` to `(hue, saturation, value)` in `0.0..=1.0`.
pub fn rgb_to_hsv(color: u32) -> (f32, f32, f32) {
    let channel = |shift: u32| ((color >> shift) & 0xff) as f32 / 255.;
    let (red, green, blue) = (channel(16), channel(8), channel(0));
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    let hue = if delta == 0. {
        0.
    } else if max == red {
        ((green - blue) / delta).rem_euclid(6.) / 6.
    } else if max == green {
        ((blue - red) / delta + 2.) / 6.
    } else {
        ((red - green) / delta + 4.) / 6.
    };
    let saturation = if max == 0. { 0. } else { delta / max };
    (hue, saturation, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    // `gpui_kit::*` brings GPUI's own `test` attribute into scope.
    use core::prelude::v1::test;

    #[test]
    fn hsv_round_trips_through_rgb() {
        for color in [
            0xff0000, 0x00ff00, 0x0000ff, 0x3b82f6, 0xf59e0b, 0x123456, 0xffffff, 0,
        ] {
            assert_eq!(hsv_to_rgb(rgb_to_hsv(color)), color, "{color:06x}");
        }
    }

    #[test]
    fn corners_of_the_square_are_white_black_and_the_hue() {
        assert_eq!(hsv_to_rgb((0.5, 0., 1.)), 0xffffff);
        assert_eq!(hsv_to_rgb((0.5, 1., 0.)), 0x000000);
        assert_eq!(hsv_to_rgb((0., 1., 1.)), 0xff0000);
        assert_eq!(hsv_to_rgb((1. / 3., 1., 1.)), 0x00ff00);
    }
}
