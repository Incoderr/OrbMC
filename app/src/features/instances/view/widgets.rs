//! Progress and size labels of build cards.

pub(crate) use crate::features::mods::view::widgets::mod_icon_placeholder;
use crate::features::settings::*;
use crate::shared::theme::Palette;
use gpui_kit::*;

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
    const THRESHOLD_KIB: f64 = 10. * KIB;
    const MIB: f64 = KIB * KIB;
    let bytes = bytes_per_second.max(0.);
    if bytes >= MIB {
        format!("{:.1} MiB/s", bytes / MIB)
    } else if bytes >= THRESHOLD_KIB {
        format!("{:.0} KiB/s", bytes / KIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB/s", bytes / KIB)
    } else {
        format!("{bytes:.0} B/s")
    }
}

/// Play time as hours and minutes: `3 ч 5 мин`, `45 min`.
pub(crate) fn format_play_time(time: std::time::Duration, locale: Locale) -> String {
    let minutes = time.as_secs() / 60;
    let (hours, minutes) = (minutes / 60, minutes % 60);
    let (hour, minute) = (locale.text("ч", "h"), locale.text("мин", "min"));
    match (hours, minutes) {
        (0, 0) => locale.text("меньше минуты", "under a minute").to_owned(),
        (0, minutes) => format!("{minutes} {minute}"),
        (hours, 0) => format!("{hours} {hour}"),
        (hours, minutes) => format!("{hours} {hour} {minutes} {minute}"),
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

#[cfg(test)]
mod tests {
    use super::{format_play_time, format_speed};
    use crate::features::settings::Locale;
    use std::time::Duration;

    #[test]
    fn speed_labels_pick_the_nearest_unit() {
        assert_eq!(format_speed(120.), "120 B/s");
        assert_eq!(format_speed(1_024.), "1.0 KiB/s");
        assert_eq!(format_speed(2_562.), "2.5 KiB/s");
        assert_eq!(format_speed(48_123.), "47 KiB/s");
        assert_eq!(format_speed(1_572_864.), "1.5 MiB/s");
        assert_eq!(format_speed(-5.), "0 B/s");
    }

    #[test]
    fn play_time_shows_hours_and_minutes() {
        let label = |secs| format_play_time(Duration::from_secs(secs), Locale::Ru);
        assert_eq!(label(59), "меньше минуты");
        assert_eq!(label(45 * 60 + 30), "45 мин");
        assert_eq!(label(2 * 3600), "2 ч");
        assert_eq!(label(3 * 3600 + 5 * 60), "3 ч 5 мин");
        assert_eq!(
            format_play_time(Duration::from_secs(100 * 3600 + 60), Locale::En),
            "100 h 1 min"
        );
    }
}
