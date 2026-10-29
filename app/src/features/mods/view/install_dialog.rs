use super::{CatalogEvent, CatalogView};
use crate::features::mods::ProjectKind;
use crate::features::mods::catalog::*;
use crate::features::versions::loader_title;
use crate::features::versions::*;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::WindowExt;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;

impl CatalogView {
    pub(crate) fn open_project_install_dialog(
        &mut self,
        project: ModProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if project.kind == ProjectKind::Mod
            && let Some(id) = self.mods.target_profile()
        {
            self.start_mod_download(project, id, cx);
            return;
        }
        let locale = self.locale;
        let palette = self.palette;
        let is_mod = project.kind == ProjectKind::Mod;
        let profiles = self
            .instances
            .read(cx)
            .profiles()
            .iter()
            .map(|profile| {
                let runtime = self.instances.read(cx).runtime(&profile.id);
                let installed = is_mod && self.mods.installed.contains(profile.id, &project.id);
                let busy = runtime.is_busy()
                    || self.mods.installed.is_installing(&project.id)
                    || (is_mod && self.mods.installed.is_loading(profile.id));
                (profile.clone(), busy, installed)
            })
            .collect::<Vec<_>>();
        let owner = cx.entity().downgrade();
        let action = match project.kind {
            ProjectKind::Mod => locale.text("Скачать мод", "Download mod"),
            ProjectKind::ResourcePack => {
                locale.text("Установить текстурпак", "Install resource pack")
            }
            ProjectKind::Shader => locale.text("Установить шейдер", "Install shader"),
            ProjectKind::Modpack => locale.text("Скачать модпак", "Download modpack"),
        };
        let title = format!("{} · {}", action, project.name);

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
                                    "Сначала создай сборку, куда установить контент.",
                                    "Create a build first, then choose it as the content destination.",
                                ))
                                .child(
                                    UiButton::new("mod-download-create-build").cursor_pointer()
                                        .px_3()
                                        .py_2()
                                        .rounded(px(8.))
                                        .bg(rgb(palette.accent))
                                        .text_color(rgb(palette.accent_foreground))
                                        .on_click(move |_, window, cx| {
                                            window.close_dialog(cx);
                                            if let Some(owner) = create_owner.upgrade() {
                                                owner.update(cx, |_, cx| {
                                                    cx.emit(CatalogEvent::OpenBuilds);
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
                            let has_mod_loader = project.kind != ProjectKind::Mod
                                || profile.loader != LoaderChoice::Vanilla;
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
                            } else if project.kind != ProjectKind::Mod {
                                format!("Minecraft {}", profile.game_version)
                            } else {
                                format!(
                                    "Minecraft {} · {}",
                                    profile.game_version,
                                    loader_title(profile.loader)
                                )
                            };
                            let select_owner = content_owner.clone();
                            let selected_project = project.clone();
                            UiButton::new(format!("download-project-to-{profile_id}")).cursor_pointer()
                                .w_full()
                                .px_3()
                                .py_3()
                                .rounded(px(8.))
                                .bg(rgb(palette.control))
                                .text_color(rgb(palette.foreground))
                                .hover(|style| style.bg(rgb(palette.hover)))
                                .disabled(*busy || *installed || !has_mod_loader)
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    if let Some(owner) = select_owner.upgrade() {
                                        owner.update(cx, |this, cx| {
                                            if selected_project.kind == ProjectKind::Mod {
                                                this.start_mod_download(
                                                    selected_project.clone(),
                                                    profile_id,
                                                    cx,
                                                );
                                            } else {
                                                this.start_content_download(
                                                    selected_project.clone(),
                                                    profile_id,
                                                    cx,
                                                );
                                            }
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
}
