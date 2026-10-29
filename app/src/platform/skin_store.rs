use crate::platform::profile_store;
use std::{io, path::PathBuf};

fn directory() -> io::Result<PathBuf> {
    Ok(profile_store::data_directory()?.join("skins"))
}

pub fn path(id: uuid::Uuid) -> io::Result<PathBuf> {
    Ok(directory()?.join(format!("{id}.png")))
}

pub fn load(id: uuid::Uuid) -> io::Result<Vec<u8>> {
    std::fs::read(path(id)?)
}

pub fn save(id: uuid::Uuid, png: &[u8]) -> io::Result<()> {
    let path = path(id)?;
    write_bytes(&path, png)
}

pub fn write_bytes(path: &std::path::Path, bytes: &[u8]) -> io::Result<()> {
    // Persist the PNG with a same-directory temporary file and atomic rename.
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("skin folder has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut temporary, bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| error.error)
}

pub fn remove(id: uuid::Uuid) -> io::Result<()> {
    match std::fs::remove_file(path(id)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn load_index<T: serde::de::DeserializeOwned + Default>() -> io::Result<T> {
    let index = directory()?.join("index.json");
    match std::fs::read(index) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error),
    }
}

pub fn save_index<T: serde::Serialize>(index: &T) -> io::Result<()> {
    profile_store::write_json_atomically(&directory()?.join("index.json"), index)
}
