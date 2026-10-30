//! Worker threads that install, launch and remove builds. They talk to the
//! UI only through [`InstanceWorkerEvent`]s.

use crate::features::auth::{self, PlayerAccount};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use futures::channel::mpsc::UnboundedSender;
use mc_launcher_core::LauncherError;
use mc_launcher_core::prelude::{
    Account, DownloadControl, InstallRequest, JavaInstallPolicy, LaunchOptions, Launcher,
    LoaderSpec, LoaderVersion, ProgressEvent,
};

use super::super::java;
use super::{InstanceId, InstanceProfile, OFFLINE_PLAYER_NAME, is_valid_player_name};
use crate::features::versions::LoaderChoice;
use crate::platform::instance_layout;
use orbmc_core::minecraft::loaders::forge_version_id;

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
        id: InstanceId,
        update: InstallProgressUpdate,
    },
    Installed {
        id: InstanceId,
        version_id: String,
    },
    InstallFailed {
        id: InstanceId,
        message: String,
    },
    Launched {
        id: InstanceId,
        process_id: u32,
    },
    /// Java for the launch is being resolved or downloaded; `None` ends the
    /// preparation phase.
    PreparingJava {
        id: InstanceId,
        detail: Option<String>,
    },
    /// One line the game printed to stdout or stderr.
    Output {
        id: InstanceId,
        line: String,
    },
    GameExited {
        id: InstanceId,
        code: Option<i32>,
        stderr: Option<String>,
    },
    LaunchFailed {
        id: InstanceId,
        message: String,
    },
    ProfileRemoved {
        id: InstanceId,
    },
    ProfileRemovalFailed {
        id: InstanceId,
        message: String,
    },
}

/// Why a build could not be installed. The messages are shown to the user.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("Для выбранного загрузчика не указана версия")]
    MissingLoaderVersion,
    #[error("Не удалось получить метаданные Minecraft {version}: {source}")]
    Metadata {
        version: String,
        #[source]
        source: LauncherError,
    },
    #[error(transparent)]
    Java(#[from] java::JavaError),
    #[error("Установка отменена")]
    Cancelled,
    #[error(transparent)]
    Engine(LauncherError),
}

/// Why a build could not be started or watched. The messages are shown to
/// the user.
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("Профиль ещё не установлен")]
    NotInstalled,
    #[error("Не удалось создать папку профиля {}: {source}", path.display())]
    GameDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Version(LauncherError),
    #[error(transparent)]
    Java(#[from] java::JavaError),
    #[error("{0}")]
    QuickPlay(String),
    #[error("Не удалось открыть мир: {0}")]
    World(#[source] std::io::Error),
    #[error(transparent)]
    Command(LauncherError),
    #[error(transparent)]
    Account(#[from] auth::AuthError),
    #[error("Не удалось запустить Java: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("Не удалось прочитать вывод Java: {0}")]
    Output(#[source] std::io::Error),
    #[error("Не удалось дождаться завершения игры: {0}")]
    Wait(#[source] std::io::Error),
}

pub fn install_profile(
    profile: InstanceProfile,
    minecraft_directory: PathBuf,
    control: DownloadControl,
    events: UnboundedSender<InstanceWorkerEvent>,
) {
    let id = profile.id;
    let event = match install(profile, minecraft_directory, &control, &events) {
        Ok(version_id) => InstanceWorkerEvent::Installed { id, version_id },
        Err(error) => {
            tracing::error!(profile_id = %id, %error, "Minecraft install failed");
            InstanceWorkerEvent::InstallFailed {
                id,
                message: error.to_string(),
            }
        }
    };
    let _ = events.unbounded_send(event);
}

/// Installs the game and its loader; returns the installed version id.
fn install(
    profile: InstanceProfile,
    minecraft_directory: PathBuf,
    control: &DownloadControl,
    events: &UnboundedSender<InstanceWorkerEvent>,
) -> Result<String, InstallError> {
    let id = profile.id;
    let loader = match profile.loader {
        LoaderChoice::Vanilla => None,
        LoaderChoice::Fabric => Some(LoaderSpec::Fabric {
            version: LoaderVersion::Exact(
                profile
                    .loader_version
                    .clone()
                    .ok_or(InstallError::MissingLoaderVersion)?,
            ),
        }),
        LoaderChoice::Forge => Some(LoaderSpec::Forge {
            version: LoaderVersion::Exact(forge_version_id(
                &profile.game_version,
                profile
                    .loader_version
                    .as_deref()
                    .ok_or(InstallError::MissingLoaderVersion)?,
            )),
        }),
        LoaderChoice::NeoForge => Some(LoaderSpec::NeoForge {
            version: LoaderVersion::Exact(
                profile
                    .loader_version
                    .clone()
                    .ok_or(InstallError::MissingLoaderVersion)?,
            ),
        }),
    };

    let vanilla = mc_launcher_core::install::client::fetch_vanilla_version(&profile.game_version)
        .map_err(|source| InstallError::Metadata {
        version: profile.game_version.clone(),
        source,
    })?;
    let requirement = java::requirement_for(&vanilla);
    let resolution = java::resolve_java(
        profile.java_path.as_deref(),
        &requirement,
        &minecraft_directory,
        |detail| {
            let _ = events.unbounded_send(InstanceWorkerEvent::Progress {
                id,
                update: InstallProgressUpdate::Stage(detail),
            });
        },
    )?;

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

    launcher
        .install_with_java_progress(request, Some(resolution.executable), &mut reporter, control)
        .map(|installed| installed.version_id)
        .map_err(|error| match error {
            LauncherError::InstallCancelled => InstallError::Cancelled,
            error => InstallError::Engine(error),
        })
}

pub fn launch_profile(
    profile: InstanceProfile,
    minecraft_directory: PathBuf,
    world: Option<String>,
    player: PlayerAccount,
    events: UnboundedSender<InstanceWorkerEvent>,
) {
    let id = profile.id;
    if let Err(error) = launch(profile, minecraft_directory, world, &player, &events) {
        tracing::warn!(profile_id = %id, %error, "Minecraft launch failed");
        let _ = events.unbounded_send(InstanceWorkerEvent::LaunchFailed {
            id,
            message: error.to_string(),
        });
    }
}

/// Starts the game and waits for it; reports `Launched` and `GameExited`
/// along the way.
fn launch(
    profile: InstanceProfile,
    minecraft_directory: PathBuf,
    world: Option<String>,
    player: &PlayerAccount,
    events: &UnboundedSender<InstanceWorkerEvent>,
) -> Result<(), LaunchError> {
    let id = profile.id;
    let version_id = profile
        .installed_version_id
        .clone()
        .ok_or(LaunchError::NotInstalled)?;

    let game_directory = profile.game_directory();
    let data_directory = profile
        .data_directory()
        .and_then(|data| {
            instance_layout::ensure_directory(&game_directory)?;
            instance_layout::ensure_directory(&instance_layout::jvm_log_directory(&data))?;
            Ok(data)
        })
        .map_err(|source| LaunchError::GameDirectory {
            path: game_directory.clone(),
            source,
        })?;

    let launcher = Launcher::new(minecraft_directory.clone());
    let version = launcher
        .load_version(&version_id)
        .map_err(LaunchError::Version)?;

    let requirement = java::requirement_for(&version);
    let resolution = java::resolve_java(
        profile.java_path.as_deref(),
        &requirement,
        &minecraft_directory,
        |detail| {
            let _ = events.unbounded_send(InstanceWorkerEvent::PreparingJava {
                id,
                detail: Some(detail),
            });
        },
    );
    let _ = events.unbounded_send(InstanceWorkerEvent::PreparingJava { id, detail: None });
    let resolution = resolution?;
    let java_executable = resolution.executable.clone();
    let java_probe = resolution.probe;

    let (account, jvm_options) = match player {
        PlayerAccount::Offline { name } => {
            let account = Account::offline(match name.trim() {
                name if is_valid_player_name(name) => name.to_owned(),
                // The persisted name is validated at save time; fall back to a
                // sensible default if it got corrupted or dropped on disk.
                _ => OFFLINE_PLAYER_NAME.to_owned(),
            });
            (account, Vec::new())
        }
        PlayerAccount::ElyBy(account) => {
            let session = auth::launch_session(account)?;
            let injector = auth::authlib_injector()?;
            // The engine has no Yggdrasil account kind; the Microsoft one
            // carries the same three launch values.
            let account = Account::Microsoft {
                username: session.account.name,
                uuid: session.account.uuid,
                access_token: session.access_token,
            };
            let agent = format!("-javaagent:{}={}", injector.display(), auth::ELY_BY_API);
            (account, vec![agent])
        }
    };

    let quick_play_args = match world {
        Some(world) => {
            let metadata = serde_json::to_value(&version)
                .map_err(|error| LaunchError::QuickPlay(error.to_string()))?;
            let args = orbmc_core::minecraft::quick_play::singleplayer_arguments(&metadata, &world)
                .map_err(|message| LaunchError::QuickPlay(message.to_owned()))?;
            crate::platform::worlds::validate(&game_directory.join("saves"), &world)
                .map_err(LaunchError::World)?;
            args
        }
        None => Vec::new(),
    };

    let command = launcher
        .build_launch_command_from_version(
            &version,
            LaunchOptions {
                account,
                java_executable: Some(java_executable),
                game_directory: Some(game_directory.clone()),
                ..Default::default()
            },
        )
        .map_err(LaunchError::Command)?;

    // JVM crash dumps go to the build data directory, not the game folder.
    let launch_args = orbmc_core::minecraft::launch::launch_arguments(
        &command.args,
        profile.memory_mb,
        &instance_layout::jvm_log_directory(&data_directory),
        jvm_options,
        quick_play_args,
    );
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
    instance_layout::restore_game_caches(&data_directory, &game_directory);
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            instance_layout::stash_game_caches(&game_directory, &data_directory);
            return Err(LaunchError::Spawn(error));
        }
    };

    let _ = events.unbounded_send(InstanceWorkerEvent::Launched {
        id,
        process_id: child.id(),
    });

    let readers = spawn_output_reader(
        child.stdout.take(),
        format!("read-minecraft-stdout-{id}"),
        id,
        events.clone(),
    )
    .and_then(|stdout| {
        spawn_output_reader(
            child.stderr.take(),
            format!("read-minecraft-stderr-{id}"),
            id,
            events.clone(),
        )
        .map(|stderr| (stdout, stderr))
    });
    let (stdout_reader, stderr_reader) = match readers {
        Ok(readers) => readers,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            instance_layout::stash_game_caches(&game_directory, &data_directory);
            return Err(LaunchError::Output(error));
        }
    };

    let exit = child.wait();
    instance_layout::stash_game_caches(&game_directory, &data_directory);
    let status = exit.map_err(LaunchError::Wait)?;
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
    Ok(())
}

fn spawn_output_reader<R>(
    stream: Option<R>,
    name: String,
    id: InstanceId,
    events: UnboundedSender<InstanceWorkerEvent>,
) -> std::io::Result<Option<std::thread::JoinHandle<String>>>
where
    R: Read + Send + 'static,
{
    stream
        .map(|stream| {
            std::thread::Builder::new().name(name).spawn(move || {
                read_output(stream, |line| {
                    let _ = events.unbounded_send(InstanceWorkerEvent::Output { id, line });
                })
            })
        })
        .transpose()
}

/// Passes every line of `stream` to `on_line` and returns the last few
/// kilobytes, which go into the crash message.
fn read_output(stream: impl Read, mut on_line: impl FnMut(String)) -> String {
    const MAX_CAPTURED_BYTES: usize = 6 * 1024;

    let mut reader = std::io::BufReader::new(stream);
    let mut captured = Vec::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        match std::io::BufRead::read_until(&mut reader, b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                captured.extend_from_slice(&line);
                if captured.len() > MAX_CAPTURED_BYTES {
                    let excess = captured.len() - MAX_CAPTURED_BYTES;
                    captured.drain(..excess);
                }
                on_line(String::from_utf8_lossy(&line).into_owned());
            }
        }
    }

    String::from_utf8_lossy(&captured).into_owned()
}

pub fn remove_profile_data(profile: InstanceProfile, events: UnboundedSender<InstanceWorkerEvent>) {
    let id = profile.id;
    let game_directory = profile.game_directory();
    match instance_layout::remove_directory(&game_directory) {
        Ok(()) => {
            match profile
                .data_directory()
                .and_then(|data| instance_layout::remove_directory(&data))
            {
                Ok(()) => {}
                // The build itself is gone; stale bookkeeping is harmless.
                Err(error) => {
                    tracing::warn!(profile_id = %id, %error, "failed to remove build data directory")
                }
            }
            let _ = events.unbounded_send(InstanceWorkerEvent::ProfileRemoved { id });
        }
        Err(error) => {
            let _ = events.unbounded_send(InstanceWorkerEvent::ProfileRemovalFailed {
                id,
                message: format!(
                    "Не удалось удалить папку {}: {error}",
                    game_directory.display()
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_read_as_user_messages() {
        assert_eq!(InstallError::Cancelled.to_string(), "Установка отменена");
        assert_eq!(
            LaunchError::NotInstalled.to_string(),
            "Профиль ещё не установлен"
        );
        let error = LaunchError::GameDirectory {
            path: PathBuf::from("builds/Test"),
            source: std::io::Error::other("denied"),
        };
        assert!(error.to_string().ends_with(": denied"));
    }

    #[test]
    fn output_is_split_into_lines_and_the_tail_is_kept() {
        let mut lines = Vec::new();
        let tail = read_output("first\nsecond\r\nthird".as_bytes(), |line| lines.push(line));
        assert_eq!(lines, ["first\n", "second\r\n", "third"]);
        assert_eq!(tail, "first\nsecond\r\nthird");
    }
}
