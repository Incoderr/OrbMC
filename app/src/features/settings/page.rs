use super::SettingsView;
use crate::features::settings::*;
use crate::shared::ui::selectable;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, Disableable, StyledExt};
use gpui_kit::component::button::Button as ComponentButton;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Theme, ThemeMode, tooltip::Tooltip};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl SettingsView {
    pub(super) fn settings_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = self.settings.read(cx).palette();
        let locale = self.settings.read(cx).locale();
        let theme = self.settings.read(cx).theme();

        let russian_owner = cx.entity().downgrade();
        let english_owner = cx.entity().downgrade();
        let locale_dropdown = ComponentButton::new("settings-language")
            .cursor_pointer()
            .outline()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_3()
                    .child(locale.name())
                    .child(IconName::ChevronDown),
            )
            .accessibility_label(locale.text("Язык интерфейса", "Interface language"))
            .dropdown_menu(move |menu, _, _| {
                let russian_owner = russian_owner.clone();
                let english_owner = english_owner.clone();
                menu.item(
                    PopupMenuItem::new(Locale::Ru.name())
                        .checked(locale == Locale::Ru)
                        .on_click(move |_, _, cx| {
                            let _ = russian_owner.update(cx, |this, cx| {
                                this.set_locale(Locale::Ru, cx);
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new(Locale::En.name())
                        .checked(locale == Locale::En)
                        .on_click(move |_, _, cx| {
                            let _ = english_owner.update(cx, |this, cx| {
                                this.set_locale(Locale::En, cx);
                            });
                        }),
                )
            })
            .into_any_element();

        let download_directory = self
            .settings
            .read(cx)
            .download_directory()
            .to_string_lossy()
            .to_string();
        let download_directory_button = UiButton::new("settings-download-directory")
            .cursor_pointer()
            .px_3()
            .py_2()
            .rounded(px(7.))
            .bg(rgb(palette.control))
            .text_color(rgb(palette.foreground))
            .hover(|style| style.bg(rgb(palette.hover)))
            .on_click(cx.listener(|this, _, _, cx| {
                this.choose_download_directory(cx);
            }))
            .child(locale.text("Выбрать…", "Choose…"));

        let hide_to_tray = Switch::new("settings-hide-to-tray")
            .checked(self.tray_available && self.settings.read(cx).hide_to_tray())
            .disabled(!self.tray_available)
            .color(rgb(palette.accent))
            .accessibility_label(locale.text("Скрывать в трей", "Hide to tray"))
            .on_change(cx.listener(|this, checked, _, cx| {
                this.settings.update(cx, |settings, cx| {
                    settings.set_hide_to_tray(*checked);
                    cx.notify();
                });
                this.settings.read(cx).save();
                cx.notify();
            }));

        let translate_descriptions = Switch::new("settings-translate-descriptions")
            .checked(self.settings.read(cx).translate_descriptions())
            .color(rgb(palette.accent))
            .accessibility_label(locale.text("Переводить описания", "Translate descriptions"))
            .on_change(cx.listener(|this, checked, _, cx| {
                this.settings.update(cx, |settings, cx| {
                    settings.set_translate_descriptions(*checked);
                    settings.save();
                    cx.notify();
                });
            }));
        let open_logs = UiButton::new("settings-open-logs")
            .cursor_pointer()
            .px_3()
            .py_2()
            .rounded(px(7.))
            .bg(rgb(palette.control))
            .text_color(rgb(palette.foreground))
            .hover(|style| style.bg(rgb(palette.hover)))
            .on_click(|_, _, _| {
                if let Err(error) = crate::platform::logging::log_directory()
                    .and_then(|path| crate::platform::process::open_directory(&path))
                {
                    tracing::warn!(%error, "failed to open the launcher log folder");
                }
            })
            .child(locale.text("Открыть папку", "Open folder"));

        let auto_update = Switch::new("settings-auto-update")
            .checked(self.settings.read(cx).auto_update())
            .color(rgb(palette.accent))
            .accessibility_label(locale.text("Автообновление", "Automatic updates"))
            .on_change(cx.listener(|this, checked, _, cx| {
                this.settings.update(cx, |settings, cx| {
                    settings.set_auto_update(*checked);
                    settings.save();
                    cx.notify();
                });
                // An update found while they were off starts downloading now.
                if *checked {
                    this.updater.update(cx, |updater, cx| updater.install(cx));
                }
            }));
        let (update_hint, update_control) = self.update_controls(palette, locale, cx);

        let discord_presence = Switch::new("settings-discord-presence")
            .checked(self.settings.read(cx).discord_presence())
            .color(rgb(palette.accent))
            .accessibility_label(locale.text("Discord Rich Presence", "Discord Rich Presence"))
            .on_change(cx.listener(|this, checked, _, cx| {
                this.set_discord_presence(*checked, cx);
            }));

        let general_card = div()
            .v_flex()
            .gap_4()
            .p_4()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(palette.muted))
                    .child(IconName::Settings2)
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(palette.foreground))
                            .child(locale.text("Общие", "General")),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_3()
                            .text_color(rgb(palette.muted))
                            .child(IconName::Languages)
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(rgb(palette.foreground))
                                            .child(locale.text("Язык", "Language")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(palette.muted))
                                            .child(locale.text(
                                                "Язык интерфейса лаунчера",
                                                "Launcher interface language",
                                            )),
                                    ),
                            ),
                    )
                    .child(locale_dropdown),
            )
            .child(div().h(px(1.)).w_full().bg(rgb(palette.border)))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .h_flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap_3()
                            .text_color(rgb(palette.muted))
                            .child(IconName::HardDrive)
                            .child(
                                div()
                                    .v_flex()
                                    .min_w_0()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(rgb(palette.foreground))
                                            .child(locale.text(
                                                "Папка загрузки сборок",
                                                "Build download folder",
                                            )),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .text_size(px(11.))
                                            .truncate()
                                            .child(download_directory.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(rgb(palette.muted))
                                            .child(locale.text(
                                                "Для новых сборок; существующие не перемещаются. Assets и библиотеки будут храниться рядом.",
                                                "Applies to new profiles; existing ones stay put. Shared assets and libraries are stored alongside.",
                                            )),
                                    ),
                            ),
                    )
                    .child(download_directory_button),
            )
            .child(div().h(px(1.)).w_full().bg(rgb(palette.border)))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_3()
                            .text_color(rgb(palette.muted))
                            .child(IconName::PanelTopClose)
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(rgb(palette.foreground))
                                            .child(locale.text("Скрывать в трей", "Hide to tray")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(palette.muted))
                                            .child(if self.tray_available {
                                                locale.text(
                                                    "Крестик скрывает окно. Нажми значок в трее, чтобы вернуться; «Выйти» — в его меню.",
                                                    "Close hides the window. Click the tray icon to return; use its menu to exit.",
                                                )
                                            } else {
                                                locale.text("Трей недоступен в этой системе", "Tray is unavailable on this system")
                                            }),
                                    ),
                            ),
                    )
                    .child(hide_to_tray),
            )
            .child(div().h(px(1.)).w_full().bg(rgb(palette.border)))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_3()
                            .text_color(rgb(palette.muted))
                            .child(IconName::Bot)
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(rgb(palette.foreground))
                                            .child(locale.text(
                                                "Discord Rich Presence",
                                                "Discord Rich Presence",
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(palette.muted))
                                            .child(locale.text(
                                                "Показывает статус «играет в Minecraft» в Discord, пока запущена игра",
                                                "Shows a \"playing Minecraft\" status in Discord while the game is running",
                                            )),
                                    ),
                            ),
                    )
                    .child(discord_presence),
            )
            .child(div().h(px(1.)).w_full().bg(rgb(palette.border)))
            .child(setting_row(
                IconName::Languages,
                locale.text("Переводить описания модов", "Translate mod descriptions"),
                locale.text(
                    "Описания в каталоге переводятся через Google Translate: их текст отправляется в Google",
                    "Catalog descriptions are translated with Google Translate, which receives their text",
                ),
                translate_descriptions.into_any_element(),
                palette,
            ))
            .child(div().h(px(1.)).w_full().bg(rgb(palette.border)))
            .child(setting_row(
                IconName::FileText,
                locale.text("Журнал лаунчера", "Launcher log"),
                locale.text(
                    "Пригодится, если что-то сломалось: приложи launcher.log к сообщению об ошибке",
                    "Useful when something breaks: attach launcher.log to your bug report",
                ),
                open_logs.into_any_element(),
                palette,
            ))
            .child(div().h(px(1.)).w_full().bg(rgb(palette.border)))
            .child(setting_row(
                IconName::Download,
                locale.text("Автообновление", "Automatic updates"),
                locale.text(
                    "Новые версии скачиваются с GitHub сами и запускаются при следующем старте",
                    "New versions are downloaded from GitHub and start with the next launch",
                ),
                auto_update.into_any_element(),
                palette,
            ))
            .child(div().h(px(1.)).w_full().bg(rgb(palette.border)))
            .child(setting_row(
                IconName::Info,
                locale.text("Версия лаунчера", "Launcher version"),
                update_hint,
                update_control,
                palette,
            ));

        let theme_card = div()
            .v_flex()
            .gap_4()
            .p_5()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_3()
                    .text_color(rgb(palette.muted))
                    .child(IconName::Palette)
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(palette.foreground))
                                    .child(locale.text("Внешний вид", "Appearance")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(palette.muted))
                                    .child(locale.text(
                                        "Тема и цвет акцента интерфейса",
                                        "Interface theme and accent color",
                                    )),
                            ),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .child(
                        UiButton::new("settings-theme-dark")
                            .cursor_pointer()
                            .flex_1()
                            .px_4()
                            .py_3()
                            .rounded(px(8.))
                            .bg(rgb(if theme == ThemeChoice::Dark {
                                palette.selected
                            } else {
                                palette.control
                            }))
                            .border_1()
                            .border_color(rgb(if theme == ThemeChoice::Dark {
                                palette.accent
                            } else {
                                palette.border
                            }))
                            .text_color(rgb(palette.foreground))
                            .hover(|style| {
                                style.bg(rgb(if theme == ThemeChoice::Dark {
                                    palette.selected_hover
                                } else {
                                    palette.hover
                                }))
                            })
                            .active(|style| style.bg(rgb(palette.pressed)))
                            .selected(theme == ThemeChoice::Dark)
                            .accessibility_label(locale.text("Тёмная", "Dark"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.update(cx, |settings, cx| {
                                    settings.set_theme(ThemeChoice::Dark);
                                    cx.notify();
                                });
                                Theme::change(ThemeMode::Dark, None, cx);
                                this.settings.read(cx).save();
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .justify_center()
                                    .gap_2()
                                    .child(IconName::Moon)
                                    .child(locale.text("Тёмная", "Dark")),
                            ),
                    )
                    .child(
                        UiButton::new("settings-theme-light")
                            .cursor_pointer()
                            .flex_1()
                            .px_4()
                            .py_3()
                            .rounded(px(8.))
                            .bg(rgb(if theme == ThemeChoice::Light {
                                palette.selected
                            } else {
                                palette.control
                            }))
                            .border_1()
                            .border_color(rgb(if theme == ThemeChoice::Light {
                                palette.accent
                            } else {
                                palette.border
                            }))
                            .text_color(rgb(palette.foreground))
                            .hover(|style| {
                                style.bg(rgb(if theme == ThemeChoice::Light {
                                    palette.selected_hover
                                } else {
                                    palette.hover
                                }))
                            })
                            .active(|style| style.bg(rgb(palette.pressed)))
                            .selected(theme == ThemeChoice::Light)
                            .accessibility_label(locale.text("Светлая", "Light"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.update(cx, |settings, cx| {
                                    settings.set_theme(ThemeChoice::Light);
                                    cx.notify();
                                });
                                Theme::change(ThemeMode::Light, None, cx);
                                this.settings.read(cx).save();
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .justify_center()
                                    .gap_2()
                                    .child(IconName::Sun)
                                    .child(locale.text("Светлая", "Light")),
                            ),
                    ),
            );

        let selected_accent = self.settings.read(cx).accent();
        let accent_swatches: Vec<_> = AccentColor::ALL
            .into_iter()
            .map(|color| {
                let is_selected = selected_accent == color;
                let color_value = color.color(theme);
                let label = color.name(locale);
                let check = if is_selected {
                    div()
                        .w_full()
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(color.foreground(theme)))
                        .child(IconName::Check)
                        .into_any_element()
                } else {
                    div().into_any_element()
                };

                UiButton::new(color.id())
                    .cursor_pointer()
                    .w(px(30.))
                    .h(px(30.))
                    .rounded(px(18.))
                    .bg(rgb(color_value))
                    .border_2()
                    .border_color(rgb(if is_selected {
                        palette.foreground
                    } else {
                        color_value
                    }))
                    .hover(|style| style.opacity(0.84))
                    .active(|style| style.opacity(0.72))
                    .selected(is_selected)
                    .accessibility_label(label)
                    .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings.update(cx, |settings, cx| {
                            settings.set_accent(color);
                            cx.notify();
                        });
                        this.settings.read(cx).save();
                        cx.notify();
                    }))
                    .child(check)
                    .into_any_element()
            })
            .collect();

        let custom_selected = matches!(selected_accent, AccentColor::Custom(_));
        let custom_label = AccentColor::Custom(0).name(locale);
        let custom_swatch = UiButton::new("accent-custom")
            .cursor_pointer()
            .w(px(30.))
            .h(px(30.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(18.))
            .bg(rgb(if custom_selected {
                selected_accent.color(theme)
            } else {
                palette.control
            }))
            .border_2()
            .border_color(rgb(if custom_selected || self.palette_open {
                palette.foreground
            } else {
                palette.border
            }))
            .text_color(rgb(if custom_selected {
                selected_accent.foreground(theme)
            } else {
                palette.muted
            }))
            .hover(|style| style.opacity(0.84))
            .selected(custom_selected)
            .accessibility_label(custom_label)
            .tooltip(move |window, cx| Tooltip::new(custom_label).build(window, cx))
            .on_click(cx.listener(|this, _, _, cx| {
                this.palette_open = !this.palette_open;
                if this.palette_open {
                    let settings = this.settings.read(cx);
                    let accent = settings.accent().color(settings.theme());
                    this.color_picker
                        .update(cx, |picker, cx| picker.set_color(accent, cx));
                }
                cx.notify();
            }))
            .child(if custom_selected {
                IconName::Check
            } else {
                IconName::Palette
            });
        let picked = self.color_picker.read(cx).color();
        let palette_grid = div()
            .h_flex()
            .items_start()
            .gap_4()
            .p_3()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .child(self.color_picker.clone())
            .child(
                div()
                    .v_flex()
                    .gap_2()
                    .child(
                        div()
                            .w(px(64.))
                            .h(px(64.))
                            .rounded(px(10.))
                            .bg(rgb(picked))
                            .border_1()
                            .border_color(rgb(palette.border)),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(palette.foreground))
                            .child(selectable("accent-hex", format!("#{picked:06x}"))),
                    ),
            );

        let appearance_card = theme_card
            .child(div().h(px(1.)).w_full().bg(rgb(palette.border)))
            .child(
                div()
                    .v_flex()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(palette.foreground))
                            .child(locale.text("Цвет акцента", "Accent color")),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .children(accent_swatches)
                            .child(div().w(px(1.)).h(px(22.)).mx_1().bg(rgb(palette.border)))
                            .child(custom_swatch)
                            .when(custom_selected, |row| {
                                row.child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(palette.muted))
                                        .child(format!("#{:06x}", selected_accent.color(theme))),
                                )
                            }),
                    )
                    .when(self.palette_open, |column| column.child(palette_grid)),
            );

        div()
            .v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .pr_4()
            .overflow_y_scrollbar()
            .gap_4()
            .child(general_card)
            .child(appearance_card)
            .into_any_element()
    }
}

/// One settings line: icon, title with a hint, and the control on the right.
fn setting_row(
    icon: IconName,
    title: &'static str,
    hint: impl Into<SharedString>,
    control: AnyElement,
    palette: Palette,
) -> AnyElement {
    div()
        .h_flex()
        .items_center()
        .justify_between()
        .gap_3()
        .child(
            div()
                .h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_3()
                .text_color(rgb(palette.muted))
                .child(icon)
                .child(
                    div()
                        .v_flex()
                        .min_w_0()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(rgb(palette.foreground))
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(palette.muted))
                                .child(hint.into()),
                        ),
                ),
        )
        .child(control)
        .into_any_element()
}

impl SettingsView {
    /// Status line and the action button of the "Launcher version" row.
    fn update_controls(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &mut Context<Self>,
    ) -> (String, AnyElement) {
        use crate::features::updates::{UpdateStatus, Version};

        let current = Version::current();
        let action = |id: &'static str, label: String| {
            UiButton::new(id)
                .cursor_pointer()
                .px_3()
                .py_2()
                .rounded(px(7.))
                .bg(rgb(palette.control))
                .text_color(rgb(palette.foreground))
                .hover(|style| style.bg(rgb(palette.hover)))
                .child(label)
        };
        let check = || {
            action(
                "settings-update-check",
                locale.text("Проверить", "Check").to_owned(),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.updater
                    .update(cx, |updater, cx| updater.check(false, cx))
            }))
            .into_any_element()
        };
        let quiet = |text: String| {
            div()
                .text_size(px(12.))
                .text_color(rgb(palette.muted))
                .child(text)
                .into_any_element()
        };

        let updater = self.updater.read(cx);
        match updater.status().clone() {
            UpdateStatus::Idle => (
                format!("{} {current}", locale.text("Версия", "Version")),
                check(),
            ),
            UpdateStatus::Checking => (
                format!("{} {current}", locale.text("Версия", "Version")),
                quiet(locale.text("Проверяю…", "Checking…").to_owned()),
            ),
            UpdateStatus::UpToDate => (
                format!(
                    "{} {current}",
                    locale.text("Установлена последняя версия", "Up to date:")
                ),
                check(),
            ),
            UpdateStatus::Failed(error) => (error, check()),
            UpdateStatus::Available(release) => {
                let hint = match locale {
                    Locale::Ru => format!("Доступна версия {} (сейчас {current})", release.version),
                    Locale::En => format!(
                        "Version {} is available (you have {current})",
                        release.version
                    ),
                };
                let control = if release.installable() {
                    action(
                        "settings-update-install",
                        format!(
                            "{} {}",
                            locale.text("Установить", "Install"),
                            release.version
                        ),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.updater.update(cx, |updater, cx| updater.install(cx))
                    }))
                    .into_any_element()
                } else {
                    let page_url = release.page_url.clone();
                    action(
                        "settings-update-page",
                        locale.text("Открыть релиз", "Open release").to_owned(),
                    )
                    .on_click(move |_, _, cx| cx.open_url(&page_url))
                    .into_any_element()
                };
                (hint, control)
            }
            UpdateStatus::Downloading { version, transfer } => {
                let progress = transfer.progress();
                let percent = progress
                    .total
                    .filter(|total| *total > 0)
                    .map(|total| progress.received * 100 / total)
                    .unwrap_or(0);
                (
                    format!(
                        "{} {version}: {percent}%",
                        locale.text("Загрузка версии", "Downloading version")
                    ),
                    action(
                        "settings-update-cancel",
                        locale.text("Отмена", "Cancel").to_owned(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.updater.update(cx, |updater, cx| updater.cancel(cx))
                    }))
                    .into_any_element(),
                )
            }
            UpdateStatus::Ready { version, .. } => {
                if updater.can_restart(cx) {
                    (
                        format!(
                            "{} {version} {}",
                            locale.text("Версия", "Version"),
                            locale.text(
                                "скачана: перезапусти лаунчер, чтобы установить",
                                "is downloaded: restart the launcher to install it"
                            ),
                        ),
                        action(
                            "settings-update-restart",
                            locale.text("Перезапустить", "Restart").to_owned(),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.updater.update(cx, |updater, cx| updater.restart(cx))
                        }))
                        .into_any_element(),
                    )
                } else {
                    (
                        format!(
                            "{} {version} {}",
                            locale.text("Версия", "Version"),
                            locale.text(
                                "скачана и установится при выходе из лаунчера",
                                "is downloaded and installs when you quit the launcher"
                            ),
                        ),
                        quiet(String::new()),
                    )
                }
            }
        }
    }
}
