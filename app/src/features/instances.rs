use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

use futures::channel::mpsc::UnboundedSender;
use mc_launcher_core::prelude::{
    Account, DownloadControl, InstallRequest, JavaInstallPolicy, LaunchOptions, Launcher,
    LoaderSpec, LoaderVersion, ProgressEvent,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::java::{self, JavaRequirement};
use super::versions::LoaderChoice;

/// Username used for offline launches until a real account is connected.
pub const OFFLINE_PLAYER_NAME: &str = "Player";

/// Validates a Minecraft player name used for offline launches.
///
/// The vanilla rules apply: 3-16 characters, Latin letters, digits and `_`.
pub fn is_valid_player_name(name: &str) -> bool {
    let name = name.trim();
    (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstanceProfile {
    pub id: Uuid,
    pub name: String,
    pub game_version: String,
    pub loader: LoaderChoice,
    pub loader_version: Option<String>,
    #[serde(default = "default_memory_mb")]
    pub memory_mb: u32,
    #[serde(default)]
    pub java_path: Option<PathBuf>,
    #[serde(default)]
    pub installation_root: PathBuf,
    pub installed_version_id: Option<String>,
    /// Persisted logical launch order; zero means this profile has never been launched.
    #[serde(default)]
    pub last_launch_order: u64,
}

impl InstanceProfile {
    pub fn new(
        name: String,
        game_version: String,
        loader: LoaderChoice,
        loader_version: Option<String>,
        installation_root: PathBuf,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            game_version,
            loader,
            loader_version,
            memory_mb: 4096,
            java_path: None,
            installation_root,
            installed_version_id: None,
            last_launch_order: 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum InstanceRuntime {
    #[default]
    NotInstalled,
    Installing {
        /// Coarse install stage label.
        stage: String,
        /// Bytes downloaded for the current download plan.
        received: u64,
        /// Total bytes of the current download plan, when every file
        /// declared its size.
        total_bytes: Option<u64>,
        /// Smoothed download speed in bytes per second.
        speed_bps: f64,
        /// Number of finished or skipped files.
        completed_tasks: usize,
        /// Number of files the plan expects to download.
        total_tasks: Option<usize>,
        /// Whether the install is paused through its control handle.
        paused: bool,
    },
    DownloadingMod {
        name: String,
    },
    DownloadingContent {
        name: String,
    },
    ContentDownloadFailed {
        message: String,
    },
    ContentDownloaded {
        filename: String,
    },
    ModDownloadFailed {
        message: String,
    },
    ModDownloaded {
        filename: String,
    },
    Ready,
    Launching,
    /// Java for the launch is being resolved or downloaded.
    PreparingJava {
        detail: String,
    },
    Running {
        process_id: u32,
    },
    Removing,
    RemovalFailed {
        message: String,
    },
    Failed {
        message: String,
    },
}

impl InstanceRuntime {
    /// Whether this state blocks other actions on the profile.
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            InstanceRuntime::Installing { .. }
                | InstanceRuntime::DownloadingMod { .. }
                | InstanceRuntime::DownloadingContent { .. }
                | InstanceRuntime::PreparingJava { .. }
                | InstanceRuntime::Launching
                | InstanceRuntime::Running { .. }
                | InstanceRuntime::Removing
        )
    }
}

#[derive(Debug, Default)]
pub struct InstancesState {
    profiles: Vec<InstanceProfile>,
    runtime: HashMap<Uuid, InstanceRuntime>,
    worlds: HashMap<Uuid, super::worlds::WorldList>,
    /// Pause and cancel handles for running installs.
    controls: HashMap<Uuid, DownloadControl>,
    /// Last speed sample per running install.
    samples: HashMap<Uuid, ProgressSample>,
}

/// One speed measurement point for an install.
#[derive(Clone, Copy, Debug)]
struct ProgressSample {
    at: Instant,
    downloaded: u64,
}

/// Exponential smoothing factor for download speed.
const SPEED_SMOOTHING: f64 = 0.25;

impl InstancesState {
    pub fn from_profiles(profiles: Vec<InstanceProfile>) -> Self {
        let runtime = profiles
            .iter()
            .map(|profile| {
                let state = if profile.installed_version_id.is_some() {
                    InstanceRuntime::Ready
                } else {
                    InstanceRuntime::NotInstalled
                };
                (profile.id, state)
            })
            .collect();
        Self {
            profiles,
            runtime,
            controls: HashMap::new(),
            samples: HashMap::new(),
            worlds: HashMap::new(),
        }
    }

    pub fn profiles(&self) -> &[InstanceProfile] {
        &self.profiles
    }

    pub fn recent_profiles(&self) -> Vec<&InstanceProfile> {
        let mut profiles: Vec<_> = self.profiles.iter().collect();
        profiles.sort_by_key(|profile| std::cmp::Reverse(profile.last_launch_order));
        profiles
    }

    pub fn profile(&self, id: Uuid) -> Option<&InstanceProfile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    pub fn worlds(&self, id: Uuid) -> super::worlds::WorldList {
        self.worlds
            .get(&id)
            .cloned()
            .unwrap_or(super::worlds::WorldList::Loading)
    }

    pub fn set_worlds(&mut self, id: Uuid, worlds: super::worlds::WorldList) {
        if self.profile(id).is_some() {
            self.worlds.insert(id, worlds);
        }
    }

    pub fn remove(&mut self, id: Uuid) -> Option<InstanceProfile> {
        let index = self.profiles.iter().position(|profile| profile.id == id)?;
        self.runtime.remove(&id);
        self.worlds.remove(&id);
        self.controls.remove(&id);
        self.samples.remove(&id);
        Some(self.profiles.remove(index))
    }

    pub fn runtime(&self, id: &Uuid) -> InstanceRuntime {
        self.runtime.get(id).cloned().unwrap_or_default()
    }

    pub fn add(&mut self, profile: InstanceProfile) {
        self.runtime.insert(
            profile.id,
            InstanceRuntime::Installing {
                stage: "Queued".to_owned(),
                received: 0,
                total_bytes: None,
                speed_bps: 0.,
                completed_tasks: 0,
                total_tasks: None,
                paused: false,
            },
        );
        self.profiles.push(profile);
    }

    pub fn mark_installing(&mut self, id: Uuid) {
        self.runtime.insert(
            id,
            InstanceRuntime::Installing {
                stage: "Queued".to_owned(),
                received: 0,
                total_bytes: None,
                speed_bps: 0.,
                completed_tasks: 0,
                total_tasks: None,
                paused: false,
            },
        );
    }

    /// Stores the pause and cancel handle for a running install.
    pub fn set_install_control(&mut self, id: Uuid, control: DownloadControl) {
        self.controls.insert(id, control);
    }

    /// Drops the install control and progress sample once work ends.
    pub fn finish_install(&mut self, id: Uuid) {
        self.controls.remove(&id);
        self.samples.remove(&id);
    }

    /// Pauses the install through its control handle.
    pub fn pause_install(&mut self, id: Uuid) -> bool {
        self.mutate_install_control(id, |control| {
            control.pause();
            true
        })
    }

    /// Resumes a paused install.
    pub fn resume_install(&mut self, id: Uuid) -> bool {
        self.mutate_install_control(id, |control| {
            control.resume();
            true
        })
    }

    /// Cancels the install; the worker reports the outcome as a failure.
    pub fn cancel_install(&mut self, id: Uuid) -> bool {
        self.mutate_install_control(id, |control| {
            control.cancel();
            true
        })
    }

    fn mutate_install_control(
        &mut self,
        id: Uuid,
        mutate: impl FnOnce(&DownloadControl) -> bool,
    ) -> bool {
        let Some(control) = self.controls.get(&id) else {
            return false;
        };
        let applied = mutate(control);
        if applied {
            self.mark_paused_state(id);
        }
        applied
    }

    /// Mirrors the control state into the runtime so the UI can render it.
    fn mark_paused_state(&mut self, id: Uuid) {
        let paused = self
            .controls
            .get(&id)
            .is_some_and(|control| control.is_paused());
        let Some(InstanceRuntime::Installing {
            stage,
            received,
            total_bytes,
            speed_bps,
            completed_tasks,
            total_tasks,
            ..
        }) = self.runtime.get(&id).cloned()
        else {
            return;
        };
        self.runtime.insert(
            id,
            InstanceRuntime::Installing {
                stage,
                received,
                total_bytes,
                speed_bps,
                completed_tasks,
                total_tasks,
                paused,
            },
        );
    }

    pub fn mark_mod_downloading(&mut self, id: Uuid, name: String) {
        self.runtime
            .insert(id, InstanceRuntime::DownloadingMod { name });
    }

    pub fn mark_mod_downloaded(&mut self, id: Uuid, filename: String) {
        self.runtime
            .insert(id, InstanceRuntime::ModDownloaded { filename });
    }

    pub fn mark_mod_download_failed(&mut self, id: Uuid, message: String) {
        self.runtime
            .insert(id, InstanceRuntime::ModDownloadFailed { message });
    }

    pub fn mark_content_downloading(&mut self, id: Uuid, name: String) {
        self.runtime
            .insert(id, InstanceRuntime::DownloadingContent { name });
    }

    pub fn mark_content_downloaded(&mut self, id: Uuid, filename: String) {
        self.runtime
            .insert(id, InstanceRuntime::ContentDownloaded { filename });
    }

    pub fn mark_content_download_failed(&mut self, id: Uuid, message: String) {
        self.runtime
            .insert(id, InstanceRuntime::ContentDownloadFailed { message });
    }

    /// Applies a vendor progress event to the runtime state.
    pub fn update_progress(&mut self, id: Uuid, update: InstallProgressUpdate) {
        let current = self.runtime.get(&id).cloned().unwrap_or_default();
        let InstanceRuntime::Installing {
            stage,
            received,
            total_bytes,
            speed_bps,
            completed_tasks,
            total_tasks,
            paused,
        } = current
        else {
            return;
        };

        let mut next = InstanceRuntime::Installing {
            stage,
            received,
            total_bytes,
            speed_bps,
            completed_tasks,
            total_tasks,
            paused,
        };

        match update {
            InstallProgressUpdate::Stage(stage) => {
                if let InstanceRuntime::Installing { stage: target, .. } = &mut next {
                    *target = stage;
                }
            }
            InstallProgressUpdate::PlanStarted { tasks, total_bytes } => {
                self.samples.remove(&id);
                if let InstanceRuntime::Installing {
                    received: target_received,
                    total_bytes: target_total,
                    speed_bps: target_speed,
                    total_tasks: target_tasks,
                    ..
                } = &mut next
                {
                    *target_received = 0;
                    *target_total = total_bytes;
                    *target_speed = 0.;
                    *target_tasks = Some(tasks);
                }
            }
            InstallProgressUpdate::PlanProgress {
                downloaded,
                total_bytes,
            } => {
                let now = Instant::now();
                let previous = self.samples.insert(
                    id,
                    ProgressSample {
                        at: now,
                        downloaded,
                    },
                );
                let measured_speed = previous.and_then(|previous| {
                    let elapsed_seconds = (now - previous.at).as_secs_f64();
                    if elapsed_seconds <= 0. {
                        return None;
                    }
                    let delta = downloaded.saturating_sub(previous.downloaded);
                    (delta > 0).then_some(delta as f64 / elapsed_seconds)
                });
                if let InstanceRuntime::Installing {
                    received: target_received,
                    total_bytes: target_total,
                    speed_bps: target_speed,
                    ..
                } = &mut next
                {
                    *target_received = downloaded;
                    *target_total = total_bytes.or(*target_total);
                    if let Some(measured) = measured_speed {
                        *target_speed = if *target_speed <= 0. {
                            measured
                        } else {
                            SPEED_SMOOTHING * measured + (1. - SPEED_SMOOTHING) * *target_speed
                        };
                    }
                }
            }
            InstallProgressUpdate::TaskDone => {
                if let InstanceRuntime::Installing {
                    completed_tasks: target,
                    ..
                } = &mut next
                {
                    *target += 1;
                }
            }
        }

        self.runtime.insert(id, next);
    }

    pub fn mark_installed(&mut self, id: Uuid, version_id: String) {
        if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == id) {
            profile.installed_version_id = Some(version_id);
        }
        self.runtime.insert(id, InstanceRuntime::Ready);
    }

    pub fn set_memory_mb(&mut self, id: Uuid, memory_mb: u32) {
        if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == id) {
            profile.memory_mb = memory_mb.clamp(512, 16_384);
        }
    }

    pub fn set_java_path(&mut self, id: Uuid, java_path: Option<PathBuf>) {
        if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == id) {
            profile.java_path = java_path;
        }
    }

    pub fn mark_failed(&mut self, id: Uuid, message: String) {
        self.runtime.insert(id, InstanceRuntime::Failed { message });
    }

    pub fn mark_removing(&mut self, id: Uuid) {
        self.runtime.insert(id, InstanceRuntime::Removing);
    }

    pub fn mark_removal_failed(&mut self, id: Uuid, message: String) {
        self.runtime
            .insert(id, InstanceRuntime::RemovalFailed { message });
    }

    pub fn mark_launching(&mut self, id: Uuid) {
        self.runtime.insert(id, InstanceRuntime::Launching);
    }

    /// Shows Java preparation progress during launch; `None` returns the
    /// profile to the `Launching` state.
    pub fn mark_preparing_java(&mut self, id: Uuid, detail: Option<String>) {
        match detail {
            Some(detail) => {
                self.runtime
                    .insert(id, InstanceRuntime::PreparingJava { detail });
            }
            None => {
                if matches!(
                    self.runtime.get(&id),
                    Some(InstanceRuntime::PreparingJava { .. })
                ) {
                    self.runtime.insert(id, InstanceRuntime::Launching);
                }
            }
        }
    }

    pub fn mark_running(&mut self, id: Uuid, process_id: u32) {
        let next_order = self
            .profiles
            .iter()
            .map(|profile| profile.last_launch_order)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == id) {
            profile.last_launch_order = next_order;
        }
        self.runtime
            .insert(id, InstanceRuntime::Running { process_id });
    }

    pub fn mark_game_stopped(&mut self, id: Uuid, exit_code: Option<i32>, stderr: Option<String>) {
        if self
            .profiles
            .iter()
            .any(|profile| profile.id == id && profile.installed_version_id.is_some())
        {
            if exit_code == Some(0) {
                self.runtime.insert(id, InstanceRuntime::Ready);
            } else {
                self.runtime.insert(
                    id,
                    InstanceRuntime::Failed {
                        message: stderr.map_or_else(
                            || format!("Minecraft завершился с кодом {exit_code:?}"),
                            |stderr| {
                                format!(
                                    "Minecraft завершился с кодом {exit_code:?}:\n{}",
                                    stderr.trim()
                                )
                            },
                        ),
                    },
                );
            }
        } else {
            self.runtime.insert(id, InstanceRuntime::NotInstalled);
        }
    }
}

/// Install progress reported by the vendor download engine.
#[derive(Clone, Debug)]
pub enum InstallProgressUpdate {
    /// A coarse install stage started.
    Stage(String),
    /// A download plan started with its expected file count and byte total.
    PlanStarted {
        tasks: usize,
        total_bytes: Option<u64>,
    },
    /// Aggregated byte progress for the running download plan.
    PlanProgress {
        downloaded: u64,
        total_bytes: Option<u64>,
    },
    /// One file finished or was skipped.
    TaskDone,
}

#[derive(Debug)]
pub enum InstanceWorkerEvent {
    Progress {
        id: Uuid,
        update: InstallProgressUpdate,
    },
    Installed {
        id: Uuid,
        version_id: String,
    },
    InstallFailed {
        id: Uuid,
        message: String,
    },
    Launched {
        id: Uuid,
        process_id: u32,
    },
    /// Java for the launch is being resolved or downloaded; `None` ends the
    /// preparation phase.
    PreparingJava {
        id: Uuid,
        detail: Option<String>,
    },
    GameExited {
        id: Uuid,
        code: Option<i32>,
        stderr: Option<String>,
    },
    LaunchFailed {
        id: Uuid,
        message: String,
    },
    ProfileRemoved {
        id: Uuid,
    },
    ProfileRemovalFailed {
        id: Uuid,
        message: String,
    },
}

pub fn install_profile(
    profile: InstanceProfile,
    minecraft_directory: PathBuf,
    control: DownloadControl,
    events: UnboundedSender<InstanceWorkerEvent>,
) {
    let id = profile.id;
    let forge_prefix = format!("{}-", profile.game_version);
    let loader = match profile.loader {
        LoaderChoice::Vanilla => None,
        LoaderChoice::Fabric => profile.loader_version.map(|version| LoaderSpec::Fabric {
            version: LoaderVersion::Exact(version),
        }),
        LoaderChoice::Forge => profile.loader_version.map(|version| {
            let version = if version.starts_with(&forge_prefix) {
                version
            } else {
                format!("{forge_prefix}{version}")
            };
            LoaderSpec::Forge {
                version: LoaderVersion::Exact(version),
            }
        }),
        LoaderChoice::NeoForge => profile.loader_version.map(|version| LoaderSpec::NeoForge {
            version: LoaderVersion::Exact(version),
        }),
    };
    let loader = match (profile.loader, loader) {
        (LoaderChoice::Vanilla, _) => None,
        (_, Some(loader)) => Some(loader),
        _ => {
            let _ = events.unbounded_send(InstanceWorkerEvent::InstallFailed {
                id,
                message: "Для выбранного загрузчика не указана версия".to_owned(),
            });
            return;
        }
    };

    let requirement =
        match mc_launcher_core::install::client::fetch_vanilla_version(&profile.game_version) {
            Ok(version) => JavaRequirement::from_version(&version),
            Err(error) => {
                tracing::error!(
                    profile_id = %id,
                    version = %profile.game_version,
                    error = %error,
                    "could not fetch vanilla metadata to determine the Java requirement"
                );
                let _ = events.unbounded_send(InstanceWorkerEvent::InstallFailed {
                    id,
                    message: format!(
                        "Не удалось получить метаданные Minecraft {}: {error}",
                        profile.game_version
                    ),
                });
                return;
            }
        };
    let java_executable = match java::resolve_java(
        profile.java_path.as_deref(),
        &requirement,
        &minecraft_directory,
        |detail| {
            let _ = events.unbounded_send(InstanceWorkerEvent::Progress {
                id,
                update: InstallProgressUpdate::Stage(detail),
            });
        },
    ) {
        Ok(resolution) => Some(resolution.executable),
        Err(message) => {
            let _ = events.unbounded_send(InstanceWorkerEvent::InstallFailed { id, message });
            return;
        }
    };

    let launcher = Launcher::new(minecraft_directory);
    let request = InstallRequest {
        minecraft_version: profile.game_version,
        loader,
        java: JavaInstallPolicy::Auto,
    };
    let mut reporter = |event: ProgressEvent| {
        let update = match event {
            ProgressEvent::StageStarted { stage } => {
                InstallProgressUpdate::Stage(format!("{stage:?}"))
            }
            ProgressEvent::PlanStarted { tasks, total_bytes } => {
                InstallProgressUpdate::PlanStarted { tasks, total_bytes }
            }
            ProgressEvent::PlanProgress {
                downloaded,
                total_bytes,
            } => InstallProgressUpdate::PlanProgress {
                downloaded,
                total_bytes,
            },
            ProgressEvent::TaskSkipped { .. } | ProgressEvent::TaskFinished { .. } => {
                InstallProgressUpdate::TaskDone
            }
            ProgressEvent::TaskStarted { .. } => return,
        };
        let _ = events.unbounded_send(InstanceWorkerEvent::Progress { id, update });
    };

    match launcher.install_with_java_progress(request, java_executable, &mut reporter, &control) {
        Ok(installed) => {
            let _ = events.unbounded_send(InstanceWorkerEvent::Installed {
                id,
                version_id: installed.version_id,
            });
        }
        Err(error) => {
            tracing::error!(
                profile_id = %id,
                error = %error,
                "Minecraft install failed"
            );
            let message = if matches!(
                error,
                mc_launcher_core::prelude::LauncherError::InstallCancelled
            ) {
                "Установка отменена".to_owned()
            } else {
                error.to_string()
            };
            let _ = events.unbounded_send(InstanceWorkerEvent::InstallFailed { id, message });
        }
    }
}

pub fn launch_profile(
    profile: InstanceProfile,
    minecraft_directory: PathBuf,
    world: Option<String>,
    player_name: String,
    events: UnboundedSender<InstanceWorkerEvent>,
) {
    let id = profile.id;
    let Some(version_id) = profile.installed_version_id.clone() else {
        let _ = events.unbounded_send(InstanceWorkerEvent::LaunchFailed {
            id,
            message: "Профиль ещё не установлен".to_owned(),
        });
        return;
    };

    let game_directory = match crate::platform::profile_store::ensure_instance_directory(
        &profile.installation_root,
        id,
    ) {
        Ok(path) => path,
        Err(error) => {
            let path =
                crate::platform::profile_store::instance_directory(&profile.installation_root, id);
            let _ = events.unbounded_send(InstanceWorkerEvent::LaunchFailed {
                id,
                message: format!(
                    "Не удалось создать папку профиля {}: {error}",
                    path.display()
                ),
            });
            return;
        }
    };

    let launcher = Launcher::new(minecraft_directory.clone());
    let version = match launcher.load_version(&version_id) {
        Ok(version) => version,
        Err(error) => {
            let _ = events.unbounded_send(InstanceWorkerEvent::LaunchFailed {
                id,
                message: error.to_string(),
            });
            return;
        }
    };

    let requirement = JavaRequirement::from_version(&version);
    let resolution = match java::resolve_java(
        profile.java_path.as_deref(),
        &requirement,
        &minecraft_directory,
        |detail| {
            let _ = events.unbounded_send(InstanceWorkerEvent::PreparingJava {
                id,
                detail: Some(detail),
            });
        },
    ) {
        Ok(resolution) => resolution,
        Err(message) => {
            let _ = events.unbounded_send(InstanceWorkerEvent::PreparingJava { id, detail: None });
            let _ = events.unbounded_send(InstanceWorkerEvent::LaunchFailed { id, message });
            return;
        }
    };
    let _ = events.unbounded_send(InstanceWorkerEvent::PreparingJava { id, detail: None });
    let java_executable = resolution.executable.clone();
    let java_probe = resolution.probe;

    let account = Account::offline(match player_name.trim() {
        name if is_valid_player_name(name) => name.to_owned(),
        // The persisted name is validated at save time; fall back to a
        // sensible default if it got corrupted or dropped on disk.
        _ => OFFLINE_PLAYER_NAME.to_owned(),
    });
    if let Err(message) =
        super::skins::local::prepare(&profile, &game_directory, account.username())
    {
        let _ = events.unbounded_send(InstanceWorkerEvent::LaunchFailed { id, message });
        return;
    }

    let quick_play_args = if let Some(world) = world {
        let result = (|| {
            let metadata = serde_json::to_value(&version).map_err(|error| error.to_string())?;
            let args = rs_mc_launcher_core::minecraft::quick_play::singleplayer_arguments(
                &metadata, &world,
            )
            .map_err(str::to_owned)?;
            crate::platform::worlds::validate(&game_directory.join("saves"), &world)
                .map_err(|error| format!("Не удалось открыть мир: {error}"))?;
            Ok::<_, String>(args)
        })();
        match result {
            Ok(args) => args,
            Err(message) => {
                let _ = events.unbounded_send(InstanceWorkerEvent::LaunchFailed { id, message });
                return;
            }
        }
    } else {
        Vec::new()
    };

    let command = match launcher.build_launch_command_from_version(
        &version,
        LaunchOptions {
            account,
            java_executable: Some(java_executable),
            game_directory: Some(game_directory),
            ..Default::default()
        },
    ) {
        Ok(command) => command,
        Err(error) => {
            let _ = events.unbounded_send(InstanceWorkerEvent::LaunchFailed {
                id,
                message: error.to_string(),
            });
            return;
        }
    };

    let max_memory_mb = profile.memory_mb.clamp(512, 16_384);
    let initial_memory_mb = (max_memory_mb / 4).clamp(512, 1024);
    let mut launch_args = vec![
        format!("-Xms{initial_memory_mb}M"),
        format!("-Xmx{max_memory_mb}M"),
    ];
    launch_args.extend(
        command
            .args
            .iter()
            .filter(|argument| !argument.starts_with("-Xms") && !argument.starts_with("-Xmx"))
            .cloned(),
    );

    launch_args.extend(quick_play_args);
    let mut process = Command::new(&command.executable);
    process
        .args(&launch_args)
        .current_dir(&command.working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in &command.env {
        process.env(key, value);
    }
    crate::platform::process::hide_console_window(&mut process);
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = events.unbounded_send(InstanceWorkerEvent::LaunchFailed {
                id,
                message: format!("Не удалось запустить Java: {error}"),
            });
            return;
        }
    };

    let _ = events.unbounded_send(InstanceWorkerEvent::Launched {
        id,
        process_id: child.id(),
    });

    let stdout_reader =
        match spawn_output_reader(child.stdout.take(), format!("read-minecraft-stdout-{id}")) {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = events.unbounded_send(InstanceWorkerEvent::LaunchFailed {
                    id,
                    message: format!("Не удалось прочитать вывод Java: {error}"),
                });
                return;
            }
        };
    let stderr_reader =
        match spawn_output_reader(child.stderr.take(), format!("read-minecraft-stderr-{id}")) {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(reader) = stdout_reader {
                    let _ = reader.join();
                }
                let _ = events.unbounded_send(InstanceWorkerEvent::LaunchFailed {
                    id,
                    message: format!("Не удалось прочитать вывод Java: {error}"),
                });
                return;
            }
        };

    match child.wait() {
        Ok(status) => {
            let stdout = stdout_reader
                .and_then(|reader| reader.join().ok())
                .filter(|stdout| !stdout.trim().is_empty());
            let stderr = stderr_reader
                .and_then(|reader| reader.join().ok())
                .filter(|stderr| !stderr.trim().is_empty());
            let launch_details = format!(
                "Java: {} (версия {})\nSTDOUT:\n{}\nSTDERR:\n{}",
                command.executable.display(),
                java_probe.version,
                stdout.unwrap_or_default().trim(),
                stderr.unwrap_or_default().trim(),
            );
            let _ = events.unbounded_send(InstanceWorkerEvent::GameExited {
                id,
                code: status.code(),
                stderr: Some(launch_details),
            });
        }
        Err(error) => {
            let _ = events.unbounded_send(InstanceWorkerEvent::LaunchFailed {
                id,
                message: format!("Не удалось дождаться завершения игры: {error}"),
            });
        }
    }
}

fn spawn_output_reader<R>(
    stream: Option<R>,
    name: String,
) -> std::io::Result<Option<std::thread::JoinHandle<String>>>
where
    R: Read + Send + 'static,
{
    stream
        .map(|stream| {
            std::thread::Builder::new()
                .name(name)
                .spawn(move || read_output_tail(stream))
        })
        .transpose()
}

fn read_output_tail(mut stream: impl Read) -> String {
    const MAX_CAPTURED_BYTES: usize = 6 * 1024;

    let mut captured = Vec::new();
    let mut buffer = [0; 2048];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(bytes_read) => {
                captured.extend_from_slice(&buffer[..bytes_read]);
                if captured.len() > MAX_CAPTURED_BYTES {
                    let excess = captured.len() - MAX_CAPTURED_BYTES;
                    captured.drain(..excess);
                }
            }
        }
    }

    String::from_utf8_lossy(&captured).into_owned()
}

pub fn remove_profile_data(profile: InstanceProfile, events: UnboundedSender<InstanceWorkerEvent>) {
    let id = profile.id;
    match crate::platform::profile_store::remove_instance_directory(&profile.installation_root, id)
    {
        Ok(()) => {
            let _ = events.unbounded_send(InstanceWorkerEvent::ProfileRemoved { id });
        }
        Err(error) => {
            let path =
                crate::platform::profile_store::instance_directory(&profile.installation_root, id);
            let _ = events.unbounded_send(InstanceWorkerEvent::ProfileRemovalFailed {
                id,
                message: format!("Не удалось удалить папку {}: {error}", path.display()),
            });
        }
    }
}

fn default_memory_mb() -> u32 {
    4096
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str) -> InstanceProfile {
        InstanceProfile::new(
            name.into(),
            "1.21".into(),
            LoaderChoice::Vanilla,
            None,
            PathBuf::new(),
        )
    }

    #[test]
    fn successful_launches_sort_home_without_reordering_library() {
        let first = profile("First");
        let second = profile("Second");
        let first_id = first.id;
        let second_id = second.id;
        let mut state = InstancesState::from_profiles(vec![first, second]);
        state.mark_launching(second_id);
        state.mark_failed(second_id, "Java missing".into());
        assert_eq!(state.recent_profiles()[0].id, first_id);
        state.mark_running(second_id, 42);
        assert_eq!(state.recent_profiles()[0].id, second_id);
        assert_eq!(state.profiles()[0].id, first_id);
        let saved = serde_json::to_string(state.profiles()).unwrap();
        let mut restored = InstancesState::from_profiles(serde_json::from_str(&saved).unwrap());
        assert_eq!(restored.recent_profiles()[0].id, second_id);
        restored.mark_running(first_id, 43);
        assert_eq!(restored.recent_profiles()[0].id, first_id);
        restored.remove(first_id);
        assert_eq!(restored.recent_profiles()[0].id, second_id);
    }

    #[test]
    fn old_profiles_keep_their_order_until_launched() {
        let mut old = serde_json::to_value(profile("Old")).unwrap();
        old.as_object_mut().unwrap().remove("last_launch_order");
        let old: InstanceProfile = serde_json::from_value(old).unwrap();
        assert_eq!(old.last_launch_order, 0);
        let state = InstancesState::from_profiles(vec![old, profile("New")]);
        assert_eq!(state.recent_profiles()[0].name, "Old");
    }

    #[test]
    fn player_names_follow_vanilla_rules() {
        assert!(is_valid_player_name("Player"));
        assert!(is_valid_player_name("a_b_999"));
        assert!(is_valid_player_name(" Steve "));
        assert!(is_valid_player_name("abc"));
        assert!(is_valid_player_name("0123456789abcdef"));
        assert!(!is_valid_player_name("ab"));
        assert!(!is_valid_player_name("0123456789abcdefg"));
        assert!(!is_valid_player_name(""));
        assert!(!is_valid_player_name("И в нике"));
        assert!(!is_valid_player_name("with-dash"));
    }
}
