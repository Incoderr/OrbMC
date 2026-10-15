//! Forge installer compatibility for legacy installer profiles.

use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};

use serde::Deserialize;
use zip::ZipArchive;

use crate::{
    core::{maven::MavenCoordinate, version::LibraryArtifact},
    install::client::{install_version_files_controlled, load_version_json, write_version_json},
    io::{hash::sha1_file, paths::safe_join},
    net::{
        control::DownloadControl,
        download::{execute_plan_controlled, DownloadPlan, DownloadTask},
    },
    progress::ProgressReporter,
    LauncherError, Result,
};

const DEFAULT_FORGE_MAVEN_URL: &str = "https://maven.minecraftforge.net/";

#[derive(Debug, Deserialize)]
struct LegacyForgeInstallerProfile {
    install: LegacyForgeInstallInfo,
    #[serde(rename = "versionInfo")]
    version_info: crate::core::version::VersionJson,
}

#[derive(Debug, Deserialize)]
struct LegacyForgeInstallInfo {
    path: String,
    #[serde(rename = "filePath")]
    file_path: String,
    minecraft: String,
}

/// Installs a legacy Forge profile from the installer JAR's `versionInfo`.
/// Returns `None` when the installer uses the newer processor-based format.
pub(crate) fn install_legacy_forge_client(
    minecraft_dir: &Path,
    installer_path: &Path,
    minecraft_version: &str,
    reporter: &mut dyn ProgressReporter,
    control: &DownloadControl,
) -> Result<Option<String>> {
    let Some(mut profile) = read_legacy_profile(installer_path)? else {
        return Ok(None);
    };

    if profile.install.minecraft != minecraft_version {
        return Err(LauncherError::Other {
            message: format!(
                "Forge installer targets Minecraft {}, not {minecraft_version}",
                profile.install.minecraft
            ),
        });
    }

    let version_id =
        profile
            .version_info
            .id
            .clone()
            .ok_or_else(|| LauncherError::MissingField {
                context: "legacy Forge versionInfo".to_string(),
                field: "id".to_string(),
            })?;

    install_universal_library(minecraft_dir, &mut profile, reporter, control)?;
    write_version_json(minecraft_dir, &profile.version_info)?;

    let merged = load_version_json(minecraft_dir, &version_id)?;
    install_version_files_controlled(&merged, minecraft_dir, reporter, control)?;
    Ok(Some(version_id))
}

fn read_legacy_profile(installer_path: &Path) -> Result<Option<LegacyForgeInstallerProfile>> {
    let file = File::open(installer_path)?;
    let mut archive = ZipArchive::new(file)?;
    let profile_bytes = {
        let mut entry = match archive.by_name("install_profile.json") {
            Ok(entry) => entry,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        bytes
    };
    let value: serde_json::Value = serde_json::from_slice(&profile_bytes)?;
    if value.get("versionInfo").is_none() {
        return Ok(None);
    }

    serde_json::from_value(value)
        .map(Some)
        .map_err(LauncherError::from)
}

fn install_universal_library(
    minecraft_dir: &Path,
    profile: &mut LegacyForgeInstallerProfile,
    reporter: &mut dyn ProgressReporter,
    control: &DownloadControl,
) -> Result<()> {
    let coordinate = MavenCoordinate::parse(&profile.install.path)?;
    let default_artifact_path = coordinate.artifact_path();
    let parent = default_artifact_path
        .parent()
        .ok_or_else(|| LauncherError::Other {
            message: "Forge universal artifact has no Maven directory".to_string(),
        })?;
    let relative_path = safe_join(parent, &profile.install.file_path)?;
    let destination = safe_join(minecraft_dir.join("libraries"), &relative_path)?;

    let repository = profile
        .version_info
        .libraries
        .iter()
        .find(|library| library.name == profile.install.path)
        .and_then(|library| library.url.as_deref())
        .unwrap_or(DEFAULT_FORGE_MAVEN_URL);
    let url = format!(
        "{}/{}",
        repository.trim_end_matches('/'),
        relative_path.to_string_lossy().replace('\\', "/")
    );

    let plan = DownloadPlan {
        tasks: vec![DownloadTask {
            url: url.clone(),
            destination: destination.clone(),
            checksum: None,
            size: None,
            label: format!("Forge universal {}", profile.install.minecraft),
        }],
    };
    execute_plan_controlled(&plan, reporter, control)?;

    let relative_path = relative_path.to_string_lossy().replace('\\', "/");
    let sha1 = sha1_file(&destination)?;
    let size =
        i64::try_from(fs::metadata(&destination)?.len()).map_err(|_| LauncherError::Other {
            message: "Forge universal artifact is too large".to_string(),
        })?;

    let forge_library = profile
        .version_info
        .libraries
        .iter_mut()
        .find(|library| library.name == profile.install.path)
        .ok_or_else(|| LauncherError::MissingField {
            context: "legacy Forge versionInfo".to_string(),
            field: "Forge universal library".to_string(),
        })?;
    forge_library.downloads = Some(crate::core::version::LibraryDownloads {
        artifact: Some(LibraryArtifact {
            path: relative_path,
            url,
            sha1,
            size,
        }),
        classifiers: Default::default(),
    });

    Ok(())
}
