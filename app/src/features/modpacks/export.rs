//! Exporting a build as a Modrinth (`.mrpack`) or CurseForge (`.zip`) pack.
//!
//! Files in the content folders are looked up by hash: those the chosen
//! provider hosts are linked from the index or manifest, everything else
//! the author ticked is packed into `overrides/`. Hashes are the only thing
//! sent to the providers; file names and paths never leave the computer.

use std::collections::HashMap;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use orbmc_core::minecraft::mrpack::{INDEX_FILE, PackLoader};
use orbmc_core::minecraft::pack_export::{
    self, CURSEFORGE_MANIFEST, CurseForgeFile, ModrinthFile, OVERRIDES_FOLDER, PackInfo,
    PackRuntime,
};
use serde::Deserialize;

use super::super::mods::CurseForgeApi;
use super::super::versions::LoaderChoice;
use crate::platform::mod_files;

/// Hashes sent in one provider request.
const LOOKUP_BATCH: usize = 100;
/// Suffix the launcher gives switched-off mods; the game never loads them.
const DISABLED_SUFFIX: &str = ".disabled";

/// Where the pack is meant to be published.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Modrinth,
    CurseForge,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Modrinth => "mrpack",
            Self::CurseForge => "zip",
        }
    }
}

/// Why a pack could not be exported. The messages are shown to the user.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("У сборки не указана версия загрузчика")]
    MissingLoaderVersion,
    #[error("CurseForge недоступен в этой версии лаунчера")]
    CurseForgeUnavailable,
    #[error("Не удалось проверить файлы на {provider}: {message}")]
    Lookup {
        provider: &'static str,
        message: String,
    },
    #[error("Не удалось прочитать файлы сборки: {0}")]
    Io(#[from] std::io::Error),
    #[error("Не удалось записать архив: {0}")]
    Archive(#[from] zip::result::ZipError),
    #[error("Не удалось собрать описание сборки: {0}")]
    Manifest(#[from] serde_json::Error),
}

/// Everything an export needs, gathered on the UI thread.
#[derive(Clone, Debug)]
pub struct ExportRequest {
    pub format: ExportFormat,
    pub info: PackInfo,
    pub game_directory: PathBuf,
    /// Ticked top-level entries of the game directory.
    pub entries: Vec<String>,
    pub destination: PathBuf,
}

/// How the exported files ended up in the pack.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExportSummary {
    /// Files linked to the provider.
    pub linked: usize,
    /// Files packed into `overrides/`.
    pub packed: usize,
}

/// Game version and loader of a build in the form the pack formats use.
pub fn pack_runtime(
    game_version: &str,
    loader: LoaderChoice,
    loader_version: Option<&str>,
) -> Result<PackRuntime, ExportError> {
    let version = || {
        loader_version
            .map(str::to_owned)
            .ok_or(ExportError::MissingLoaderVersion)
    };
    let loader = match loader {
        LoaderChoice::Vanilla => PackLoader::Vanilla,
        LoaderChoice::Fabric => PackLoader::Fabric(version()?),
        LoaderChoice::Forge => PackLoader::Forge(version()?),
        LoaderChoice::NeoForge => PackLoader::NeoForge(version()?),
    };
    Ok(PackRuntime {
        game_version: game_version.to_owned(),
        loader,
    })
}

/// Top-level entries of a game directory, folders first, each with whether
/// it is ticked by default.
pub fn top_level_entries(game_directory: &Path) -> std::io::Result<Vec<(String, bool)>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(game_directory)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let file_type = entry.file_type()?;
        if file_type.is_dir() || file_type.is_file() {
            entries.push((file_type.is_file(), name));
        }
    }
    entries.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.to_lowercase().cmp(&right.1.to_lowercase()))
    });
    Ok(entries
        .into_iter()
        .map(|(_, name)| {
            let ticked = pack_export::included_by_default(&name);
            (name, ticked)
        })
        .collect())
}

/// Writes the pack. Blocking: run it off the UI thread.
pub fn export_pack(
    request: &ExportRequest,
    curseforge: Option<&CurseForgeApi>,
) -> Result<ExportSummary, ExportError> {
    if request.format == ExportFormat::CurseForge && curseforge.is_none() {
        return Err(ExportError::CurseForgeUnavailable);
    }
    let files = collect_files(&request.game_directory, &request.entries)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("orbmc/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| lookup_error(request.format, error))?;

    let mut content = Vec::new();
    for file in files.iter().filter(|file| file.content) {
        let sha1 =
            mc_launcher_core::io::hash::sha1_file(&file.absolute).map_err(std::io::Error::other)?;
        let fingerprint = match request.format {
            ExportFormat::CurseForge => mod_files::fingerprint(&std::fs::read(&file.absolute)?),
            ExportFormat::Modrinth => 0,
        };
        content.push((file.relative.as_str(), sha1, fingerprint));
    }

    let temporary = request.destination.with_extension("part");
    let result = (|| {
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&temporary)?);
        let summary = match request.format {
            ExportFormat::Modrinth => {
                let hashes: Vec<&str> = content.iter().map(|(_, sha1, _)| sha1.as_str()).collect();
                let hosted = modrinth_files(&client, &hashes)?;
                let linked: Vec<ModrinthFile> = content
                    .iter()
                    .filter_map(|(path, sha1, _)| {
                        hosted.get(sha1).map(|file| ModrinthFile {
                            path: (*path).to_owned(),
                            ..file.clone()
                        })
                    })
                    .collect();
                let index = pack_export::mrpack_index(&request.info, &linked)?;
                write_pack(&mut writer, INDEX_FILE, &index, &files, |path| {
                    linked.iter().any(|file| file.path == path)
                })?
            }
            ExportFormat::CurseForge => {
                let api = curseforge.ok_or(ExportError::CurseForgeUnavailable)?;
                let fingerprints: Vec<u32> = content.iter().map(|(_, _, print)| *print).collect();
                let hosted = curseforge_files(&client, api, &fingerprints)?;
                let mut linked_paths = Vec::new();
                let mut linked = Vec::new();
                for (path, sha1, _) in &content {
                    if let Some(file) = hosted.get(&sha1.to_ascii_lowercase()) {
                        linked_paths.push(*path);
                        linked.push(*file);
                    }
                }
                let manifest = pack_export::curseforge_manifest(&request.info, &linked)?;
                write_pack(
                    &mut writer,
                    CURSEFORGE_MANIFEST,
                    &manifest,
                    &files,
                    |path| linked_paths.contains(&path),
                )?
            }
        };
        writer.finish()?.sync_all()?;
        Ok(summary)
    })();
    match result {
        Ok(summary) => {
            std::fs::rename(&temporary, &request.destination)?;
            tracing::info!(
                format = ?request.format,
                linked = summary.linked,
                packed = summary.packed,
                "exported build as a modpack"
            );
            Ok(summary)
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(error)
        }
    }
}

/// One file of the game directory that goes into the pack.
#[derive(Debug)]
struct PackFile {
    /// `/`-separated path relative to the game directory.
    relative: String,
    absolute: PathBuf,
    /// The providers may host it (see `pack_export::is_content_file`).
    content: bool,
}

fn collect_files(game_directory: &Path, entries: &[String]) -> std::io::Result<Vec<PackFile>> {
    let mut files = Vec::new();
    for entry in entries {
        walk(game_directory, &game_directory.join(entry), &mut files)?;
    }
    files.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(files)
}

fn walk(root: &Path, path: &Path, files: &mut Vec<PackFile>) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path)? {
            walk(root, &entry?.path(), files)?;
        }
    } else if metadata.is_file() {
        let Some(relative) = path.strip_prefix(root).ok().and_then(|relative| {
            relative
                .components()
                .map(|component| component.as_os_str().to_str())
                .collect::<Option<Vec<_>>>()
                .map(|parts| parts.join("/"))
        }) else {
            tracing::warn!(path = %path.display(), "skipped a file with a non-UTF-8 name");
            return Ok(());
        };
        // Switched-off mods stay out of the pack, as the game never loads them.
        let switched_off = relative.ends_with(DISABLED_SUFFIX)
            && pack_export::CONTENT_FOLDERS
                .iter()
                .any(|folder| relative.starts_with(&format!("{folder}/")));
        if !switched_off {
            files.push(PackFile {
                content: pack_export::is_content_file(&relative),
                relative,
                absolute: path.to_path_buf(),
            });
        }
    }
    Ok(())
}

/// Writes the manifest and every file that is not linked into `overrides/`.
fn write_pack<W: Write + Seek>(
    writer: &mut zip::ZipWriter<W>,
    manifest_name: &str,
    manifest: &[u8],
    files: &[PackFile],
    is_linked: impl Fn(&str) -> bool,
) -> Result<ExportSummary, ExportError> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    writer.start_file(manifest_name, options)?;
    writer.write_all(manifest)?;
    let mut summary = ExportSummary::default();
    for file in files {
        if is_linked(&file.relative) {
            summary.linked += 1;
            continue;
        }
        writer.start_file(format!("{OVERRIDES_FOLDER}/{}", file.relative), options)?;
        std::io::copy(&mut std::fs::File::open(&file.absolute)?, writer)?;
        summary.packed += 1;
    }
    Ok(summary)
}

/// Modrinth files by SHA-1, for the hashes Modrinth knows.
fn modrinth_files(
    client: &reqwest::blocking::Client,
    hashes: &[&str],
) -> Result<HashMap<String, ModrinthFile>, ExportError> {
    #[derive(Deserialize)]
    struct Version {
        files: Vec<VersionFile>,
    }
    #[derive(Deserialize)]
    struct VersionFile {
        hashes: VersionHashes,
        url: String,
        size: u64,
    }
    #[derive(Deserialize)]
    struct VersionHashes {
        sha1: String,
        sha512: String,
    }

    let mut found = HashMap::new();
    for batch in hashes.chunks(LOOKUP_BATCH) {
        let versions: HashMap<String, Version> = client
            .post("https://api.modrinth.com/v2/version_files")
            .header("Content-Type", "application/json")
            .body(serde_json::json!({ "hashes": batch, "algorithm": "sha1" }).to_string())
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.text())
            .map_err(|error| lookup_error(ExportFormat::Modrinth, error))
            .and_then(|body| {
                serde_json::from_str(&body)
                    .map_err(|error| lookup_error(ExportFormat::Modrinth, error))
            })?;
        for (sha1, version) in versions {
            // A version may carry several files; the hash picks ours.
            if let Some(file) = version
                .files
                .into_iter()
                .find(|file| file.hashes.sha1.eq_ignore_ascii_case(&sha1))
            {
                found.insert(
                    sha1,
                    ModrinthFile {
                        path: String::new(),
                        sha1: file.hashes.sha1,
                        sha512: file.hashes.sha512,
                        url: file.url,
                        size: file.size,
                    },
                );
            }
        }
    }
    Ok(found)
}

/// CurseForge files by lowercase SHA-1, for the fingerprints CurseForge knows.
fn curseforge_files(
    client: &reqwest::blocking::Client,
    api: &CurseForgeApi,
    fingerprints: &[u32],
) -> Result<HashMap<String, CurseForgeFile>, ExportError> {
    #[derive(Deserialize)]
    struct Response {
        data: Data,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Data {
        exact_matches: Vec<Match>,
    }
    #[derive(Deserialize)]
    struct Match {
        file: File,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct File {
        id: u64,
        mod_id: u64,
        hashes: Vec<Hash>,
    }
    #[derive(Deserialize)]
    struct Hash {
        algo: u8,
        value: String,
    }
    /// `algo` of a SHA-1 hash in CurseForge answers.
    const SHA1: u8 = 1;

    let mut found = HashMap::new();
    for batch in fingerprints.chunks(LOOKUP_BATCH) {
        let response: Response = api
            .authorize(client.post(api.url("/v1/fingerprints/432")))
            .header("Content-Type", "application/json")
            .body(serde_json::json!({ "fingerprints": batch }).to_string())
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.text())
            .map_err(|error| lookup_error(ExportFormat::CurseForge, error))
            .and_then(|body| {
                serde_json::from_str(&body)
                    .map_err(|error| lookup_error(ExportFormat::CurseForge, error))
            })?;
        // SHA-1 guards against 32-bit fingerprint collisions.
        for found_match in response.data.exact_matches {
            let file = found_match.file;
            for hash in file.hashes.iter().filter(|hash| hash.algo == SHA1) {
                found.insert(
                    hash.value.to_ascii_lowercase(),
                    CurseForgeFile {
                        project_id: file.mod_id,
                        file_id: file.id,
                    },
                );
            }
        }
    }
    Ok(found)
}

fn lookup_error(format: ExportFormat, error: impl std::fmt::Display) -> ExportError {
    ExportError::Lookup {
        provider: match format {
            ExportFormat::Modrinth => "Modrinth",
            ExportFormat::CurseForge => "CurseForge",
        },
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn game_directory() -> tempfile::TempDir {
        let game = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = game.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("mods/linked.jar", "linked");
        write("mods/own.jar", "own");
        write("mods/off.jar.disabled", "off");
        write("config/own.toml", "a = 1");
        write("saves/world/level.dat", "world");
        write("options.txt", "fov:1.0");
        game
    }

    #[test]
    fn collects_ticked_entries_without_switched_off_mods() {
        let game = game_directory();
        let files = collect_files(
            game.path(),
            &["mods".into(), "config".into(), "options.txt".into()],
        )
        .unwrap();
        let paths: Vec<_> = files
            .iter()
            .map(|file| (file.relative.as_str(), file.content))
            .collect();
        assert_eq!(
            paths,
            [
                ("config/own.toml", false),
                ("mods/linked.jar", true),
                ("mods/own.jar", true),
                ("options.txt", false),
            ]
        );
    }

    #[test]
    fn linked_files_stay_out_of_the_overrides() {
        let game = game_directory();
        let files = collect_files(game.path(), &["mods".into(), "config".into()]).unwrap();
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let summary = write_pack(&mut writer, INDEX_FILE, b"{}", &files, |path| {
            path == "mods/linked.jar"
        })
        .unwrap();
        assert_eq!(
            summary,
            ExportSummary {
                linked: 1,
                packed: 2
            }
        );
        let mut archive = zip::ZipArchive::new(writer.finish().unwrap()).unwrap();
        let mut names: Vec<_> = archive.file_names().map(str::to_owned).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "modrinth.index.json",
                "overrides/config/own.toml",
                "overrides/mods/own.jar",
            ]
        );
        let mut text = String::new();
        archive
            .by_name("overrides/config/own.toml")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "a = 1");
    }

    #[test]
    fn personal_folders_are_offered_unticked() {
        let game = game_directory();
        let entries = top_level_entries(game.path()).unwrap();
        assert_eq!(
            entries,
            [
                ("config".to_owned(), true),
                ("mods".to_owned(), true),
                ("saves".to_owned(), false),
                ("options.txt".to_owned(), true),
            ]
        );
    }

    #[test]
    fn loader_builds_need_a_loader_version() {
        assert!(matches!(
            pack_runtime("1.20.1", LoaderChoice::Fabric, None),
            Err(ExportError::MissingLoaderVersion)
        ));
        assert_eq!(
            pack_runtime("1.20.1", LoaderChoice::Vanilla, None)
                .unwrap()
                .loader,
            PackLoader::Vanilla
        );
    }
}
