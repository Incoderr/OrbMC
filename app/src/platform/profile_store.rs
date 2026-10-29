use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

pub fn load<T: DeserializeOwned>() -> std::io::Result<Vec<T>> {
    let path = profiles_path()?;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    serde_json::from_slice(&bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

pub fn save<T: Serialize>(profiles: &[T]) -> std::io::Result<()> {
    write_json_atomically(&profiles_path()?, profiles)
}

pub fn load_settings<T: DeserializeOwned + Default>() -> std::io::Result<T> {
    let path = settings_path()?;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(T::default());
        }
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

pub fn save_settings<T: Serialize>(settings: &T) -> std::io::Result<()> {
    write_json_atomically(&settings_path()?, settings)
}

pub fn default_download_directory() -> std::io::Result<PathBuf> {
    data_directory()
}

pub fn shared_game_directory(download_directory: &Path) -> PathBuf {
    download_directory.join("shared")
}

fn profiles_path() -> std::io::Result<PathBuf> {
    Ok(data_directory()?.join("instances.json"))
}

fn settings_path() -> std::io::Result<PathBuf> {
    Ok(data_directory()?.join("settings.json"))
}

pub(crate) fn write_json_atomically<T: Serialize + ?Sized>(
    path: &Path,
    value: &T,
) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "launcher data file has no parent directory",
        )
    })?;
    std::fs::create_dir_all(parent)?;

    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, value)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| error.error)
}

/// Maps a path inside the pre-rename data directory (`RS MC Launcher`) onto
/// the current one. `data_directory` moves that folder, but saved settings
/// and builds may still point at the old location.
pub fn relocate_legacy_path(path: &Path) -> Option<PathBuf> {
    let relative = path
        .strip_prefix(data_root().ok()?.join(LEGACY_NAME))
        .ok()?;
    Some(data_directory().ok()?.join(relative))
}

const LEGACY_NAME: &str = "RS MC Launcher";

fn data_root() -> std::io::Result<PathBuf> {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "launcher data directory is unavailable",
            )
        })
}

pub(crate) fn data_directory() -> std::io::Result<PathBuf> {
    let root = data_root()?;
    let directory = root.join("OrbMC");
    let legacy = root.join(LEGACY_NAME);
    if !directory.exists() && legacy.is_dir() {
        // Keep profiles created before the launcher was renamed.
        std::fs::rename(&legacy, &directory)?;
    }
    Ok(directory)
}
