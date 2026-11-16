//! Mods published as jars in GitHub releases. A build records them as the
//! project `github:<owner>/<repo>`; installing and updating take the newest
//! release's jar that fits the build.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::http_client::{AsyncBody, HttpClient, HttpRequestExt, Request};
use serde::Deserialize;

use super::ModLoaderFilter;

pub const PROJECT_PREFIX: &str = "github:";

/// Releases looked through for a fitting jar, newest first.
const RELEASES_PER_PAGE: u32 = 10;

/// `owner/repo` of a GitHub link (`https://github.com/o/r/releases/…`,
/// `github.com/o/r`, `o/r`).
pub fn parse_repository(link: &str) -> Option<String> {
    let link = link.trim().trim_end_matches('/');
    let path = ["https://", "http://"]
        .iter()
        .find_map(|scheme| link.strip_prefix(scheme))
        .unwrap_or(link);
    let path = path
        .strip_prefix("www.")
        .unwrap_or(path)
        .strip_prefix("github.com/")
        .unwrap_or(path);
    let mut parts = path.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?.trim_end_matches(".git");
    let valid = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (valid(owner) && valid(repo)).then(|| format!("{owner}/{repo}"))
}

/// The `owner/repo` of a recorded `github:` project id.
pub fn repository_of(project_id: &str) -> Option<&str> {
    project_id.strip_prefix(PROJECT_PREFIX)
}

#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub id: u64,
    /// Changes when the asset is re-uploaded under the same name.
    #[serde(default)]
    pub updated_at: String,
    /// `sha256:<hex>`; GitHub fills it for assets uploaded since mid-2025.
    #[serde(default)]
    pub digest: Option<String>,
}

impl Asset {
    fn sha256(&self) -> Option<String> {
        self.digest
            .as_deref()?
            .strip_prefix("sha256:")
            .map(str::to_ascii_lowercase)
    }

    /// Identity of this exact upload: its content hash when GitHub has one,
    /// otherwise asset id plus upload time.
    fn revision(&self) -> String {
        match self.sha256() {
            Some(hash) => format!("sha256:{hash}"),
            None => format!("asset:{}:{}", self.id, self.updated_at),
        }
    }
}

/// A release jar chosen for a build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseJar {
    pub url: String,
    pub filename: String,
    pub size: Option<u64>,
    /// Release tag, shown as the mod version.
    pub version: String,
    /// See [`Asset::revision`].
    pub revision: String,
    pub sha256: Option<String>,
}

/// Picks the jar for a build from releases listed newest first: the first
/// stable release that has a fitting jar wins. A jar fits when it is not a
/// sources/dev/javadoc jar and does not name another loader or game version
/// than the build's. A release without such hints in its names still fits.
pub fn pick_jar(
    releases: &[Release],
    loader: ModLoaderFilter,
    game_version: &str,
) -> Option<ReleaseJar> {
    releases
        .iter()
        .filter(|release| !release.draft && !release.prerelease)
        .find_map(|release| {
            let jar = best_jar(release, loader, game_version)?;
            Some(ReleaseJar {
                url: jar.browser_download_url.clone(),
                filename: jar.name.clone(),
                size: jar.size,
                version: release.tag_name.clone(),
                revision: jar.revision(),
                sha256: jar.sha256(),
            })
        })
}

fn best_jar<'a>(
    release: &'a Release,
    loader: ModLoaderFilter,
    game_version: &str,
) -> Option<&'a Asset> {
    let loader_name = loader.modrinth_slug();
    let jars: Vec<&Asset> = release
        .assets
        .iter()
        .filter(|asset| {
            let name = asset.name.to_ascii_lowercase();
            name.ends_with(".jar")
                && !["-sources", "-dev", "-javadoc", "-api"]
                    .iter()
                    .any(|suffix| name.ends_with(&format!("{suffix}.jar")))
        })
        .filter(|asset| {
            let words = words(&asset.name);
            let named: Vec<&str> = LOADERS
                .into_iter()
                .filter(|candidate| words.iter().any(|word| word == candidate))
                .collect();
            named.is_empty() || named.contains(&loader_name)
        })
        .collect();
    // Mod versions look like game versions (`1.5.2`), so the game version
    // only narrows the choice when some jar names it.
    let for_game: Vec<&Asset> = jars
        .iter()
        .copied()
        .filter(|asset| names_version(&asset.name, game_version))
        .collect();
    let candidates = if for_game.is_empty() { jars } else { for_game };
    // Prefer a jar that names the build's loader.
    candidates
        .iter()
        .copied()
        .find(|asset| words(&asset.name).iter().any(|word| word == loader_name))
        .or_else(|| candidates.first().copied())
}

const LOADERS: [&str; 4] = ["fabric", "forge", "neoforge", "quilt"];

/// Lower-case words of a file name; "neoforge" stays one word, so it is
/// never taken for "forge".
fn words(name: &str) -> Vec<String> {
    name.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether `name` contains `version` as a whole version token (`1.20.1`
/// matches `mod-1.20.1.jar` and `mc1.20.1`, not `1.20.10`).
fn names_version(name: &str, version: &str) -> bool {
    name.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map(|part| part.trim_matches('.'))
        .any(|part| part == version)
}

/// Fetches the newest releases of `owner/repo` and picks the jar.
pub async fn resolve_jar(
    client: Arc<dyn HttpClient>,
    repository: &str,
    loader: ModLoaderFilter,
    game_version: &str,
) -> Result<Option<ReleaseJar>, String> {
    let url =
        format!("https://api.github.com/repos/{repository}/releases?per_page={RELEASES_PER_PAGE}");
    let request = Request::builder()
        .uri(&url)
        .timeout(Duration::from_secs(20))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "OrbMC")
        .body(AsyncBody::empty())
        .map_err(|error| format!("Could not prepare the GitHub request: {error}"))?;
    let response = client
        .send(request)
        .await
        .map_err(|error| format!("GitHub request failed: {error}"))?;
    let status = response.status();
    if status.as_u16() == 404 {
        return Err(format!("GitHub repository {repository} was not found"));
    }
    if status.as_u16() == 403 || status.as_u16() == 429 {
        return Err("GitHub request limit reached, try again later".into());
    }
    if !status.is_success() {
        return Err(format!("GitHub returned HTTP {status}"));
    }
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    futures::io::AsyncReadExt::read_to_end(&mut body, &mut bytes)
        .await
        .map_err(|error| format!("Could not read the GitHub response: {error}"))?;
    let releases: Vec<Release> = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid GitHub releases response: {error}"))?;
    Ok(pick_jar(&releases, loader, game_version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, assets: &[&str]) -> Release {
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease,
            assets: assets
                .iter()
                .map(|name| Asset {
                    name: (*name).into(),
                    browser_download_url: format!("https://github.com/o/r/{name}"),
                    size: Some(1),
                    id: 1,
                    updated_at: "2026-01-01T00:00:00Z".into(),
                    digest: None,
                })
                .collect(),
        }
    }

    #[test]
    fn links_are_reduced_to_the_repository() {
        for link in [
            "https://github.com/owner/repo",
            "https://github.com/owner/repo/releases/tag/v1",
            "github.com/owner/repo.git",
            "owner/repo/",
        ] {
            assert_eq!(
                parse_repository(link).as_deref(),
                Some("owner/repo"),
                "{link}"
            );
        }
        assert_eq!(parse_repository("https://github.com/owner"), None);
        assert_eq!(parse_repository("owner/../x"), None);
    }

    #[test]
    fn picks_the_jar_for_the_loader_and_game_version() {
        let releases = [
            release("v2-beta", true, &["mod-2-fabric-1.20.1.jar"]),
            release(
                "v1",
                false,
                &[
                    "mod-1-forge-1.20.1.jar",
                    "mod-1-fabric-1.20.1.jar",
                    "mod-1-fabric-1.20.1-sources.jar",
                    "mod-1-fabric-1.21.jar",
                ],
            ),
        ];
        let jar = pick_jar(&releases, ModLoaderFilter::Fabric, "1.20.1").unwrap();
        assert_eq!(jar.filename, "mod-1-fabric-1.20.1.jar");
        assert_eq!(jar.version, "v1");
    }

    #[test]
    fn neoforge_is_not_mistaken_for_forge_and_plain_jars_fit() {
        let releases = [release("v1", false, &["mod-neoforge.jar"])];
        assert!(pick_jar(&releases, ModLoaderFilter::Forge, "1.20.1").is_none());
        let releases = [release("v1", false, &["mod.jar"])];
        assert!(pick_jar(&releases, ModLoaderFilter::Forge, "1.20.1").is_some());
    }
}
