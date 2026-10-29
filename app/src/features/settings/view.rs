//! Launcher settings page: language, theme, accent, downloads folder, tray
//! and Discord toggles. Every change is saved right away.

use gpui_kit::*;

use super::{AccentColor, Locale, SettingsState};
use crate::features::updates::Updater;
use crate::shared::color_picker::{ColorPicked, ColorPicker};

pub struct SettingsView {
    pub(super) settings: Entity<SettingsState>,
    /// The tray toggle is offered only where the tray icon could be created.
    pub(super) tray_available: bool,
    /// Whether the custom accent picker is unfolded.
    pub(super) palette_open: bool,
    pub(super) color_picker: Entity<ColorPicker>,
    pub(super) updater: Entity<Updater>,
    _subscriptions: [Subscription; 3],
}

impl SettingsView {
    pub fn new(
        settings: Entity<SettingsState>,
        updater: Entity<Updater>,
        tray_available: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let current = settings.read(cx);
        let accent = current.accent().color(current.theme());
        let color_picker = cx.new(|_| ColorPicker::new(accent));
        let subscriptions = [
            cx.observe(&settings, |_, _, cx| cx.notify()),
            cx.observe(&updater, |_, _, cx| cx.notify()),
            // The accent follows the drag; it is saved once the mouse is released.
            cx.subscribe(&color_picker, |this, _, picked: &ColorPicked, cx| {
                this.settings.update(cx, |settings, cx| {
                    settings.set_accent(AccentColor::Custom(picked.color));
                    if picked.committed {
                        settings.save();
                    }
                    cx.notify();
                });
            }),
        ];
        Self {
            settings,
            tray_available,
            palette_open: false,
            color_picker,
            updater,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn set_locale(&mut self, locale: Locale, cx: &mut Context<Self>) {
        self.settings.update(cx, |settings, cx| {
            settings.set_locale(locale);
            settings.save();
            cx.notify();
        });
    }

    pub(super) fn set_discord_presence(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.settings.update(cx, |settings, cx| {
            settings.set_discord_presence(enabled);
            cx.notify();
        });
        self.settings.read(cx).save();
    }

    pub(super) fn choose_download_directory(&mut self, cx: &mut Context<Self>) {
        let locale = self.settings.read(cx).locale();
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
                this.settings.update(cx, |settings, cx| {
                    settings.set_download_directory(path.clone());
                    cx.notify();
                });
                this.settings.read(cx).save();
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.settings_page(cx)
    }
}
