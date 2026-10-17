use crate::app::theme::Palette;
use crate::features::mods::catalog::*;
use crate::features::mods::*;
use crate::features::settings::*;
use crate::features::versions::*;
use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt;
use gpui_kit::*;

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

pub(crate) fn version_type_label(version_type: &str, locale: Locale) -> &'static str {
    match version_type {
        "release" => locale.text("Релиз", "Release"),
        "snapshot" => locale.text("Снапшот", "Snapshot"),
        "old_beta" => locale.text("Бета", "Beta"),
        "old_alpha" => locale.text("Альфа", "Alpha"),
        _ => locale.text("Другое", "Other"),
    }
}

pub(crate) fn field_label(label: &'static str) -> AnyElement {
    div()
        .text_size(px(13.))
        .font_weight(FontWeight::BOLD)
        .child(label)
        .into_any_element()
}

pub(crate) fn loader_title(loader: LoaderChoice) -> &'static str {
    match loader {
        LoaderChoice::Vanilla => "Vanilla",
        LoaderChoice::Fabric => "Fabric",
        LoaderChoice::NeoForge => "NeoForge",
        LoaderChoice::Forge => "Forge",
    }
}

pub(crate) fn mod_loader_filter(loader: LoaderChoice) -> ModLoaderFilter {
    match loader {
        LoaderChoice::Vanilla => ModLoaderFilter::Vanilla,
        LoaderChoice::Fabric => ModLoaderFilter::Fabric,
        LoaderChoice::Forge => ModLoaderFilter::Forge,
        LoaderChoice::NeoForge => ModLoaderFilter::NeoForge,
    }
}

pub(crate) fn format_bytes(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= 1024 {
        format!("{:.0} KiB", bytes as f64 / 1024.)
    } else {
        format!("{bytes} B")
    }
}

pub(crate) fn format_speed(bytes_per_second: f64) -> String {
    const KIB: f64 = 1024.;
    const MIB: f64 = KIB * KIB;
    if bytes_per_second >= MIB {
        format!("{bytes_per_second:.1} MiB/s")
    } else if bytes_per_second >= KIB {
        format!("{bytes_per_second:.0} KiB/s")
    } else {
        format!("{bytes_per_second:.0} B/s")
    }
}

/// Maps vendor install stage names to localized labels.
pub(crate) fn stage_label(stage: &str, locale: Locale) -> &str {
    match stage {
        "Queued" => locale.text("В очереди", "Queued"),
        "ResolveVersion" => locale.text("Получение метаданных версии", "Fetching version metadata"),
        "DownloadLibraries" => locale.text(
            "Скачивание клиента и библиотек",
            "Downloading client and libraries",
        ),
        "DownloadAssets" => locale.text("Скачивание ресурсов игры", "Downloading game assets"),
        "ExtractNatives" => locale.text(
            "Распаковка natives-библиотек",
            "Extracting native libraries",
        ),
        "LoaderInstall" => locale.text("Установка загрузчика", "Installing mod loader"),
        "InstallRuntime" => locale.text("Установка Java", "Installing Java"),
        "Verify" => locale.text("Проверка файлов", "Verifying files"),
        other => other,
    }
}

/// Builds the slim progress bar shown while an install downloads.
///
/// Byte totals drive the bar when the plan declared file sizes; otherwise the
/// finished file count stands in. Plans without either stay without a bar.
pub(crate) fn install_progress_view(
    received: u64,
    total_bytes: Option<u64>,
    speed_bps: f64,
    completed_tasks: usize,
    total_tasks: Option<usize>,
    palette: Palette,
    _locale: Locale,
) -> Option<AnyElement> {
    let fraction = if let Some(total) = total_bytes.filter(|total| *total > 0) {
        Some(received as f64 / total as f64)
    } else {
        total_tasks
            .filter(|total| *total > 0)
            .map(|total| completed_tasks as f64 / total as f64)
    }?;
    let fraction = fraction.clamp(0., 1.);

    Some(
        div()
            .h(px(4.))
            .w_full()
            .rounded(px(2.))
            .bg(rgb(palette.control))
            .overflow_hidden()
            .child(
                div()
                    .h_full()
                    .w(relative(fraction as f32))
                    .bg(rgb(if speed_bps > 0. {
                        palette.accent
                    } else {
                        palette.muted
                    })),
            )
            .into_any_element(),
    )
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
