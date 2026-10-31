use super::DetailsTab;
use super::widgets::*;
use super::{InstancesView, InstancesViewEvent};
use crate::features::instances::*;
use crate::features::mods::*;
use crate::features::settings::*;
use crate::features::versions::*;
use crate::shared::theme::Palette;
use crate::shared::ui::selectable;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, Disableable, StyledExt};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button as ComponentButton;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl InstancesView {
    pub(crate) fn instances_page(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let profiles = self
            .instances
            .read(cx)
            .profiles()
            .iter()
            .map(|profile| self.instance_card(profile, palette, locale, cx))
            .collect::<Vec<_>>();

        div()
            .v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .pr_4()
            .overflow_y_scrollbar()
            .gap_4()
            .child(
                div().h_flex().w_full().items_center().justify_end().child(
                    UiButton::new("instances-create-build")
                        .cursor_pointer()
                        .px_3()
                        .py_2()
                        .rounded(px(8.))
                        .bg(rgb(palette.accent))
                        .text_color(rgb(palette.accent_foreground))
                        .hover(|style| style.opacity(0.9))
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(InstancesViewEvent::CreateBuild);
                        }))
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap_2()
                                .child(IconName::Plus)
                                .child(locale.text("Создать сборку", "Create build")),
                        ),
                ),
            )
            .when(profiles.is_empty(), |this| {
                this.child(
                    div()
                        .w_full()
                        .p_6()
                        .rounded(px(10.))
                        .bg(rgb(palette.surface))
                        .border_1()
                        .border_color(rgb(palette.border))
                        .text_size(px(12.))
                        .text_color(rgb(palette.muted))
                        .child(locale.text(
                            "Сборок пока нет. Создай первую сборку, чтобы установить игру и моды.",
                            "No builds yet. Create one to install Minecraft and add mods.",
                        )),
                )
            })
            .children(profiles)
            .into_any_element()
    }

    pub(crate) fn instances_section(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let cards = self
            .instances
            .read(cx)
            .recent_profiles()
            .into_iter()
            .map(|profile| self.instance_card(profile, palette, locale, cx))
            .collect::<Vec<_>>();

        div()
            .v_flex()
            .w_full()
            .gap_3()
            .children(cards)
            .into_any_element()
    }

    pub(crate) fn instance_card(
        &self,
        profile: &InstanceProfile,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let runtime = self.instances.read(cx).runtime(&profile.id);
        let is_busy = runtime.is_busy();
        let is_installed = profile.installed_version_id.is_some();
        let install_paused = matches!(&runtime, InstanceRuntime::Installing { paused: true, .. });
        let install_active = matches!(&runtime, InstanceRuntime::Installing { .. });
        let transfer_active = matches!(
            &runtime,
            InstanceRuntime::DownloadingMod { .. } | InstanceRuntime::DownloadingContent { .. }
        );
        let mut install_progress = None;
        let (status_text, status_detail, action_text) = match &runtime {
            InstanceRuntime::NotInstalled => (
                locale.text("Не установлена", "Not installed").to_string(),
                locale
                    .text("Нажми, чтобы скачать игру.", "Click to download the game.")
                    .to_string(),
                locale.text("Скачать", "Download"),
            ),
            InstanceRuntime::Installing {
                stage,
                received,
                total_bytes,
                speed_bps,
                completed_tasks,
                total_tasks,
                paused,
            } => {
                install_progress = install_progress_view(
                    *received,
                    *total_bytes,
                    *speed_bps,
                    *completed_tasks,
                    *total_tasks,
                    palette,
                    locale,
                );
                let mut detail = stage_label(stage, locale).to_owned();
                if let Some(total) = total_bytes.filter(|total| *total > 0) {
                    detail.push_str(&format!(
                        " · {} / {}",
                        format_bytes(*received),
                        format_bytes(total)
                    ));
                } else if let Some(total_tasks) = total_tasks.filter(|total| *total > 0) {
                    detail.push_str(&format!(
                        " · {} / {} {}",
                        completed_tasks,
                        total_tasks,
                        locale.text("файлов", "files")
                    ));
                }
                if *speed_bps > 0. {
                    detail.push_str(&format!(" · {}", format_speed(*speed_bps)));
                }
                (
                    if *paused {
                        locale.text("Пауза", "Paused").to_string()
                    } else {
                        locale.text("Скачивание", "Downloading").to_string()
                    },
                    detail,
                    if *paused {
                        locale.text("Продолжить…", "Resume…")
                    } else {
                        locale.text("Скачивание…", "Downloading…")
                    },
                )
            }
            InstanceRuntime::DownloadingMod { name, transfer }
            | InstanceRuntime::DownloadingContent { name, transfer } => {
                let progress = transfer.progress();
                install_progress = install_progress_view(
                    progress.received,
                    progress.total,
                    progress.speed_bps,
                    0,
                    None,
                    palette,
                    locale,
                );
                let mut detail = name.clone();
                if let Some((done, total)) = progress.files {
                    detail.push_str(&format!(
                        " · {done} / {total} {}",
                        locale.text("файлов", "files")
                    ));
                }
                if progress.received > 0 {
                    detail.push_str(&format!(" · {}", format_bytes(progress.received)));
                    if let Some(total) = progress.total {
                        detail.push_str(&format!(" / {}", format_bytes(total)));
                    }
                }
                if progress.speed_bps > 0. {
                    detail.push_str(&format!(" · {}", format_speed(progress.speed_bps)));
                }
                (
                    if matches!(&runtime, InstanceRuntime::DownloadingMod { .. }) {
                        locale.text("Скачивание мода", "Downloading mod").to_owned()
                    } else {
                        locale
                            .text("Установка контента", "Installing content")
                            .to_owned()
                    },
                    detail,
                    locale.text("Скачивание…", "Downloading…"),
                )
            }
            InstanceRuntime::ContentDownloadFailed { message } => (
                locale
                    .text(
                        "Не удалось установить контент",
                        "Content installation failed",
                    )
                    .to_owned(),
                message.clone(),
                if is_installed {
                    locale.text("Играть", "Play")
                } else {
                    locale.text("Установить", "Install")
                },
            ),
            InstanceRuntime::ContentDownloaded { filename } => (
                locale
                    .text("Контент установлен", "Content installed")
                    .to_owned(),
                format!(
                    "{}{}",
                    locale.text("Файл добавлен: ", "File added: "),
                    filename
                ),
                if is_installed {
                    locale.text("Играть", "Play")
                } else {
                    locale.text("Установить игру", "Install game")
                },
            ),
            InstanceRuntime::ModDownloadFailed { message } => (
                locale
                    .text("Не удалось скачать мод", "Mod download failed")
                    .to_owned(),
                message.clone(),
                if is_installed {
                    locale.text("Играть", "Play")
                } else {
                    locale.text("Установить", "Install")
                },
            ),
            InstanceRuntime::ModDownloaded { filename } => (
                locale.text("Мод скачан", "Mod downloaded").to_owned(),
                format!(
                    "{}{}",
                    locale.text("Файл добавлен: ", "File added: "),
                    filename
                ),
                if is_installed {
                    locale.text("Играть", "Play")
                } else {
                    locale.text("Установить игру", "Install game")
                },
            ),
            InstanceRuntime::Ready => (String::new(), String::new(), locale.text("Играть", "Play")),
            InstanceRuntime::Launching => (
                locale.text("Запуск игры", "Starting game").to_string(),
                locale
                    .text("Подготавливаем процесс Java…", "Starting the Java process…")
                    .to_string(),
                locale.text("Запуск…", "Starting…"),
            ),
            InstanceRuntime::PreparingJava { detail } => (
                locale.text("Подготовка Java", "Preparing Java").to_string(),
                detail.clone(),
                locale.text("Подготовка…", "Preparing…"),
            ),
            InstanceRuntime::Running { process_id } => (
                locale.text("Игра запущена", "Game is running").to_string(),
                format!("{} {process_id}", locale.text("Процесс", "Process")),
                locale.text("Игра запущена", "Running"),
            ),
            InstanceRuntime::Removing => (
                locale
                    .text("Удаление сборки", "Deleting profile")
                    .to_string(),
                locale
                    .text("Удаляем локальные данные…", "Removing profile data…")
                    .to_string(),
                locale.text("Удаление…", "Deleting…"),
            ),
            InstanceRuntime::RemovalFailed { message } => (
                locale
                    .text("Не удалось удалить", "Could not delete")
                    .to_string(),
                message.clone(),
                if is_installed {
                    locale.text("Играть", "Play")
                } else {
                    locale.text("Повторить скачивание", "Retry download")
                },
            ),
            InstanceRuntime::Failed { message } => (
                locale
                    .text(
                        if is_installed {
                            "Не удалось запустить"
                        } else {
                            "Не удалось скачать"
                        },
                        if is_installed {
                            "Could not launch"
                        } else {
                            "Download failed"
                        },
                    )
                    .to_string(),
                message.clone(),
                if is_installed {
                    locale.text("Играть снова", "Play again")
                } else {
                    locale.text("Повторить скачивание", "Retry download")
                },
            ),
        };
        let loader = loader_title(profile.loader);
        let profile_version = profile.loader_version.as_deref().unwrap_or("");
        let profile_details = if profile.loader == LoaderChoice::Vanilla {
            format!("Minecraft {} · {loader}", profile.game_version)
        } else {
            format!(
                "Minecraft {} · {loader} {profile_version}",
                profile.game_version
            )
        };
        let profile_id = profile.id;
        let folder_label = locale.text("Открыть папку сборки", "Open build folder");
        let mod_filter = ModCompatibilityFilter {
            game_version: profile.game_version.clone(),
            loader: mod_loader_filter(profile.loader),
        };

        div()
            .v_flex()
            .w_full()
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
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(palette.foreground))
                                    .child(selectable(
                                        format!("build-name-{profile_id}"),
                                        profile.name.clone(),
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.muted))
                                    .child(profile_details),
                            ),
                    )
                    .when(!status_text.is_empty(), |row| {
                        row.child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(px(6.))
                                .bg(rgb(palette.control))
                                .text_size(px(10.))
                                .text_color(rgb(if is_installed {
                                    palette.accent
                                } else {
                                    palette.muted
                                }))
                                .child(status_text),
                        )
                    }),
            )
            .children(install_progress)
            .when(!status_detail.is_empty(), |card| {
                // Crash output can be long: it scrolls and can be copied.
                card.child(
                    div()
                        .id(format!("build-status-{profile_id}"))
                        .max_h(px(160.))
                        .overflow_y_scroll()
                        .text_size(px(11.))
                        .text_color(rgb(palette.muted))
                        .child(selectable(
                            format!("build-status-text-{profile_id}"),
                            status_detail,
                        )),
                )
            })
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .when(install_active, |row| {
                        let pause_label = if install_paused {
                            locale.text("Продолжить", "Resume").to_owned()
                        } else {
                            locale.text("Пауза", "Pause").to_owned()
                        };
                        let cancel_label = locale
                            .text("Отменить установку", "Cancel install")
                            .to_owned();
                        row.child(
                            UiButton::new(format!("pause-instance-{profile_id}"))
                                .cursor_pointer()
                                .px_3()
                                .py_2()
                                .rounded(px(8.))
                                .bg(rgb(palette.control))
                                .text_color(rgb(palette.foreground))
                                .hover(|style| style.bg(rgb(palette.hover)))
                                .accessibility_label(pause_label.clone())
                                .tooltip(move |window, cx| {
                                    Tooltip::new(pause_label.clone()).build(window, cx)
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let applied = if install_paused {
                                        this.instances.update(cx, |instances, _| {
                                            instances.resume_install(profile_id)
                                        })
                                    } else {
                                        this.instances.update(cx, |instances, _| {
                                            instances.pause_install(profile_id)
                                        })
                                    };
                                    if applied {
                                        cx.notify();
                                    }
                                }))
                                .child(if install_paused {
                                    IconName::Play
                                } else {
                                    IconName::Pause
                                }),
                        )
                        .child(
                            UiButton::new(format!("cancel-instance-{profile_id}"))
                                .cursor_pointer()
                                .px_3()
                                .py_2()
                                .rounded(px(8.))
                                .bg(rgb(palette.control))
                                .text_color(rgb(palette.muted))
                                .hover(|style| style.bg(rgb(palette.hover)))
                                .accessibility_label(cancel_label.clone())
                                .tooltip(move |window, cx| {
                                    Tooltip::new(cancel_label.clone()).build(window, cx)
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.instances.update(cx, |instances, _| {
                                        instances.cancel_install(profile_id)
                                    }) {
                                        cx.notify();
                                    }
                                }))
                                .child(IconName::CircleX),
                        )
                    })
                    .when(transfer_active, |row| {
                        let cancel_label = locale
                            .text("Отменить загрузку", "Cancel download")
                            .to_owned();
                        row.child(
                            UiButton::new(format!("cancel-transfer-{profile_id}"))
                                .cursor_pointer()
                                .px_3()
                                .py_2()
                                .rounded(px(8.))
                                .bg(rgb(palette.control))
                                .text_color(rgb(palette.muted))
                                .hover(|style| style.bg(rgb(palette.hover)))
                                .accessibility_label(cancel_label.clone())
                                .tooltip(move |window, cx| {
                                    Tooltip::new(cancel_label.clone()).build(window, cx)
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.instances.update(cx, |instances, _| {
                                        instances.cancel_transfer(profile_id)
                                    }) {
                                        cx.notify();
                                    }
                                }))
                                .child(IconName::CircleX),
                        )
                    })
                    .child(
                        UiButton::new(format!("open-instance-folder-{profile_id}"))
                            .cursor_pointer()
                            .px_3()
                            .py_2()
                            .rounded(px(8.))
                            .bg(rgb(palette.control))
                            .text_color(rgb(palette.muted))
                            .hover(|style| style.bg(rgb(palette.hover)))
                            .accessibility_label(folder_label)
                            .tooltip(move |window, cx| Tooltip::new(folder_label).build(window, cx))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_profile_directory(profile_id, cx);
                            }))
                            .child(IconName::FolderOpen),
                    )
                    .child(
                        UiButton::new(format!("add-mods-instance-{profile_id}"))
                            .cursor_pointer()
                            .px_3()
                            .py_2()
                            .rounded(px(8.))
                            .bg(rgb(palette.control))
                            .text_color(rgb(palette.foreground))
                            .hover(|style| style.bg(rgb(palette.hover)))
                            .disabled(is_busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.catalog.update(cx, |catalog, _| {
                                    catalog.select_profile(profile_id, mod_filter.clone())
                                });
                                cx.emit(InstancesViewEvent::OpenCatalog);
                            }))
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(IconName::Puzzle)
                                    .child(locale.text("Добавить моды", "Add mods")),
                            ),
                    )
                    .child(
                        UiButton::new(format!("open-instance-{profile_id}"))
                            .cursor_pointer()
                            .px_3()
                            .py_2()
                            .rounded(px(8.))
                            .bg(rgb(palette.control))
                            .text_color(rgb(palette.foreground))
                            .hover(|style| style.bg(rgb(palette.hover)))
                            .accessibility_label(
                                locale.text("Открыть страницу сборки", "Open build page"),
                            )
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(InstancesViewEvent::OpenBuild(profile_id));
                            }))
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(IconName::Box)
                                    .child(locale.text("Открыть сборку", "Open build")),
                            ),
                    )
                    .child(
                        UiButton::new(format!("play-instance-{profile_id}"))
                            .cursor_pointer()
                            .px_4()
                            .py_2()
                            .rounded(px(8.))
                            .bg(rgb(palette.accent))
                            .text_color(rgb(palette.accent_foreground))
                            .hover(|style| style.opacity(0.9))
                            .disabled(is_busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.start_profile_launch(profile_id, cx);
                            }))
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(if is_installed {
                                        IconName::Play
                                    } else {
                                        IconName::Download
                                    })
                                    .child(action_text),
                            ),
                    )
                    .when(
                        is_installed
                            && orbmc_core::minecraft::quick_play::potentially_supported(
                                &profile.game_version,
                            ),
                        |row| {
                            row.child(
                                ComponentButton::new(format!("world-instance-{profile_id}"))
                                    .cursor_pointer()
                                    .outline()
                                    .label(locale.text("В мир…", "World…"))
                                    .disabled(is_busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_world_picker(profile_id, window, cx)
                                    })),
                            )
                        },
                    )
                    .child(
                        UiButton::new(format!("settings-instance-{profile_id}"))
                            .cursor_pointer()
                            .px_3()
                            .py_2()
                            .rounded(px(8.))
                            .bg(rgb(palette.control))
                            .text_color(rgb(palette.muted))
                            .hover(|style| style.bg(rgb(palette.hover)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_profile_settings(profile_id, window, cx);
                            }))
                            .child(IconName::Settings),
                    )
                    .child(
                        UiButton::new(format!("delete-instance-{profile_id}"))
                            .cursor_pointer()
                            .px_3()
                            .py_2()
                            .rounded(px(8.))
                            .bg(rgb(palette.control))
                            .text_color(rgb(0xef6a65))
                            .hover(|style| style.bg(rgb(0x3b1b1b)))
                            .disabled(is_busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_delete_profile_confirmation(profile_id, window, cx);
                            }))
                            .child(IconName::Delete),
                    ),
            )
            .into_any_element()
    }
}
impl InstancesView {
    /// Build details page: metadata of the build and its installed mods with
    /// enable/disable switches.
    pub(crate) fn instance_details_page(
        &self,
        profile_id: crate::features::instances::InstanceId,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(profile) = self.instances.read(cx).profile(profile_id) else {
            return crate::shared::ui::empty_page(palette, locale);
        };
        let profile = profile.clone();
        let runtime = self.instances.read(cx).runtime(&profile_id);
        let is_busy = runtime.is_busy();
        let scan_loading = self.catalog.read(cx).is_scanning(profile_id);
        let loader = loader_title(profile.loader);
        let loader_version = profile.loader_version.as_deref().unwrap_or("");
        let details = if profile.loader == LoaderChoice::Vanilla {
            format!("Minecraft {} · {loader}", profile.game_version)
        } else {
            format!(
                "Minecraft {} · {loader} {loader_version}",
                profile.game_version
            )
        };
        let entries: Vec<installed::InstalledModEntry> =
            self.catalog.read(cx).build_mods(profile_id).to_vec();

        let back_button = UiButton::new("instance-details-back")
            .cursor_pointer()
            .px_2()
            .py_2()
            .rounded(px(7.))
            .bg(rgb(palette.background))
            .text_color(rgb(palette.muted))
            .hover(|style| style.bg(rgb(palette.hover)))
            .accessibility_label(locale.text("Назад к сборкам", "Back to builds"))
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(InstancesViewEvent::OpenBuilds);
            }))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .child(IconName::ArrowLeft)
                    .child(locale.text("Назад", "Back")),
            );

        let folder_label = locale.text("Открыть папку сборки", "Open build folder");
        let play_label = if profile.installed_version_id.is_some() {
            locale.text("Играть", "Play")
        } else {
            locale.text("Установить игру", "Install game")
        };

        let rows = entries
            .into_iter()
            .map(|entry| {
                let row_filename = entry.filename.clone();
                let disabled = entry.disabled;
                // Display name falls back to the file name for legacy
                // records, so the list never shows an empty label.
                let display_name = entry.name.clone().filter(|name| !name.trim().is_empty());
                let author = entry.author.clone().filter(|name| !name.trim().is_empty());
                let version = entry
                    .version
                    .clone()
                    .filter(|label| !label.trim().is_empty());

                // Controlled switch: `checked` is the enabled state; the
                // handler receives the requested new value.
                let switch_owner = cx.entity().downgrade();
                let switch_filename = row_filename.clone();
                let toggle = Switch::new(format!(
                    "mod-switch-{}-{row_filename}",
                    profile_id.uuid().as_simple()
                ))
                .checked(!disabled)
                .disabled(is_busy || scan_loading)
                .accessibility_label(
                    if disabled {
                        locale.text("Включить мод", "Enable mod")
                    } else {
                        locale.text("Отключить мод", "Disable mod")
                    }
                    .to_owned(),
                )
                .on_click(move |checked: &bool, _, cx| {
                    let Some(owner) = switch_owner.upgrade() else {
                        return;
                    };
                    owner.update(cx, |this, cx| {
                        this.catalog.update(cx, |catalog, cx| {
                            catalog.toggle_mod_disabled(
                                profile_id,
                                switch_filename.clone(),
                                !*checked,
                                cx,
                            )
                        });
                    });
                });

                // One delete action per row, by file name: catalog-recorded
                // jars and hand-added mods are removed the same way, and the
                // identical action block keeps the columns aligned.
                let removal_label = locale
                    .text("Удалить из сборки", "Remove from build")
                    .to_owned();
                let removal_filename = row_filename.clone();
                let uninstalls = [UiButton::new(format!(
                    "unbuild-mod-{}-{row_filename}",
                    profile_id.uuid().as_simple()
                ))
                .cursor_pointer()
                .px_2()
                .py_2()
                .rounded(px(6.))
                .bg(rgb(palette.control))
                .text_color(rgb(0xef6a65))
                .hover(|style| style.bg(rgb(0x3b1b1b)))
                .disabled(is_busy || scan_loading)
                .accessibility_label(removal_label.clone())
                .tooltip(move |window, cx| Tooltip::new(removal_label.clone()).build(window, cx))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.catalog.update(cx, |catalog, cx| {
                        catalog.uninstall_mod(removal_filename.clone(), profile_id, cx)
                    });
                }))
                .child(IconName::Delete)
                .into_any_element()];

                let (label_color, sub_color) = if disabled {
                    (palette.muted, palette.muted)
                } else {
                    (palette.foreground, palette.muted)
                };
                let icon = mod_row_icon(
                    entry.icon_url.as_deref(),
                    entry.icon_path.as_deref(),
                    palette,
                );
                let identity = div()
                    .v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(label_color))
                            .truncate()
                            .child(display_name.unwrap_or(mod_title_fallback(&row_filename))),
                    )
                    .when_some(author, |block, author| {
                        let author_name = author.clone();
                        block.child(
                            UiButton::new(format!(
                                "mod-author-{}-{row_filename}",
                                profile_id.uuid().as_simple()
                            ))
                            .cursor_pointer()
                            .h_flex()
                            .items_center()
                            .gap_1p5()
                            .text_size(px(11.))
                            .text_color(rgb(sub_color))
                            .hover(|style| style.text_color(rgb(palette.accent)))
                            .tooltip(move |window, cx| {
                                Tooltip::new(
                                    locale
                                        .text("Все проекты автора", "All projects by this author"),
                                )
                                .build(window, cx)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let name = author_name.clone();
                                this.catalog
                                    .update(cx, |catalog, _| catalog.filter_by_author(name));
                                cx.emit(InstancesViewEvent::OpenCatalog);
                            }))
                            .child(IconName::CircleUser)
                            .child(div().min_w_0().truncate().child(author)),
                        )
                    });
                let release = div()
                    .v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .when_some(version.clone(), |block, version| {
                        block
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(label_color))
                                    .truncate()
                                    .child(version),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(sub_color))
                                    .truncate()
                                    .child(row_filename.clone()),
                            )
                    })
                    .when(version.is_none(), |block| {
                        // Unknown version: the file name is the only label
                        // the user can rely on.
                        block.child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(sub_color))
                                .truncate()
                                .child(row_filename.clone()),
                        )
                    });

                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded(px(8.))
                    .bg(rgb(palette.background))
                    .child(icon)
                    .child(identity)
                    .child(release)
                    .children(uninstalls)
                    .child(toggle)
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        div()
            .v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_4()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_3()
                    .child(back_button)
                    .child(
                        div()
                            .v_flex()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(21.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(palette.foreground))
                                    .truncate()
                                    .child(selectable(
                                        format!("build-title-{profile_id}"),
                                        profile.name.clone(),
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(palette.muted))
                                    .child(details),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .flex_1()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            // Keep the shared card actions close at hand.
                            .child(
                                UiButton::new(format!("details-folder-instance-{profile_id}"))
                                    .cursor_pointer()
                                    .px_2()
                                    .py_2()
                                    .rounded(px(7.))
                                    .bg(rgb(palette.control))
                                    .text_color(rgb(palette.muted))
                                    .hover(|style| style.bg(rgb(palette.hover)))
                                    .accessibility_label(folder_label)
                                    .tooltip(move |window, cx| {
                                        Tooltip::new(folder_label).build(window, cx)
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_profile_directory(profile_id, cx);
                                    }))
                                    .child(IconName::FolderOpen),
                            )
                            .child(
                                UiButton::new(format!("details-play-instance-{profile_id}"))
                                    .cursor_pointer()
                                    .px_4()
                                    .py_2()
                                    .rounded(px(7.))
                                    .bg(rgb(palette.accent))
                                    .text_color(rgb(palette.accent_foreground))
                                    .hover(|style| style.opacity(0.9))
                                    .disabled(is_busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.start_profile_launch(profile_id, cx);
                                    }))
                                    .child(play_label),
                            ),
                    ),
            )
            .child(
                div()
                    .v_flex()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .gap_2()
                    .child(self.details_tabs(profile_id, rows.len(), palette, locale, cx))
                    .when(self.details_tab == DetailsTab::Log, |section| {
                        section.child(self.game_log_panel(profile_id, palette, locale, cx))
                    })
                    .when(
                        self.details_tab == DetailsTab::Mods && rows.is_empty(),
                        |section| {
                            section.child(
                                div()
                                    .w_full()
                                    .p_6()
                                    .rounded(px(10.))
                                    .bg(rgb(palette.surface))
                                    .border_1()
                                    .border_color(rgb(palette.border))
                                    .text_size(px(12.))
                                    .text_color(rgb(palette.muted))
                                    .when(scan_loading, |panel| {
                                        panel.child(locale.text(
                                            "Проверяем папку модов…",
                                            "Checking the mods directory…",
                                        ))
                                    })
                                    .when(!scan_loading, |panel| {
                                        panel.child(locale.text(
                                            "Моды ещё не установлены. Добавьте их из каталога.",
                                            "No mods installed yet. Add them from the catalog.",
                                        ))
                                    }),
                            )
                        },
                    )
                    .when(
                        self.details_tab == DetailsTab::Mods && !rows.is_empty(),
                        |section| {
                            section.child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_h_0()
                                    .min_w_0()
                                    .pr_4()
                                    .overflow_y_scrollbar()
                                    .gap_2()
                                    .children(rows),
                            )
                        },
                    ),
            )
            .into_any_element()
    }
}

impl InstancesView {
    /// "Mods" / "Log" switch of a build's page.
    fn details_tabs(
        &self,
        profile_id: InstanceId,
        mod_count: usize,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let running = matches!(
            self.instances.read(cx).runtime(&profile_id),
            InstanceRuntime::Running { .. }
        );
        let tab = |tab: DetailsTab, label: String| {
            let active = self.details_tab == tab;
            UiButton::new(format!("build-tab-{tab:?}"))
                .cursor_pointer()
                .px_3()
                .py_1p5()
                .rounded(px(7.))
                .bg(rgb(if active {
                    palette.selected
                } else {
                    palette.background
                }))
                .text_size(px(13.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(if active {
                    palette.foreground
                } else {
                    palette.muted
                }))
                .hover(|style| style.bg(rgb(palette.hover)))
                .selected(active)
                .on_click(cx.listener(move |this, _, _, cx| this.set_details_tab(tab, cx)))
                .child(label)
        };
        div()
            .h_flex()
            .items_center()
            .gap_1()
            .child(tab(
                DetailsTab::Mods,
                format!("{} · {mod_count}", locale.text("Моды", "Mods")),
            ))
            .child(tab(
                DetailsTab::Log,
                if running {
                    format!("{} ●", locale.text("Журнал", "Log"))
                } else {
                    locale.text("Журнал", "Log").to_owned()
                },
            ))
            .into_any_element()
    }

    /// Live output of the game with copy, clear and follow controls.
    fn game_log_panel(
        &self,
        profile_id: InstanceId,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let text = self
            .log_text
            .as_ref()
            .filter(|(id, _, _)| *id == profile_id)
            .map(|(_, _, text)| text.clone())
            .unwrap_or_default();
        let tool = |id: &str, icon: IconName, label: &'static str, active: bool| {
            UiButton::new(format!("{id}-{profile_id}"))
                .cursor_pointer()
                .px_2()
                .py_1p5()
                .rounded(px(6.))
                .bg(rgb(if active {
                    palette.selected
                } else {
                    palette.control
                }))
                .text_color(rgb(if active {
                    palette.accent
                } else {
                    palette.muted
                }))
                .hover(|style| style.bg(rgb(palette.hover)))
                .accessibility_label(label)
                .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
                .child(icon)
        };
        let toolbar = div()
            .h_flex()
            .items_center()
            .gap_1()
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.))
                    .text_color(rgb(palette.muted))
                    .child(locale.text(
                        "Вывод игры за последний запуск",
                        "Game output of the latest launch",
                    )),
            )
            .child(
                tool(
                    "log-follow",
                    IconName::ArrowDown,
                    locale.text("Прокручивать к новым строкам", "Follow new lines"),
                    self.log_follow,
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle_log_follow(cx))),
            )
            .child(
                tool(
                    "log-copy",
                    IconName::Copy,
                    locale.text("Скопировать весь журнал", "Copy the whole log"),
                    false,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.copy_log(profile_id, cx))),
            )
            .child(
                tool(
                    "log-clear",
                    IconName::Delete,
                    locale.text("Очистить", "Clear"),
                    false,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.instances.update(cx, |instances, cx| {
                        instances.clear_game_log(profile_id);
                        cx.notify();
                    });
                })),
            )
            .child(
                tool(
                    "log-folder",
                    IconName::FolderOpen,
                    locale.text("Папка логов Minecraft", "Minecraft log folder"),
                    false,
                )
                .on_click(
                    cx.listener(move |this, _, _, cx| this.open_log_directory(profile_id, cx)),
                ),
            );
        let body = if text.is_empty() {
            div()
                .p_4()
                .text_size(px(12.))
                .text_color(rgb(palette.muted))
                .child(locale.text(
                    "Журнал пуст. Запусти игру: здесь появится всё, что она пишет.",
                    "The log is empty. Launch the game and its output appears here.",
                ))
                .into_any_element()
        } else {
            selectable(format!("game-log-{profile_id}"), text).into_any_element()
        };
        div()
            .v_flex()
            .flex_1()
            .min_h_0()
            .gap_2()
            .child(toolbar)
            .child(
                div()
                    .id(format!("game-log-scroll-{profile_id}"))
                    .flex_1()
                    .min_h_0()
                    .p_3()
                    .rounded(px(10.))
                    .bg(rgb(palette.surface))
                    .border_1()
                    .border_color(rgb(palette.border))
                    .overflow_y_scroll()
                    .track_scroll(&self.log_scroll)
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(px(12.))
                    .text_color(rgb(palette.foreground))
                    .child(body),
            )
            .into_any_element()
    }
}

/// Small square cover of one installed mod: the provider icon when the
/// catalog knows the project, else the icon extracted from the jar itself,
/// else a puzzle fallback tile.
fn mod_row_icon(
    icon_url: Option<&str>,
    icon_path: Option<&std::path::Path>,
    palette: Palette,
) -> AnyElement {
    let size = px(34.);
    let background = palette.control;
    let foreground = palette.muted;
    let fallback = move || mod_icon_placeholder(IconName::Package, background, foreground, size);
    let icon_element = |source: ImageSource| {
        div()
            .w(size)
            .h(size)
            .rounded(px(8.))
            .overflow_hidden()
            .bg(rgb(background))
            .child(
                img(source)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .rounded(px(8.))
                    .with_loading(fallback)
                    .with_fallback(fallback),
            )
            .into_any_element()
    };
    match (icon_url, icon_path) {
        (Some(url), _) => icon_element(url.into()),
        (None, Some(path)) => icon_element(path.to_path_buf().into()),
        (None, None) => {
            mod_icon_placeholder(IconName::Package, background, foreground, size).into_any_element()
        }
    }
}

/// A record without a stored project name falls back to its file name as the
/// row title; the `.jar` extension says nothing there.
fn mod_title_fallback(filename: &str) -> String {
    let at = filename.len().saturating_sub(4);
    let stem = match filename.get(at..) {
        // `get` also rejects a byte boundary inside a multi-byte character,
        // which can never be a `.jar` suffix anyway.
        Some(suffix) if suffix.eq_ignore_ascii_case(".jar") && at > 0 => &filename[..at],
        _ => filename,
    };
    stem.to_owned()
}
