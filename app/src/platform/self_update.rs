//! Installing a downloaded launcher update.
//!
//! Releases ship an installer (`installer/orbmc.iss`). An update downloads it
//! and runs it silently: the installer waits for this launcher to quit,
//! replaces the installed files and starts the new version. Nothing is renamed
//! next to the running executable.

use std::io;
use std::path::PathBuf;
use std::process::Command;

/// Name of the release file this platform installs; `None` where releases
/// are not published yet.
pub const RELEASE_ASSET: Option<&str> = if cfg!(target_os = "windows") {
    Some("OrbMC-Setup.exe")
} else {
    None
};

/// Where the update installer is downloaded.
pub fn staging_path() -> PathBuf {
    std::env::temp_dir()
        .join("OrbMC update")
        .join(RELEASE_ASSET.unwrap_or("orbmc-update"))
}

/// Starts the silent update; the caller quits right after so the installer
/// can replace the files. With `launch` the installer starts the updated
/// launcher when it is done.
pub fn run_installer(launch: bool) -> io::Result<()> {
    let mut command = Command::new(staging_path());
    command.args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]);
    if !launch {
        command.arg(NO_LAUNCH_FLAG);
    }
    command.spawn().map(drop)
}

/// Installer flag (read by `installer/orbmc.iss`): do not start the launcher
/// after a silent update, the user has just quit it.
const NO_LAUNCH_FLAG: &str = "/NOLAUNCH";

/// Removes what earlier updates left behind: the downloaded installer and the
/// files of the old self-replacing updater. Failures only leave a file.
pub fn remove_leftovers() {
    let mut leftovers = vec![staging_path()];
    if let Ok(executable) = std::env::current_exe() {
        leftovers.push(executable.with_file_name("orbmc.old.exe"));
        leftovers.push(executable.with_file_name("orbmc.update.exe"));
    }
    for path in leftovers {
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                tracing::debug!(path = %path.display(), %error, "update leftover stays");
            }
            _ => {}
        }
    }
}
