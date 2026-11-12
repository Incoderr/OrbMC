//! Importing a modpack file from disk (`.mrpack` or a CurseForge `.zip`)
//! as a new build.

use std::path::PathBuf;
use std::time::Duration;

use super::InstancesView;
use crate::features::downloads::{Transfer, TransferHandle};
use crate::features::instances::{InstanceId, InstanceProfile, InstanceRuntime};
use crate::features::modpacks::{PACK_EXTENSIONS, import_pack, read_pack_header};
use crate::shared::locale::Locale;
use gpui_kit::component::WindowExt;
use gpui_kit::*;

/// How often the build card redraws download progress.
const PROGRESS_REDRAW: Duration = Duration::from_millis(250);
/// Names of files to add by hand listed in the notification; the rest are
/// counted.
const LISTED_SKIPPED: usize = 8;

impl InstancesView {
    /// Lets the player pick a pack file and installs it as a new build.
    pub(crate) fn import_modpack(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let locale = self.locale;
        let title = locale.text("Выбери файл сборки", "Choose a modpack file");
        let filter = crate::platform::file_dialog::FileFilter {
            name: locale.text(
                "Сборки Modrinth и CurseForge (.mrpack, .zip)",
                "Modrinth and CurseForge packs (.mrpack, .zip)",
            ),
            extensions: PACK_EXTENSIONS,
        };
        let native = crate::platform::file_dialog::pick_file(title, filter);
        // Without a filtered system dialog GPUI's picker is used and the
        // extension is checked after the pick.
        let fallback = native.is_none().then(|| {
            cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(title.into()),
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let picked = if let Some(native) = native {
                native.await
            } else if let Some(fallback) = fallback {
                match fallback.await {
                    Ok(Ok(Some(paths))) => paths.into_iter().next(),
                    Ok(Ok(None)) => None,
                    Ok(Err(error)) => {
                        tracing::warn!(%error, "failed to open the modpack file picker");
                        None
                    }
                    Err(error) => {
                        tracing::warn!(%error, "modpack file picker response channel closed");
                        None
                    }
                }
            } else {
                None
            };
            let Some(archive) = picked.filter(|path| path.is_file()) else {
                return;
            };
            if !filter.matches(&archive) {
                let _ = this.update_in(cx, |_, window, cx| {
                    window.push_notification(
                        locale.text(
                            "Нужен файл сборки .mrpack или .zip",
                            "Pick a .mrpack or .zip modpack file",
                        ),
                        cx,
                    );
                });
                return;
            }
            let header = {
                let archive = archive.clone();
                cx.background_executor()
                    .spawn(async move { read_pack_header(&archive) })
                    .await
            };
            let _ = this.update_in(cx, |this, window, cx| match header {
                Ok(header) => this.start_pack_import(archive, header, window, cx),
                Err(error) => {
                    tracing::warn!(path = %archive.display(), %error, "not an importable modpack");
                    window.push_notification(error.to_string(), cx);
                }
            });
        })
        .detach();
    }

    fn start_pack_import(
        &mut self,
        archive: PathBuf,
        header: crate::features::modpacks::PackHeader,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = self.locale;
        let target = header.target;
        let profile = InstanceProfile::new(
            header.name.clone(),
            target.game_version,
            target.loader,
            target.loader_version,
            self.settings.read(cx).download_directory().clone(),
        );
        let id = profile.id;
        let handle = TransferHandle::default();
        self.instances.update(cx, |instances, cx| {
            instances.add(profile);
            instances.save();
            instances.mark_content_downloading(id, header.name.clone(), handle.clone());
            cx.notify();
        });
        self.redraw_while_downloading(id, cx);

        let Some(profile) = self.instances.read(cx).profile(id).cloned() else {
            return;
        };
        let game_directory = profile.game_directory();
        let work_directory = match profile.data_directory() {
            Ok(data) => data.join("modpack"),
            Err(error) => {
                self.instances.update(cx, |instances, cx| {
                    instances.mark_content_download_failed(id, error.to_string());
                    cx.notify();
                });
                return;
            }
        };
        let client = cx.http_client();
        let transfer = Transfer {
            handle,
            executor: cx.background_executor().clone(),
        };
        let task = cx.background_executor().spawn(async move {
            import_pack(
                client,
                &archive,
                &game_directory,
                &work_directory,
                &transfer,
            )
            .await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(installed) => {
                        if !installed.skipped.is_empty() {
                            tracing::warn!(
                                profile_id = %id,
                                files = ?installed.skipped,
                                "modpack files left for the player to add"
                            );
                            window
                                .push_notification(skipped_message(locale, &installed.skipped), cx);
                        }
                        let target = installed.target;
                        this.instances.update(cx, |instances, cx| {
                            instances.set_game(
                                id,
                                target.game_version,
                                target.loader,
                                target.loader_version,
                            );
                            instances.save();
                            instances.start_install(id, cx);
                            cx.notify();
                        });
                    }
                    Err(error) => {
                        let cancelled = error.is_cancelled();
                        if cancelled {
                            tracing::info!(profile_id = %id, "modpack import cancelled");
                        } else {
                            tracing::warn!(profile_id = %id, %error, "modpack import failed");
                        }
                        this.instances.update(cx, |instances, cx| {
                            instances.mark_content_download_failed(id, error.to_string());
                            // A cancelled import leaves a half-filled build
                            // nobody asked to keep.
                            if cancelled {
                                instances.start_removal(id, cx);
                            }
                            cx.notify();
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Redraws the build card while its pack files download: the transfer
    /// counters change without notifying anyone.
    fn redraw_while_downloading(&self, id: InstanceId, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PROGRESS_REDRAW).await;
                let active = this
                    .update(cx, |this, cx| {
                        let active = matches!(
                            this.instances.read(cx).runtime(&id),
                            InstanceRuntime::DownloadingContent { .. }
                        );
                        if active {
                            cx.notify();
                        }
                        active
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        })
        .detach();
    }
}

fn skipped_message(locale: Locale, skipped: &[String]) -> String {
    let mut names = skipped
        .iter()
        .take(LISTED_SKIPPED)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let rest = skipped.len().saturating_sub(LISTED_SKIPPED);
    match locale {
        Locale::Ru => {
            if rest > 0 {
                names.push_str(&format!(" и ещё {rest}"));
            }
            format!(
                "Не скачались файлы ({}): {names}. CurseForge отдаёт их только своему приложению (или это не моды) — скачай их вручную и положи в папку сборки.",
                skipped.len()
            )
        }
        Locale::En => {
            if rest > 0 {
                names.push_str(&format!(" and {rest} more"));
            }
            format!(
                "{} files were not downloaded: {names}. CurseForge serves them only to its own app (or they are not mods); download them by hand into the build folder.",
                skipped.len()
            )
        }
    }
}
