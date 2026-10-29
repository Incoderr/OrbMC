//! Final adjustments to the launch command built by the install engine.

use std::path::Path;

/// Smallest and largest heap a build may ask for, in MiB.
pub const MEMORY_LIMITS_MB: (u32, u32) = (512, 16_384);

/// Puts the launcher's JVM options in front of the engine's arguments.
///
/// The heap size follows the build setting (`-Xmx` = `memory_mb`, `-Xms` a
/// quarter of it within 512–1024 MiB) and replaces any heap flags the engine
/// produced. JVM crash dumps are written to `crash_logs` instead of the game
/// directory. `extra` (Quick Play) goes last.
pub fn launch_arguments(
    engine_arguments: &[String],
    memory_mb: u32,
    crash_logs: &Path,
    extra: Vec<String>,
) -> Vec<String> {
    let max_memory_mb = memory_mb.clamp(MEMORY_LIMITS_MB.0, MEMORY_LIMITS_MB.1);
    let initial_memory_mb = (max_memory_mb / 4).clamp(512, 1024);
    let mut arguments = vec![
        format!("-Xms{initial_memory_mb}M"),
        format!("-Xmx{max_memory_mb}M"),
        format!(
            "-XX:ErrorFile={}",
            crash_logs.join("hs_err_pid%p.log").display()
        ),
        format!(
            "-XX:ReplayDataFile={}",
            crash_logs.join("replay_pid%p.log").display()
        ),
    ];
    arguments.extend(
        engine_arguments
            .iter()
            .filter(|argument| !argument.starts_with("-Xms") && !argument.starts_with("-Xmx"))
            .cloned(),
    );
    arguments.extend(extra);
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_flags_follow_the_build_setting() {
        let engine = vec![
            "-Xmx2G".to_owned(),
            "-cp".to_owned(),
            "client.jar".to_owned(),
        ];
        let arguments = launch_arguments(
            &engine,
            8192,
            Path::new("logs"),
            vec!["--quickPlaySingleplayer".into()],
        );
        assert_eq!(arguments[0], "-Xms1024M");
        assert_eq!(arguments[1], "-Xmx8192M");
        assert!(arguments[2].starts_with("-XX:ErrorFile=logs"));
        assert!(!arguments.contains(&"-Xmx2G".to_owned()));
        assert_eq!(
            &arguments[4..],
            ["-cp", "client.jar", "--quickPlaySingleplayer"]
        );
    }

    #[test]
    fn memory_is_clamped_to_the_limits() {
        let low = launch_arguments(&[], 100, Path::new("l"), Vec::new());
        assert_eq!(&low[..2], ["-Xms512M", "-Xmx512M"]);
        let high = launch_arguments(&[], 1_000_000, Path::new("l"), Vec::new());
        assert_eq!(high[1], "-Xmx16384M");
    }
}
