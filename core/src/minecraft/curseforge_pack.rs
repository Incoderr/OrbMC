//! CurseForge modpack manifest (`manifest.json` at the root of a pack zip,
//! `manifestVersion` 1), as the CurseForge app writes it.

use serde::Deserialize;

use super::mrpack::PackLoader;

/// Name of the manifest at the root of a CurseForge pack.
pub const MANIFEST_FILE: &str = "manifest.json";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeManifest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    pub minecraft: ManifestMinecraft,
    #[serde(default)]
    pub files: Vec<ManifestFile>,
    /// Folder inside the zip copied over the game directory.
    #[serde(default = "default_overrides")]
    pub overrides: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestMinecraft {
    pub version: String,
    #[serde(default)]
    pub mod_loaders: Vec<ManifestLoader>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ManifestLoader {
    /// `forge-47.2.0`, `neoforge-21.1.77`, `fabric-0.15.11`, `quilt-0.26.0`.
    pub id: String,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ManifestFile {
    #[serde(rename = "projectID")]
    pub project_id: u64,
    #[serde(rename = "fileID")]
    pub file_id: u64,
    /// `false` marks a file the author left switched off.
    #[serde(default = "default_required")]
    pub required: bool,
}

fn default_overrides() -> String {
    "overrides".to_owned()
}

fn default_required() -> bool {
    true
}

impl CurseForgeManifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    /// The primary loader, or the first one when none is marked primary.
    /// An id this launcher does not know reads as vanilla.
    pub fn loader(&self) -> PackLoader {
        let loader = self
            .minecraft
            .mod_loaders
            .iter()
            .find(|loader| loader.primary)
            .or_else(|| self.minecraft.mod_loaders.first());
        let Some((kind, version)) = loader.and_then(|loader| loader.id.split_once('-')) else {
            return PackLoader::Vanilla;
        };
        let version = version.to_owned();
        match kind {
            "forge" => PackLoader::Forge(version),
            "neoforge" => PackLoader::NeoForge(version),
            "fabric" => PackLoader::Fabric(version),
            "quilt" => PackLoader::Quilt(version),
            _ => PackLoader::Vanilla,
        }
    }

    /// Files the pack needs; optional ones the author switched off are left out.
    pub fn required_files(&self) -> impl Iterator<Item = &ManifestFile> {
        self.files.iter().filter(|file| file.required)
    }
}

/// Game directory folder for a CurseForge project class (`classId`), or
/// `None` for classes a pack file cannot simply be dropped in for (worlds,
/// data packs, modpacks).
pub fn class_folder(class_id: u64) -> Option<&'static str> {
    match class_id {
        6 => Some("mods"),
        12 => Some("resourcepacks"),
        6552 => Some("shaderpacks"),
        _ => None,
    }
}

/// Whether a CurseForge file name can be used as a file name as it is.
pub fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
        "minecraft": {
            "version": "1.20.1",
            "modLoaders": [{"id": "forge-47.2.0", "primary": true}]
        },
        "manifestType": "minecraftModpack",
        "manifestVersion": 1,
        "name": "All the Mods",
        "version": "1.0.0",
        "author": "someone",
        "files": [
            {"projectID": 238222, "fileID": 4712345, "required": true},
            {"projectID": 1, "fileID": 2, "required": false}
        ],
        "overrides": "overrides"
    }"#;

    #[test]
    fn reads_versions_loader_and_required_files() {
        let manifest = CurseForgeManifest::parse(MANIFEST.as_bytes()).unwrap();
        assert_eq!(manifest.name, "All the Mods");
        assert_eq!(manifest.minecraft.version, "1.20.1");
        assert_eq!(manifest.loader(), PackLoader::Forge("47.2.0".into()));
        let files: Vec<_> = manifest
            .required_files()
            .map(|file| (file.project_id, file.file_id))
            .collect();
        assert_eq!(files, [(238222, 4712345)]);
        assert_eq!(manifest.overrides, "overrides");
    }

    #[test]
    fn loaders_without_a_primary_mark_use_the_first() {
        let manifest = CurseForgeManifest::parse(
            br#"{"minecraft":{"version":"1.21.1","modLoaders":[{"id":"neoforge-21.1.77"}]}}"#,
        )
        .unwrap();
        assert_eq!(manifest.loader(), PackLoader::NeoForge("21.1.77".into()));
        assert_eq!(manifest.overrides, "overrides");
        assert!(manifest.files.is_empty());
    }

    #[test]
    fn missing_or_unknown_loaders_read_as_vanilla() {
        let manifest = CurseForgeManifest::parse(br#"{"minecraft":{"version":"1.20.1"}}"#).unwrap();
        assert_eq!(manifest.loader(), PackLoader::Vanilla);
        let manifest = CurseForgeManifest::parse(
            br#"{"minecraft":{"version":"1.20.1","modLoaders":[{"id":"liteloader"}]}}"#,
        )
        .unwrap();
        assert_eq!(manifest.loader(), PackLoader::Vanilla);
    }

    #[test]
    fn classes_map_to_content_folders() {
        assert_eq!(class_folder(6), Some("mods"));
        assert_eq!(class_folder(12), Some("resourcepacks"));
        assert_eq!(class_folder(6552), Some("shaderpacks"));
        assert_eq!(class_folder(17), None);
    }

    #[test]
    fn file_names_cannot_leave_their_folder() {
        assert!(is_plain_file_name("jei-1.20.1.jar"));
        assert!(!is_plain_file_name("../jei.jar"));
        assert!(!is_plain_file_name("a\\b.jar"));
        assert!(!is_plain_file_name(".."));
        assert!(!is_plain_file_name(""));
    }
}
