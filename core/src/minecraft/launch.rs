//! Final adjustments to the launch command built by the install engine.

use std::path::Path;

/// Smallest and largest heap a build may ask for, in MiB.
pub const MEMORY_LIMITS_MB: (u32, u32) = (512, 16_384);

/// Puts the launcher's JVM options in front of the engine's arguments.
///
/// The heap size follows the build setting (`-Xmx` = `memory_mb`, `-Xms` a
/// quarter of it within 512–1024 MiB) and replaces any heap flags the engine
/// produced. JVM crash dumps are written to `crash_logs` instead of the game
/// directory. `jvm_options` (the authlib-injector agent) follow the
/// launcher's own options, then the user's `custom_options` from the build
/// settings, all in front of the classpath and main class. The JVM takes the
/// last value of a repeated flag, so a custom `-Xmx` wins over the build's
/// memory setting. `extra` (Quick Play) goes last.
pub fn launch_arguments(
    engine_arguments: &[String],
    memory_mb: u32,
    crash_logs: &Path,
    jvm_options: Vec<String>,
    custom_options: Vec<String>,
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
    arguments.extend(jvm_options);
    arguments.extend(custom_options);
    arguments.extend(
        engine_arguments
            .iter()
            .filter(|argument| !argument.starts_with("-Xms") && !argument.starts_with("-Xmx"))
            .cloned(),
    );
    arguments.extend(extra);
    arguments
}

/// Splits JVM options typed by the user into arguments: whitespace separates
/// them, double quotes keep spaces inside one (`-Dpath="C:/My Games"`).
/// An unclosed quote runs to the end of the text.
pub fn split_arguments(text: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut in_argument = false;
    let mut quoted = false;
    for character in text.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                in_argument = true;
            }
            character if character.is_whitespace() && !quoted => {
                if in_argument {
                    arguments.push(std::mem::take(&mut current));
                    in_argument = false;
                }
            }
            character => {
                current.push(character);
                in_argument = true;
            }
        }
    }
    if in_argument {
        arguments.push(current);
    }
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
            vec!["-javaagent:injector.jar=ely.by".into()],
            vec!["-XX:+UseG1GC".into()],
            vec!["--quickPlaySingleplayer".into()],
        );
        assert_eq!(arguments[0], "-Xms1024M");
        assert_eq!(arguments[1], "-Xmx8192M");
        assert!(arguments[2].starts_with("-XX:ErrorFile=logs"));
        assert!(!arguments.contains(&"-Xmx2G".to_owned()));
        assert_eq!(
            &arguments[4..],
            [
                "-javaagent:injector.jar=ely.by",
                "-XX:+UseG1GC",
                "-cp",
                "client.jar",
                "--quickPlaySingleplayer"
            ]
        );
    }

    #[test]
    fn memory_is_clamped_to_the_limits() {
        let low = launch_arguments(&[], 100, Path::new("l"), vec![], vec![], vec![]);
        assert_eq!(&low[..2], ["-Xms512M", "-Xmx512M"]);
        let high = launch_arguments(&[], 1_000_000, Path::new("l"), vec![], vec![], vec![]);
        assert_eq!(high[1], "-Xmx16384M");
    }

    #[test]
    fn custom_options_split_on_whitespace_and_keep_quoted_spaces() {
        assert_eq!(
            split_arguments(
                "  -XX:+UseG1GC	-Dfile.encoding=UTF-8
"
            ),
            ["-XX:+UseG1GC", "-Dfile.encoding=UTF-8"]
        );
        assert_eq!(
            split_arguments(r#"-Dpath="C:/My Games/mc" "-Dname=a b""#),
            ["-Dpath=C:/My Games/mc", "-Dname=a b"]
        );
        assert_eq!(split_arguments(r#"-Dempty="""#), ["-Dempty="]);
        assert_eq!(split_arguments(r#""""#), [""]);
        assert!(split_arguments("   ").is_empty());
        assert_eq!(split_arguments(r#"-Dx="open end"#), ["-Dx=open end"]);
    }
}
