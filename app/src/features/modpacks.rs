//! Modpacks installed as new builds: Modrinth `.mrpack` and CurseForge
//! `.zip` packs, from the catalog or from a file on disk (import), and
//! builds exported as packs (`export`, not offered yet).
//!
//! The pack archive is unpacked into a work folder, every file its index or
//! manifest lists is downloaded into the game directory (verified, several
//! at a time) and the overrides folders are copied over it. The game and
//! loader are then installed like for any other build.

mod curseforge;
mod export;

pub use export::{
    ExportFormat, ExportRequest, ExportSummary, export_pack, pack_runtime, top_level_entries,
};

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::TryStreamExt as _;
use gpui_kit::http_client::HttpClient;
use orbmc_core::minecraft::curseforge_pack::{self, CurseForgeManifest};
use orbmc_core::minecraft::mrpack::{
    INDEX_FILE, MrpackIndex, OVERRIDE_FOLDERS, PackLoader, safe_relative_path,
};

use super::downloads::{DownloadError, FileDownload, Transfer, download_file};
use super::versions::LoaderChoice;

/// Exporting builds as packs works but is not offered yet: the menu item
/// stays hidden until the feature is wanted.
pub const EXPORT_ENABLED: bool = false;

/// Extensions a pack file picked on disk may have.
pub const PACK_EXTENSIONS: &[&str] = &["mrpack", "zip"];

/// Files of a pack downloaded at the same time.
const PARALLEL_DOWNLOADS: usize = 4;
/// Largest single file a pack may reference.
const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;
/// Largest index or manifest read from a pack.
const MAX_MANIFEST_BYTES: u64 = 32 * 1024 * 1024;

/// Why a modpack could not be installed. The messages are shown to the user.
#[derive(Debug, thiserror::Error)]
pub enum ModpackError {
    #[error("Не удалось скачать сборку: {0}")]
    Download(#[from] DownloadError),
    #[error("Архив сборки повреждён: {0}")]
    Archive(String),
    #[error(
        "Это не сборка Modrinth или CurseForge: в архиве нет modrinth.index.json или manifest.json"
    )]
    NotAPack,
    #[error("В сборке есть файл с недопустимым путём: {0}")]
    UnsafePath(String),
    #[error("Сборка не указывает версию Minecraft")]
    MissingGameVersion,
    #[error("Загрузчик Quilt пока не поддерживается")]
    UnsupportedLoader,
    #[error("Сборки CurseForge недоступны в этой версии лаунчера")]
    CurseForgeUnavailable,
    #[error("Не удалось получить файлы сборки с CurseForge: {0}")]
    CurseForge(String),
    #[error("Не удалось разложить файлы сборки: {0}")]
    Io(#[from] std::io::Error),
}

impl ModpackError {
    /// The player cancelled the install.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Download(DownloadError::Cancelled))
    }
}

/// Game version and loader a pack asks for.
#[derive(Clone, Debug)]
pub struct PackTarget {
    pub game_version: String,
    pub loader: LoaderChoice,
    pub loader_version: Option<String>,
}

impl PackTarget {
    fn new(game_version: Option<&str>, loader: PackLoader) -> Result<Self, ModpackError> {
        let game_version = game_version
            .filter(|version| !version.trim().is_empty())
            .ok_or(ModpackError::MissingGameVersion)?
            .to_owned();
        let (loader, loader_version) = match loader {
            PackLoader::Vanilla => (LoaderChoice::Vanilla, None),
            PackLoader::Fabric(version) => (LoaderChoice::Fabric, Some(version)),
            PackLoader::Forge(version) => (LoaderChoice::Forge, Some(version)),
            PackLoader::NeoForge(version) => (LoaderChoice::NeoForge, Some(version)),
            PackLoader::Quilt(_) => return Err(ModpackError::UnsupportedLoader),
        };
        Ok(Self {
            game_version,
            loader,
            loader_version,
        })
    }
}

/// What a pack archive is, told by the file at its root.
enum PackManifest {
    Modrinth(MrpackIndex),
    CurseForge(CurseForgeManifest),
}

impl PackManifest {
    /// Reads the index or manifest straight from the archive.
    fn from_archive(archive: &Path) -> Result<Self, ModpackError> {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?)
            .map_err(|error| ModpackError::Archive(error.to_string()))?;
        let mut read = |name: &str| -> Result<Option<Vec<u8>>, ModpackError> {
            let entry = match zip.by_name(name) {
                Ok(entry) => entry,
                Err(zip::result::ZipError::FileNotFound) => return Ok(None),
                Err(error) => return Err(ModpackError::Archive(error.to_string())),
            };
            let mut bytes = Vec::new();
            entry.take(MAX_MANIFEST_BYTES).read_to_end(&mut bytes)?;
            Ok(Some(bytes))
        };
        let invalid = |error: serde_json::Error| ModpackError::Archive(error.to_string());
        if let Some(bytes) = read(INDEX_FILE)? {
            return MrpackIndex::parse(&bytes)
                .map(Self::Modrinth)
                .map_err(invalid);
        }
        if let Some(bytes) = read(curseforge_pack::MANIFEST_FILE)? {
            return CurseForgeManifest::parse(&bytes)
                .map(Self::CurseForge)
                .map_err(invalid);
        }
        Err(ModpackError::NotAPack)
    }

    fn name(&self) -> &str {
        match self {
            Self::Modrinth(index) => &index.name,
            Self::CurseForge(manifest) => &manifest.name,
        }
    }

    fn target(&self) -> Result<PackTarget, ModpackError> {
        match self {
            Self::Modrinth(index) => PackTarget::new(index.minecraft_version(), index.loader()),
            Self::CurseForge(manifest) => {
                PackTarget::new(Some(&manifest.minecraft.version), manifest.loader())
            }
        }
    }
}

/// Name and target of a pack file, read before its build is created.
#[derive(Clone, Debug)]
pub struct PackHeader {
    pub name: String,
    pub target: PackTarget,
}

/// Reads what a pack file on disk is. Blocking: run it off the UI thread.
pub fn read_pack_header(archive: &Path) -> Result<PackHeader, ModpackError> {
    let manifest = PackManifest::from_archive(archive)?;
    let target = manifest.target()?;
    let name = manifest.name().trim();
    let name = if name.is_empty() {
        archive
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Modpack".to_owned())
    } else {
        name.to_owned()
    };
    Ok(PackHeader { name, target })
}

/// A laid-out pack.
#[derive(Clone, Debug)]
pub struct PackInstall {
    pub target: PackTarget,
    /// Files the pack lists that could not be downloaded (CurseForge files
    /// whose authors forbid other launchers, or content that is not a mod,
    /// resource pack or shader). The player has to add them by hand.
    pub skipped: Vec<String>,
}

/// Maps a Modrinth loader slug to the build loader, before the index is read.
pub fn loader_from_slug(slug: Option<&str>) -> LoaderChoice {
    match slug {
        Some("fabric") => LoaderChoice::Fabric,
        Some("forge") => LoaderChoice::Forge,
        Some("neoforge") => LoaderChoice::NeoForge,
        _ => LoaderChoice::Vanilla,
    }
}

/// Downloads `pack` (the `.mrpack` file) and lays the pack out in
/// `game_directory`. `work_directory` is scratch space, removed afterwards.
pub async fn install_pack(
    client: Arc<dyn HttpClient>,
    pack: FileDownload<'_>,
    game_directory: &Path,
    work_directory: &Path,
    transfer: &Transfer,
) -> Result<PackInstall, ModpackError> {
    let result = async {
        download_file(client.clone(), pack, transfer).await?;
        install_archive(
            client,
            pack.destination,
            game_directory,
            work_directory,
            transfer,
        )
        .await
    }
    .await;
    remove_work_directory(work_directory);
    result
}

/// Lays out a pack file from disk (`.mrpack` or a CurseForge `.zip`) in
/// `game_directory`. `work_directory` is scratch space, removed afterwards.
pub async fn import_pack(
    client: Arc<dyn HttpClient>,
    archive: &Path,
    game_directory: &Path,
    work_directory: &Path,
    transfer: &Transfer,
) -> Result<PackInstall, ModpackError> {
    let result = install_archive(client, archive, game_directory, work_directory, transfer).await;
    remove_work_directory(work_directory);
    result
}

fn remove_work_directory(work_directory: &Path) {
    if let Err(error) = std::fs::remove_dir_all(work_directory)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "failed to remove the modpack work folder");
    }
}

/// One file of a pack to fetch into the game directory.
struct PackDownload {
    url: String,
    destination: PathBuf,
    sha1: Option<String>,
    size: u64,
}

async fn install_archive(
    client: Arc<dyn HttpClient>,
    archive: &Path,
    game_directory: &Path,
    work_directory: &Path,
    transfer: &Transfer,
) -> Result<PackInstall, ModpackError> {
    let manifest = PackManifest::from_archive(archive)?;
    let target = manifest.target()?;
    let unpacked = work_directory.join("unpacked");
    mc_launcher_core::io::archive::extract_zip_safely(archive, &unpacked)
        .map_err(|error| ModpackError::Archive(error.to_string()))?;
    ensure_not_cancelled(transfer)?;

    let (files, skipped, overrides) = match &manifest {
        PackManifest::Modrinth(index) => {
            let files = index
                .client_files()
                .map(|file| {
                    let relative = safe_relative_path(&file.path)
                        .ok_or_else(|| ModpackError::UnsafePath(file.path.clone()))?;
                    let url = file.downloads.first().cloned().ok_or_else(|| {
                        ModpackError::Archive(format!("{} has no URL", file.path))
                    })?;
                    Ok(PackDownload {
                        url,
                        destination: game_directory.join(relative),
                        sha1: Some(file.hashes.sha1.clone()),
                        size: file.file_size,
                    })
                })
                .collect::<Result<Vec<_>, ModpackError>>()?;
            let overrides = OVERRIDE_FOLDERS.map(str::to_owned).to_vec();
            (files, Vec::new(), overrides)
        }
        PackManifest::CurseForge(manifest) => {
            safe_relative_path(&manifest.overrides)
                .ok_or_else(|| ModpackError::UnsafePath(manifest.overrides.clone()))?;
            let api = super::mods::curseforge_api().ok_or(ModpackError::CurseForgeUnavailable)?;
            let resolved = curseforge::resolve_files(&api, manifest, &transfer.handle)?;
            let files = resolved
                .files
                .into_iter()
                .map(|file| PackDownload {
                    url: file.url,
                    destination: game_directory.join(file.folder).join(file.file_name),
                    sha1: file.sha1,
                    size: file.size,
                })
                .collect();
            (files, resolved.skipped, vec![manifest.overrides.clone()])
        }
    };

    download_all(client, files, transfer).await?;
    ensure_not_cancelled(transfer)?;
    for folder in overrides {
        let overrides = unpacked.join(folder);
        if overrides.is_dir() {
            copy_over(&overrides, game_directory)?;
        }
    }
    tracing::info!(
        pack = %manifest.name(),
        skipped = skipped.len(),
        "installed modpack files"
    );
    Ok(PackInstall { target, skipped })
}

async fn download_all(
    client: Arc<dyn HttpClient>,
    files: Vec<PackDownload>,
    transfer: &Transfer,
) -> Result<(), ModpackError> {
    transfer
        .handle
        .begin_batch(files.iter().map(|file| file.size).sum(), files.len() as u64);
    // The first failure (a cancel included) stops the whole batch: files
    // not started yet are never requested.
    futures::stream::iter(files.into_iter().map(Ok))
        .try_for_each_concurrent(PARALLEL_DOWNLOADS, |file| {
            let client = client.clone();
            let transfer = transfer.clone();
            async move {
                transfer.handle.wait_while_paused(&transfer.executor).await;
                if transfer.handle.is_cancelled() {
                    return Err(DownloadError::Cancelled);
                }
                download_file(
                    client,
                    FileDownload {
                        url: &file.url,
                        destination: &file.destination,
                        size: Some(file.size).filter(|size| *size > 0),
                        sha1: file.sha1.as_deref(),
                        max_size: MAX_FILE_BYTES,
                    },
                    &transfer,
                )
                .await?;
                transfer.handle.file_done();
                Ok::<_, DownloadError>(())
            }
        })
        .await?;
    Ok(())
}

/// Stops the install between its steps once the player cancelled it.
fn ensure_not_cancelled(transfer: &Transfer) -> Result<(), ModpackError> {
    if transfer.handle.is_cancelled() {
        Err(ModpackError::Download(DownloadError::Cancelled))
    } else {
        Ok(())
    }
}

/// Copies a folder tree over `destination`, replacing existing files.
fn copy_over(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_over(&entry.path(), &target)?;
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
    fn overrides_replace_and_extend_the_game_directory() {
        let source = tempfile::tempdir().unwrap();
        let game = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(source.path().join("config")).unwrap();
        std::fs::write(source.path().join("config/a.toml"), "new").unwrap();
        std::fs::write(source.path().join("options.txt"), "pack").unwrap();
        std::fs::create_dir_all(game.path().join("config")).unwrap();
        std::fs::write(game.path().join("config/a.toml"), "old").unwrap();
        std::fs::write(game.path().join("config/b.toml"), "kept").unwrap();

        copy_over(source.path(), game.path()).unwrap();

        let read = |path: &str| std::fs::read_to_string(game.path().join(path)).unwrap();
        assert_eq!(read("config/a.toml"), "new");
        assert_eq!(read("config/b.toml"), "kept");
        assert_eq!(read("options.txt"), "pack");
    }

    /// A zip at `path` holding `entries` (name, text).
    fn write_zip(path: &Path, entries: &[(&str, &str)]) {
        use std::io::Write as _;
        let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, text) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn modrinth_pack_files_are_recognised() {
        let folder = tempfile::tempdir().unwrap();
        let pack = folder.path().join("pack.mrpack");
        write_zip(
            &pack,
            &[(
                INDEX_FILE,
                r#"{"versionId":"1","name":"Fabulous","files":[],
                    "dependencies":{"minecraft":"1.21.1","fabric-loader":"0.16.0"}}"#,
            )],
        );
        let header = read_pack_header(&pack).unwrap();
        assert_eq!(header.name, "Fabulous");
        assert_eq!(header.target.game_version, "1.21.1");
        assert_eq!(header.target.loader, LoaderChoice::Fabric);
        assert_eq!(header.target.loader_version.as_deref(), Some("0.16.0"));
    }

    #[test]
    fn curseforge_pack_files_are_recognised() {
        let folder = tempfile::tempdir().unwrap();
        let pack = folder.path().join("All the Mods.zip");
        write_zip(
            &pack,
            &[
                (
                    "manifest.json",
                    r#"{"minecraft":{"version":"1.20.1",
                        "modLoaders":[{"id":"forge-47.2.0","primary":true}]},
                        "name":"","files":[],"overrides":"overrides"}"#,
                ),
                ("overrides/config/a.toml", "a = 1"),
            ],
        );
        let header = read_pack_header(&pack).unwrap();
        // A pack without a name is named after its file.
        assert_eq!(header.name, "All the Mods");
        assert_eq!(header.target.game_version, "1.20.1");
        assert_eq!(header.target.loader, LoaderChoice::Forge);
        assert_eq!(header.target.loader_version.as_deref(), Some("47.2.0"));
    }

    #[test]
    fn other_archives_are_not_packs() {
        let folder = tempfile::tempdir().unwrap();
        let archive = folder.path().join("world.zip");
        write_zip(&archive, &[("level.dat", "world")]);
        assert!(matches!(
            read_pack_header(&archive),
            Err(ModpackError::NotAPack)
        ));
        let text = folder.path().join("notes.zip");
        std::fs::write(&text, "not a zip").unwrap();
        assert!(matches!(
            read_pack_header(&text),
            Err(ModpackError::Archive(_))
        ));
    }

    #[test]
    fn quilt_packs_are_refused_with_a_clear_message() {
        let index = MrpackIndex::parse(
            br#"{"versionId":"1","name":"Q","files":[],
                 "dependencies":{"minecraft":"1.20.1","quilt-loader":"0.26.0"}}"#,
        )
        .unwrap();
        assert!(matches!(
            PackTarget::new(index.minecraft_version(), index.loader()),
            Err(ModpackError::UnsupportedLoader)
        ));
    }
}
