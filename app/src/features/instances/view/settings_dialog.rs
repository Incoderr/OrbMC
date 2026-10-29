use super::InstancesView;
use crate::features::settings::*;
use crate::shared::ui::field_label;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::WindowExt;
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::*;

impl InstancesView {
    pub(crate) fn open_profile_settings(
        &mut self,
        id: crate::features::instances::InstanceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.instances.read(cx).profile(id).cloned() else {
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
        let locale = self.locale;
        let palette = self.palette;
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
                        .and_then(|owner| owner.read(cx).instances.read(cx).profile(id).cloned());
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
                                                    .cursor_pointer()
                                                    .px_3()
                                                    .py_2()
                                                    .rounded(px(7.))
                                                    .bg(rgb(palette.control))
                                                    .text_color(rgb(palette.foreground))
                                                    .hover(|style| style.bg(rgb(palette.hover)))
                                                    .on_click(move |_, _, cx| {
                                                        if let Some(owner) = choose_owner.upgrade()
                                                        {
                                                            owner.update(cx, |this, cx| {
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
                                                    .cursor_pointer()
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
                                                            owner.update(cx, |this, cx| {
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
                                    .cursor_pointer()
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
                                            owner.update(cx, |this, cx| {
                                                this.instances.update(cx, |instances, _| {
                                                    instances.set_memory_mb(id, memory_mb)
                                                });
                                                this.instances.read(cx).save();
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

    pub(crate) fn open_delete_profile_confirmation(
        &mut self,
        id: crate::features::instances::InstanceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.instances.read(cx).profile(id).cloned() else {
            return;
        };
        let profile_name = profile.name.clone();
        let locale = self.locale;
        let palette = self.palette;
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
                )};

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
                                        UiButton::new(format!("cancel-delete-profile-{id}")).cursor_pointer()
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
                                        UiButton::new(format!("confirm-delete-profile-{id}")).cursor_pointer()
                                            .px_3()
                                            .py_2()
                                            .rounded(px(8.))
                                            .bg(rgb(0xc0392b))
                                            .text_color(rgb(0xffffff))
                                            .hover(|style| style.opacity(0.9))
                                            .on_click(move |_, window, cx| {
                                                if let Some(owner) = action_owner.upgrade() {
                                                    owner.update(cx, |this, cx| {
                                                        this.instances.update(cx, |instances, cx| {
                                                            instances.start_removal(id, cx)
                                                        });
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
}
