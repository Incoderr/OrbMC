use crate::app::LauncherApp;
use crate::app::theme::Palette;
use crate::features::settings::*;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, Disableable, StyledExt};
use gpui_kit::component::button::Button as ComponentButton;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Theme, ThemeMode, tooltip::Tooltip};
use gpui_kit::*;

impl LauncherApp {
    pub(crate) fn settings_page(&self, palette: Palette, cx: &mut Context<Self>) -> AnyElement {
        let locale = self.settings.locale();
        let theme = self.settings.theme();

        let russian_owner = cx.entity().downgrade();
        let english_owner = cx.entity().downgrade();
        let locale_dropdown = ComponentButton::new("settings-language")
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
                        .on_click(move |_, window, cx| {
                            let _ = russian_owner.update(cx, |this, cx| {
                                this.set_locale(Locale::Ru, window, cx);
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new(Locale::En.name())
                        .checked(locale == Locale::En)
                        .on_click(move |_, window, cx| {
                            let _ = english_owner.update(cx, |this, cx| {
                                this.set_locale(Locale::En, window, cx);
                            });
                        }),
                )
            })
            .into_any_element();

        let download_directory = self
            .settings
            .download_directory()
            .to_string_lossy()
            .to_string();
        let download_directory_button = UiButton::new("settings-download-directory")
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
            .checked(self.tray_available && self.settings.hide_to_tray())
            .disabled(!self.tray_available)
            .color(rgb(palette.accent))
            .accessibility_label(locale.text("Скрывать в трей", "Hide to tray"))
            .on_change(cx.listener(|this, checked, _, cx| {
                this.settings.set_hide_to_tray(*checked);
                this.persist_settings();
                cx.notify();
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
            );

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
                                this.settings.set_theme(ThemeChoice::Dark);
                                Theme::change(ThemeMode::Dark, None, cx);
                                this.persist_settings();
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
                                this.settings.set_theme(ThemeChoice::Light);
                                Theme::change(ThemeMode::Light, None, cx);
                                this.persist_settings();
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

        let selected_accent = self.settings.accent();
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
                        this.settings.set_accent(color);
                        this.persist_settings();
                        cx.notify();
                    }))
                    .child(check)
                    .into_any_element()
            })
            .collect();

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
                            .children(accent_swatches),
                    ),
            );

        let preview = div()
            .v_flex()
            .w(px(232.))
            .gap_3()
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
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(palette.foreground))
                    .child(IconName::PanelLeft)
                    .child(locale.text("Предварительный просмотр", "Preview")),
            )
            .child(
                div()
                    .v_flex()
                    .h(px(270.))
                    .gap_3()
                    .p_3()
                    .rounded(px(8.))
                    .bg(rgb(palette.background))
                    .border_1()
                    .border_color(rgb(palette.border))
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .h_flex()
                                    .w_full()
                                    .items_center()
                                    .gap_2()
                                    .px_2()
                                    .py_2()
                                    .rounded(px(6.))
                                    .bg(rgb(palette.selected))
                                    .text_color(rgb(palette.accent))
                                    .child(IconName::House)
                                    .child(locale.text("Главная", "Home")),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .px_2()
                                    .py_2()
                                    .text_color(rgb(palette.muted))
                                    .child(IconName::Blocks)
                                    .child(locale.text("Модпаки", "Modpacks")),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .py_2()
                            .rounded(px(7.))
                            .bg(rgb(palette.accent))
                            .text_color(rgb(palette.accent_foreground))
                            .font_weight(FontWeight::BOLD)
                            .child(IconName::Play)
                            .child(locale.text("Играть", "Play")),
                    ),
            );

        div()
            .h_flex()
            .w_full()
            .gap_4()
            .child(
                div()
                    .v_flex()
                    .flex_1()
                    .gap_4()
                    .child(general_card)
                    .child(appearance_card),
            )
            .child(preview)
            .into_any_element()
    }
}
