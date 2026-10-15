//! Pause and cancellation for download plan execution.
//!
//! A [`DownloadControl`] is a cheap cloneable handle. The install worker
//! holds one side and passes clones into `mc_launcher_core`, while the UI
//! keeps another clone to pause, resume, or cancel the running work.

use std::{
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use crate::{LauncherError, Result};

/// How long a checkpoint waits between pause wake-up re-checks.
const PAUSE_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Running,
    Paused,
    Cancelled,
}

/// Handle for pausing, resuming, and cancelling install work.
///
/// Clones share the same underlying state.
#[derive(Debug, Clone, Default)]
pub struct DownloadControl {
    inner: Arc<ControlInner>,
}

#[derive(Debug, Default)]
struct ControlInner {
    mode: Mutex<Mode>,
    resume_signal: Condvar,
}

impl DownloadControl {
    /// Creates a control handle in the running state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pauses work at the next checkpoint.
    ///
    /// Already running downloads finish their current chunk first. Pausing an
    /// already paused or cancelled control does nothing.
    pub fn pause(&self) {
        let mut mode = self.lock();
        if *mode == Mode::Running {
            *mode = Mode::Paused;
        }
    }

    /// Resumes paused work.
    pub fn resume(&self) {
        let mut mode = self.lock();
        if *mode == Mode::Paused {
            *mode = Mode::Running;
            drop(mode);
            self.inner.resume_signal.notify_all();
        }
    }

    /// Cancels work at the next checkpoint.
    ///
    /// Cancellation also wakes up paused checkpoints, so a paused install can
    /// always be cancelled.
    pub fn cancel(&self) {
        let mut mode = self.lock();
        *mode = Mode::Cancelled;
        drop(mode);
        self.inner.resume_signal.notify_all();
    }

    /// Returns whether [`DownloadControl::pause`] is currently active.
    pub fn is_paused(&self) -> bool {
        *self.lock() == Mode::Paused
    }

    /// Returns whether [`DownloadControl::cancel`] was called.
    pub fn is_cancelled(&self) -> bool {
        *self.lock() == Mode::Cancelled
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Mode> {
        self.inner
            .mode
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Blocks while paused and fails once cancelled.
    ///
    /// Workers call this between downloads and between streamed chunks, so a
    /// pause parks them quickly and a cancel unwinds promptly.
    pub(crate) fn checkpoint(&self) -> Result<()> {
        let mut mode = self.lock();
        loop {
            match *mode {
                Mode::Running => return Ok(()),
                Mode::Cancelled => return Err(LauncherError::InstallCancelled),
                Mode::Paused => {}
            }
            let (guard, _timeout) = self
                .inner
                .resume_signal
                .wait_timeout(mode, PAUSE_POLL_INTERVAL)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            mode = guard;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn checkpoint_passes_while_running() {
        let control = DownloadControl::new();
        assert!(control.checkpoint().is_ok());
    }

    #[test]
    fn cancel_wakes_paused_checkpoint() {
        let control = DownloadControl::new();
        control.pause();

        thread::scope(|scope| {
            let handle = control.clone();
            let blocked = scope.spawn(move || handle.checkpoint());
            thread::sleep(Duration::from_millis(50));
            assert!(control.is_paused());
            control.cancel();
            assert!(matches!(
                blocked.join().unwrap(),
                Err(LauncherError::InstallCancelled)
            ));
        });
        assert!(control.is_cancelled());
    }

    #[test]
    fn resume_unblocks_paused_checkpoint() {
        let control = DownloadControl::new();
        control.pause();

        thread::scope(|scope| {
            let handle = control.clone();
            let blocked = scope.spawn(move || handle.checkpoint());
            thread::sleep(Duration::from_millis(50));
            control.resume();
            assert!(blocked.join().unwrap().is_ok());
        });
    }

    #[test]
    fn cancel_after_completed_work_stays_cancelled() {
        let control = DownloadControl::new();
        control.cancel();
        assert!(control.checkpoint().is_err());
        control.resume();
        assert!(control.checkpoint().is_err());
    }
}
