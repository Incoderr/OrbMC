//! Where builds live on disk: named game directories and the upgrade of
//! builds saved by older launcher versions.

use super::InstanceProfile;
use crate::platform::instance_layout;

/// Picks a free game directory name for `profile`: its build name, then
/// `name (1)`, `name (2)`, … when another build or a leftover folder already
/// uses it.
pub(super) fn allocate_directory(
    profiles: &[InstanceProfile],
    profile: &InstanceProfile,
) -> String {
    let root = instance_layout::instances_root(&profile.installation_root);
    instance_layout::unique_directory_name(&profile.name, |candidate| {
        let lowercase = candidate.to_lowercase();
        profiles.iter().any(|other| {
            other.id != profile.id
                && other.installation_root == profile.installation_root
                && other.directory.to_lowercase() == lowercase
        }) || root.join(candidate).exists()
    })
}

/// Upgrades builds saved by older launcher versions: id-named game
/// directories get the build name, and launcher files move out of the game
/// directory. Returns whether any profile changed and must be saved again.
pub fn migrate_layout(profiles: &mut [InstanceProfile]) -> bool {
    let mut changed = false;
    for index in 0..profiles.len() {
        let legacy = instance_layout::game_directory(
            &profiles[index].installation_root,
            &profiles[index].id.to_string(),
        );
        let unnamed = profiles[index].directory.is_empty();
        // A named build whose folder is missing may still sit in the
        // id-named folder (e.g. its root was relocated after naming).
        let stranded = !unnamed && !profiles[index].game_directory().exists() && legacy.is_dir();
        if unnamed || stranded {
            let directory = if unnamed {
                allocate_directory(profiles, &profiles[index])
            } else {
                profiles[index].directory.clone()
            };
            let profile = &mut profiles[index];
            let renamed = instance_layout::game_directory(&profile.installation_root, &directory);
            profile.directory = if !legacy.exists() {
                directory
            } else {
                match instance_layout::move_directory(&legacy, &renamed) {
                    Ok(()) => {
                        tracing::info!(profile_id = %profile.id, %directory, "renamed build directory");
                        directory
                    }
                    Err(error) => {
                        // Keep using the old folder; the build still works.
                        tracing::warn!(profile_id = %profile.id, %error, "failed to rename build directory");
                        profile.id.to_string()
                    }
                }
            };
            changed = true;
        }
        if let Err(error) = move_launcher_files(&profiles[index]) {
            tracing::warn!(
                profile_id = %profiles[index].id,
                %error,
                "failed to move launcher files out of the build directory"
            );
        }
    }
    changed
}

/// Moves launcher bookkeeping that older versions kept inside the game
/// directory into the build data directory.
fn move_launcher_files(profile: &InstanceProfile) -> std::io::Result<()> {
    let game = profile.game_directory();
    if !game.is_dir() {
        return Ok(());
    }
    let data = profile.data_directory()?;
    crate::features::mods::installed::migrate_legacy_index(&profile.mods_location()?)?;
    // Records of the removed local-skin mod setup; the jars stay in `mods`.
    for record in [
        game.join("mods").join(".rs-launcher-local-skin.json"),
        data.join("local-skin.json"),
    ] {
        instance_layout::remove_file(&record)?;
    }
    instance_layout::stash_game_caches(&game, &data);
    instance_layout::stash_jvm_logs(&game, &data);
    Ok(())
}
