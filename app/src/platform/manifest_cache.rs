use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedManifest {
    fetched_at: u64,
    pub body: String,
}

impl CachedManifest {
    pub fn is_fresh(&self) -> bool {
        let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return false;
        };

        now.as_secs().saturating_sub(self.fetched_at) < CACHE_TTL.as_secs()
    }
}

pub fn read(key: &str) -> std::io::Result<Option<CachedManifest>> {
    let path = cache_path(key)?;
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn write(key: &str, body: String) -> std::io::Result<()> {
    let path = cache_path(key)?;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "manifest cache path has no parent",
        )
    })?;
    std::fs::create_dir_all(parent)?;

    let fetched_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_secs();
    let cached = CachedManifest { fetched_at, body };
    let bytes = serde_json::to_vec(&cached)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    std::fs::write(path, bytes)
}

fn cache_path(key: &str) -> std::io::Result<std::path::PathBuf> {
    let root = dirs::cache_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "system cache directory is unavailable",
        )
    })?;
    let file_name: String = key
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if file_name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "manifest cache key is empty",
        ));
    }

    Ok(root
        .join("orbmc")
        .join("manifests")
        .join(format!("{file_name}.json")))
}
