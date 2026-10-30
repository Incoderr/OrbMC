//! The skin an Ely.by account wears in game. Ely.by has no API for changing
//! it (docs.ely.by: the skin system only serves textures), so the launcher
//! reads it and sends the player to the site to change it.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::RenderImage;
use serde::Deserialize;

use super::{SkinModel, SkinState, decode_skin, render_head_image};

/// Ely.by page for uploading a new skin.
pub const UPLOAD_PAGE: &str = "https://ely.by/skins/add";
const TEXTURES: &str = "https://skinsystem.ely.by/textures";

pub struct ElySkin {
    pub model: SkinModel,
    pub preview: Arc<RenderImage>,
    pub head: Arc<RenderImage>,
}

#[derive(Deserialize)]
struct Textures {
    #[serde(rename = "SKIN")]
    skin: Option<Texture>,
}

#[derive(Deserialize)]
struct Texture {
    url: String,
    #[serde(default)]
    metadata: Option<Metadata>,
}

#[derive(Deserialize)]
struct Metadata {
    #[serde(default)]
    model: Option<String>,
}

/// The skin of `nickname` on Ely.by, or `None` when the account has none
/// (the game then shows a default one). Blocks; run it off the UI thread.
pub fn fetch(nickname: &str) -> Result<Option<ElySkin>, String> {
    let message = |error: reqwest::Error| format!("Не удалось загрузить скин с Ely.by: {error}");
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("OrbMC/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(message)?;
    let response = client
        .get(format!("{TEXTURES}/{nickname}"))
        .send()
        .map_err(message)?;
    // 204: the account has no textures.
    if response.status() == reqwest::StatusCode::NO_CONTENT
        || response.status() == reqwest::StatusCode::NOT_FOUND
    {
        return Ok(None);
    }
    let textures: Textures = response
        .error_for_status()
        .and_then(|response| response.json())
        .map_err(message)?;
    let Some(skin) = textures.skin else {
        return Ok(None);
    };
    let model = match skin.metadata.and_then(|metadata| metadata.model).as_deref() {
        Some("slim") => SkinModel::Slim,
        _ => SkinModel::Classic,
    };
    let bytes = client
        .get(&skin.url)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.bytes())
        .map_err(message)?;
    let texture = decode_skin(&bytes)?;
    Ok(Some(ElySkin {
        model,
        preview: SkinState::render_frame(&texture, model, -0.24, 224, 384),
        head: render_head_image(&texture),
    }))
}

/// Copies a library skin to a file named after it, so it can be picked on
/// the Ely.by upload page. Returns the copy's path.
pub fn export_for_upload(id: super::SkinId, name: &str) -> Result<std::path::PathBuf, String> {
    let bytes = crate::platform::skin_store::load(id.uuid())
        .map_err(|error| format!("Не удалось открыть PNG: {error}"))?;
    let stem: String = name
        .trim_end_matches(".png")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = if stem.is_empty() {
        "skin".to_owned()
    } else {
        stem
    };
    let directory = std::env::temp_dir().join("OrbMC skins");
    let path = directory.join(format!("{stem}.png"));
    crate::platform::skin_store::write_bytes(&path, &bytes)
        .map_err(|error| format!("Не удалось подготовить файл скина: {error}"))?;
    Ok(path)
}
