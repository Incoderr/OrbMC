//! Account menu at the bottom of the sidebar: offline mode with its player
//! name, or one of the signed-in Ely.by accounts.

use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button as ComponentButton, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::ely::AuthError;
use super::state::{AuthState, sign_in};
use crate::features::instances::is_valid_player_name;
use crate::features::settings::SettingsState;
use crate::features::skins::SkinsView;
use crate::shared::locale::Locale;
use crate::shared::theme::Palette;

/// Progress of the Ely.by sign-in dialog.
#[derive(Default)]
struct SignInForm {
    in_progress: bool,
    /// Ely.by asked for a one-time code; the code field is shown.
    needs_code: bool,
    error: Option<String>,
}

pub struct AccountView {
    settings: Entity<SettingsState>,
    auth: Entity<AuthState>,
    skins: Entity<SkinsView>,
    nickname_input: Entity<InputState>,
    login_input: Entity<InputState>,
    password_input: Entity<InputState>,
    code_input: Entity<InputState>,
    sign_in: SignInForm,
    _subscriptions: [Subscription; 3],
}

impl AccountView {
    pub fn new(
        settings: Entity<SettingsState>,
        auth: Entity<AuthState>,
        skins: Entity<SkinsView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let locale = settings.read(cx).locale();
        let nickname_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(nickname_placeholder(locale)));
        let login_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(login_placeholder(locale)));
        let password_input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(password_placeholder(locale))
        });
        let code_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(code_placeholder(locale)));
        let subscriptions = [
            cx.observe_in(&settings, window, |this, settings, window, cx| {
                let locale = settings.read(cx).locale();
                for (input, placeholder) in [
                    (&this.nickname_input, nickname_placeholder(locale)),
                    (&this.login_input, login_placeholder(locale)),
                    (&this.password_input, password_placeholder(locale)),
                    (&this.code_input, code_placeholder(locale)),
                ] {
                    input.update(cx, |input, cx| {
                        input.set_placeholder(placeholder, window, cx);
                    });
                }
                cx.notify();
            }),
            cx.observe(&auth, |_, _, cx| cx.notify()),
            // The account button shows the face of the selected skin.
            cx.observe(&skins, |_, _, cx| cx.notify()),
        ];
        Self {
            settings,
            auth,
            skins,
            nickname_input,
            login_input,
            password_input,
            code_input,
            sign_in: SignInForm::default(),
            _subscriptions: subscriptions,
        }
    }

    /// Sidebar account block: skin head plus player name, with a menu to
    /// switch between offline mode and Ely.by accounts.
    fn account_block(&self, palette: Palette, locale: Locale, cx: &Context<Self>) -> AnyElement {
        let collapsed = self.settings.read(cx).sidebar_collapsed();
        let auth = self.auth.read(cx);
        let selected = auth.selected().cloned();
        let ely_accounts = auth.ely_accounts().to_vec();
        let offline_label = locale
            .text("Офлайн-режим (без аккаунта)", "Offline mode (no account)")
            .to_owned();
        let (player_name, subtitle) = match &selected {
            Some(account) => (account.name.clone(), "Ely.by".to_owned()),
            None => (
                self.settings.read(cx).offline_player().to_owned(),
                offline_label.clone(),
            ),
        };
        let block_label = locale.text("Выбор аккаунта", "Account picker").to_owned();
        let menu_title = locale.text("Аккаунт", "Account").to_owned();
        let edit_label = locale
            .text("Изменить офлайн-ник", "Change offline name")
            .to_owned();
        let sign_in_label = locale
            .text("Войти через Ely.by…", "Sign in with Ely.by…")
            .to_owned();
        let sign_out_label = selected.as_ref().map(|account| {
            format!(
                "{} {}",
                locale.text("Выйти из", "Sign out of"),
                account.name
            )
        });
        let owner = cx.weak_entity();

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
                                        .child(subtitle),
                                ),
                        ),
                )
        };

        trigger
            .dropdown_menu(move |mut menu, _, _| {
                let selected_uuid = selected.as_ref().map(|account| account.uuid.clone());
                let select = |uuid: Option<String>| {
                    let owner = owner.clone();
                    move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        if let Some(owner) = owner.upgrade() {
                            let auth = owner.read(cx).auth.clone();
                            auth.update(cx, |auth, cx| auth.select(uuid.clone(), cx));
                        }
                    }
                };
                menu = menu.label(menu_title.clone()).item(
                    PopupMenuItem::new(offline_label.clone())
                        .checked(selected_uuid.is_none())
                        .on_click(select(None)),
                );
                for account in &ely_accounts {
                    menu = menu.item(
                        PopupMenuItem::new(format!("{} · Ely.by", account.name))
                            .checked(selected_uuid.as_deref() == Some(account.uuid.as_str()))
                            .on_click(select(Some(account.uuid.clone()))),
                    );
                }
                menu = menu
                    .separator()
                    .item(PopupMenuItem::new(edit_label.clone()).on_click({
                        let owner = owner.clone();
                        move |_, window, cx| {
                            if let Some(owner) = owner.upgrade() {
                                owner.update(cx, |this, cx| {
                                    this.open_nickname_dialog(window, cx);
                                });
                            }
                        }
                    }))
                    .item(PopupMenuItem::new(sign_in_label.clone()).on_click({
                        let owner = owner.clone();
                        move |_, window, cx| {
                            if let Some(owner) = owner.upgrade() {
                                owner.update(cx, |this, cx| {
                                    this.open_sign_in_dialog(window, cx);
                                });
                            }
                        }
                    }));
                if let (Some(label), Some(uuid)) = (sign_out_label.clone(), selected_uuid) {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        if let Some(owner) = owner.upgrade() {
                            let auth = owner.read(cx).auth.clone();
                            auth.update(cx, |auth, cx| auth.sign_out(&uuid, cx));
                        }
                    }));
                }
                menu
            })
            .into_any_element()
    }

    /// Opens the Ely.by sign-in dialog.
    fn open_sign_in_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let palette = self.settings.read(cx).palette();
        let locale = self.settings.read(cx).locale();
        let title = locale
            .text("Вход через Ely.by", "Sign in with Ely.by")
            .to_owned();
        self.sign_in = SignInForm::default();
        for input in [&self.password_input, &self.code_input] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        let (login_input, password_input, code_input) = (
            self.login_input.clone(),
            self.password_input.clone(),
            self.code_input.clone(),
        );
        let owner = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            let (login_input, password_input, code_input) = (
                login_input.clone(),
                password_input.clone(),
                code_input.clone(),
            );
            dialog
                .w(px(420.))
                .title(title.clone())
                .content(move |content, _, cx| {
                    let Some(view) = owner.upgrade() else {
                        return content;
                    };
                    let form = &view.read(cx).sign_in;
                    let (in_progress, needs_code, error) =
                        (form.in_progress, form.needs_code, form.error.clone());
                    let can_submit = !in_progress
                        && !login_input.read(cx).value().trim().is_empty()
                        && !password_input.read(cx).value().is_empty()
                        && (!needs_code || !code_input.read(cx).value().trim().is_empty());
                    let field = |input: &Entity<InputState>| {
                        Input::new(input)
                            .w_full()
                            .h(px(40.))
                            .px_3()
                            .rounded(px(8.))
                            .bg(rgb(palette.control))
                            .border_1()
                            .border_color(rgb(palette.border))
                    };
                    let submit_owner = owner.clone();
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
                                        "Пароль уходит только на Ely.by. Лаунчер хранит токен входа в хранилище паролей системы.",
                                        "The password goes only to Ely.by. The launcher keeps the sign-in token in the system password store.",
                                    )),
                            )
                            .child(field(&login_input))
                            .child(field(&password_input))
                            .when(needs_code, |column| column.child(field(&code_input)))
                            .when_some(error, |column, error| {
                                column.child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(rgb(0xef8580))
                                        .child(error),
                                )
                            })
                            .child(
                                UiButton::new("ely-sign-in")
                                    .cursor_pointer()
                                    .w_full()
                                    .px_3()
                                    .py_2()
                                    .rounded(px(8.))
                                    .bg(rgb(if can_submit {
                                        palette.accent
                                    } else {
                                        palette.control
                                    }))
                                    .text_color(rgb(if can_submit {
                                        palette.accent_foreground
                                    } else {
                                        palette.muted
                                    }))
                                    .hover(|style| style.opacity(0.9))
                                    .on_click(move |_, window, cx| {
                                        if !can_submit {
                                            return;
                                        }
                                        if let Some(owner) = submit_owner.upgrade() {
                                            owner.update(cx, |this, cx| {
                                                this.submit_sign_in(window, cx);
                                            });
                                        }
                                    })
                                    .child(if in_progress {
                                        locale.text("Входим…", "Signing in…")
                                    } else {
                                        locale.text("Войти", "Sign in")
                                    }),
                            ),
                    )
                })
        });
    }

    fn submit_sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let login = self.login_input.read(cx).value().trim().to_owned();
        let password = self.password_input.read(cx).value().to_string();
        let code = self
            .sign_in
            .needs_code
            .then(|| self.code_input.read(cx).value().trim().to_owned());
        self.sign_in.in_progress = true;
        self.sign_in.error = None;
        cx.notify();

        let task = cx
            .background_executor()
            .spawn(async move { sign_in(&login, &password, code.as_deref()) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.sign_in.in_progress = false;
                match result {
                    Ok(account) => {
                        tracing::info!(account = %account.name, "signed in to Ely.by");
                        this.auth.update(cx, |auth, cx| auth.add(account, cx));
                        this.password_input
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        window.close_dialog(cx);
                    }
                    Err(AuthError::TwoFactorRequired) if !this.sign_in.needs_code => {
                        this.sign_in.needs_code = true;
                        this.sign_in.error = Some(AuthError::TwoFactorRequired.to_string());
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Ely.by sign-in failed");
                        this.sign_in.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
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

fn login_placeholder(locale: Locale) -> &'static str {
    locale.text("Ник или e-mail на Ely.by", "Ely.by nickname or e-mail")
}

fn password_placeholder(locale: Locale) -> &'static str {
    locale.text("Пароль", "Password")
}

fn code_placeholder(locale: Locale) -> &'static str {
    locale.text("Код из приложения", "Code from the authenticator app")
}
