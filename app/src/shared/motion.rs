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
use gpui_kit::*;
use gpui_kit::{base, component};
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
/// plays on every show. There is no exit: GPUI drops the tooltip in the frame
/// the pointer leaves its trigger, so nothing is left to animate out. A
/// `TransitionState` would outlive the element, but not keep it painted.
struct AnimatedTooltip(AnyView);

impl Render for AnimatedTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        enter(div().child(self.0.clone()), "tooltip-in", 0, Entrance::Drop)
    }
}

/// Dialog bodies that spring up into the window. Closing needs nothing from
/// us: since gpui-component 0.8.1 a dialog stays mounted while it closes and
/// plays its own entrance (the surface sliding down) backwards.
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

/// Dropdown menus that fall out of their button and fade back into it when
/// dismissed. Same use as gpui-component's `DropdownMenu`, which shows and
/// hides its menu without motion.
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

/// How long a closing menu takes to fade back into its button.
const MENU_EXIT: Duration = Duration::from_millis(120);
/// How far it rises while doing so: the way back of [`Entrance::Drop`].
const MENU_EXIT_RISE: f32 = 6.;

/// Open state of one [`MotionMenu`], kept by the element between renders.
#[derive(Default)]
struct MenuState {
    /// What the user asked for: false as soon as the menu is dismissed.
    open: bool,
    /// Whether the popover is shown: stays true until the exit has played.
    shown: bool,
    /// Built once per opening, kept until the menu is gone.
    menu: Option<Entity<component::menu::PopupMenu>>,
}

impl MenuState {
    fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            cx.notify();
        }
    }
}

impl RenderOnce for MotionMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // The popover is controlled: a dismissal only clears `open`, and the
        // popover stays mounted until the menu has animated out. Changes to
        // the state re-render the view that holds this element.
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| MenuState::default());
        let presence_id = SharedString::from(format!("menu-presence:{:?}", self.id));
        let builder = self.builder;
        component::popover::Popover::new(SharedString::from(format!("popover:{:?}", self.id)))
            .appearance(false)
            .overlay_closable(false)
            .trigger(self.trigger)
            .anchor(self.anchor)
            .open(state.read(cx).shown)
            .on_open_change({
                let state = state.clone();
                move |open, _, cx| {
                    state.update(cx, |state, cx| {
                        if *open {
                            state.open = true;
                            state.shown = true;
                            cx.notify();
                        } else {
                            state.close(cx);
                        }
                    })
                }
            })
            .content(move |_, window, cx| {
                let open = state.read(cx).open;
                let presence = base::motion::Presence::new(presence_id.clone(), open)
                    .transition(base::motion::Transition::new(MENU_EXIT))
                    .sample(window, cx);
                if !presence.should_render() {
                    // The exit has played: let the popover close for real,
                    // which also gives focus back to where it was.
                    state.update(cx, |state, cx| {
                        state.shown = false;
                        state.menu = None;
                        cx.notify();
                    });
                    return div().into_any_element();
                }

                let menu = match state.read(cx).menu.clone() {
                    Some(menu) => menu,
                    None => {
                        let builder = builder.clone();
                        let menu = component::menu::PopupMenu::build(
                            window,
                            cx,
                            move |menu, window, cx| builder(menu, window, cx),
                        );
                        state.update(cx, |state, _| state.menu = Some(menu.clone()));
                        menu.focus_handle(cx).focus(window, cx);
                        // Weak: the listener lives as long as the menu, which
                        // the state owns.
                        let weak_state = state.downgrade();
                        window
                            .subscribe(&menu, cx, move |_, _: &DismissEvent, _, cx| {
                                let _ = weak_state.update(cx, |state, cx| state.close(cx));
                            })
                            .detach();
                        menu
                    }
                };
                // Not rendered while closed, so each opening plays it again.
                let menu = enter(div().child(menu), "menu-in", 0, Entrance::Drop);
                if open {
                    return menu.into_any_element();
                }

                // Leaving: the menu fades back into its button and no longer
                // takes clicks or keys, so an item cannot fire twice.
                let away = 1. - presence.progress;
                div()
                    .relative()
                    .top(px(-MENU_EXIT_RISE * away))
                    .opacity(presence.progress)
                    .capture_key_down(|_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .child(menu)
                    .child(
                        div()
                            .id("menu-exit-shield")
                            .absolute()
                            .inset_0()
                            .occlude()
                            .on_any_mouse_down(|_, _, cx| cx.stop_propagation()),
                    )
                    .into_any_element()
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
    use super::{BOUNCY, MenuMotion as _, SNAPPY, SpringHover as _, mix_rgb};
    use gpui_kit::component;
    use gpui_kit::{
        AppContext as _, Context, InteractiveElement as _, IntoElement, Modifiers,
        ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, Window, div,
        point, px, rgb,
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

    /// A motion menu on a button in the window's corner.
    struct MenuProbe;

    impl Render for MenuProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                component::button::Button::new("menu-trigger")
                    .label("Open")
                    .motion_menu(|menu, _, _| {
                        menu.item(component::menu::PopupMenuItem::element(|_, _| {
                            div().debug_selector(|| "menu-item".into()).child("Item")
                        }))
                    }),
            )
        }
    }

    #[gpui_kit::test]
    fn dismissed_menu_animates_out_before_it_closes(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            component::Theme::change(component::ThemeMode::Dark, None, cx);
        });
        let (_, cx) = cx.add_window_view(|_, _| MenuProbe);
        let frame = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.refresh();
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        };
        frame(cx);

        cx.simulate_click(point(px(10.), px(10.)), Modifiers::default());
        frame(cx);
        assert!(cx.debug_bounds("menu-item").is_some(), "the menu opens");
        // Let it finish appearing: a menu dismissed mid-entrance leaves from
        // where it got to, which at the very start is nowhere.
        cx.executor().advance_clock(Duration::from_millis(500));
        frame(cx);

        cx.simulate_keystrokes("escape");
        frame(cx);
        assert!(
            cx.debug_bounds("menu-item").is_some(),
            "a dismissed menu stays on screen while it leaves"
        );

        cx.executor().advance_clock(Duration::from_millis(500));
        for _ in 0..3 {
            frame(cx);
        }
        assert!(cx.debug_bounds("menu-item").is_none(), "and then closes");
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
