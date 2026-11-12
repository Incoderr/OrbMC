//! Icons the user picks for a build: a square PNG copy kept in the build
//! data directory, so the build keeps it when the original file moves.

use std::path::{Path, PathBuf};

use image::imageops::FilterType;

/// Picture files offered for an icon; all of them decode with `image`.
pub const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

/// Side of the stored icon; covers the largest place the icon is drawn.
const ICON_SIZE: u32 = 256;

#[derive(Debug, thiserror::Error)]
pub enum IconError {
    #[error("не удалось прочитать картинку {path}: {source}")]
    Decode {
        path: PathBuf,
        source: image::ImageError,
    },
    #[error("не удалось сохранить иконку: {0}")]
    Save(#[from] image::ImageError),
    #[error("не удалось сохранить иконку: {0}")]
    Io(#[from] std::io::Error),
}

/// Stores `source` as the icon of a build and returns its file name inside
/// `data_directory`. The name is new on every pick: GPUI caches images by
/// path, and a reused name would keep showing the old picture.
pub fn store(data_directory: &Path, source: &Path) -> Result<String, IconError> {
    let picture = image::open(source).map_err(|error| IconError::Decode {
        path: source.to_path_buf(),
        source: error,
    })?;
    let icon = picture.resize_to_fill(ICON_SIZE, ICON_SIZE, FilterType::Lanczos3);
    std::fs::create_dir_all(data_directory)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    let name = format!("icon-{stamp}.png");
    icon.save_with_format(data_directory.join(&name), image::ImageFormat::Png)?;
    Ok(name)
}

/// Removes an icon file that is no longer used; a missing file is fine.
pub fn remove(data_directory: &Path, name: &str) {
    match std::fs::remove_file(data_directory.join(name)) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, icon = name, "failed to remove old build icon"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_a_square_png_copy() {
        let directory = tempfile::tempdir().expect("temp dir");
        let source = directory.path().join("wide.png");
        image::RgbaImage::new(64, 32)
            .save(&source)
            .expect("source image");
        let data = directory.path().join("data");

        let name = store(&data, &source).expect("stored icon");
        let stored = image::open(data.join(&name)).expect("stored icon opens");
        assert_eq!((stored.width(), stored.height()), (ICON_SIZE, ICON_SIZE));

        remove(&data, &name);
        assert!(!data.join(&name).exists());
    }

    #[test]
    fn rejects_files_that_are_not_images() {
        let directory = tempfile::tempdir().expect("temp dir");
        let source = directory.path().join("notes.png");
        std::fs::write(&source, b"not a picture").expect("source file");
        assert!(matches!(
            store(directory.path(), &source),
            Err(IconError::Decode { .. })
        ));
    }
}
