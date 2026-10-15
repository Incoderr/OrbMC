//! Progress events emitted while installation work is performed.
//!
//! Callers can pass a closure to
//! [`crate::launcher::Launcher::install_with_progress`] because closures that
//! accept [`ProgressEvent`] automatically implement [`ProgressReporter`].

use std::path::PathBuf;

/// Coarse install stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallStage {
    /// Resolve version metadata.
    ResolveVersion,
    /// Download libraries.
    DownloadLibraries,
    /// Download asset index and objects.
    DownloadAssets,
    /// Install a Java runtime.
    InstallRuntime,
    /// Extract native libraries.
    ExtractNatives,
    /// Run or write loader installation metadata.
    LoaderInstall,
    /// Verify installed files.
    Verify,
}

/// Reason a download task was skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Existing file matched the expected checksum.
    ChecksumMatched,
    /// Existing file was accepted because no checksum was available.
    FileExistsWithoutChecksum,
}

/// Event emitted during install and download operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressEvent {
    /// A coarse install stage started.
    StageStarted {
        /// Stage that started.
        stage: InstallStage,
    },
    /// A download plan started.
    ///
    /// `tasks` counts the files that are expected to download; files already
    /// present with valid checksums are skipped without being counted.
    PlanStarted {
        /// Number of tasks the plan will execute.
        tasks: usize,
        /// Summed expected size of the pending tasks, when every task declared
        /// its size.
        total_bytes: Option<u64>,
    },
    /// Aggregated byte progress across the tasks of a plan.
    ///
    /// Emitted periodically while downloads run; `downloaded` counts the
    /// bytes received since the plan started.
    PlanProgress {
        /// Bytes downloaded so far in this plan.
        downloaded: u64,
        /// Same total as reported by [`ProgressEvent::PlanStarted`].
        total_bytes: Option<u64>,
    },
    /// A concrete file task started.
    TaskStarted {
        /// Human-readable task label.
        label: String,
        /// Destination path being written.
        path: PathBuf,
    },
    /// A concrete file task was skipped.
    TaskSkipped {
        /// Human-readable task label.
        label: String,
        /// Why the task did not need to run.
        reason: SkipReason,
    },
    /// A concrete file task finished.
    TaskFinished {
        /// Human-readable task label.
        label: String,
    },
}

/// Receives installation progress events.
pub trait ProgressReporter {
    /// Handles a progress event.
    fn report(&mut self, event: ProgressEvent);
}

impl<F> ProgressReporter for F
where
    F: FnMut(ProgressEvent),
{
    fn report(&mut self, event: ProgressEvent) {
        self(event);
    }
}
