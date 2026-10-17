//! Persistent cache for machine-translated texts.
//!
//! Translations are stored in one JSON file per (language, provider) pair as
//! a map of `sha1(original text) -> translated text`. Entry versioning is the
//! original-text hash instead of a TTL: if a provider description changes the
//! hash no longer matches and the entry is treated as a miss.
//!
//! All entries of a shard live in memory once loaded, so lookups are cheap
//! and every finished translation only rewrites a single small file.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};

/// On-disk format version, bumped when the layout changes.
const FORMAT_VERSION: u32 = 1;

/// Lowercase hex SHA-1 of the text.
fn text_hash(text: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(text.as_bytes());
    hex(&hasher.finalize())
}

fn hex(digest: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[derive(Default)]
struct Shard {
    loaded: bool,
    entries: HashMap<String, String>,
}

/// All shards of the process; each is read from disk exactly once.
static SHARDS: OnceLock<Mutex<HashMap<ShardId, Shard>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ShardId {
    source: &'static str,
    lang: &'static str,
}

#[derive(Debug, Serialize, Deserialize)]
struct ShardFile {
    version: u32,
    entries: HashMap<String, String>,
}

/// Reads a cached translation. Returns `None` when the shard has no entry or
/// when the stored translation was made from a different original text.
pub fn read(source: &'static str, language: &'static str, original: &str) -> Option<String> {
    let key = text_hash(original);
    let collection = SHARDS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut collection = collection.lock().expect("translation cache lock");
    let shard = collection
        .entry(ShardId {
            source,
            lang: language,
        })
        .or_default();
    if !shard.loaded {
        shard.entries = load_shard_file(source, language).unwrap_or_default();
        shard.loaded = true;
    }
    shard.entries.get(&key).cloned()
}

/// Adds a translation to its shard and persists the shard atomically.
pub fn write(source: &'static str, language: &'static str, original: &str, text: &str) {
    let key = text_hash(original);
    let collection = SHARDS.get_or_init(|| Mutex::new(HashMap::new()));
    let snapshot = {
        let mut collection = collection.lock().expect("translation cache lock");
        let shard = collection
            .entry(ShardId {
                source,
                lang: language,
            })
            .or_default();
        shard.loaded = true;
        shard.entries.insert(key, text.to_owned());
        shard.entries.clone()
        // Lock released: the disk write happens meanwhile.
    };
    save_shard_file(source, language, &snapshot);
}

/// `<data>/translations/<lang>/<Source>.json`
fn shard_path(source: &str, language: &str) -> std::io::Result<std::path::PathBuf> {
    let root = crate::platform::profile_store::data_directory()?;
    Ok(root
        .join("translations")
        .join(language)
        .join(format!("{source}.json")))
}

fn load_shard_file(source: &str, language: &str) -> Option<HashMap<String, String>> {
    let path = shard_path(source, language).ok()?;
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<ShardFile>(&bytes) {
            Ok(file) if file.version == FORMAT_VERSION => Some(file.entries),
            // Future versions are treated as empty until an upgrade path
            // exists; old versions simply get overwritten.
            Ok(_) => Some(HashMap::new()),
            Err(error) => {
                tracing::warn!(%error, "ignoring corrupt translation shard file");
                None
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            tracing::warn!(%error, "failed to read translation shard file");
            None
        }
    }
}

fn save_shard_file(source: &str, language: &str, entries: &HashMap<String, String>) {
    let file = ShardFile {
        version: FORMAT_VERSION,
        entries: entries.clone(),
    };
    let Ok(path) = shard_path(source, language) else {
        return;
    };
    if let Err(error) = crate::platform::profile_store::write_json_atomically(&path, &file) {
        tracing::warn!(%error, "failed to write translation shard file");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique original texts keep the shared shard files isolated between
    /// test runs.
    fn unique(seed: u32) -> String {
        format!("{seed}-{}", uuid::Uuid::new_v4())
    }

    #[test]
    fn text_hash_is_stable_and_hex() {
        let first = text_hash("hello");
        let second = text_hash("hello");
        assert_eq!(first, second);
        assert_eq!(first.len(), 40);
        assert!(
            first
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        );

        let other = text_hash("hello2");
        assert_ne!(first, other);
    }

    #[test]
    fn read_missing_entry_returns_none() {
        // Never written: neither for this text nor for anything sharing it.
        assert_eq!(read("Modrinth", "ru", &unique(1)), None);
    }

    #[test]
    fn write_then_read_roundtrip() {
        let source = "Modrinth";
        let original = unique(2);
        assert_eq!(read(source, "ru", &original), None);

        write(source, "ru", &original, "перевод проверки");
        assert_eq!(
            read(source, "ru", &original),
            Some("перевод проверки".into())
        );
    }

    #[test]
    fn changed_original_text_is_a_cache_miss() {
        let source = "Modrinth";
        // Only this exact text is stored; any other original must miss,
        // which is what invalidates translations of updated descriptions.
        let stored = unique(3);
        write(source, "ru", &stored, "перевод");
        assert_eq!(read(source, "ru", &stored), Some("перевод".into()));
        assert_eq!(read(source, "ru", &format!("{stored} changed")), None);
    }

    #[test]
    fn several_entries_share_the_same_shard_file() {
        let source = "CurseForge";
        let first = unique(4);
        let second = unique(5);
        write(source, "ru", &first, "первый");
        write(source, "ru", &second, "второй");
        // The previous write must not have erased the earlier one.
        assert_eq!(read(source, "ru", &first), Some("первый".into()));
        assert_eq!(read(source, "ru", &second), Some("второй".into()));
    }

    #[test]
    fn shard_file_has_the_versioned_layout() {
        let source = "Modrinth";
        let original = unique(6);
        write(source, "ru", &original, "разметка файла");
        // After persistence the file is readable as a shard.
        let file_path = crate::platform::profile_store::data_directory()
            .expect("data directory")
            .join("translations")
            .join("ru")
            .join(format!("{source}.json"));
        let file: ShardFile =
            serde_json::from_slice(&std::fs::read(file_path).expect("shard file exists"))
                .expect("valid shard JSON");
        assert_eq!(file.version, FORMAT_VERSION);
        assert!(file.entries.contains_key(&text_hash(&original)));
    }
}
