use std::rc::Rc;

use crate::app::LauncherApp;
use crate::app::theme::Palette;
use crate::app::ui::*;
use crate::features::mods::catalog::*;
use crate::features::mods::*;
use crate::features::settings::*;
use crate::router::Page;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::button::Button as ComponentButton;
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::scroll::{ScrollableElement, Scrollbar};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl LauncherApp {
    pub(crate) fn mods_page(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let selected_kind = self.mods.kind();
        let selected_category = self.mods.category();
        let selected_sort = self.mods.sort();
        let selected_source = self.mods.source();
        let target_filter_chip = (selected_kind == ProjectKind::Mod)
            .then(|| self.mods.compatibility_filter())
            .flatten()
            .map(|filter| {
                let label = format!(
                    "{} · Minecraft {} · {} ×",
                    locale.text("Сборка", "Build"),
                    filter.game_version,
                    filter.loader.title(locale)
                );
                UiButton::new("clear-mod-compatibility-filter")
                    .px_3()
                    .py_2()
                    .rounded(px(8.))
                    .bg(rgb(palette.selected))
                    .text_color(rgb(palette.accent))
                    .hover(|style| style.bg(rgb(palette.hover)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.mods.clear_compatibility_filter();
                        this.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
                        this.load_mod_catalog(cx, false);
                    }))
                    .child(label)
            });
        let categories: Vec<_> = ModCategory::ALL
            .into_iter()
            .map(|category| {
                let is_selected = category == selected_category;
                UiButton::new(category.id())
                    .w_full()
                    .px_2()
                    .py_2()
                    .rounded(px(6.))
                    .justify_start()
                    .text_size(px(14.))
                    .text_color(rgb(if is_selected {
                        palette.accent
                    } else {
                        palette.foreground
                    }))
                    .hover(|style| {
                        style.bg(rgb(if is_selected {
                            palette.selected
                        } else {
                            palette.hover
                        }))
                    })
                    .when(is_selected, |row| row.bg(rgb(palette.selected)))
                    .selected(is_selected)
                    .accessibility_label(category.title(locale))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.mods.set_category(category);
                        this.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
                        this.load_mod_catalog(cx, false);
                        cx.notify();
                    }))
                    .child(category.title(locale))
                    .into_any_element()
            })
            .collect();

        let sources: Vec<_> = ModSource::FILTERS
            .into_iter()
            .map(|source| {
                let is_selected = source == selected_source;
                UiButton::new(format!("mods-source-{source:?}"))
                    .w_full()
                    .px_2()
                    .py_2()
                    .rounded(px(6.))
                    .justify_start()
                    .text_size(px(14.))
                    .text_color(rgb(if is_selected {
                        palette.accent
                    } else {
                        palette.foreground
                    }))
                    .hover(|style| {
                        style.bg(rgb(if is_selected {
                            palette.selected
                        } else {
                            palette.hover
                        }))
                    })
                    .when(is_selected, |row| row.bg(rgb(palette.selected)))
                    .selected(is_selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.mods.set_source(source);
                        this.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
                        this.load_mod_catalog(cx, false);
                    }))
                    .child(source.title(locale))
                    .into_any_element()
            })
            .collect();

        let sort_owner = cx.entity().downgrade();
        let sort_menu = ComponentButton::new("mods-sort")
            .outline()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .child(IconName::ArrowDown)
                    .child(selected_sort.title(locale))
                    .child(IconName::ChevronDown),
            )
            .dropdown_menu(move |menu, _, _| {
                let sort_owner = sort_owner.clone();
                let sort_owner_downloads = sort_owner.clone();
                let sort_owner_name = sort_owner.clone();
                menu.item(
                    PopupMenuItem::new(ModSort::Popular.title(locale))
                        .checked(selected_sort == ModSort::Popular)
                        .on_click(move |_, _, cx| {
                            let _ = sort_owner.update(cx, |this, cx| {
                                this.mods.set_sort(ModSort::Popular);
                                this.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
                                this.load_mod_catalog(cx, false);
                                cx.notify();
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new(ModSort::Downloads.title(locale))
                        .checked(selected_sort == ModSort::Downloads)
                        .on_click(move |_, _, cx| {
                            let _ = sort_owner_downloads.update(cx, |this, cx| {
                                this.mods.set_sort(ModSort::Downloads);
                                this.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
                                this.load_mod_catalog(cx, false);
                                cx.notify();
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new(ModSort::Relevance.title(locale))
                        .checked(selected_sort == ModSort::Relevance)
                        .on_click(move |_, _, cx| {
                            let _ = sort_owner_name.update(cx, |this, cx| {
                                this.mods.set_sort(ModSort::Relevance);
                                this.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
                                this.load_mod_catalog(cx, false);
                                cx.notify();
                            });
                        }),
                )
            });

        let visible_count = self.mods.visible_mods().len();
        let current_page = self.mods.page();
        let total_pages = self.mods.total_pages();
        let is_loading = self.mods.is_loading();

        let page_button = |owner: WeakEntity<LauncherApp>,
                           key: String,
                           page: usize,
                           label: SharedString,
                           disabled: bool|
         -> AnyElement {
            let is_current = page == current_page;
            // `key` must be unique per rendered button: nav buttons ("Назад",
            // "Дальше") reuse page numbers, so they need a distinct key to
            // avoid duplicate element ids.
            UiButton::new(key)
                .px_3()
                .py_2()
                .rounded(px(7.))
                .bg(rgb(if is_current {
                    palette.accent
                } else {
                    palette.control
                }))
                .text_color(rgb(if is_current {
                    palette.accent_foreground
                } else {
                    palette.foreground
                }))
                .disabled(disabled || is_loading)
                .hover(|style| style.opacity(0.9))
                .on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.mods.set_page(page);
                        this.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
                        this.load_mod_catalog(cx, false);
                    });
                })
                .child(label)
                .into_any_element()
        };

        let pagination = if total_pages <= 1 {
            div().into_any_element()
        } else {
            let owner = cx.entity().downgrade();
            let mut visible: Vec<usize> = Vec::new();
            if total_pages <= 7 {
                visible.extend(1..=total_pages);
            } else {
                visible.push(1);
                let start = current_page.saturating_sub(2).max(2);
                let end = (current_page + 2).min(total_pages.saturating_sub(1));
                if start > 2 {
                    visible.push(0); // 0 = ellipsis
                }
                for p in start..=end {
                    visible.push(p);
                }
                if end < total_pages.saturating_sub(1) {
                    visible.push(0);
                }
                visible.push(total_pages);
            }
            div()
                .h_flex()
                .w_full()
                .items_center()
                .justify_center()
                .gap_2()
                .child(page_button(
                    owner.clone(),
                    "mods-page-prev".to_owned(),
                    current_page.saturating_sub(1),
                    locale.text("Назад", "Previous").into(),
                    current_page <= 1,
                ))
                .children(
                    visible
                        .into_iter()
                        .map(|page| {
                            if page == 0 {
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(palette.muted))
                                    .child("…")
                                    .into_any_element()
                            } else {
                                page_button(
                                    owner.clone(),
                                    format!("mods-page-{page}"),
                                    page,
                                    format!("{page}").into(),
                                    false,
                                )
                            }
                        })
                        .collect::<Vec<_>>(),
                )
                .child(page_button(
                    owner.clone(),
                    "mods-page-next".to_owned(),
                    current_page.saturating_add(1),
                    locale.text("Дальше", "Next").into(),
                    current_page >= total_pages,
                ))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(palette.muted))
                        .child(format!("{current_page} / {total_pages}")),
                )
                .into_any_element()
        };
        let item_sizes = Rc::new(vec![size(px(1.), px(96.)); visible_count]);
        let scroll_handle = self.mods_scroll.clone();
        let errors = self.mods.errors().iter().map(|error| {
            let message = if error.missing_api_key {
                locale.text(
                    "Чтобы включить CurseForge, задай переменную среды CURSEFORGE_API_KEY.",
                    "Set the CURSEFORGE_API_KEY environment variable to enable CurseForge.",
                )
            } else {
                error.message.as_str()
            };
            div()
                .px_3()
                .py_2()
                .rounded(px(8.))
                .bg(rgb(palette.control))
                .text_size(px(11.))
                .text_color(rgb(palette.muted))
                .child(format!("{}: {message}", error.provider.title(locale)))
                .into_any_element()
        });

        let cards_content = if visible_count > 0 {
            div()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .w_full()
                .overflow_hidden()
                .child(
                    div().size_full().min_w_0().child(
                        gpui_kit::base::v_virtual_list(
                            cx.entity(),
                            format!("{:?}-catalog-virtual-list", selected_kind),
                            item_sizes,
                            move |this, visible_range, _, cx| {
                                let projects = this.mods.visible_mods();
                                visible_range
                                    .filter_map(|index| projects.get(index).copied())
                                    .map(|project| {
                                        div()
                                            .v_flex()
                                            .h(px(96.))
                                            .w_full()
                                            .min_w_0()
                                            .pr_4()
                                            .justify_center()
                                            .child(this.mod_card(project, palette, locale, cx))
                                    })
                                    .collect::<Vec<_>>()
                            },
                        )
                        .with_sizing_behavior(ListSizingBehavior::Infer)
                        .track_scroll(&scroll_handle),
                    ),
                )
                .child(Scrollbar::vertical(&self.mods_scroll))
                .into_any_element()
        } else {
            let message = if self.mods.is_loading() {
                locale.text("Загружаем каталог…", "Loading catalog…")
            } else if self.mods.errors().is_empty() {
                locale.text("Ничего не найдено", "No results found")
            } else {
                locale.text("Не удалось загрузить каталог", "Could not load catalog")
            };
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(palette.muted))
                .child(message)
                .into_any_element()
        };

        let retry = if !self.mods.errors().is_empty() && !self.mods.is_loading() {
            Some(
                UiButton::new("retry-mod-catalog")
                    .px_3()
                    .py_2()
                    .rounded(px(7.))
                    .bg(rgb(palette.control))
                    .text_color(rgb(palette.foreground))
                    .on_click(cx.listener(|this, _, _, cx| this.load_mod_catalog(cx, false)))
                    .child(locale.text("Повторить", "Retry")),
            )
        } else {
            None
        };

        let target_name = self
            .mods
            .target_profile()
            .and_then(|id| self.instances.profile(id))
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| locale.text("Все сборки", "All builds").into());
        let profiles = self.instances.profiles().to_vec();
        let target_owner = cx.weak_entity();
        let target_menu = ComponentButton::new("catalog-target")
            .outline()
            .label(target_name)
            .dropdown_menu(move |mut menu, _, _| {
                let clear_owner = target_owner.clone();
                menu = menu.item(
                    PopupMenuItem::new(locale.text("Все сборки", "All builds")).on_click(
                        move |_, _, cx| {
                            let _ = clear_owner.update(cx, |this, cx| {
                                this.mods.clear_compatibility_filter();
                                this.load_mod_catalog(cx, false);
                            });
                        },
                    ),
                );
                for profile in &profiles {
                    let id = profile.id;
                    let filter = ModCompatibilityFilter {
                        game_version: profile.game_version.clone(),
                        loader: mod_loader_filter(profile.loader),
                    };
                    let owner = target_owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(format!(
                            "{} · {} · {}",
                            profile.name,
                            profile.game_version,
                            mod_loader_filter(profile.loader).title(locale)
                        ))
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.mods.select_profile(id, filter.clone());
                                this.refresh_installed_mods(cx);
                                this.load_mod_catalog(cx, false);
                            });
                        }),
                    );
                }
                menu
            });
        let filters = self.mods.search_filters();
        let loaders = [
            None,
            Some(ModLoaderFilter::Fabric),
            Some(ModLoaderFilter::Forge),
            Some(ModLoaderFilter::NeoForge),
        ]
        .into_iter()
        .map(|loader| {
            let selected = filters.loader == loader;
            let label = loader.map_or(
                locale.text("Все загрузчики", "All loaders"),
                |loader| loader.title(locale),
            );
            UiButton::new(format!("catalog-loader-{loader:?}"))
                .w_full()
                .px_2()
                .py_2()
                .rounded(px(6.))
                .justify_start()
                .text_size(px(14.))
                .text_color(rgb(if selected {
                    palette.accent
                } else {
                    palette.foreground
                }))
                .hover(|style| {
                    style.bg(rgb(if selected {
                        palette.selected
                    } else {
                        palette.hover
                    }))
                })
                .when(selected, |row| row.bg(rgb(palette.selected)))
                .selected(selected)
                .disabled(self.mods.target_profile().is_some())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.mods.set_loader_filter(loader);
                    this.load_mod_catalog(cx, false);
                }))
                .child(label)
        });
        let filter_count = usize::from(filters.game_version.is_some())
            + usize::from(filters.loader.is_some())
            + usize::from(selected_category != ModCategory::All)
            + usize::from(selected_source != ModSource::All);
        let version_query = self.mod_version_input.read(cx).value().to_string();
        let version_choices: Vec<_> = self
            .versions
            .manifest()
            .into_iter()
            .flat_map(|manifest| &manifest.versions)
            .filter(|version| {
                version.version_type == "release" && version.id.contains(version_query.trim())
            })
            // The version manifest is already sorted newest-first.
            .map(|version| {
                let id = version.id.clone();
                UiButton::new(format!("catalog-version-{id}"))
                    .w_full()
                    .flex_shrink_0()
                    .px_2()
                    .py_2()
                    .rounded(px(6.))
                    .justify_start()
                    .text_size(px(14.))
                    .text_color(rgb(
                        if filters.game_version.as_deref() == Some(id.as_str()) {
                            palette.accent
                        } else {
                            palette.foreground
                        },
                    ))
                    .hover(|style| {
                        style.bg(rgb(
                            if filters.game_version.as_deref() == Some(id.as_str()) {
                                palette.selected
                            } else {
                                palette.hover
                            },
                        ))
                    })
                    .when(
                        filters.game_version.as_deref() == Some(id.as_str()),
                        |row| row.bg(rgb(palette.selected)),
                    )
                    .disabled(self.mods.target_profile().is_some())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.mod_version_input
                            .update(cx, |input, cx| input.set_value(id.clone(), window, cx));
                    }))
                    .child(version.id.clone())
            })
            .collect();
        let filters_panel = div()
            .v_flex()
            .w(px(280.))
            .flex_shrink_0()
            .min_h_0()
            .overflow_y_scrollbar()
            .p_2()
            .pr_4()
            .gap_3()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .child(
                div()
                    .h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::BOLD)
                            .child(format!(
                                "{} · {filter_count}",
                                locale.text("Фильтры", "Filters")
                            )),
                    )
                    .child(
                        UiButton::new("reset-catalog-filters")
                            .text_size(px(10.))
                            .text_color(rgb(palette.muted))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.mods.reset_filters();
                                this.mod_version_input
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                                this.load_mod_catalog(cx, false);
                            }))
                            .child(locale.text("Сбросить", "Reset")),
                    ),
            )
            .child(Self::filter_section(
                FilterSection::Version,
                locale
                    .text("Версия Minecraft", "Minecraft version")
                    .to_owned(),
                self.mods.filters_collapsed(FilterSection::Version),
                palette,
                cx,
                div()
                    .v_flex()
                    .gap_2()
                    .child(
                        Input::new(&self.mod_version_input)
                            .w_full()
                            .disabled(self.mods.target_profile().is_some())
                            .aria_label(
                                locale.text("Точная версия Minecraft", "Exact Minecraft version"),
                            ),
                    )
                    .child(
                        // Own scrollable list so the versions scroll inside the
                        // filter instead of only showing the first entries.
                        // The max height must live on an ancestor: if it sits
                        // directly on the scroll element, the content box gets
                        // clamped too and the list never gains a scroll offset.
                        div().v_flex().w_full().max_h(px(220.)).child(
                            div().v_flex().flex_1().min_h_0().overflow_hidden().child(
                                div()
                                    .v_flex()
                                    .size_full()
                                    // Reserve room on the right so the
                                    // overlay scrollbar does not cover
                                    // the version labels.
                                    .pr_4()
                                    .overflow_y_scrollbar()
                                    .children(version_choices),
                            ),
                        ),
                    )
                    .when(self.mods.target_profile().is_some(), |this| {
                        this.child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(palette.muted))
                                .child(locale.text(
                                    "Версия и загрузчик взяты из сборки",
                                    "Version and loader follow the build",
                                )),
                        )
                    })
                    .into_any_element(),
            ))
            .when(selected_kind == ProjectKind::Mod, |this| {
                this.child(div().h(px(1.)).bg(rgb(palette.border)))
                    .child(Self::filter_section(
                        FilterSection::Loader,
                        locale.text("Загрузчик", "Loader").to_owned(),
                        self.mods.filters_collapsed(FilterSection::Loader),
                        palette,
                        cx,
                        div().v_flex().gap_1().children(loaders).into_any_element(),
                    ))
                    .child(div().h(px(1.)).bg(rgb(palette.border)))
                    .child(Self::filter_section(
                        FilterSection::Categories,
                        locale.text("Категории", "Categories").to_owned(),
                        self.mods.filters_collapsed(FilterSection::Categories),
                        palette,
                        cx,
                        div()
                            .v_flex()
                            .gap_1()
                            .children(categories)
                            .into_any_element(),
                    ))
            })
            .child(div().h(px(1.)).bg(rgb(palette.border)))
            .child(Self::filter_section(
                FilterSection::Source,
                locale.text("Источник", "Source").to_owned(),
                self.mods.filters_collapsed(FilterSection::Source),
                palette,
                cx,
                div().v_flex().gap_1().children(sources).into_any_element(),
            ));
        let checking = self
            .instances
            .profiles()
            .iter()
            .any(|profile| self.mods.installed.is_loading(profile.id));
        let incomplete = self
            .instances
            .profiles()
            .iter()
            .any(|profile| self.mods.installed.warning(profile.id).is_some());
        div().h_flex().items_stretch().w_full().flex_1().min_h_0().min_w_0().gap_4()
            .child(filters_panel)
            .child(div().v_flex().flex_1().min_h_0().min_w_0().gap_3()
                .child(self.search_bar(palette, locale))
                .when(selected_kind == ProjectKind::Mod, |this| this.child(div().h_flex().items_center().gap_2()
                    .child(div().text_size(px(12.)).text_color(rgb(palette.muted)).child(locale.text("Установка в", "Install into")))
                    .child(target_menu)))
                .when_some(target_filter_chip, |this, chip| this.child(chip))
                .child(div().h_flex().w_full().items_center().justify_between()
                    .child(div().h_flex().items_center().gap_2().text_size(px(12.)).text_color(rgb(palette.muted))
                        .child(format!("{} {}", self.mods.total_results(), selected_kind.title(locale)))
                        .when(is_loading, |this| this.child(locale.text("· Загрузка…", "· Loading…"))))
                    .child(sort_menu))
                .when(checking || incomplete, |this| this.child(div().h_flex().items_center().gap_2()
                    .child(div().text_size(px(11.)).text_color(rgb(palette.muted)).child(if checking {
                        locale.text("Проверяем установленные моды…", "Checking installed mods…")
                    } else { locale.text("Не все файлы удалось распознать. Отметки могут быть неполными.", "Some files could not be identified. Installed markers may be incomplete.") }))
                    .when(!checking, |this| this.child(UiButton::new("retry-mod-inventory").text_size(px(11.))
                        .on_click(cx.listener(|this, _, _, cx| { this.refresh_installed_mods(cx); cx.notify(); }))
                        .child(locale.text("Проверить снова", "Check again"))))))
                .children(errors).when_some(retry, |this, button| this.child(button))
                .child(cards_content).child(pagination))
            .into_any_element()
    }
    /// Collapsible section header for the catalog filters sidebar. Closed
    /// sections leave only the header row visible.
    pub(crate) fn filter_section(
        section: FilterSection,
        title: String,
        collapsed: bool,
        palette: Palette,
        cx: &Context<Self>,
        body: AnyElement,
    ) -> AnyElement {
        let is_open = !collapsed;
        div()
            .v_flex()
            .gap_1()
            .child(
                UiButton::new(format!("filters-section-{section:?}"))
                    .w_full()
                    .px_2()
                    .py_1()
                    .rounded(px(6.))
                    .text_color(rgb(palette.foreground))
                    .hover(|style| style.bg(rgb(palette.hover)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.mods.toggle_filters(section);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(title),
                            )
                            .child(if is_open {
                                IconName::ChevronUp
                            } else {
                                IconName::ChevronDown
                            }),
                    ),
            )
            .when(is_open, |panel| panel.child(body))
            .into_any_element()
    }

    pub(crate) fn mod_install_action(
        &self,
        project: &ModProject,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let target = self.mods.target_profile();
        let checking = self.instances.profiles().iter().any(|profile| {
            target.is_none_or(|id| profile.id == id) && self.mods.installed.is_loading(profile.id)
        });
        let busy = target.is_some_and(|id| self.instances.runtime(&id).is_busy());
        let already_installed =
            target.is_some_and(|id| self.mods.installed.contains(id, &project.id));
        // Per-item state: only the card whose download is in flight changes
        // its label. Profile-wide flags (busy/checking) must not leak their
        // text onto unrelated cards.
        let installing = self.mods.installed.is_installing(&project.id);
        let download_project = project.clone();
        let uninstall_target = self.uninstall_target(&project.id);
        div()
            .v_flex()
            .items_end()
            .gap_2()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        UiButton::new(format!("install-project-{}", project.id))
                            .px_3()
                            .py_2()
                            .rounded(px(7.))
                            .bg(rgb(if already_installed {
                                palette.control
                            } else {
                                palette.accent
                            }))
                            .text_color(rgb(if already_installed {
                                palette.muted
                            } else {
                                palette.accent_foreground
                            }))
                            .disabled(already_installed || busy || checking || installing)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_project_install_dialog(download_project.clone(), window, cx)
                            }))
                            .child(if installing {
                                locale.text("Устанавливаем…", "Installing…")
                            } else if already_installed {
                                locale.text("Установлен", "Installed")
                            } else {
                                locale.text("Установить", "Install")
                            }),
                    )
                    .when_some(uninstall_target, |row, target_id| {
                        let removal_label = locale
                            .text("Удалить из сборки", "Remove from build")
                            .to_owned();
                        let uninstall_project = project.id.clone();
                        let target_blocked = checking
                            || self.instances.runtime(&target_id).is_busy()
                            || self.mods.installed.is_loading(target_id);
                        row.child(
                            UiButton::new(format!("uninstall-project-{}", project.id))
                                .px_3()
                                .py_2()
                                .rounded(px(7.))
                                .bg(rgb(palette.control))
                                .text_color(rgb(0xef6a65))
                                .hover(|style| style.bg(rgb(0x3b1b1b)))
                                .disabled(target_blocked)
                                .accessibility_label(removal_label.clone())
                                .tooltip(move |window, cx| {
                                    Tooltip::new(removal_label.clone()).build(window, cx)
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.uninstall_mod(uninstall_project.clone(), target_id, cx);
                                }))
                                .child(IconName::Delete),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(crate) fn content_install_action(
        &self,
        project: &ModProject,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let installing = self.mods.installed.is_installing(&project.id);
        let download_project = project.clone();
        UiButton::new(format!("install-content-{}", project.id))
            .px_3()
            .py_2()
            .rounded(px(7.))
            .bg(rgb(palette.accent))
            .text_color(rgb(palette.accent_foreground))
            .hover(|style| style.opacity(0.9))
            .disabled(installing)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_project_install_dialog(download_project.clone(), window, cx)
            }))
            .child(if installing {
                locale.text("Скачиваем…", "Downloading…")
            } else {
                locale.text("Установить", "Install")
            })
            .into_any_element()
    }

    pub(crate) fn mod_card(
        &self,
        project: &ModProject,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let description = self.mods.card_description(project, locale);
        let tooltip_description = description.clone();
        let project_id = project.id.clone();
        let selected_project = project.clone();
        let tags: Vec<_> = project
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

        let details_button = UiButton::new(format!("open-mod-{}", project.id))
            .flex_1()
            .min_w_0()
            .px_3()
            .py_2()
            .rounded(px(8.))
            .text_color(rgb(palette.foreground))
            .accessibility_label(locale.text("Открыть описание проекта", "Open project details"))
            .on_click(cx.listener(move |this, _, window, cx| {
                let request_id = this.mods.select_mod(selected_project.clone());
                this.load_mod_details(
                    selected_project.id.clone(),
                    selected_project.source,
                    request_id,
                    cx,
                );
                this.navigate_to(Page::ModDetails, window, cx);
            }))
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_3()
                    .child(mod_project_thumbnail(project, px(56.)))
                    .child(
                        div()
                            .v_flex()
                            .flex_1()
                            .min_w_0()
                            .w_full()
                            .gap_1()
                            .items_start()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(palette.foreground))
                                    .w_full()
                                    .min_w_0()
                                    .truncate()
                                    .child(project.name.clone()),
                            )
                            .child(
                                div()
                                    .id(format!("mod-description-{}", project.id))
                                    .w_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.muted))
                                    .truncate()
                                    .tooltip(move |window, cx| {
                                        Tooltip::new(tooltip_description.clone()).build(window, cx)
                                    })
                                    .child(description.clone()),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .items_center()
                                    .overflow_hidden()
                                    .gap_3()
                                    .text_size(px(10.))
                                    .text_color(rgb(palette.muted))
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_1()
                                            .child(IconName::User)
                                            .child(project.author.clone()),
                                    )
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_1()
                                            .child(IconName::ArrowDown)
                                            .child(project.downloads_label.clone()),
                                    )
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_1()
                                            .child(IconName::Heart)
                                            .child(project.likes_label.clone()),
                                    )
                                    .children(tags),
                            ),
                    ),
            );

        let action = match project.kind {
            ProjectKind::Mod => self.mod_install_action(project, palette, locale, cx),
            ProjectKind::ResourcePack | ProjectKind::Shader => {
                self.content_install_action(project, palette, locale, cx)
            }
            ProjectKind::Modpack => {
                let page_url = project.page_url.clone();
                UiButton::new(format!("open-project-{project_id}"))
                    .px_3()
                    .py_2()
                    .rounded(px(7.))
                    .bg(rgb(palette.control))
                    .text_color(rgb(palette.foreground))
                    .hover(|style| style.bg(rgb(palette.hover)))
                    .on_click(move |_, _, cx| cx.open_url(&page_url))
                    .child(locale.text("Открыть", "Open"))
                    .into_any_element()
            }
        };

        div()
            .id(format!("mod-card-{}", project.id))
            .h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .hover(|style| style.bg(rgb(palette.hover)))
            .border_1()
            .border_color(rgb(palette.border))
            .child(details_button)
            .child(
                div()
                    .h_flex()
                    .w(px(220.))
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .pr_3()
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(6.))
                            .bg(rgb(palette.control))
                            .text_size(px(10.))
                            .text_color(rgb(palette.muted))
                            .child(project.game_version.clone()),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(6.))
                            .bg(rgb(palette.control))
                            .text_size(px(9.))
                            .text_color(rgb(palette.muted))
                            .child(project.source.title(locale)),
                    )
                    .child(action),
            )
            .into_any_element()
    }
}
