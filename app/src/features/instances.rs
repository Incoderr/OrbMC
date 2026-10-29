//! Builds (Minecraft instances): what they are, where they live, their
//! runtime state, the worker threads behind them and their screens.

pub mod game_log;
mod layout;
mod model;
mod service;
mod state;
pub mod view;
mod worker;

pub use layout::migrate_layout;
pub use model::*;
pub use service::InstanceEvent;
pub use state::*;
pub use view::{BuildsScreen, InstancesView, InstancesViewEvent};
pub use worker::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::versions::LoaderChoice;
    use std::path::PathBuf;

    fn profile(name: &str) -> InstanceProfile {
        InstanceProfile::new(
            name.into(),
            "1.21".into(),
            LoaderChoice::Vanilla,
            None,
            PathBuf::new(),
        )
    }

    #[test]
    fn builds_with_the_same_name_get_numbered_directories() {
        let root = tempfile::tempdir().unwrap();
        let mut state = InstancesState::default();
        for _ in 0..3 {
            let mut build = profile("Fabric 1.21");
            build.installation_root = root.path().into();
            state.add(build);
        }
        let directories: Vec<_> = state
            .profiles()
            .iter()
            .map(|build| build.directory.as_str())
            .collect();
        assert_eq!(
            directories,
            ["Fabric 1.21", "Fabric 1.21 (1)", "Fabric 1.21 (2)"]
        );
    }

    #[test]
    fn migration_renames_id_named_directories() {
        let root = tempfile::tempdir().unwrap();
        let mut build = profile("Old: build");
        build.installation_root = root.path().into();
        let legacy = root.path().join("instances").join(build.id.to_string());
        std::fs::create_dir_all(legacy.join("saves")).unwrap();
        let mut profiles = vec![build];

        assert!(migrate_layout(&mut profiles));

        assert_eq!(profiles[0].directory, "Old_ build");
        assert!(!legacy.exists());
        assert!(profiles[0].game_directory().join("saves").is_dir());
    }

    #[test]
    fn migration_recovers_named_builds_left_in_id_folders() {
        let root = tempfile::tempdir().unwrap();
        let mut build = profile("Named");
        build.installation_root = root.path().into();
        build.directory = "Named".into();
        let legacy = root.path().join("instances").join(build.id.to_string());
        std::fs::create_dir_all(legacy.join("saves")).unwrap();
        let mut profiles = vec![build];

        assert!(migrate_layout(&mut profiles));

        assert_eq!(profiles[0].directory, "Named");
        assert!(!legacy.exists());
        assert!(profiles[0].game_directory().join("saves").is_dir());
    }

    #[test]
    fn successful_launches_sort_home_without_reordering_library() {
        let first = profile("First");
        let second = profile("Second");
        let first_id = first.id;
        let second_id = second.id;
        let mut state = InstancesState::from_profiles(vec![first, second]);
        state.mark_launching(second_id);
        state.mark_failed(second_id, "Java missing".into());
        assert_eq!(state.recent_profiles()[0].id, first_id);
        state.mark_running(second_id, 42);
        assert_eq!(state.recent_profiles()[0].id, second_id);
        assert_eq!(state.profiles()[0].id, first_id);
        let saved = serde_json::to_string(state.profiles()).unwrap();
        let mut restored = InstancesState::from_profiles(serde_json::from_str(&saved).unwrap());
        assert_eq!(restored.recent_profiles()[0].id, second_id);
        restored.mark_running(first_id, 43);
        assert_eq!(restored.recent_profiles()[0].id, first_id);
        restored.remove(first_id);
        assert_eq!(restored.recent_profiles()[0].id, second_id);
    }

    #[test]
    fn old_profiles_keep_their_order_until_launched() {
        let mut old = serde_json::to_value(profile("Old")).unwrap();
        old.as_object_mut().unwrap().remove("last_launch_order");
        let old: InstanceProfile = serde_json::from_value(old).unwrap();
        assert_eq!(old.last_launch_order, 0);
        let state = InstancesState::from_profiles(vec![old, profile("New")]);
        assert_eq!(state.recent_profiles()[0].name, "Old");
    }

    #[test]
    fn player_names_follow_vanilla_rules() {
        assert!(is_valid_player_name("Player"));
        assert!(is_valid_player_name("a_b_999"));
        assert!(is_valid_player_name(" Steve "));
        assert!(is_valid_player_name("abc"));
        assert!(is_valid_player_name("0123456789abcdef"));
        assert!(!is_valid_player_name("ab"));
        assert!(!is_valid_player_name("0123456789abcdefg"));
        assert!(!is_valid_player_name(""));
        assert!(!is_valid_player_name("И в нике"));
        assert!(!is_valid_player_name("with-dash"));
    }
}
