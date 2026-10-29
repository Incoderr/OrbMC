use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::instances::{OFFLINE_PLAYER_NAME, is_valid_player_name};
use crate::platform::profile_store;
pub use crate::shared::locale::Locale;
use crate::shared::theme::Palette;

mod page;
mod view;

pub use crate::shared::theme::{AccentColor, ThemeChoice};
pub use view::SettingsView;

/// Reads a saved custom accent such as `custom:#3a7bd5`.
fn parse_custom_accent(value: &str) -> Option<AccentColor> {
    let hex = value.strip_prefix("custom:#")?;
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
        .map(AccentColor::Custom)
}

/// Persisted form of the launcher settings (`settings.json`).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct SavedLauncherSettings {
    pub download_directory: PathBuf,
    pub locale: String,
    pub theme: String,
    pub accent: String,
    pub hide_to_tray: bool,
    pub offline_player: String,
    pub discord_presence: bool,
    /// Whether catalog descriptions are machine-translated (Google Translate).
    pub translate_descriptions: bool,
    /// Whether launcher updates are downloaded and installed by themselves.
    pub auto_update: bool,
    /// Whether the navigation sidebar was left collapsed.
    pub sidebar_collapsed: bool,
}

impl Default for SavedLauncherSettings {
    fn default() -> Self {
        Self {
            download_directory: PathBuf::new(),
            locale: "ru".to_owned(),
            theme: "dark".to_owned(),
            accent: "orange".to_owned(),
            hide_to_tray: false,
            offline_player: OFFLINE_PLAYER_NAME.to_owned(),
            discord_presence: true,
            translate_descriptions: true,
            auto_update: true,
            sidebar_collapsed: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct SettingsState {
    locale: Locale,
    theme: ThemeChoice,
    accent: AccentColor,
    hide_to_tray: bool,
    download_directory: PathBuf,
    offline_player: String,
    /// Whether game launches update the Discord status.
    discord_presence: bool,
    translate_descriptions: bool,
    auto_update: bool,
    sidebar_collapsed: bool,
}

impl SettingsState {
    /// Reads `settings.json`; missing or broken values fall back to defaults.
    pub fn load() -> Self {
        let mut saved =
            profile_store::load_settings::<SavedLauncherSettings>().unwrap_or_else(|error| {
                tracing::warn!(%error, "failed to load launcher settings");
                SavedLauncherSettings::default()
            });
        if saved.download_directory.as_os_str().is_empty() {
            match profile_store::default_download_directory() {
                Ok(path) => saved.download_directory = path,
                Err(error) => {
                    tracing::warn!(%error, "launcher default download directory is unavailable")
                }
            }
        }
        if let Some(relocated) = profile_store::relocate_legacy_path(&saved.download_directory) {
            saved.download_directory = relocated;
        }

        let mut settings = Self {
            download_directory: saved.download_directory,
            locale: match saved.locale.as_str() {
                "en" => Locale::En,
                _ => Locale::Ru,
            },
            theme: match saved.theme.as_str() {
                "light" => ThemeChoice::Light,
                _ => ThemeChoice::Dark,
            },
            accent: match saved.accent.as_str() {
                "red" => AccentColor::Red,
                "blue" => AccentColor::Blue,
                "green" => AccentColor::Green,
                "purple" => AccentColor::Purple,
                "pink" => AccentColor::Pink,
                custom => parse_custom_accent(custom).unwrap_or_default(),
            },
            hide_to_tray: saved.hide_to_tray,
            discord_presence: saved.discord_presence,
            translate_descriptions: saved.translate_descriptions,
            auto_update: saved.auto_update,
            sidebar_collapsed: saved.sidebar_collapsed,
            offline_player: String::new(),
        };
        if is_valid_player_name(&saved.offline_player) {
            settings.set_offline_player(saved.offline_player);
        } else {
            settings.reset_offline_player();
        }
        settings
    }

    /// Writes `settings.json`; failures are logged, the state stays.
    pub fn save(&self) {
        let accent = match self.accent {
            AccentColor::Orange => "orange".to_owned(),
            AccentColor::Red => "red".to_owned(),
            AccentColor::Blue => "blue".to_owned(),
            AccentColor::Green => "green".to_owned(),
            AccentColor::Purple => "purple".to_owned(),
            AccentColor::Pink => "pink".to_owned(),
            AccentColor::Custom(color) => format!("custom:#{color:06x}"),
        };
        let saved = SavedLauncherSettings {
            download_directory: self.download_directory.clone(),
            locale: self.locale.tag().to_owned(),
            theme: match self.theme {
                ThemeChoice::Dark => "dark",
                ThemeChoice::Light => "light",
            }
            .to_owned(),
            accent,
            hide_to_tray: self.hide_to_tray,
            offline_player: self.offline_player.clone(),
            discord_presence: self.discord_presence,
            translate_descriptions: self.translate_descriptions,
            auto_update: self.auto_update,
            sidebar_collapsed: self.sidebar_collapsed,
        };
        if let Err(error) = profile_store::save_settings(&saved) {
            tracing::error!(%error, "failed to persist launcher settings");
        }
    }

    /// Colours of the chosen theme and accent.
    pub fn palette(&self) -> Palette {
        Palette::for_settings(self.theme, self.accent)
    }

    pub fn sidebar_collapsed(&self) -> bool {
        self.sidebar_collapsed
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn theme(&self) -> ThemeChoice {
        self.theme
    }

    pub fn accent(&self) -> AccentColor {
        self.accent
    }

    pub fn hide_to_tray(&self) -> bool {
        self.hide_to_tray
    }

    pub fn download_directory(&self) -> &PathBuf {
        &self.download_directory
    }

    /// Player name used for offline launches (launcher has no account yet).
    pub fn offline_player(&self) -> &str {
        &self.offline_player
    }

    /// Whether game launches update the Discord status.
    pub fn discord_presence(&self) -> bool {
        self.discord_presence
    }

    /// Whether catalog descriptions are machine-translated.
    pub fn translate_descriptions(&self) -> bool {
        self.translate_descriptions
    }

    pub fn set_translate_descriptions(&mut self, enabled: bool) {
        self.translate_descriptions = enabled;
    }

    pub fn auto_update(&self) -> bool {
        self.auto_update
    }

    pub fn set_auto_update(&mut self, enabled: bool) {
        self.auto_update = enabled;
    }

    pub fn set_discord_presence(&mut self, enabled: bool) {
        self.discord_presence = enabled;
    }

    pub fn set_locale(&mut self, locale: Locale) {
        self.locale = locale;
    }

    pub fn set_theme(&mut self, theme: ThemeChoice) {
        self.theme = theme;
    }

    pub fn set_accent(&mut self, accent: AccentColor) {
        self.accent = accent;
    }

    pub fn set_hide_to_tray(&mut self, hide_to_tray: bool) {
        self.hide_to_tray = hide_to_tray;
    }

    pub fn set_download_directory(&mut self, path: PathBuf) {
        self.download_directory = path;
    }

    /// Stores a validated offline player name; invalid names keep the default.
    pub fn set_offline_player(&mut self, name: String) {
        if is_valid_player_name(&name) {
            self.offline_player = name.trim().to_owned();
        }
    }

    /// Fallback used when the saved name is missing or invalid.
    pub fn reset_offline_player(&mut self) {
        self.offline_player = OFFLINE_PLAYER_NAME.to_owned();
    }
}
