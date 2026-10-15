//! A read-only summary of active transfers, derived from their owning feature state.
use super::instances::{InstanceRuntime, InstancesState};

#[derive(Debug)]
pub struct DownloadSummary {
    pub active_count: usize,
    pub profile_name: String,
    /// Bytes downloaded by the current install plan.
    pub received: u64,
    /// Total bytes of the install plan, when known.
    pub total_bytes: Option<u64>,
    /// Smoothed download speed in bytes per second.
    pub speed_bps: f64,
}

impl DownloadSummary {
    pub fn from_instances(instances: &InstancesState) -> Option<Self> {
        let mut active = instances.profiles().iter().filter_map(|profile| {
            let (received, total_bytes, speed_bps) = match instances.runtime(&profile.id) {
                InstanceRuntime::Installing {
                    received,
                    total_bytes,
                    speed_bps,
                    ..
                } => (received, total_bytes, speed_bps),
                InstanceRuntime::DownloadingMod { .. } => (0, None, 0.),
                _ => return None,
            };
            Some(Self {
                active_count: 1,
                profile_name: profile.name.clone(),
                received,
                total_bytes: total_bytes.filter(|total| *total > 0),
                speed_bps,
            })
        });
        let mut summary = active.next()?;
        summary.active_count += active.count();
        if summary.active_count > 1 {
            summary.received = 0;
            summary.total_bytes = None;
            summary.speed_bps = 0.;
        }
        Some(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::instances::{InstallProgressUpdate, InstanceProfile};
    use crate::features::versions::LoaderChoice;
    use std::path::PathBuf;

    fn state() -> InstancesState {
        InstancesState::from_profiles(vec![
            InstanceProfile::new(
                "First".into(),
                "1.21".into(),
                LoaderChoice::Vanilla,
                None,
                PathBuf::new(),
            ),
            InstanceProfile::new(
                "Second".into(),
                "1.21".into(),
                LoaderChoice::Vanilla,
                None,
                PathBuf::new(),
            ),
        ])
    }

    #[test]
    fn tracks_active_jobs_and_clears_after_success_or_failure() {
        let mut state = state();
        let first = state.profiles()[0].id;
        let second = state.profiles()[1].id;
        assert!(DownloadSummary::from_instances(&state).is_none());
        state.mark_installing(first);
        state.update_progress(
            first,
            InstallProgressUpdate::PlanStarted {
                tasks: 2,
                total_bytes: Some(100),
            },
        );
        state.update_progress(
            first,
            InstallProgressUpdate::PlanProgress {
                downloaded: 25,
                total_bytes: Some(100),
            },
        );
        let summary = DownloadSummary::from_instances(&state).unwrap();
        assert_eq!(summary.received, 25);
        assert_eq!(summary.total_bytes, Some(100));
        state.mark_mod_downloading(second, "Sodium".into());
        let summary = DownloadSummary::from_instances(&state).unwrap();
        assert_eq!(summary.active_count, 2);
        assert_eq!(summary.received, 0);
        state.mark_installed(first, "1.21".into());
        let summary = DownloadSummary::from_instances(&state).unwrap();
        assert_eq!(summary.active_count, 1);
        state.mark_mod_download_failed(second, "Offline".into());
        assert!(DownloadSummary::from_instances(&state).is_none());
    }

    #[test]
    fn plan_progress_smooths_speed_from_byte_deltas() {
        let mut state = state();
        let id = state.profiles()[0].id;
        state.mark_installing(id);
        state.update_progress(
            id,
            InstallProgressUpdate::PlanStarted {
                tasks: 1,
                total_bytes: Some(1_000),
            },
        );
        state.update_progress(
            id,
            InstallProgressUpdate::PlanProgress {
                downloaded: 100,
                total_bytes: Some(1_000),
            },
        );
        let InstanceRuntime::Installing {
            received,
            speed_bps,
            ..
        } = state.runtime(&id)
        else {
            panic!("expected installing runtime");
        };
        assert_eq!(received, 100);
        // The first sample has no previous point, so speed stays zero.
        assert_eq!(speed_bps, 0.);
    }

    #[test]
    fn task_done_events_count_completed_files() {
        let mut state = state();
        let id = state.profiles()[0].id;
        state.mark_installing(id);
        state.update_progress(id, InstallProgressUpdate::TaskDone);
        state.update_progress(id, InstallProgressUpdate::TaskDone);
        let InstanceRuntime::Installing {
            completed_tasks, ..
        } = state.runtime(&id)
        else {
            panic!("expected installing runtime");
        };
        assert_eq!(completed_tasks, 2);
    }
}
