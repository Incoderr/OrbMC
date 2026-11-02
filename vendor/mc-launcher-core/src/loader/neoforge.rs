//! NeoForge metadata helpers.

use crate::{
    loader::{forge::MavenMetadata, LoaderKind},
    net::http,
    LauncherError, Result,
};

const NEOFORGE_METADATA_URL: &str =
    "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
const LEGACY_METADATA_URL: &str =
    "https://maven.neoforged.net/releases/net/neoforged/forge/maven-metadata.xml";
/// Minecraft version whose NeoForge builds live in the old `forge` artifact.
const LEGACY_MINECRAFT: &str = "1.20.1";

/// Parses NeoForge Maven metadata XML.
///
/// # Errors
///
/// Returns [`crate::LauncherError`] if required metadata fields are missing.
pub fn parse_maven_metadata(xml: &str) -> Result<MavenMetadata> {
    crate::loader::forge::parse_maven_metadata(xml)
}

/// Fetches all NeoForge versions from the NeoForge Maven repository.
///
/// # Errors
///
/// Returns [`crate::LauncherError`] if the metadata request or parsing fails.
pub fn list_neoforge_versions() -> Result<Vec<String>> {
    Ok(parse_maven_metadata(&http::get_text(NEOFORGE_METADATA_URL)?)?.versions)
}

/// Fetches the NeoForge builds for Minecraft 1.20.1 (`1.20.1-47.1.x`) from
/// the legacy `forge` artifact.
///
/// # Errors
///
/// Returns [`crate::LauncherError`] if the metadata request or parsing fails.
pub fn list_legacy_versions() -> Result<Vec<String>> {
    Ok(parse_maven_metadata(&http::get_text(LEGACY_METADATA_URL)?)?.versions)
}

/// Whether NeoForge for this Minecraft version comes from the legacy artifact.
pub fn is_legacy_minecraft(minecraft_version: &str) -> bool {
    minecraft_version == LEGACY_MINECRAFT
}

/// Whether a NeoForge version string belongs to the legacy artifact.
pub fn is_legacy_version(neoforge_version: &str) -> bool {
    neoforge_version.starts_with("1.20.1-") || neoforge_version.starts_with("47.")
}

/// Full legacy version (`1.20.1-47.1.106`) from one that may lack the game
/// version prefix (`47.1.106`, as modpack manifests list it).
pub fn normalize_version(neoforge_version: &str) -> String {
    if neoforge_version.starts_with("47.") {
        format!("{LEGACY_MINECRAFT}-{neoforge_version}")
    } else {
        neoforge_version.to_string()
    }
}

/// Returns the newest legacy NeoForge build. The metadata is not strictly
/// sorted, so builds are compared numerically.
///
/// # Errors
///
/// Returns [`crate::LauncherError::LoaderVersionNotFound`] when no build is listed.
pub fn latest_legacy(versions: &[String]) -> Result<&str> {
    versions
        .iter()
        .filter_map(|version| {
            let build = version.strip_prefix("1.20.1-")?;
            let key: Vec<u32> = build
                .split('.')
                .map(|part| part.parse().ok())
                .collect::<Option<_>>()?;
            Some((key, version.as_str()))
        })
        .max()
        .map(|(_, version)| version)
        .ok_or_else(|| LauncherError::LoaderVersionNotFound {
            loader: LoaderKind::NeoForge,
            version: format!("latest for Minecraft {LEGACY_MINECRAFT}"),
        })
}

/// Returns the newest advertised NeoForge version for a Minecraft version.
///
/// NeoForge maps Minecraft `1.x.y` releases to loader versions beginning with
/// `x.y.`. A release without a patch component, such as `1.21`, maps to
/// `21.0.`. Minecraft's calendar-versioned releases retain their full version,
/// so `26.1.2` maps to `26.1.2.`.
///
/// # Errors
///
/// Returns [`crate::LauncherError::LoaderVersionNotFound`] when the Minecraft
/// version is unsupported or the metadata has no matching NeoForge version.
pub fn latest_for_minecraft<'a>(
    versions: &'a [String],
    minecraft_version: &str,
) -> Result<&'a str> {
    let prefix = neoforge_version_prefix(minecraft_version);
    prefix
        .as_deref()
        .and_then(|prefix| versions.iter().rfind(|version| version.starts_with(prefix)))
        .map(String::as_str)
        .ok_or_else(|| LauncherError::LoaderVersionNotFound {
            loader: LoaderKind::NeoForge,
            version: format!("latest for Minecraft {minecraft_version}"),
        })
}

fn neoforge_version_prefix(minecraft_version: &str) -> Option<String> {
    if let Some(legacy) = minecraft_version.strip_prefix("1.") {
        let mut components = legacy.split('.');
        let release = components.next()?;
        let patch = components.next().unwrap_or("0");

        if components.next().is_some() || !numeric_components(&[release, patch]) {
            return None;
        }

        return Some(format!("{release}.{patch}."));
    }

    let mut components = minecraft_version.split('.');
    let year = components.next()?;
    let release = components.next()?;
    let patch = components.next().unwrap_or("0");

    if components.next().is_some()
        || year.parse::<u32>().ok()? < 26
        || !numeric_components(&[year, release, patch])
    {
        return None;
    }

    Some(format!("{year}.{release}.{patch}."))
}

fn numeric_components(components: &[&str]) -> bool {
    components.iter().all(|component| {
        !component.is_empty()
            && component
                .chars()
                .all(|character| character.is_ascii_digit())
    })
}

/// Returns the installed NeoForge profile id.
pub fn neoforge_installed_version_id(_minecraft_version: &str, neoforge_version: &str) -> String {
    if is_legacy_version(neoforge_version) {
        let version = normalize_version(neoforge_version);
        let build = version.strip_prefix("1.20.1-").unwrap_or(&version);
        return format!("{LEGACY_MINECRAFT}-forge-{build}");
    }
    format!("neoforge-{neoforge_version}")
}

/// Returns the NeoForge installer jar URL for a NeoForge version.
pub fn installer_url(neoforge_version: &str) -> String {
    if is_legacy_version(neoforge_version) {
        return format!(
            "https://maven.neoforged.net/releases/net/neoforged/forge/{0}/forge-{0}-installer.jar",
            normalize_version(neoforge_version)
        );
    }
    format!(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar",
        neoforge_version
    )
}
