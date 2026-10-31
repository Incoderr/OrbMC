//! Content catalog (mods, modpacks, resource packs, shaders): search,
//! project pages, installs into builds and the installed-mods inventory.
//!
//! The app embeds [`CatalogView`], tells it where the user navigated with
//! [`CatalogView::navigated`], and follows its [`CatalogEvent`]s.

mod catalog_page;
mod details_page;
mod gallery;
mod install_dialog;
pub(crate) mod widgets;

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;

use super::catalog::{
    ContentInstall, ModInstall, ModProject, ModSource, ModpackRelease, fetch_project_description,
    fetch_project_info, install_compatible_content, install_compatible_mod, resolve_modpack,
    search_catalog,
};
use super::{
    AuthorFilter, ModCompatibilityFilter, ModsState, ProjectKind, installed, mod_loader_filter,
    translation,
};
use crate::features::downloads::{FileDownload, Transfer, TransferHandle};
use crate::features::instances::InstanceProfile;
use crate::features::instances::{InstanceEvent, InstanceId, InstanceRuntime, InstancesState};
use crate::features::settings::SettingsState;
use crate::features::versions::VersionPicker;
use crate::shared::locale::Locale;
use crate::shared::theme::Palette;

/// Navigation the catalog asks the app for.
#[derive(Clone, Copy, Debug)]
pub enum CatalogEvent {
    OpenDetails,
    BackToCatalog,
    OpenBuilds,
}

/// Where the user is, as far as the catalog cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogTarget {
    Catalog,
    Details,
    Build(InstanceId),
    Elsewhere,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    List,
    Details,
}

pub struct CatalogView {
    mods: ModsState,
    instances: Entity<InstancesState>,
    version_picker: Entity<VersionPicker>,
    search_input: Entity<InputState>,
    mod_version_input: Entity<InputState>,
    mods_scroll: gpui_kit::base::VirtualListScrollHandle,
    screen: Screen,
    /// One-off message above the results (a modpack that cannot be installed).
    notice: Option<String>,
    /// Build page currently open; its mod list reloads after scans.
    open_build: Option<InstanceId>,
    palette: Palette,
    locale: Locale,
    settings: Entity<SettingsState>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<CatalogEvent> for CatalogView {}

impl CatalogView {
    pub fn new(
        instances: Entity<InstancesState>,
        version_picker: Entity<VersionPicker>,
        settings: &Entity<SettingsState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (palette, locale) = {
            let settings = settings.read(cx);
            (settings.palette(), settings.locale())
        };
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(search_placeholder(locale)));
        let mod_version_input = cx.new(|cx| InputState::new(window, cx).placeholder("1.21.1"));
        let subscriptions = vec![
            cx.subscribe(&mod_version_input, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value().to_string();
                    this.apply_version_filter(&value, true, cx);
                    // The suggestion list narrows with every keystroke.
                    cx.notify();
                }
            }),
            cx.subscribe(&search_input, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = input.read(cx).value().to_string();
                    this.mods.set_query(query);
                    this.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
                    if this.screen == Screen::List {
                        this.load_mod_catalog(cx, true);
                    }
                    cx.notify();
                }
            }),
            // Install states and the game version filter come from these.
            cx.observe(&instances, |_, _, cx| cx.notify()),
            cx.subscribe(&instances, |this, _, event: &InstanceEvent, _| {
                if let InstanceEvent::Removed(id) = *event {
                    this.forget_profile(id);
                }
            }),
            cx.observe(&version_picker, |_, _, cx| cx.notify()),
            cx.observe_in(settings, window, |this, settings, window, cx| {
                let (palette, locale, translate) = {
                    let settings = settings.read(cx);
                    (
                        settings.palette(),
                        settings.locale(),
                        settings.translate_descriptions(),
                    )
                };
                if this.mods.translates() != translate {
                    this.mods.set_translate(translate);
                    this.translate_catalog_descriptions(cx);
                }
                this.follow_settings(palette, locale, window, cx);
            }),
        ];
        Self {
            mods: {
                let mut mods = ModsState::default();
                mods.set_translate(settings.read(cx).translate_descriptions());
                mods
            },
            instances,
            version_picker,
            search_input,
            mod_version_input,
            mods_scroll: gpui_kit::base::VirtualListScrollHandle::new(),
            screen: Screen::List,
            open_build: None,
            notice: None,
            palette,
            locale,
            settings: settings.clone(),
            _subscriptions: subscriptions,
        }
    }

    fn follow_settings(
        &mut self,
        palette: Palette,
        locale: Locale,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.palette = palette;
        if self.locale != locale {
            self.locale = locale;
            self.search_input.update(cx, |input, cx| {
                input.set_placeholder(search_placeholder(locale), window, cx);
            });
            self.translate_catalog_descriptions(cx);
        }
        cx.notify();
    }

    /// Chooses which kind of content the list shows the next time it opens.
    pub fn set_kind(&mut self, kind: ProjectKind) {
        self.mods.set_kind(kind);
    }

    /// Switches the open list to another kind of content.
    pub(crate) fn switch_kind(&mut self, kind: ProjectKind, cx: &mut Context<Self>) {
        if self.mods.kind() == kind {
            return;
        }
        self.mods.set_kind(kind);
        self.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.refresh_installed_mods(cx);
        self.load_mod_catalog(cx, false);
        cx.notify();
    }

    /// Follows app navigation. `keep_query` holds the search text while the
    /// user moves between the list and a project page.
    pub fn navigated(
        &mut self,
        target: CatalogTarget,
        keep_query: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !keep_query {
            self.mods.set_query(String::new());
            self.search_input
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.open_build = None;
        self.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
        match target {
            CatalogTarget::Catalog => {
                self.screen = Screen::List;
                self.version_picker
                    .update(cx, |picker, cx| picker.load_version_manifests(cx));
                self.refresh_installed_mods(cx);
                self.load_mod_catalog(cx, false);
            }
            CatalogTarget::Details => self.screen = Screen::Details,
            CatalogTarget::Build(id) => {
                self.open_build = Some(id);
                self.load_installed_entries(id, cx);
                // The scan enriches legacy records with names, authors and
                // icons; the listed entries reload when the scan finishes.
                self.refresh_installed_mods(cx);
            }
            CatalogTarget::Elsewhere => {}
        }
        cx.notify();
    }

    /// Opens the catalog filtered for one build ("add mods" on a build page).
    pub fn select_profile(&mut self, id: InstanceId, filter: ModCompatibilityFilter) {
        self.mods.select_profile(id, filter);
    }

    /// Applies a typed or picked game version to the catalog. Partial input
    /// ("1.2") only narrows the suggestion list; the catalog is filtered once
    /// the text is empty or names a known release.
    fn apply_version_filter(&mut self, value: &str, debounce: bool, cx: &mut Context<Self>) {
        if self.mods.target_profile().is_some() {
            return;
        }
        let value = value.trim();
        let releases = self.version_picker.read(cx).release_ids();
        // Before the version list loads, trust what the user typed.
        let complete =
            value.is_empty() || releases.is_empty() || releases.iter().any(|id| id == value);
        if complete {
            self.mods.set_game_version_filter(value.to_owned());
            self.load_mod_catalog(cx, debounce);
        }
    }

    /// Redraws the byte progress of a build's download while it runs; the
    /// download itself only updates shared counters.
    fn watch_transfer(&self, id: InstanceId, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let active = this
                    .update(cx, |this, cx| {
                        let active = matches!(
                            this.instances.read(cx).runtime(&id),
                            InstanceRuntime::DownloadingMod { .. }
                                | InstanceRuntime::DownloadingContent { .. }
                        );
                        if active {
                            this.instances.update(cx, |_, cx| cx.notify());
                        }
                        active
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        })
        .detach();
    }

    /// Installs a Modrinth modpack as a new build: the build appears at once
    /// with download progress, the game installs when the pack files are in.
    pub(crate) fn install_modpack(&mut self, project: ModProject, cx: &mut Context<Self>) {
        if self.mods.installed.is_installing(&project.id) {
            return;
        }
        self.mods.installed.begin_install(&project.id);
        let client = cx.http_client();
        let project_id = project.id.clone();
        cx.spawn(async move |this, cx| {
            let release = resolve_modpack(client.clone(), &project.id).await;
            let _ = this.update(cx, |this, cx| match release {
                Ok(release) => this.start_modpack_download(project, release, cx),
                Err(error) => {
                    tracing::warn!(project = %project.id, error = %error.message, "modpack unavailable");
                    this.mods.installed.finish_install(&project_id);
                    this.notice = Some(error.message);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn start_modpack_download(
        &mut self,
        project: ModProject,
        release: ModpackRelease,
        cx: &mut Context<Self>,
    ) {
        let mut profile = InstanceProfile::new(
            project.name.clone(),
            release.game_version.clone(),
            crate::features::modpacks::loader_from_slug(release.loader.as_deref()),
            None,
            self.settings.read(cx).download_directory().clone(),
        );
        profile.modpack_icon = project.image_url.clone();
        let id = profile.id;
        let transfer = TransferHandle::default();
        self.instances.update(cx, |instances, _| {
            instances.add(profile);
            instances.save();
            instances.mark_content_downloading(id, project.name.clone(), transfer.clone());
        });
        self.watch_transfer(id, cx);
        cx.emit(CatalogEvent::OpenBuilds);

        let Some(profile) = self.instances.read(cx).profile(id).cloned() else {
            return;
        };
        let game_directory = profile.game_directory();
        let work_directory = match profile.data_directory() {
            Ok(data) => data.join("modpack"),
            Err(error) => {
                self.instances.update(cx, |instances, _| {
                    instances.mark_content_download_failed(id, error.to_string())
                });
                self.mods.installed.finish_install(&project.id);
                return;
            }
        };
        let client = cx.http_client();
        let transfer = Transfer {
            handle: transfer,
            executor: cx.background_executor().clone(),
        };
        let task = cx.background_executor().spawn(async move {
            let destination = work_directory.join(&release.filename);
            crate::features::modpacks::install_pack(
                client,
                FileDownload {
                    url: &release.url,
                    destination: &destination,
                    size: release.size,
                    sha1: release.sha1.as_deref(),
                    max_size: 1024 * 1024 * 1024,
                },
                &game_directory,
                &work_directory,
                &transfer,
            )
            .await
        });
        let project_id = project.id.clone();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.mods.installed.finish_install(&project_id);
                this.instances.update(cx, |instances, cx| match result {
                    Ok(target) => {
                        instances.set_game(
                            id,
                            target.game_version,
                            target.loader,
                            target.loader_version,
                        );
                        instances.save();
                        instances.start_install(id, cx);
                    }
                    Err(error) => {
                        tracing::warn!(profile_id = %id, %error, "modpack install failed");
                        instances.mark_content_download_failed(id, error.to_string());
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    /// Prepares the catalog to list the projects of a Modrinth author; the
    /// catalog loads when the app navigates to it.
    pub fn filter_by_author(&mut self, name: String) {
        self.mods.set_author(Some(AuthorFilter {
            name,
            curseforge_id: None,
        }));
    }

    /// Lists the projects of the author of `project`.
    pub(crate) fn show_author(&mut self, project: &ModProject, cx: &mut Context<Self>) {
        self.mods.set_author(Some(AuthorFilter {
            name: project.author.clone(),
            curseforge_id: project.author_id,
        }));
        self.mods_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.load_mod_catalog(cx, false);
        if self.screen == Screen::Details {
            self.mods.clear_selected_mod();
            cx.emit(CatalogEvent::BackToCatalog);
        }
        cx.notify();
    }

    /// Drops the build filter when that build is deleted.
    fn forget_profile(&mut self, id: InstanceId) {
        if self.mods.target_profile() == Some(id) {
            self.mods.clear_compatibility_filter();
        }
    }

    pub fn is_scanning(&self, id: InstanceId) -> bool {
        self.mods.installed.is_loading(id)
    }

    pub fn build_mods(&self, id: InstanceId) -> &[installed::InstalledModEntry] {
        self.mods.build_mods(id)
    }

    pub(crate) fn refresh_installed_mods(&mut self, cx: &mut Context<Self>) {
        for profile in self.instances.read(cx).profiles().to_vec() {
            let id = profile.id;
            if self.mods.installed.is_loading(id)
                || matches!(
                    self.instances.read(cx).runtime(&id),
                    InstanceRuntime::DownloadingMod { .. }
                        | InstanceRuntime::DownloadingContent { .. }
                        | InstanceRuntime::Removing
                        | InstanceRuntime::Installing { .. }
                )
            {
                continue;
            }
            let location = match profile.mods_location() {
                Ok(location) => location,
                Err(error) => {
                    tracing::warn!(profile_id = %id, %error, "build data directory is unavailable");
                    continue;
                }
            };
            let generation = self.mods.installed.begin(id);
            let key = super::curseforge_api_key();
            let task = cx
                .background_executor()
                .spawn(async move { installed::scan(&location, key.as_deref()) });
            cx.spawn(async move |this, cx| {
                let inventory = task.await;
                let _ = this.update(cx, |this, cx| {
                    if this.instances.read(cx).profile(id).is_some() {
                        if let Some(warning) = &inventory.warning {
                            tracing::warn!(profile_id = %id, %warning, "mod inventory incomplete");
                        }
                        this.mods.installed.finish(id, generation, inventory);
                        // A build page currently open must see the enriched
                        // records without re-entering it.
                        if this.open_build == Some(id) {
                            this.load_installed_entries(id, cx);
                        }
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    pub(crate) fn load_mod_catalog(&mut self, cx: &mut Context<Self>, debounce: bool) {
        let (request_id, query, sort, source, compatibility_filter, kind, page) =
            self.mods.begin_search();
        let client = cx.http_client();
        let curseforge_api_key = super::curseforge_api_key();
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
    pub(crate) fn translate_catalog_descriptions(&mut self, cx: &mut Context<Self>) {
        if !self.mods.translates() {
            return;
        }
        let locale = self.locale;
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

    pub(crate) fn load_mod_details(
        &self,
        project_id: String,
        source: ModSource,
        request_id: u64,
        cx: &mut Context<Self>,
    ) {
        let client = cx.http_client();
        let curseforge_api_key = super::curseforge_api_key();
        cx.spawn(async move |this, cx| {
            let (description, info) = futures::join!(
                fetch_project_description(
                    client.clone(),
                    project_id.clone(),
                    source,
                    curseforge_api_key.clone(),
                ),
                fetch_project_info(client, project_id, source, curseforge_api_key),
            );
            let _ = this.update(cx, |this, cx| {
                if this.mods.finish_description_load(request_id, description) {
                    cx.notify();
                }
                if this.mods.finish_info_load(request_id, info) {
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(crate) fn start_mod_download(
        &mut self,
        project: ModProject,
        profile_id: InstanceId,
        cx: &mut Context<Self>,
    ) {
        if self.mods.installed.contains(profile_id, &project.id)
            || self.mods.installed.is_loading(profile_id)
        {
            return;
        }
        let Some(profile) = self.instances.read(cx).profile(profile_id).cloned() else {
            return;
        };
        let runtime = self.instances.read(cx).runtime(&profile_id);
        if runtime.is_busy() {
            return;
        }

        let compatibility = ModCompatibilityFilter {
            game_version: profile.game_version.clone(),
            loader: mod_loader_filter(profile.loader),
        };
        let mods_location = match profile.mods_location() {
            Ok(location) => location,
            Err(error) => {
                tracing::warn!(%profile_id, %error, "build data directory is unavailable");
                return;
            }
        };
        let transfer = TransferHandle::default();
        self.instances.update(cx, |instances, _| {
            instances.mark_mod_downloading(profile_id, project.name.clone(), transfer.clone())
        });
        self.watch_transfer(profile_id, cx);
        let transfer = Transfer {
            handle: transfer,
            executor: cx.background_executor().clone(),
        };
        self.mods.installed.begin_install(&project.id);
        let client = cx.http_client();
        let curseforge_api_key = super::curseforge_api_key();
        let project_id = project.id;
        let pending_project_id = project_id.clone();
        let source = project.source;
        // Catalog metadata travels with the install so the build page shows
        // the full mod card from the first listing.
        let main_meta = installed::InstalledMeta {
            name: Some(project.name.clone()),
            author: Some(project.author.clone()).filter(|author| {
                !author.trim().is_empty() && !author.eq_ignore_ascii_case("unknown")
            }),
            version: None,
            icon_url: project.image_url.clone(),
        };
        cx.notify();

        let download_task = cx.background_executor().spawn(async move {
            install_compatible_mod(
                client,
                ModInstall {
                    project_id,
                    meta: main_meta,
                    source,
                    compatibility,
                    mods_location,
                },
                curseforge_api_key,
                transfer,
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
                            this.instances.update(cx, |instances, _| {
                                instances.mark_mod_downloaded(profile_id, filename.clone())
                            });
                        }
                        this.load_installed_entries(profile_id, cx);
                    }
                    Err(error) => {
                        this.instances.update(cx, |instances, _| {
                            instances.mark_mod_download_failed(profile_id, error.message)
                        });
                    }
                }
                this.mods.installed.finish_install(&pending_project_id);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn start_content_download(
        &mut self,
        project: ModProject,
        profile_id: InstanceId,
        cx: &mut Context<Self>,
    ) {
        if project.kind.install_directory().is_none()
            || self.mods.installed.is_installing(&project.id)
        {
            return;
        }
        let Some(profile) = self.instances.read(cx).profile(profile_id).cloned() else {
            return;
        };
        if self.instances.read(cx).runtime(&profile_id).is_busy() {
            return;
        }

        let game_directory = profile.game_directory();
        let game_version = profile.game_version.clone();
        let project_id = project.id.clone();
        let pending_project_id = project_id.clone();
        let source = project.source;
        let kind = project.kind;
        let locale = self.locale;
        let transfer = TransferHandle::default();
        self.instances.update(cx, |instances, _| {
            instances.mark_content_downloading(
                profile_id,
                format!("{} · {}", kind.title(locale), project.name),
                transfer.clone(),
            )
        });
        self.watch_transfer(profile_id, cx);
        let transfer = Transfer {
            handle: transfer,
            executor: cx.background_executor().clone(),
        };
        self.mods.installed.begin_install(&project_id);
        let client = cx.http_client();
        let curseforge_api_key = super::curseforge_api_key();
        cx.notify();

        let download_task = cx.background_executor().spawn(async move {
            install_compatible_content(
                client,
                ContentInstall {
                    project_id,
                    source,
                    kind,
                    game_version,
                    game_directory,
                },
                curseforge_api_key,
                transfer,
            )
            .await
        });
        cx.spawn(async move |this, cx| {
            let result = download_task.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(filename) => {
                        tracing::info!(
                            profile_id = %profile_id,
                            ?kind,
                            %filename,
                            "installed project content into profile"
                        );
                        this.instances.update(cx, |instances, _| {
                            instances.mark_content_downloaded(profile_id, filename)
                        });
                    }
                    Err(error) => {
                        this.instances.update(cx, |instances, _| {
                            instances.mark_content_download_failed(profile_id, error.message)
                        });
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
    pub(crate) fn load_installed_entries(
        &mut self,
        profile_id: InstanceId,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.instances.read(cx).profile(profile_id).cloned() else {
            return;
        };
        let location = match profile.mods_location() {
            Ok(location) => location,
            Err(error) => {
                tracing::warn!(%profile_id, %error, "build data directory is unavailable");
                return;
            }
        };
        let task = cx
            .background_executor()
            .spawn(async move { installed::list_entries(&location) });
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
    pub(crate) fn toggle_mod_disabled(
        &mut self,
        profile_id: InstanceId,
        filename: String,
        disable: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.instances.read(cx).profile(profile_id).cloned() else {
            return;
        };
        if self.instances.read(cx).runtime(&profile_id).is_busy()
            || self.mods.installed.is_loading(profile_id)
        {
            return;
        }
        let directory = profile.game_directory().join("mods");
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
    pub(crate) fn uninstall_mod(
        &mut self,
        project_id: String,
        profile_id: InstanceId,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.instances.read(cx).profile(profile_id).cloned() else {
            return;
        };
        if self.instances.read(cx).runtime(&profile_id).is_busy()
            || self.mods.installed.is_loading(profile_id)
        {
            return;
        }
        let location = match profile.mods_location() {
            Ok(location) => location,
            Err(error) => {
                tracing::warn!(%profile_id, %error, "build data directory is unavailable");
                return;
            }
        };
        let generation = self.mods.installed.begin(profile_id);
        let task = cx.background_executor().spawn(async move {
            let mut removal_error = None;
            match installed::uninstall(&location, &project_id) {
                Ok(files) if files.is_empty() => {}
                Ok(files) => tracing::info!(
                    profile_id = %profile_id,
                    removed = ?files,
                    "uninstalled mod files from profile"
                ),
                Err(error) => removal_error = Some(error),
            }
            let api_key = super::curseforge_api_key();
            let mut inventory = installed::scan(&location, api_key.as_deref());
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
                if this.instances.read(cx).profile(profile_id).is_some() {
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

    /// The selected build, when the mod is installed there. Without a
    /// selected build the catalog offers no removal.
    pub(crate) fn uninstall_target(&self, project_id: &str) -> Option<InstanceId> {
        let id = self.mods.target_profile()?;
        self.mods.installed.contains(id, project_id).then_some(id)
    }
}

impl Render for CatalogView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (palette, locale) = (self.palette, self.locale);
        match self.screen {
            Screen::List => self.mods_page(palette, locale, cx),
            Screen::Details => self.mod_details_page(palette, locale, cx),
        }
    }
}

fn search_placeholder(locale: Locale) -> &'static str {
    locale.text(
        "Поиск по названию, описанию или автору",
        "Search by name, description, or author",
    )
}
