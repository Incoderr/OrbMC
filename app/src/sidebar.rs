use crate::app::LauncherApp;
use crate::app::theme::Palette;
use crate::features::instances::*;
use crate::features::settings::*;
use crate::router::Page;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::button::{Button as ComponentButton, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{WindowExt, tooltip::Tooltip};
use gpui_kit::*;

impl LauncherApp {
    pub(crate) fn search_bar(&self, palette: Palette, locale: Locale) -> AnyElement {
        Input::new(&self.search_input)
            .w_full()
            .h(px(40.))
            .px_3()
            .rounded(px(8.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .prefix(IconName::Search)
            .aria_label(locale.text("Поиск по каталогу", "Search catalog"))
            .into_any_element()
    }

    pub(crate) fn navigation_button(
        &self,
        page: Page,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_selected = self.active_page == page
            || (page == Page::Mods && self.active_page == Page::ModDetails);
        let is_collapsed = self.sidebar.is_collapsed();
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

    /// Sidebar account placeholder: skin head plus player name, with a menu to
    /// pick the offline (no account) mode and edit the offline player name.
    pub(crate) fn account_block(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let collapsed = self.sidebar.is_collapsed();
        let player_name = self.settings.offline_player().to_owned();
        let offline_label = locale
            .text("Офлайн-режим (без аккаунта)", "Offline mode (no account)")
            .to_owned();
        let block_label = locale.text("Выбор аккаунта", "Account picker").to_owned();
        let menu_title = locale.text("Аккаунт", "Account").to_owned();
        let edit_label = locale.text("Изменить ник", "Change name").to_owned();
        let edit_owner = cx.weak_entity();

        let head = match self.skins.head() {
            Some(head) => div()
                .w(px(32.))
                .h(px(32.))
                .flex_shrink_0()
                .rounded(px(8.))
                .overflow_hidden()
                .bg(rgb(palette.control))
                .child(img(head).size_full().object_fit(ObjectFit::Cover))
                .into_any_element(),
            None => div()
                .w(px(32.))
                .h(px(32.))
                .flex_shrink_0()
                .rounded(px(8.))
                .bg(rgb(palette.control))
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(palette.muted))
                .child(IconName::User)
                .into_any_element(),
        };

        let trigger = if collapsed {
            ComponentButton::new("account-switcher")
                .ghost()
                .w_full()
                .h_11()
                .accessibility_label(block_label)
                .child(head)
        } else {
            ComponentButton::new("account-switcher")
                .ghost()
                .w_full()
                .h_11()
                .accessibility_label(block_label)
                .child(
                    div()
                        .h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_2()
                        .child(head)
                        .child(
                            div()
                                .v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_0()
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb(palette.foreground))
                                        .truncate()
                                        .child(player_name),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(rgb(palette.muted))
                                        .truncate()
                                        .child(offline_label.clone()),
                                ),
                        ),
                )
        };

        trigger
            .dropdown_menu(move |menu, _, _| {
                menu.label(menu_title.clone())
                    .item(
                        PopupMenuItem::new(offline_label.clone())
                            .checked(true)
                            .disabled(true),
                    )
                    .item(PopupMenuItem::new(edit_label.clone()).on_click({
                        let edit_owner = edit_owner.clone();
                        move |_, window, cx| {
                            if let Some(owner) = edit_owner.upgrade() {
                                let _ = owner.update(cx, |this, cx| {
                                    this.open_nickname_dialog(window, cx);
                                });
                            }
                        }
                    }))
            })
            .into_any_element()
    }

    /// Opens the dialog for editing the offline player name.
    pub(crate) fn open_nickname_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let palette = Palette::for_settings(self.settings.theme(), self.settings.accent());
        let locale = self.settings.locale();
        let title = locale.text("Ник игрока", "Player name").to_owned();
        let nickname_input = self.nickname_input.clone();
        let current = self.settings.offline_player().to_owned();
        nickname_input.update(cx, |input, cx| {
            input.set_value(current, window, cx);
        });
        let owner = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _, _| {
            let content_owner = owner.clone();
            let content_input = nickname_input.clone();
            dialog
                .w(px(420.))
                .title(title.clone())
                .content(move |content, _, cx| {
                    let name = content_input.read(cx).value().trim().to_owned();
                    let is_valid = is_valid_player_name(&name);
                    let save_owner = content_owner.clone();
                    content.child(
                        div()
                            .v_flex()
                            .w_full()
                            .gap_3()
                            .p_4()
                            .text_color(rgb(palette.foreground))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(palette.muted))
                                    .child(locale.text(
                                        "Ник используется в офлайн-режиме без аккаунта. От 3 до 16 символов: латиница, цифры и _",
                                        "Used for offline play without an account. 3-16 characters: Latin letters, digits and _",
                                    )),
                            )
                            .child(
                                Input::new(&content_input)
                                    .w_full()
                                    .h(px(40.))
                                    .px_3()
                                    .rounded(px(8.))
                                    .bg(rgb(palette.control))
                                    .border_1()
                                    .border_color(rgb(if is_valid {
                                        palette.border
                                    } else {
                                        palette.accent
                                    })),
                            )
                            .child(
                                UiButton::new("save-nickname")
                                    .w_full()
                                    .px_3()
                                    .py_2()
                                    .rounded(px(8.))
                                    .bg(rgb(if is_valid {
                                        palette.accent
                                    } else {
                                        palette.control
                                    }))
                                    .text_color(rgb(if is_valid {
                                        palette.accent_foreground
                                    } else {
                                        palette.muted
                                    }))
                                    .hover(|style| style.opacity(0.9))
                                    .on_click(move |_, window, cx| {
                                        if !is_valid {
                                            return;
                                        }
                                        if let Some(owner) = save_owner.upgrade() {
                                            let _ = owner.update(cx, |this, cx| {
                                                this.settings.set_offline_player(name.clone());
                                                this.persist_settings();
                                                cx.notify();
                                            });
                                        }
                                        window.close_dialog(cx);
                                    })
                                    .child(locale.text("Сохранить", "Save")),
                            ),
                    )
                })
        });
    }

    pub(crate) fn sidebar_toggle_button(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_collapsed = self.sidebar.is_collapsed();
        let title = if is_collapsed {
            locale.text("Развернуть панель", "Expand sidebar")
        } else {
            locale.text("Свернуть панель", "Collapse sidebar")
        };

        UiButton::new("sidebar-toggle")
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
                this.sidebar.toggle();
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
