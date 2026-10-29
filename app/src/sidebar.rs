use crate::app::LauncherApp;
use crate::features::settings::*;
use crate::router::Page;
use crate::shared::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;

impl LauncherApp {
    pub(crate) fn navigation_button(
        &self,
        page: Page,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_selected = self.active_page == page
            || (page == Page::Mods && self.active_page == Page::ModDetails);
        let is_collapsed = self.settings.read(cx).sidebar_collapsed();
        let title = page.title(locale);
        let content = if is_collapsed {
            div()
                .w_full()
                .flex()
                .justify_center()
                .child(page.icon())
                .into_any_element()
        } else {
            div()
                .w_full()
                .h_flex()
                .items_center()
                .gap_3()
                .child(page.icon())
                .child(title)
                .into_any_element()
        };

        let button = UiButton::new(page.element_id())
            .cursor_pointer()
            .w_full()
            .px_3()
            .py_3()
            .rounded(px(8.))
            .bg(rgb(if is_selected {
                palette.selected
            } else {
                palette.sidebar
            }))
            .text_color(rgb(if is_selected {
                palette.accent
            } else {
                palette.foreground
            }))
            .hover(|style| {
                style.bg(rgb(if is_selected {
                    palette.selected_hover
                } else {
                    palette.hover
                }))
            })
            .active(|style| style.bg(rgb(palette.pressed)))
            .selected(is_selected)
            .accessibility_label(title)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.navigate_to(page, window, cx);
            }))
            .child(content);

        let button = if is_collapsed {
            button.tooltip(move |window, cx| Tooltip::new(title).build(window, cx))
        } else {
            button
        };

        button.into_any_element()
    }

    pub(crate) fn sidebar_toggle_button(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_collapsed = self.settings.read(cx).sidebar_collapsed();
        let title = if is_collapsed {
            locale.text("Развернуть панель", "Expand sidebar")
        } else {
            locale.text("Свернуть панель", "Collapse sidebar")
        };

        UiButton::new("sidebar-toggle")
            .cursor_pointer()
            .w(px(24.))
            .h(px(24.))
            .rounded(px(6.))
            .bg(rgb(palette.sidebar))
            .text_color(rgb(palette.muted))
            .hover(|style| {
                style
                    .bg(rgb(palette.hover))
                    .text_color(rgb(palette.foreground))
            })
            .active(|style| style.bg(rgb(palette.pressed)))
            .accessibility_label(title)
            .tooltip(move |window, cx| Tooltip::new(title).build(window, cx))
            .on_click(cx.listener(|this, _, _, cx| {
                this.settings.update(cx, |settings, cx| {
                    settings.toggle_sidebar();
                    settings.save();
                    cx.notify();
                });
                cx.notify();
            }))
            .child(if is_collapsed {
                IconName::PanelLeftOpen
            } else {
                IconName::PanelLeftClose
            })
            .into_any_element()
    }
}
