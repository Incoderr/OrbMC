//! Exporting a build as a modpack: the Modrinth index (`.mrpack`) and the
//! CurseForge manifest (`manifest.json`), and which files of the game
//! directory go into the archive.
//!
//! Formats:
//! <https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack>
//! and the `manifest.json` the CurseForge app writes (`manifestVersion` 1).

use std::collections::BTreeMap;

use serde::Serialize;

use super::mrpack::PackLoader;

/// Folder inside both archives whose contents are copied over the game
/// directory on install.
pub const OVERRIDES_FOLDER: &str = "overrides";
/// Name of the CurseForge manifest at the root of its archive.
pub const CURSEFORGE_MANIFEST: &str = "manifest.json";

/// Folders whose files the providers may know: their files are looked up by
/// hash and linked instead of being packed.
pub const CONTENT_FOLDERS: [&str; 3] = ["mods", "resourcepacks", "shaderpacks"];

/// Top-level entries of a game directory that belong to the player or the
/// session, not to the pack. They are unticked by default.
const PERSONAL_ENTRIES: [&str; 15] = [
    "saves",
    "logs",
    "crash-reports",
    "screenshots",
    "backups",
    "downloads",
    "journeymap",
    "xaero",
    ".fabric",
    ".mixin.out",
    "servers.dat",
    "servers.dat_old",
    "usercache.json",
    "usernamecache.json",
    "command_history.txt",
];

/// Whether a top-level entry of the game directory is offered ticked.
pub fn included_by_default(name: &str) -> bool {
    !PERSONAL_ENTRIES
        .iter()
        .any(|personal| personal.eq_ignore_ascii_case(name))
        && !name.ends_with(".log")
}

/// Whether a pack path (`/`-separated, relative to the game directory) is a
/// file a provider may host: directly inside one of `CONTENT_FOLDERS`.
pub fn is_content_file(path: &str) -> bool {
    let Some((folder, file)) = path.split_once('/') else {
        return false;
    };
    CONTENT_FOLDERS.contains(&folder)
        && !file.contains('/')
        && [".jar", ".zip"]
            .iter()
            .any(|extension| file.to_ascii_lowercase().ends_with(extension))
}

/// What a pack runs on: the game version and the loader with its version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackRuntime {
    pub game_version: String,
    pub loader: PackLoader,
}

impl PackRuntime {
    /// Loader version without a `<game version>-` prefix: older Forge builds
    /// store the full id (`1.20.1-47.2.0`), both formats want `47.2.0`.
    fn bare_loader_version<'a>(&self, version: &'a str) -> &'a str {
        version
            .strip_prefix(&self.game_version)
            .and_then(|rest| rest.strip_prefix('-'))
            .unwrap_or(version)
    }

    /// `dependencies` of a Modrinth index.
    fn mrpack_dependencies(&self) -> BTreeMap<&'static str, String> {
        let mut dependencies = BTreeMap::from([("minecraft", self.game_version.clone())]);
        let loader = match &self.loader {
            PackLoader::Vanilla => None,
            PackLoader::Fabric(version) => Some(("fabric-loader", version)),
            PackLoader::Forge(version) => Some(("forge", version)),
            PackLoader::NeoForge(version) => Some(("neoforge", version)),
            PackLoader::Quilt(version) => Some(("quilt-loader", version)),
        };
        if let Some((key, version)) = loader {
            dependencies.insert(key, self.bare_loader_version(version).to_owned());
        }
        dependencies
    }

    /// `modLoaders[].id` of a CurseForge manifest (`forge-47.2.0`).
    fn curseforge_loader_id(&self) -> Option<String> {
        let (prefix, version) = match &self.loader {
            PackLoader::Vanilla => return None,
            PackLoader::Fabric(version) => ("fabric", version),
            PackLoader::Forge(version) => ("forge", version),
            PackLoader::NeoForge(version) => ("neoforge", version),
            PackLoader::Quilt(version) => ("quilt", version),
        };
        Some(format!("{prefix}-{}", self.bare_loader_version(version)))
    }
}

/// Name and version the author gives the pack.
#[derive(Clone, Debug)]
pub struct PackInfo {
    pub name: String,
    pub version: String,
    pub author: String,
    pub runtime: PackRuntime,
}

/// A file Modrinth hosts, linked from the index instead of packed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModrinthFile {
    pub path: String,
    pub sha1: String,
    pub sha512: String,
    pub url: String,
    pub size: u64,
}

/// A file CurseForge hosts, linked from the manifest instead of packed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurseForgeFile {
    pub project_id: u64,
    pub file_id: u64,
}

/// `modrinth.index.json` of an exported pack.
pub fn mrpack_index(info: &PackInfo, files: &[ModrinthFile]) -> serde_json::Result<Vec<u8>> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Index<'a> {
        format_version: u32,
        game: &'static str,
        version_id: &'a str,
        name: &'a str,
        files: Vec<File<'a>>,
        dependencies: BTreeMap<&'static str, String>,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct File<'a> {
        path: &'a str,
        hashes: Hashes<'a>,
        downloads: [&'a str; 1],
        file_size: u64,
    }
    #[derive(Serialize)]
    struct Hashes<'a> {
        sha1: &'a str,
        sha512: &'a str,
    }

    serde_json::to_vec_pretty(&Index {
        format_version: 1,
        game: "minecraft",
        version_id: &info.version,
        name: &info.name,
        files: files
            .iter()
            .map(|file| File {
                path: &file.path,
                hashes: Hashes {
                    sha1: &file.sha1,
                    sha512: &file.sha512,
                },
                downloads: [&file.url],
                file_size: file.size,
            })
            .collect(),
        dependencies: info.runtime.mrpack_dependencies(),
    })
}

/// `manifest.json` of an exported CurseForge pack.
pub fn curseforge_manifest(
    info: &PackInfo,
    files: &[CurseForgeFile],
) -> serde_json::Result<Vec<u8>> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Manifest<'a> {
        minecraft: Minecraft<'a>,
        manifest_type: &'static str,
        manifest_version: u32,
        name: &'a str,
        version: &'a str,
        author: &'a str,
        files: Vec<File>,
        overrides: &'static str,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Minecraft<'a> {
        version: &'a str,
        mod_loaders: Vec<ModLoader>,
    }
    #[derive(Serialize)]
    struct ModLoader {
        id: String,
        primary: bool,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct File {
        #[serde(rename = "projectID")]
        project_id: u64,
        #[serde(rename = "fileID")]
        file_id: u64,
        required: bool,
    }

    serde_json::to_vec_pretty(&Manifest {
        minecraft: Minecraft {
            version: &info.runtime.game_version,
            mod_loaders: info
                .runtime
                .curseforge_loader_id()
                .map(|id| ModLoader { id, primary: true })
                .into_iter()
                .collect(),
        },
        manifest_type: "minecraftModpack",
        manifest_version: 1,
        name: &info.name,
        version: &info.version,
        author: &info.author,
        files: files
            .iter()
            .map(|file| File {
                project_id: file.project_id,
                file_id: file.file_id,
                required: true,
            })
            .collect(),
        overrides: OVERRIDES_FOLDER,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minecraft::mrpack::MrpackIndex;

    fn info(loader: PackLoader) -> PackInfo {
        PackInfo {
            name: "My Pack".into(),
            version: "1.0.0".into(),
            author: "me".into(),
            runtime: PackRuntime {
                game_version: "1.20.1".into(),
                loader,
            },
        }
    }

    #[test]
    fn mrpack_index_reads_back_with_the_install_parser() {
        let file = ModrinthFile {
            path: "mods/sodium.jar".into(),
            sha1: "aa".into(),
            sha512: "bb".into(),
            url: "https://cdn.modrinth.com/sodium.jar".into(),
            size: 100,
        };
        let bytes = mrpack_index(&info(PackLoader::Fabric("0.15.11".into())), &[file]).unwrap();
        let index = MrpackIndex::parse(&bytes).unwrap();
        assert_eq!(index.name, "My Pack");
        assert_eq!(index.version_id, "1.0.0");
        assert_eq!(index.minecraft_version(), Some("1.20.1"));
        assert_eq!(index.loader(), PackLoader::Fabric("0.15.11".into()));
        assert_eq!(index.files[0].path, "mods/sodium.jar");
        assert_eq!(index.files[0].hashes.sha1, "aa");
        assert_eq!(index.files[0].file_size, 100);
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["formatVersion"], 1);
        assert_eq!(json["game"], "minecraft");
        assert_eq!(json["files"][0]["hashes"]["sha512"], "bb");
    }

    #[test]
    fn forge_versions_lose_the_game_version_prefix() {
        let bytes = mrpack_index(&info(PackLoader::Forge("1.20.1-47.2.0".into())), &[]).unwrap();
        let index = MrpackIndex::parse(&bytes).unwrap();
        assert_eq!(index.loader(), PackLoader::Forge("47.2.0".into()));

        let bytes =
            curseforge_manifest(&info(PackLoader::Forge("1.20.1-47.2.0".into())), &[]).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["minecraft"]["modLoaders"][0]["id"], "forge-47.2.0");
    }

    #[test]
    fn curseforge_manifest_has_the_fields_the_app_reads() {
        let files = [CurseForgeFile {
            project_id: 238222,
            file_id: 4712345,
        }];
        let bytes =
            curseforge_manifest(&info(PackLoader::NeoForge("21.1.77".into())), &files).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["manifestType"], "minecraftModpack");
        assert_eq!(json["manifestVersion"], 1);
        assert_eq!(json["minecraft"]["version"], "1.20.1");
        assert_eq!(json["minecraft"]["modLoaders"][0]["id"], "neoforge-21.1.77");
        assert_eq!(json["minecraft"]["modLoaders"][0]["primary"], true);
        assert_eq!(json["files"][0]["projectID"], 238222);
        assert_eq!(json["files"][0]["fileID"], 4712345);
        assert_eq!(json["files"][0]["required"], true);
        assert_eq!(json["overrides"], "overrides");
    }

    #[test]
    fn vanilla_packs_name_no_loader() {
        let bytes = curseforge_manifest(&info(PackLoader::Vanilla), &[]).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["minecraft"]["modLoaders"].as_array().unwrap().len(), 0);
        let bytes = mrpack_index(&info(PackLoader::Vanilla), &[]).unwrap();
        assert_eq!(MrpackIndex::parse(&bytes).unwrap().dependencies.len(), 1);
    }

    #[test]
    fn content_files_sit_directly_in_content_folders() {
        assert!(is_content_file("mods/sodium.jar"));
        assert!(is_content_file("resourcepacks/Faithful.ZIP"));
        assert!(is_content_file("shaderpacks/complementary.zip"));
        assert!(!is_content_file("mods/sodium.jar.disabled"));
        assert!(!is_content_file("mods/sub/inner.jar"));
        assert!(!is_content_file("config/sodium.json"));
        assert!(!is_content_file("sodium.jar"));
    }

    #[test]
    fn personal_entries_are_left_out_by_default() {
        assert!(included_by_default("mods"));
        assert!(included_by_default("config"));
        assert!(included_by_default("options.txt"));
        assert!(!included_by_default("saves"));
        assert!(!included_by_default("logs"));
        assert!(!included_by_default("Screenshots"));
        assert!(!included_by_default("latest.log"));
        assert!(!included_by_default("command_history.txt"));
    }
}
