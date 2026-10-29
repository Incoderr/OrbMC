use crate::{
    features::instances::InstanceProfile,
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
