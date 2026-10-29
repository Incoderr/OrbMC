//! What a build is: its identity and saved configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::super::mods::installed::ModsLocation;
use super::super::versions::LoaderChoice;
use crate::platform::instance_layout;

/// Username used for offline launches until a real account is connected.
pub const OFFLINE_PLAYER_NAME: &str = "Player";

/// Validates a Minecraft player name used for offline launches.
///
/// The vanilla rules apply: 3-16 characters, Latin letters, digits and `_`.
pub fn is_valid_player_name(name: &str) -> bool {
    let name = name.trim();
    (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Stable identity of a build; never derived from its name or directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InstanceId(Uuid);

impl InstanceId {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn uuid(self) -> Uuid {
        self.0
    }
}

impl std::fmt::Display for InstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstanceProfile {
    pub id: InstanceId,
    pub name: String,
    pub game_version: String,
    pub loader: LoaderChoice,
    pub loader_version: Option<String>,
    #[serde(default = "default_memory_mb")]
    pub memory_mb: u32,
    #[serde(default)]
    pub java_path: Option<PathBuf>,
    #[serde(default)]
    pub installation_root: PathBuf,
    /// Name of the game directory under `<installation_root>/instances`,
    /// derived from the build name. Empty only for profiles saved by older
    /// launcher versions, which named the directory after the id.
    #[serde(default)]
    pub directory: String,
    pub installed_version_id: Option<String>,
    /// Persisted logical launch order; zero means this profile has never been launched.
    #[serde(default)]
    pub last_launch_order: u64,
    /// Public cover image URL (for example a Modrinth or CurseForge build
    /// icon); used as the Discord status icon when present.
    #[serde(default)]
    pub modpack_icon: Option<String>,
}

impl InstanceProfile {
    pub fn new(
        name: String,
        game_version: String,
        loader: LoaderChoice,
        loader_version: Option<String>,
        installation_root: PathBuf,
    ) -> Self {
        Self {
            id: InstanceId::new(),
            name,
            game_version,
            loader,
            loader_version,
            memory_mb: 4096,
            java_path: None,
            installation_root,
            directory: String::new(),
            installed_version_id: None,
            last_launch_order: 0,
            modpack_icon: None,
        }
    }

    /// Directory Minecraft runs in for this build.
    pub fn game_directory(&self) -> PathBuf {
        instance_layout::game_directory(&self.installation_root, &self.directory)
    }

    /// Launcher-owned directory with the bookkeeping of this build.
    pub fn data_directory(&self) -> std::io::Result<PathBuf> {
        instance_layout::data_directory(self.id.uuid())
    }

    pub fn mods_location(&self) -> std::io::Result<ModsLocation> {
        Ok(ModsLocation {
            mods: self.game_directory().join("mods"),
            index: self.data_directory()?,
        })
    }
}

fn default_memory_mb() -> u32 {
    4096
}
