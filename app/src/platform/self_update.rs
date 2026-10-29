//! Replacing the launcher's own executable with a downloaded update.
//!
//! Windows lets a running executable be renamed but not overwritten, so the
//! running file is moved aside to `<name>.old.exe` and the update takes its
//! place. The old file is removed on the next start.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Command-line flag of a launcher started by an update: it waits for the
/// previous process to release the single-instance lock.
pub const AFTER_UPDATE_FLAG: &str = "--after-update";

/// Name of the release file this platform installs; `None` where releases
/// are not published yet.
pub const RELEASE_ASSET: Option<&str> = if cfg!(target_os = "windows") {
    Some("orbmc.exe")
} else {
    None
};

/// The launcher executable as it was started.
pub fn current_executable() -> io::Result<PathBuf> {
    std::env::current_exe()
}

/// Where an update is downloaded: next to the executable, so moving it into
/// place stays on one volume.
pub fn staging_path(executable: &Path) -> PathBuf {
    sibling(executable, "update")
}

fn backup_path(executable: &Path) -> PathBuf {
    sibling(executable, "old")
}

/// `orbmc.exe` → `orbmc.<tag>.exe`.
fn sibling(executable: &Path, tag: &str) -> PathBuf {
    let stem = executable
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("orbmc");
    let name = match executable
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some(extension) => format!("{stem}.{tag}.{extension}"),
        None => format!("{stem}.{tag}"),
    };
    executable.with_file_name(name)
}

/// Puts `update` in place of `executable`. The running process keeps working
/// from the moved-aside file; the next start runs the update.
pub fn install(executable: &Path, update: &Path) -> io::Result<()> {
    let backup = backup_path(executable);
    match std::fs::remove_file(&backup) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    std::fs::rename(executable, &backup)?;
    if let Err(error) = std::fs::rename(update, executable) {
        // Never leave the user without a launcher.
        if let Err(restore) = std::fs::rename(&backup, executable) {
            tracing::error!(%restore, "failed to restore the launcher after a failed update");
        }
        return Err(error);
    }
    Ok(())
}

/// Removes what an earlier update left behind. Failures only leave a file.
pub fn remove_leftovers() {
    let Ok(executable) = current_executable() else {
        return;
    };
    for path in [backup_path(&executable), staging_path(&executable)] {
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                tracing::debug!(path = %path.display(), %error, "update leftover stays");
            }
            _ => {}
        }
    }
}

/// Starts the updated launcher; the caller quits right after.
pub fn restart(executable: &Path) -> io::Result<()> {
    Command::new(executable)
        .arg(AFTER_UPDATE_FLAG)
        .spawn()
        .map(drop)
}

/// Whether this process was started by [`restart`].
pub fn started_after_update() -> bool {
    std::env::args().any(|argument| argument == AFTER_UPDATE_FLAG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_swaps_files_and_keeps_the_old_one_aside() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("orbmc.exe");
        std::fs::write(&executable, b"old").unwrap();
        let update = staging_path(&executable);
        assert_eq!(update, directory.path().join("orbmc.update.exe"));
        std::fs::write(&update, b"new").unwrap();

        install(&executable, &update).unwrap();

        assert_eq!(std::fs::read(&executable).unwrap(), b"new");
        assert_eq!(std::fs::read(backup_path(&executable)).unwrap(), b"old");
        assert!(!update.exists());
    }

    #[test]
    fn failed_update_restores_the_launcher() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("orbmc.exe");
        std::fs::write(&executable, b"old").unwrap();

        assert!(install(&executable, &directory.path().join("missing.exe")).is_err());

        assert_eq!(std::fs::read(&executable).unwrap(), b"old");
    }
}
