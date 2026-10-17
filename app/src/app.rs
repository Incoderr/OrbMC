use std::sync::Arc;

use self::theme::{Palette, SidebarState};
use self::ui::mod_loader_filter;
use crate::platform::profile_store;
use crate::{
    features::instances::{
        InstanceRuntime, InstanceWorkerEvent, InstancesState, install_profile,
        is_valid_player_name, launch_profile,
    },
    features::mods::{
        ModCompatibilityFilter, ModsState,
        catalog::{
            ModProject, ModSource, fetch_project_description, install_compatible_mod,
            search_catalog,
        },
        installed, translation,
    },
    features::settings::{AccentColor, Locale, SettingsState, ThemeChoice},
    features::versions::{
        LoaderChoice, VersionCatalogState, fetch_fabric_game_loaders, fetch_manifests,
    },
    router::Page,
};
use futures::StreamExt;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use mc_launcher_core::prelude::DownloadControl;
use std::time::Duration;

#[path = "dialogs/mod.rs"]
mod dialogs;
#[path = "pages/mod.rs"]
mod pages;
#[path = "sidebar.rs"]
mod sidebar;
#[path = "skin_view.rs"]
mod skin_view;
#[path = "theme.rs"]
mod theme;
#[path = "ui.rs"]
mod ui;

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
