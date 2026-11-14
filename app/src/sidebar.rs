use crate::app::LauncherApp;
use crate::features::settings::*;
use crate::router::Page;
use crate::shared::motion::TooltipMotion as _;
use crate::shared::motion::{SpringHover, mix_rgb};
use crate::shared::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;

/// How far a hovered button pushes its icon and label to the right.
const HOVER_SHIFT: f32 = 6.;

/// Icon with an optional label; `hover` slides it to the right.
fn button_content(icon: IconName, label: Option<&'static str>, hover: f32) -> Div {
    match label {
        None => div().w_full().flex().justify_center().child(icon),
        Some(label) => div()
            .w_full()
            .h_flex()
            .items_center()
            .gap_3()
            .pl(px(HOVER_SHIFT * hover.max(0.)))
            .child(icon)
            .child(label),
    }
}

impl LauncherApp {
    pub(crate) fn navigation_button(
        &self,
        page: Page,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_selected = self.active_page == page
            || (page == Page::Catalog && self.active_page == Page::ModDetails);
        let is_collapsed = self.settings.read(cx).sidebar_collapsed();
        let title = page.title(locale);
        let id = page.element_id();
        let (background, hover_background) = if is_selected {
            (palette.selected, palette.selected_hover)
        } else {
            (palette.sidebar, palette.hover)
        };

        let button = UiButton::new(id)
            .cursor_pointer()
            .w_full()
            .px_3()
            .py_3()
            .rounded(px(8.))
            .text_color(rgb(if is_selected {
                palette.accent
            } else {
                palette.foreground
            }))
            .active(|style| style.bg(rgb(palette.pressed)))
            .selected(is_selected)
            .accessibility_label(title)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.navigate_to(page, window, cx);
            }));

        let button = if is_collapsed {
            button.tooltip(move |window, cx| Tooltip::new(title).build_animated(window, cx))
        } else {
            button
        };

        let label = (!is_collapsed).then_some(title);
        button
            .spring_hover(id, move |button, hover| {
                button
                    .bg(mix_rgb(background, hover_background, hover))
                    .child(button_content(page.icon(), label, hover))
            })
            .into_any_element()
    }

    pub(crate) fn sidebar_toggle_button(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        const ID: &str = "sidebar-toggle";
        let is_collapsed = self.settings.read(cx).sidebar_collapsed();
        let title = if is_collapsed {
            locale.text("Развернуть панель", "Expand sidebar")
        } else {
            locale.text("Свернуть панель", "Collapse sidebar")
        };

        let icon = if is_collapsed {
            IconName::PanelLeftOpen
        } else {
            IconName::PanelLeftClose
        };

        let button = UiButton::new(ID)
            .cursor_pointer()
            .w_full()
            .px_3()
            .py_3()
            .rounded(px(8.))
            .active(|style| style.bg(rgb(palette.pressed)))
            .accessibility_label(title)
            .on_click(cx.listener(|this, _, _, cx| {
                this.settings.update(cx, |settings, cx| {
                    settings.toggle_sidebar();
                    settings.save();
                    cx.notify();
                });
            }));
        // Expanded, the label already says what the button does.
        let button = if is_collapsed {
            button.tooltip(move |window, cx| Tooltip::new(title).build_animated(window, cx))
        } else {
            button
        };

        let label = (!is_collapsed).then_some(locale.text("Свернуть", "Collapse"));
        button
            .spring_hover(ID, move |button, hover| {
                button
                    .bg(mix_rgb(palette.sidebar, palette.hover, hover))
                    .text_color(mix_rgb(palette.muted, palette.foreground, hover))
                    .child(button_content(icon, label, hover))
            })
            .into_any_element()
    }
}
