//! Modrinth modpacks (`.mrpack`) installed as new builds.
//!
//! The pack archive is unpacked into a work folder, every client file of its
//! index is downloaded into the game directory (verified, several at a time)
//! and the `overrides` folders are copied over it. The game and loader are
//! then installed like for any other build.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt as _;
use gpui_kit::http_client::HttpClient;
use orbmc_core::minecraft::mrpack::{
    INDEX_FILE, MrpackIndex, OVERRIDE_FOLDERS, PackLoader, safe_relative_path,
};

use super::downloads::{DownloadError, FileDownload, Transfer, download_file};
use super::versions::LoaderChoice;

/// Files of a pack downloaded at the same time.
const PARALLEL_DOWNLOADS: usize = 4;
/// Largest single file a pack may reference.
const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;

/// Why a modpack could not be installed. The messages are shown to the user.
#[derive(Debug, thiserror::Error)]
pub enum ModpackError {
    #[error("Не удалось скачать сборку: {0}")]
    Download(#[from] DownloadError),
    #[error("Архив сборки повреждён: {0}")]
    Archive(String),
    #[error("В сборке есть файл с недопустимым путём: {0}")]
    UnsafePath(String),
    #[error("Сборка не указывает версию Minecraft")]
    MissingGameVersion,
    #[error("Загрузчик Quilt пока не поддерживается")]
    UnsupportedLoader,
    #[error("Не удалось разложить файлы сборки: {0}")]
    Io(#[from] std::io::Error),
}

/// Game version and loader a pack asks for.
#[derive(Clone, Debug)]
pub struct PackTarget {
    pub game_version: String,
    pub loader: LoaderChoice,
    pub loader_version: Option<String>,
}

impl PackTarget {
    fn from_index(index: &MrpackIndex) -> Result<Self, ModpackError> {
        let game_version = index
            .minecraft_version()
            .ok_or(ModpackError::MissingGameVersion)?
            .to_owned();
        let (loader, loader_version) = match index.loader() {
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
) -> Result<PackTarget, ModpackError> {
    let result = install_pack_inner(client, pack, game_directory, work_directory, transfer).await;
    if let Err(error) = std::fs::remove_dir_all(work_directory)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "failed to remove the modpack work folder");
    }
    result
}

async fn install_pack_inner(
    client: Arc<dyn HttpClient>,
    pack: FileDownload<'_>,
    game_directory: &Path,
    work_directory: &Path,
    transfer: &Transfer,
) -> Result<PackTarget, ModpackError> {
    download_file(client.clone(), pack, transfer).await?;
    let unpacked = work_directory.join("unpacked");
    mc_launcher_core::io::archive::extract_zip_safely(pack.destination, &unpacked)
        .map_err(|error| ModpackError::Archive(error.to_string()))?;
    let index = MrpackIndex::parse(&std::fs::read(unpacked.join(INDEX_FILE))?)
        .map_err(|error| ModpackError::Archive(error.to_string()))?;
    let target = PackTarget::from_index(&index)?;

    let files = index
        .client_files()
        .map(|file| {
            let relative = safe_relative_path(&file.path)
                .ok_or_else(|| ModpackError::UnsafePath(file.path.clone()))?;
            let url = file
                .downloads
                .first()
                .cloned()
                .ok_or_else(|| ModpackError::Archive(format!("{} has no URL", file.path)))?;
            Ok((
                url,
                game_directory.join(relative),
                file.hashes.sha1.clone(),
                file.file_size,
            ))
        })
        .collect::<Result<Vec<(String, PathBuf, String, u64)>, ModpackError>>()?;
    transfer
        .handle
        .begin_batch(files.iter().map(|file| file.3).sum(), files.len() as u64);
    let file_count = files.len();
    let downloads =
        futures::stream::iter(files.into_iter().map(|(url, destination, sha1, size)| {
            let client = client.clone();
            let transfer = transfer.clone();
            async move {
                download_file(
                    client,
                    FileDownload {
                        url: &url,
                        destination: &destination,
                        size: Some(size).filter(|size| *size > 0),
                        sha1: Some(sha1.as_str()),
                        max_size: MAX_FILE_BYTES,
                    },
                    &transfer,
                )
                .await?;
                transfer.handle.file_done();
                Ok::<_, DownloadError>(())
            }
        }))
        .buffer_unordered(PARALLEL_DOWNLOADS)
        .collect::<Vec<_>>()
        .await;
    downloads.into_iter().collect::<Result<(), _>>()?;

    for folder in OVERRIDE_FOLDERS {
        let overrides = unpacked.join(folder);
        if overrides.is_dir() {
            copy_over(&overrides, game_directory)?;
        }
    }
    tracing::info!(
        pack = %index.name,
        version = %index.version_id,
        files = file_count,
        "installed modpack files"
    );
    Ok(target)
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

    #[test]
    fn quilt_packs_are_refused_with_a_clear_message() {
        let index = MrpackIndex::parse(
            br#"{"versionId":"1","name":"Q","files":[],
                 "dependencies":{"minecraft":"1.20.1","quilt-loader":"0.26.0"}}"#,
        )
        .unwrap();
        assert!(matches!(
            PackTarget::from_index(&index),
            Err(ModpackError::UnsupportedLoader)
        ));
    }
}
