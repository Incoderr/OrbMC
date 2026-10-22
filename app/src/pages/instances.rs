use crate::app::LauncherApp;
use crate::app::theme::Palette;
use crate::app::ui::*;
use crate::features::instances::*;
use crate::features::mods::*;
use crate::features::settings::*;
use crate::features::versions::*;
use crate::router::Page;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, Disableable, StyledExt};
use gpui_kit::component::button::Button as ComponentButton;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl LauncherApp {
    pub(crate) fn instances_page(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let profiles = self
            .instances
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
                        .px_3()
                        .py_2()
                        .rounded(px(8.))
                        .bg(rgb(palette.accent))
                        .text_color(rgb(palette.accent_foreground))
                        .hover(|style| style.opacity(0.9))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_version_picker(window, cx);
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
            .recent_profiles()
            .into_iter()
            .map(|profile| self.instance_card(profile, palette, locale, cx))
            .collect::<Vec<_>>();

        div()
            .v_flex()
            .w_full()
            .gap_3()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(palette.foreground))
                    .child(IconName::Box)
                    .child(locale.text("Мои профили", "My profiles")),
            )
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
        let runtime = self.instances.runtime(&profile.id);
        let is_busy = runtime.is_busy();
        let is_installed = profile.installed_version_id.is_some();
        let install_paused = matches!(&runtime, InstanceRuntime::Installing { paused: true, .. });
        let install_active = matches!(&runtime, InstanceRuntime::Installing { .. });
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
            InstanceRuntime::DownloadingMod { name } => (
                locale.text("Скачивание мода", "Downloading mod").to_owned(),
                name.clone(),
                locale.text("Скачивание…", "Downloading…"),
            ),
            InstanceRuntime::DownloadingContent { name } => (
                locale
                    .text("Установка контента", "Installing content")
                    .to_owned(),
                name.clone(),
                locale.text("Скачивание…", "Downloading…"),
            ),
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
                                    .child(profile.name.clone()),
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
                card.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(palette.muted))
                        .child(status_detail),
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
                                        this.instances.resume_install(profile_id)
                                    } else {
                                        this.instances.pause_install(profile_id)
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
                                    if this.instances.cancel_install(profile_id) {
                                        cx.notify();
                                    }
                                }))
                                .child(IconName::CircleX),
                        )
                    })
                    .child(
                        UiButton::new(format!("open-instance-folder-{profile_id}"))
                            .px_3()
                            .py_2()
                            .rounded(px(8.))
                            .bg(rgb(palette.control))
                            .text_color(rgb(palette.muted))
                            .hover(|style| style.bg(rgb(palette.hover)))
                            .accessibility_label(folder_label)
                            .tooltip(move |window, cx| Tooltip::new(folder_label).build(window, cx))
                            .on_click(cx.listener(move |this, _, _, _| {
                                this.open_profile_directory(profile_id);
                            }))
                            .child(IconName::FolderOpen),
                    )
                    .child(
                        UiButton::new(format!("add-mods-instance-{profile_id}"))
                            .px_3()
                            .py_2()
                            .rounded(px(8.))
                            .bg(rgb(palette.control))
                            .text_color(rgb(palette.foreground))
                            .hover(|style| style.bg(rgb(palette.hover)))
                            .disabled(is_busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.mods.select_profile(profile_id, mod_filter.clone());
                                this.navigate_to(Page::Mods, window, cx);
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
                            .px_3()
                            .py_2()
                            .rounded(px(8.))
                            .bg(rgb(palette.control))
                            .text_color(rgb(palette.foreground))
                            .hover(|style| style.bg(rgb(palette.hover)))
                            .accessibility_label(
                                locale.text("Открыть страницу сборки", "Open build page"),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.navigate_to(Page::InstanceDetails(profile_id), window, cx);
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
                            && rs_mc_launcher_core::minecraft::quick_play::potentially_supported(
                                &profile.game_version,
                            ),
                        |row| {
                            row.child(
                                ComponentButton::new(format!("world-instance-{profile_id}"))
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
impl LauncherApp {
    /// Build details page: metadata of the build and its installed mods with
    /// enable/disable switches.
    pub(crate) fn instance_details_page(
        &self,
        profile_id: uuid::Uuid,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(profile) = self.instances.profile(profile_id) else {
            return self.empty_page(palette, locale);
        };
        let profile = profile.clone();
        let runtime = self.instances.runtime(&profile_id);
        let is_busy = runtime.is_busy();
        let scan_loading = self.mods.installed.is_loading(profile_id);
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
        let entries: Vec<installed::InstalledModEntry> = self.mods.build_mods(profile_id).to_vec();

        let back_button = UiButton::new("instance-details-back")
            .px_2()
            .py_2()
            .rounded(px(7.))
            .bg(rgb(palette.background))
            .text_color(rgb(palette.muted))
            .hover(|style| style.bg(rgb(palette.hover)))
            .accessibility_label(locale.text("Назад к сборкам", "Back to builds"))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.navigate_to(Page::Instances, window, cx);
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
                // Controlled switch: `checked` is the enabled state; the
                // handler receives the requested new value.
                let switch_owner = cx.entity().downgrade();
                let switch_filename = row_filename.clone();
                let toggle = Switch::new(format!(
                    "mod-switch-{}-{row_filename}",
                    profile_id.as_simple()
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
                    let _ = owner.update(cx, |this, cx| {
                        this.toggle_mod_disabled(
                            profile_id,
                            switch_filename.clone(),
                            !*checked,
                            cx,
                        );
                    });
                });

                // A jar file can serve several catalog projects; each gets its
                // delete action.
                let uninstalls = entry.projects.iter().map(|project| {
                    let project = project.clone();
                    let remove_label = locale
                        .text("Удалить из сборки", "Remove from build")
                        .to_owned();
                    UiButton::new(format!("unbuild-mod-{}-{project}", profile_id.as_simple()))
                        .px_2()
                        .py_2()
                        .rounded(px(6.))
                        .bg(rgb(palette.control))
                        .text_color(rgb(0xef6a65))
                        .hover(|style| style.bg(rgb(0x3b1b1b)))
                        .disabled(is_busy || scan_loading)
                        .accessibility_label(remove_label.clone())
                        .tooltip(move |window, cx| {
                            Tooltip::new(remove_label.clone()).build(window, cx)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.uninstall_mod(project.clone(), profile_id, cx);
                        }))
                        .child(IconName::Delete)
                        .into_any_element()
                });

                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded(px(8.))
                    .bg(rgb(palette.background))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .text_color(rgb(if disabled {
                                palette.muted
                            } else {
                                palette.foreground
                            }))
                            .truncate()
                            .child(entry.filename.clone()),
                    )
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
                                    .child(profile.name.clone()),
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
                                    .on_click(cx.listener(move |this, _, _, _| {
                                        this.open_profile_directory(profile_id);
                                    }))
                                    .child(IconName::FolderOpen),
                            )
                            .child(
                                UiButton::new(format!("details-play-instance-{profile_id}"))
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
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(palette.foreground))
                            .child(locale.text("Установленные моды", "Installed mods")),
                    )
                    .when(rows.is_empty(), |section| {
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
                    })
                    .when(!rows.is_empty(), |section| {
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
                    }),
            )
            .into_any_element()
    }
}
