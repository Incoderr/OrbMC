use std::{io::Write, path::Path, path::PathBuf, sync::Arc, time::Duration};

use gpui_kit::http_client::{AsyncBody, HttpClient, HttpRequestExt, Request};
use serde::Deserialize;

use super::{
    CATALOG_PAGE_SIZE, ModCategory, ModCompatibilityFilter, ModSearchFilters, ModSort, ProjectKind,
};
use crate::features::settings::Locale;
use gpui_kit::assets::IconName;

const MODRINTH_SEARCH_URL: &str = "https://api.modrinth.com/v2/search";
const CURSEFORGE_SEARCH_URL: &str = "https://api.curseforge.com/v1/mods/search";
const CURSEFORGE_MOD_DESCRIPTION_URL: &str = "https://api.curseforge.com/v1/mods";
const CURSEFORGE_MINECRAFT_GAME_ID: u32 = 432;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ModSource {
    #[default]
    All,
    Modrinth,
    CurseForge,
}

impl ModSource {
    pub const FILTERS: [Self; 3] = [Self::All, Self::Modrinth, Self::CurseForge];

    pub fn title(self, locale: Locale) -> &'static str {
        match self {
            Self::All => locale.text("Все источники", "All sources"),
            Self::Modrinth => "Modrinth",
            Self::CurseForge => "CurseForge",
        }
    }

    pub fn api_name(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Modrinth => "Modrinth",
            Self::CurseForge => "CurseForge",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ModProject {
    pub id: String,
    pub kind: ProjectKind,
    pub source: ModSource,
    pub name: String,
    pub description: String,
    pub author: String,
    pub game_version: String,
    pub downloads_label: String,
    pub downloads: u64,
    pub likes_label: String,
    pub updated: String,
    pub file_size: String,
    pub license: String,
    pub loaders: String,
    pub files_count: u32,
    pub categories: Vec<ModCategory>,
    pub icon: IconName,
    pub icon_background: u32,
    pub icon_foreground: u32,
    pub image_url: Option<String>,
    pub page_url: String,
}

#[derive(Clone, Debug)]
pub struct CatalogProviderError {
    pub provider: ModSource,
    pub message: String,
    pub missing_api_key: bool,
    /// The provider has no file of this project for the requested game
    /// version / loader. For a dependency, that usually means the dependency
    /// simply does not target this build (e.g. Fabric API listed for a
    /// NeoForge mod) and must be skipped, not reported as a failure.
    pub no_compatible_file: bool,
}

#[derive(Debug, Default)]
pub struct ModCatalogResult {
    pub projects: Vec<ModProject>,
    pub errors: Vec<CatalogProviderError>,
    pub total_results: usize,
    pub page_count: usize,
}

pub async fn fetch_project_description(
    client: Arc<dyn HttpClient>,
    project_id: String,
    source: ModSource,
    curseforge_api_key: Option<String>,
) -> Result<String, CatalogProviderError> {
    let (url, api_key) = match source {
        ModSource::Modrinth => {
            let project_id = project_id.strip_prefix("modrinth:").unwrap_or(&project_id);
            (
                format!("https://api.modrinth.com/v2/project/{project_id}"),
                None,
            )
        }
        ModSource::CurseForge => {
            let Some(api_key) = curseforge_api_key.filter(|key| !key.trim().is_empty()) else {
                return Err(CatalogProviderError {
                    provider: source,
                    message:
                        "Set the CURSEFORGE_API_KEY environment variable to enable CurseForge."
                            .into(),
                    missing_api_key: true,
                    no_compatible_file: false,
                });
            };
            let project_id = project_id
                .strip_prefix("curseforge:")
                .unwrap_or(&project_id)
                .parse::<u64>()
                .map_err(|error| {
                    provider_error(source, format!("Invalid CurseForge project id: {error}"))
                })?;
            (
                format!("{CURSEFORGE_MOD_DESCRIPTION_URL}/{project_id}/description?stripped=true"),
                Some(api_key),
            )
        }
        ModSource::All => {
            return Err(provider_error(
                source,
                "A catalog provider is required to load a project description.".into(),
            ));
        }
    };

    let body = request_body(client, &url, api_key.as_deref(), source).await?;
    match source {
        ModSource::Modrinth => {
            let project: ModrinthProjectDescription =
                serde_json::from_str(&body).map_err(|error| {
                    provider_error(
                        source,
                        format!("Invalid project description response: {error}"),
                    )
                })?;
            Ok(project.body)
        }
        ModSource::CurseForge => {
            let description: CurseForgeDescriptionResponse =
                serde_json::from_str(&body).map_err(|error| {
                    provider_error(
                        source,
                        format!("Invalid project description response: {error}"),
                    )
                })?;
            Ok(description.data)
        }
        ModSource::All => Err(provider_error(
            source,
            "A catalog provider is required to load a project description.".into(),
        )),
    }
}

pub async fn search_catalog(
    client: Arc<dyn HttpClient>,
    query: String,
    sort: ModSort,
    source: ModSource,
    curseforge_api_key: Option<String>,
    compatibility_filter: ModSearchFilters,
    kind: ProjectKind,
    page: usize,
) -> ModCatalogResult {
    let provider_page_size = if source == ModSource::All {
        CATALOG_PAGE_SIZE / 2
    } else {
        CATALOG_PAGE_SIZE
    };
    let offset = page.saturating_sub(1).saturating_mul(provider_page_size);
    let (modrinth, curseforge) = match source {
        ModSource::All => {
            futures::join!(
                search_modrinth(
                    client.clone(),
                    &query,
                    sort,
                    compatibility_filter.clone(),
                    kind,
                    offset,
                    provider_page_size,
                ),
                search_curseforge(
                    client,
                    &query,
                    sort,
                    curseforge_api_key,
                    compatibility_filter,
                    kind,
                    offset,
                    provider_page_size,
                ),
            )
        }
        ModSource::Modrinth => (
            search_modrinth(
                client,
                &query,
                sort,
                compatibility_filter,
                kind,
                offset,
                provider_page_size,
            )
            .await,
            Ok((Vec::new(), 0)),
        ),
        ModSource::CurseForge => (
            Ok((Vec::new(), 0)),
            search_curseforge(
                client,
                &query,
                sort,
                curseforge_api_key,
                compatibility_filter,
                kind,
                offset,
                provider_page_size,
            )
            .await,
        ),
    };

    let mut result = ModCatalogResult::default();
    let (modrinth_projects, modrinth_total) =
        collect_provider_results(&mut result, ModSource::Modrinth, modrinth);
    let curseforge_projects =
        collect_provider_results(&mut result, ModSource::CurseForge, curseforge);
    let curseforge_total = curseforge_projects.1;
    let curseforge_projects = curseforge_projects.0;
    result.total_results = modrinth_total.saturating_add(curseforge_total);
    result.page_count = if source == ModSource::All {
        modrinth_total
            .div_ceil(provider_page_size)
            .max(curseforge_total.div_ceil(provider_page_size))
    } else {
        result.total_results.div_ceil(CATALOG_PAGE_SIZE)
    };

    if source == ModSource::All && sort == ModSort::Popular {
        let count = modrinth_projects.len().max(curseforge_projects.len());
        for index in 0..count {
            if let Some(project) = modrinth_projects.get(index) {
                result.projects.push(project.clone());
            }
            if let Some(project) = curseforge_projects.get(index) {
                result.projects.push(project.clone());
            }
        }
    } else {
        result.projects.extend(modrinth_projects);
        result.projects.extend(curseforge_projects);
    }
    result
}

/// Result of installing one mod: the requested project plus any required
/// dependencies that were resolved and downloaded alongside it.
#[derive(Clone, Debug)]
pub struct ModInstallOutcome {
    /// Project ids (provider-prefixed) that were downloaded, main mod first.
    pub installed: Vec<String>,
    /// File names written to the mods directory, main mod first.
    pub files: Vec<String>,
}

/// Downloads `project_id` and every required dependency that is compatible
/// with the build.
///
/// Dependency traversal is breadth-first and cycle-safe. Projects already
/// recorded in the mods manifest are skipped, so installing an already
/// present mod only refreshes nothing and downloading the same dependency
/// twice never happens.
pub async fn install_compatible_mod(
    client: Arc<dyn HttpClient>,
    project_id: String,
    source: ModSource,
    compatibility: ModCompatibilityFilter,
    curseforge_api_key: Option<String>,
    mods_directory: PathBuf,
) -> Result<ModInstallOutcome, CatalogProviderError> {
    if compatibility.loader == super::ModLoaderFilter::Vanilla {
        return Err(provider_error(
            source,
            "A mod loader is required. Create a build with Fabric, Forge, or NeoForge first."
                .into(),
        ));
    }

    let mut files = Vec::new();
    let mut installed = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    let main_project = project_id.clone();
    queue.push_back(project_id);

    while let Some(project) = queue.pop_front() {
        if !visited.insert(project.clone()) {
            continue;
        }
        let is_main = project == main_project;
        // Skip anything the build already has; dependencies of an installed
        // project were settled when that project was installed.
        if super::installed::recorded_projects(&mods_directory).contains(&project) {
            continue;
        }
        let (file, dependencies) = match resolve_compatible_mod_file(
            client.clone(),
            &project,
            source,
            &compatibility,
            curseforge_api_key.as_deref(),
        )
        .await
        {
            Ok(resolved) => resolved,
            Err(error) => {
                // Providers list dependencies from every variant of a mod:
                // a Fabric-only dep (e.g. Fabric API of a NeoForge mod) has
                // no file for this build, which is not an error.
                if !is_main && error.no_compatible_file {
                    tracing::info!(
                        dependency = %project,
                        "dependency has no file for this build; treating it as not required here"
                    );
                    continue;
                }
                return Err(error);
            }
        };
        let filename = download_mod_file(client.clone(), &project, file, &mods_directory).await?;
        installed.push(project);
        files.push(filename);
        queue.extend(dependencies);
    }

    Ok(ModInstallOutcome { installed, files })
}

/// Validates and stores one resolved mod file, then records the project in
/// the mods manifest.
async fn download_mod_file(
    client: Arc<dyn HttpClient>,
    project_id: &str,
    file: ModDownloadFile,
    mods_directory: &Path,
) -> Result<String, CatalogProviderError> {
    let source = file.source;
    if file.filename.trim().is_empty()
        || file.filename == "."
        || file.filename == ".."
        || file.filename.contains('/')
        || file.filename.contains('\\')
        || !file.filename.to_ascii_lowercase().ends_with(".jar")
    {
        return Err(provider_error(
            source,
            format!(
                "The provider returned an invalid mod filename: {}",
                file.filename
            ),
        ));
    }

    if let Some(size) = file.size.filter(|size| *size > 512 * 1024 * 1024) {
        return Err(provider_error(
            source,
            format!("The mod file is larger than the 512 MiB download limit ({size} bytes)."),
        ));
    }

    let bytes = request_bytes(client, &file.url, source).await?;
    if let Some(expected_size) = file.size.filter(|size| *size > 0)
        && bytes.len() as u64 != expected_size
    {
        return Err(provider_error(
            source,
            format!(
                "Downloaded mod size does not match metadata ({} vs {expected_size} bytes).",
                bytes.len()
            ),
        ));
    }

    std::fs::create_dir_all(mods_directory).map_err(|error| {
        provider_error(
            source,
            format!("Could not create the instance mods directory: {error}"),
        )
    })?;
    let destination = mods_directory.join(&file.filename);
    let mut temporary = tempfile::NamedTempFile::new_in(mods_directory).map_err(|error| {
        provider_error(
            source,
            format!("Could not create a temporary mod file: {error}"),
        )
    })?;
    temporary.write_all(&bytes).map_err(|error| {
        provider_error(
            source,
            format!("Could not save the downloaded mod: {error}"),
        )
    })?;
    temporary.flush().map_err(|error| {
        provider_error(
            source,
            format!("Could not flush the downloaded mod: {error}"),
        )
    })?;
    temporary.as_file().sync_all().map_err(|error| {
        provider_error(
            source,
            format!("Could not sync the downloaded mod: {error}"),
        )
    })?;

    if let Some(expected_sha1) = file.sha1 {
        let actual_sha1 =
            mc_launcher_core::io::hash::sha1_file(temporary.path()).map_err(|error| {
                provider_error(
                    source,
                    format!("Could not verify the downloaded mod: {error}"),
                )
            })?;
        if !actual_sha1.eq_ignore_ascii_case(&expected_sha1) {
            return Err(provider_error(
                source,
                "The downloaded mod failed its SHA-1 checksum.".into(),
            ));
        }
    }

    temporary.persist(&destination).map_err(|error| {
        provider_error(
            source,
            format!("Could not install the downloaded mod: {}", error.error),
        )
    })?;

    super::installed::record_install(mods_directory, project_id.to_owned(), &file.filename)
        .map_err(|error| {
            provider_error(
                source,
                format!("Mod saved, but could not record installation: {error}"),
            )
        })?;

    Ok(file.filename)
}

/// Resolves the best compatible file of `project_id` plus its required
/// dependency projects.
async fn resolve_compatible_mod_file(
    client: Arc<dyn HttpClient>,
    project_id: &str,
    source: ModSource,
    compatibility: &ModCompatibilityFilter,
    curseforge_api_key: Option<&str>,
) -> Result<(ModDownloadFile, Vec<String>), CatalogProviderError> {
    match source {
        ModSource::Modrinth => {
            let id = project_id.strip_prefix("modrinth:").unwrap_or(project_id);
            let game_versions = serde_json::to_string(&[compatibility.game_version.as_str()])
                .map_err(|error| {
                    provider_error(
                        source,
                        format!("Could not prepare the Minecraft version filter: {error}"),
                    )
                })?;
            let loaders = serde_json::to_string(&[compatibility.loader.modrinth_slug()]).map_err(
                |error| {
                    provider_error(
                        source,
                        format!("Could not prepare the loader filter: {error}"),
                    )
                },
            )?;
            let url = format!(
                "https://api.modrinth.com/v2/project/{}/version?game_versions={}&loaders={}&include_changelog=false",
                encode_query_component(id),
                encode_query_component(&game_versions),
                encode_query_component(&loaders),
            );
            let body = request_body(client, &url, None, source).await?;
            let versions: Vec<ModrinthVersion> = serde_json::from_str(&body).map_err(|error| {
                provider_error(source, format!("Invalid mod versions response: {error}"))
            })?;
            let version = versions
                .into_iter()
                .find(|version| {
                    version.game_versions.contains(&compatibility.game_version)
                        && version
                            .loaders
                            .iter()
                            .any(|loader| loader == compatibility.loader.modrinth_slug())
                })
                .ok_or_else(|| {
                    no_compatible_mod_file(
                        source,
                        &compatibility.game_version,
                        compatibility.loader,
                    )
                })?;
            let file = version
                .files
                .iter()
                .find(|file| file.primary)
                .or_else(|| version.files.first())
                .ok_or_else(|| provider_error(source, "The mod version has no files.".into()))?;
            let required_dependencies = modrinth_required_dependencies(&version);
            Ok((
                ModDownloadFile {
                    source,
                    url: file.url.clone(),
                    filename: file.filename.clone(),
                    sha1: file.hashes.get("sha1").cloned(),
                    size: Some(file.size),
                },
                required_dependencies,
            ))
        }
        ModSource::CurseForge => {
            let api_key = curseforge_api_key
                .filter(|key| !key.trim().is_empty())
                .ok_or_else(|| CatalogProviderError {
                    provider: source,
                    message:
                        "Set the CURSEFORGE_API_KEY environment variable to enable CurseForge."
                            .into(),
                    missing_api_key: true,
                    no_compatible_file: false,
                })?;
            let id = project_id
                .strip_prefix("curseforge:")
                .unwrap_or(project_id)
                .parse::<u64>()
                .map_err(|error| {
                    provider_error(source, format!("Invalid CurseForge project id: {error}"))
                })?;
            let url = format!(
                "{CURSEFORGE_MOD_DESCRIPTION_URL}/{id}/files?gameVersion={}&modLoaderType={}&pageSize=50&index=0",
                encode_query_component(&compatibility.game_version),
                compatibility.loader.curseforge_id(),
            );
            let body = request_body(client.clone(), &url, Some(api_key), source).await?;
            let response: CurseForgeFilesResponse =
                serde_json::from_str(&body).map_err(|error| {
                    provider_error(source, format!("Invalid mod files response: {error}"))
                })?;
            let mut compatible_files = response
                .data
                .into_iter()
                .filter(|file| file.game_versions.contains(&compatibility.game_version))
                .collect::<Vec<_>>();
            let file_index = compatible_files
                .iter()
                .position(|file| file.release_type == 1)
                .or_else(|| (!compatible_files.is_empty()).then_some(0))
                .ok_or_else(|| {
                    no_compatible_mod_file(
                        source,
                        &compatibility.game_version,
                        compatibility.loader,
                    )
                })?;
            let file = compatible_files.swap_remove(file_index);
            let required_dependencies = curseforge_required_dependencies(&file);
            let download_url = match file.download_url.filter(|url| !url.trim().is_empty()) {
                Some(url) => url,
                None => {
                    let url = format!(
                        "{CURSEFORGE_MOD_DESCRIPTION_URL}/{id}/files/{}/download-url",
                        file.id
                    );
                    let body = request_body(client, &url, Some(api_key), source).await?;
                    let response: CurseForgeDownloadUrlResponse = serde_json::from_str(&body)
                        .map_err(|error| {
                            provider_error(
                                source,
                                format!("Invalid mod download URL response: {error}"),
                            )
                        })?;
                    response.data
                }
            };
            let sha1 = file
                .hashes
                .into_iter()
                .find(|hash| hash.algo == 1)
                .map(|hash| hash.value);
            Ok((
                ModDownloadFile {
                    source,
                    url: download_url,
                    filename: file.file_name,
                    sha1,
                    size: Some(file.file_length),
                },
                required_dependencies,
            ))
        }
        ModSource::All => Err(provider_error(
            source,
            "A catalog provider is required to download a mod.".into(),
        )),
    }
}

async fn request_bytes(
    client: Arc<dyn HttpClient>,
    url: &str,
    provider: ModSource,
) -> Result<Vec<u8>, CatalogProviderError> {
    if !url.starts_with("https://") {
        return Err(provider_error(
            provider,
            "The mod file URL is invalid.".into(),
        ));
    }
    let request = Request::builder()
        .uri(url)
        .timeout(Duration::from_secs(120))
        .body(AsyncBody::empty())
        .map_err(|error| {
            provider_error(provider, format!("Could not prepare download: {error}"))
        })?;
    let response = client
        .send(request)
        .await
        .map_err(|error| provider_error(provider, format!("Mod download failed: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(provider_error(
            provider,
            format!("The mod download returned HTTP {status}."),
        ));
    }
    let mut body = futures::io::AsyncReadExt::take(response.into_body(), 512 * 1024 * 1024 + 1);
    let mut bytes = Vec::new();
    futures::io::AsyncReadExt::read_to_end(&mut body, &mut bytes)
        .await
        .map_err(|error| {
            provider_error(provider, format!("Could not read mod download: {error}"))
        })?;
    if bytes.len() as u64 > 512 * 1024 * 1024 {
        return Err(provider_error(
            provider,
            "The mod file is larger than the 512 MiB download limit.".into(),
        ));
    }
    Ok(bytes)
}

fn no_compatible_mod_file(
    source: ModSource,
    game_version: &str,
    loader: super::ModLoaderFilter,
) -> CatalogProviderError {
    CatalogProviderError {
        no_compatible_file: true,
        ..provider_error(
            source,
            format!(
                "No compatible file found for Minecraft {game_version} with {}.",
                loader.title(Locale::En)
            ),
        )
    }
}

/// Hard dependencies of a Modrinth version as prefixed project ids.
fn modrinth_required_dependencies(version: &ModrinthVersion) -> Vec<String> {
    version
        .dependencies
        .iter()
        .filter_map(|dependency| {
            if dependency.dependency_type == "required" {
                dependency.project_id.clone()
            } else {
                None
            }
        })
        .map(|project| format!("modrinth:{project}"))
        .collect()
}

/// Hard dependency mods of a CurseForge file as prefixed project ids.
fn curseforge_required_dependencies(file: &CurseForgeFile) -> Vec<String> {
    file.dependencies
        .iter()
        .filter(|dependency| dependency.relation_type == CURSEFORGE_RELATION_REQUIRED)
        .map(|dependency| format!("curseforge:{}", dependency.mod_id))
        .collect()
}

#[derive(Clone, Debug)]
struct ModDownloadFile {
    source: ModSource,
    url: String,
    filename: String,
    sha1: Option<String>,
    size: Option<u64>,
}

fn collect_provider_results(
    result: &mut ModCatalogResult,
    provider: ModSource,
    response: Result<(Vec<ModProject>, usize), CatalogProviderError>,
) -> (Vec<ModProject>, usize) {
    match response {
        Ok(result) => result,
        Err(error) => {
            tracing::warn!(provider = provider.api_name(), message = %error.message, "mod catalog search failed");
            result.errors.push(error);
            (Vec::new(), 0)
        }
    }
}

fn modrinth_facets(kind: ProjectKind, filters: &ModSearchFilters) -> Vec<Vec<String>> {
    let mut facets = vec![vec![format!("project_type:{}", kind.modrinth_type())]];
    if let Some(version) = &filters.game_version {
        facets.push(vec![format!("versions:{version}")]);
    }
    if kind == ProjectKind::Mod {
        if let Some(loader) = filters.loader {
            facets.push(vec![format!("categories:{}", loader.modrinth_slug())]);
        }
        if let Some(category) = filters.category.modrinth_slug() {
            facets.push(vec![format!("categories:{category}")]);
        }
    }
    facets
}

async fn search_modrinth(
    client: Arc<dyn HttpClient>,
    query: &str,
    sort: ModSort,
    compatibility_filter: ModSearchFilters,
    kind: ProjectKind,
    offset: usize,
    limit: usize,
) -> Result<(Vec<ModProject>, usize), CatalogProviderError> {
    let index = match sort {
        ModSort::Popular => "follows",
        ModSort::Downloads => "downloads",
        ModSort::Relevance => "relevance",
    };
    let facets = modrinth_facets(kind, &compatibility_filter);
    let facets = serde_json::to_string(&facets).map_err(|error| {
        provider_error(
            ModSource::Modrinth,
            format!("Could not prepare compatibility filters: {error}"),
        )
    })?;
    let mut url = format!(
        "{MODRINTH_SEARCH_URL}?facets={}&index={index}&limit={limit}&offset={offset}",
        encode_query_component(&facets),
    );
    if !query.trim().is_empty() {
        url.push_str("&query=");
        url.push_str(&encode_query_component(query.trim()));
    }

    let body = request_body(client, &url, None, ModSource::Modrinth).await?;
    let response: ModrinthSearchResponse = serde_json::from_str(&body).map_err(|error| {
        provider_error(
            ModSource::Modrinth,
            format!("Invalid search response: {error}"),
        )
    })?;

    let projects = response
        .hits
        .into_iter()
        .map(|hit| {
            let mut project = ModProject::from_modrinth(hit, kind);
            if let Some(game_version) = &compatibility_filter.game_version {
                project.game_version = game_version.clone();
            }
            project
        })
        .collect();

    Ok((projects, response.total_hits))
}

async fn search_curseforge(
    client: Arc<dyn HttpClient>,
    query: &str,
    sort: ModSort,
    api_key: Option<String>,
    compatibility_filter: ModSearchFilters,
    kind: ProjectKind,
    offset: usize,
    limit: usize,
) -> Result<(Vec<ModProject>, usize), CatalogProviderError> {
    if compatibility_filter.loader == Some(super::ModLoaderFilter::Vanilla) {
        return Ok((Vec::new(), 0));
    }

    let Some(api_key) = api_key.filter(|key| !key.trim().is_empty()) else {
        return Err(CatalogProviderError {
            provider: ModSource::CurseForge,
            message: "Set the CURSEFORGE_API_KEY environment variable to enable CurseForge.".into(),
            missing_api_key: true,
            no_compatible_file: false,
        });
    };

    let sort_field = match sort {
        ModSort::Popular => 2,
        ModSort::Downloads => 6,
        ModSort::Relevance => 1,
    };
    let mut url = format!(
        "{CURSEFORGE_SEARCH_URL}?gameId={CURSEFORGE_MINECRAFT_GAME_ID}&classId={}&pageSize={limit}&index={offset}&sortField={sort_field}&sortOrder=desc",
        kind.curseforge_class_id(),
    );
    if let Some(game_version) = &compatibility_filter.game_version {
        url.push_str("&gameVersion=");
        url.push_str(&encode_query_component(game_version));
    }
    if let Some(loader) = compatibility_filter.loader {
        // The plural filter also works without selecting a game version.
        url.push_str("&modLoaderTypes=");
        url.push_str(&encode_query_component(&format!(
            "[{}]",
            loader.curseforge_id()
        )));
    }
    if kind == ProjectKind::Mod
        && let Some(slug) = compatibility_filter.category.curseforge_slug()
    {
        let body = request_body(
            client.clone(),
            "https://api.curseforge.com/v1/categories?gameId=432&classId=6",
            Some(&api_key),
            ModSource::CurseForge,
        )
        .await?;
        #[derive(Deserialize)]
        struct Category {
            id: u64,
            slug: String,
        }
        #[derive(Deserialize)]
        struct Categories {
            data: Vec<Category>,
        }
        let categories: Categories = serde_json::from_str(&body)
            .map_err(|error| provider_error(ModSource::CurseForge, error.to_string()))?;
        let category = categories
            .data
            .iter()
            .find(|category| category.slug == slug)
            .ok_or_else(|| {
                provider_error(
                    ModSource::CurseForge,
                    "This category is not available from CurseForge.".into(),
                )
            })?;
        url.push_str(&format!("&categoryId={}", category.id));
    }
    if !query.trim().is_empty() {
        url.push_str("&searchFilter=");
        url.push_str(&encode_query_component(query.trim()));
    }

    let body = request_body(client, &url, Some(&api_key), ModSource::CurseForge).await?;
    let response: CurseForgeSearchResponse = serde_json::from_str(&body).map_err(|error| {
        provider_error(
            ModSource::CurseForge,
            format!("Invalid search response: {error}"),
        )
    })?;

    let total_results = response
        .pagination
        .map_or(0, |pagination| pagination.total_count);
    let projects = response
        .data
        .into_iter()
        .map(|hit| {
            let mut project = ModProject::from_curseforge(hit, kind);
            if let Some(game_version) = &compatibility_filter.game_version {
                project.game_version = game_version.clone();
            }
            project
        })
        .collect();

    Ok((projects, total_results))
}

async fn request_body(
    client: Arc<dyn HttpClient>,
    url: &str,
    api_key: Option<&str>,
    provider: ModSource,
) -> Result<String, CatalogProviderError> {
    let mut request = Request::builder().uri(url).timeout(Duration::from_secs(20));
    if let Some(api_key) = api_key {
        request = request.header("x-api-key", api_key);
    }
    let request = request
        .body(AsyncBody::empty())
        .map_err(|error| provider_error(provider, format!("Could not prepare request: {error}")))?;
    let response = client
        .send(request)
        .await
        .map_err(|error| provider_error(provider, format!("Request failed: {error}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(provider_error(
            provider,
            format!("Server returned HTTP {status}"),
        ));
    }

    let mut body = response.into_body();
    let mut bytes = Vec::new();
    futures::io::AsyncReadExt::read_to_end(&mut body, &mut bytes)
        .await
        .map_err(|error| provider_error(provider, format!("Could not read response: {error}")))?;
    String::from_utf8(bytes)
        .map_err(|error| provider_error(provider, format!("Response is not valid UTF-8: {error}")))
}

fn provider_error(provider: ModSource, message: String) -> CatalogProviderError {
    CatalogProviderError {
        provider,
        message,
        missing_api_key: false,
        no_compatible_file: false,
    }
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

#[derive(Deserialize)]
struct ModrinthSearchResponse {
    #[serde(default)]
    hits: Vec<ModrinthHit>,
    #[serde(default)]
    total_hits: usize,
}

#[derive(Deserialize)]
struct ModrinthProjectDescription {
    #[serde(default)]
    body: String,
}

#[derive(Deserialize)]
struct CurseForgeDescriptionResponse {
    #[serde(default)]
    data: String,
}

#[derive(Deserialize)]
struct ModrinthVersion {
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    files: Vec<ModrinthVersionFile>,
    #[serde(default)]
    dependencies: Vec<ModrinthDependency>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct ModrinthDependency {
    /// `project_id` is missing for `version_id`-linked dependencies; those are
    /// skipped during dependency resolution.
    project_id: Option<String>,
    #[serde(default)]
    dependency_type: String,
}

#[derive(Deserialize)]
struct ModrinthVersionFile {
    #[serde(default)]
    hashes: std::collections::HashMap<String, String>,
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    size: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeFilesResponse {
    #[serde(default)]
    data: Vec<CurseForgeFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeFile {
    id: u64,
    file_name: String,
    #[serde(default)]
    release_type: u32,
    #[serde(default)]
    hashes: Vec<CurseForgeFileHash>,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    file_length: u64,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    dependencies: Vec<CurseForgeFileDependency>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeFileDependency {
    mod_id: u64,
    relation_type: u32,
}

/// CurseForge relation types: 3 is "required".
const CURSEFORGE_RELATION_REQUIRED: u32 = 3;

#[derive(Deserialize)]
struct CurseForgeFileHash {
    value: String,
    algo: u32,
}

#[derive(Deserialize)]
struct CurseForgeDownloadUrlResponse {
    data: String,
}

#[derive(Deserialize)]
struct ModrinthHit {
    project_id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    versions: Vec<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    follows: u64,
    date_modified: Option<String>,
    license: Option<String>,
    icon_url: Option<String>,
    slug: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeSearchResponse {
    #[serde(default)]
    data: Vec<CurseForgeMod>,
    #[serde(default)]
    pagination: Option<CurseForgePagination>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgePagination {
    total_count: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeMod {
    id: u64,
    name: String,
    slug: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    download_count: u64,
    thumbs_up_count: u64,
    date_modified: Option<String>,
    logo: Option<CurseForgeAsset>,
    #[serde(default)]
    authors: Vec<CurseForgeAuthor>,
    #[serde(default)]
    categories: Vec<CurseForgeCategory>,
    #[serde(default)]
    latest_files_indexes: Vec<CurseForgeFileIndex>,
}

#[derive(Deserialize)]
struct CurseForgeAsset {
    url: Option<String>,
}

#[derive(Deserialize)]
struct CurseForgeAuthor {
    name: String,
}

#[derive(Deserialize)]
struct CurseForgeCategory {
    name: String,
    slug: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeFileIndex {
    game_version: String,
    mod_loader: Option<u32>,
}

impl ModProject {
    fn from_modrinth(hit: ModrinthHit, kind: ProjectKind) -> Self {
        let categories = map_categories(hit.categories.iter().map(String::as_str));
        let loaders = hit
            .categories
            .iter()
            .filter(|tag| is_loader(tag))
            .map(|tag| display_loader(tag))
            .collect::<Vec<_>>();
        let slug = hit.slug.as_deref().unwrap_or(&hit.project_id);
        let page_url = format!("https://modrinth.com/{}/{slug}", kind.modrinth_path());
        Self {
            id: format!("modrinth:{}", hit.project_id),
            kind,
            source: ModSource::Modrinth,
            name: hit.title,
            description: hit.description,
            author: hit.author,
            game_version: hit
                .versions
                .first()
                .cloned()
                .unwrap_or_else(|| "Any version".into()),
            downloads_label: format_count(hit.downloads),
            downloads: hit.downloads,
            likes_label: format_count(hit.follows),
            updated: date_label(hit.date_modified.as_deref()),
            file_size: "—".into(),
            license: hit.license.unwrap_or_else(|| "—".into()),
            loaders: loader_label(loaders),
            files_count: 0,
            icon: icon_for_categories(&categories),
            icon_background: 0x2f6f4e,
            icon_foreground: 0xffffff,
            image_url: hit.icon_url,
            categories,
            page_url,
        }
    }

    fn from_curseforge(project: CurseForgeMod, kind: ProjectKind) -> Self {
        let raw_categories = project
            .categories
            .iter()
            .map(|category| format!("{} {}", category.slug, category.name))
            .collect::<Vec<_>>();
        let categories = map_categories(raw_categories.iter().map(String::as_str));
        let loaders = project
            .latest_files_indexes
            .iter()
            .filter_map(|index| index.mod_loader.map(mod_loader_name))
            .collect::<Vec<_>>();
        let game_version = project
            .latest_files_indexes
            .first()
            .map(|index| index.game_version.clone())
            .unwrap_or_else(|| "Any version".into());
        let author = project
            .authors
            .first()
            .map(|author| author.name.clone())
            .unwrap_or_else(|| "Unknown".into());
        let path = match kind {
            ProjectKind::Mod => "mc-mods",
            ProjectKind::Modpack => "modpacks",
            ProjectKind::ResourcePack => "texture-packs",
            ProjectKind::Shader => "shaders",
        };
        let page_url = format!(
            "https://www.curseforge.com/minecraft/{path}/{}",
            project.slug
        );
        Self {
            id: format!("curseforge:{}", project.id),
            kind,
            source: ModSource::CurseForge,
            name: project.name,
            description: project.summary,
            author,
            game_version,
            downloads_label: format_count(project.download_count),
            downloads: project.download_count,
            likes_label: format_count(u64::from(project.thumbs_up_count)),
            updated: date_label(project.date_modified.as_deref()),
            file_size: "—".into(),
            license: "—".into(),
            loaders: loader_label(loaders),
            files_count: 0,
            icon: icon_for_categories(&categories),
            icon_background: 0x775d32,
            icon_foreground: 0xffffff,
            image_url: project.logo.and_then(|asset| asset.url),
            categories,
            page_url,
        }
    }
}

fn map_categories<'a>(tags: impl Iterator<Item = &'a str>) -> Vec<ModCategory> {
    let mut categories = Vec::new();
    for tag in tags.map(str::to_ascii_lowercase) {
        let category = if tag.contains("adventure") {
            Some(ModCategory::Adventure)
        } else if tag.contains("technology") || tag.contains("tech") {
            Some(ModCategory::Technology)
        } else if tag.contains("decoration") || tag.contains("decor") {
            Some(ModCategory::Decoration)
        } else if tag.contains("optimization") || tag.contains("performance") {
            Some(ModCategory::Optimization)
        } else if tag.contains("librar") || tag == "library" {
            Some(ModCategory::Libraries)
        } else if tag.contains("magic") {
            Some(ModCategory::Magic)
        } else if tag.contains("rpg") || tag.contains("roleplay") {
            Some(ModCategory::Rpg)
        } else {
            None
        };
        if let Some(category) = category.filter(|category| !categories.contains(category)) {
            categories.push(category);
        }
    }
    categories
}

fn is_loader(tag: &str) -> bool {
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "fabric" | "forge" | "neoforge" | "quilt"
    )
}

fn display_loader(loader: &str) -> String {
    match loader.to_ascii_lowercase().as_str() {
        "fabric" => "Fabric".into(),
        "forge" => "Forge".into(),
        "neoforge" => "NeoForge".into(),
        "quilt" => "Quilt".into(),
        _ => loader.to_owned(),
    }
}

fn mod_loader_name(loader: u32) -> String {
    display_loader(match loader {
        1 => "forge",
        4 => "fabric",
        5 => "quilt",
        6 => "neoforge",
        _ => "unknown",
    })
}

fn loader_label(loaders: Vec<String>) -> String {
    let mut unique = Vec::new();
    for loader in loaders {
        if !loader.eq_ignore_ascii_case("unknown") && !unique.contains(&loader) {
            unique.push(loader);
        }
    }
    if unique.is_empty() {
        "—".into()
    } else {
        unique.join(", ")
    }
}

fn icon_for_categories(categories: &[ModCategory]) -> IconName {
    if categories.contains(&ModCategory::Adventure) || categories.contains(&ModCategory::Rpg) {
        IconName::Map
    } else if categories.contains(&ModCategory::Optimization) {
        IconName::Cpu
    } else if categories.contains(&ModCategory::Technology) {
        IconName::Blocks
    } else if categories.contains(&ModCategory::Magic) {
        IconName::Asterisk
    } else {
        IconName::Puzzle
    }
}

fn format_count(count: u64) -> String {
    if count >= 1_000_000_000 {
        format!("{:.1}B", count as f64 / 1_000_000_000.0)
    } else if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

fn date_label(date: Option<&str>) -> String {
    date.and_then(|date| date.get(..10))
        .unwrap_or("—")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        CurseForgeFile, CurseForgeMod, ModProject, ModSource, ModrinthHit, ModrinthVersion,
        curseforge_required_dependencies, encode_query_component, modrinth_required_dependencies,
        no_compatible_mod_file, provider_error,
    };
    use crate::features::mods::{ModCategory, ProjectKind};

    #[test]
    fn search_filters_are_combined_on_the_server() {
        let filters = super::ModSearchFilters {
            game_version: Some("1.21.1".into()),
            loader: Some(crate::features::mods::ModLoaderFilter::Fabric),
            category: ModCategory::Optimization,
        };
        assert_eq!(
            super::modrinth_facets(ProjectKind::Mod, &filters),
            vec![
                vec!["project_type:mod"],
                vec!["versions:1.21.1"],
                vec!["categories:fabric"],
                vec!["categories:optimization"]
            ]
        );
        assert_eq!(
            super::modrinth_facets(ProjectKind::Shader, &filters),
            vec![vec!["project_type:shader"], vec!["versions:1.21.1"]]
        );
    }

    #[test]
    fn maps_modrinth_search_hit_into_catalog_project() {
        let hit: ModrinthHit = serde_json::from_str(
            r#"{
                "project_id":"abc123",
                "slug":"example-mod",
                "title":"Example Mod",
                "description":"A useful mod",
                "author":"Example Author",
                "categories":["technology","fabric"],
                "versions":["1.21.1"],
                "downloads":1200,
                "follows":25,
                "date_modified":"2026-01-02T03:04:05Z",
                "license":"MIT",
                "icon_url":"https://cdn.example.org/icon.png"
            }"#,
        )
        .expect("valid Modrinth search hit");

        let project = ModProject::from_modrinth(hit, ProjectKind::Mod);

        assert_eq!(project.source, ModSource::Modrinth);
        assert_eq!(project.name, "Example Mod");
        assert_eq!(project.game_version, "1.21.1");
        assert_eq!(project.downloads_label, "1.2K");
        assert_eq!(project.updated, "2026-01-02");
        assert_eq!(project.loaders, "Fabric");
        assert!(project.categories.contains(&ModCategory::Technology));
        assert_eq!(project.page_url, "https://modrinth.com/mod/example-mod");
    }

    #[test]
    fn maps_curseforge_camel_case_search_hit_into_catalog_project() {
        let hit: CurseForgeMod = serde_json::from_str(
            r#"{
                "id":42,
                "name":"Example Mod",
                "slug":"example-mod",
                "summary":"A useful mod",
                "downloadCount":9876,
                "thumbsUpCount":31,
                "dateModified":"2026-02-03T04:05:06Z",
                "logo":{"url":"https://media.example.org/icon.png"},
                "authors":[{"name":"Example Author"}],
                "categories":[{"name":"Technology","slug":"technology"}],
                "latestFilesIndexes":[{"gameVersion":"1.20.1","modLoader":4}]
            }"#,
        )
        .expect("valid CurseForge search hit");

        let project = ModProject::from_curseforge(hit, ProjectKind::Mod);

        assert_eq!(project.source, ModSource::CurseForge);
        assert_eq!(project.author, "Example Author");
        assert_eq!(project.game_version, "1.20.1");
        assert_eq!(project.loaders, "Fabric");
        assert!(project.categories.contains(&ModCategory::Technology));
        assert_eq!(
            project.page_url,
            "https://www.curseforge.com/minecraft/mc-mods/example-mod"
        );
    }

    #[test]
    fn query_encoding_handles_spaces_symbols_and_unicode() {
        assert_eq!(
            encode_query_component("Sodium + 日本語"),
            "Sodium%20%2B%20%E6%97%A5%E6%9C%AC%E8%AA%9E"
        );
    }

    #[test]
    fn modrinth_required_dependencies_keep_only_required_projects() {
        let version: ModrinthVersion = serde_json::from_str(
            r#"{
                "game_versions": ["1.20.1"],
                "loaders": ["fabric"],
                "files": [{"url": "https://cdn.modrinth.com/a.jar", "filename": "a.jar", "primary": true, "size": 10}],
                "dependencies": [
                    {"project_id": "A", "dependency_type": "required"},
                    {"project_id": "B", "dependency_type": "optional"},
                    {"project_id": "C", "dependency_type": "embedded"},
                    {"dependency_type": "required", "version_id": "v1"}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(
            modrinth_required_dependencies(&version),
            vec!["modrinth:A".to_owned()]
        );
    }

    #[test]
    fn curseforge_required_dependencies_keep_only_relation_3() {
        let file: CurseForgeFile = serde_json::from_str(
            r#"{
                "id": 1,
                "fileName": "a.jar",
                "downloadUrl": "https://mediafilez.forgecdn.com/files/a.jar",
                "dependencies": [
                    {"modId": 11, "relationType": 3},
                    {"modId": 12, "relationType": 2},
                    {"modId": 13, "relationType": 1}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(
            curseforge_required_dependencies(&file),
            vec!["curseforge:11".to_owned()]
        );
    }

    #[test]
    fn versions_without_dependencies_resolve_to_nothing() {
        let version: ModrinthVersion = serde_json::from_str(
            r#"{"files": [{"url": "https://cdn.modrinth.com/a.jar", "filename": "a.jar", "primary": true, "size": 10}]}"#,
        )
        .unwrap();
        assert!(modrinth_required_dependencies(&version).is_empty());
    }

    #[test]
    fn only_a_missing_compatible_file_counts_as_skippable_for_dependencies() {
        let skippable = no_compatible_mod_file(
            ModSource::Modrinth,
            "26.3",
            super::super::ModLoaderFilter::NeoForge,
        );
        assert!(skippable.no_compatible_file);
        let hard_error = provider_error(ModSource::Modrinth, "network glitch".into());
        assert!(!hard_error.no_compatible_file);
    }
}
