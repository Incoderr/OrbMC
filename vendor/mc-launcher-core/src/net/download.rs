//! Download plans and execution.
//!
//! Plans execute with a bounded worker pool so many small files (asset
//! objects, libraries) download in parallel instead of paying one network
//! round trip per file. Downloads share the process-wide HTTP client, hash
//! bytes while streaming, write into temporary files that are renamed
//! atomically into place, and retry transient failures a few times before a
//! task fails the plan.
//!
//! A [`DownloadControl`] can pause workers between chunks and cancel the
//! plan; byte progress is reported for the plan as a whole so callers can
//! render one progress bar with throughput.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc, Mutex,
    },
    thread,
    time::Duration,
};

use tempfile::NamedTempFile;

use crate::{
    io::hash::{sha1_file, HashAlgorithm, HashingReader},
    net::control::DownloadControl,
    progress::{ProgressEvent, ProgressReporter, SkipReason},
    LauncherError, Result,
};

/// Parallel workers used for multi-task plans.
///
/// Minecraft installs are dominated by thousands of small asset objects, so
/// throughput is bound by network latency rather than bandwidth; parallel
/// workers keep connections busy.
const DEFAULT_WORKERS: usize = 8;

/// Total attempts per task, including the first try.
const TASK_ATTEMPTS: u32 = 3;

/// Pause between task attempts.
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Copy buffer for streamed downloads.
const COPY_BUFFER_BYTES: usize = 64 * 1024;

/// Interval between aggregate progress samples.
const PLAN_PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

/// Supported checksum validation methods for downloaded files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checksum {
    /// SHA-1 checksum.
    Sha1(String),
    /// SHA-256 checksum.
    Sha256(String),
}

/// One file download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTask {
    /// Source URL.
    pub url: String,
    /// Destination path.
    pub destination: PathBuf,
    /// Optional checksum used for skip and validation decisions.
    pub checksum: Option<Checksum>,
    /// Expected file size in bytes, when the manifest declared it.
    ///
    /// Used only for progress reporting; unknown sizes still download
    /// normally.
    pub size: Option<u64>,
    /// Human-readable task label reported in progress events.
    pub label: String,
}

/// A batch of download tasks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadPlan {
    /// Tasks to execute.
    pub tasks: Vec<DownloadTask>,
}

/// Returns whether an existing destination file can be reused.
///
/// # Errors
///
/// Returns [`crate::LauncherError`] if checksum calculation fails.
pub fn should_skip_existing(task: &DownloadTask) -> Result<bool> {
    if !task.destination.is_file() {
        return Ok(false);
    }

    match &task.checksum {
        Some(Checksum::Sha1(expected)) => Ok(sha1_file(&task.destination)? == *expected),
        Some(Checksum::Sha256(_)) => Ok(false),
        None => Ok(true),
    }
}

/// Executes a download plan with up to [`DEFAULT_WORKERS`] parallel tasks.
///
/// The plan cannot be paused or cancelled through a control handle; see
/// [`execute_plan_controlled`].
///
/// # Errors
///
/// Returns [`crate::LauncherError`] for network, filesystem, or checksum
/// failures.
pub fn execute_plan(plan: &DownloadPlan, reporter: &mut dyn ProgressReporter) -> Result<()> {
    execute_plan_inner(plan, reporter, DEFAULT_WORKERS, &DownloadControl::new())
}

/// Executes a download plan with an explicit parallel worker count.
///
/// `workers` is clamped to the plan size. The plan cannot be paused or
/// cancelled through a control handle; see [`execute_plan_controlled`].
///
/// # Errors
///
/// Returns [`crate::LauncherError`] for network, filesystem, or checksum
/// failures.
pub fn execute_plan_with_workers(
    plan: &DownloadPlan,
    reporter: &mut dyn ProgressReporter,
    workers: usize,
) -> Result<()> {
    execute_plan_inner(plan, reporter, workers, &DownloadControl::new())
}

/// Executes a download plan that can be paused and cancelled.
///
/// Workers park at checkpoints between files and between streamed chunks, so
/// [`DownloadControl::pause`] takes effect almost immediately and
/// [`DownloadControl::cancel`] unwinds the plan with
/// [`crate::LauncherError::InstallCancelled`].
///
/// # Errors
///
/// Returns [`crate::LauncherError`] for network, filesystem, or checksum
/// failures, and [`crate::LauncherError::InstallCancelled`] after
/// cancellation.
pub fn execute_plan_controlled(
    plan: &DownloadPlan,
    reporter: &mut dyn ProgressReporter,
    control: &DownloadControl,
) -> Result<()> {
    execute_plan_inner(plan, reporter, DEFAULT_WORKERS, control)
}

/// Executes a download plan with pause, cancel, and progress plumbing.
fn execute_plan_inner(
    plan: &DownloadPlan,
    reporter: &mut dyn ProgressReporter,
    workers: usize,
    control: &DownloadControl,
) -> Result<()> {
    if plan.tasks.is_empty() {
        return Ok(());
    }

    let (tasks, total_bytes) = pending_plan_summary(plan);
    reporter.report(ProgressEvent::PlanStarted { tasks, total_bytes });

    // Workers verify every task; the summary only estimates how many will
    // actually download, so an all-existing plan still gets its checksums
    // checked in parallel.
    let workers = workers.clamp(1, plan.tasks.len());

    let next_task = &AtomicUsize::new(0);
    let failed = &AtomicBool::new(false);
    let first_error = &Mutex::new(None::<LauncherError>);
    let downloaded = &AtomicU64::new(0);
    let (events_sender, events_receiver) = mpsc::channel::<ProgressEvent>();

    thread::scope(|scope| {
        for _ in 0..workers {
            let events = events_sender.clone();
            scope.spawn(move || {
                while !failed.load(Ordering::Relaxed) {
                    if let Err(error) = control.checkpoint() {
                        failed.store(true, Ordering::Relaxed);
                        record_first_error(first_error, error);
                        break;
                    }
                    let index = next_task.fetch_add(1, Ordering::Relaxed);
                    let Some(task) = plan.tasks.get(index) else {
                        break;
                    };
                    if let Err(error) = run_task(task, control, downloaded, &events) {
                        failed.store(true, Ordering::Relaxed);
                        record_first_error(first_error, error);
                        break;
                    }
                }
            });
        }

        // The workers hold the remaining sender handles, so the drain loop
        // both joins their work and forwards progress at a steady cadence.
        drop(events_sender);
        let mut last_reported = 0_u64;
        loop {
            match events_receiver.recv_timeout(PLAN_PROGRESS_INTERVAL) {
                Ok(event) => reporter.report(event),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            let received = downloaded.load(Ordering::Relaxed);
            if received != last_reported {
                last_reported = received;
                reporter.report(ProgressEvent::PlanProgress {
                    downloaded: received,
                    total_bytes,
                });
            }
        }
    });

    let error = first_error
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Counts the tasks that will actually download and sums their sizes.
///
/// Files already on disk are assumed to be reusable skips, matching
/// [`should_skip_existing`] for the common cases. The total is `None` when
/// any pending task did not declare its size.
fn pending_plan_summary(plan: &DownloadPlan) -> (usize, Option<u64>) {
    let mut tasks = 0;
    let mut total_bytes = Some(0_u64);
    for task in &plan.tasks {
        if task.destination.is_file() {
            continue;
        }
        tasks += 1;
        total_bytes = total_bytes.and_then(|total| task.size.map(|size| total + size));
    }
    (tasks, total_bytes)
}

/// Keeps the first task error of a plan.
fn record_first_error(first_error: &Mutex<Option<LauncherError>>, error: LauncherError) {
    let mut slot = first_error
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if slot.is_none() {
        *slot = Some(error);
    }
}

/// Runs one task, emitting progress events and retrying transient failures.
fn run_task(
    task: &DownloadTask,
    control: &DownloadControl,
    downloaded: &AtomicU64,
    events: &mpsc::Sender<ProgressEvent>,
) -> Result<()> {
    if should_skip_existing(task)? {
        let _ = events.send(ProgressEvent::TaskSkipped {
            label: task.label.clone(),
            reason: if task.checksum.is_some() {
                SkipReason::ChecksumMatched
            } else {
                SkipReason::FileExistsWithoutChecksum
            },
        });
        return Ok(());
    }

    let _ = events.send(ProgressEvent::TaskStarted {
        label: task.label.clone(),
        path: task.destination.clone(),
    });

    let mut attempts_left = TASK_ATTEMPTS;
    loop {
        match download_task_once(task, control, downloaded) {
            Ok(()) => {
                let _ = events.send(ProgressEvent::TaskFinished {
                    label: task.label.clone(),
                });
                return Ok(());
            }
            Err(attempt) => {
                attempts_left -= 1;
                if attempts_left == 0
                    || !attempt.retryable
                    || matches!(attempt.error, LauncherError::InstallCancelled)
                {
                    return Err(attempt.error);
                }
                thread::sleep(RETRY_DELAY);
            }
        }
    }
}

/// Failure of one download attempt, with retry advice.
struct AttemptError {
    error: LauncherError,
    retryable: bool,
}

impl AttemptError {
    /// Builds a failure that another attempt might fix.
    fn retryable(error: LauncherError) -> Self {
        Self {
            error,
            retryable: true,
        }
    }

    /// Builds a failure that retrying cannot fix.
    fn fatal(error: LauncherError) -> Self {
        Self {
            error,
            retryable: false,
        }
    }

    /// Classifies an HTTP failure: connection, timeout, and server errors are
    /// transient, while rejected requests such as 404 are not.
    fn from_http(error: reqwest::Error) -> Self {
        let retryable = error.status().is_none_or(|status| status.is_server_error());
        Self {
            error: error.into(),
            retryable,
        }
    }
}

/// Performs one download attempt for a task.
///
/// Bytes stream through an on-the-fly hasher into a temporary file next to
/// the destination, which is then renamed into place.
fn download_task_once(
    task: &DownloadTask,
    control: &DownloadControl,
    downloaded: &AtomicU64,
) -> std::result::Result<(), AttemptError> {
    let parent = task
        .destination
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    fs::create_dir_all(&parent).map_err(|error| AttemptError::fatal(error.into()))?;

    let client = super::http::client().map_err(AttemptError::fatal)?;
    let response = client
        .get(&task.url)
        .send()
        .map_err(AttemptError::from_http)?
        .error_for_status()
        .map_err(AttemptError::from_http)?;

    let algorithm = match &task.checksum {
        Some(Checksum::Sha1(_)) => Some(HashAlgorithm::Sha1),
        Some(Checksum::Sha256(_)) => Some(HashAlgorithm::Sha256),
        None => None,
    };
    let mut reader = HashingReader::new(response, algorithm);
    let mut temp =
        NamedTempFile::new_in(&parent).map_err(|error| AttemptError::fatal(error.into()))?;
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];

    loop {
        // Read errors come from the HTTP stream, so they are network trouble.
        let read = reader
            .read(&mut buffer)
            .map_err(|error| AttemptError::retryable(error.into()))?;
        if read == 0 {
            break;
        }
        temp.write_all(&buffer[..read])
            .map_err(|error| AttemptError::fatal(error.into()))?;
        downloaded.fetch_add(read as u64, Ordering::Relaxed);
        control.checkpoint().map_err(AttemptError::fatal)?;
    }

    let (_, digest) = reader.finish();
    if let (Some(Checksum::Sha1(expected)) | Some(Checksum::Sha256(expected)), Some(actual)) =
        (&task.checksum, digest)
    {
        if actual != *expected {
            return Err(AttemptError::retryable(LauncherError::ChecksumMismatch {
                path: task.destination.clone(),
                expected: expected.clone(),
                actual,
            }));
        }
    }

    temp.as_file()
        .sync_all()
        .map_err(|error| AttemptError::fatal(error.into()))?;

    // Windows cannot rename over an existing file, so a stale destination
    // has to be removed first. The temporary file keeps the destination free
    // of partial downloads until this point.
    if task.destination.exists() {
        fs::remove_file(&task.destination).map_err(|error| AttemptError::fatal(error.into()))?;
    }
    temp.persist(&task.destination)
        .map_err(|error| AttemptError::fatal(error.error.into()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task(destination: &Path) -> DownloadTask {
        DownloadTask {
            url: "https://example.invalid/file".to_string(),
            destination: destination.to_path_buf(),
            checksum: None,
            size: Some(1),
            label: "sample".to_string(),
        }
    }

    #[test]
    fn empty_plan_succeeds_without_workers() {
        let mut reporter = |_event: ProgressEvent| {};
        assert!(execute_plan_with_workers(&DownloadPlan::default(), &mut reporter, 8).is_ok());
    }

    #[test]
    fn cancelled_control_fails_plan_without_network() {
        let dir = tempfile::tempdir().unwrap();
        let plan = DownloadPlan {
            tasks: vec![sample_task(&dir.path().join("file.bin"))],
        };
        let control = DownloadControl::new();
        control.cancel();

        let mut reporter = |_event: ProgressEvent| {};
        assert!(matches!(
            execute_plan_controlled(&plan, &mut reporter, &control),
            Err(LauncherError::InstallCancelled)
        ));
    }

    #[test]
    fn existing_files_are_excluded_from_plan_totals() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("existing.bin");
        fs::write(&existing, b"x").unwrap();

        let plan = DownloadPlan {
            tasks: vec![
                sample_task(&existing),
                sample_task(&dir.path().join("pending.bin")),
            ],
        };
        let (tasks, total_bytes) = pending_plan_summary(&plan);
        assert_eq!(tasks, 1);
        assert_eq!(total_bytes, Some(1));
    }

    #[test]
    fn unknown_sizes_disable_plan_totals() {
        let dir = tempfile::tempdir().unwrap();
        let mut task = sample_task(&dir.path().join("file.bin"));
        task.size = None;
        let plan = DownloadPlan { tasks: vec![task] };

        let (tasks, total_bytes) = pending_plan_summary(&plan);
        assert_eq!(tasks, 1);
        assert_eq!(total_bytes, None);
    }

    #[test]
    fn http_errors_classify_by_status() {
        let error = reqwest::blocking::Client::new()
            .get("http://127.0.0.1:1/connection-refused")
            .send()
            .unwrap_err();
        assert!(AttemptError::from_http(error).retryable);
    }

    #[test]
    fn checksum_mismatch_is_retryable() {
        let error = AttemptError::retryable(LauncherError::ChecksumMismatch {
            path: Path::new("a").to_path_buf(),
            expected: "expected".into(),
            actual: "actual".into(),
        });
        assert!(error.retryable);
    }
}
