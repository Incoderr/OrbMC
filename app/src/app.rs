use std::sync::Arc;

use crate::platform::profile_store;
use crate::{
    features::instances::{
        InstanceProfile, InstanceRuntime, InstanceWorkerEvent, InstancesState, install_profile,
        is_valid_player_name, launch_profile, remove_profile_data,
    },
    features::mods::{
        FilterSection, ModCategory, ModCompatibilityFilter, ModDetailTab, ModLoaderFilter,
        ModSample, ModSort, ModsState, ProjectKind, SAMPLE_MODS,
        catalog::{
            ModProject, ModSource, fetch_project_description, install_compatible_mod,
            search_catalog,
        },
        installed, translation,
    },
    features::settings::{AccentColor, Locale, SettingsState, ThemeChoice},
    features::versions::{
        LoaderChoice, LoaderVersionMode, VersionCatalogState, fetch_fabric_game_loaders,
        fetch_manifests,
    },
    router::Page,
};
use futures::StreamExt;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, Disableable, StyledExt};
use gpui_kit::component::button::{Button as ComponentButton, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::scroll::{ScrollableElement, Scrollbar};
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Theme, ThemeMode, WindowExt, tooltip::Tooltip};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use mc_launcher_core::prelude::DownloadControl;
use std::rc::Rc;
use std::time::Duration;

#[path = "skin_view.rs"]
mod skin_view;

#[derive(Clone, Copy)]
struct Palette {
    background: u32,
    sidebar: u32,
    surface: u32,
    border: u32,
    selected: u32,
    control: u32,
    hover: u32,
    selected_hover: u32,
    pressed: u32,
    foreground: u32,
    muted: u32,
    accent: u32,
    accent_foreground: u32,
}

impl Palette {
    fn for_settings(theme: ThemeChoice, accent: AccentColor) -> Self {
        let mut palette = match theme {
            ThemeChoice::Dark => Self {
                background: 0x111111,
                sidebar: 0x0b0b0b,
                surface: 0x181818,
                border: 0x2b2b2b,
                selected: 0x252525,
                control: 0x141414,
                hover: 0x262626,
                selected_hover: 0x343434,
                pressed: 0x3b3b3b,
                foreground: 0xf2f2f2,
                muted: 0xa0a0a0,
                accent: 0,
                accent_foreground: 0,
            },
            ThemeChoice::Light => Self {
                background: 0xf3f3f1,
                sidebar: 0xffffff,
                surface: 0xffffff,
                border: 0xe2e2df,
                selected: 0xe9e9e7,
                control: 0xf8f8f6,
                hover: 0xefefec,
                selected_hover: 0xdfdfdc,
                pressed: 0xe4e4e1,
                foreground: 0x202020,
                muted: 0x747474,
                accent: 0,
                accent_foreground: 0,
            },
        };
        palette.accent = accent.color(theme);
        palette.accent_foreground = accent.foreground(theme);
        palette
    }
}

#[derive(Debug, Default)]
struct SidebarState {
    collapsed: bool,
}

impl SidebarState {
    fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
    }
}

pub struct LauncherApp {
    active_page: Page,
    settings: SettingsState,
    skins: crate::features::skins::SkinState,
    skin_yaw: f32,
    skin_dragging: bool,
    skin_last_x: f32,
    skin_preview_request: Option<(uuid::Uuid, i16, crate::features::skins::SkinModel)>,
    skin_thumbnails_pending:
        std::collections::HashSet<(uuid::Uuid, crate::features::skins::SkinModel)>,
    skin_message: Option<String>,
    mods: ModsState,
    versions: VersionCatalogState,
    instances: InstancesState,
    sidebar: SidebarState,
    search_input: Entity<InputState>,
    mod_version_input: Entity<InputState>,
    profile_name_input: Entity<InputState>,
    loader_version_input: Entity<InputState>,
    nickname_input: Entity<InputState>,
    manifest_scroll: gpui_kit::base::VirtualListScrollHandle,
    mods_scroll: gpui_kit::base::VirtualListScrollHandle,
    _search_subscription: Subscription,
    _mod_version_subscription: Subscription,
    tray_available: bool,
}

impl LauncherApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut saved_settings = match profile_store::load_settings() {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(%error, "failed to load launcher settings");
                profile_store::SavedLauncherSettings::default()
            }
        };
        let default_download_directory = match profile_store::default_download_directory() {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(%error, "launcher default download directory is unavailable");
                std::path::PathBuf::new()
            }
        };
        if saved_settings.download_directory.as_os_str().is_empty() {
            saved_settings.download_directory = default_download_directory;
        }

        let mut settings = SettingsState::default();
        settings.set_download_directory(saved_settings.download_directory.clone());
        settings.set_locale(match saved_settings.locale.as_str() {
            "en" => Locale::En,
            _ => Locale::Ru,
        });
        settings.set_theme(match saved_settings.theme.as_str() {
            "light" => ThemeChoice::Light,
            _ => ThemeChoice::Dark,
        });
        settings.set_accent(match saved_settings.accent.as_str() {
            "red" => AccentColor::Red,
            "blue" => AccentColor::Blue,
            "green" => AccentColor::Green,
            "purple" => AccentColor::Purple,
            "pink" => AccentColor::Pink,
            _ => AccentColor::Orange,
        });
        settings.set_hide_to_tray(saved_settings.hide_to_tray);
        if is_valid_player_name(&saved_settings.offline_player) {
            settings.set_offline_player(saved_settings.offline_player.clone());
        } else {
            settings.reset_offline_player();
        }

        let locale = settings.locale();
        gpui_kit::component::set_locale(locale.tag());
        Theme::change(
            match settings.theme() {
                ThemeChoice::Dark => ThemeMode::Dark,
                ThemeChoice::Light => ThemeMode::Light,
            },
            None,
            cx,
        );
        let search_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(locale.text(
                "Поиск по названию, описанию или автору",
                "Search by name, description, or author",
            ))
        });
        let mod_version_input = cx.new(|cx| InputState::new(window, cx).placeholder("1.21.1"));
        let mod_version_subscription =
            cx.subscribe(&mod_version_input, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) && this.mods.target_profile().is_none() {
                    this.mods
                        .set_game_version_filter(input.read(cx).value().to_string());
                    this.load_mod_catalog(cx, true);
                }
            });
        let profile_name_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(locale.text("Название профиля", "Profile name"))
        });
        let loader_version_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(locale.text("Версия загрузчика", "Loader version"))
        });
        let nickname_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(locale.text("Ник игрока", "Player name"))
        });
        let search_subscription =
            cx.subscribe(&search_input, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = input.read(cx).value().to_string();
                    this.mods.set_query(query);
                    this.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
                    if this.active_page.catalog_kind().is_some() {
                        this.load_mod_catalog(cx, true);
                    }
                    cx.notify();
                }
            });
        let download_directory = saved_settings.download_directory.clone();

        let instances = match profile_store::load() {
            Ok(mut profiles) => {
                for profile in &mut profiles {
                    if profile.installation_root.as_os_str().is_empty() {
                        profile.installation_root = download_directory.clone();
                    }
                    profile.memory_mb = profile.memory_mb.clamp(512, 16_384);
                }
                InstancesState::from_profiles(profiles)
            }
            Err(error) => {
                tracing::warn!(%error, "failed to load saved Minecraft profiles");
                InstancesState::default()
            }
        };

        let tray_available = crate::shell::tray::install(window, cx.weak_entity(), cx);

        Self {
            tray_available,
            active_page: Page::Home,
            settings,
            skins: crate::features::skins::SkinState::load(),
            skin_yaw: -0.24,
            skin_dragging: false,
            skin_last_x: 0.0,
            skin_preview_request: None,
            skin_thumbnails_pending: std::collections::HashSet::new(),
            skin_message: None,
            mods: ModsState::default(),
            versions: VersionCatalogState::default(),
            instances,
            sidebar: SidebarState::default(),
            search_input,
            mod_version_input,
            profile_name_input,
            loader_version_input,
            nickname_input,
            manifest_scroll: gpui_kit::base::VirtualListScrollHandle::new(),
            mods_scroll: gpui_kit::base::VirtualListScrollHandle::new(),
            _search_subscription: search_subscription,
            _mod_version_subscription: mod_version_subscription,
        }
    }

    pub(crate) fn hide_to_tray(&self) -> bool {
        self.settings.hide_to_tray()
    }

    pub(crate) fn tray_uses_english(&self) -> bool {
        self.settings.locale() == Locale::En
    }

    fn set_locale(&mut self, locale: Locale, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.set_locale(locale);
        gpui_kit::component::set_locale(locale.tag());
        self.search_input.update(cx, |input, cx| {
            input.set_placeholder(
                locale.text(
                    "Поиск по названию, описанию или автору",
                    "Search by name, description, or author",
                ),
                window,
                cx,
            );
        });
        self.profile_name_input.update(cx, |input, cx| {
            input.set_placeholder(locale.text("Название профиля", "Profile name"), window, cx);
        });
        self.loader_version_input.update(cx, |input, cx| {
            input.set_placeholder(
                locale.text("Версия загрузчика", "Loader version"),
                window,
                cx,
            );
        });
        self.nickname_input.update(cx, |input, cx| {
            input.set_placeholder(locale.text("Ник игрока", "Player name"), window, cx);
        });
        self.persist_settings();
        if self.active_page == Page::ModDetails || self.active_page.catalog_kind().is_some() {
            self.translate_catalog_descriptions(cx);
        }
        cx.notify();
    }

    fn persist_settings(&self) {
        let accent = match self.settings.accent() {
            AccentColor::Orange => "orange",
            AccentColor::Red => "red",
            AccentColor::Blue => "blue",
            AccentColor::Green => "green",
            AccentColor::Purple => "purple",
            AccentColor::Pink => "pink",
        };
        let saved = profile_store::SavedLauncherSettings {
            download_directory: self.settings.download_directory().clone(),
            locale: self.settings.locale().tag().to_owned(),
            theme: match self.settings.theme() {
                ThemeChoice::Dark => "dark",
                ThemeChoice::Light => "light",
            }
            .to_owned(),
            accent: accent.to_owned(),
            hide_to_tray: self.settings.hide_to_tray(),
            offline_player: self.settings.offline_player().to_owned(),
        };
        if let Err(error) = profile_store::save_settings(&saved) {
            tracing::error!(%error, "failed to persist launcher settings");
        }
    }

    fn choose_download_directory(&mut self, cx: &mut Context<Self>) {
        let locale = self.settings.locale();
        let response = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(
                locale
                    .text("Выбери папку для загрузок", "Choose a downloads folder")
                    .into(),
            ),
        });
        cx.spawn(async move |this, cx| {
            let selected_path = match response.await {
                Ok(Ok(Some(paths))) => paths.into_iter().find(|path| path.is_dir()),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    tracing::warn!(%error, "failed to open folder picker");
                    None
                }
                Err(error) => {
                    tracing::warn!(%error, "folder picker response channel closed");
                    None
                }
            };
            let Some(path) = selected_path else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.settings.set_download_directory(path.clone());
                this.persist_settings();
                cx.notify();
            });
        })
        .detach();
    }

    fn choose_java_for_profile(&mut self, id: uuid::Uuid, cx: &mut Context<Self>) {
        let locale = self.settings.locale();
        let response = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                locale
                    .text("Выбери java.exe", "Choose java executable")
                    .into(),
            ),
        });
        let owner = cx.entity().downgrade();
        cx.spawn(async move |this, cx| {
            match response.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(java_path) = paths.into_iter().find(|path| path.is_file()) {
                        let _ = this.update(cx, |this, cx| {
                            this.set_profile_java_path(id, Some(java_path), cx);
                        });
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    tracing::warn!(%error, "failed to open Java file picker");
                }
                Err(error) => {
                    tracing::warn!(%error, "Java file picker response channel closed");
                }
            }
            let _ = owner.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    fn set_profile_java_path(
        &mut self,
        id: uuid::Uuid,
        java_path: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.instances.set_java_path(id, java_path);
        self.persist_profiles();
        cx.notify();
    }

    fn persist_profiles(&self) {
        if let Err(error) = profile_store::save(self.instances.profiles()) {
            tracing::error!(%error, "failed to persist Minecraft profiles");
        }
    }

    fn navigate_to(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_page == page {
            return;
        }

        let is_catalog_detail_transition =
            page == Page::ModDetails || self.active_page == Page::ModDetails;
        self.active_page = page;
        if !is_catalog_detail_transition {
            self.mods.set_query(String::new());
            self.search_input
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        if let Some(kind) = page.catalog_kind() {
            self.mods.set_kind(kind);
        }
        self.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
        if page.catalog_kind().is_some() {
            self.load_version_manifests(cx);
            self.refresh_installed_mods(cx);
            self.load_mod_catalog(cx, false);
        }
        if let Page::InstanceDetails(id) = page {
            self.load_installed_entries(id, cx);
        }
        cx.notify();
    }

    fn refresh_installed_mods(&mut self, cx: &mut Context<Self>) {
        for profile in self.instances.profiles().to_vec() {
            let id = profile.id;
            if self.mods.installed.is_loading(id)
                || matches!(
                    self.instances.runtime(&id),
                    InstanceRuntime::DownloadingMod { .. }
                        | InstanceRuntime::Removing
                        | InstanceRuntime::Installing { .. }
                )
            {
                continue;
            }
            let generation = self.mods.installed.begin(id);
            let directory =
                profile_store::instance_directory(&profile.installation_root, id).join("mods");
            let key = std::env::var("CURSEFORGE_API_KEY").ok();
            let task = cx.background_executor().spawn(async move {
                crate::features::mods::installed::scan(&directory, key.as_deref())
            });
            cx.spawn(async move |this, cx| {
                let inventory = task.await;
                let _ = this.update(cx, |this, cx| {
                    if this.instances.profile(id).is_some() {
                        if let Some(warning) = &inventory.warning {
                            tracing::warn!(profile_id = %id, %warning, "mod inventory incomplete");
                        }
                        this.mods.installed.finish(id, generation, inventory);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    fn load_mod_catalog(&mut self, cx: &mut Context<Self>, debounce: bool) {
        let (request_id, query, sort, source, compatibility_filter, kind, page) =
            self.mods.begin_search();
        let client = cx.http_client();
        let curseforge_api_key = std::env::var("CURSEFORGE_API_KEY").ok();
        cx.notify();

        cx.spawn(async move |this, cx| {
            if debounce {
                cx.background_executor()
                    .timer(Duration::from_millis(350))
                    .await;
                let still_current = this
                    .update(cx, |this, _| this.mods.is_current_request(request_id))
                    .unwrap_or(false);
                if !still_current {
                    return;
                }
            }

            let result = search_catalog(
                client,
                query,
                sort,
                source,
                curseforge_api_key,
                compatibility_filter,
                kind,
                page,
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                if this.mods.finish_search(request_id, result) {
                    this.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
                    this.translate_catalog_descriptions(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Translates the descriptions of the current catalog page in the
    /// background. Per-item state is kept in [`ModsState`]; failures fall
    /// back to the original text without bothering the user.
    fn translate_catalog_descriptions(&mut self, cx: &mut Context<Self>) {
        let locale = self.settings.locale();
        let page: Vec<ModProject> = self.mods.visible_mods().into_iter().cloned().collect();
        let mut dirty = false;
        for project in &page {
            if self.mods.begin_translation(project, locale) {
                dirty = true;
            }
        }
        if !dirty {
            return;
        }
        cx.notify();

        let client = cx.http_client();
        // Four concurrent requests is polite towards the free endpoint.
        let task = cx.background_executor().spawn(async move {
            futures::stream::iter(page)
                .map(|project| {
                    let client = Arc::clone(&client);
                    async move {
                        let result =
                            translation::translate_card_description(client, &project, locale).await;
                        (project, result)
                    }
                })
                .buffer_unordered(4)
                .collect::<Vec<_>>()
                .await
        });
        cx.spawn(async move |this, cx| {
            let results = task.await;
            let _ = this.update(cx, |this, cx| {
                for (project, result) in results {
                    if this.mods.finish_translation(&project, locale, result) {
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn load_mod_description(
        &self,
        project_id: String,
        source: ModSource,
        request_id: u64,
        cx: &mut Context<Self>,
    ) {
        let client = cx.http_client();
        let curseforge_api_key = std::env::var("CURSEFORGE_API_KEY").ok();
        cx.spawn(async move |this, cx| {
            let result =
                fetch_project_description(client, project_id, source, curseforge_api_key).await;
            let _ = this.update(cx, |this, cx| {
                if this.mods.finish_description_load(request_id, result) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn open_mod_download_dialog(
        &mut self,
        project: ModProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.mods.target_profile() {
            self.start_mod_download(project, id, cx);
            return;
        }
        let locale = self.settings.locale();
        let palette = Palette::for_settings(self.settings.theme(), self.settings.accent());
        let profiles = self
            .instances
            .profiles()
            .iter()
            .map(|profile| {
                let runtime = self.instances.runtime(&profile.id);
                let busy = self.mods.installed.contains(profile.id, &project.id)
                    || self.mods.installed.is_loading(profile.id)
                    || runtime.is_busy();
                (
                    profile.clone(),
                    busy,
                    self.mods.installed.contains(profile.id, &project.id),
                )
            })
            .collect::<Vec<_>>();
        let owner = cx.entity().downgrade();
        let title = format!(
            "{} · {}",
            locale.text("Скачать мод", "Download mod"),
            project.name
        );

        window.open_dialog(cx, move |dialog, _, _| {
            let content_owner = owner.clone();
            let project = project.clone();
            let content_profiles = profiles.clone();
            dialog
                .w(px(500.))
                .title(title.clone())
                .content(move |content, _, _| {
                    if content_profiles.is_empty() {
                        let create_owner = content_owner.clone();
                        return content.child(
                            div()
                                .v_flex()
                                .w_full()
                                .gap_3()
                                .p_4()
                                .text_color(rgb(palette.foreground))
                                .child(locale.text(
                                    "Сначала создай сборку, куда установить мод.",
                                    "Create a build first, then choose it as the mod destination.",
                                ))
                                .child(
                                    UiButton::new("mod-download-create-build")
                                        .px_3()
                                        .py_2()
                                        .rounded(px(8.))
                                        .bg(rgb(palette.accent))
                                        .text_color(rgb(palette.accent_foreground))
                                        .on_click(move |_, window, cx| {
                                            window.close_dialog(cx);
                                            if let Some(owner) = create_owner.upgrade() {
                                                let _ = owner.update(cx, |this, cx| {
                                                    this.navigate_to(Page::Instances, window, cx);
                                                });
                                            }
                                        })
                                        .child(locale.text("Открыть сборки", "Open builds")),
                                ),
                        );
                    }

                    let rows = content_profiles
                        .iter()
                        .map(|(profile, busy, installed)| {
                            let profile_id = profile.id;
                            let has_mod_loader = profile.loader != LoaderChoice::Vanilla;
                            let profile_details = if *installed {
                                locale
                                    .text("Установлен в этой сборке", "Installed in this build")
                                    .to_owned()
                            } else if !has_mod_loader {
                                locale
                                    .text(
                                        "Для модов нужна сборка с Fabric, Forge или NeoForge",
                                        "Mods need a Fabric, Forge, or NeoForge build",
                                    )
                                    .to_owned()
                            } else if *busy {
                                locale
                                    .text("Сборка занята другой операцией", "This build is busy")
                                    .to_owned()
                            } else {
                                format!(
                                    "Minecraft {} · {}",
                                    profile.game_version,
                                    loader_title(profile.loader)
                                )
                            };
                            let select_owner = content_owner.clone();
                            let selected_project = project.clone();
                            UiButton::new(format!("download-mod-to-{profile_id}"))
                                .w_full()
                                .px_3()
                                .py_3()
                                .rounded(px(8.))
                                .bg(rgb(palette.control))
                                .text_color(rgb(palette.foreground))
                                .hover(|style| style.bg(rgb(palette.hover)))
                                .disabled(*busy || !has_mod_loader)
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    if let Some(owner) = select_owner.upgrade() {
                                        let _ = owner.update(cx, |this, cx| {
                                            this.start_mod_download(
                                                selected_project.clone(),
                                                profile_id,
                                                cx,
                                            );
                                        });
                                    }
                                })
                                .child(
                                    div()
                                        .v_flex()
                                        .w_full()
                                        .items_start()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(13.))
                                                .font_weight(FontWeight::BOLD)
                                                .child(profile.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(rgb(palette.muted))
                                                .child(profile_details),
                                        ),
                                )
                                .into_any_element()
                        })
                        .collect::<Vec<_>>();

                    content.child(
                        div()
                            .v_flex()
                            .w_full()
                            .max_h(px(460.))
                            .gap_2()
                            .overflow_y_scrollbar()
                            .p_4()
                            .children(rows),
                    )
                })
        });
    }

    fn start_mod_download(
        &mut self,
        project: ModProject,
        profile_id: uuid::Uuid,
        cx: &mut Context<Self>,
    ) {
        if self.mods.installed.contains(profile_id, &project.id)
            || self.mods.installed.is_loading(profile_id)
        {
            return;
        }
        let Some(profile) = self.instances.profile(profile_id).cloned() else {
            return;
        };
        let runtime = self.instances.runtime(&profile_id);
        if runtime.is_busy() {
            return;
        }

        let compatibility = ModCompatibilityFilter {
            game_version: profile.game_version.clone(),
            loader: mod_loader_filter(profile.loader),
        };
        let mods_directory =
            profile_store::instance_directory(&profile.installation_root, profile_id).join("mods");
        self.instances
            .mark_mod_downloading(profile_id, project.name.clone());
        self.mods.installed.begin_install(&project.id);
        let client = cx.http_client();
        let curseforge_api_key = std::env::var("CURSEFORGE_API_KEY").ok();
        let project_id = project.id;
        let pending_project_id = project_id.clone();
        let source = project.source;
        cx.notify();

        let download_task = cx.background_executor().spawn(async move {
            install_compatible_mod(
                client,
                project_id,
                source,
                compatibility,
                curseforge_api_key,
                mods_directory,
            )
            .await
        });
        cx.spawn(async move |this, cx| {
            let result = download_task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(outcome) => {
                        // Mark the main mod and every dependency as installed
                        // so catalog cards reflect the new build state.
                        for installed_project in &outcome.installed {
                            this.mods
                                .installed
                                .mark_installed(profile_id, installed_project.clone());
                        }
                        for filename in &outcome.files {
                            tracing::info!(
                                profile_id = %profile_id,
                                %filename,
                                "installed mod into profile"
                            );
                        }
                        if let Some(filename) = outcome.files.first() {
                            this.instances
                                .mark_mod_downloaded(profile_id, filename.clone());
                        }
                        this.load_installed_entries(profile_id, cx);
                    }
                    Err(error) => {
                        this.instances
                            .mark_mod_download_failed(profile_id, error.message);
                    }
                }
                this.mods.installed.finish_install(&pending_project_id);
                cx.notify();
            });
        })
        .detach();
    }

    /// Refreshes the installed-mods list of one build (used by the build
    /// details page). Reads the mods directory on the background executor.
    fn load_installed_entries(&mut self, profile_id: uuid::Uuid, cx: &mut Context<Self>) {
        let Some(profile) = self.instances.profile(profile_id).cloned() else {
            return;
        };
        let directory =
            profile_store::instance_directory(&profile.installation_root, profile_id).join("mods");
        let task = cx
            .background_executor()
            .spawn(async move { crate::features::mods::installed::list_entries(&directory) });
        cx.spawn(async move |this, cx| {
            let entries = task.await;
            let _ = this.update(cx, |this, cx| {
                this.mods.set_build_mods(profile_id, entries);
                cx.notify();
            });
        })
        .detach();
    }

    /// Enables or disables one installed mod of a build without deleting it.
    fn toggle_mod_disabled(
        &mut self,
        profile_id: uuid::Uuid,
        filename: String,
        disable: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.instances.profile(profile_id).cloned() else {
            return;
        };
        if self.instances.runtime(&profile_id).is_busy()
            || self.mods.installed.is_loading(profile_id)
        {
            return;
        }
        let directory =
            profile_store::instance_directory(&profile.installation_root, profile_id).join("mods");
        match installed::set_disabled(&directory, &filename, disable) {
            Ok(()) => {
                tracing::info!(
                    profile_id = %profile_id,
                    %filename,
                    disabled = disable,
                    "toggled mod state"
                );
                // Refresh the list immediately, then re-scan the inventory so
                // installed markers stay accurate.
                self.load_installed_entries(profile_id, cx);
                self.refresh_installed_mods(cx);
                cx.notify();
            }
            Err(message) => {
                tracing::warn!(
                    profile_id = %profile_id,
                    %filename,
                    %message,
                    "could not toggle mod state"
                );
            }
        }
    }

    /// Removes a mod from a profile's mods directory, then re-scans the
    /// inventory from disk so installed markers stay accurate.
    fn uninstall_mod(
        &mut self,
        project_id: String,
        profile_id: uuid::Uuid,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.instances.profile(profile_id).cloned() else {
            return;
        };
        if self.instances.runtime(&profile_id).is_busy()
            || self.mods.installed.is_loading(profile_id)
        {
            return;
        }
        let generation = self.mods.installed.begin(profile_id);
        let directory =
            profile_store::instance_directory(&profile.installation_root, profile_id).join("mods");
        let task = cx.background_executor().spawn(async move {
            let mut removal_error = None;
            match installed::uninstall(&directory, &project_id) {
                Ok(files) if files.is_empty() => {}
                Ok(files) => tracing::info!(
                    profile_id = %profile_id,
                    removed = ?files,
                    "uninstalled mod files from profile"
                ),
                Err(error) => removal_error = Some(error),
            }
            let api_key = std::env::var("CURSEFORGE_API_KEY").ok();
            let mut inventory = installed::scan(&directory, api_key.as_deref());
            if let Some(error) = removal_error {
                inventory.warning = Some(match inventory.warning.take() {
                    Some(existing) => format!("{existing}; {error}"),
                    None => error,
                });
            }
            inventory
        });
        cx.spawn(async move |this, cx| {
            let inventory = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.instances.profile(profile_id).is_some() {
                    this.mods
                        .installed
                        .finish(profile_id, generation, inventory);
                    this.load_installed_entries(profile_id, cx);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// The profile a mod should be removed from: the selected target if one is
    /// active, otherwise the single profile the mod is installed in.
    fn uninstall_target(&self, project_id: &str) -> Option<uuid::Uuid> {
        if let Some(id) = self.mods.target_profile() {
            return self.mods.installed.contains(id, project_id).then_some(id);
        }
        let mut candidates = self
            .instances
            .profiles()
            .iter()
            .filter(|profile| self.mods.installed.contains(profile.id, project_id))
            .map(|profile| profile.id);
        let first = candidates.next()?;
        candidates.all(|id| id == first).then_some(first)
    }

    fn load_version_manifests(&mut self, cx: &mut Context<Self>) {
        if !self.versions.begin_loading() {
            return;
        }

        let client = cx.http_client();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = fetch_manifests(client).await;
            let _ = this.update(cx, |this, cx| {
                this.versions.finish_loading(result);
                this.load_fabric_game_loaders(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn load_fabric_game_loaders(&mut self, cx: &mut Context<Self>) {
        if self.versions.loader_choice() != LoaderChoice::Fabric {
            return;
        }
        let Some(game_version) = self.versions.game_version().map(str::to_owned) else {
            return;
        };
        if !self.versions.begin_fabric_game_load(&game_version) {
            return;
        }

        let client = cx.http_client();
        cx.spawn(async move |this, cx| {
            let result = fetch_fabric_game_loaders(client, game_version.clone()).await;
            let _ = this.update(cx, move |this, cx| {
                this.versions.finish_fabric_game_load(game_version, result);
                cx.notify();
            });
        })
        .detach();
    }

    fn start_profile_install(&mut self, id: uuid::Uuid, cx: &mut Context<Self>) {
        let Some(profile) = self.instances.profile(id).cloned() else {
            return;
        };
        let minecraft_directory = profile_store::shared_game_directory(&profile.installation_root);

        self.instances.mark_installing(id);
        let control = DownloadControl::new();
        self.instances.set_install_control(id, control.clone());
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        let worker = std::thread::Builder::new()
            .name(format!("install-minecraft-{id}"))
            .spawn(move || install_profile(profile, minecraft_directory, control, sender));

        match worker {
            Ok(_) => self.listen_for_instance_events(receiver, cx),
            Err(error) => self
                .instances
                .mark_failed(id, format!("Не удалось запустить установщик: {error}")),
        }
        cx.notify();
    }

    fn open_world_picker(&mut self, id: uuid::Uuid, window: &mut Window, cx: &mut Context<Self>) {
        use crate::features::worlds::WorldList;
        let Some(profile) = self.instances.profile(id).cloned() else {
            return;
        };
        let locale = self.settings.locale();
        let title = format!(
            "{} · {}",
            locale.text("Войти в мир", "Play a world"),
            profile.name
        );
        self.instances.set_worlds(id, WorldList::Loading);
        let task = cx
            .background_executor()
            .spawn(async move { crate::features::worlds::load(profile) });
        cx.spawn(async move |this, cx| {
            let worlds = task.await;
            let _ = this.update(cx, |this, cx| {
                this.instances.set_worlds(id, worlds);
                cx.notify();
            });
        })
        .detach();
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            dialog.title(title.clone()).content(move |content, _, cx| {
                let worlds = owner
                    .upgrade()
                    .map(|owner| owner.read(cx).instances.worlds(id));
                let body = match worlds {
                    Some(WorldList::Ready(worlds)) if !worlds.is_empty() => div()
                        .v_flex()
                        .gap_2()
                        .children(worlds.into_iter().map(|world| {
                            let owner = owner.clone();
                            let label = world.clone();
                            ComponentButton::new(format!("quick-play-{world}"))
                                .outline()
                                .label(label)
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    let _ = owner.update(cx, |this, cx| {
                                        this.launch_into(id, Some(world.clone()), cx)
                                    });
                                })
                        }))
                        .into_any_element(),
                    Some(WorldList::Ready(_)) => div()
                        .child(locale.text(
                            "Миров пока нет. Создай мир в игре — он появится здесь.",
                            "No worlds yet. Create one in the game first.",
                        ))
                        .into_any_element(),
                    Some(WorldList::Failed(error)) => div().child(error).into_any_element(),
                    _ => div()
                        .child(locale.text("Читаем сохранения…", "Loading worlds…"))
                        .into_any_element(),
                };
                content.child(
                    div()
                        .p_4()
                        .max_h(px(420.))
                        .overflow_y_scrollbar()
                        .child(body),
                )
            })
        });
    }

    fn start_profile_launch(&mut self, id: uuid::Uuid, cx: &mut Context<Self>) {
        self.launch_into(id, None, cx);
    }

    fn launch_into(&mut self, id: uuid::Uuid, world: Option<String>, cx: &mut Context<Self>) {
        if self.instances.runtime(&id).is_busy() {
            return;
        }
        let Some(profile) = self.instances.profile(id).cloned() else {
            return;
        };
        if profile.installed_version_id.is_none() {
            self.start_profile_install(id, cx);
            return;
        }
        let minecraft_directory = profile_store::shared_game_directory(&profile.installation_root);

        self.instances.mark_launching(id);
        let player_name = self.settings.offline_player().to_owned();
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        let worker = std::thread::Builder::new()
            .name(format!("launch-minecraft-{id}"))
            .spawn(move || {
                launch_profile(profile, minecraft_directory, world, player_name, sender)
            });

        match worker {
            Ok(_) => self.listen_for_instance_events(receiver, cx),
            Err(error) => self
                .instances
                .mark_failed(id, format!("Не удалось запустить игру: {error}")),
        }
        cx.notify();
    }

    fn open_profile_directory(&self, id: uuid::Uuid) {
        let Some(profile) = self.instances.profile(id) else {
            return;
        };
        match profile_store::ensure_instance_directory(&profile.installation_root, id)
            .and_then(|path| crate::platform::process::open_directory(&path).map(|()| path))
        {
            Ok(_) => {}
            Err(error) => {
                tracing::error!(profile_id = %id, %error, "failed to open profile directory");
            }
        }
    }

    fn listen_for_instance_events(
        &self,
        mut events: futures::channel::mpsc::UnboundedReceiver<InstanceWorkerEvent>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                let _ = this.update(cx, |this, cx| {
                    this.handle_instance_event(event);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn handle_instance_event(&mut self, event: InstanceWorkerEvent) {
        match event {
            InstanceWorkerEvent::Progress { id, update } => {
                self.instances.update_progress(id, update);
            }
            InstanceWorkerEvent::Installed { id, version_id } => {
                self.instances.finish_install(id);
                self.instances.mark_installed(id, version_id);
                if let Err(error) = profile_store::save(self.instances.profiles()) {
                    tracing::error!(%error, "failed to save installed Minecraft profile");
                    self.instances.mark_failed(
                        id,
                        format!("Установка завершилась, но профиль не сохранился: {error}"),
                    );
                }
            }
            InstanceWorkerEvent::InstallFailed { id, message } => {
                self.instances.finish_install(id);
                self.instances.mark_failed(id, message);
            }
            InstanceWorkerEvent::LaunchFailed { id, message } => {
                self.instances.mark_failed(id, message);
            }
            InstanceWorkerEvent::ProfileRemoved { id } => {
                self.instances.remove(id);
                if self.mods.target_profile() == Some(id) {
                    self.mods.clear_compatibility_filter();
                }
                self.persist_profiles();
            }
            InstanceWorkerEvent::ProfileRemovalFailed { id, message } => {
                self.instances.mark_removal_failed(id, message);
            }
            InstanceWorkerEvent::Launched { id, process_id } => {
                self.instances.mark_running(id, process_id);
                self.persist_profiles();
            }
            InstanceWorkerEvent::PreparingJava { id, detail } => {
                self.instances.mark_preparing_java(id, detail);
            }
            InstanceWorkerEvent::GameExited { id, code, stderr } => {
                self.instances.mark_game_stopped(id, code, stderr);
            }
        }
    }

    fn open_profile_settings(
        &mut self,
        id: uuid::Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.instances.profile(id) else {
            return;
        };
        let profile_name = profile.name.clone();
        let memory_slider = cx.new(|_| {
            SliderState::new()
                .max(16_384.)
                .min(512.)
                .step(256.)
                .default_value(profile.memory_mb as f32)
        });
        let locale = self.settings.locale();
        let palette = Palette::for_settings(self.settings.theme(), self.settings.accent());
        let title = format!(
            "{} · {profile_name}",
            locale.text("Настройки сборки", "Profile settings")
        );
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let content_owner = owner.clone();
            let content_slider = memory_slider.clone();
            dialog
                .w(px(520.))
                .title(title.clone())
                .content(move |content, _, cx| {
                    let current_profile = content_owner
                        .upgrade()
                        .and_then(|owner| owner.read(cx).instances.profile(id).cloned());
                    let java_path = current_profile
                        .as_ref()
                        .and_then(|profile| profile.java_path.as_ref())
                        .map(|path| path.to_string_lossy().to_string())
                        .unwrap_or_else(|| {
                            locale
                                .text(
                                    "Автоматически (подбирается нужная Java)",
                                    "Automatic (the right Java is picked for you)",
                                )
                                .to_string()
                        });
                    let memory_mb = content_slider.read(cx).value().end().round() as u32;
                    let choose_owner = content_owner.clone();
                    let automatic_owner = content_owner.clone();
                    let save_owner = content_owner.clone();
                    let save_slider = content_slider.clone();

                    content.child(
                        div()
                            .v_flex()
                            .w_full()
                            .gap_4()
                            .p_4()
                            .text_color(rgb(palette.foreground))
                            .child(
                                div()
                                    .v_flex()
                                    .gap_2()
                                    .child(field_label(
                                        locale.text("Оперативная память", "Memory allocation"),
                                    ))
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgb(palette.muted))
                                            .child(locale.text(
                                                "Лимит памяти для этой сборки.",
                                                "Heap limit for this profile.",
                                            )),
                                    )
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_3()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .child(Slider::new(&content_slider).h(px(24.))),
                                            )
                                            .child(
                                                div()
                                                    .w(px(88.))
                                                    .px_2()
                                                    .py_2()
                                                    .rounded(px(7.))
                                                    .bg(rgb(palette.control))
                                                    .text_center()
                                                    .text_color(rgb(palette.foreground))
                                                    .child(format!("{memory_mb} MB")),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .h_flex()
                                            .justify_between()
                                            .text_size(px(10.))
                                            .text_color(rgb(palette.muted))
                                            .child("512 MB")
                                            .child("16 GB"),
                                    ),
                            )
                            .child(div().h(px(1.)).w_full().bg(rgb(palette.border)))
                            .child(
                                div()
                                    .v_flex()
                                    .gap_2()
                                    .child(field_label(locale.text("Java", "Java")))
                                    .child(
                                        div()
                                            .w_full()
                                            .px_3()
                                            .py_2()
                                            .rounded(px(7.))
                                            .bg(rgb(palette.control))
                                            .text_size(px(11.))
                                            .text_color(rgb(palette.muted))
                                            .child(java_path),
                                    )
                                    .child(
                                        div()
                                            .h_flex()
                                            .gap_2()
                                            .child(
                                                UiButton::new(format!("profile-java-choose-{id}"))
                                                    .px_3()
                                                    .py_2()
                                                    .rounded(px(7.))
                                                    .bg(rgb(palette.control))
                                                    .text_color(rgb(palette.foreground))
                                                    .hover(|style| style.bg(rgb(palette.hover)))
                                                    .on_click(move |_, _, cx| {
                                                        if let Some(owner) = choose_owner.upgrade()
                                                        {
                                                            let _ = owner.update(cx, |this, cx| {
                                                                this.choose_java_for_profile(
                                                                    id, cx,
                                                                );
                                                            });
                                                        }
                                                    })
                                                    .child(
                                                        locale
                                                            .text("Выбрать Java…", "Choose Java…"),
                                                    ),
                                            )
                                            .child(
                                                UiButton::new(format!("profile-java-auto-{id}"))
                                                    .px_3()
                                                    .py_2()
                                                    .rounded(px(7.))
                                                    .bg(rgb(palette.control))
                                                    .text_color(rgb(palette.muted))
                                                    .hover(|style| style.bg(rgb(palette.hover)))
                                                    .on_click(move |_, _, cx| {
                                                        if let Some(owner) =
                                                            automatic_owner.upgrade()
                                                        {
                                                            let _ = owner.update(cx, |this, cx| {
                                                                this.set_profile_java_path(
                                                                    id, None, cx,
                                                                );
                                                            });
                                                        }
                                                    })
                                                    .child(
                                                        locale.text("Автоматически", "Automatic"),
                                                    ),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(palette.muted))
                                    .child(locale.text(
                                        "Для запуска нужна Java, совместимая с версией Minecraft.",
                                        "The selected Java must support this Minecraft version.",
                                    )),
                            )
                            .child(
                                UiButton::new(format!("profile-settings-save-{id}"))
                                    .w_full()
                                    .px_3()
                                    .py_2()
                                    .rounded(px(8.))
                                    .bg(rgb(palette.accent))
                                    .text_color(rgb(palette.accent_foreground))
                                    .hover(|style| style.opacity(0.9))
                                    .on_click(move |_, window, cx| {
                                        let memory_mb =
                                            save_slider.read(cx).value().end().round() as u32;
                                        if let Some(owner) = save_owner.upgrade() {
                                            let _ = owner.update(cx, |this, cx| {
                                                this.instances.set_memory_mb(id, memory_mb);
                                                this.persist_profiles();
                                                cx.notify();
                                            });
                                            window.close_dialog(cx);
                                        }
                                    })
                                    .child(locale.text("Сохранить", "Save")),
                            ),
                    )
                })
        });
    }

    fn open_delete_profile_confirmation(
        &mut self,
        id: uuid::Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.instances.profile(id) else {
            return;
        };
        let profile_name = profile.name.clone();
        let locale = self.settings.locale();
        let palette = Palette::for_settings(self.settings.theme(), self.settings.accent());
        let title = locale.text("Удалить сборку?", "Delete profile?");
        let owner = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _, _| {
            let delete_owner = owner.clone();
            let message = match locale {
                Locale::Ru => format!(
                    "Удалить «{profile_name}» и его локальные данные? Миры, настройки и папка профиля будут удалены. Общие assets и библиотеки сохранятся."
                ),
                Locale::En => format!(
                    "Delete “{profile_name}” and its local data? Worlds, settings, and the profile folder will be removed. Shared assets and libraries will be kept."
                ),
            };

            dialog
                .w(px(460.))
                .title(title)
                .content(move |content, _, _| {
                    let action_owner = delete_owner.clone();
                    content.child(
                        div()
                            .v_flex()
                            .w_full()
                            .gap_4()
                            .p_4()
                            .text_color(rgb(palette.foreground))
                            .child(div().text_size(px(13.)).child(message.clone()))
                            .child(
                                div()
                                    .h_flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        UiButton::new(format!("cancel-delete-profile-{id}"))
                                            .px_3()
                                            .py_2()
                                            .rounded(px(8.))
                                            .bg(rgb(palette.control))
                                            .text_color(rgb(palette.foreground))
                                            .hover(|style| style.bg(rgb(palette.hover)))
                                            .on_click(|_, window, cx| window.close_dialog(cx))
                                            .child(locale.text("Отмена", "Cancel")),
                                    )
                                    .child(
                                        UiButton::new(format!("confirm-delete-profile-{id}"))
                                            .px_3()
                                            .py_2()
                                            .rounded(px(8.))
                                            .bg(rgb(0xc0392b))
                                            .text_color(rgb(0xffffff))
                                            .hover(|style| style.opacity(0.9))
                                            .on_click(move |_, window, cx| {
                                                if let Some(owner) = action_owner.upgrade() {
                                                    let _ = owner.update(cx, |this, cx| {
                                                        let Some(profile) = this
                                                            .instances
                                                            .profile(id)
                                                            .cloned()
                                                        else {
                                                            return;
                                                        };
                                                        let runtime = this.instances.runtime(&id);
                                                        if runtime.is_busy() {
                                                            return;
                                                        }

                                                        this.instances.mark_removing(id);
                                                        let (sender, receiver) =
                                                            futures::channel::mpsc::unbounded();
                                                        let worker = std::thread::Builder::new()
                                                            .name(format!("remove-profile-{id}"))
                                                            .spawn(move || {
                                                                remove_profile_data(profile, sender)
                                                            });
                                                        match worker {
                                                            Ok(_) => this.listen_for_instance_events(
                                                                receiver, cx,
                                                            ),
                                                            Err(error) => this.instances
                                                                .mark_removal_failed(
                                                                    id,
                                                                    format!(
                                                                        "Не удалось запустить удаление: {error}"
                                                                    ),
                                                                ),
                                                        }
                                                        cx.notify();
                                                    });
                                                }
                                                window.close_dialog(cx);
                                            })
                                            .child(locale.text("Удалить", "Delete")),
                                    ),
                            ),
                    )
                })
        });
    }

    fn open_version_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn version_picker_body(
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

    fn search_bar(&self, palette: Palette, locale: Locale) -> AnyElement {
        Input::new(&self.search_input)
            .w_full()
            .h(px(40.))
            .px_3()
            .rounded(px(8.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .prefix(IconName::Search)
            .aria_label(locale.text("Поиск по каталогу", "Search catalog"))
            .into_any_element()
    }

    fn navigation_button(
        &self,
        page: Page,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_selected = self.active_page == page
            || (page == Page::Mods && self.active_page == Page::ModDetails);
        let is_collapsed = self.sidebar.is_collapsed();
        let title = page.title(locale);
        let content = if is_collapsed {
            div()
                .w_full()
                .flex()
                .justify_center()
                .child(page.icon())
                .into_any_element()
        } else {
            div()
                .w_full()
                .h_flex()
                .items_center()
                .gap_3()
                .child(page.icon())
                .child(title)
                .into_any_element()
        };

        let button = UiButton::new(page.element_id())
            .w_full()
            .px_3()
            .py_3()
            .rounded(px(8.))
            .bg(rgb(if is_selected {
                palette.selected
            } else {
                palette.sidebar
            }))
            .text_color(rgb(if is_selected {
                palette.accent
            } else {
                palette.foreground
            }))
            .hover(|style| {
                style.bg(rgb(if is_selected {
                    palette.selected_hover
                } else {
                    palette.hover
                }))
            })
            .active(|style| style.bg(rgb(palette.pressed)))
            .selected(is_selected)
            .accessibility_label(title)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.navigate_to(page, window, cx);
            }))
            .child(content);

        let button = if is_collapsed {
            button.tooltip(move |window, cx| Tooltip::new(title).build(window, cx))
        } else {
            button
        };

        button.into_any_element()
    }

    /// Sidebar account placeholder: skin head plus player name, with a menu to
    /// pick the offline (no account) mode and edit the offline player name.
    fn account_block(&self, palette: Palette, locale: Locale, cx: &Context<Self>) -> AnyElement {
        let collapsed = self.sidebar.is_collapsed();
        let player_name = self.settings.offline_player().to_owned();
        let offline_label = locale
            .text("Офлайн-режим (без аккаунта)", "Offline mode (no account)")
            .to_owned();
        let block_label = locale.text("Выбор аккаунта", "Account picker").to_owned();
        let menu_title = locale.text("Аккаунт", "Account").to_owned();
        let edit_label = locale.text("Изменить ник", "Change name").to_owned();
        let edit_owner = cx.weak_entity();

        let head = match self.skins.head() {
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
                .ghost()
                .w_full()
                .h_11()
                .accessibility_label(block_label)
                .child(head)
        } else {
            ComponentButton::new("account-switcher")
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
                                let _ = owner.update(cx, |this, cx| {
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
        let palette = Palette::for_settings(self.settings.theme(), self.settings.accent());
        let locale = self.settings.locale();
        let title = locale.text("Ник игрока", "Player name").to_owned();
        let nickname_input = self.nickname_input.clone();
        let current = self.settings.offline_player().to_owned();
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
                                UiButton::new("save-nickname")
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
                                            let _ = owner.update(cx, |this, cx| {
                                                this.settings.set_offline_player(name.clone());
                                                this.persist_settings();
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

    fn sidebar_toggle_button(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_collapsed = self.sidebar.is_collapsed();
        let title = if is_collapsed {
            locale.text("Развернуть панель", "Expand sidebar")
        } else {
            locale.text("Свернуть панель", "Collapse sidebar")
        };

        UiButton::new("sidebar-toggle")
            .w(px(24.))
            .h(px(24.))
            .rounded(px(6.))
            .bg(rgb(palette.sidebar))
            .text_color(rgb(palette.muted))
            .hover(|style| {
                style
                    .bg(rgb(palette.hover))
                    .text_color(rgb(palette.foreground))
            })
            .active(|style| style.bg(rgb(palette.pressed)))
            .accessibility_label(title)
            .tooltip(move |window, cx| Tooltip::new(title).build(window, cx))
            .on_click(cx.listener(|this, _, _, cx| {
                this.sidebar.toggle();
                cx.notify();
            }))
            .child(if is_collapsed {
                IconName::PanelLeftOpen
            } else {
                IconName::PanelLeftClose
            })
            .into_any_element()
    }

    fn home_page(&self, palette: Palette, locale: Locale, cx: &Context<Self>) -> AnyElement {
        div()
            .v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .pr_4()
            .overflow_y_scrollbar()
            .gap_4()
            .when(self.instances.profiles().is_empty(), |this| {
                this.child(
                    div().v_flex().items_start().gap_3().p_6()
                        .rounded(px(12.)).bg(rgb(palette.surface))
                        .border_1().border_color(rgb(palette.border))
                        .child(div().text_size(px(20.)).font_weight(FontWeight::BOLD)
                            .child(locale.text("Создай первую сборку", "Create your first build")))
                        .child(div().text_size(px(13.)).text_color(rgb(palette.muted))
                            .child(locale.text("Выбери версию Minecraft. Здесь появятся твои сборки и кнопки запуска.", "Choose a Minecraft version. Your builds and launch controls will appear here.")))
                        .child(UiButton::new("home-create-build")
                            .px_4().py_3().rounded(px(8.))
                            .bg(rgb(palette.accent)).text_color(rgb(palette.accent_foreground))
                            .hover(|style| style.opacity(0.9))
                            .on_click(cx.listener(|this, _, window, cx| this.open_version_picker(window, cx)))
                            .child(locale.text("Создать сборку", "Create build"))),
                )
            })
            .when(!self.instances.profiles().is_empty(), |this| {
                this.child(self.instances_section(palette, locale, cx))
            })
            .into_any_element()
    }
    fn download_indicator(
        &self,
        summary: crate::features::downloads::DownloadSummary,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let title = format!(
            "{} · {}",
            locale.text("Загрузки и установка", "Downloads and installation"),
            summary.active_count
        );
        let detail = if summary.active_count == 1 {
            let mut detail = summary.profile_name;
            if let Some(total) = summary.total_bytes
                && (summary.received > 0 || total > 0)
            {
                detail.push_str(&format!(
                    " · {} / {}",
                    format_bytes(summary.received),
                    format_bytes(total)
                ));
            }
            if summary.speed_bps > 0. {
                detail.push_str(&format!(" · {}", format_speed(summary.speed_bps)));
            }
            detail
        } else {
            locale
                .text(
                    "Открыть сборки и посмотреть прогресс",
                    "Open builds to see progress",
                )
                .to_owned()
        };
        UiButton::new("global-downloads")
            .w_full()
            .flex_shrink_0()
            .px_4()
            .py_3()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .text_color(rgb(palette.foreground))
            .hover(|style| style.bg(rgb(palette.hover)))
            .accessibility_label(format!("{title}. {detail}"))
            .on_click(
                cx.listener(|this, _, window, cx| this.navigate_to(Page::Instances, window, cx)),
            )
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_color(rgb(palette.accent))
                            .child(IconName::Download),
                    )
                    .child(
                        div()
                            .v_flex()
                            .flex_1()
                            .min_w_0()
                            .items_start()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.muted))
                                    .child(detail),
                            ),
                    )
                    .child(IconName::ChevronRight),
            )
            .into_any_element()
    }

    fn instances_page(&self, palette: Palette, locale: Locale, cx: &Context<Self>) -> AnyElement {
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

    fn instances_section(
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

    fn instance_card(
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

    fn home_mod_card(&self, sample: &ModSample, palette: Palette, locale: Locale) -> AnyElement {
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

    fn mods_page(&self, palette: Palette, locale: Locale, cx: &Context<Self>) -> AnyElement {
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
    fn filter_section(
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

    fn mod_install_action(
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
                                this.open_mod_download_dialog(download_project.clone(), window, cx)
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

    fn mod_card(
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
                this.load_mod_description(
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

        let action = if project.kind == ProjectKind::Mod {
            self.mod_install_action(project, palette, locale, cx)
        } else {
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

    /// Build details page: metadata of the build and its installed mods with
    /// enable/disable switches.
    fn instance_details_page(
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

    fn mod_details_page(&self, palette: Palette, locale: Locale, cx: &Context<Self>) -> AnyElement {
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

    fn mod_detail_content(
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

    fn mod_screenshots(&self, sample: &ModProject, palette: Palette, locale: Locale) -> AnyElement {
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

    fn empty_page(&self, palette: Palette, locale: Locale) -> AnyElement {
        div()
            .text_size(px(13.))
            .text_color(rgb(palette.muted))
            .child(locale.text("Здесь пока пусто", "Nothing here yet"))
            .into_any_element()
    }

    fn settings_page(&self, palette: Palette, cx: &mut Context<Self>) -> AnyElement {
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

impl Render for LauncherApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = self.settings.locale();
        let palette = Palette::for_settings(self.settings.theme(), self.settings.accent());
        let active_page = self.active_page;

        let navigation_items: Vec<_> = Page::LIBRARY
            .into_iter()
            .map(|page| self.navigation_button(page, palette, locale, cx))
            .collect();
        let settings_button = self.navigation_button(Page::Settings, palette, locale, cx);
        let collapse_button = self.sidebar_toggle_button(palette, locale, cx);
        let sidebar_header = div()
            .h_flex()
            .w_full()
            .items_center()
            .justify_end()
            .child(collapse_button)
            .into_any_element();
        let navigation = if self.sidebar.is_collapsed() {
            div().v_flex().gap_2()
        } else {
            div().v_flex().gap_2().child(
                div()
                    .px_3()
                    .text_size(px(10.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(palette.muted))
                    .child(locale.text("БИБЛИОТЕКА", "LIBRARY")),
            )
        }
        .children(navigation_items);
        let page_body = match active_page {
            Page::Home => self.home_page(palette, locale, cx),
            Page::Instances => self.instances_page(palette, locale, cx),
            Page::Mods => self.mods_page(palette, locale, cx),
            Page::Modpacks | Page::ResourcePacks | Page::Shaders => {
                self.mods_page(palette, locale, cx)
            }
            Page::Settings => self.settings_page(palette, cx),
            Page::Skins => skin_view::skin_page(self, palette, locale, cx),
            Page::ModDetails => self.mod_details_page(palette, locale, cx),
            Page::InstanceDetails(id) => self.instance_details_page(id, palette, locale, cx),
        };
        let page_subtitle = match active_page {
            Page::Home => "",
            Page::Instances => locale.text(
                "Версия игры, загрузчик и установленные моды",
                "Game version, loader, and installed mods",
            ),
            Page::Mods => locale.text("", "Find add-ons for your setup"),
            Page::Modpacks => locale.text(
                "Готовые сборки модов для Minecraft",
                "Ready-made Minecraft mod collections",
            ),
            Page::ResourcePacks => locale.text(
                "Измени текстуры и оформление игры",
                "Change the game's textures and appearance",
            ),
            Page::Shaders => locale.text(
                "Найди шейдеры и измени графику игры",
                "Find shaders and change the game's visuals",
            ),
            Page::Settings => locale.text(
                "Язык и оформление лаунчера",
                "Launcher language and appearance",
            ),
            Page::Skins => locale.text(
                "Загружай и выбирай скины персонажа",
                "Upload and choose your character skins",
            ),
            Page::ModDetails => "",
            Page::InstanceDetails(_) => "",
        };
        let page_header = if matches!(active_page, Page::ModDetails | Page::InstanceDetails(_)) {
            div().into_any_element()
        } else {
            div()
                .v_flex()
                .gap_2()
                .child(
                    div()
                        .text_size(px(26.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(palette.foreground))
                        .child(active_page.title(locale)),
                )
                .when(!page_subtitle.is_empty(), |header| {
                    header.child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(palette.muted))
                            .child(page_subtitle),
                    )
                })
                .into_any_element()
        };

        div()
            .flex()
            .size_full()
            .bg(rgb(palette.background))
            .child(
                div()
                    .v_flex()
                    .w(if self.sidebar.is_collapsed() {
                        px(76.)
                    } else {
                        px(252.)
                    })
                    .h_full()
                    .px_3()
                    .py_5()
                    .gap_6()
                    .bg(rgb(palette.sidebar))
                    .border_r_1()
                    .border_color(rgb(palette.border))
                    .child(sidebar_header)
                    .child(navigation)
                    .child(div().flex_1())
                    .child(div().h(px(1.)).w_full().bg(rgb(palette.border)))
                    .child(self.account_block(palette, locale, cx))
                    .child(settings_button),
            )
            .child(
                div()
                    .v_flex()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .px_8()
                    .pt_4()
                    .pb_8()
                    .gap_4()
                    .child(page_header)
                    .child(page_body)
                    .when_some(
                        crate::features::downloads::DownloadSummary::from_instances(
                            &self.instances,
                        ),
                        |this, summary| {
                            this.child(self.download_indicator(summary, palette, locale, cx))
                        },
                    ),
            )
    }
}

fn mod_thumbnail(sample: &ModSample, size: Pixels) -> AnyElement {
    mod_thumbnail_data(
        sample.icon,
        sample.icon_background,
        sample.icon_foreground,
        sample.image_url,
        size,
    )
}

fn mod_project_thumbnail(project: &ModProject, size: Pixels) -> AnyElement {
    mod_thumbnail_data(
        project.icon,
        project.icon_background,
        project.icon_foreground,
        project.image_url.as_deref(),
        size,
    )
}

fn mod_thumbnail_data(
    icon: IconName,
    background: u32,
    foreground: u32,
    image_url: Option<&str>,
    size: Pixels,
) -> AnyElement {
    if let Some(url) = image_url {
        div()
            .w(size)
            .h(size)
            .rounded(px(12.))
            .overflow_hidden()
            .bg(rgb(background))
            .child(
                img(url)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .rounded(px(12.))
                    .with_loading(move || mod_icon_placeholder(icon, background, foreground, size))
                    .with_fallback(move || {
                        mod_icon_placeholder(icon, background, foreground, size)
                    }),
            )
            .into_any_element()
    } else {
        mod_icon_placeholder(icon, background, foreground, size)
    }
}

fn version_type_label(version_type: &str, locale: Locale) -> &'static str {
    match version_type {
        "release" => locale.text("Релиз", "Release"),
        "snapshot" => locale.text("Снапшот", "Snapshot"),
        "old_beta" => locale.text("Бета", "Beta"),
        "old_alpha" => locale.text("Альфа", "Alpha"),
        _ => locale.text("Другое", "Other"),
    }
}

fn field_label(label: &'static str) -> AnyElement {
    div()
        .text_size(px(13.))
        .font_weight(FontWeight::BOLD)
        .child(label)
        .into_any_element()
}

fn loader_title(loader: LoaderChoice) -> &'static str {
    match loader {
        LoaderChoice::Vanilla => "Vanilla",
        LoaderChoice::Fabric => "Fabric",
        LoaderChoice::NeoForge => "NeoForge",
        LoaderChoice::Forge => "Forge",
    }
}

fn mod_loader_filter(loader: LoaderChoice) -> ModLoaderFilter {
    match loader {
        LoaderChoice::Vanilla => ModLoaderFilter::Vanilla,
        LoaderChoice::Fabric => ModLoaderFilter::Fabric,
        LoaderChoice::Forge => ModLoaderFilter::Forge,
        LoaderChoice::NeoForge => ModLoaderFilter::NeoForge,
    }
}

fn format_bytes(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= 1024 {
        format!("{:.0} KiB", bytes as f64 / 1024.)
    } else {
        format!("{bytes} B")
    }
}

fn format_speed(bytes_per_second: f64) -> String {
    const KIB: f64 = 1024.;
    const MIB: f64 = KIB * KIB;
    if bytes_per_second >= MIB {
        format!("{bytes_per_second:.1} MiB/s")
    } else if bytes_per_second >= KIB {
        format!("{bytes_per_second:.0} KiB/s")
    } else {
        format!("{bytes_per_second:.0} B/s")
    }
}

/// Maps vendor install stage names to localized labels.
fn stage_label(stage: &str, locale: Locale) -> &str {
    match stage {
        "Queued" => locale.text("В очереди", "Queued"),
        "ResolveVersion" => locale.text("Получение метаданных версии", "Fetching version metadata"),
        "DownloadLibraries" => locale.text(
            "Скачивание клиента и библиотек",
            "Downloading client and libraries",
        ),
        "DownloadAssets" => locale.text("Скачивание ресурсов игры", "Downloading game assets"),
        "ExtractNatives" => locale.text(
            "Распаковка natives-библиотек",
            "Extracting native libraries",
        ),
        "LoaderInstall" => locale.text("Установка загрузчика", "Installing mod loader"),
        "InstallRuntime" => locale.text("Установка Java", "Installing Java"),
        "Verify" => locale.text("Проверка файлов", "Verifying files"),
        other => other,
    }
}

/// Builds the slim progress bar shown while an install downloads.
///
/// Byte totals drive the bar when the plan declared file sizes; otherwise the
/// finished file count stands in. Plans without either stay without a bar.
fn install_progress_view(
    received: u64,
    total_bytes: Option<u64>,
    speed_bps: f64,
    completed_tasks: usize,
    total_tasks: Option<usize>,
    palette: Palette,
    _locale: Locale,
) -> Option<AnyElement> {
    let fraction = if let Some(total) = total_bytes.filter(|total| *total > 0) {
        Some(received as f64 / total as f64)
    } else {
        total_tasks
            .filter(|total| *total > 0)
            .map(|total| completed_tasks as f64 / total as f64)
    }?;
    let fraction = fraction.clamp(0., 1.);

    Some(
        div()
            .h(px(4.))
            .w_full()
            .rounded(px(2.))
            .bg(rgb(palette.control))
            .overflow_hidden()
            .child(
                div()
                    .h_full()
                    .w(relative(fraction as f32))
                    .bg(rgb(if speed_bps > 0. {
                        palette.accent
                    } else {
                        palette.muted
                    })),
            )
            .into_any_element(),
    )
}

fn mod_icon_placeholder(
    icon: IconName,
    background: u32,
    foreground: u32,
    size: Pixels,
) -> AnyElement {
    div()
        .w(size)
        .h(size)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(12.))
        .bg(rgb(background))
        .text_color(rgb(foreground))
        .text_size(px((size.as_f32() * 0.5).max(14.)))
        .child(icon)
        .into_any_element()
}

fn detail_meta_row(
    icon: IconName,
    label: &'static str,
    value: impl Into<String>,
    palette: Palette,
) -> AnyElement {
    div()
        .h_flex()
        .w_full()
        .min_w_0()
        .items_start()
        .gap_2()
        .text_color(rgb(palette.muted))
        .child(icon)
        .child(
            div()
                .v_flex()
                .flex_1()
                .min_w_0()
                .w_full()
                .gap_1()
                .child(div().text_size(px(10.)).child(label))
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_size(px(11.))
                        .text_color(rgb(palette.foreground))
                        .child(value.into()),
                ),
        )
        .into_any_element()
}
