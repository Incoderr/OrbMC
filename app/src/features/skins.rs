use gpui_kit::RenderImage;
use image::GenericImageView as _;
use serde::{Deserialize, Serialize};
use std::{io::Cursor, path::Path, sync::Arc};
use uuid::Uuid;

const MAX_SKINS: usize = 10;
const MAX_PNG_BYTES: usize = 2 * 1024 * 1024;

pub use rs_mc_launcher_core::minecraft::skin::SkinModel;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SavedSkin {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub model: SkinModel,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SkinIndex {
    skins: Vec<SavedSkin>,
    selected: Option<Uuid>,
    #[serde(default = "default_apply_locally")]
    apply_locally: bool,
}

fn default_apply_locally() -> bool {
    true
}
impl Default for SkinIndex {
    fn default() -> Self {
        Self {
            skins: Vec::new(),
            selected: None,
            apply_locally: true,
        }
    }
}

pub mod local;

#[derive(Debug, Default)]
pub struct SkinState {
    index: SkinIndex,
    error: Option<String>,
    thumbnails: std::collections::HashMap<Uuid, Arc<RenderImage>>,
    textures: std::collections::HashMap<Uuid, Arc<image::RgbaImage>>,
    selected_preview: Option<(Uuid, i16, SkinModel, Arc<RenderImage>)>,
    /// Face crop of the selected skin for the sidebar account block.
    head: Option<Arc<RenderImage>>,
}

impl SkinState {
    pub fn load() -> Self {
        let mut state = Self::default();
        match crate::platform::skin_store::load_index::<SkinIndex>() {
            Ok(mut index) => {
                index.skins.truncate(MAX_SKINS);
                index.skins.retain(|skin| {
                    crate::platform::skin_store::path(skin.id).is_ok_and(|path| path.is_file())
                });
                if !index
                    .selected
                    .is_some_and(|selected| index.skins.iter().any(|skin| skin.id == selected))
                {
                    index.selected = index.skins.first().map(|skin| skin.id);
                }
                state.index = index;
            }
            Err(error) => {
                state.error = Some(format!("Не удалось прочитать список скинов: {error}"))
            }
        }
        state.refresh_head();
        state
    }

    pub fn skins(&self) -> &[SavedSkin] {
        &self.index.skins
    }
    pub fn selected(&self) -> Option<Uuid> {
        self.index.selected
    }
    pub fn selected_skin(&self) -> Option<&SavedSkin> {
        self.index
            .selected
            .and_then(|selected| self.index.skins.iter().find(|skin| skin.id == selected))
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn can_add(&self) -> bool {
        self.index.skins.len() < MAX_SKINS
    }
    pub fn max_skins(&self) -> usize {
        MAX_SKINS
    }

    pub fn apply_locally(&self) -> bool {
        self.index.apply_locally
    }
    pub fn set_apply_locally(&mut self, enabled: bool) -> Result<(), String> {
        let before = self.index.apply_locally;
        self.index.apply_locally = enabled;
        if let Err(error) = self.persist() {
            self.index.apply_locally = before;
            return Err(error);
        }
        Ok(())
    }

    pub fn select(&mut self, id: Uuid) -> Result<(), String> {
        let previous = self.index.selected;
        if !self.index.skins.iter().any(|skin| skin.id == id) {
            return Err("Скин уже удалён".into());
        }
        self.index.selected = Some(id);
        self.selected_preview = None;
        if let Err(error) = self.persist() {
            self.index.selected = previous;
            return Err(error);
        }
        self.refresh_head();
        Ok(())
    }

    pub fn add(
        &mut self,
        name: &str,
        png: &[u8],
        model: SkinModel,
        locale: super::settings::Locale,
    ) -> Result<Uuid, String> {
        if let Some(error) = self.error.as_ref() {
            return Err(error.clone());
        }
        if !self.can_add() {
            return Err("Можно сохранить не больше 10 скинов".into());
        }
        decode_skin(png)?;
        let name = Path::new(name)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("skin");
        let name: String = name
            .chars()
            .filter(|ch| !ch.is_control())
            .take(48)
            .collect();
        let opaque_file_name = name.len() > 28
            && name
                .chars()
                .all(|ch| ch.is_ascii_hexdigit() || matches!(ch, '-' | '_'));
        let generated_name = format!(
            "{} {}",
            locale.text("Скин", "Skin"),
            self.index.skins.len() + 1
        );
        let name = if name.trim().is_empty() || opaque_file_name {
            generated_name.as_str()
        } else {
            name.trim()
        };
        let id = Uuid::new_v4();
        crate::platform::skin_store::save(id, png)
            .map_err(|error| format!("Не удалось сохранить скин: {error}"))?;
        self.index.skins.push(SavedSkin {
            id,
            name: name.to_owned(),
            model,
        });
        self.index.selected = Some(id);
        self.selected_preview = None;
        self.thumbnails.clear();
        if let Err(error) = self.persist() {
            self.index.skins.retain(|skin| skin.id != id);
            self.index.selected = self.index.skins.first().map(|skin| skin.id);
            let _ = crate::platform::skin_store::remove(id);
            return Err(error);
        }
        self.refresh_head();
        Ok(id)
    }

    pub fn remove(&mut self, id: Uuid) -> Result<(), String> {
        let before = self.index.clone();
        self.index.skins.retain(|skin| skin.id != id);
        self.thumbnails.remove(&id);
        self.textures.remove(&id);
        self.selected_preview = None;
        if self.index.selected == Some(id) {
            self.index.selected = self.index.skins.first().map(|skin| skin.id);
        }
        if let Err(error) = self.persist() {
            self.index = before;
            return Err(error);
        }
        self.refresh_head();
        crate::platform::skin_store::remove(id)
            .map_err(|error| format!("Скин удалён из списка, но файл не удалось очистить: {error}"))
    }

    pub fn thumbnail(&self, id: Uuid) -> Option<Arc<RenderImage>> {
        self.thumbnails.get(&id).cloned()
    }

    pub fn selected_preview(&self) -> Option<Arc<RenderImage>> {
        self.selected_preview
            .as_ref()
            .and_then(|(id, _, model, image)| {
                (Some(*id) == self.index.selected
                    && self
                        .selected_skin()
                        .is_some_and(|skin| skin.model == *model))
                .then(|| image.clone())
            })
    }

    pub fn selected_preview_ready(&self, id: Uuid, degree: i16, model: SkinModel) -> bool {
        self.selected_preview
            .as_ref()
            .is_some_and(|(cached_id, cached_degree, cached_model, _)| {
                *cached_id == id && *cached_degree == degree && *cached_model == model
            })
    }

    pub fn texture(&self, id: Uuid) -> Option<Arc<image::RgbaImage>> {
        self.textures.get(&id).cloned()
    }

    /// Face crop of the selected skin for the sidebar account block.
    pub fn head(&self) -> Option<Arc<RenderImage>> {
        self.head.clone()
    }

    /// Recrops the sidebar face from the currently selected skin. Falls back to
    /// `None` when no skin is selected or its texture cannot be decoded.
    fn refresh_head(&mut self) {
        self.head = self.selected_skin().and_then(|skin| {
            let texture = match self.textures.get(&skin.id) {
                Some(texture) => texture.clone(),
                None => {
                    let bytes = crate::platform::skin_store::load(skin.id).ok()?;
                    Arc::new(decode_skin(&bytes).ok()?)
                }
            };
            Some(render_head_image(&texture))
        });
    }

    pub fn cache_thumbnail(
        &mut self,
        id: Uuid,
        model: SkinModel,
        texture: Arc<image::RgbaImage>,
        frame: Arc<RenderImage>,
    ) {
        if self
            .index
            .skins
            .iter()
            .any(|skin| skin.id == id && skin.model == model)
        {
            self.textures.insert(id, texture);
            self.thumbnails.insert(id, frame);
        }
    }

    pub fn render_frame(
        texture: &image::RgbaImage,
        model: SkinModel,
        yaw: f32,
        width: u32,
        height: u32,
    ) -> Arc<RenderImage> {
        let mut buffer =
            rs_mc_launcher_core::minecraft::skin::render(texture, model, yaw, width, height);
        for pixel in buffer.pixels_mut() {
            pixel.0.swap(0, 2);
        }
        Arc::new(RenderImage::new([image::Frame::new(buffer)]))
    }

    pub fn render_skin_thumbnail(
        id: Uuid,
        model: SkinModel,
    ) -> Result<(Arc<image::RgbaImage>, Arc<RenderImage>), String> {
        let bytes = crate::platform::skin_store::load(id)
            .map_err(|error| format!("Не удалось открыть PNG: {error}"))?;
        let texture = Arc::new(decode_skin(&bytes)?);
        let frame = Self::render_frame(&texture, model, -0.24, 96, 144);
        Ok((texture, frame))
    }

    pub fn cache_selected_preview(
        &mut self,
        id: Uuid,
        degree: i16,
        model: SkinModel,
        frame: Arc<RenderImage>,
    ) -> Option<Arc<RenderImage>> {
        if self.index.selected == Some(id)
            && self.selected_skin().is_some_and(|skin| skin.model == model)
        {
            return self
                .selected_preview
                .replace((id, degree, model, frame))
                .map(|(_, _, _, image)| image);
        }
        None
    }

    pub fn selected_model(&self) -> Option<SkinModel> {
        self.selected_skin().map(|skin| skin.model)
    }

    pub fn update_model(&mut self, id: Uuid, model: SkinModel) -> Result<(), String> {
        let skin = self
            .index
            .skins
            .iter_mut()
            .find(|skin| skin.id == id)
            .ok_or("Скин уже удалён")?;
        let previous = skin.model;
        skin.model = model;
        self.thumbnails.remove(&id);
        self.selected_preview = None;
        if let Err(error) = self.persist() {
            if let Some(skin) = self.index.skins.iter_mut().find(|skin| skin.id == id) {
                skin.model = previous;
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn set_model(&mut self, id: Uuid, model: SkinModel) -> Result<(), String> {
        self.update_model(id, model)
    }

    fn persist(&mut self) -> Result<(), String> {
        crate::platform::skin_store::save_index(&self.index).map_err(|error| {
            self.error = Some(format!("Не удалось сохранить список скинов: {error}"));
            self.error.clone().unwrap_or_else(|| error.to_string())
        })?;
        self.error = None;
        Ok(())
    }
}

pub fn decode_skin(bytes: &[u8]) -> Result<image::RgbaImage, String> {
    if bytes.len() > MAX_PNG_BYTES {
        return Err("PNG скина должен быть не больше 2 МиБ".into());
    }
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Выбери PNG-файл скина Minecraft".into());
    }
    let reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let dimensions = reader
        .into_dimensions()
        .map_err(|error| format!("PNG не удалось прочитать: {error}"))?;
    if !matches!(dimensions, (64, 64) | (64, 32)) {
        return Err("Размер PNG должен быть 64 × 64 или 64 × 32 пикселя".into());
    }
    image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map(|image| image.into_rgba8())
        .map_err(|error| format!("PNG не удалось декодировать: {error}"))
}

/// Renders the selected skin's face (front head layer plus hat layer) as a
/// square image for the sidebar account block.
fn render_head_image(texture: &image::RgbaImage) -> Arc<RenderImage> {
    let scale = (texture.width() / 64).max(1);
    let face = texture
        .view(8 * scale, 8 * scale, 8 * scale, 8 * scale)
        .to_image();
    let hat = texture
        .view(40 * scale, 8 * scale, 8 * scale, 8 * scale)
        .to_image();
    let mut head = image::RgbaImage::new(8 * scale, 8 * scale);
    image::imageops::overlay(&mut head, &face, 0, 0);
    image::imageops::overlay(&mut head, &hat, 0, 0);
    let mut face_image =
        image::imageops::resize(&head, 64, 64, image::imageops::FilterType::Nearest);
    for pixel in face_image.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new([image::Frame::new(face_image)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completed_frames_are_presented_during_rotation_and_models_do_not_mix() {
        let id = Uuid::new_v4();
        let mut state = SkinState::default();
        state.index.skins.push(SavedSkin {
            id,
            name: "Skin".into(),
            model: SkinModel::Classic,
        });
        state.index.selected = Some(id);
        let texture = image::RgbaImage::from_pixel(64, 64, image::Rgba([10, 20, 30, 255]));
        let frame = SkinState::render_frame(&texture, SkinModel::Classic, 0., 224, 384);
        assert!(
            frame
                .as_bytes(0)
                .unwrap()
                .chunks_exact(4)
                .any(|pixel| pixel == [30, 20, 10, 255])
        );
        state.cache_selected_preview(id, 10, SkinModel::Classic, frame.clone());
        assert!(state.selected_preview().is_some());
        assert!(!state.selected_preview_ready(id, 12, SkinModel::Classic));
        let old = state
            .cache_selected_preview(id, 12, SkinModel::Classic, frame.clone())
            .unwrap();
        assert_eq!(old.id, frame.id);
        state.index.skins[0].model = SkinModel::Slim;
        state.cache_selected_preview(id, 13, SkinModel::Classic, frame);
        assert!(state.selected_preview().is_none());
    }
    #[test]
    fn old_skin_libraries_default_to_local_use() {
        let index: SkinIndex = serde_json::from_str(r#"{"skins":[],"selected":null}"#).unwrap();
        assert!(index.apply_locally);
    }
}
