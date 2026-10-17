use std::rc::Rc;

use crate::app::LauncherApp;
use crate::app::theme::Palette;
use crate::app::ui::*;
use crate::features::instances::*;
use crate::features::settings::*;
use crate::features::versions::*;
use crate::platform::profile_store;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::WindowExt;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl LauncherApp {
    pub(crate) fn open_version_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.load_version_manifests(cx);
        self.versions.close_game_version_menu();
        self.profile_name_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.loader_version_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        let owner = cx.entity().downgrade();
        let title = self
            .settings
            .locale()
            .text("Добавить Minecraft", "Add Minecraft");
        window.open_dialog(cx, move |dialog, _, _| {
            let content_owner = owner.clone();
            dialog
                .w(px(560.))
                .title(title)
                .content(move |content, _, cx| {
                    let Some(owner) = content_owner.upgrade() else {
                        return content;
                    };
                    let app = owner.read(cx);
                    let palette =
                        Palette::for_settings(app.settings.theme(), app.settings.accent());
                    let locale = app.settings.locale();
                    content.child(app.version_picker_body(
                        owner.clone(),
                        &app.profile_name_input,
                        &app.loader_version_input,
                        palette,
                        locale,
                    ))
                })
        });
    }

    pub(crate) fn version_picker_body(
        &self,
        owner: Entity<Self>,
        profile_name_input: &Entity<InputState>,
        loader_version_input: &Entity<InputState>,
        palette: Palette,
        locale: Locale,
    ) -> AnyElement {
        let current_loader = self.versions.loader_choice();
        let loader_options = [
            (LoaderChoice::Vanilla, "Vanilla"),
            (LoaderChoice::Fabric, "Fabric"),
            (LoaderChoice::NeoForge, "NeoForge"),
            (LoaderChoice::Forge, "Forge"),
        ];
        let loader_buttons = loader_options.into_iter().map(|(loader, label)| {
            let is_selected = current_loader == loader;
            let loader_owner = owner.downgrade();
            UiButton::new(format!("version-loader-{label}"))
                .flex_1()
                .px_2()
                .py_2()
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(if is_selected {
                    palette.accent
                } else {
                    palette.border
                }))
                .bg(rgb(if is_selected {
                    palette.accent
                } else {
                    palette.control
                }))
                .text_color(rgb(if is_selected {
                    palette.accent_foreground
                } else {
                    palette.muted
                }))
                .hover(|style| style.bg(rgb(palette.hover)))
                .on_click(move |_, _, cx| {
                    if let Some(owner) = loader_owner.upgrade() {
                        let _ = owner.update(cx, |this, cx| {
                            this.versions.set_loader_choice(loader);
                            this.load_fabric_game_loaders(cx);
                            cx.notify();
                        });
                    }
                })
                .child(label)
        });

        let mode_buttons = [
            (
                LoaderVersionMode::Stable,
                locale.text("Стабильная", "Stable"),
            ),
            (
                LoaderVersionMode::Latest,
                locale.text("Последняя", "Latest"),
            ),
            (LoaderVersionMode::Other, locale.text("Другая", "Other")),
        ]
        .into_iter()
        .map(|(mode, label)| {
            let is_selected = self.versions.loader_version_mode() == mode;
            let mode_owner = owner.downgrade();
            UiButton::new(format!("loader-version-mode-{mode:?}"))
                .flex_1()
                .px_2()
                .py_2()
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(if is_selected {
                    palette.accent
                } else {
                    palette.border
                }))
                .bg(rgb(if is_selected {
                    palette.accent
                } else {
                    palette.control
                }))
                .text_color(rgb(if is_selected {
                    palette.accent_foreground
                } else {
                    palette.muted
                }))
                .hover(|style| style.bg(rgb(palette.hover)))
                .on_click(move |_, _, cx| {
                    if let Some(owner) = mode_owner.upgrade() {
                        let _ = owner.update(cx, |this, cx| {
                            this.versions.set_loader_version_mode(mode);
                            cx.notify();
                        });
                    }
                })
                .child(label)
        });

        let game_version_label = self.versions.game_version().map_or_else(
            || {
                locale
                    .text("Загрузка манифеста…", "Loading manifest…")
                    .to_string()
            },
            str::to_owned,
        );
        let game_menu =
            if self.versions.game_version_menu_open() {
                if let Some(manifest) = self.versions.manifest() {
                    let show_all_versions = self.versions.shows_all_game_versions();
                    let visible_indices = Rc::new(
                        manifest
                            .versions
                            .iter()
                            .enumerate()
                            .filter_map(|(index, version)| {
                                (show_all_versions || version.version_type == "release")
                                    .then_some(index)
                            })
                            .collect::<Vec<_>>(),
                    );
                    let item_sizes = Rc::new(vec![size(px(1.), px(42.)); visible_indices.len()]);
                    let list_owner = owner.clone();
                    let scroll_handle = self.manifest_scroll.clone();
                    let visible_indices_for_render = visible_indices.clone();
                    let toggle_owner = owner.downgrade();
                    let release_count = manifest
                        .versions
                        .iter()
                        .filter(|version| version.version_type == "release")
                        .count();
                    let toggle_label = if show_all_versions {
                        locale
                            .text("Показывать только релизы", "Show releases only")
                            .to_string()
                    } else {
                        format!(
                            "{} ({})",
                            locale.text("Показать все версии", "Show all versions"),
                            manifest.versions.len().saturating_sub(release_count)
                        )
                    };
                    Some(
                        div()
                            .v_flex()
                            .w_full()
                            .gap_2()
                            .child(
                                UiButton::new("toggle-manifest-version-filter")
                                    .w_full()
                                    .px_2()
                                    .py_1()
                                    .rounded(px(6.))
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.muted))
                                    .hover(|style| style.bg(rgb(palette.hover)))
                                    .on_click(move |_, _, cx| {
                                        if let Some(owner) = toggle_owner.upgrade() {
                                            let _ = owner.update(cx, |this, cx| {
                                                this.versions.toggle_all_game_versions();
                                                this.manifest_scroll
                                                    .scroll_to_item(0, ScrollStrategy::Top);
                                                cx.notify();
                                            });
                                        }
                                    })
                                    .child(toggle_label),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .h(px(250.))
                                    .rounded(px(8.))
                                    .border_1()
                                    .border_color(rgb(palette.border))
                                    .bg(rgb(palette.surface))
                                    .overflow_hidden()
                                    .child(
                                        gpui_kit::base::v_virtual_list(
                                            owner.clone(),
                                            "minecraft-version-manifest",
                                            item_sizes,
                                            move |this, visible_range, _, _| {
                                                let Some(manifest) = this.versions.manifest()
                                                else {
                                                    return Vec::new();
                                                };
                                                visible_range
                                                .filter_map(|visible_index| {
                                                    let source_index = *visible_indices_for_render
                                                        .get(visible_index)?;
                                                    manifest.versions.get(source_index)
                                                })
                                                .map(|version| {
                                            let id = version.id.clone();
                                            let selected =
                                                this.versions.game_version() == Some(id.as_str());
                                            let is_release = version.version_type == "release";
                                            let version_type = version.version_type.clone();
                                            let select_owner = list_owner.downgrade();
                                            let selected_id = id.clone();
                                            UiButton::new(format!("select-game-version-{id}"))
                                                .w_full()
                                                .h(px(42.))
                                                .px_3()
                                                .pr_4()
                                                .rounded(px(6.))
                                                .bg(rgb(if selected {
                                                    palette.selected
                                                } else {
                                                    palette.surface
                                                }))
                                                .text_color(rgb(palette.foreground))
                                                .hover(|style| style.bg(rgb(palette.hover)))
                                                .on_click(move |_, _, cx| {
                                                    if let Some(owner) = select_owner.upgrade() {
                                                        let selected_id = selected_id.clone();
                                                        let _ = owner.update(cx, |this, cx| {
                                                            this.versions
                                                                .select_game_version(selected_id);
                                                            this.load_fabric_game_loaders(cx);
                                                            cx.notify();
                                                        });
                                                    }
                                                })
                                                .child(
                                                    div()
                                                        .h_flex()
                                                        .w_full()
                                                        .items_center()
                                                        .justify_between()
                                                        .child(id)
                                                        .child(
                                                            div()
                                                                .text_size(px(10.))
                                                                .text_color(rgb(if is_release {
                                                                    palette.accent
                                                                } else {
                                                                    palette.muted
                                                                }))
                                                                .child(version_type_label(
                                                                    &version_type,
                                                                    locale,
                                                                )),
                                                        ),
                                                )
                                                })
                                                .collect::<Vec<_>>()
                                            },
                                        )
                                        .with_sizing_behavior(ListSizingBehavior::Infer)
                                        .track_scroll(&scroll_handle)
                                        .into_any_element(),
                                    )
                                    .child(Scrollbar::vertical(&scroll_handle)),
                            )
                            .into_any_element(),
                    )
                } else if self.versions.is_loading() {
                    Some(
                        div()
                            .h(px(100.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .text_color(rgb(palette.muted))
                            .child(IconName::LoaderCircle)
                            .child(locale.text("Загружаем версии…", "Loading versions…"))
                            .into_any_element(),
                    )
                } else {
                    let retry_owner = owner.downgrade();
                    Some(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .text_color(rgb(palette.muted))
                            .child(locale.text("Манифест недоступен", "Manifest unavailable"))
                            .child(
                                UiButton::new("retry-version-manifests")
                                    .px_3()
                                    .py_2()
                                    .rounded(px(7.))
                                    .bg(rgb(palette.control))
                                    .text_color(rgb(palette.foreground))
                                    .on_click(move |_, _, cx| {
                                        if let Some(owner) = retry_owner.upgrade() {
                                            let _ = owner.update(cx, |this, cx| {
                                                this.load_version_manifests(cx);
                                            });
                                        }
                                    })
                                    .child(locale.text("Повторить", "Retry")),
                            )
                            .into_any_element(),
                    )
                }
            } else {
                None
            };

        let open_versions_owner = owner.downgrade();
        let profile_input = profile_name_input.clone();
        let loader_input = loader_version_input.clone();
        let create_owner = owner.downgrade();
        let show_loader_version = current_loader != LoaderChoice::Vanilla;
        let show_custom_loader_version =
            show_loader_version && self.versions.loader_version_mode() == LoaderVersionMode::Other;
        let loader_version_feedback = if show_loader_version {
            if show_custom_loader_version {
                Some(
                    locale
                        .text("Укажи номер версии вручную.", "Enter a loader version.")
                        .to_string(),
                )
            } else if let Some(version) = self.versions.loader_version(None) {
                Some(format!(
                    "{} {version}",
                    locale.text("Будет выбрана версия", "Selected version:")
                ))
            } else if current_loader == LoaderChoice::Fabric {
                self.versions.game_version().map(|game_version| {
                    if self.versions.fabric_game_loading(game_version) {
                        locale
                            .text(
                                "Загружаем совместимые версии Fabric…",
                                "Loading compatible Fabric versions…",
                            )
                            .to_string()
                    } else if let Some(error) = self.versions.fabric_game_error(game_version) {
                        format!(
                            "{}: {error}",
                            locale.text("Не удалось загрузить Fabric", "Could not load Fabric")
                        )
                    } else if self.versions.fabric_game_loader_count(game_version) == Some(0) {
                        if game_version == "1.7.10" {
                            locale
                                .text(
                                    "Официальный Fabric не поддерживает Minecraft 1.7.10. Для него нужен отдельный загрузчик LegacyFabric.",
                                    "Official Fabric does not support Minecraft 1.7.10. It requires the separate LegacyFabric loader.",
                                )
                                .to_string()
                        } else {
                            locale
                                .text(
                                    "Fabric API не предлагает совместимую версию для выбранного Minecraft.",
                                    "The Fabric API has no compatible loader for this Minecraft version.",
                                )
                                .to_string()
                        }
                    } else {
                        locale
                            .text(
                                "Для этой версии нет доступной версии загрузчика.",
                                "No compatible loader version is available.",
                            )
                            .to_string()
                    }
                })
            } else {
                Some(
                    locale
                        .text(
                            "Для этой версии игры нет доступной версии загрузчика.",
                            "No loader version is available for this game version.",
                        )
                        .to_string(),
                )
            }
        } else {
            None
        };
        let manifest_errors = self.versions.error().map(|error| {
            div()
                .text_size(px(10.))
                .text_color(rgb(palette.muted))
                .child(error.to_owned())
        });

        div()
            .v_flex()
            .w_full()
            .gap_3()
            .text_color(rgb(palette.foreground))
            .child(field_label(locale.text("Название", "Name")))
            .child(
                Input::new(profile_name_input)
                    .w_full()
                    .h(px(40.))
                    .px_3()
                    .rounded(px(8.))
                    .bg(rgb(palette.control))
                    .border_1()
                    .border_color(rgb(palette.border)),
            )
            .child(field_label(locale.text("Загрузчик", "Loader")))
            .child(div().h_flex().w_full().gap_2().children(loader_buttons))
            .child(field_label(locale.text("Версия игры", "Game version")))
            .child(
                UiButton::new("manifest-game-version-dropdown")
                    .w_full()
                    .h(px(40.))
                    .px_3()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.control))
                    .text_color(rgb(palette.foreground))
                    .hover(|style| style.bg(rgb(palette.hover)))
                    .on_click(move |_, _, cx| {
                        if let Some(owner) = open_versions_owner.upgrade() {
                            let _ = owner.update(cx, |this, cx| {
                                let opening = !this.versions.game_version_menu_open();
                                if opening {
                                    let selected_index = this
                                        .versions
                                        .game_version()
                                        .and_then(|selected| {
                                            this.versions.manifest().and_then(|manifest| {
                                                manifest
                                                    .versions
                                                    .iter()
                                                    .position(|version| version.id == selected)
                                            })
                                        })
                                        .unwrap_or(0);
                                    this.manifest_scroll
                                        .scroll_to_item(selected_index, ScrollStrategy::Center);
                                }
                                this.versions.toggle_game_version_menu();
                                cx.notify();
                            });
                        }
                    })
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .child(game_version_label)
                            .child(IconName::ChevronDown),
                    ),
            )
            .children(game_menu)
            .when(show_loader_version, |this| {
                this.child(field_label(
                    locale.text("Версия загрузчика", "Loader version"),
                ))
                .child(div().h_flex().w_full().gap_2().children(mode_buttons))
                .when(show_custom_loader_version, |this| {
                    this.child(
                        Input::new(loader_version_input)
                            .w_full()
                            .h(px(40.))
                            .px_3()
                            .rounded(px(8.))
                            .bg(rgb(palette.control))
                            .border_1()
                            .border_color(rgb(palette.border)),
                    )
                })
            })
            .when_some(loader_version_feedback, |this, feedback| {
                this.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(palette.muted))
                        .child(feedback),
                )
            })
            .children(manifest_errors)
            .child(
                UiButton::new("create-minecraft-profile")
                    .w_full()
                    .px_3()
                    .py_2()
                    .rounded(px(8.))
                    .bg(rgb(palette.accent))
                    .text_color(rgb(palette.accent_foreground))
                    .hover(|style| style.opacity(0.9))
                    .on_click(move |_, window, cx| {
                        if let Some(owner) = create_owner.upgrade() {
                            let Some(game_version) =
                                owner.read(cx).versions.game_version().map(str::to_owned)
                            else {
                                return;
                            };
                            let name = profile_input.read(cx).value().trim().to_owned();
                            let custom_loader_version =
                                loader_input.read(cx).value().trim().to_owned();
                            let loader = owner.read(cx).versions.loader_choice();
                            let loader_version = if loader == LoaderChoice::Vanilla {
                                None
                            } else {
                                owner
                                    .read(cx)
                                    .versions
                                    .loader_version(Some(&custom_loader_version))
                            };
                            if loader != LoaderChoice::Vanilla && loader_version.is_none() {
                                return;
                            }
                            let _ = owner.update(cx, move |this, cx| {
                                let loader_name = match loader {
                                    LoaderChoice::Vanilla => "Vanilla",
                                    LoaderChoice::Fabric => "Fabric",
                                    LoaderChoice::NeoForge => "NeoForge",
                                    LoaderChoice::Forge => "Forge",
                                };
                                let profile_name = if name.is_empty() {
                                    format!("{loader_name} {game_version}")
                                } else {
                                    name
                                };
                                let selection = if loader == LoaderChoice::Vanilla {
                                    format!("{profile_name} · Minecraft {game_version}")
                                } else {
                                    let Some(version) = loader_version.as_deref() else {
                                        return;
                                    };
                                    format!(
                                        "{profile_name} · {game_version} · {loader_name} {version}"
                                    )
                                };
                                let installation_root = this.settings.download_directory().clone();
                                let profile = InstanceProfile::new(
                                    profile_name,
                                    game_version,
                                    loader,
                                    loader_version,
                                    installation_root,
                                );
                                let profile_id = profile.id;
                                this.instances.add(profile);
                                this.versions.select(selection);
                                this.versions.close_game_version_menu();
                                if let Err(error) = profile_store::save(this.instances.profiles()) {
                                    tracing::error!(%error, "failed to save Minecraft profile");
                                }
                                this.start_profile_install(profile_id, cx);
                                cx.notify();
                            });
                            window.close_dialog(cx);
                        }
                    })
                    .child(locale.text("Создать профиль", "Create profile")),
            )
            .into_any_element()
    }
}
