use crate::features::mods::ProjectKind;
use crate::platform::profile_store;
use crate::{
    features::instances::InstancesState,
    features::settings::{Locale, SettingsState, ThemeChoice},
    router::Page,
};
use gpui_kit::base::StyledExt;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

#[path = "dialogs/mod.rs"]
mod dialogs;
#[path = "pages/mod.rs"]
mod pages;
#[path = "sidebar.rs"]
mod sidebar;

const SIDEBAR_WIDTH: f32 = 252.;
const SIDEBAR_COLLAPSED_WIDTH: f32 = 76.;
const SIDEBAR_ANIMATION: std::time::Duration = std::time::Duration::from_millis(220);

pub struct LauncherApp {
    active_page: Page,
    settings: Entity<SettingsState>,
    _settings_observer: Subscription,
    skins: Entity<crate::features::skins::SkinsView>,
    catalog: Entity<crate::features::mods::CatalogView>,
    builds: Entity<crate::features::instances::InstancesView>,
    _builds_subscription: Subscription,
    _catalog_subscription: Subscription,
    version_picker: Entity<crate::features::versions::VersionPicker>,
    _version_picker_subscriptions: [Subscription; 1],
    instances: Entity<InstancesState>,
    _instances_observer: Subscription,
    _presence: Entity<crate::features::rich_presence::DiscordPresence>,
    account: Entity<crate::features::auth::AccountView>,
    settings_view: Entity<crate::features::settings::SettingsView>,
    updater: Entity<crate::features::updates::Updater>,
    _updater_observer: Subscription,
    /// Counts sidebar toggles in this run: a new value gives the sidebar a new
    /// element id, which replays its width animation (none on startup).
    sidebar_toggles: u32,
}
impl LauncherApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = SettingsState::load();
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
        let download_directory = settings.download_directory().clone();
        let settings = cx.new(|_| settings);
        let settings_observer = cx.observe(&settings, |_, settings, cx| {
            let locale = settings.read(cx).locale();
            gpui_kit::component::set_locale(locale.tag());
            cx.notify();
        });

        let instances = match profile_store::load::<crate::features::instances::InstanceProfile>() {
            Ok(mut profiles) => {
                let mut relocated = false;
                for profile in &mut profiles {
                    if profile.installation_root.as_os_str().is_empty() {
                        profile.installation_root = download_directory.clone();
                    }
                    if let Some(root) =
                        profile_store::relocate_legacy_path(&profile.installation_root)
                    {
                        profile.installation_root = root;
                        relocated = true;
                    }
                    let (min, max) = orbmc_core::minecraft::launch::MEMORY_LIMITS_MB;
                    profile.memory_mb = profile.memory_mb.clamp(min, max);
                }
                if (crate::features::instances::migrate_layout(&mut profiles) || relocated)
                    && let Err(error) = profile_store::save(&profiles)
                {
                    tracing::error!(%error, "failed to save migrated Minecraft profiles");
                }
                InstancesState::from_profiles(profiles)
            }
            Err(error) => {
                tracing::warn!(%error, "failed to load saved Minecraft profiles");
                InstancesState::default()
            }
        };

        let tray_available = crate::shell::tray::install(window, cx.weak_entity(), cx);
        let auth = cx.new(|_| crate::features::auth::AuthState::load());
        let skins =
            cx.new(|cx| crate::features::skins::SkinsView::new(&settings, auth.clone(), cx));

        let version_picker =
            cx.new(|cx| crate::features::versions::VersionPicker::new(&settings, window, cx));
        let version_picker_subscriptions = [cx.subscribe(
            &version_picker,
            |this, _, request: &crate::features::versions::CreateProfile, cx| {
                this.create_profile(request, cx);
            },
        )];

        let instances = cx.new(|_| instances);

        let catalog = cx.new(|cx| {
            crate::features::mods::CatalogView::new(
                instances.clone(),
                version_picker.clone(),
                &settings,
                window,
                cx,
            )
        });
        let catalog_subscription = cx.subscribe_in(
            &catalog,
            window,
            |this, _, event: &crate::features::mods::CatalogEvent, window, cx| {
                use crate::features::mods::CatalogEvent;
                let page = match *event {
                    CatalogEvent::OpenDetails => Page::ModDetails,
                    CatalogEvent::BackToCatalog => Page::Catalog,
                    CatalogEvent::OpenBuilds => Page::Instances,
                };
                this.navigate_to(page, window, cx);
            },
        );

        let builds = cx.new(|cx| {
            crate::features::instances::InstancesView::new(
                instances.clone(),
                catalog.clone(),
                &settings,
                auth.clone(),
                cx,
            )
        });
        let builds_subscription = cx.subscribe_in(
            &builds,
            window,
            |this, _, event: &crate::features::instances::InstancesViewEvent, window, cx| {
                use crate::features::instances::InstancesViewEvent;
                match *event {
                    InstancesViewEvent::CreateBuild => this.open_version_picker(window, cx),
                    InstancesViewEvent::OpenCatalog => {
                        this.catalog
                            .update(cx, |catalog, _| catalog.set_kind(ProjectKind::Mod));
                        this.navigate_to(Page::Catalog, window, cx)
                    }
                    InstancesViewEvent::OpenBuild(id) => {
                        this.navigate_to(Page::InstanceDetails(id), window, cx)
                    }
                    InstancesViewEvent::OpenBuilds => this.navigate_to(Page::Instances, window, cx),
                }
            },
        );

        let presence = cx.new(|cx| {
            crate::features::rich_presence::DiscordPresence::new(instances.clone(), &settings, cx)
        });
        // Builds change from background work and from feature views alike.
        let instances_observer = cx.observe(&instances, |_, _, cx| cx.notify());

        let account = cx.new(|cx| {
            crate::features::auth::AccountView::new(
                settings.clone(),
                auth.clone(),
                skins.clone(),
                window,
                cx,
            )
        });
        let updater = cx.new(|cx| {
            crate::features::updates::Updater::new(settings.clone(), instances.clone(), cx)
        });
        let updater_observer = cx.observe(&updater, |_, _, cx| cx.notify());
        let settings_view = cx.new(|cx| {
            crate::features::settings::SettingsView::new(
                settings.clone(),
                updater.clone(),
                tray_available,
                cx,
            )
        });

        Self {
            settings_view,
            active_page: Page::Home,
            settings,
            _settings_observer: settings_observer,
            skins,
            catalog,
            builds,
            _builds_subscription: builds_subscription,
            _catalog_subscription: catalog_subscription,
            version_picker,
            _version_picker_subscriptions: version_picker_subscriptions,
            instances,
            _instances_observer: instances_observer,
            _presence: presence,
            account,
            updater,
            _updater_observer: updater_observer,
            sidebar_toggles: 0,
        }
    }

    pub(crate) fn hide_to_tray(&self, cx: &App) -> bool {
        self.settings.read(cx).hide_to_tray()
    }

    pub(crate) fn tray_uses_english(&self, cx: &App) -> bool {
        self.settings.read(cx).locale() == Locale::En
    }

    fn navigate_to(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_page == page {
            return;
        }

        use crate::features::mods::CatalogTarget;
        let keep_query = page == Page::ModDetails || self.active_page == Page::ModDetails;
        self.active_page = page;
        let target = match page {
            Page::ModDetails => CatalogTarget::Details,
            Page::InstanceDetails(id) => CatalogTarget::Build(id),
            Page::Catalog => CatalogTarget::Catalog,
            _ => CatalogTarget::Elsewhere,
        };
        self.catalog.update(cx, |catalog, cx| {
            catalog.navigated(target, keep_query, window, cx)
        });
        cx.notify();
    }
}

impl Render for LauncherApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = self.settings.read(cx).locale();
        let palette = self.settings.read(cx).palette();
        let active_page = self.active_page;

        let navigation_items: Vec<_> = Page::LIBRARY
            .into_iter()
            .map(|page| self.navigation_button(page, palette, locale, cx))
            .collect();
        let settings_button = self.navigation_button(Page::Settings, palette, locale, cx);
        let collapse_button = self.sidebar_toggle_button(palette, locale, cx);
        let navigation = div().v_flex().gap_2().children(navigation_items);
        let page_body = match active_page {
            Page::Home | Page::Instances | Page::InstanceDetails(_) => {
                use crate::features::instances::BuildsScreen;
                let screen = match active_page {
                    Page::InstanceDetails(id) => BuildsScreen::Details(id),
                    Page::Instances => BuildsScreen::List,
                    _ => BuildsScreen::Home,
                };
                self.builds.update(cx, |builds, cx| builds.show(screen, cx));
                self.builds.clone().into_any_element()
            }
            Page::Catalog | Page::ModDetails => self.catalog.clone().into_any_element(),
            Page::Settings => self.settings_view.clone().into_any_element(),
            Page::Skins => self.skins.clone().into_any_element(),
        };
        let downloads =
            crate::features::downloads::DownloadSummary::from_instances(self.instances.read(cx))
                .map(|summary| {
                    let collapsed = self.settings.read(cx).sidebar_collapsed();
                    self.download_indicator(summary, collapsed, palette, locale, cx)
                });

        let update = {
            let collapsed = self.settings.read(cx).sidebar_collapsed();
            self.update_indicator(collapsed, palette, locale, cx)
        };

        let sidebar_collapsed = self.settings.read(cx).sidebar_collapsed();
        let (from_width, to_width) = if sidebar_collapsed {
            (SIDEBAR_WIDTH, SIDEBAR_COLLAPSED_WIDTH)
        } else {
            (SIDEBAR_COLLAPSED_WIDTH, SIDEBAR_WIDTH)
        };
        let sidebar = div()
            .id(("sidebar", self.sidebar_toggles))
            .v_flex()
            .flex_none()
            .w(px(to_width))
            .overflow_hidden()
            .h_full()
            .px_3()
            .py_5()
            .gap_6()
            .bg(rgb(palette.sidebar))
            .border_r_1()
            .border_color(rgb(palette.border))
            .child(collapse_button)
            .child(navigation)
            .child(div().flex_1())
            .children(update)
            .children(downloads)
            .child(div().h(px(1.)).w_full().bg(rgb(palette.border)))
            .child(self.account.clone())
            .child(settings_button);
        let sidebar = if self.sidebar_toggles == 0 {
            sidebar.into_any_element()
        } else {
            sidebar
                .with_animation(
                    ("sidebar-width", self.sidebar_toggles),
                    Animation::new(SIDEBAR_ANIMATION).with_easing(ease_out_quint()),
                    move |sidebar, delta| {
                        sidebar.w(px(from_width + (to_width - from_width) * delta))
                    },
                )
                .into_any_element()
        };

        div()
            .flex()
            .size_full()
            .bg(rgb(palette.background))
            .child(sidebar)
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
                    .child(page_body),
            )
    }
}
