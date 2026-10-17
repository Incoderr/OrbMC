use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::features::instances::{InstanceProfile, OFFLINE_PLAYER_NAME};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct SavedLauncherSettings {
    pub download_directory: PathBuf,
    pub locale: String,
    pub theme: String,
    pub accent: String,
    pub hide_to_tray: bool,
    pub offline_player: String,
}

impl Default for SavedLauncherSettings {
    fn default() -> Self {
        Self {
            download_directory: PathBuf::new(),
            locale: "ru".to_owned(),
            theme: "dark".to_owned(),
            accent: "orange".to_owned(),
            hide_to_tray: false,
            offline_player: OFFLINE_PLAYER_NAME.to_owned(),
        }
    }
}

pub fn load() -> std::io::Result<Vec<InstanceProfile>> {
    let path = profiles_path()?;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    serde_json::from_slice(&bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

pub fn save(profiles: &[InstanceProfile]) -> std::io::Result<()> {
    write_json_atomically(&profiles_path()?, profiles)
}

pub fn load_settings() -> std::io::Result<SavedLauncherSettings> {
    let path = settings_path()?;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SavedLauncherSettings::default());
        }
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

pub fn save_settings(settings: &SavedLauncherSettings) -> std::io::Result<()> {
    write_json_atomically(&settings_path()?, settings)
}

pub fn default_download_directory() -> std::io::Result<PathBuf> {
    data_directory()
}

pub fn instance_directory(download_directory: &Path, id: uuid::Uuid) -> PathBuf {
    download_directory.join("instances").join(id.to_string())
}

pub fn ensure_instance_directory(
    download_directory: &Path,
    id: uuid::Uuid,
) -> std::io::Result<PathBuf> {
    let path = instance_directory(download_directory, id);
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn remove_instance_directory(download_directory: &Path, id: uuid::Uuid) -> std::io::Result<()> {
    let path = instance_directory(download_directory, id);
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
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

pub(crate) fn data_directory() -> std::io::Result<PathBuf> {
    let root = dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "launcher data directory is unavailable",
            )
        })?;
    Ok(root.join("RS MC Launcher"))
}
