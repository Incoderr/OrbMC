use crate::app::LauncherApp;
use crate::app::theme::Palette;
use crate::app::ui::*;
use crate::features::mods::catalog::*;
use crate::features::mods::*;
use crate::features::settings::*;
use crate::router::Page;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl LauncherApp {
    pub(crate) fn mod_details_page(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(sample) = self.mods.selected_mod() else {
            return self.empty_page(palette, locale);
        };
        let active_tab = self.mods.detail_tab();
        let project_url = sample.page_url.clone();
        let project_kind = sample.kind;
        let back_page = match project_kind {
            ProjectKind::Mod => Page::Mods,
            ProjectKind::Modpack => Page::Modpacks,
            ProjectKind::ResourcePack => Page::ResourcePacks,
            ProjectKind::Shader => Page::Shaders,
        };

        let back_button = UiButton::new("mod-details-back")
            .px_2()
            .py_2()
            .rounded(px(7.))
            .bg(rgb(palette.background))
            .text_color(rgb(palette.muted))
            .hover(|style| style.bg(rgb(palette.hover)))
            .accessibility_label(locale.text("Назад в каталог", "Back to catalog"))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.mods.clear_selected_mod();
                this.navigate_to(back_page, window, cx);
            }))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .child(IconName::ArrowLeft)
                    .child(locale.text("Назад", "Back")),
            );

        let category_tags: Vec<_> = sample
            .categories
            .iter()
            .map(|category| {
                div()
                    .px_2()
                    .py_1()
                    .rounded(px(12.))
                    .bg(rgb(palette.control))
                    .text_size(px(10.))
                    .text_color(rgb(palette.muted))
                    .child(category.title(locale))
                    .into_any_element()
            })
            .collect();

        let primary_action = if project_kind == ProjectKind::Mod {
            self.mod_install_action(sample, palette, locale, cx)
        } else {
            let page_url = project_url.clone();
            UiButton::new(format!("open-project-page-{}", sample.id))
                .px_3()
                .py_2()
                .rounded(px(7.))
                .bg(rgb(palette.accent))
                .text_color(rgb(palette.accent_foreground))
                .hover(|style| style.opacity(0.9))
                .on_click(move |_, _, cx| cx.open_url(&page_url))
                .child(locale.text("Открыть страницу", "Open project page"))
                .into_any_element()
        };

        let hero = div()
            .h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_4()
            .p_4()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .child(mod_project_thumbnail(sample, px(76.)))
            .child(
                div()
                    .v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(21.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(palette.foreground))
                            .w_full()
                            .min_w_0()
                            .truncate()
                            .child(sample.name.clone()),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .overflow_hidden()
                            .gap_3()
                            .text_size(px(11.))
                            .text_color(rgb(palette.muted))
                            .child(sample.author.clone())
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(IconName::ArrowDown)
                                    .child(sample.downloads_label.clone()),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(IconName::Heart)
                                    .child(sample.likes_label.clone()),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .min_w_0()
                            .overflow_hidden()
                            .gap_2()
                            .children(category_tags),
                    ),
            )
            .child(
                div()
                    .v_flex()
                    .w(px(190.))
                    .items_end()
                    .gap_2()
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(6.))
                            .bg(rgb(palette.control))
                            .text_size(px(10.))
                            .text_color(rgb(palette.muted))
                            .child(sample.game_version.clone()),
                    )
                    .child(primary_action)
                    .child(
                        UiButton::new(format!("open-mod-page-{}", sample.id))
                            .px_3()
                            .py_2()
                            .rounded(px(7.))
                            .bg(rgb(palette.control))
                            .text_color(rgb(palette.foreground))
                            .hover(|style| style.bg(rgb(palette.hover)))
                            .on_click(move |_, _, cx| cx.open_url(&project_url))
                            .child(locale.text("Страница проекта", "Project page")),
                    ),
            );

        let detail_tabs = if project_kind == ProjectKind::Mod {
            ModDetailTab::ALL.to_vec()
        } else {
            vec![ModDetailTab::Description]
        };
        let tabs: Vec<_> = detail_tabs
            .into_iter()
            .map(|tab| {
                let is_selected = tab == active_tab;
                let title = if tab == ModDetailTab::Files {
                    if sample.files_count > 0 {
                        format!("{} ({})", tab.title(locale), sample.files_count)
                    } else {
                        tab.title(locale).to_owned()
                    }
                } else {
                    tab.title(locale).to_owned()
                };

                UiButton::new(tab.id())
                    .px_3()
                    .py_2()
                    .rounded(px(7.))
                    .bg(rgb(if is_selected {
                        palette.selected
                    } else {
                        palette.background
                    }))
                    .text_color(rgb(if is_selected {
                        palette.accent
                    } else {
                        palette.muted
                    }))
                    .hover(|style| style.bg(rgb(palette.hover)))
                    .selected(is_selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.mods.set_detail_tab(tab);
                        cx.notify();
                    }))
                    .child(title)
                    .into_any_element()
            })
            .collect();

        let tab_content = self.mod_detail_content(sample, active_tab, palette, locale, cx);
        let metadata = div()
            .v_flex()
            .w(px(204.))
            .min_w_0()
            .gap_3()
            .p_4()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .child(detail_meta_row(
                IconName::Calendar,
                locale.text("Последнее обновление", "Last updated"),
                sample.updated.clone(),
                palette,
            ))
            .child(detail_meta_row(
                IconName::HardDrive,
                locale.text("Размер", "File size"),
                sample.file_size.clone(),
                palette,
            ))
            .child(detail_meta_row(
                IconName::FileText,
                locale.text("Лицензия", "License"),
                sample.license.clone(),
                palette,
            ))
            .child(detail_meta_row(
                IconName::Blocks,
                locale.text("Игровые версии", "Game versions"),
                sample.game_version.clone(),
                palette,
            ))
            .child(detail_meta_row(
                IconName::Cpu,
                locale.text("Загрузчики", "Loaders"),
                sample.loaders.clone(),
                palette,
            ));

        div()
            .v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_3()
            .child(back_button)
            .child(hero)
            .child(div().h_flex().w_full().gap_2().children(tabs))
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .gap_3()
                    .items_start()
                    .child(
                        div()
                            .v_flex()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .pr_4()
                            .overflow_y_scrollbar()
                            .child(tab_content),
                    )
                    .child(metadata),
            )
            .into_any_element()
    }

    pub(crate) fn mod_detail_content(
        &self,
        sample: &ModProject,
        tab: ModDetailTab,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        match tab {
            ModDetailTab::Description => {
                let description = self
                    .mods
                    .full_description()
                    .unwrap_or(&sample.description)
                    .to_owned();
                let description_view = match sample.source {
                    ModSource::CurseForge => gpui_kit::component::text::html(description),
                    ModSource::All | ModSource::Modrinth => {
                        gpui_kit::component::text::markdown(description)
                    }
                }
                .w_full()
                .text_size(px(12.))
                .text_color(rgb(palette.muted))
                .style(
                    gpui_kit::component::text::TextViewStyle::default()
                        .paragraph_gap(rems(0.5))
                        .heading_font_size(|level, _| {
                            px(match level {
                                1 => 22.,
                                2 => 19.,
                                3 => 16.,
                                _ => 14.,
                            })
                        }),
                );
                let description_status =
                    if self.mods.description_loading() {
                        Some(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(palette.muted))
                                .child(locale.text(
                                    "Загружаем полное описание…",
                                    "Loading full description…",
                                ))
                                .into_any_element(),
                        )
                    } else {
                        self.mods.description_error().map(|error| {
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(palette.muted))
                                .child(format!(
                                    "{}{error}",
                                    locale.text(
                                        "Полное описание недоступно: ",
                                        "Full description unavailable: "
                                    )
                                ))
                                .into_any_element()
                        })
                    };
                let screenshots = if sample.kind == ProjectKind::Mod {
                    self.mod_screenshots(sample, palette, locale)
                } else {
                    div().into_any_element()
                };

                div()
                    .v_flex()
                    .w_full()
                    .min_w_0()
                    .gap_4()
                    .child(
                        div()
                            .v_flex()
                            .w_full()
                            .min_w_0()
                            .gap_2()
                            .p_4()
                            .rounded(px(10.))
                            .bg(rgb(palette.surface))
                            .border_1()
                            .border_color(rgb(palette.border))
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(palette.foreground))
                                    .child(locale.text("Описание", "Description")),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap_2()
                                    .child(description_view)
                                    .when_some(description_status, |this, status| {
                                        this.child(status)
                                    }),
                            ),
                    )
                    .child(screenshots)
                    .into_any_element()
            }
            ModDetailTab::Files => div()
                .v_flex()
                .gap_3()
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(palette.muted))
                        .child(locale.text(
                            "Выбери сборку — лаунчер скачает совместимую версию мода.",
                            "Choose a build and the launcher will download a compatible mod file.",
                        )),
                )
                .child(
                    UiButton::new(format!("download-mod-file-{}", sample.id))
                        .px_3()
                        .py_2()
                        .rounded(px(7.))
                        .bg(rgb(palette.accent))
                        .text_color(rgb(palette.accent_foreground))
                        .hover(|style| style.opacity(0.9))
                        .on_click(cx.listener({
                            let project = sample.clone();
                            move |this, _, window, cx| {
                                this.open_mod_download_dialog(project.clone(), window, cx);
                            }
                        }))
                        .child(locale.text("Скачать в сборку", "Download to a build")),
                )
                .into_any_element(),
            ModDetailTab::Dependencies => div()
                .v_flex()
                .gap_3()
                .child(detail_meta_row(
                    IconName::Blocks,
                    locale.text("Игра", "Game"),
                    sample.game_version.clone(),
                    palette,
                ))
                .child(detail_meta_row(
                    IconName::Cpu,
                    locale.text("Поддерживаемые загрузчики", "Supported loaders"),
                    sample.loaders.clone(),
                    palette,
                ))
                .into_any_element(),
            ModDetailTab::Screenshots => self.mod_screenshots(sample, palette, locale),
            ModDetailTab::Similar => {
                let similar: Vec<_> = SAMPLE_MODS
                    .iter()
                    .take(3)
                    .map(|candidate| self.home_mod_card(candidate, palette, locale))
                    .collect();
                div()
                    .h_flex()
                    .w_full()
                    .gap_3()
                    .children(similar)
                    .into_any_element()
            }
        }
    }

    pub(crate) fn mod_screenshots(
        &self,
        sample: &ModProject,
        palette: Palette,
        locale: Locale,
    ) -> AnyElement {
        let thumbnails: Vec<_> = [0, 1, 2]
            .into_iter()
            .map(|index| {
                if index == 0 && sample.image_url.is_some() {
                    mod_project_thumbnail(sample, px(112.))
                } else {
                    div()
                        .flex_1()
                        .h(px(112.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.))
                        .bg(rgb(if index == 0 {
                            sample.icon_background
                        } else if index == 1 {
                            palette.selected
                        } else {
                            palette.control
                        }))
                        .text_color(rgb(if index == 0 {
                            sample.icon_foreground
                        } else {
                            palette.accent
                        }))
                        .text_size(px(30.))
                        .child(sample.icon)
                        .into_any_element()
                }
            })
            .collect();

        div()
            .v_flex()
            .gap_3()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(14.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(palette.foreground))
                    .child(IconName::GalleryVerticalEnd)
                    .child(locale.text("Скриншоты", "Screenshots")),
            )
            .child(div().h_flex().w_full().gap_2().children(thumbnails))
            .into_any_element()
    }

    pub(crate) fn empty_page(&self, palette: Palette, locale: Locale) -> AnyElement {
        div()
            .text_size(px(13.))
            .text_color(rgb(palette.muted))
            .child(locale.text("Здесь пока пусто", "Nothing here yet"))
            .into_any_element()
    }
}
