use std::path::{Path, PathBuf};
use std::{io, time::SystemTime};

pub fn list(saves: &Path) -> io::Result<Vec<String>> {
    let entries = match std::fs::read_dir(saves) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut worlds = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && entry.path().join("level.dat").is_file()
            && let Ok(name) = entry.file_name().into_string()
        {
            worlds.push(name);
        }
    }
    worlds.sort_by_key(|name| name.to_lowercase());
    Ok(worlds)
}

/// A world folder with what the home page shows about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedWorld {
    pub folder: String,
    /// When `level.dat` was last written: the game saves it while playing
    /// and on exit, so this is when the world was last played.
    pub last_played: SystemTime,
    /// `icon.png` the game takes when the world is first opened.
    pub icon: Option<PathBuf>,
}

/// Worlds in `saves` with their last play time, in no particular order.
pub fn saved(saves: &Path) -> io::Result<Vec<SavedWorld>> {
    let mut worlds = Vec::new();
    for folder in list(saves)? {
        let world = saves.join(&folder);
        let Ok(last_played) = std::fs::metadata(world.join("level.dat")).and_then(|m| m.modified())
        else {
            continue;
        };
        let icon = Some(world.join("icon.png")).filter(|icon| icon.is_file());
        worlds.push(SavedWorld {
            folder,
            last_played,
            icon,
        });
    }
    Ok(worlds)
}

pub fn validate(saves: &Path, folder: &str) -> io::Result<()> {
    let root = saves.canonicalize()?;
    let world = saves.join(folder).canonicalize()?;
    if world.parent() != Some(root.as_path()) || !world.join("level.dat").is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Мир отсутствует в папке сохранений сборки",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_world_folders_are_listed_and_outside_paths_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let saves = directory.path().join("saves");
        assert!(list(&saves).unwrap().is_empty());
        std::fs::create_dir_all(saves.join("Мой мир")).unwrap();
        std::fs::create_dir_all(saves.join("unfinished")).unwrap();
        std::fs::write(saves.join("Мой мир/level.dat"), b"world").unwrap();
        std::fs::create_dir_all(directory.path().join("other")).unwrap();
        std::fs::write(directory.path().join("other/level.dat"), b"world").unwrap();
        assert_eq!(list(&saves).unwrap(), ["Мой мир"]);
        assert!(validate(&saves, "Мой мир").is_ok());
        assert!(validate(&saves, "../other").is_err());
        assert!(validate(&saves, "missing").is_err());

        let saved = saved(&saves).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].folder, "Мой мир");
        assert_eq!(saved[0].icon, None);
        std::fs::write(saves.join("Мой мир/icon.png"), b"png").unwrap();
        assert_eq!(
            super::saved(&saves).unwrap()[0].icon,
            Some(saves.join("Мой мир").join("icon.png"))
        );
    }
}
