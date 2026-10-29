use serde::{Deserialize, Serialize};
use std::{io, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModFile {
    pub filename: String,
    pub sha1: String,
    pub fingerprint: u32,
}

pub fn scan(directory: &Path) -> io::Result<Vec<ModFile>> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file()
            || !entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
        {
            continue;
        }
        let Some(filename) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if entry.metadata()?.len() > 512 * 1024 * 1024 {
            return Err(io::Error::other("Mod file exceeds the 512 MiB scan limit"));
        }
        let sha1 = mc_launcher_core::io::hash::sha1_file(entry.path()).map_err(io::Error::other)?;
        let bytes = std::fs::read(entry.path())?;
        files.push(ModFile {
            filename,
            sha1,
            fingerprint: fingerprint(&bytes),
        });
    }
    Ok(files)
}

/// CurseForge file fingerprint: MurmurHash2, seed 1, excluding ASCII whitespace.
pub fn fingerprint(bytes: &[u8]) -> u32 {
    let bytes: Vec<_> = bytes
        .iter()
        .copied()
        .filter(|b| !matches!(b, 9 | 10 | 13 | 32))
        .collect();
    const M: u32 = 0x5bd1e995;
    let mut hash = 1 ^ bytes.len() as u32;
    let (chunks, tail) = bytes.as_chunks::<4>();
    for chunk in chunks {
        let mut k = u32::from_le_bytes(*chunk).wrapping_mul(M);
        k ^= k >> 24;
        hash = hash.wrapping_mul(M) ^ k.wrapping_mul(M);
    }
    for (index, byte) in tail.iter().enumerate() {
        hash ^= u32::from(*byte) << (index * 8);
    }
    if !tail.is_empty() {
        hash = hash.wrapping_mul(M);
    }
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(M);
    hash ^ (hash >> 15)
}

pub fn read_json<T: serde::de::DeserializeOwned + Default>(path: &Path) -> io::Result<T> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprint_ignores_only_curseforge_whitespace() {
        assert_eq!(fingerprint(b"a b\tc\nd\re"), fingerprint(b"abcde"));
        assert_ne!(fingerprint(b"abcde"), fingerprint(b"abcd"));
        assert_eq!(fingerprint(b""), 0x5bd15e36);
    }
}
