#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod features;
mod platform;
mod router;
mod shared;
mod shell;

use std::borrow::Cow;
use std::sync::Arc;

use app::LauncherApp;
use features::settings::Locale;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::http_client::HttpClient;
use gpui_kit::*;
use gpui_kit::{AssetSource, SharedString};
use shell::http_client::ReqwestHttpClient;

gpui_kit::assets::icon_assets!(
    ExtraLucideIcons,
    [
        Blocks,
        Box,
        Download,
        House,
        Info,
        Languages,
        LoaderCircle,
        Paintbrush,
        PanelTopClose,
        Play,
        Puzzle,
    ]
);

struct LauncherAssets;

impl AssetSource for LauncherAssets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        if let Some(icon) = ExtraLucideIcons.load(path)? {
            return Ok(Some(icon));
        }

        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        let mut assets = gpui_kit::assets::Assets.list(path)?;
        assets.extend(ExtraLucideIcons.list(path)?);
        Ok(assets)
    }
}

fn main() -> Result<(), reqwest::Error> {
    // A second start only brings the running launcher forward.
    platform::logging::init();
    let Some(_instance) = platform::single_instance::acquire() else {
        platform::single_instance::activate_running();
        return Ok(());
    };
    platform::self_update::remove_leftovers();
    let http_client = Arc::new(ReqwestHttpClient::new()?);

    gpui_kit::application()
        .with_http_client(http_client as Arc<dyn HttpClient>)
        .with_assets(LauncherAssets)
        .run(|cx| {
            gpui_kit::init(cx);
            gpui_kit::component::set_locale(Locale::Ru.tag());
            Theme::change(ThemeMode::Dark, None, cx);

            #[cfg(all(feature = "devtools", debug_assertions))]
            {
                gpui_devtools::init_with(gpui_devtools::Config::default().key_binding(None), cx);
                shell::devtools::initialize(cx);
            }

            let window_options = WindowOptions {
                window_min_size: Some(size(px(900.), px(620.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("OrbMC".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };

            gpui_kit::open_window(window_options, cx, |window, cx| {
                cx.new(|cx| LauncherApp::new(window, cx))
            })
            .expect("Failed to open launcher window");
        });

    Ok(())
}
