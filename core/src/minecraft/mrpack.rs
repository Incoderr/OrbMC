//! Modrinth modpack index (`modrinth.index.json` inside a `.mrpack`).
//!
//! Format: <https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack>

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

/// Name of the index file at the root of a `.mrpack` archive.
pub const INDEX_FILE: &str = "modrinth.index.json";
/// Folders whose contents are copied over the game directory.
pub const OVERRIDE_FOLDERS: [&str; 2] = ["overrides", "client-overrides"];

#[derive(Clone, Debug, Deserialize)]
pub struct MrpackIndex {
    pub name: String,
    #[serde(rename = "versionId")]
    pub version_id: String,
    #[serde(default)]
    pub files: Vec<MrpackFile>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MrpackFile {
    /// Destination relative to the game directory, `/`-separated.
    pub path: String,
    pub hashes: MrpackHashes,
    pub downloads: Vec<String>,
    #[serde(rename = "fileSize", default)]
    pub file_size: u64,
    #[serde(default)]
    pub env: Option<MrpackEnv>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MrpackHashes {
    pub sha1: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MrpackEnv {
    #[serde(default)]
    pub client: Option<String>,
}

/// Mod loader a pack is built for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackLoader {
    Vanilla,
    Fabric(String),
    Forge(String),
    NeoForge(String),
    Quilt(String),
}

impl MrpackIndex {
    pub fn parse(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    pub fn minecraft_version(&self) -> Option<&str> {
        self.dependencies.get("minecraft").map(String::as_str)
    }

    pub fn loader(&self) -> PackLoader {
        let version = |key: &str| self.dependencies.get(key).cloned();
        if let Some(version) = version("fabric-loader") {
            PackLoader::Fabric(version)
        } else if let Some(version) = version("neoforge") {
            PackLoader::NeoForge(version)
        } else if let Some(version) = version("forge") {
            PackLoader::Forge(version)
        } else if let Some(version) = version("quilt-loader") {
            PackLoader::Quilt(version)
        } else {
            PackLoader::Vanilla
        }
    }

    /// Files the game client needs: everything not marked unsupported on the
    /// client (optional files are taken too, as the Modrinth App does).
    pub fn client_files(&self) -> impl Iterator<Item = &MrpackFile> {
        self.files.iter().filter(|file| {
            file.env.as_ref().and_then(|env| env.client.as_deref()) != Some("unsupported")
        })
    }
}

/// A pack path as a relative path that stays inside the game directory, or
/// `None` for absolute paths, `..` and drive prefixes.
pub fn safe_relative_path(path: &str) -> Option<PathBuf> {
    if path.is_empty() || path.contains('\\') || path.contains(':') {
        return None;
    }
    let path = Path::new(path);
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
        .then(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"{
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": "5.2.0",
        "name": "Example Pack",
        "files": [
            {
                "path": "mods/sodium.jar",
                "hashes": {"sha1": "aa", "sha512": "bb"},
                "downloads": ["https://cdn.modrinth.com/sodium.jar"],
                "fileSize": 100,
                "env": {"client": "required", "server": "unsupported"}
            },
            {
                "path": "mods/server-only.jar",
                "hashes": {"sha1": "cc"},
                "downloads": ["https://cdn.modrinth.com/server.jar"],
                "fileSize": 50,
                "env": {"client": "unsupported", "server": "required"}
            }
        ],
        "dependencies": {"minecraft": "1.20.1", "fabric-loader": "0.15.11"}
    }"#;

    #[test]
    fn reads_versions_loader_and_client_files() {
        let index = MrpackIndex::parse(INDEX.as_bytes()).unwrap();
        assert_eq!(index.name, "Example Pack");
        assert_eq!(index.minecraft_version(), Some("1.20.1"));
        assert_eq!(index.loader(), PackLoader::Fabric("0.15.11".into()));
        let files: Vec<_> = index
            .client_files()
            .map(|file| file.path.as_str())
            .collect();
        assert_eq!(files, ["mods/sodium.jar"]);
    }

    #[test]
    fn rejects_paths_leaving_the_game_directory() {
        assert_eq!(
            safe_relative_path("mods/a.jar"),
            Some(PathBuf::from("mods/a.jar"))
        );
        assert_eq!(safe_relative_path("../a.jar"), None);
        assert_eq!(safe_relative_path("mods/../../a.jar"), None);
        assert_eq!(safe_relative_path("/etc/passwd"), None);
        assert_eq!(safe_relative_path("C:/Windows/a.dll"), None);
        assert_eq!(safe_relative_path("mods\\a.jar"), None);
        assert_eq!(safe_relative_path(""), None);
    }
}
