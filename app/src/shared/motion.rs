//! Spring-driven motion on top of GPUI's `with_spring`.
//!
//! A spring keeps its speed when the target changes mid-flight, so an
//! interrupted animation (the pointer leaving halfway, a double toggle)
//! continues without a jump. The state lives in the element, by its id:
//! views keep no fields for it.
//!
//! ```ignore
//! // Any value: the spring follows the target between renders.
//! panel.with_spring("panel-width", spring(BOUNCY).to(target), |panel, width| panel.w(px(width)))
//!
//! // Hover: 0 away, 1 under the pointer, a little past 1 on arrival.
//! card.spring_hover("card-hover", move |card, hover| card.bg(mix_rgb(base, lit, hover)))
//! ```

use super::theme::Palette;
use gpui_kit::component;
use gpui_kit::*;
use std::rc::Rc;
use std::time::Duration;

/// Visibly overshoots the target once: panels and other large pieces.
pub(crate) const BOUNCY: SpringConfig = SpringConfig::new(300., 22., 1.);
/// Fast with a slight overshoot: hover and press feedback.
pub(crate) const SNAPPY: SpringConfig = SpringConfig::new(420., 26., 1.);

/// Springs here run between 0 and 1 (the caller scales the value to pixels
/// or colours), so "close enough to stop" is the same for all of them.
const REST_DISTANCE: f32 = 0.002;

/// A spring that rests once it is within [`REST_DISTANCE`] of its target.
/// Meant for 0..1 targets; give pixel targets their own `with_epsilon`.
pub(crate) fn spring(config: SpringConfig) -> SpringAnimation {
    SpringAnimation::new(config).with_epsilon(REST_DISTANCE)
}

/// Hover that springs instead of switching, for anything with an element id.
pub(crate) trait SpringHover: StatefulInteractiveElement + IntoElement + 'static {
    /// Calls `animator` with how hovered the element is: 0 away, 1 under the
    /// pointer, slightly outside that range while the spring overshoots.
    /// `id` must be unique among the element's siblings. Replaces `on_hover`.
    fn spring_hover(
        self,
        id: impl Into<ElementId>,
        animator: impl FnOnce(Self, f32) -> Self + 'static,
    ) -> SpringHoverElement<Self> {
        SpringHoverElement {
            id: id.into(),
            element: self,
            animator: Box::new(animator),
        }
    }
}

impl<E: StatefulInteractiveElement + IntoElement + 'static> SpringHover for E {}

/// See [`SpringHover::spring_hover`].
#[derive(IntoElement)]
pub(crate) struct SpringHoverElement<E: StatefulInteractiveElement + IntoElement + 'static> {
    id: ElementId,
    element: E,
    animator: Box<dyn FnOnce(E, f32) -> E>,
}

impl<E: StatefulInteractiveElement + IntoElement + 'static> RenderOnce for SpringHoverElement<E> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Changing it re-renders the view that holds the element.
        let hovered = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let target = if *hovered.read(cx) { 1. } else { 0. };
        self.element
            .on_hover(move |is_hovered, _, cx| {
                hovered.update(cx, |hovered, cx| {
                    *hovered = *is_hovered;
                    cx.notify();
                });
            })
            .with_spring(self.id, spring(SNAPPY).to(target), self.animator)
    }
}

/// How far a card rises under the pointer.
const HOVER_LIFT: f32 = 3.;

/// A card that answers the pointer: it rises a little and its border takes
/// the accent colour. `id` must differ from the card's own id.
pub(crate) fn lift_on_hover<E>(
    card: E,
    id: impl Into<ElementId>,
    palette: Palette,
) -> SpringHoverElement<E>
where
    E: StatefulInteractiveElement + Styled + IntoElement + 'static,
{
    card.spring_hover(id, move |card, hover| {
        card.relative()
            .top(px(-HOVER_LIFT * hover))
            .border_color(mix_rgb(palette.border, palette.accent, hover * 0.8))
    })
}

/// Delay between neighbours of a staggered list…
const STAGGER_STEP: Duration = Duration::from_millis(35);
/// …and the longest one, so long lists do not keep the user waiting.
const STAGGER_MAX: Duration = Duration::from_millis(280);

/// Where an element comes from when it appears. Different parts of the
/// interface use different ones, so the motion says what kind of thing
/// arrived: a page you went forward to, a menu falling out of its button…
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entrance {
    /// Up from slightly below: list items and cards.
    Rise,
    /// Down from slightly above: menus and tooltips out of their trigger.
    Drop,
    /// In from the left: going back, side lists.
    FromLeft,
    /// In from the right: going forward, search results.
    FromRight,
    /// Up from further below with a visible overshoot: dialogs and tiles.
    Bounce,
}

impl Entrance {
    /// Start offset in pixels and the spring that brings it home.
    fn motion(self) -> (f32, f32, SpringConfig) {
        match self {
            Self::Rise => (0., 10., SNAPPY),
            Self::Drop => (0., -8., SNAPPY),
            Self::FromLeft => (-28., 0., SNAPPY),
            Self::FromRight => (28., 0., SNAPPY),
            Self::Bounce => (0., 26., BOUNCY),
        }
    }
}

/// Fades the element in while it springs into place from `entrance`, once
/// per mount of `id`. `index` staggers neighbours: 0 starts at once, each
/// next one a little later.
pub(crate) fn enter<E: Styled + IntoElement + 'static>(
    element: E,
    id: impl Into<ElementId>,
    index: usize,
    entrance: Entrance,
) -> AnimationElement<E> {
    let (dx, dy, config) = entrance.motion();
    let delay = (STAGGER_STEP * index as u32).min(STAGGER_MAX);
    let (settle, ease) = sampled_easing(config, REST_DISTANCE);
    let total = (delay + settle).as_secs_f32();
    let (delay, settle) = (delay.as_secs_f32(), settle.as_secs_f32());
    let animation = Animation::new(Duration::from_secs_f32(total)).with_easing(move |t| {
        let elapsed = t * total - delay;
        if elapsed <= 0. {
            0.
        } else {
            ease(elapsed / settle)
        }
    });
    element.with_animation(id, animation, move |element, progress| {
        let away = 1. - progress;
        element
            .relative()
            .left(px(dx * away))
            .top(px(dy * away))
            // Opaque well before the overshoot, so a bounce reads as motion
            // rather than as a long fade.
            .opacity((progress * 1.6).clamp(0., 1.))
    })
}

/// [`enter`] from below: the common case for lists.
pub(crate) fn appear<E: Styled + IntoElement + 'static>(
    element: E,
    id: impl Into<ElementId>,
    index: usize,
) -> AnimationElement<E> {
    enter(element, id, index, Entrance::Rise)
}

/// Tooltips that drop out of their trigger instead of popping up.
pub(crate) trait TooltipMotion {
    /// Like `Tooltip::build`, with an entrance.
    fn build_animated(self, window: &mut Window, cx: &mut App) -> AnyView;
}

impl TooltipMotion for component::tooltip::Tooltip {
    fn build_animated(self, window: &mut Window, cx: &mut App) -> AnyView {
        let tooltip = self.build(window, cx);
        cx.new(|_| AnimatedTooltip(tooltip)).into()
    }
}

/// GPUI builds a fresh tooltip view each time one shows, so the entrance
/// plays on every show.
struct AnimatedTooltip(AnyView);

impl Render for AnimatedTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        enter(div().child(self.0.clone()), "tooltip-in", 0, Entrance::Drop)
    }
}

/// Dialog bodies that spring up into the window.
pub(crate) trait DialogMotion {
    /// Like `Dialog::content`, with a springy entrance for the body.
    fn animated_content(
        self,
        builder: impl Fn(
            component::dialog::DialogContent,
            &mut Window,
            &mut App,
        ) -> component::dialog::DialogContent
        + 'static,
    ) -> Self;
}

impl DialogMotion for component::dialog::Dialog {
    fn animated_content(
        self,
        builder: impl Fn(
            component::dialog::DialogContent,
            &mut Window,
            &mut App,
        ) -> component::dialog::DialogContent
        + 'static,
    ) -> Self {
        // The given content carries the dialog's paddings; the outer one
        // carries nothing, only the animated body.
        self.content(move |content, window, cx| {
            let body = div().w_full().flex_1().child(builder(content, window, cx));
            component::dialog::DialogContent::new().child(enter(
                body,
                "dialog-body-in",
                0,
                Entrance::Bounce,
            ))
        })
    }
}

type MenuBuilder = Rc<
    dyn Fn(
        component::menu::PopupMenu,
        &mut Window,
        &mut Context<component::menu::PopupMenu>,
    ) -> component::menu::PopupMenu,
>;

/// Dropdown menus that fall out of their button. Same use as
/// gpui-component's `DropdownMenu`, which shows its menu without motion.
pub(crate) trait MenuMotion: Sized {
    fn motion_menu_with_anchor(
        self,
        anchor: impl Into<Anchor>,
        builder: impl Fn(
            component::menu::PopupMenu,
            &mut Window,
            &mut Context<component::menu::PopupMenu>,
        ) -> component::menu::PopupMenu
        + 'static,
    ) -> MotionMenu;

    fn motion_menu(
        self,
        builder: impl Fn(
            component::menu::PopupMenu,
            &mut Window,
            &mut Context<component::menu::PopupMenu>,
        ) -> component::menu::PopupMenu
        + 'static,
    ) -> MotionMenu {
        self.motion_menu_with_anchor(Anchor::TopLeft, builder)
    }
}

impl MenuMotion for component::button::Button {
    fn motion_menu_with_anchor(
        mut self,
        anchor: impl Into<Anchor>,
        builder: impl Fn(
            component::menu::PopupMenu,
            &mut Window,
            &mut Context<component::menu::PopupMenu>,
        ) -> component::menu::PopupMenu
        + 'static,
    ) -> MotionMenu {
        let id = self
            .interactivity()
            .element_id
            .clone()
            .unwrap_or_else(|| "motion-menu".into());
        MotionMenu {
            id: SharedString::from(format!("motion-menu:{id:?}")).into(),
            anchor: anchor.into(),
            trigger: self,
            builder: Rc::new(builder),
        }
    }
}

/// See [`MenuMotion`].
#[derive(IntoElement)]
pub(crate) struct MotionMenu {
    id: ElementId,
    anchor: Anchor,
    trigger: component::button::Button,
    builder: MenuBuilder,
}

impl RenderOnce for MotionMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // The menu is built once per opening and kept while it is open; the
        // popover asks for its content on every render.
        let slot = window.use_keyed_state(self.id.clone(), cx, |_, _| {
            None::<Entity<component::menu::PopupMenu>>
        });
        let builder = self.builder;
        component::popover::Popover::new(SharedString::from(format!("popover:{:?}", self.id)))
            .appearance(false)
            .overlay_closable(false)
            .trigger(self.trigger)
            .anchor(self.anchor)
            .content(move |_, window, cx| {
                let menu = match slot.read(cx).clone() {
                    Some(menu) => menu,
                    None => {
                        let builder = builder.clone();
                        let menu = component::menu::PopupMenu::build(
                            window,
                            cx,
                            move |menu, window, cx| builder(menu, window, cx),
                        );
                        slot.update(cx, |slot, _| *slot = Some(menu.clone()));
                        menu.focus_handle(cx).focus(window, cx);
                        // Weak handles: the listener lives as long as the
                        // menu, which the slot owns.
                        let popover = cx.entity().downgrade();
                        let weak_slot = slot.downgrade();
                        window
                            .subscribe(&menu, cx, move |_, _: &DismissEvent, window, cx| {
                                if let Some(popover) = popover.upgrade() {
                                    popover.update(cx, |popover, cx| popover.dismiss(window, cx));
                                }
                                let _ = weak_slot.update(cx, |slot, _| *slot = None);
                            })
                            .detach();
                        menu
                    }
                };
                // Not rendered while closed, so each opening plays it again.
                enter(div().child(menu), "menu-in", 0, Entrance::Drop)
            })
    }
}

/// The colour `progress` of the way from `from` to `to` (both `0xRRGGBB`).
pub(crate) fn mix_rgb(from: u32, to: u32, progress: f32) -> Rgba {
    let progress = progress.clamp(0., 1.);
    let (from, to) = (rgb(from), rgb(to));
    Rgba {
        r: from.r + (to.r - from.r) * progress,
        g: from.g + (to.g - from.g) * progress,
        b: from.b + (to.b - from.b) * progress,
        a: 1.,
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: it would bring in GPUI's own `test` attribute.
    use super::{BOUNCY, SNAPPY, SpringHover as _, mix_rgb};
    use gpui_kit::{
        AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
        Styled as _, TestAppContext, Window, div, point, px, rgb,
    };
    use std::{cell::RefCell, rc::Rc, time::Duration};

    /// A 100px hover target in the window's corner; records what it is given.
    struct HoverProbe(Rc<RefCell<Vec<f32>>>);

    impl Render for HoverProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let values = self.0.clone();
            div()
                .size_full()
                .child(div().id("target").size(px(100.)).spring_hover(
                    "target-hover",
                    move |target, hover| {
                        values.borrow_mut().push(hover);
                        target
                    },
                ))
        }
    }

    #[gpui_kit::test]
    fn hover_springs_in_and_back_out(cx: &mut TestAppContext) {
        let values = Rc::new(RefCell::new(Vec::new()));
        let window = cx.add_window({
            let values = values.clone();
            move |_, _| HoverProbe(values)
        });
        cx.run_until_parked();
        assert_eq!(values.borrow().last().copied(), Some(0.));

        let mut move_pointer_and_wait = |x: f32, wait: Duration| {
            cx.update_window(window.into(), |_, window, cx| {
                window.simulate_mouse_move(point(px(x), px(20.)), cx);
            })
            .expect("move the pointer");
            cx.run_until_parked();
            cx.executor().advance_clock(wait);
            cx.update_window(window.into(), |_, window, cx| {
                window.simulate_next_frame(cx);
            })
            .expect("next frame");
            cx.run_until_parked();
            values.borrow().last().copied().expect("rendered")
        };

        let entering = move_pointer_and_wait(20., Duration::from_millis(50));
        assert!(entering > 0. && entering < 1.2, "{entering}");
        // Leaving halfway continues from where the spring is, towards zero.
        let leaving = move_pointer_and_wait(400., Duration::from_millis(50));
        assert!(leaving > 0. && leaving < 1.2, "{leaving}");
        let rested = move_pointer_and_wait(400., Duration::from_secs(2));
        assert!(rested.abs() < 0.001, "{rested}");
    }

    #[test]
    fn presets_overshoot_without_ringing() {
        for config in [BOUNCY, SNAPPY] {
            let (_, damping_ratio) = config.canonical();
            assert!((0.55..0.8).contains(&damping_ratio), "{damping_ratio}");
        }
    }

    #[test]
    fn mix_rgb_reaches_both_ends() {
        assert_eq!(mix_rgb(0x102030, 0xffffff, 0.), rgb(0x102030));
        assert_eq!(mix_rgb(0x102030, 0xffffff, 1.), rgb(0xffffff));
        assert_eq!(mix_rgb(0x000000, 0xffffff, 7.), rgb(0xffffff));
    }
}
