//! Runtime state of all builds: install progress, launches, removals.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use mc_launcher_core::prelude::DownloadControl;

use crate::features::downloads::TransferHandle;

use super::layout::allocate_directory;
use super::{InstallProgressUpdate, InstanceId, InstanceProfile};

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
        transfer: TransferHandle,
    },
    DownloadingContent {
        name: String,
        transfer: TransferHandle,
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
    pub(super) profiles: Vec<InstanceProfile>,
    runtime: HashMap<InstanceId, InstanceRuntime>,
    worlds: HashMap<InstanceId, crate::features::worlds::WorldList>,
    /// Pause and cancel handles for running installs.
    controls: HashMap<InstanceId, DownloadControl>,
    /// Last speed sample per running install.
    samples: HashMap<InstanceId, ProgressSample>,
    /// Output of the latest game session per build.
    logs: HashMap<InstanceId, super::game_log::GameLog>,
    /// When the running game of a build started, for its play time.
    sessions: HashMap<InstanceId, Instant>,
}

/// One speed measurement point for an install: the anchor from which the
/// next speed window is measured.
#[derive(Clone, Copy, Debug)]
struct ProgressSample {
    at: Instant,
    downloaded: u64,
}

/// Minimum span between accepted speed measurements. Progress events arrive
/// every ~150 ms, so measuring over a longer window keeps the estimate stable
/// instead of chasing per-event bursts.
const SPEED_MEASURE_INTERVAL: Duration = Duration::from_millis(750);

/// Exponential smoothing factor for download speed, applied per window.
const SPEED_SMOOTHING: f64 = 0.35;

/// Measures the download speed across one window when the previous sample is
/// old enough, using the cumulative plan counter so bytes from interim events
/// are always included.
fn measure_speed(sample: &ProgressSample, downloaded: u64, now: Instant) -> Option<f64> {
    let elapsed = now.duration_since(sample.at);
    if elapsed < SPEED_MEASURE_INTERVAL {
        return None;
    }
    Some(downloaded.saturating_sub(sample.downloaded) as f64 / elapsed.as_secs_f64())
}

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
            logs: HashMap::new(),
            sessions: HashMap::new(),
        }
    }

    /// Play time of a build, the running session included.
    pub fn play_time(&self, id: InstanceId) -> Duration {
        let saved = self.profile(id).map_or(0, |profile| profile.play_time_secs);
        let current = self
            .sessions
            .get(&id)
            .map_or(Duration::ZERO, Instant::elapsed);
        Duration::from_secs(saved) + current
    }

    pub fn game_log(&self, id: InstanceId) -> Option<&super::game_log::GameLog> {
        self.logs.get(&id)
    }

    pub fn push_log_line(&mut self, id: InstanceId, line: String) {
        self.logs.entry(id).or_default().push(line);
    }

    pub fn clear_game_log(&mut self, id: InstanceId) {
        if let Some(log) = self.logs.get_mut(&id) {
            log.clear();
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

    pub fn profile(&self, id: InstanceId) -> Option<&InstanceProfile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    pub fn worlds(&self, id: InstanceId) -> crate::features::worlds::WorldList {
        self.worlds
            .get(&id)
            .cloned()
            .unwrap_or(crate::features::worlds::WorldList::Loading)
    }

    pub fn set_worlds(&mut self, id: InstanceId, worlds: crate::features::worlds::WorldList) {
        if self.profile(id).is_some() {
            self.worlds.insert(id, worlds);
        }
    }

    pub fn remove(&mut self, id: InstanceId) -> Option<InstanceProfile> {
        let index = self.profiles.iter().position(|profile| profile.id == id)?;
        self.runtime.remove(&id);
        self.worlds.remove(&id);
        self.controls.remove(&id);
        self.samples.remove(&id);
        self.logs.remove(&id);
        Some(self.profiles.remove(index))
    }

    pub fn runtime(&self, id: &InstanceId) -> InstanceRuntime {
        self.runtime.get(id).cloned().unwrap_or_default()
    }

    pub fn add(&mut self, mut profile: InstanceProfile) {
        if profile.directory.is_empty() {
            profile.directory = allocate_directory(&self.profiles, &profile);
        }
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

    /// Stops a mod or content download of a build; `false` when none runs.
    pub fn cancel_transfer(&mut self, id: InstanceId) -> bool {
        match self.runtime.get(&id) {
            Some(
                InstanceRuntime::DownloadingMod { transfer, .. }
                | InstanceRuntime::DownloadingContent { transfer, .. },
            ) => {
                transfer.cancel();
                true
            }
            _ => false,
        }
    }

    pub fn mark_installing(&mut self, id: InstanceId) {
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
    pub fn set_install_control(&mut self, id: InstanceId, control: DownloadControl) {
        self.controls.insert(id, control);
    }

    /// Drops the install control and progress sample once work ends.
    pub fn finish_install(&mut self, id: InstanceId) {
        self.controls.remove(&id);
        self.samples.remove(&id);
    }

    /// Pauses the install through its control handle.
    pub fn pause_install(&mut self, id: InstanceId) -> bool {
        self.mutate_install_control(id, |control| {
            control.pause();
            true
        })
    }

    /// Resumes a paused install.
    pub fn resume_install(&mut self, id: InstanceId) -> bool {
        self.mutate_install_control(id, |control| {
            control.resume();
            true
        })
    }

    /// Cancels the install; the worker reports the outcome as a failure.
    pub fn cancel_install(&mut self, id: InstanceId) -> bool {
        self.mutate_install_control(id, |control| {
            control.cancel();
            true
        })
    }

    fn mutate_install_control(
        &mut self,
        id: InstanceId,
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
    ///
    /// Pausing hides the stale speed and restarts the measurement window, so
    /// the readout resumes only from freshly measured bytes.
    fn mark_paused_state(&mut self, id: InstanceId) {
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
        self.samples.remove(&id);
        self.runtime.insert(
            id,
            InstanceRuntime::Installing {
                stage,
                received,
                total_bytes,
                speed_bps: if paused { 0. } else { speed_bps },
                completed_tasks,
                total_tasks,
                paused,
            },
        );
    }

    pub fn mark_mod_downloading(&mut self, id: InstanceId, name: String, transfer: TransferHandle) {
        self.runtime
            .insert(id, InstanceRuntime::DownloadingMod { name, transfer });
    }

    pub fn mark_mod_downloaded(&mut self, id: InstanceId, filename: String) {
        self.runtime
            .insert(id, InstanceRuntime::ModDownloaded { filename });
    }

    pub fn mark_mod_download_failed(&mut self, id: InstanceId, message: String) {
        self.runtime
            .insert(id, InstanceRuntime::ModDownloadFailed { message });
    }

    pub fn mark_content_downloading(
        &mut self,
        id: InstanceId,
        name: String,
        transfer: TransferHandle,
    ) {
        self.runtime
            .insert(id, InstanceRuntime::DownloadingContent { name, transfer });
    }

    pub fn mark_content_downloaded(&mut self, id: InstanceId, filename: String) {
        self.runtime
            .insert(id, InstanceRuntime::ContentDownloaded { filename });
    }

    pub fn mark_content_download_failed(&mut self, id: InstanceId, message: String) {
        self.runtime
            .insert(id, InstanceRuntime::ContentDownloadFailed { message });
    }

    /// Applies a vendor progress event to the runtime state.
    pub fn update_progress(&mut self, id: InstanceId, update: InstallProgressUpdate) {
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
                // Stages without download progress (Verify, extract, Java)
                // must not inherit the previous stage's speed readout.
                self.samples.remove(&id);
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
                let anchored = self.samples.contains_key(&id);
                let measured_speed = self
                    .samples
                    .get(&id)
                    .and_then(|sample| measure_speed(sample, downloaded, now));
                // Interim events with a live anchor leave the sample in
                // place; their bytes add up inside the next window because
                // the plan counter is cumulative.
                if measured_speed.is_some() || !anchored {
                    self.samples.insert(
                        id,
                        ProgressSample {
                            at: now,
                            downloaded,
                        },
                    );
                }
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

    pub fn mark_installed(&mut self, id: InstanceId, version_id: String) {
        if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == id) {
            profile.installed_version_id = Some(version_id);
        }
        self.runtime.insert(id, InstanceRuntime::Ready);
    }

    pub fn set_memory_mb(&mut self, id: InstanceId, memory_mb: u32) {
        if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == id) {
            let (min, max) = orbmc_core::minecraft::launch::MEMORY_LIMITS_MB;
            profile.memory_mb = memory_mb.clamp(min, max);
        }
    }

    pub fn set_jvm_arguments(&mut self, id: InstanceId, arguments: String) {
        if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == id) {
            profile.jvm_arguments = arguments.trim().to_owned();
        }
    }

    /// Sets the game and loader of a build created from a modpack once the
    /// pack's own index is known.
    pub fn set_game(
        &mut self,
        id: InstanceId,
        game_version: String,
        loader: crate::features::versions::LoaderChoice,
        loader_version: Option<String>,
    ) {
        if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == id) {
            profile.game_version = game_version;
            profile.loader = loader;
            profile.loader_version = loader_version;
        }
    }

    pub fn set_java_path(&mut self, id: InstanceId, java_path: Option<PathBuf>) {
        if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == id) {
            profile.java_path = java_path;
        }
    }

    pub fn mark_failed(&mut self, id: InstanceId, message: String) {
        self.runtime.insert(id, InstanceRuntime::Failed { message });
    }

    pub fn mark_removing(&mut self, id: InstanceId) {
        self.runtime.insert(id, InstanceRuntime::Removing);
    }

    pub fn mark_removal_failed(&mut self, id: InstanceId, message: String) {
        self.runtime
            .insert(id, InstanceRuntime::RemovalFailed { message });
    }

    pub fn mark_launching(&mut self, id: InstanceId) {
        self.runtime.insert(id, InstanceRuntime::Launching);
        // A new session starts a new log.
        self.clear_game_log(id);
    }

    /// Shows Java preparation progress during launch; `None` returns the
    /// profile to the `Launching` state.
    pub fn mark_preparing_java(&mut self, id: InstanceId, detail: Option<String>) {
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

    pub fn mark_running(&mut self, id: InstanceId, process_id: u32) {
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
        self.sessions.insert(id, Instant::now());
    }

    pub fn mark_game_stopped(
        &mut self,
        id: InstanceId,
        exit_code: Option<i32>,
        stderr: Option<String>,
    ) {
        if let Some(started) = self.sessions.remove(&id)
            && let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == id)
        {
            profile.play_time_secs = profile
                .play_time_secs
                .saturating_add(started.elapsed().as_secs());
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::versions::LoaderChoice;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn profile(name: &str) -> InstanceProfile {
        InstanceProfile::new(
            name.into(),
            "1.21".into(),
            LoaderChoice::Vanilla,
            None,
            PathBuf::new(),
        )
    }

    fn installing() -> (InstancesState, InstanceId) {
        let mut state = InstancesState::from_profiles(vec![profile("First")]);
        let id = state.profiles()[0].id;
        state.mark_installing(id);
        (state, id)
    }

    fn installing_speed(state: &InstancesState, id: InstanceId) -> (u64, f64) {
        let InstanceRuntime::Installing {
            received,
            speed_bps,
            ..
        } = state.runtime(&id)
        else {
            panic!("expected installing runtime");
        };
        (received, speed_bps)
    }

    #[test]
    fn measure_speed_requires_the_minimum_window() {
        let now = Instant::now();
        let fresh = ProgressSample {
            at: now,
            downloaded: 0,
        };
        assert_eq!(measure_speed(&fresh, 600, now), None);

        // A window with no downloaded bytes measures zero, not None.
        let stale = ProgressSample {
            at: now - Duration::from_secs(2),
            downloaded: 0,
        };
        assert_eq!(measure_speed(&stale, 0, now).unwrap(), 0.);
    }

    #[test]
    fn measure_speed_uses_the_cumulative_delta_across_the_window() {
        let now = Instant::now();
        let sample = ProgressSample {
            at: now - Duration::from_secs(1),
            downloaded: 100,
        };
        let measured = measure_speed(&sample, 1_100, now).unwrap();
        // Real elapsed is the full window plus scheduling noise, so the rate
        // can only be equal to or slightly lower than the ideal value.
        assert!(
            (400.0..=1000.0).contains(&measured),
            "expected ≈1000 B/s, got {measured}"
        );
        // Non-monotonic counters cannot inflate the speed.
        let measured = measure_speed(&sample, 50, now).unwrap();
        assert_eq!(measured, 0.);
    }

    #[test]
    fn rapid_progress_keeps_the_window_anchor() {
        let (mut state, id) = installing();
        state.samples.insert(
            id,
            ProgressSample {
                at: Instant::now(),
                downloaded: 100,
            },
        );

        state.update_progress(
            id,
            InstallProgressUpdate::PlanProgress {
                downloaded: 220,
                total_bytes: Some(1_000),
            },
        );
        state.update_progress(
            id,
            InstallProgressUpdate::PlanProgress {
                downloaded: 340,
                total_bytes: Some(1_000),
            },
        );

        // No window has elapsed yet, so the anchor and the speed survive.
        let (received, speed_bps) = installing_speed(&state, id);
        assert_eq!(received, 340);
        assert_eq!(speed_bps, 0.);
        let anchor = state.samples.get(&id).copied().unwrap();
        assert_eq!(anchor.downloaded, 100);
    }

    #[test]
    fn elapsed_windows_seed_the_speed_from_cumulative_bytes() {
        let (mut state, id) = installing();
        let sample_at = Instant::now() - Duration::from_millis(1_500);
        state.samples.insert(
            id,
            ProgressSample {
                at: sample_at,
                downloaded: 100,
            },
        );

        state.update_progress(
            id,
            InstallProgressUpdate::PlanProgress {
                downloaded: 1_600,
                total_bytes: Some(4_000),
            },
        );

        let (received, speed_bps) = installing_speed(&state, id);
        assert_eq!(received, 1_600);
        assert!(
            (400.0..=1000.0).contains(&speed_bps),
            "expected ≈1000 B/s, got {speed_bps}"
        );
        // The accepted measurement adopts a fresh anchor.
        let anchor = state.samples.get(&id).copied().unwrap();
        assert_eq!(anchor.downloaded, 1_600);
        assert!(anchor.at > sample_at);
    }

    #[test]
    fn stage_changes_restart_the_speed_window() {
        let (mut state, id) = installing();
        state.samples.insert(
            id,
            ProgressSample {
                at: Instant::now(),
                downloaded: 100,
            },
        );

        state.update_progress(id, InstallProgressUpdate::Stage("Verify".to_owned()));
        assert!(!state.samples.contains_key(&id));

        state.update_progress(
            id,
            InstallProgressUpdate::PlanProgress {
                downloaded: 380,
                total_bytes: Some(1_000),
            },
        );
        // The stale anchor is gone, so this event only reanchors.
        let anchor = state.samples.get(&id).copied().unwrap();
        assert_eq!(anchor.downloaded, 380);
        let (_, speed_bps) = installing_speed(&state, id);
        assert_eq!(speed_bps, 0.);
    }

    #[test]
    fn pausing_hides_speed_and_restarts_the_window() {
        let (mut state, id) = installing();
        state.set_install_control(id, DownloadControl::new());
        state.samples.insert(
            id,
            ProgressSample {
                at: Instant::now(),
                downloaded: 100,
            },
        );
        state.update_progress(
            id,
            InstallProgressUpdate::PlanProgress {
                downloaded: 100,
                total_bytes: Some(1_000),
            },
        );

        assert!(state.pause_install(id));
        let (_, speed_bps) = installing_speed(&state, id);
        assert_eq!(speed_bps, 0.);
        assert!(!state.samples.contains_key(&id));
        let InstanceRuntime::Installing {
            paused, received, ..
        } = state.runtime(&id)
        else {
            panic!("expected installing runtime");
        };
        assert!(paused);
        assert_eq!(received, 100);

        // Resuming reanchors from scratch instead of measuring across the pause.
        assert!(state.resume_install(id));
        assert!(!state.samples.contains_key(&id));
        state.update_progress(
            id,
            InstallProgressUpdate::PlanProgress {
                downloaded: 150,
                total_bytes: Some(1_000),
            },
        );
        assert!(state.samples.contains_key(&id));
    }

    #[test]
    fn plans_restart_the_speed_window() {
        let (mut state, id) = installing();
        state.samples.insert(
            id,
            ProgressSample {
                at: Instant::now(),
                downloaded: 100,
            },
        );
        state.update_progress(
            id,
            InstallProgressUpdate::PlanStarted {
                tasks: 2,
                total_bytes: Some(500),
            },
        );
        assert!(!state.samples.contains_key(&id));
        let (received, speed_bps) = installing_speed(&state, id);
        assert_eq!(received, 0);
        assert_eq!(speed_bps, 0.);
    }
}
