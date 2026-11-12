use super::widgets::{build_icon, format_ago};
use super::{InstancesView, InstancesViewEvent};
use crate::features::settings::*;
use crate::features::worlds::RecentWorld;
use crate::shared::theme::Palette;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl InstancesView {
    pub(crate) fn home_page(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let has_builds = !self.instances.read(cx).profiles().is_empty();
        let worlds = self
            .recent_worlds
            .iter()
            .filter_map(|world| self.recent_world_row(world, palette, locale, cx))
            .collect::<Vec<_>>();
        div()
            .v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .pr_4()
            .overflow_y_scrollbar()
            .gap_4()
            .when(!has_builds, |this| {
                this.child(
                    div().v_flex().items_start().gap_3().p_6()
                        .rounded(px(12.)).bg(rgb(palette.surface))
                        .border_1().border_color(rgb(palette.border))
                        .child(div().text_size(px(20.)).font_weight(FontWeight::BOLD)
                            .child(locale.text("Создай первую сборку", "Create your first build")))
                        .child(div().text_size(px(13.)).text_color(rgb(palette.muted))
                            .child(locale.text("Выбери версию Minecraft. Здесь появятся твои сборки и кнопки запуска.", "Choose a Minecraft version. Your builds and launch controls will appear here.")))
                        .child(UiButton::new("home-create-build").cursor_pointer()
                            .px_4().py_3().rounded(px(8.))
                            .bg(rgb(palette.accent)).text_color(rgb(palette.accent_foreground))
                            .hover(|style| style.opacity(0.9))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(InstancesViewEvent::CreateBuild)))
                            .child(locale.text("Создать сборку", "Create build"))),
                )
            })
            .when(!worlds.is_empty(), |this| {
                this.child(section_title(
                    locale.text("Продолжить игру", "Continue playing"),
                    palette,
                ))
                .child(div().v_flex().w_full().gap_2().children(worlds))
            })
            .when(has_builds, |this| {
                this.child(
                    div()
                        .h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .child(section_title(locale.text("Сборки", "Builds"), palette))
                        .child(self.builds_layout_toggle(palette, locale, cx)),
                )
                    .child(self.instances_section(palette, locale, cx))
            })
            .into_any_element()
    }

    /// One last played world: its picture, where it lives and a button that
    /// starts the game right in it.
    fn recent_world_row(
        &self,
        world: &RecentWorld,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let instances = self.instances.read(cx);
        let profile = instances.profile(world.profile)?;
        let is_busy = instances.runtime(&profile.id).is_busy();
        let profile_id = profile.id;
        let folder = world.folder.clone();
        let ago = world
            .last_played
            .elapsed()
            .map(|elapsed| format_ago(elapsed, locale))
            .unwrap_or_else(|_| locale.text("только что", "just now").to_owned());
        let size = px(44.);
        let radius = size * 0.22;
        let icon = match &world.icon {
            Some(path) => {
                let fallback_profile = profile.clone();
                div()
                    .flex_none()
                    .size(size)
                    .rounded(radius)
                    .overflow_hidden()
                    .child(
                        img(path.clone())
                            .size_full()
                            .rounded(radius)
                            .object_fit(ObjectFit::Cover)
                            .with_fallback(move || build_icon(&fallback_profile, size)),
                    )
                    .into_any_element()
            }
            None => build_icon(profile, size),
        };

        Some(
            div()
                .h_flex()
                .w_full()
                .items_center()
                .gap_3()
                .p_3()
                .rounded(px(14.))
                .bg(rgb(palette.surface))
                .border_1()
                .border_color(rgb(palette.border))
                .child(icon)
                .child(
                    div()
                        .v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(palette.foreground))
                                .truncate()
                                .child(world.folder.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(palette.muted))
                                .truncate()
                                .child(format!("{} · {ago}", profile.name)),
                        ),
                )
                .child(
                    UiButton::new(format!("recent-world-{profile_id}-{}", world.folder))
                        .cursor_pointer()
                        .flex_none()
                        .h(px(40.))
                        .px_5()
                        .rounded(px(10.))
                        .bg(rgb(palette.accent))
                        .text_color(rgb(palette.accent_foreground))
                        .hover(|style| style.opacity(0.9))
                        .disabled(is_busy)
                        .accessibility_label(locale.text("Играть в этом мире", "Play this world"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.launch_into(profile_id, Some(folder.clone()), cx);
                        }))
                        .child(
                            div()
                                .h_full()
                                .h_flex()
                                .items_center()
                                .gap_2()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(
                                    gpui_kit::component::Icon::empty().path("icons/play-fill.svg"),
                                )
                                .child(locale.text("Играть", "Play")),
                        ),
                )
                .into_any_element(),
        )
    }
}

fn section_title(title: &'static str, palette: Palette) -> impl IntoElement {
    div()
        .text_size(px(18.))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(palette.foreground))
        .child(title)
}
