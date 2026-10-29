//! Who plays: for now the offline player name, edited from the account menu
//! at the bottom of the sidebar.

use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button as ComponentButton, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::*;

use super::instances::is_valid_player_name;
use super::settings::SettingsState;
use super::skins::SkinsView;
use crate::shared::locale::Locale;
use crate::shared::theme::Palette;

pub struct AccountView {
    settings: Entity<SettingsState>,
    skins: Entity<SkinsView>,
    nickname_input: Entity<InputState>,
    _subscriptions: [Subscription; 2],
}

impl AccountView {
    pub fn new(
        settings: Entity<SettingsState>,
        skins: Entity<SkinsView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let locale = settings.read(cx).locale();
        let nickname_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(nickname_placeholder(locale)));
        let subscriptions = [
            cx.observe_in(&settings, window, |this, settings, window, cx| {
                let locale = settings.read(cx).locale();
                this.nickname_input.update(cx, |input, cx| {
                    input.set_placeholder(nickname_placeholder(locale), window, cx);
                });
                cx.notify();
            }),
            // The account button shows the face of the selected skin.
            cx.observe(&skins, |_, _, cx| cx.notify()),
        ];
        Self {
            settings,
            skins,
            nickname_input,
            _subscriptions: subscriptions,
        }
    }

    /// Sidebar account placeholder: skin head plus player name, with a menu to
    /// pick the offline (no account) mode and edit the offline player name.
    fn account_block(&self, palette: Palette, locale: Locale, cx: &Context<Self>) -> AnyElement {
        let collapsed = self.settings.read(cx).sidebar_collapsed();
        let player_name = self.settings.read(cx).offline_player().to_owned();
        let offline_label = locale
            .text("Офлайн-режим (без аккаунта)", "Offline mode (no account)")
            .to_owned();
        let block_label = locale.text("Выбор аккаунта", "Account picker").to_owned();
        let menu_title = locale.text("Аккаунт", "Account").to_owned();
        let edit_label = locale.text("Изменить ник", "Change name").to_owned();
        let edit_owner = cx.weak_entity();

        let head = match self.skins.read(cx).head() {
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
                .cursor_pointer()
                .ghost()
                .w_full()
                .h_11()
                .accessibility_label(block_label)
                .child(head)
        } else {
            ComponentButton::new("account-switcher")
                .cursor_pointer()
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
                                owner.update(cx, |this, cx| {
                                    this.open_nickname_dialog(window, cx);
                                });
                            }
                        }
                    }))
            })
            .into_any_element()
    }

    /// Opens the dialog for editing the offline player name.
    fn open_nickname_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let palette = self.settings.read(cx).palette();
        let locale = self.settings.read(cx).locale();
        let title = locale.text("Ник игрока", "Player name").to_owned();
        let nickname_input = self.nickname_input.clone();
        let current = self.settings.read(cx).offline_player().to_owned();
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
                                UiButton::new("save-nickname").cursor_pointer()
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
                                            owner.update(cx, |this, cx| {
                                                this.settings.update(cx, |settings, cx| {
 settings.set_offline_player(name.clone());
 cx.notify();
 });
                                                this.settings.read(cx).save();
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
}

impl Render for AccountView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (palette, locale) = {
            let settings = self.settings.read(cx);
            (settings.palette(), settings.locale())
        };
        self.account_block(palette, locale, cx)
    }
}

fn nickname_placeholder(locale: Locale) -> &'static str {
    locale.text("Ник игрока", "Player name")
}
