//! File downloads with progress, retries and cancellation.
//!
//! Game files and Java runtimes are downloaded by the install engine
//! (`mc-launcher-core`), which has its own queue and progress. This module
//! covers the files OrbMC downloads itself: mods, resource packs, shaders.

use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::io::AsyncReadExt as _;
use gpui_kit::BackgroundExecutor;
use gpui_kit::http_client::{AsyncBody, HttpClient, HttpRequestExt, RedirectPolicy, Request};

/// Attempts per file: network hiccups and truncated bodies are retried.
const ATTEMPTS: u32 = 3;
const CHUNK_BYTES: usize = 64 * 1024;

/// Progress and cancellation of one transfer, shared by the download task
/// and the UI. A transfer may cover several files (a mod and its
/// dependencies); progress shows the file in flight.
#[derive(Clone, Default)]
pub struct TransferHandle {
    state: Arc<TransferState>,
}

#[derive(Default)]
struct TransferState {
    received: AtomicU64,
    /// Zero while the size is unknown.
    total: AtomicU64,
    cancelled: AtomicBool,
    started: Mutex<Option<Instant>>,
    /// Batch mode: several files share one byte counter.
    batch: AtomicBool,
    files_done: AtomicU64,
    files_total: AtomicU64,
}

/// Snapshot of a transfer for display.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TransferProgress {
    pub received: u64,
    pub total: Option<u64>,
    /// Average speed, in bytes per second.
    pub speed_bps: f64,
    /// Finished and total files of a batch; `None` for single files.
    pub files: Option<(u64, u64)>,
}

impl TransferHandle {
    /// Asks the download to stop; it ends with [`DownloadError::Cancelled`].
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Relaxed)
    }

    pub fn progress(&self) -> TransferProgress {
        let received = self.state.received.load(Ordering::Relaxed);
        let total = self.state.total.load(Ordering::Relaxed);
        let elapsed = self
            .state
            .started
            .lock()
            .ok()
            .and_then(|started| *started)
            .map(|started| started.elapsed().as_secs_f64())
            .unwrap_or_default();
        let files_total = self.state.files_total.load(Ordering::Relaxed);
        TransferProgress {
            files: self
                .state
                .batch
                .load(Ordering::Relaxed)
                .then(|| (self.state.files_done.load(Ordering::Relaxed), files_total)),
            received,
            total: (total > 0).then_some(total),
            speed_bps: if elapsed > 0.5 {
                received as f64 / elapsed
            } else {
                0.
            },
        }
    }

    /// Switches to one shared counter for `files` files of `bytes` in total.
    pub fn begin_batch(&self, bytes: u64, files: u64) {
        self.begin_file(Some(bytes));
        self.state.batch.store(true, Ordering::Relaxed);
        self.state.files_done.store(0, Ordering::Relaxed);
        self.state.files_total.store(files, Ordering::Relaxed);
    }

    pub fn file_done(&self) {
        self.state.files_done.fetch_add(1, Ordering::Relaxed);
    }

    fn begin_file(&self, total: Option<u64>) {
        if self.state.batch.load(Ordering::Relaxed) {
            return;
        }
        self.state.received.store(0, Ordering::Relaxed);
        self.state
            .total
            .store(total.unwrap_or_default(), Ordering::Relaxed);
        if let Ok(mut started) = self.state.started.lock() {
            *started = Some(Instant::now());
        }
    }

    fn add(&self, bytes: u64) {
        self.state.received.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Takes back the bytes of a failed attempt.
    fn rollback(&self, bytes: u64) {
        self.state.received.fetch_sub(
            bytes.min(self.state.received.load(Ordering::Relaxed)),
            Ordering::Relaxed,
        );
    }
}

impl std::fmt::Debug for TransferHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferHandle")
            .field("progress", &self.progress())
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

/// A transfer as a download task sees it: the handle shared with the UI and
/// the executor that times pauses between attempts.
#[derive(Clone)]
pub struct Transfer {
    pub handle: TransferHandle,
    pub executor: BackgroundExecutor,
}

/// One file to download.
#[derive(Clone, Copy, Debug)]
pub struct FileDownload<'a> {
    /// Must be `https://`.
    pub url: &'a str,
    /// Final path; the file appears there only after it was fully verified.
    pub destination: &'a Path,
    /// Expected size, when the provider publishes it.
    pub size: Option<u64>,
    /// Expected SHA-1 (hex), when the provider publishes it.
    pub sha1: Option<&'a str>,
    /// Hard limit against runaway responses.
    pub max_size: u64,
}

/// Why a download failed. The messages are shown to the user.
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("Некорректная ссылка на файл")]
    InvalidUrl,
    #[error("Загрузка отменена")]
    Cancelled,
    #[error("Сервер ответил HTTP {0}")]
    Http(u16),
    #[error("Сетевая ошибка: {0}")]
    Network(String),
    #[error("Файл больше допустимых {} МиБ", limit / 1024 / 1024)]
    TooLarge { limit: u64 },
    #[error("Размер файла не совпал: получено {actual} байт, ожидалось {expected}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("Контрольная сумма файла не совпала")]
    Checksum,
    #[error("Не удалось сохранить файл: {0}")]
    Io(#[from] std::io::Error),
}

impl DownloadError {
    /// Failures another attempt can fix.
    fn is_retryable(&self) -> bool {
        match self {
            Self::Network(_) | Self::SizeMismatch { .. } | Self::Checksum => true,
            Self::Http(status) => *status == 429 || *status >= 500,
            _ => false,
        }
    }
}

/// Downloads `file`, verifying size and SHA-1, retrying transient failures
/// and honouring cancellation through the transfer handle.
pub async fn download_file(
    client: Arc<dyn HttpClient>,
    file: FileDownload<'_>,
    transfer: &Transfer,
) -> Result<(), DownloadError> {
    let Transfer {
        handle: transfer,
        executor,
    } = transfer;
    if !file.url.starts_with("https://") {
        return Err(DownloadError::InvalidUrl);
    }
    if file.size.is_some_and(|size| size > file.max_size) {
        return Err(DownloadError::TooLarge {
            limit: file.max_size,
        });
    }
    let directory = file
        .destination
        .parent()
        .ok_or_else(|| std::io::Error::other("download destination has no folder"))?;
    std::fs::create_dir_all(directory)?;

    let mut attempt = 1;
    loop {
        let mut written = 0;
        let result = attempt_download(&client, &file, directory, transfer, &mut written).await;
        if result.is_err() {
            transfer.rollback(written);
        }
        match result {
            Ok(temporary) => {
                temporary
                    .persist(file.destination)
                    .map_err(|error| error.error)?;
                return Ok(());
            }
            Err(error)
                if error.is_retryable() && attempt < ATTEMPTS && !transfer.is_cancelled() =>
            {
                tracing::warn!(url = file.url, attempt, %error, "download failed; retrying");
                executor
                    .timer(Duration::from_secs(u64::from(attempt)))
                    .await;
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn attempt_download(
    client: &Arc<dyn HttpClient>,
    file: &FileDownload<'_>,
    directory: &Path,
    transfer: &TransferHandle,
    written: &mut u64,
) -> Result<tempfile::NamedTempFile, DownloadError> {
    transfer.begin_file(file.size);
    let request = Request::builder()
        .uri(file.url)
        .timeout(Duration::from_secs(120))
        // Release files (GitHub) are served through a redirect; the result
        // is checked against the published size and checksum anyway.
        .follow_redirects(RedirectPolicy::FollowAll)
        .body(AsyncBody::empty())
        .map_err(|error| DownloadError::Network(error.to_string()))?;
    let response = client
        .send(request)
        .await
        .map_err(|error| DownloadError::Network(error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(DownloadError::Http(status.as_u16()));
    }

    let mut body = response.into_body();
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    let mut buffer = vec![0; CHUNK_BYTES];
    loop {
        if transfer.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        let read = body
            .read(&mut buffer)
            .await
            .map_err(|error| DownloadError::Network(error.to_string()))?;
        if read == 0 {
            break;
        }
        *written += read as u64;
        if *written > file.max_size {
            return Err(DownloadError::TooLarge {
                limit: file.max_size,
            });
        }
        temporary.write_all(&buffer[..read])?;
        transfer.add(read as u64);
    }
    temporary.flush()?;
    temporary.as_file().sync_all()?;

    verify(temporary.path(), *written, file.size, file.sha1)?;
    Ok(temporary)
}

/// Checks a downloaded file against the published size and SHA-1.
fn verify(
    path: &Path,
    written: u64,
    size: Option<u64>,
    sha1: Option<&str>,
) -> Result<(), DownloadError> {
    if let Some(expected) = size.filter(|size| *size > 0)
        && written != expected
    {
        return Err(DownloadError::SizeMismatch {
            expected,
            actual: written,
        });
    }
    if let Some(expected) = sha1 {
        let actual = mc_launcher_core::io::hash::sha1_file(path).map_err(std::io::Error::other)?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(DownloadError::Checksum);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_restarts_for_each_file_and_cancel_sticks() {
        let transfer = TransferHandle::default();
        transfer.begin_file(Some(100));
        transfer.add(40);
        assert_eq!(transfer.progress().received, 40);
        assert_eq!(transfer.progress().total, Some(100));
        transfer.begin_file(None);
        assert_eq!(transfer.progress().received, 0);
        assert_eq!(transfer.progress().total, None);
        transfer.clone().cancel();
        assert!(transfer.is_cancelled());
    }

    #[test]
    fn batches_share_one_counter_and_count_files() {
        let transfer = TransferHandle::default();
        transfer.begin_batch(300, 2);
        transfer.begin_file(Some(100));
        transfer.add(100);
        transfer.file_done();
        transfer.begin_file(Some(200));
        transfer.add(50);
        transfer.rollback(50);
        let progress = transfer.progress();
        assert_eq!(progress.received, 100);
        assert_eq!(progress.total, Some(300));
        assert_eq!(progress.files, Some((1, 2)));
    }

    #[test]
    fn verification_checks_size_and_sha1() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"abc").unwrap();
        let sha1_of_abc = "a9993e364706816aba3e25717850c26c9cd0d89d";
        assert!(verify(file.path(), 3, Some(3), Some(sha1_of_abc)).is_ok());
        assert!(matches!(
            verify(file.path(), 3, Some(4), None),
            Err(DownloadError::SizeMismatch { .. })
        ));
        assert!(matches!(
            verify(file.path(), 3, None, Some("00")),
            Err(DownloadError::Checksum)
        ));
    }

    #[test]
    fn only_transient_failures_are_retried() {
        assert!(DownloadError::Network("reset".into()).is_retryable());
        assert!(DownloadError::Http(503).is_retryable());
        assert!(DownloadError::Http(429).is_retryable());
        assert!(!DownloadError::Http(404).is_retryable());
        assert!(!DownloadError::Cancelled.is_retryable());
    }
}
