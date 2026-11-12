use std::path::PathBuf;
use std::time::SystemTime;

use crate::{
    features::instances::{InstanceId, InstanceProfile},
    platform::{profile_store, worlds},
};

#[derive(Clone, Debug)]
pub enum WorldList {
    Loading,
    Ready(Vec<String>),
    Failed(String),
}

pub fn load(profile: InstanceProfile) -> WorldList {
    let result = (|| {
        let version_id = profile
            .installed_version_id
            .as_ref()
            .ok_or("Сначала установи сборку")?;
        let launcher = mc_launcher_core::prelude::Launcher::new(
            profile_store::shared_game_directory(&profile.installation_root),
        );
        let version = launcher
            .load_version(version_id)
            .map_err(|error| error.to_string())?;
        let metadata = serde_json::to_value(version).map_err(|error| error.to_string())?;
        if !orbmc_core::minecraft::quick_play::supports_singleplayer(&metadata) {
            return Err("Эта версия не поддерживает вход прямо в мир (Quick Play)".into());
        }
        worlds::list(&profile.game_directory().join("saves"))
            .map_err(|error| format!("Не удалось прочитать сохранения: {error}"))
    })();
    match result {
        Ok(worlds) => WorldList::Ready(worlds),
        Err(error) => WorldList::Failed(error),
    }
}

/// A world on the home page: one of the last played ones over all builds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentWorld {
    pub profile: InstanceId,
    pub folder: String,
    pub last_played: SystemTime,
    pub icon: Option<PathBuf>,
}

/// The `limit` most recently played worlds of the builds that can open a
/// world directly (Quick Play). Reads the disk: call off the UI thread.
pub fn recent(profiles: &[InstanceProfile], limit: usize) -> Vec<RecentWorld> {
    let worlds = profiles
        .iter()
        .filter(|profile| {
            profile.installed_version_id.is_some()
                && orbmc_core::minecraft::quick_play::potentially_supported(&profile.game_version)
        })
        .flat_map(|profile| {
            let saves = profile.game_directory().join("saves");
            let worlds = worlds::saved(&saves).unwrap_or_else(|error| {
                tracing::warn!(profile_id = %profile.id, %error, "failed to read build worlds");
                Vec::new()
            });
            worlds.into_iter().map(|world| RecentWorld {
                profile: profile.id,
                folder: world.folder,
                last_played: world.last_played,
                icon: world.icon,
            })
        })
        .collect();
    newest_first(worlds, limit)
}

fn newest_first(mut worlds: Vec<RecentWorld>, limit: usize) -> Vec<RecentWorld> {
    worlds.sort_by_key(|world| std::cmp::Reverse(world.last_played));
    worlds.truncate(limit);
    worlds
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn newest_worlds_come_first_up_to_the_limit() {
        let world = |folder: &str, secs: u64| RecentWorld {
            profile: InstanceId::new(),
            folder: folder.into(),
            last_played: SystemTime::UNIX_EPOCH + Duration::from_secs(secs),
            icon: None,
        };
        let picked = newest_first(vec![world("old", 1), world("new", 3), world("mid", 2)], 2);
        let folders: Vec<_> = picked.iter().map(|world| world.folder.as_str()).collect();
        assert_eq!(folders, ["new", "mid"]);
    }
}
