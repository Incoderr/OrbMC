//! Filesystem layout of builds.
//!
//! Every build has two directories:
//! - the game directory `<download>/instances/<name>` Minecraft runs in,
//!   named after the build so it is easy to find by hand;
//! - the launcher data directory `<launcher data>/instance-data/<id>`, keyed
//!   by the stable build id, holding the launcher's own bookkeeping (mod
//!   index, icon cache, game caches, JVM crash logs) so none of it clutters
//!   the game directory.

use std::io;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::profile_store;

const INSTANCES: &str = "instances";
const INSTANCE_DATA: &str = "instance-data";
const GAME_CACHE: &str = "game-cache";
const JVM_LOGS: &str = "jvm-logs";

/// Lookup caches the game writes into its working directory. The launcher
/// keeps them in the build data directory between sessions.
pub const GAME_CACHE_FILES: [&str; 2] = ["usercache.json", "usernamecache.json"];

/// Fallback directory name for a build whose name has no usable characters.
const DEFAULT_DIRECTORY_NAME: &str = "Instance";

/// Names Windows refuses for files and directories, with or without extension.
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub fn instances_root(download_directory: &Path) -> PathBuf {
    download_directory.join(INSTANCES)
}

pub fn game_directory(download_directory: &Path, directory_name: &str) -> PathBuf {
    instances_root(download_directory).join(directory_name)
}

pub fn data_directory(id: Uuid) -> io::Result<PathBuf> {
    Ok(profile_store::data_directory()?
        .join(INSTANCE_DATA)
        .join(id.to_string()))
}

pub fn jvm_log_directory(data_directory: &Path) -> PathBuf {
    data_directory.join(JVM_LOGS)
}

pub fn ensure_directory(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Deletes a file; a missing file is not an error.
pub fn remove_file(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn remove_directory(path: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Turns a build name into a directory name valid on every supported OS.
pub fn sanitize_directory_name(name: &str) -> String {
    let replaced: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    // Windows silently strips trailing dots and spaces.
    let trimmed = replaced.trim_end_matches(['.', ' ']).trim_start();
    let mut name: String = trimmed.chars().take(64).collect();
    name = name.trim_end_matches(['.', ' ']).to_owned();
    if name.is_empty() || name.chars().all(|c| c == '_') {
        return DEFAULT_DIRECTORY_NAME.to_owned();
    }
    let stem = name.split('.').next().unwrap_or_default();
    if RESERVED_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem))
    {
        name.push('_');
    }
    name
}

/// Picks `name`, then `name (1)`, `name (2)`, … — the first one not `taken`.
pub fn unique_directory_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let base = sanitize_directory_name(name);
    if !taken(&base) {
        return base;
    }
    (1u32..)
        .map(|index| format!("{base} ({index})"))
        .find(|candidate| !taken(candidate))
        .unwrap_or(base)
}

/// Moves the game caches kept by the launcher back into the game directory
/// before a launch. A copy already present in the game directory (left by a
/// session the launcher did not see end) is newer and wins.
pub fn restore_game_caches(data_directory: &Path, game_directory: &Path) {
    let cache = data_directory.join(GAME_CACHE);
    for name in GAME_CACHE_FILES {
        let target = game_directory.join(name);
        if target.exists() {
            continue;
        }
        if let Err(error) = move_file(&cache.join(name), &target) {
            tracing::warn!(file = name, %error, "failed to restore game cache file");
        }
    }
}

/// Moves the game caches out of the game directory after the game exits.
pub fn stash_game_caches(game_directory: &Path, data_directory: &Path) {
    let cache = data_directory.join(GAME_CACHE);
    for name in GAME_CACHE_FILES {
        if let Err(error) = move_file(&game_directory.join(name), &cache.join(name)) {
            tracing::warn!(file = name, %error, "failed to stash game cache file");
        }
    }
}

/// Moves JVM crash dumps (`hs_err_pid*.log`, `replay_pid*.log`) that older
/// launches left in the game directory into the build data directory.
pub fn stash_jvm_logs(game_directory: &Path, data_directory: &Path) {
    let Ok(entries) = std::fs::read_dir(game_directory) else {
        return;
    };
    let logs = jvm_log_directory(data_directory);
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let is_jvm_log = (name.starts_with("hs_err_pid") || name.starts_with("replay_pid"))
            && name.ends_with(".log");
        if is_jvm_log && let Err(error) = move_file(&entry.path(), &logs.join(name)) {
            tracing::warn!(file = name, %error, "failed to move JVM crash log");
        }
    }
}

/// Moves `from` to `to`, replacing `to`. A missing `from` is not an error.
///
/// Falls back to copy + delete because the launcher data directory and the
/// download directory may live on different volumes.
pub fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if from.is_dir() {
        copy_directory(from, to)?;
        std::fs::remove_dir_all(from)
    } else {
        std::fs::copy(from, to)?;
        std::fs::remove_file(from)
    }
}

/// Renames a directory, falling back to a recursive copy across volumes.
pub fn move_directory(from: &Path, to: &Path) -> io::Result<()> {
    if to.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", to.display()),
        ));
    }
    move_file(from, to)
}

fn copy_directory(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_directory_names() {
        assert_eq!(
            sanitize_directory_name("  Мои моды 1.20  "),
            "Мои моды 1.20"
        );
        assert_eq!(sanitize_directory_name("a/b:c*?"), "a_b_c__");
        assert_eq!(sanitize_directory_name("build..."), "build");
        assert_eq!(sanitize_directory_name("con"), "con_");
        assert_eq!(sanitize_directory_name("Com1.txt"), "Com1.txt_");
        assert_eq!(sanitize_directory_name(""), DEFAULT_DIRECTORY_NAME);
        assert_eq!(sanitize_directory_name("///"), DEFAULT_DIRECTORY_NAME);
    }

    #[test]
    fn numbers_duplicate_names() {
        let taken = ["Fabric 1.21", "Fabric 1.21 (1)"];
        let name = unique_directory_name("Fabric 1.21", |candidate| taken.contains(&candidate));
        assert_eq!(name, "Fabric 1.21 (2)");
        assert_eq!(unique_directory_name("Forge", |_| false), "Forge");
    }

    #[test]
    fn stashes_and_restores_game_caches() {
        let game = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(game.path().join("usercache.json"), "[]").unwrap();
        stash_game_caches(game.path(), data.path());
        assert!(!game.path().join("usercache.json").exists());
        assert!(
            data.path()
                .join(GAME_CACHE)
                .join("usercache.json")
                .is_file()
        );

        restore_game_caches(data.path(), game.path());
        assert!(game.path().join("usercache.json").is_file());
        assert!(!data.path().join(GAME_CACHE).join("usercache.json").exists());
    }

    #[test]
    fn moves_jvm_crash_logs() {
        let game = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(game.path().join("hs_err_pid42.log"), "x").unwrap();
        std::fs::write(game.path().join("options.txt"), "x").unwrap();
        stash_jvm_logs(game.path(), data.path());
        assert!(
            jvm_log_directory(data.path())
                .join("hs_err_pid42.log")
                .is_file()
        );
        assert!(game.path().join("options.txt").is_file());
    }
}
