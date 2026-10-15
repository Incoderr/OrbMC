//! Client-only skin preparation, executed by the launch worker before Java starts.
use super::{SkinIndex, SkinModel, decode_skin};
use crate::{
    features::{instances::InstanceProfile, versions::LoaderChoice},
    platform::{mod_files, profile_store, skin_store},
};
use mc_launcher_core::{
    net::download::{Checksum, DownloadPlan, DownloadTask, execute_plan},
    progress::ProgressEvent,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{io::Read, path::Path, time::Duration};

const LOCAL_PROVIDER: &str = "RS MC Launcher Local";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Backend {
    SkinPort,
    CustomSkinLoader,
}

impl Backend {
    fn prefix(self) -> &'static str {
        match self {
            Self::SkinPort => "skinport",
            Self::CustomSkinLoader => "customskinloader",
        }
    }
}

pub fn prepare(profile: &InstanceProfile, directory: &Path, username: &str) -> Result<(), String> {
    let index: SkinIndex = skin_store::load_index()
        .map_err(|error| format!("Не удалось прочитать выбранный скин: {error}"))?;
    if !index.apply_locally {
        return Ok(());
    }
    let Some(skin) = index
        .selected
        .and_then(|id| index.skins.iter().find(|skin| skin.id == id))
    else {
        return Ok(());
    };
    let png = skin_store::load(skin.id)
        .map_err(|error| format!("Не удалось открыть выбранный скин: {error}"))?;
    decode_skin(&png)?;
    if username.is_empty()
        || username.len() > 16
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("Имя игрока непригодно для локального скина".into());
    }
    let backend = backend(profile)?;
    tracing::info!(profile_id = %profile.id, ?backend, "preparing local player skin");
    ensure_mod(profile, directory, backend)?;
    write_local_files(directory, backend, username, skin.model, &png)
}

fn backend(profile: &InstanceProfile) -> Result<Backend, String> {
    if profile.loader == LoaderChoice::Vanilla {
        return Err("Для локального скина нужна сборка с Forge, Fabric или NeoForge. Создай такую сборку либо отключи «Использовать в игре локально» на странице скинов.".into());
    }
    if profile.game_version == "1.7.10" && profile.loader == LoaderChoice::Forge {
        return Ok(Backend::SkinPort);
    }
    if profile.game_version.starts_with("1.7.") || profile.game_version.starts_with("1.6.") {
        return Err("Локальный скин для этой версии не поддерживается. Для старых версий доступен Forge 1.7.10.".into());
    }
    Ok(Backend::CustomSkinLoader)
}

fn write_local_files(
    directory: &Path,
    backend: Backend,
    username: &str,
    model: SkinModel,
    png: &[u8],
) -> Result<(), String> {
    let skin_path = match backend {
        Backend::SkinPort => directory
            .join("cachedImages/skins")
            .join(format!("{username}.png")),
        Backend::CustomSkinLoader => directory
            .join("CustomSkinLoader/LocalSkin/skins")
            .join(format!("{username}.png")),
    };
    skin_store::write_bytes(&skin_path, png)
        .map_err(|error| format!("Не удалось применить локальный скин: {error}"))?;
    if backend == Backend::CustomSkinLoader {
        let config_path = directory.join("CustomSkinLoader/CustomSkinLoader.json");
        let mut config: Value = mod_files::read_json(&config_path)
            .map_err(|error| format!("Не удалось прочитать настройки CustomSkinLoader: {error}"))?;
        configure_local_provider(&mut config, model)?;
        profile_store::write_json_atomically(&config_path, &config)
            .map_err(|error| format!("Не удалось сохранить настройки CustomSkinLoader: {error}"))?;
    }
    Ok(())
}

fn configure_local_provider(config: &mut Value, model: SkinModel) -> Result<(), String> {
    if config.is_null() {
        *config = json!({});
    }
    let object = config
        .as_object_mut()
        .ok_or("Настройки CustomSkinLoader должны быть JSON-объектом")?;
    let providers = object
        .entry("loadlist")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("Список источников CustomSkinLoader имеет неверный формат")?;
    providers
        .retain(|provider| provider.get("name").and_then(Value::as_str) != Some(LOCAL_PROVIDER));
    providers.insert(
        0,
        json!({
            "name": LOCAL_PROVIDER, "type": "Legacy",
            "skin": "LocalSkin/skins/{USERNAME}.png",
            "model": match model { SkinModel::Classic => "default", SkinModel::Slim => "slim" }
        }),
    );
    object.insert("enableLocalProfileCache".into(), json!(false));
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Installation {
    backend: Backend,
    game_version: String,
    loader: String,
    filename: String,
    sha1: String,
}

fn loader_name(loader: LoaderChoice) -> &'static str {
    match loader {
        LoaderChoice::Fabric => "fabric",
        LoaderChoice::Forge => "forge",
        LoaderChoice::NeoForge => "neoforge",
        LoaderChoice::Vanilla => "minecraft",
    }
}

fn ensure_mod(profile: &InstanceProfile, directory: &Path, backend: Backend) -> Result<(), String> {
    let mods = directory.join("mods");
    let record_path = mods.join(".rs-launcher-local-skin.json");
    let installed: Option<Installation> = mod_files::read_json(&record_path)
        .map_err(|error| format!("Не удалось прочитать данные мода скинов: {error}"))?;
    if let Some(installed) = &installed
        && installed.backend == backend
        && installed.game_version == profile.game_version
        && installed.loader == loader_name(profile.loader)
        && safe_filename(&installed.filename)
        && mods.join(&installed.filename).is_file()
        && mc_launcher_core::io::hash::sha1_file(mods.join(&installed.filename))
            .map_err(|error| error.to_string())?
            == installed.sha1
    {
        return Ok(());
    }
    // Respect a skin loader the user has already installed.
    match std::fs::read_dir(&mods) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|error| error.to_string())?;
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.starts_with(backend.prefix())
                    && name.ends_with(".jar")
                    && valid_jar(&entry.path())
                {
                    return Ok(());
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Не удалось прочитать папку модов: {error}")),
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(45))
        .user_agent("RS-MC-Launcher/0.1 (local skins)")
        .build()
        .map_err(|error| error.to_string())?;
    let asset = resolve_asset(&client, profile, backend)?;
    if !safe_filename(&asset.filename) || asset.size == 0 || asset.size > 32 * 1024 * 1024 {
        return Err("Источник вернул неверный файл мода скинов".into());
    }
    let cache = skin_store::mod_cache_dir().map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
    let cached = cache.join(&asset.filename);
    let reusable = valid_jar(&cached)
        && std::fs::metadata(&cached).is_ok_and(|meta| meta.len() == asset.size)
        && asset.sha1.as_ref().is_none_or(|hash| {
            mc_launcher_core::io::hash::sha1_file(&cached)
                .is_ok_and(|actual| actual.eq_ignore_ascii_case(hash))
        });
    if !reusable {
        let temporary = tempfile::tempdir_in(&cache).map_err(|error| error.to_string())?;
        let download = temporary.path().join("skin-loader.jar");
        let plan = DownloadPlan {
            tasks: vec![DownloadTask {
                url: asset.url,
                destination: download.clone(),
                checksum: asset.sha1.clone().map(Checksum::Sha1),
                size: Some(asset.size),
                label: format!("{} · локальный скин", asset.filename),
            }],
        };
        execute_plan(&plan, &mut |event: ProgressEvent| {
            tracing::debug!(?event, "skin mod download progress")
        })
        .map_err(|error| format!("Не удалось скачать мод локального скина: {error}"))?;
        if !valid_jar(&download)
            || std::fs::metadata(&download)
                .map_err(|error| error.to_string())?
                .len()
                != asset.size
        {
            return Err("Файл мода скинов повреждён".into());
        }
        let bytes = std::fs::read(download).map_err(|error| error.to_string())?;
        skin_store::write_bytes(&cached, &bytes).map_err(|error| error.to_string())?;
    }
    let destination = mods.join(&asset.filename);
    let bytes = std::fs::read(&cached).map_err(|error| error.to_string())?;
    skin_store::write_bytes(&destination, &bytes)
        .map_err(|error| format!("Не удалось установить мод скинов: {error}"))?;
    let sha1 =
        mc_launcher_core::io::hash::sha1_file(&destination).map_err(|error| error.to_string())?;
    profile_store::write_json_atomically(
        &record_path,
        &Some(Installation {
            backend,
            game_version: profile.game_version.clone(),
            loader: loader_name(profile.loader).into(),
            filename: asset.filename.clone(),
            sha1,
        }),
    )
    .map_err(|error| error.to_string())?;
    if let Some(project) = asset.project_id {
        crate::features::mods::installed::record_install(
            &mods,
            format!("modrinth:{project}"),
            &asset.filename,
        )?;
    }
    Ok(())
}

fn safe_filename(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['/', '\\', ':'])
        && name.to_ascii_lowercase().ends_with(".jar")
}
fn valid_jar(path: &Path) -> bool {
    let mut signature = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut signature))
        .is_ok()
        && signature == *b"PK\x03\x04"
}

struct Asset {
    filename: String,
    url: String,
    sha1: Option<String>,
    size: u64,
    project_id: Option<String>,
}

fn resolve_asset(
    client: &reqwest::blocking::Client,
    profile: &InstanceProfile,
    backend: Backend,
) -> Result<Asset, String> {
    if backend == Backend::SkinPort {
        #[derive(Deserialize)]
        struct Release {
            draft: bool,
            prerelease: bool,
            assets: Vec<GithubAsset>,
        }
        #[derive(Deserialize)]
        struct GithubAsset {
            name: String,
            browser_download_url: String,
            size: u64,
        }
        let body = client
            .get("https://api.github.com/repos/zlainsama/SkinPort/releases")
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.text())
            .map_err(|error| format!("Не удалось найти SkinPort для 1.7.10: {error}"))?;
        let releases: Vec<Release> =
            serde_json::from_str(&body).map_err(|error| error.to_string())?;
        let asset = releases
            .into_iter()
            .filter(|release| !release.draft && !release.prerelease)
            .flat_map(|release| release.assets)
            .find(|asset| {
                asset
                    .name
                    .to_ascii_lowercase()
                    .starts_with("skinport-1.7.10-")
                    && asset.name.ends_with(".jar")
            })
            .ok_or("Официальный выпуск SkinPort для 1.7.10 не найден")?;
        if !asset
            .browser_download_url
            .starts_with("https://github.com/zlainsama/SkinPort/releases/download/")
        {
            return Err("Источник SkinPort вернул неожиданный адрес".into());
        }
        return Ok(Asset {
            filename: asset.name,
            url: asset.browser_download_url,
            size: asset.size,
            sha1: None,
            project_id: None,
        });
    }
    #[derive(Deserialize)]
    struct ModVersion {
        project_id: String,
        files: Vec<ModFile>,
    }
    #[derive(Deserialize)]
    struct ModFile {
        filename: String,
        url: String,
        size: u64,
        primary: bool,
        hashes: std::collections::HashMap<String, String>,
    }
    let versions =
        serde_json::to_string(&[&profile.game_version]).map_err(|error| error.to_string())?;
    let loaders =
        serde_json::to_string(&[loader_name(profile.loader)]).map_err(|error| error.to_string())?;
    let mut url =
        reqwest::Url::parse("https://api.modrinth.com/v2/project/customskinloader/version")
            .map_err(|error| error.to_string())?;
    url.query_pairs_mut()
        .append_pair("game_versions", &versions)
        .append_pair("loaders", &loaders);
    let body = client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.text())
        .map_err(|error| format!("Не удалось найти CustomSkinLoader: {error}"))?;
    let mut versions: Vec<ModVersion> =
        serde_json::from_str(&body).map_err(|error| error.to_string())?;
    let version = versions.drain(..).next().ok_or_else(|| format!("CustomSkinLoader для Minecraft {} / {} не найден. Можно отключить локальный скин и запустить игру.", profile.game_version, loader_name(profile.loader)))?;
    let file = version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
        .ok_or("У CustomSkinLoader нет файла для скачивания")?;
    if !file.url.starts_with("https://cdn.modrinth.com/") {
        return Err("Источник мода скинов вернул неожиданный адрес".into());
    }
    Ok(Asset {
        filename: file.filename.clone(),
        url: file.url.clone(),
        sha1: file.hashes.get("sha1").cloned(),
        size: file.size,
        project_id: Some(version.project_id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_skin_files_target_the_launched_player_and_keep_other_providers() {
        let directory = tempfile::tempdir().unwrap();
        let image = image::RgbaImage::from_pixel(64, 64, image::Rgba([12, 34, 56, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        write_local_files(
            directory.path(),
            Backend::SkinPort,
            "Player",
            SkinModel::Classic,
            png.get_ref(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(directory.path().join("cachedImages/skins/Player.png")).unwrap(),
            *png.get_ref()
        );
        let config_path = directory
            .path()
            .join("CustomSkinLoader/CustomSkinLoader.json");
        profile_store::write_json_atomically(
            &config_path,
            &json!({"enableCape":false,"loadlist":[{"name":"Other","type":"MojangAPI"}]}),
        )
        .unwrap();
        write_local_files(
            directory.path(),
            Backend::CustomSkinLoader,
            "Player",
            SkinModel::Slim,
            png.get_ref(),
        )
        .unwrap();
        write_local_files(
            directory.path(),
            Backend::CustomSkinLoader,
            "Player",
            SkinModel::Classic,
            png.get_ref(),
        )
        .unwrap();
        let config: Value = mod_files::read_json(&config_path).unwrap();
        assert_eq!(config["enableCape"], false);
        assert_eq!(config["loadlist"].as_array().unwrap().len(), 2);
        assert_eq!(config["loadlist"][0]["name"], LOCAL_PROVIDER);
        assert_eq!(config["loadlist"][0]["model"], "default");
        assert_eq!(config["loadlist"][1]["name"], "Other");
    }
}
