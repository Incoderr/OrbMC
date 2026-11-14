//! "Export as a modpack" dialog: format, name and version, the folders of
//! the build that go into the pack, then a save dialog and the export.

use crate::shared::motion::DialogMotion as _;
use std::path::PathBuf;

use super::InstancesView;
use crate::features::modpacks::{
    ExportFormat, ExportRequest, ExportSummary, export_pack, pack_runtime, top_level_entries,
};
use crate::features::mods::curseforge_api;
use crate::shared::locale::Locale;
use crate::shared::theme::Palette;
use crate::shared::ui::field_label;
use gpui_kit::base::{Button as UiButton, Disableable, StyledExt};
use gpui_kit::component::WindowExt;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use orbmc_core::minecraft::pack_export::PackInfo;

/// What the dialog is doing; lives only while it is open.
struct ExportDraft {
    format: ExportFormat,
    /// Top-level entries of the game directory and whether they are ticked;
    /// `None` while they are read.
    entries: Option<Result<Vec<(String, bool)>, String>>,
    status: ExportStatus,
}

enum ExportStatus {
    Idle,
    Exporting,
    Done {
        summary: ExportSummary,
        path: PathBuf,
    },
    Failed(String),
}

impl InstancesView {
    pub(crate) fn open_export_dialog(
        &mut self,
        id: crate::features::instances::InstanceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.instances.read(cx).profile(id).cloned() else {
            return;
        };
        let (locale, palette) = (self.locale, self.palette);
        let runtime = match pack_runtime(
            &profile.game_version,
            profile.loader,
            profile.loader_version.as_deref(),
        ) {
            Ok(runtime) => runtime,
            Err(error) => {
                tracing::warn!(profile_id = %id, %error, "build cannot be exported");
                window.push_notification(error.to_string(), cx);
                return;
            }
        };
        let curseforge_available = curseforge_api().is_some();
        let draft = cx.new(|_| ExportDraft {
            format: ExportFormat::Modrinth,
            entries: None,
            status: ExportStatus::Idle,
        });
        let game_directory = profile.game_directory();
        {
            let draft = draft.clone();
            let directory = game_directory.clone();
            let task = cx
                .background_executor()
                .spawn(async move { top_level_entries(&directory) });
            cx.spawn(async move |_, cx| {
                let entries = task.await.map_err(|error| {
                    tracing::warn!(profile_id = %id, %error, "failed to list build files");
                    error.to_string()
                });
                draft.update(cx, |draft, cx| {
                    draft.entries = Some(entries);
                    cx.notify();
                });
            })
            .detach();
        }
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(locale.text("Название сборки", "Pack name"))
                .default_value(profile.name.clone())
        });
        let version_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("1.0.0")
                .default_value("1.0.0")
        });
        let author_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(locale.text("Автор", "Author"))
                .default_value(self.player_name.clone())
        });
        let title = format!(
            "{} · {}",
            locale.text("Экспорт сборки", "Export build"),
            profile.name
        );

        window.open_dialog(cx, move |dialog, _, _| {
            let draft = draft.clone();
            let (name_input, version_input, author_input) = (
                name_input.clone(),
                version_input.clone(),
                author_input.clone(),
            );
            let runtime = runtime.clone();
            let game_directory = game_directory.clone();
            dialog
                .w(px(520.))
                .title(title.clone())
                .animated_content(move |content, _, cx| {
                    let state = draft.read(cx);
                    let format = state.format;
                    let busy = matches!(state.status, ExportStatus::Exporting);
                    let status = match &state.status {
                        ExportStatus::Idle => None,
                        ExportStatus::Exporting => Some((
                            locale
                                .text(
                                    "Проверяем файлы и собираем архив…",
                                    "Checking files and writing the archive…",
                                )
                                .to_owned(),
                            palette.muted,
                        )),
                        ExportStatus::Done { summary, path } => Some((
                            done_message(locale, *summary, path),
                            palette.foreground,
                        )),
                        ExportStatus::Failed(message) => Some((message.clone(), 0xc0392b)),
                    };
                    let done_path = match &state.status {
                        ExportStatus::Done { path, .. } => Some(path.clone()),
                        _ => None,
                    };
                    let entries = entry_list(&draft, state, busy, locale, palette);
                    let ready = matches!(state.entries, Some(Ok(_)));

                    let format_button = |target: ExportFormat, label: &'static str| {
                        let selected = format == target;
                        let enabled =
                            !busy && (target == ExportFormat::Modrinth || curseforge_available);
                        let draft = draft.clone();
                        UiButton::new(format!("export-format-{}", target.extension()))
                            .cursor_pointer()
                            .flex_1()
                            .h(px(36.))
                            .rounded(px(8.))
                            .bg(rgb(if selected {
                                palette.accent
                            } else {
                                palette.control
                            }))
                            .text_color(rgb(if selected {
                                palette.accent_foreground
                            } else {
                                palette.foreground
                            }))
                            .when(!selected && enabled, |button| {
                                button.hover(|style| style.bg(rgb(palette.hover)))
                            })
                            .disabled(!enabled)
                            .when(!enabled, |button| button.opacity(0.5))
                            .on_click(move |_, _, cx| {
                                draft.update(cx, |draft, cx| {
                                    draft.format = target;
                                    draft.status = ExportStatus::Idle;
                                    cx.notify();
                                });
                            })
                            .child(
                                div()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(label),
                            )
                    };

                    let export_click = {
                        let draft = draft.clone();
                        let (name_input, version_input, author_input) = (
                            name_input.clone(),
                            version_input.clone(),
                            author_input.clone(),
                        );
                        let runtime = runtime.clone();
                        let game_directory = game_directory.clone();
                        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                            let text = |input: &Entity<InputState>, fallback: &str| {
                                let value = input.read(cx).value().trim().to_owned();
                                if value.is_empty() {
                                    fallback.to_owned()
                                } else {
                                    value
                                }
                            };
                            let info = PackInfo {
                                name: text(&name_input, "Modpack"),
                                version: text(&version_input, "1.0.0"),
                                author: text(&author_input, "OrbMC"),
                                runtime: runtime.clone(),
                            };
                            start_export(&draft, info, game_directory.clone(), cx);
                        }
                    };

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
                                    .child(field_label(locale.text("Формат", "Format")))
                                    .child(
                                        div()
                                            .h_flex()
                                            .gap_2()
                                            .child(format_button(
                                                ExportFormat::Modrinth,
                                                "Modrinth (.mrpack)",
                                            ))
                                            .child(format_button(
                                                ExportFormat::CurseForge,
                                                "CurseForge (.zip)",
                                            )),
                                    )
                                    .when(!curseforge_available, |column| {
                                        column.child(hint(
                                            locale.text(
                                                "CurseForge недоступен в этой версии лаунчера.",
                                                "CurseForge is not available in this launcher build.",
                                            ),
                                            palette,
                                        ))
                                    }),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .gap_2()
                                    .child(field_label(locale.text("Название", "Name")))
                                    .child(Input::new(&name_input).disabled(busy)),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .gap_3()
                                    .child(
                                        div()
                                            .v_flex()
                                            .flex_1()
                                            .gap_2()
                                            .child(field_label(locale.text("Версия", "Version")))
                                            .child(Input::new(&version_input).disabled(busy)),
                                    )
                                    .child(
                                        div()
                                            .v_flex()
                                            .flex_1()
                                            .gap_2()
                                            .child(field_label(locale.text("Автор", "Author")))
                                            .child(Input::new(&author_input).disabled(busy)),
                                    ),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .gap_2()
                                    .child(field_label(locale.text(
                                        "Что положить в сборку",
                                        "What goes into the pack",
                                    )))
                                    .child(hint(
                                        match format {
                                            ExportFormat::Modrinth => locale.text(
                                                "Моды, ресурспаки и шейдеры с Modrinth попадут ссылками, остальное — внутрь архива.",
                                                "Mods, resource packs and shaders from Modrinth are linked, everything else is packed.",
                                            ),
                                            ExportFormat::CurseForge => locale.text(
                                                "Моды, ресурспаки и шейдеры с CurseForge попадут ссылками, остальное — внутрь архива.",
                                                "Mods, resource packs and shaders from CurseForge are linked, everything else is packed.",
                                            ),
                                        },
                                        palette,
                                    ))
                                    .child(entries),
                            )
                            .when_some(status, |column, (message, color)| {
                                column.child(
                                    div()
                                        .text_size(px(13.))
                                        .text_color(rgb(color))
                                        .child(message),
                                )
                            })
                            .child(
                                div()
                                    .h_flex()
                                    .justify_end()
                                    .gap_2()
                                    .when_some(done_path, |row, path| {
                                        row.child(
                                            secondary_button(
                                                "export-reveal",
                                                locale.text("Показать файл", "Show file"),
                                                palette,
                                            )
                                            .on_click(move |_, _, _| {
                                                if let Err(error) =
                                                    crate::platform::process::reveal_file(&path)
                                                {
                                                    tracing::warn!(%error, "failed to show the exported pack");
                                                }
                                            }),
                                        )
                                    })
                                    .child(
                                        secondary_button(
                                            "export-close",
                                            locale.text("Закрыть", "Close"),
                                            palette,
                                        )
                                        .on_click(|_, window, cx| window.close_dialog(cx)),
                                    )
                                    .child(
                                        UiButton::new("export-start")
                                            .cursor_pointer()
                                            .px_3()
                                            .py_2()
                                            .rounded(px(8.))
                                            .bg(rgb(palette.accent))
                                            .text_color(rgb(palette.accent_foreground))
                                            .hover(|style| style.opacity(0.9))
                                            .disabled(busy || !ready)
                                            .when(busy || !ready, |button| button.opacity(0.6))
                                            .on_click(export_click)
                                            .child(locale.text("Экспортировать…", "Export…")),
                                    ),
                            ),
                    )
                })
        });
    }
}

/// Asks where to save the pack, then exports it off the UI thread.
fn start_export(
    draft: &Entity<ExportDraft>,
    info: PackInfo,
    game_directory: PathBuf,
    cx: &mut App,
) {
    let (format, entries) = {
        let state = draft.read(cx);
        let entries: Vec<String> = match &state.entries {
            Some(Ok(entries)) => entries
                .iter()
                .filter(|(_, ticked)| *ticked)
                .map(|(name, _)| name.clone())
                .collect(),
            _ => return,
        };
        (state.format, entries)
    };
    let directory = dirs::download_dir().unwrap_or_else(|| game_directory.clone());
    let suggested = format!(
        "{}-{}.{}",
        file_name_part(&info.name),
        file_name_part(&info.version),
        format.extension()
    );
    let response = cx.prompt_for_new_path(&directory, Some(&suggested));
    let draft = draft.clone();
    cx.spawn(async move |cx| {
        let destination = match response.await {
            Ok(Ok(Some(path))) => with_extension(path, format.extension()),
            Ok(Ok(None)) => return,
            Ok(Err(error)) => {
                tracing::warn!(%error, "failed to open the save dialog");
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "save dialog response channel closed");
                return;
            }
        };
        draft.update(cx, |draft, cx| {
            draft.status = ExportStatus::Exporting;
            cx.notify();
        });
        let request = ExportRequest {
            format,
            info,
            game_directory,
            entries,
            destination: destination.clone(),
        };
        let result = cx
            .background_executor()
            .spawn(async move { export_pack(&request, curseforge_api().as_ref()) })
            .await;
        draft.update(cx, |draft, cx| {
            draft.status = match result {
                Ok(summary) => ExportStatus::Done {
                    summary,
                    path: destination,
                },
                Err(error) => {
                    tracing::error!(%error, "modpack export failed");
                    ExportStatus::Failed(error.to_string())
                }
            };
            cx.notify();
        });
    })
    .detach();
}

/// Ticks for the top-level files and folders of the build.
fn entry_list(
    draft: &Entity<ExportDraft>,
    state: &ExportDraft,
    busy: bool,
    locale: Locale,
    palette: Palette,
) -> AnyElement {
    let entries = match &state.entries {
        None => {
            return hint(
                locale.text("Читаем папку сборки…", "Reading the build folder…"),
                palette,
            );
        }
        Some(Err(error)) => return hint_owned(error.clone(), palette),
        Some(Ok(entries)) if entries.is_empty() => {
            return hint(
                locale.text(
                    "Папка сборки пуста — сначала установи и запусти сборку.",
                    "The build folder is empty: install and run the build first.",
                ),
                palette,
            );
        }
        Some(Ok(entries)) => entries,
    };
    div()
        .id("export-entries")
        .v_flex()
        .gap_1p5()
        .max_h(px(200.))
        .p_2()
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(palette.border))
        .overflow_y_scrollbar()
        .children(entries.iter().enumerate().map(|(index, (name, ticked))| {
            let draft = draft.clone();
            Checkbox::new(format!("export-entry-{index}"))
                .label(name.clone())
                .checked(*ticked)
                .disabled(busy)
                .on_click(move |checked, _, cx| {
                    let checked = *checked;
                    draft.update(cx, |draft, cx| {
                        if let Some(Ok(entries)) = &mut draft.entries
                            && let Some(entry) = entries.get_mut(index)
                        {
                            entry.1 = checked;
                        }
                        cx.notify();
                    });
                })
        }))
        .into_any_element()
}

fn done_message(locale: Locale, summary: ExportSummary, path: &std::path::Path) -> String {
    let file = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    match locale {
        Locale::Ru => format!(
            "Готово: {file}. Ссылками: {}, внутри архива: {}.",
            summary.linked, summary.packed
        ),
        Locale::En => format!(
            "Done: {file}. Linked: {}, packed: {}.",
            summary.linked, summary.packed
        ),
    }
}

fn hint(text: &'static str, palette: Palette) -> AnyElement {
    hint_owned(text.to_owned(), palette)
}

fn hint_owned(text: String, palette: Palette) -> AnyElement {
    div()
        .text_size(px(12.))
        .text_color(rgb(palette.muted))
        .child(text)
        .into_any_element()
}

fn secondary_button(id: &'static str, label: &'static str, palette: Palette) -> UiButton {
    UiButton::new(id)
        .cursor_pointer()
        .px_3()
        .py_2()
        .rounded(px(8.))
        .bg(rgb(palette.control))
        .text_color(rgb(palette.foreground))
        .hover(|style| style.bg(rgb(palette.hover)))
        .child(label)
}

/// A pack name usable in a file name on every OS.
fn file_name_part(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').to_owned();
    if cleaned.is_empty() {
        "modpack".to_owned()
    } else {
        cleaned
    }
}

/// The save dialog may drop the extension the format needs.
fn with_extension(path: PathBuf, extension: &str) -> PathBuf {
    let has_it = path
        .extension()
        .is_some_and(|current| current.eq_ignore_ascii_case(extension));
    if has_it {
        path
    } else {
        let mut name = path.into_os_string();
        name.push(".");
        name.push(extension);
        PathBuf::from(name)
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: the GPUI prelude brings its own `test` attribute.
    use super::{file_name_part, with_extension};
    use std::path::PathBuf;

    #[test]
    fn file_names_lose_characters_windows_refuses() {
        assert_eq!(file_name_part("My: Pack?"), "My_ Pack_");
        assert_eq!(file_name_part(" ... "), "modpack");
        assert_eq!(file_name_part("Сборка 1.0"), "Сборка 1.0");
    }

    #[test]
    fn missing_extensions_are_added() {
        assert_eq!(
            with_extension(PathBuf::from("pack"), "mrpack"),
            PathBuf::from("pack.mrpack")
        );
        assert_eq!(
            with_extension(PathBuf::from("pack.MRPACK"), "mrpack"),
            PathBuf::from("pack.MRPACK")
        );
        assert_eq!(
            with_extension(PathBuf::from("pack-1.0"), "zip"),
            PathBuf::from("pack-1.0.zip")
        );
    }
}
