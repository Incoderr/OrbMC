use super::widgets::*;
use super::{CatalogEvent, CatalogView};
use crate::features::mods::catalog::*;
use crate::features::mods::*;
use crate::features::settings::*;
use crate::shared::theme::Palette;
use crate::shared::ui::{empty_page, selectable};
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl CatalogView {
    pub(crate) fn mod_details_page(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(sample) = self.mods.selected_mod() else {
            return empty_page(palette, locale);
        };
        let active_tab = self.mods.detail_tab();
        let project_url = sample.page_url.clone();
        let project_kind = sample.kind;

        let back_button = UiButton::new("mod-details-back")
            .cursor_pointer()
            .px_2()
            .py_2()
            .rounded(px(7.))
            .bg(rgb(palette.background))
            .text_color(rgb(palette.muted))
            .hover(|style| style.bg(rgb(palette.hover)))
            .accessibility_label(locale.text("Назад в каталог", "Back to catalog"))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.mods.clear_selected_mod();
                cx.emit(CatalogEvent::BackToCatalog(project_kind));
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

        let primary_action = match project_kind {
            ProjectKind::Mod => self.mod_install_action(sample, palette, locale, cx),
            ProjectKind::ResourcePack | ProjectKind::Shader => {
                self.content_install_action(sample, palette, locale, cx)
            }
            ProjectKind::Modpack => self.modpack_install_action(sample, palette, locale, cx),
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
                            .child(selectable("project-title", sample.name.clone())),
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
                            .child({
                                let author_project = sample.clone();
                                UiButton::new("details-author")
                                    .cursor_pointer()
                                    .h_flex()
                                    .items_center()
                                    .gap_1()
                                    .text_color(rgb(palette.muted))
                                    .hover(|style| style.text_color(rgb(palette.accent)))
                                    .tooltip(move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(locale.text(
                                            "Все проекты автора",
                                            "All projects by this author",
                                        ))
                                        .build(window, cx)
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.show_author(&author_project, cx);
                                    }))
                                    .child(IconName::User)
                                    .child(sample.author.clone())
                            })
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
                            .cursor_pointer()
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
                    .cursor_pointer()
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
        let license = self
            .mods
            .project_info()
            .and_then(|info| info.license.clone())
            .filter(|license| !license.trim().is_empty())
            .unwrap_or_else(|| sample.license.clone());

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
                license.clone(),
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
            ))
            // Real provider details once the project endpoint responds.
            .when_some(self.mods.project_info().cloned(), |this, info| {
                this.child(detail_meta_row(
                    IconName::Calendar,
                    locale.text("Дата создания", "Created"),
                    info.date_created.clone().unwrap_or_else(|| "—".into()),
                    palette,
                ))
                .child(detail_meta_row(
                    IconName::ArrowDown,
                    locale.text("Загрузок", "Downloads"),
                    info.downloads
                        .map(format_count)
                        .unwrap_or_else(|| "—".into()),
                    palette,
                ))
                .child(detail_meta_row(
                    IconName::Heart,
                    locale.text("Оценок", "Follows"),
                    info.likes.map(format_count).unwrap_or_else(|| "—".into()),
                    palette,
                ))
                .children(info.source_url.map(|url| {
                    detail_link_button(
                        format!("source-link-{}", sample.id),
                        locale.text("Исходники", "Source code"),
                        url,
                        palette,
                    )
                }))
                .children(info.issues_url.map(|url| {
                    detail_link_button(
                        format!("issues-link-{}", sample.id),
                        locale.text("Сообщения об ошибках", "Issue tracker"),
                        url,
                        palette,
                    )
                }))
                .children(info.wiki_url.map(|url| {
                    detail_link_button(
                        format!("wiki-link-{}", sample.id),
                        locale.text("Вики", "Wiki"),
                        url,
                        palette,
                    )
                }))
                .children(info.discord_url.map(|url| {
                    detail_link_button(
                        format!("discord-link-{}", sample.id),
                        locale.text("Discord", "Discord"),
                        url,
                        palette,
                    )
                }))
            });

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
                        gpui_kit::component::text::markdown(enlarge_markdown_badges(description))
                    }
                }
                .selectable(true)
                .w_full()
                .text_size(px(15.))
                .text_color(rgb(palette.foreground))
                .style(
                    gpui_kit::component::text::TextViewStyle::default()
                        .paragraph_gap(rems(0.65))
                        .heading_font_size(|level, _| {
                            px(match level {
                                1 => 28.,
                                2 => 24.,
                                3 => 20.,
                                4 => 18.,
                                5 => 16.,
                                _ => 15.,
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
                    self.mod_screenshots(sample, palette, locale, cx)
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
                        .cursor_pointer()
                        .px_3()
                        .py_2()
                        .rounded(px(7.))
                        .bg(rgb(palette.accent))
                        .text_color(rgb(palette.accent_foreground))
                        .hover(|style| style.opacity(0.9))
                        .on_click(cx.listener({
                            let project = sample.clone();
                            move |this, _, window, cx| {
                                this.open_project_install_dialog(project.clone(), window, cx);
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
            ModDetailTab::Screenshots => self.mod_screenshots(sample, palette, locale, cx),
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
        cx: &Context<Self>,
    ) -> AnyElement {
        let header = div()
            .h_flex()
            .items_center()
            .gap_2()
            .text_size(px(14.))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(palette.foreground))
            .child(IconName::GalleryVerticalEnd)
            .child(locale.text("Скриншоты", "Screenshots"));

        let gallery: Vec<ModGalleryImage> = self
            .mods
            .project_info()
            .map(|info| info.gallery.clone())
            .unwrap_or_default();

        let body = if self.mods.info_loading() {
            div()
                .text_size(px(11.))
                .text_color(rgb(palette.muted))
                .child(locale.text(
                    "Загружаем скриншоты с сервера проекта…",
                    "Loading project screenshots…",
                ))
                .into_any_element()
        } else if gallery.is_empty() {
            match self.mods.info_error() {
                Some(error) => div()
                    .text_size(px(11.))
                    .text_color(rgb(palette.muted))
                    .child(format!(
                        "{}{error}",
                        locale.text("Скриншоты недоступны: ", "Screenshots unavailable: ")
                    ))
                    .into_any_element(),
                None => div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(rgb(palette.muted))
                    .child(locale.text(
                        "У проекта нет скриншотов.",
                        "This project has no screenshots.",
                    ))
                    .into_any_element(),
            }
        } else {
            let gallery_len = gallery.len();
            let index = self.mods.gallery_index().min(gallery_len - 1);
            let current = &gallery[index];
            let project_id = sample.id.clone();

            let viewer = div()
                .id(format!("mod-gallery-viewer-{project_id}"))
                .w_full()
                .min_w_0()
                .h(px(300.))
                .rounded(px(10.))
                .overflow_hidden()
                .bg(rgb(palette.background))
                .border_1()
                .border_color(rgb(palette.border))
                .cursor_pointer()
                .child(
                    img(current
                        .full_url
                        .clone()
                        .unwrap_or_else(|| current.url.clone()))
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .with_loading(move || gallery_placeholder(palette))
                    .with_fallback(move || gallery_placeholder(palette)),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_gallery_dialog(index, window, cx);
                }));

            let bar = div()
                .h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_2()
                .child(
                    UiButton::new(format!("mod-gallery-prev-{project_id}"))
                        .cursor_pointer()
                        .px_3()
                        .py_2()
                        .rounded(px(7.))
                        .bg(rgb(palette.control))
                        .text_color(rgb(palette.foreground))
                        .hover(|style| style.bg(rgb(palette.hover)))
                        .accessibility_label(
                            locale.text("Предыдущий скриншот", "Previous screenshot"),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.mods.set_gallery_index(
                                (index + gallery_len - 1) % gallery_len,
                                gallery_len,
                            );
                            cx.notify();
                        }))
                        .child(IconName::ChevronLeft),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(palette.muted))
                        .child(format!("{} / {gallery_len}", index + 1)),
                )
                .child(
                    UiButton::new(format!("mod-gallery-next-{project_id}"))
                        .cursor_pointer()
                        .px_3()
                        .py_2()
                        .rounded(px(7.))
                        .bg(rgb(palette.control))
                        .text_color(rgb(palette.foreground))
                        .hover(|style| style.bg(rgb(palette.hover)))
                        .accessibility_label(locale.text("Следующий скриншот", "Next screenshot"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.mods
                                .set_gallery_index((index + 1) % gallery_len, gallery_len);
                            cx.notify();
                        }))
                        .child(IconName::ChevronRight),
                )
                .child(div().flex_1())
                .child(
                    UiButton::new(format!("mod-gallery-fullscreen-{project_id}"))
                        .cursor_pointer()
                        .px_3()
                        .py_2()
                        .rounded(px(7.))
                        .bg(rgb(palette.control))
                        .text_color(rgb(palette.foreground))
                        .hover(|style| style.bg(rgb(palette.hover)))
                        .accessibility_label(
                            locale.text("Открыть на весь экран", "View fullscreen"),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_gallery_dialog(index, window, cx);
                        }))
                        .child(IconName::Expand),
                );

            let strip = if gallery_len > 1 {
                let thumbs: Vec<_> = gallery
                    .iter()
                    .enumerate()
                    .map(|(thumb_index, image)| {
                        let is_active = thumb_index == index;
                        let project_id = project_id.clone();
                        let icon = sample.icon;
                        let icon_background = sample.icon_background;
                        let icon_foreground = sample.icon_foreground;
                        let url = if image.thumbnail.trim().is_empty() {
                            image.url.clone()
                        } else {
                            image.thumbnail.clone()
                        };
                        div()
                            .id(format!("mod-gallery-thumb-{project_id}-{thumb_index}"))
                            .flex_shrink_0()
                            .w(px(64.))
                            .h(px(44.))
                            .rounded(px(6.))
                            .overflow_hidden()
                            .bg(rgb(palette.control))
                            .border_2()
                            .border_color(rgb(if is_active {
                                palette.accent
                            } else {
                                palette.border
                            }))
                            .cursor_pointer()
                            .hover(|style| style.opacity(0.85))
                            .child(
                                img(url)
                                    .size_full()
                                    .object_fit(ObjectFit::Cover)
                                    .with_loading(move || {
                                        mod_icon_placeholder(
                                            icon,
                                            icon_background,
                                            icon_foreground,
                                            px(44.),
                                        )
                                    })
                                    .with_fallback(move || {
                                        mod_icon_placeholder(
                                            icon,
                                            icon_background,
                                            icon_foreground,
                                            px(44.),
                                        )
                                    }),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.mods.set_gallery_index(thumb_index, gallery_len);
                                cx.notify();
                            }))
                    })
                    .collect();
                div()
                    .id(format!("mod-gallery-strip-{project_id}"))
                    .h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .overflow_x_scrollbar()
                    .children(thumbs)
                    .into_any_element()
            } else {
                div().into_any_element()
            };

            let caption = if current.title.trim().is_empty() {
                current.description.trim().to_owned()
            } else {
                current.title.trim().to_owned()
            };

            div()
                .v_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(viewer)
                .child(bar)
                .when(!caption.is_empty(), |this| {
                    this.child(
                        div()
                            .w_full()
                            .min_w_0()
                            .text_size(px(11.))
                            .text_color(rgb(palette.muted))
                            .child(caption),
                    )
                })
                .child(strip)
                .into_any_element()
        };

        div()
            .v_flex()
            .gap_3()
            .child(header)
            .child(body)
            .into_any_element()
    }
}

/// Small metadata-panel button that opens a project link.
fn detail_link_button(
    id: String,
    label: impl Into<String>,
    url: String,
    palette: Palette,
) -> AnyElement {
    UiButton::new(id)
        .cursor_pointer()
        .w_full()
        .px_2()
        .py_1()
        .rounded(px(6.))
        .bg(rgb(palette.control))
        .text_color(rgb(palette.foreground))
        .hover(|style| style.bg(rgb(palette.hover)))
        .on_click(move |_, _, cx| cx.open_url(&url))
        .child(label.into())
        .into_any_element()
}

/// Fills gallery slots while a screenshot loads.
fn gallery_placeholder(palette: Palette) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(28.))
        .text_color(rgb(palette.muted))
        .child(IconName::GalleryVerticalEnd)
        .into_any_element()
}

/// Markdown renders inline images at text-line height, which makes provider
/// badges nearly unreadable. Give loader badges extra height and keep other
/// linked badges compact; leave standalone screenshots at their original size.
fn enlarge_markdown_badges(markdown: String) -> String {
    let mut rendered = String::with_capacity(markdown.len());
    let mut remaining = markdown.as_str();

    while let Some(image_start) = remaining.find("![") {
        rendered.push_str(&remaining[..image_start]);
        let image = &remaining[image_start..];
        let linked_image = image_start > 0 && remaining[..image_start].ends_with('[');
        if let Some((image_len, badge)) = enlarged_markdown_badge(image, linked_image) {
            rendered.push_str(&badge);
            remaining = &image[image_len..];
        } else {
            rendered.push_str("![");
            remaining = &image[2..];
        }
    }

    rendered.push_str(remaining);
    rendered
}

fn enlarged_markdown_badge(image: &str, linked_image: bool) -> Option<(usize, String)> {
    let alt_end = image[2..].find("](")? + 2;
    let alt = &image[2..alt_end];
    let alt_lower = alt.to_ascii_lowercase();
    let is_loader_badge =
        alt_lower.starts_with("available for ") || alt_lower.starts_with("available on ");
    if !linked_image && !is_loader_badge {
        return None;
    }

    let url_start = alt_end + 2;
    let destination_and_title = &image[url_start..];
    let mut depth = 1usize;
    let mut escaped = false;
    let mut close = None;
    for (offset, character) in destination_and_title.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let destination = destination_and_title[..close].split_whitespace().next()?;
    if !destination.starts_with("https://") {
        return None;
    }

    let source = escape_html_attribute(destination);
    let alt = escape_html_attribute(alt);
    let height = if is_loader_badge { 32 } else { 20 };
    let image_len = url_start + close + 1;
    Some((
        image_len,
        format!(r#"<img src="{source}" alt="{alt}" height="{height}" />"#),
    ))
}

fn escape_html_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

impl CatalogView {
    pub(crate) fn home_mod_card(
        &self,
        sample: &ModSample,
        palette: Palette,
        locale: Locale,
    ) -> AnyElement {
        div()
            .v_flex()
            .flex_1()
            .min_w_0()
            .gap_3()
            .p_3()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .child(mod_thumbnail(sample, px(34.)))
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(palette.foreground))
                            .child(sample.name),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.muted))
                            .child(locale.text("от ", "by "))
                            .child(sample.author),
                    ),
            )
            .into_any_element()
    }
}
