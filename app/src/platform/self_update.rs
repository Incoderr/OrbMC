//! Installing a downloaded launcher update.
//!
//! Releases ship an installer (`installer/orbmc.iss`). An update downloads it
//! and runs it silently: the installer waits for this launcher to quit,
//! replaces the installed files and starts the new version. Nothing is renamed
//! next to the running executable.

use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;

/// Platform part of release file names (`orbmc-1.2.3-windows-x64-setup.exe`);
/// `None` where releases are not published yet.
const RELEASE_PLATFORM: Option<&str> = if cfg!(not(target_os = "windows")) {
    None
} else if cfg!(target_arch = "x86_64") {
    Some("windows-x64")
} else if cfg!(target_arch = "aarch64") {
    Some("windows-arm64")
} else {
    None
};

/// Name of the release installer this platform updates with.
pub fn release_asset(version: &str) -> Option<String> {
    RELEASE_PLATFORM.map(|platform| format!("orbmc-{version}-{platform}-setup.exe"))
}

/// Downloaded installers of earlier versions, removed on start.
const OLD_STAGING_NAMES: [&str; 1] = ["OrbMC-Setup.exe"];

/// Where the update installer is downloaded.
pub fn staging_path() -> PathBuf {
    staging_directory().join("orbmc-setup.exe")
}

fn staging_directory() -> PathBuf {
    std::env::temp_dir().join("OrbMC update")
}

/// Whether this launcher was set up by the installer. A portable copy must
/// not run it: that would install a second launcher instead of updating it.
pub fn installed() -> bool {
    static INSTALLED: LazyLock<bool> = LazyLock::new(|| {
        std::env::current_exe()
            .map(|executable| executable.with_file_name("unins000.exe").is_file())
            .unwrap_or(false)
    });
    *INSTALLED
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
    leftovers.extend(
        OLD_STAGING_NAMES
            .into_iter()
            .map(|name| staging_directory().join(name)),
    );
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
