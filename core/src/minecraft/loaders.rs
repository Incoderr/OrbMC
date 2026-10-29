use std::collections::BTreeMap;

use serde::Deserialize;

pub const FORGE_PROMOTIONS_URL: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
pub const FABRIC_LOADER_MANIFEST_URL: &str = "https://meta.fabricmc.net/v2/versions/loader";
pub const NEOFORGE_MAVEN_METADATA_URL: &str =
    "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";

/// Which loader build to take when the user does not type one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoaderRelease {
    /// The build the loader authors recommend.
    Stable,
    /// The newest build, betas included.
    Latest,
}

#[derive(Clone, Debug)]
pub struct ForgeManifest {
    pub promotions: Vec<ForgePromotion>,
}

impl ForgeManifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        let raw: ForgeManifestData = serde_json::from_slice(bytes)?;
        let promotions = raw
            .promos
            .into_iter()
            .filter_map(|(key, version)| {
                let (minecraft_version, channel) = key
                    .strip_suffix("-recommended")
                    .map(|version| (version, ForgeChannel::Recommended))
                    .or_else(|| {
                        key.strip_suffix("-latest")
                            .map(|version| (version, ForgeChannel::Latest))
                    })?;

                Some(ForgePromotion {
                    minecraft_version: minecraft_version.to_owned(),
                    channel,
                    version,
                })
            })
            .collect();

        Ok(Self { promotions })
    }

    /// Full version id (`1.20.1-47.2.0`) promoted for a game version.
    pub fn pick(&self, game_version: &str, release: LoaderRelease) -> Option<String> {
        let channel = match release {
            LoaderRelease::Stable => ForgeChannel::Recommended,
            LoaderRelease::Latest => ForgeChannel::Latest,
        };
        self.promotions
            .iter()
            .find(|promotion| {
                promotion.minecraft_version == game_version && promotion.channel == channel
            })
            .map(|promotion| format!("{}-{}", promotion.minecraft_version, promotion.version))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgeChannel {
    Recommended,
    Latest,
}

#[derive(Clone, Debug)]
pub struct ForgePromotion {
    pub minecraft_version: String,
    pub channel: ForgeChannel,
    pub version: String,
}

#[derive(Deserialize)]
struct ForgeManifestData {
    #[serde(default)]
    promos: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct FabricLoaderManifest {
    pub versions: Vec<FabricLoaderVersion>,
}

impl FabricLoaderManifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        let versions = serde_json::from_slice(bytes)?;
        Ok(Self { versions })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct FabricLoaderVersion {
    pub version: String,
    pub stable: bool,
    pub maven: String,
}

#[derive(Clone, Debug)]
pub struct FabricGameLoaderManifest {
    pub versions: Vec<FabricLoaderVersion>,
}

impl FabricGameLoaderManifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        let compatible_loaders: Vec<FabricCompatibleLoader> = serde_json::from_slice(bytes)?;
        let versions = compatible_loaders
            .into_iter()
            .map(|entry| entry.loader)
            .collect();
        Ok(Self { versions })
    }

    /// Loader build for this game version; the list is newest first and a
    /// stable request falls back to the newest build when none is stable.
    pub fn pick(&self, release: LoaderRelease) -> Option<&str> {
        match release {
            LoaderRelease::Stable => self
                .versions
                .iter()
                .find(|version| version.stable)
                .or_else(|| self.versions.first()),
            LoaderRelease::Latest => self.versions.first(),
        }
        .map(|version| version.version.as_str())
    }
}

#[derive(Deserialize)]
struct FabricCompatibleLoader {
    loader: FabricLoaderVersion,
}

#[derive(Clone, Debug)]
pub struct NeoForgeManifest {
    pub versions: Vec<NeoForgeVersion>,
}

impl NeoForgeManifest {
    pub fn parse(xml: &str) -> Result<Self, String> {
        let metadata: MavenMetadata = quick_xml::de::from_str(xml)
            .map_err(|error| format!("Не удалось разобрать Maven metadata NeoForge: {error}"))?;
        let versions = metadata
            .versioning
            .versions
            .version
            .into_iter()
            .filter_map(|version| {
                Some(NeoForgeVersion {
                    minecraft_version: minecraft_version_for_neoforge(&version)?,
                    version,
                })
            })
            .collect();

        Ok(Self { versions })
    }

    /// NeoForge build for a game version. Maven lists builds oldest first;
    /// stable builds have no `-beta` style suffix.
    pub fn pick(&self, game_version: &str, release: LoaderRelease) -> Option<&str> {
        let mut newest_first = self
            .versions
            .iter()
            .rev()
            .filter(|version| version.minecraft_version == game_version);
        match release {
            LoaderRelease::Stable => newest_first
                .clone()
                .find(|version| !version.version.contains('-'))
                .or_else(|| newest_first.next()),
            LoaderRelease::Latest => newest_first.next(),
        }
        .map(|version| version.version.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct NeoForgeVersion {
    pub minecraft_version: String,
    pub version: String,
}

#[derive(Deserialize)]
struct MavenMetadata {
    versioning: MavenVersioning,
}

#[derive(Deserialize)]
struct MavenVersioning {
    versions: MavenVersions,
}

#[derive(Deserialize)]
struct MavenVersions {
    #[serde(rename = "version", default)]
    version: Vec<String>,
}

fn minecraft_version_for_neoforge(version: &str) -> Option<String> {
    let release = version.split('-').next()?;
    let parts: Vec<_> = release.split('.').collect();
    let major: u32 = parts.first()?.parse().ok()?;
    let minor: u32 = parts.get(1)?.parse().ok()?;

    if major == 1 {
        let patch: u32 = parts.get(2)?.parse().ok()?;
        Some(format!("1.{minor}.{patch}"))
    } else if (20..=25).contains(&major) {
        Some(format!("1.{major}.{minor}"))
    } else if major >= 26 {
        Some(format!("{major}.{minor}"))
    } else {
        None
    }
}

/// Full Forge version id (`1.20.1-47.2.0`) from a loader version that may
/// lack the game version prefix (`47.2.0`), as Forge promotions list it.
pub fn forge_version_id(game_version: &str, loader_version: &str) -> String {
    let prefix = format!("{game_version}-");
    if loader_version.starts_with(&prefix) {
        loader_version.to_owned()
    } else {
        format!("{prefix}{loader_version}")
    }
}

#[cfg(test)]
mod forge_version_tests {
    use super::*;

    #[test]
    fn forge_picks_the_promotion_of_the_channel() {
        let manifest = ForgeManifest::parse(
            br#"{"promos":{"1.20.1-recommended":"47.2.0","1.20.1-latest":"47.3.1"}}"#,
        )
        .unwrap();
        assert_eq!(
            manifest.pick("1.20.1", LoaderRelease::Stable).as_deref(),
            Some("1.20.1-47.2.0")
        );
        assert_eq!(
            manifest.pick("1.20.1", LoaderRelease::Latest).as_deref(),
            Some("1.20.1-47.3.1")
        );
        assert_eq!(manifest.pick("1.21", LoaderRelease::Stable), None);
    }

    #[test]
    fn fabric_stable_falls_back_to_the_newest_build() {
        let manifest = FabricGameLoaderManifest::parse(
            br#"[{"loader":{"version":"0.16.1","stable":false,"maven":"m"}},
                 {"loader":{"version":"0.16.0","stable":true,"maven":"m"}}]"#,
        )
        .unwrap();
        assert_eq!(manifest.pick(LoaderRelease::Stable), Some("0.16.0"));
        assert_eq!(manifest.pick(LoaderRelease::Latest), Some("0.16.1"));
        let unstable = FabricGameLoaderManifest::parse(
            br#"[{"loader":{"version":"0.1.0","stable":false,"maven":"m"}}]"#,
        )
        .unwrap();
        assert_eq!(unstable.pick(LoaderRelease::Stable), Some("0.1.0"));
    }

    #[test]
    fn neoforge_stable_skips_beta_builds() {
        let manifest = NeoForgeManifest::parse(
            "<metadata><versioning><versions>             <version>21.1.1</version><version>21.1.2</version>             <version>21.1.3-beta</version><version>21.4.1-beta</version>             </versions></versioning></metadata>",
        )
        .unwrap();
        assert_eq!(
            manifest.pick("1.21.1", LoaderRelease::Stable),
            Some("21.1.2")
        );
        assert_eq!(
            manifest.pick("1.21.1", LoaderRelease::Latest),
            Some("21.1.3-beta")
        );
        assert_eq!(
            manifest.pick("1.21.4", LoaderRelease::Stable),
            Some("21.4.1-beta")
        );
    }

    #[test]
    fn adds_the_game_version_prefix_once() {
        assert_eq!(forge_version_id("1.20.1", "47.2.0"), "1.20.1-47.2.0");
        assert_eq!(forge_version_id("1.20.1", "1.20.1-47.2.0"), "1.20.1-47.2.0");
    }
}
