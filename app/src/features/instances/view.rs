//! Build screens: the home page, the build list and a build's own page,
//! with their dialogs (build settings, deletion, world picker).

mod builds_page;
mod home_page;
mod settings_dialog;
pub(crate) mod widgets;
mod world_picker;

use crate::features::auth::AuthState;
use gpui_kit::*;

use super::{InstanceId, InstancesState};
use crate::features::mods::CatalogView;
use crate::features::settings::SettingsState;
use crate::shared::locale::Locale;
use crate::shared::theme::Palette;

/// Which build screen is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildsScreen {
    Home,
    List,
    Details(InstanceId),
}

/// Navigation and dialogs the build screens ask the app for.
#[derive(Clone, Copy, Debug)]
pub enum InstancesViewEvent {
    CreateBuild,
    OpenCatalog,
    OpenBuild(InstanceId),
    OpenBuilds,
}

/// Tabs of a build's page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DetailsTab {
    #[default]
    Mods,
    Log,
}

/// Lines of the game log drawn at once; the copy button copies all of them.
const LOG_TAIL_LINES: usize = 1500;

pub struct InstancesView {
    instances: Entity<InstancesState>,
    details_tab: DetailsTab,
    log_scroll: ScrollHandle,
    /// Keep the log scrolled to the newest line.
    log_follow: bool,
    /// Text of the log tab, rebuilt only when the log changes.
    log_text: Option<(InstanceId, u64, SharedString)>,
    catalog: Entity<CatalogView>,
    screen: BuildsScreen,
    palette: Palette,
    locale: Locale,
    /// Offline name the game is launched with.
    /// Offline player name; used when no Ely.by account is selected.
    player_name: String,
    auth: Entity<AuthState>,
    _subscriptions: [Subscription; 3],
}

impl EventEmitter<InstancesViewEvent> for InstancesView {}

impl InstancesView {
    pub fn new(
        instances: Entity<InstancesState>,
        catalog: Entity<CatalogView>,
        settings: &Entity<SettingsState>,
        auth: Entity<AuthState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (palette, locale, player_name) = {
            let settings = settings.read(cx);
            (
                settings.palette(),
                settings.locale(),
                settings.offline_player().to_owned(),
            )
        };
        let subscriptions = [
            cx.observe(settings, |this, settings, cx| {
                let settings = settings.read(cx);
                this.palette = settings.palette();
                this.locale = settings.locale();
                this.player_name = settings.offline_player().to_owned();
                cx.notify();
            }),
            cx.observe(&instances, |this, _, cx| {
                this.refresh_log(cx);
                cx.notify();
            }),
            // Installed-mod lists and scan states of a build page.
            cx.observe(&catalog, |_, _, cx| cx.notify()),
        ];
        Self {
            auth,
            instances,
            details_tab: DetailsTab::default(),
            log_scroll: ScrollHandle::new(),
            log_follow: true,
            log_text: None,
            catalog,
            screen: BuildsScreen::Home,
            palette,
            locale,
            player_name,
            _subscriptions: subscriptions,
        }
    }

    pub fn show(&mut self, screen: BuildsScreen, cx: &mut Context<Self>) {
        if self.screen != screen {
            self.screen = screen;
            self.details_tab = DetailsTab::Mods;
            cx.notify();
        }
    }

    pub(crate) fn set_details_tab(&mut self, tab: DetailsTab, cx: &mut Context<Self>) {
        self.details_tab = tab;
        self.log_follow = true;
        self.refresh_log(cx);
        cx.notify();
    }

    pub(crate) fn toggle_log_follow(&mut self, cx: &mut Context<Self>) {
        self.log_follow = !self.log_follow;
        if self.log_follow {
            self.log_scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    /// Rebuilds the log text of the open build when its log changed, and
    /// keeps the view at the newest line while following.
    fn refresh_log(&mut self, cx: &mut Context<Self>) {
        let (BuildsScreen::Details(id), DetailsTab::Log) = (self.screen, self.details_tab) else {
            return;
        };
        let (version, text) = match self.instances.read(cx).game_log(id) {
            Some(log) => (log.version(), log.tail(LOG_TAIL_LINES)),
            None => (0, String::new()),
        };
        if self
            .log_text
            .as_ref()
            .is_some_and(|(cached, cached_version, _)| *cached == id && *cached_version == version)
        {
            return;
        }
        self.log_text = Some((id, version, text.into()));
        if self.log_follow {
            self.log_scroll.scroll_to_bottom();
        }
    }

    /// Copies the whole log of a build to the clipboard.
    pub(crate) fn copy_log(&self, id: InstanceId, cx: &mut Context<Self>) {
        if let Some(log) = self.instances.read(cx).game_log(id) {
            cx.write_to_clipboard(ClipboardItem::new_string(log.text()));
        }
    }

    /// Opens the folder where Minecraft writes its own log files.
    pub(crate) fn open_log_directory(&self, id: InstanceId, cx: &App) {
        let Some(profile) = self.instances.read(cx).profile(id) else {
            return;
        };
        let path = profile.game_directory().join("logs");
        if let Err(error) = crate::platform::instance_layout::ensure_directory(&path)
            .and_then(|()| crate::platform::process::open_directory(&path))
        {
            tracing::error!(profile_id = %id, %error, "failed to open the game log folder");
        }
    }

    pub(crate) fn launch_into(
        &mut self,
        id: InstanceId,
        world: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let player = self.auth.read(cx).player_account(&self.player_name);
        self.instances
            .update(cx, |instances, cx| instances.launch(id, world, player, cx));
    }

    pub(crate) fn open_profile_directory(&self, id: InstanceId, cx: &App) {
        let Some(profile) = self.instances.read(cx).profile(id) else {
            return;
        };
        let path = profile.game_directory();
        if let Err(error) = crate::platform::instance_layout::ensure_directory(&path)
            .and_then(|()| crate::platform::process::open_directory(&path))
        {
            tracing::error!(profile_id = %id, %error, "failed to open profile directory");
        }
    }

    pub(crate) fn choose_java_for_profile(&mut self, id: InstanceId, cx: &mut Context<Self>) {
        let locale = self.locale;
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

    pub(crate) fn set_profile_java_path(
        &mut self,
        id: InstanceId,
        java_path: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.instances.update(cx, |instances, _| {
            instances.set_java_path(id, java_path);
            instances.save();
        });
        cx.notify();
    }
}

impl Render for InstancesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (palette, locale) = (self.palette, self.locale);
        match self.screen {
            BuildsScreen::Home => self.home_page(palette, locale, cx),
            BuildsScreen::List => self.instances_page(palette, locale, cx),
            BuildsScreen::Details(id) => self.instance_details_page(id, palette, locale, cx),
        }
    }
}
