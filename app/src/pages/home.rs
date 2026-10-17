use crate::app::LauncherApp;
use crate::app::theme::Palette;
use crate::app::ui::*;
use crate::features::mods::ModSample;
use crate::features::settings::*;
use crate::router::Page;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl LauncherApp {
    pub(crate) fn home_page(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .pr_4()
            .overflow_y_scrollbar()
            .gap_4()
            .when(self.instances.profiles().is_empty(), |this| {
                this.child(
                    div().v_flex().items_start().gap_3().p_6()
                        .rounded(px(12.)).bg(rgb(palette.surface))
                        .border_1().border_color(rgb(palette.border))
                        .child(div().text_size(px(20.)).font_weight(FontWeight::BOLD)
                            .child(locale.text("Создай первую сборку", "Create your first build")))
                        .child(div().text_size(px(13.)).text_color(rgb(palette.muted))
                            .child(locale.text("Выбери версию Minecraft. Здесь появятся твои сборки и кнопки запуска.", "Choose a Minecraft version. Your builds and launch controls will appear here.")))
                        .child(UiButton::new("home-create-build")
                            .px_4().py_3().rounded(px(8.))
                            .bg(rgb(palette.accent)).text_color(rgb(palette.accent_foreground))
                            .hover(|style| style.opacity(0.9))
                            .on_click(cx.listener(|this, _, window, cx| this.open_version_picker(window, cx)))
                            .child(locale.text("Создать сборку", "Create build"))),
                )
            })
            .when(!self.instances.profiles().is_empty(), |this| {
                this.child(self.instances_section(palette, locale, cx))
            })
            .into_any_element()
    }
    pub(crate) fn download_indicator(
        &self,
        summary: crate::features::downloads::DownloadSummary,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let title = format!(
            "{} · {}",
            locale.text("Загрузки и установка", "Downloads and installation"),
            summary.active_count
        );
        let detail = if summary.active_count == 1 {
            let mut detail = summary.profile_name;
            if let Some(total) = summary.total_bytes
                && (summary.received > 0 || total > 0)
            {
                detail.push_str(&format!(
                    " · {} / {}",
                    format_bytes(summary.received),
                    format_bytes(total)
                ));
            }
            if summary.speed_bps > 0. {
                detail.push_str(&format!(" · {}", format_speed(summary.speed_bps)));
            }
            detail
        } else {
            locale
                .text(
                    "Открыть сборки и посмотреть прогресс",
                    "Open builds to see progress",
                )
                .to_owned()
        };
        UiButton::new("global-downloads")
            .w_full()
            .flex_shrink_0()
            .px_4()
            .py_3()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .text_color(rgb(palette.foreground))
            .hover(|style| style.bg(rgb(palette.hover)))
            .accessibility_label(format!("{title}. {detail}"))
            .on_click(
                cx.listener(|this, _, window, cx| this.navigate_to(Page::Instances, window, cx)),
            )
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_color(rgb(palette.accent))
                            .child(IconName::Download),
                    )
                    .child(
                        div()
                            .v_flex()
                            .flex_1()
                            .min_w_0()
                            .items_start()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .text_size(px(11.))
                                    .text_color(rgb(palette.muted))
                                    .child(detail),
                            ),
                    )
                    .child(IconName::ChevronRight),
            )
            .into_any_element()
    }
}
impl LauncherApp {
    pub(crate) fn home_mod_card(
        &self,
        sample: &ModSample,
        palette: Palette,
        locale: Locale,
    ) -> AnyElement {
        div()
            .v_flex()
            .flex_1()
            .min_w_0()
            .gap_3()
            .p_3()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .child(mod_thumbnail(sample, px(34.)))
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(palette.foreground))
                            .child(sample.name),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.muted))
                            .child(locale.text("от ", "by "))
                            .child(sample.author),
                    ),
            )
            .into_any_element()
    }
}
