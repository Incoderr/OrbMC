//! Loader profile writing and installer process helpers.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{core::version::VersionJson, loader::LoaderKind, LauncherError, Result};

/// Returns the standard local path for a loader profile JSON.
pub fn loader_profile_path(minecraft_dir: impl AsRef<Path>, version_id: &str) -> PathBuf {
    minecraft_dir
        .as_ref()
        .join("versions")
        .join(version_id)
        .join(format!("{version_id}.json"))
}

/// Writes a loader profile JSON to `<minecraft_dir>/versions/<id>/<id>.json`.
///
/// # Errors
///
/// Returns [`crate::LauncherError`] if the profile has no `id`, the directory
/// cannot be created, or the profile cannot be serialized.
pub fn write_loader_profile(
    minecraft_dir: impl AsRef<Path>,
    profile: &VersionJson,
) -> Result<PathBuf> {
    let version_id = profile
        .id
        .as_deref()
        .ok_or_else(|| LauncherError::MissingField {
            context: "loader profile".to_string(),
            field: "id".to_string(),
        })?;
    let path = loader_profile_path(minecraft_dir, version_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_vec_pretty(profile)?)?;
    Ok(path)
}

/// Process inputs for a Java-based loader installer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallerInvocation {
    /// Loader family being installed.
    pub loader: LoaderKind,
    /// Java executable used to run the installer jar.
    pub java_executable: PathBuf,
    /// Downloaded installer jar path.
    pub installer_path: PathBuf,
    /// Minecraft directory passed to the installer.
    pub minecraft_dir: PathBuf,
}

/// Builds the argument list used to run a loader installer jar.
pub fn installer_command_args(invocation: &InstallerInvocation) -> Vec<String> {
    vec![
        "-jar".to_string(),
        invocation.installer_path.to_string_lossy().to_string(),
        "--installClient".to_string(),
        invocation.minecraft_dir.to_string_lossy().to_string(),
    ]
}

/// Runs a Java-based loader installer.
///
/// Installer stdout and stderr are captured, and the tail of that output is
/// attached to the failure error so the caller can show why the installer
/// exited. The process runs inside the Minecraft directory so installer
/// side-effect files (for example `installer.log`) stay with the game.
///
/// # Errors
///
/// Returns [`crate::LauncherError`] if the installer process cannot be started
/// or exits with a non-zero status.
pub fn run_loader_installer(invocation: &InstallerInvocation) -> Result<()> {
    ensure_launcher_profiles(&invocation.minecraft_dir)?;

    let mut command = Command::new(&invocation.java_executable);
    command
        .args(installer_command_args(invocation))
        .current_dir(&invocation.minecraft_dir);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output()?;

    if output.status.success() {
        return Ok(());
    }

    Err(LauncherError::InstallerFailed {
        loader: invocation.loader,
        status: output.status.code(),
        output: installer_output_tail(&output.stdout, &output.stderr),
    })
}

/// Writes an empty Mojang launcher profile when none exists.
///
/// Modern Forge and NeoForge installers refuse to run without
/// `launcher_profiles.json`, which normally only the official launcher
/// creates. Third-party launchers write this placeholder instead. An
/// existing file is never overwritten.
fn ensure_launcher_profiles(minecraft_dir: &Path) -> Result<()> {
    let path = minecraft_dir.join("launcher_profiles.json");
    if path.exists() {
        return Ok(());
    }
    fs::write(&path, r#"{"profiles": {}, "version": 3}"#)?;
    Ok(())
}

/// Maximum installer output lines kept per stream in failure errors.
const INSTALLER_OUTPUT_TAIL_LINES: usize = 40;

/// Formats the tail of installer stdout and stderr for error context.
fn installer_output_tail(stdout_bytes: &[u8], stderr_bytes: &[u8]) -> String {
    let mut tail = String::new();

    let stdout_lossy = String::from_utf8_lossy(stdout_bytes);
    let stdout = stdout_lossy.trim_end();
    if !stdout.is_empty() {
        tail.push_str("stdout:\n");
        tail.push_str(&truncate_lines(stdout));
    }

    let stderr_lossy = String::from_utf8_lossy(stderr_bytes);
    let stderr = stderr_lossy.trim_end();
    if !stderr.is_empty() {
        if !tail.is_empty() {
            tail.push_str("\n\n");
        }
        tail.push_str("stderr:\n");
        tail.push_str(&truncate_lines(stderr));
    }

    if tail.is_empty() {
        tail.push_str("(no output)");
    }
    tail
}

/// Keeps only the last lines of an installer stream.
fn truncate_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= INSTALLER_OUTPUT_TAIL_LINES {
        return text.to_string();
    }
    let mut truncated = String::from("…\n");
    truncated.push_str(&lines[lines.len() - INSTALLER_OUTPUT_TAIL_LINES..].join("\n"));
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_output_tail_labels_both_streams() {
        let tail = installer_output_tail(b"hello\n", b"boom\n");
        assert!(tail.contains("stdout:\nhello"));
        assert!(tail.contains("stderr:\nboom"));
    }

    #[test]
    fn installer_output_tail_reports_missing_output() {
        assert_eq!(installer_output_tail(b"", b"  \n"), "(no output)");
    }

    #[test]
    fn installer_output_tail_truncates_long_streams() {
        let long = (0..500)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let tail = installer_output_tail(long.as_bytes(), b"");
        assert!(tail.starts_with("stdout:\n…\n"));
        assert!(tail.contains("line 499"));
        assert!(!tail.contains("line 100\n"));
    }

    #[test]
    fn launcher_profiles_placeholder_is_created_once_and_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();

        ensure_launcher_profiles(dir.path()).unwrap();
        let created = fs::read_to_string(dir.path().join("launcher_profiles.json")).unwrap();
        assert_eq!(created, r#"{"profiles": {}, "version": 3}"#);

        fs::write(dir.path().join("launcher_profiles.json"), "keep me").unwrap();
        ensure_launcher_profiles(dir.path()).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("launcher_profiles.json")).unwrap(),
            "keep me"
        );
    }
}
