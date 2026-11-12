//! Turning the file list of a CurseForge manifest (project and file ids)
//! into downloads: where each file comes from and which folder it goes to.

use std::collections::HashMap;
use std::time::Duration;

use orbmc_core::minecraft::curseforge_pack::{self, CurseForgeManifest};
use serde::Deserialize;

use super::ModpackError;
use crate::features::downloads::{DownloadError, TransferHandle};
use crate::features::mods::CurseForgeApi;

/// Ids sent in one request (the proxy takes at most 100).
const BATCH: usize = 100;
/// `algo` of a SHA-1 hash in CurseForge answers.
const SHA1: u8 = 1;

/// A manifest file ready to download.
pub(super) struct ResolvedFile {
    pub url: String,
    /// Content folder in the game directory (`mods`, `resourcepacks`, …).
    pub folder: &'static str,
    pub file_name: String,
    pub sha1: Option<String>,
    pub size: u64,
}

pub(super) struct Resolved {
    pub files: Vec<ResolvedFile>,
    /// Names of files that cannot be downloaded here.
    pub skipped: Vec<String>,
}

#[derive(Deserialize)]
struct ListResponse<T> {
    data: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    id: u64,
    file_name: String,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    file_length: u64,
    #[serde(default)]
    hashes: Vec<Hash>,
}

#[derive(Deserialize)]
struct Hash {
    algo: u8,
    value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Project {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    class_id: Option<u64>,
}

#[derive(Deserialize)]
struct DownloadUrl {
    data: Option<String>,
}

/// Looks the manifest files up on CurseForge. Blocking: run it off the UI
/// thread.
pub(super) fn resolve_files(
    api: &CurseForgeApi,
    manifest: &CurseForgeManifest,
    transfer: &TransferHandle,
) -> Result<Resolved, ModpackError> {
    // Lookups of a big pack take a while: a cancel stops them between requests.
    let check = || {
        if transfer.is_cancelled() {
            Err(ModpackError::Download(DownloadError::Cancelled))
        } else {
            Ok(())
        }
    };
    let wanted: Vec<_> = manifest.required_files().copied().collect();
    if wanted.is_empty() {
        return Ok(Resolved {
            files: Vec::new(),
            skipped: Vec::new(),
        });
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("orbmc/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(failed)?;

    let file_ids: Vec<u64> = wanted.iter().map(|file| file.file_id).collect();
    let mut files: HashMap<u64, File> = HashMap::new();
    for batch in file_ids.chunks(BATCH) {
        check()?;
        let response: ListResponse<File> = post(
            &client,
            api,
            "/v1/mods/files",
            serde_json::json!({ "fileIds": batch }),
        )?;
        files.extend(response.data.into_iter().map(|file| (file.id, file)));
    }
    let mut project_ids: Vec<u64> = wanted.iter().map(|file| file.project_id).collect();
    project_ids.sort_unstable();
    project_ids.dedup();
    let mut projects: HashMap<u64, Project> = HashMap::new();
    for batch in project_ids.chunks(BATCH) {
        check()?;
        let response: ListResponse<Project> = post(
            &client,
            api,
            "/v1/mods",
            serde_json::json!({ "modIds": batch }),
        )?;
        projects.extend(
            response
                .data
                .into_iter()
                .map(|project| (project.id, project)),
        );
    }

    let mut resolved = Resolved {
        files: Vec::new(),
        skipped: Vec::new(),
    };
    for wanted in wanted {
        let project = projects.get(&wanted.project_id);
        let Some(file) = files.remove(&wanted.file_id) else {
            resolved.skipped.push(match project {
                Some(project) if !project.name.is_empty() => project.name.clone(),
                _ => format!("CurseForge {}/{}", wanted.project_id, wanted.file_id),
            });
            continue;
        };
        if !curseforge_pack::is_plain_file_name(&file.file_name) {
            return Err(ModpackError::UnsafePath(file.file_name));
        }
        // A project the API does not return is taken for a mod, as nearly
        // every pack file is one.
        let folder = match project.and_then(|project| project.class_id) {
            Some(class_id) => curseforge_pack::class_folder(class_id),
            None => Some("mods"),
        };
        let Some(folder) = folder else {
            resolved.skipped.push(file.file_name);
            continue;
        };
        let url = match file.download_url.filter(|url| !url.trim().is_empty()) {
            Some(url) => Some(url),
            None => {
                check()?;
                download_url(&client, api, wanted.project_id, file.id)?
            }
        };
        // No URL: the author forbids downloads outside the CurseForge app.
        let Some(url) = url else {
            resolved.skipped.push(file.file_name);
            continue;
        };
        resolved.files.push(ResolvedFile {
            url,
            folder,
            sha1: file
                .hashes
                .iter()
                .find(|hash| hash.algo == SHA1)
                .map(|hash| hash.value.clone()),
            size: file.file_length,
            file_name: file.file_name,
        });
    }
    Ok(resolved)
}

fn post<T: serde::de::DeserializeOwned>(
    client: &reqwest::blocking::Client,
    api: &CurseForgeApi,
    path: &str,
    body: serde_json::Value,
) -> Result<T, ModpackError> {
    let text = api
        .authorize(client.post(api.url(path)))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.text())
        .map_err(failed)?;
    serde_json::from_str(&text).map_err(failed)
}

/// The download URL CurseForge gives on request; `None` when there is none.
fn download_url(
    client: &reqwest::blocking::Client,
    api: &CurseForgeApi,
    project_id: u64,
    file_id: u64,
) -> Result<Option<String>, ModpackError> {
    let response = api
        .authorize(client.get(api.url(&format!(
            "/v1/mods/{project_id}/files/{file_id}/download-url"
        ))))
        .send()
        .map_err(failed)?;
    // Files without third-party distribution answer with an error status.
    if !response.status().is_success() {
        return Ok(None);
    }
    let text = response.text().map_err(failed)?;
    let answer: DownloadUrl = serde_json::from_str(&text).map_err(failed)?;
    Ok(answer.data.filter(|url| !url.trim().is_empty()))
}

fn failed(error: impl std::fmt::Display) -> ModpackError {
    ModpackError::CurseForge(error.to_string())
}
