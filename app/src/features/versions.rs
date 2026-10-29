use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::http_client::{AsyncBody, HttpClient, HttpRequestExt, Request};
use orbmc_core::minecraft::loaders::{
    FABRIC_LOADER_MANIFEST_URL, FORGE_PROMOTIONS_URL, FabricGameLoaderManifest,
    FabricLoaderManifest, ForgeManifest, LoaderRelease, NEOFORGE_MAVEN_METADATA_URL,
    NeoForgeManifest,
};
use orbmc_core::minecraft::manifest::{VERSION_MANIFEST_URL, VersionManifest};
use serde::{Deserialize, Serialize};

use crate::platform::manifest_cache;

mod picker;

pub use picker::{CreateProfile, VersionPicker};

pub fn loader_title(loader: LoaderChoice) -> &'static str {
    match loader {
        LoaderChoice::Vanilla => "Vanilla",
        LoaderChoice::Fabric => "Fabric",
        LoaderChoice::NeoForge => "NeoForge",
        LoaderChoice::Forge => "Forge",
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoaderChoice {
    #[default]
    Vanilla,
    Fabric,
    NeoForge,
    Forge,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoaderVersionMode {
    #[default]
    Stable,
    Latest,
    Other,
}

#[derive(Debug, Default)]
pub struct VersionCatalogState {
    loading: bool,
    manifest: Option<VersionManifest>,
    forge: Option<ForgeManifest>,
    fabric: Option<FabricLoaderManifest>,
    fabric_by_game: HashMap<String, FabricGameLoaderManifest>,
    fabric_loading_games: HashSet<String>,
    fabric_errors: HashMap<String, String>,
    neoforge: Option<NeoForgeManifest>,
    error: Option<String>,
    selected_version: Option<String>,
    loader_choice: LoaderChoice,
    loader_version_mode: LoaderVersionMode,
    selected_game_version: Option<String>,
    game_version_menu_open: bool,
    show_all_game_versions: bool,
}

impl VersionCatalogState {
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn manifest(&self) -> Option<&VersionManifest> {
        self.manifest.as_ref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn loader_choice(&self) -> LoaderChoice {
        self.loader_choice
    }

    pub fn set_loader_choice(&mut self, loader: LoaderChoice) {
        self.loader_choice = loader;
    }

    pub fn loader_version_mode(&self) -> LoaderVersionMode {
        self.loader_version_mode
    }

    pub fn set_loader_version_mode(&mut self, mode: LoaderVersionMode) {
        self.loader_version_mode = mode;
    }

    pub fn loader_version(&self, custom_version: Option<&str>) -> Option<String> {
        if self.loader_choice == LoaderChoice::Vanilla {
            return None;
        }
        if self.loader_version_mode == LoaderVersionMode::Other {
            return custom_version
                .map(str::trim)
                .filter(|version| !version.is_empty())
                .map(str::to_owned);
        }

        let release = match self.loader_version_mode {
            LoaderVersionMode::Stable => LoaderRelease::Stable,
            LoaderVersionMode::Latest => LoaderRelease::Latest,
            LoaderVersionMode::Other => return None,
        };
        let game_version = self.game_version()?;
        match self.loader_choice {
            LoaderChoice::Vanilla => None,
            LoaderChoice::Forge => self.forge.as_ref()?.pick(game_version, release),
            LoaderChoice::Fabric => self
                .fabric_by_game
                .get(game_version)?
                .pick(release)
                .map(str::to_owned),
            LoaderChoice::NeoForge => self
                .neoforge
                .as_ref()?
                .pick(game_version, release)
                .map(str::to_owned),
        }
    }

    pub fn begin_fabric_game_load(&mut self, game_version: &str) -> bool {
        if self.fabric_by_game.contains_key(game_version)
            || !self.fabric_loading_games.insert(game_version.to_owned())
        {
            return false;
        }
        self.fabric_errors.remove(game_version);
        true
    }

    pub fn finish_fabric_game_load(
        &mut self,
        game_version: String,
        result: Result<FabricGameLoaderManifest, String>,
    ) {
        self.fabric_loading_games.remove(&game_version);
        match result {
            Ok(manifest) => {
                self.fabric_by_game.insert(game_version.clone(), manifest);
                self.fabric_errors.remove(&game_version);
            }
            Err(error) => {
                self.fabric_errors.insert(game_version, error);
            }
        }
    }

    pub fn fabric_game_loading(&self, game_version: &str) -> bool {
        self.fabric_loading_games.contains(game_version)
    }

    pub fn fabric_game_error(&self, game_version: &str) -> Option<&str> {
        self.fabric_errors.get(game_version).map(String::as_str)
    }

    pub fn fabric_game_loader_count(&self, game_version: &str) -> Option<usize> {
        self.fabric_by_game
            .get(game_version)
            .map(|manifest| manifest.versions.len())
    }

    pub fn game_version(&self) -> Option<&str> {
        self.selected_game_version.as_deref().or_else(|| {
            self.manifest
                .as_ref()
                .map(|manifest| manifest.latest.release.as_str())
        })
    }

    pub fn select_game_version(&mut self, version: String) {
        self.selected_game_version = Some(version);
        self.game_version_menu_open = false;
    }

    pub fn game_version_menu_open(&self) -> bool {
        self.game_version_menu_open
    }

    pub fn toggle_game_version_menu(&mut self) {
        self.game_version_menu_open = !self.game_version_menu_open;
    }

    pub fn close_game_version_menu(&mut self) {
        self.game_version_menu_open = false;
    }

    pub fn shows_all_game_versions(&self) -> bool {
        self.show_all_game_versions
    }

    pub fn toggle_all_game_versions(&mut self) {
        self.show_all_game_versions = !self.show_all_game_versions;
    }

    pub fn begin_loading(&mut self) -> bool {
        let is_complete = self.manifest.is_some()
            && self.forge.is_some()
            && self.fabric.is_some()
            && self.neoforge.is_some();
        if self.loading || is_complete {
            return false;
        }

        self.loading = true;
        self.error = None;
        true
    }

    pub fn finish_loading(&mut self, results: VersionManifestResults) {
        self.loading = false;
        let mut errors = Vec::new();
        save_result(
            &mut self.manifest,
            &mut errors,
            results.minecraft,
            "Minecraft",
        );
        save_result(&mut self.forge, &mut errors, results.forge, "Forge");
        save_result(&mut self.fabric, &mut errors, results.fabric, "Fabric");
        save_result(
            &mut self.neoforge,
            &mut errors,
            results.neoforge,
            "NeoForge",
        );
        self.error = (!errors.is_empty()).then(|| errors.join("\n"));
    }

    pub fn select(&mut self, version: String) {
        self.selected_version = Some(version);
    }
}

#[derive(Debug)]
pub struct VersionManifestResults {
    pub minecraft: Result<VersionManifest, String>,
    pub forge: Result<ForgeManifest, String>,
    pub fabric: Result<FabricLoaderManifest, String>,
    pub neoforge: Result<NeoForgeManifest, String>,
}

pub async fn fetch_manifests(client: Arc<dyn HttpClient>) -> VersionManifestResults {
    let (minecraft, forge, fabric, neoforge) = futures::join!(
        fetch_minecraft(client.clone()),
        fetch_forge(client.clone()),
        fetch_fabric(client.clone()),
        fetch_neoforge(client),
    );

    VersionManifestResults {
        minecraft,
        forge,
        fabric,
        neoforge,
    }
}

async fn fetch_minecraft(client: Arc<dyn HttpClient>) -> Result<VersionManifest, String> {
    let body = fetch_cached_text(client, "minecraft_v2", VERSION_MANIFEST_URL, "Minecraft").await?;
    VersionManifest::parse(body.as_bytes())
        .map_err(|error| format!("Не удалось разобрать манифест Minecraft: {error}"))
}

async fn fetch_forge(client: Arc<dyn HttpClient>) -> Result<ForgeManifest, String> {
    let body = fetch_cached_text(client, "forge_promotions", FORGE_PROMOTIONS_URL, "Forge").await?;
    ForgeManifest::parse(body.as_bytes())
        .map_err(|error| format!("Не удалось разобрать манифест Forge: {error}"))
}

async fn fetch_fabric(client: Arc<dyn HttpClient>) -> Result<FabricLoaderManifest, String> {
    let body = fetch_cached_text(
        client,
        "fabric_loader_v2",
        FABRIC_LOADER_MANIFEST_URL,
        "Fabric",
    )
    .await?;
    FabricLoaderManifest::parse(body.as_bytes())
        .map_err(|error| format!("Не удалось разобрать манифест Fabric: {error}"))
}

pub async fn fetch_fabric_game_loaders(
    client: Arc<dyn HttpClient>,
    game_version: String,
) -> Result<FabricGameLoaderManifest, String> {
    let encoded_version = encode_path_segment(&game_version);
    let url = format!("{FABRIC_LOADER_MANIFEST_URL}/{encoded_version}");
    let cache_key = format!("fabric_loader_game_{encoded_version}");
    let body = fetch_cached_text(client, &cache_key, &url, "Fabric").await?;
    FabricGameLoaderManifest::parse(body.as_bytes()).map_err(|error| {
        format!("Не удалось разобрать версии Fabric для Minecraft {game_version}: {error}")
    })
}

async fn fetch_neoforge(client: Arc<dyn HttpClient>) -> Result<NeoForgeManifest, String> {
    let body = fetch_cached_text(
        client,
        "neoforge_maven_metadata",
        NEOFORGE_MAVEN_METADATA_URL,
        "NeoForge",
    )
    .await?;
    NeoForgeManifest::parse(&body)
}

async fn fetch_cached_text(
    client: Arc<dyn HttpClient>,
    cache_key: &str,
    url: &str,
    source: &str,
) -> Result<String, String> {
    let cached = match manifest_cache::read(cache_key) {
        Ok(cached) => cached,
        Err(error) => {
            tracing::warn!(manifest = cache_key, %error, "failed to read manifest cache");
            None
        }
    };

    if let Some(cached) = cached.as_ref().filter(|cached| cached.is_fresh()) {
        return Ok(cached.body.clone());
    }

    match request_text(client, url, source).await {
        Ok(body) => {
            if let Err(error) = manifest_cache::write(cache_key, body.clone()) {
                tracing::warn!(manifest = cache_key, %error, "failed to write manifest cache");
            }
            Ok(body)
        }
        Err(error) => {
            if let Some(cached) = cached {
                tracing::warn!(manifest = cache_key, %error, "using stale cached manifest after request failed");
                Ok(cached.body)
            } else {
                Err(error)
            }
        }
    }
}

async fn request_text(
    client: Arc<dyn HttpClient>,
    url: &str,
    source: &str,
) -> Result<String, String> {
    let request = Request::builder()
        .uri(url)
        .timeout(Duration::from_secs(20))
        .body(AsyncBody::empty())
        .map_err(|error| format!("Не удалось подготовить запрос {source}: {error}"))?;

    let response = client
        .send(request)
        .await
        .map_err(|error| format!("Не удалось загрузить манифест {source}: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Сервер манифеста {source} вернул HTTP {status}"));
    }

    let mut body = response.into_body();
    let mut bytes = Vec::new();
    futures::io::AsyncReadExt::read_to_end(&mut body, &mut bytes)
        .await
        .map_err(|error| format!("Не удалось прочитать манифест {source}: {error}"))?;
    String::from_utf8(bytes)
        .map_err(|error| format!("Манифест {source} содержит некорректный UTF-8: {error}"))
}

fn encode_path_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn save_result<T>(
    target: &mut Option<T>,
    errors: &mut Vec<String>,
    result: Result<T, String>,
    source: &str,
) {
    match result {
        Ok(manifest) => *target = Some(manifest),
        Err(error) => errors.push(format!("{source}: {error}")),
    }
}
