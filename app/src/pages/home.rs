use crate::app::LauncherApp;
use crate::features::downloads::DownloadSummary;
use crate::features::instances::view::widgets::{format_bytes, format_speed};
use crate::features::settings::*;
use crate::router::Page;
use crate::shared::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl LauncherApp {
    /// Active downloads and installs, shown in the sidebar above the account.
    /// A click opens the builds page with the details.
    pub(crate) fn download_indicator(
        &self,
        summary: DownloadSummary,
        collapsed: bool,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        let title = locale.text("Загрузки", "Downloads").to_owned();
        let detail = if summary.active_count == 1 {
            let mut detail = summary.profile_name.clone();
            if summary.speed_bps > 0. {
                detail.push_str(&format!(" · {}", format_speed(summary.speed_bps)));
            }
            detail
        } else {
            format!(
                "{} {}",
                summary.active_count,
                locale.text("активных задач", "active tasks")
            )
        };
        let bytes = summary.total_bytes.map(|total| {
            format!(
                "{} / {}",
                format_bytes(summary.received),
                format_bytes(total)
            )
        });
        let fraction = summary
            .total_bytes
            .filter(|total| *total > 0)
            .map(|total| (summary.received as f32 / total as f32).clamp(0., 1.));
        let tooltip = [Some(title.clone()), Some(detail.clone()), bytes.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n");

        let button = UiButton::new("global-downloads")
            .cursor_pointer()
            .w_full()
            .flex_shrink_0()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.border))
            .text_color(rgb(palette.foreground))
            .hover(|style| style.bg(rgb(palette.hover)))
            .accessibility_label(tooltip.clone())
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            .on_click(
                cx.listener(|this, _, window, cx| this.navigate_to(Page::Instances, window, cx)),
            );

        let progress_bar = |fraction: Option<f32>| {
            div()
                .h(px(3.))
                .w_full()
                .rounded(px(2.))
                .bg(rgb(palette.control))
                .overflow_hidden()
                .child(
                    div()
                        .h_full()
                        .w(relative(fraction.unwrap_or(0.)))
                        .bg(rgb(palette.accent)),
                )
        };

        if collapsed {
            return button
                .h(px(44.))
                .v_flex()
                .items_center()
                .justify_center()
                .gap_1()
                .px_2()
                .child(
                    div()
                        .relative()
                        .text_color(rgb(palette.accent))
                        .child(IconName::Download)
                        .when(summary.active_count > 1, |icon| {
                            icon.child(
                                div()
                                    .absolute()
                                    .top(px(-6.))
                                    .right(px(-10.))
                                    .px_1()
                                    .rounded(px(6.))
                                    .bg(rgb(palette.accent))
                                    .text_size(px(9.))
                                    .text_color(rgb(palette.accent_foreground))
                                    .child(summary.active_count.to_string()),
                            )
                        }),
                )
                .when(fraction.is_some(), |button| {
                    button.child(progress_bar(fraction))
                })
                .into_any_element();
        }

        button
            .v_flex()
            .gap_2()
            .px_3()
            .py_2()
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_color(rgb(palette.accent))
                            .child(IconName::Download),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .child(title),
                    )
                    .when_some(bytes, |row, bytes| {
                        row.child(
                            div()
                                .text_size(px(10.))
                                .text_color(rgb(palette.muted))
                                .child(bytes),
                        )
                    }),
            )
            .child(
                div()
                    .w_full()
                    .truncate()
                    .text_size(px(11.))
                    .text_color(rgb(palette.muted))
                    .child(detail),
            )
            .when(fraction.is_some(), |button| {
                button.child(progress_bar(fraction))
            })
            .into_any_element()
    }
}

impl LauncherApp {
    /// A newer launcher in the sidebar. A click does the next step itself:
    /// downloads an available update, restarts into a downloaded one, or opens
    /// the release page when this build cannot install updates. While a
    /// download runs, or a game keeps the launcher from restarting, it opens
    /// the settings with the details.
    pub(crate) fn update_indicator(
        &self,
        collapsed: bool,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        use crate::features::updates::UpdateStatus;

        enum Click {
            Install,
            OpenPage(String),
            Restart,
            Settings,
        }

        let updater = self.updater.read(cx);
        let (label, click) = match updater.status() {
            UpdateStatus::Available(release) if release.installable() => (
                format!(
                    "{} {}",
                    locale.text("Обновить до", "Update to"),
                    release.version
                ),
                Click::Install,
            ),
            UpdateStatus::Available(release) => (
                format!(
                    "{} {}",
                    locale.text("Скачать версию", "Download version"),
                    release.version
                ),
                Click::OpenPage(release.page_url.clone()),
            ),
            UpdateStatus::Downloading { version, transfer } => {
                let progress = transfer.progress();
                let percent = progress
                    .total
                    .filter(|total| *total > 0)
                    .map(|total| progress.received * 100 / total)
                    .unwrap_or(0);
                (
                    format!(
                        "{} {version} · {percent}%",
                        locale.text("Обновление", "Updating to")
                    ),
                    Click::Settings,
                )
            }
            UpdateStatus::Ready { version, .. } if updater.can_restart(cx) => (
                format!(
                    "{} {version}",
                    locale.text("Перезапустить в", "Restart into")
                ),
                Click::Restart,
            ),
            UpdateStatus::Ready { version, .. } => (
                format!(
                    "{} {version} {}",
                    locale.text("Версия", "Version"),
                    locale.text("установится при выходе", "installs when you quit")
                ),
                Click::Settings,
            ),
            _ => return None,
        };

        let button = UiButton::new("sidebar-update")
            .cursor_pointer()
            .w_full()
            .flex_shrink_0()
            .rounded(px(10.))
            .bg(rgb(palette.surface))
            .border_1()
            .border_color(rgb(palette.accent))
            .text_color(rgb(palette.foreground))
            .hover(|style| style.bg(rgb(palette.hover)))
            .accessibility_label(label.clone())
            .tooltip({
                let label = label.clone();
                move |window, cx| Tooltip::new(label.clone()).build(window, cx)
            })
            .on_click(cx.listener(move |this, _, window, cx| match &click {
                Click::Install => this.updater.update(cx, |updater, cx| updater.install(cx)),
                Click::OpenPage(url) => cx.open_url(url),
                Click::Restart => this.updater.update(cx, |updater, cx| updater.restart(cx)),
                Click::Settings => this.navigate_to(Page::Settings, window, cx),
            }));
        let icon = div()
            .text_color(rgb(palette.accent))
            .child(IconName::Download);
        Some(if collapsed {
            button
                .h(px(44.))
                .h_flex()
                .items_center()
                .justify_center()
                .child(icon)
                .into_any_element()
        } else {
            button
                .h_flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .child(icon)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.))
                        .font_weight(FontWeight::BOLD)
                        .child(label),
                )
                .into_any_element()
        })
    }
}
