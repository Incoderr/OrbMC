//! Switching a build to another build of its mod loader.

use super::InstancesView;
use crate::features::instances::InstanceId;
use crate::features::versions::{LoaderBuild, fetch_loader_builds, loader_title};
use crate::shared::motion::DialogMotion as _;
use gpui_kit::base::{Disableable as _, StyledExt};
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button as ComponentButton, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// Loader builds listed by the dialog.
#[derive(Clone, Debug, Default)]
pub(crate) enum LoaderBuilds {
    #[default]
    Loading,
    Ready(Vec<LoaderBuild>),
    Failed(String),
}

impl InstancesView {
    /// Opens the list of loader builds for the build's game version; picking
    /// one reinstalls the build with it.
    pub(crate) fn open_loader_version_picker(
        &mut self,
        id: InstanceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = self.locale;
        let Some(profile) = self.instances.read(cx).profile(id).cloned() else {
            return;
        };
        let loader = profile.loader;
        let current = profile.loader_version.clone();
        let game_version = profile.game_version.clone();
        let title = format!(
            "{} {} · Minecraft {game_version}",
            locale.text("Версия", "Version of"),
            loader_title(loader),
        );

        self.loader_builds = LoaderBuilds::Loading;
        let client = cx.http_client();
        let task = cx.background_executor().spawn(fetch_loader_builds(
            client,
            game_version.clone(),
            loader,
        ));
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.loader_builds = match result {
                    Ok(builds) => LoaderBuilds::Ready(builds),
                    Err(error) => {
                        tracing::warn!(profile_id = %id, %error, "loader builds lookup failed");
                        LoaderBuilds::Failed(error)
                    }
                };
                cx.notify();
            });
        })
        .detach();

        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            let current = current.clone();
            let game_version = game_version.clone();
            dialog
                .title(title.clone())
                .animated_content(move |content, _, cx| {
                    let builds = owner
                        .upgrade()
                        .map(|owner| owner.read(cx).loader_builds.clone())
                        .unwrap_or_default();
                    let body = match builds {
                        LoaderBuilds::Ready(builds) if !builds.is_empty() => div()
                            .v_flex()
                            .gap_1()
                            .children(builds.into_iter().map(|build| {
                                let owner = owner.clone();
                                let game_version = game_version.clone();
                                let installed = current.as_deref() == Some(build.version.as_str());
                                let mut label = build.version.clone();
                                if build.stable {
                                    label.push_str(locale.text(" · стабильная", " · stable"));
                                }
                                if installed {
                                    label.push_str(locale.text(" · установлена", " · installed"));
                                }
                                ComponentButton::new(format!("loader-build-{}", build.version))
                                    .cursor_pointer()
                                    .w_full()
                                    .when(installed, |button| button.primary())
                                    .when(!installed, |button| button.ghost())
                                    .disabled(installed)
                                    .label(label)
                                    .on_click(move |_, window, cx| {
                                        window.close_dialog(cx);
                                        let _ = owner.update(cx, |this, cx| {
                                            this.set_loader_version(
                                                id,
                                                game_version.clone(),
                                                build.version.clone(),
                                                cx,
                                            )
                                        });
                                    })
                            }))
                            .into_any_element(),
                        LoaderBuilds::Ready(_) => div()
                            .child(locale.text(
                                "Для этой версии Minecraft сборок лоадера нет.",
                                "The loader has no builds for this Minecraft version.",
                            ))
                            .into_any_element(),
                        LoaderBuilds::Failed(error) => div().child(error).into_any_element(),
                        LoaderBuilds::Loading => div()
                            .child(locale.text("Загружаем список версий…", "Loading versions…"))
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

    /// Records the picked loader build and reinstalls the build with it.
    fn set_loader_version(
        &mut self,
        id: InstanceId,
        game_version: String,
        version: String,
        cx: &mut Context<Self>,
    ) {
        if self.instances.read(cx).runtime(&id).is_busy() {
            return;
        }
        let Some(profile) = self.instances.read(cx).profile(id) else {
            return;
        };
        let loader = profile.loader;
        tracing::info!(
            profile_id = %id,
            from = ?profile.loader_version,
            to = %version,
            "changing build loader version"
        );
        self.instances.update(cx, |instances, cx| {
            instances.set_game(id, game_version, loader, Some(version));
            instances.save();
            instances.start_install(id, cx);
            cx.notify();
        });
    }
}
