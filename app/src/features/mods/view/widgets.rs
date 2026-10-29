//! Visual pieces of catalog cards and project pages.

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt;
use gpui_kit::*;

use crate::features::mods::ModSample;
use crate::features::mods::catalog::ModProject;
use crate::shared::theme::Palette;

pub(crate) fn mod_thumbnail(sample: &ModSample, size: Pixels) -> AnyElement {
    mod_thumbnail_data(
        sample.icon,
        sample.icon_background,
        sample.icon_foreground,
        sample.image_url,
        size,
    )
}

pub(crate) fn mod_project_thumbnail(project: &ModProject, size: Pixels) -> AnyElement {
    mod_thumbnail_data(
        project.icon,
        project.icon_background,
        project.icon_foreground,
        project.image_url.as_deref(),
        size,
    )
}

pub(crate) fn mod_thumbnail_data(
    icon: IconName,
    background: u32,
    foreground: u32,
    image_url: Option<&str>,
    size: Pixels,
) -> AnyElement {
    if let Some(url) = image_url {
        div()
            .w(size)
            .h(size)
            .rounded(px(12.))
            .overflow_hidden()
            .bg(rgb(background))
            .child(
                img(url)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .rounded(px(12.))
                    .with_loading(move || mod_icon_placeholder(icon, background, foreground, size))
                    .with_fallback(move || {
                        mod_icon_placeholder(icon, background, foreground, size)
                    }),
            )
            .into_any_element()
    } else {
        mod_icon_placeholder(icon, background, foreground, size)
    }
}

pub(crate) fn mod_icon_placeholder(
    icon: IconName,
    background: u32,
    foreground: u32,
    size: Pixels,
) -> AnyElement {
    div()
        .w(size)
        .h(size)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(12.))
        .bg(rgb(background))
        .text_color(rgb(foreground))
        .text_size(px((size.as_f32() * 0.5).max(14.)))
        .child(icon)
        .into_any_element()
}

pub(crate) fn detail_meta_row(
    icon: IconName,
    label: &'static str,
    value: impl Into<String>,
    palette: Palette,
) -> AnyElement {
    div()
        .h_flex()
        .w_full()
        .min_w_0()
        .items_start()
        .gap_2()
        .text_color(rgb(palette.muted))
        .child(icon)
        .child(
            div()
                .v_flex()
                .flex_1()
                .min_w_0()
                .w_full()
                .gap_1()
                .child(div().text_size(px(10.)).child(label))
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_size(px(11.))
                        .text_color(rgb(palette.foreground))
                        .child(value.into()),
                ),
        )
        .into_any_element()
}
