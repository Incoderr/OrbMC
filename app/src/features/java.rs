//! Java runtime resolution for launching and installing Minecraft.
//!
//! The flow mirrors the Modrinth App: read the required Java major version
//! from version metadata, probe installed runtimes, and auto-download an
//! official Mojang runtime when nothing suitable is installed.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

use mc_launcher_core::{core::version::VersionJson, runtime, types::CallbackDict};

use crate::platform;

/// Major version assumed when version metadata does not declare one. Matches
/// Mojang's convention for legacy versions.
pub const DEFAULT_JAVA_MAJOR_VERSION: u32 = 8;

/// Main class of the legacy mod loader that only works on Java 8.
const LAUNCHWRAPPER_MAIN_CLASS: &str = "net.minecraft.launchwrapper.Launch";

/// Which Java a Minecraft version needs.
#[derive(Clone, Copy, Debug)]
pub struct JavaRequirement {
    /// Minimum supported major version.
    pub minimum_major_version: u32,
    /// Inclusive upper bound for legacy main classes that break on newer JVMs.
    pub maximum_major_version: Option<u32>,
}

impl JavaRequirement {
    /// Reads the requirement from version metadata.
    pub fn from_version(version: &VersionJson) -> Self {
        let minimum_major_version = version
            .java_version
            .as_ref()
            .map_or(DEFAULT_JAVA_MAJOR_VERSION, |java| {
                u32::try_from(java.major_version).unwrap_or(DEFAULT_JAVA_MAJOR_VERSION)
            });
        let maximum_major_version = (version.main_class.as_deref()
            == Some(LAUNCHWRAPPER_MAIN_CLASS))
        .then_some(DEFAULT_JAVA_MAJOR_VERSION);
        Self {
            minimum_major_version,
            maximum_major_version,
        }
    }

    /// Human-readable label, such as `Java 21+` or `Java 8`.
    pub fn label(&self) -> String {
        match self.maximum_major_version {
            Some(maximum) => format!("Java {maximum}"),
            None => format!("Java {}+", self.minimum_major_version),
        }
    }

    /// Whether a runtime with this major version can run the version.
    pub fn is_satisfied_by(&self, major_version: u32) -> bool {
        major_version >= self.minimum_major_version
            && self
                .maximum_major_version
                .is_none_or(|maximum| major_version <= maximum)
    }
}

/// A verified local Java runtime.
#[derive(Clone, Debug)]
pub struct JavaProbe {
    pub executable: PathBuf,
    /// Full version string, such as `21.0.8`.
    pub version: String,
    pub major_version: u32,
}

/// The runtime chosen for a launch or install.
#[derive(Clone, Debug)]
pub struct JavaResolution {
    pub executable: PathBuf,
    pub probe: JavaProbe,
}

/// Probes a `java` executable and parses its version.
pub fn probe(executable: &Path) -> Option<JavaProbe> {
    let mut command = Command::new(executable);
    command.arg("-version");
    crate::platform::process::hide_console_window(&mut command);
    let output = command.output().ok()?;
    let version = [
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    ]
    .into_iter()
    .find_map(|stream| {
        stream
            .lines()
            .find_map(|line| line.split('"').nth(1).map(str::to_owned))
    })?;
    let major_version = parse_major_version(&version)?;
    Some(JavaProbe {
        executable: executable.to_path_buf(),
        version,
        major_version,
    })
}

/// Extracts the major version from a `java -version` string such as `21.0.8`
/// or the legacy `1.8.0_412`.
pub fn parse_major_version(version: &str) -> Option<u32> {
    let mut components = version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|component| !component.is_empty())
        .filter_map(|component| component.parse::<u32>().ok());
    let first = components.next()?;
    if first == 1 {
        components.next()
    } else {
        Some(first)
    }
}

/// Chooses a Java runtime that satisfies `requirement`.
///
/// An explicitly configured `custom_executable` is honored as long as it can
/// be probed and satisfies the requirement. Otherwise installed runtimes are
/// probed and, when none fits, an official Mojang runtime for the required
/// major version is downloaded into the launcher's `runtime` directory.
///
/// `on_status` receives progress text for long downloads.
pub fn resolve_java(
    custom_executable: Option<&Path>,
    requirement: &JavaRequirement,
    minecraft_directory: &Path,
    mut on_status: impl FnMut(String),
) -> Result<JavaResolution, String> {
    if let Some(executable) = custom_executable {
        let Some(probe) = probe(executable) else {
            return Err(format!(
                "Не удалось определить версию Java по пути {}. Проверь выбор Java в настройках профиля.",
                executable.display()
            ));
        };
        if !requirement.is_satisfied_by(probe.major_version) {
            return Err(mismatch_message(requirement, &probe));
        }
        return Ok(JavaResolution {
            executable: executable.to_path_buf(),
            probe,
        });
    }

    let probes = installed_probes(minecraft_directory);
    if let Some(matched) = pick_probe(&probes, requirement) {
        tracing::info!(
            java = %matched.version,
            path = %matched.executable.display(),
            "selected Java runtime"
        );
        return Ok(JavaResolution {
            executable: matched.executable.clone(),
            probe: matched.clone(),
        });
    }

    let major_version = requirement.minimum_major_version;
    on_status(format!("Скачиваем Java {major_version}…"));
    tracing::info!(
        major = major_version,
        "no compatible Java runtime found; downloading Mojang runtime"
    );
    let executable = match download_runtime(major_version, minecraft_directory) {
        Ok(executable) => executable,
        Err(error) => {
            let found = probes
                .iter()
                .map(|probe| format!("Java {}", probe.major_version))
                .collect::<Vec<_>>()
                .join(", ");
            let found = if found.is_empty() {
                String::new()
            } else {
                format!(" (найдены: {found})")
            };
            return Err(format!(
                "Требуется {}, но подходящая Java не найдена{found}: {error}. Установи Java {major_version} и укажи путь к java.exe в настройках профиля.",
                requirement.label()
            ));
        }
    };
    let Some(probe) = probe(&executable) else {
        return Err(format!(
            "Скачанная Java {major_version} не запускается: не удалось определить версию."
        ));
    };
    tracing::info!(java = %probe.version, "installed Java runtime");
    Ok(JavaResolution { executable, probe })
}

/// Probes every locally present Java runtime: runtimes managed by the launcher
/// plus system installations.
fn installed_probes(minecraft_directory: &Path) -> Vec<JavaProbe> {
    let mut executables = BTreeSet::new();

    for component in runtime::get_installed_jvm_runtimes(minecraft_directory) {
        if let Some(executable) = runtime::get_executable_path(&component, minecraft_directory) {
            executables.insert(executable);
        }
    }
    for candidate in platform::java::java_candidates() {
        if let Some(executable) = java_executable_in(&candidate) {
            executables.insert(executable);
        }
    }

    executables
        .iter()
        .filter_map(|executable| probe(executable))
        .collect()
}

/// Resolves the `java` executable inside a candidate directory. The candidate
/// may be a Java home (`bin/java`), a `bin` directory (`java`), or a macOS
/// bundle (`Contents/Home/bin/java`).
fn java_executable_in(directory: &Path) -> Option<PathBuf> {
    let executable = if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    };
    [
        directory.join("bin").join(executable),
        directory.join(executable),
        directory
            .join("Contents")
            .join("Home")
            .join("bin")
            .join(executable),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

/// Prefers an exact major match, then the closest newer runtime; among equals
/// the highest version string wins.
fn pick_probe<'a>(probes: &'a [JavaProbe], requirement: &JavaRequirement) -> Option<&'a JavaProbe> {
    probes
        .iter()
        .filter(|probe| requirement.is_satisfied_by(probe.major_version))
        .min_by_key(|probe| {
            (
                probe
                    .major_version
                    .abs_diff(requirement.minimum_major_version),
                std::cmp::Reverse(&probe.version),
            )
        })
}

/// Downloads and installs the official Mojang runtime for a major version
/// into `<minecraft_directory>/runtime` and returns the Java executable.
fn download_runtime(major_version: u32, minecraft_directory: &Path) -> Result<PathBuf, String> {
    let component = runtime::find_jvm_component_for_major(major_version)
        .map_err(|error| format!("не удалось получить список рантаймов Mojang: {error}"))?
        .ok_or_else(|| format!("Mojang не публикует Java {major_version} для этой платформы"))?;
    runtime::install_jvm_runtime(&component, minecraft_directory, &CallbackDict::default())
        .map_err(|error| format!("не удалось установить {component}: {error}"))?;
    runtime::get_executable_path(&component, minecraft_directory)
        .ok_or_else(|| format!("{component} установлен, но исполняемый файл Java не найден"))
}

fn mismatch_message(requirement: &JavaRequirement, probe: &JavaProbe) -> String {
    format!(
        "Требуется {}, а выбрана Java {} ({}). Обнови Java или выбери подходящую в настройках профиля.",
        requirement.label(),
        probe.major_version,
        probe.version
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn requirement_from(value: serde_json::Value) -> JavaRequirement {
        let version: VersionJson = serde_json::from_value(value).expect("valid version json");
        JavaRequirement::from_version(&version)
    }

    #[test]
    fn modern_version_requires_metadata_java() {
        let requirement = requirement_from(json!({
            "id": "26.3",
            "javaVersion": { "component": "java-runtime-x", "majorVersion": 25 }
        }));
        assert_eq!(requirement.minimum_major_version, 25);
        assert_eq!(requirement.label(), "Java 25+");
        assert!(requirement.is_satisfied_by(25));
        assert!(requirement.is_satisfied_by(26));
        assert!(!requirement.is_satisfied_by(21));
    }

    #[test]
    fn legacy_version_defaults_to_java_8() {
        let requirement = requirement_from(json!({ "id": "1.8.9" }));
        assert_eq!(
            requirement.minimum_major_version,
            DEFAULT_JAVA_MAJOR_VERSION
        );
        assert!(requirement.is_satisfied_by(21));
    }

    #[test]
    fn launchwrapper_is_capped_at_java_8() {
        let requirement = requirement_from(json!({
            "id": "1.6.4-forge",
            "mainClass": "net.minecraft.launchwrapper.Launch"
        }));
        assert_eq!(requirement.label(), "Java 8");
        assert!(requirement.is_satisfied_by(8));
        assert!(!requirement.is_satisfied_by(9));
        assert!(!requirement.is_satisfied_by(21));
    }

    #[test]
    fn parse_major_version_handles_modern_and_legacy() {
        assert_eq!(parse_major_version("21.0.8"), Some(21));
        assert_eq!(parse_major_version("25"), Some(25));
        assert_eq!(parse_major_version("1.8.0_412"), Some(8));
        assert_eq!(parse_major_version(""), None);
    }

    #[test]
    fn pick_probe_prefers_exact_major() {
        let probes = vec![
            probe_with("17.0.2", 17),
            probe_with("21.0.8", 21),
            probe_with("25.0.1", 25),
        ];
        let requirement = JavaRequirement {
            minimum_major_version: 25,
            maximum_major_version: None,
        };
        assert_eq!(
            pick_probe(&probes, &requirement).map(|probe| &probe.version),
            Some(&"25.0.1".to_owned())
        );
    }

    #[test]
    fn pick_probe_falls_back_to_closest_newer() {
        let probes = vec![probe_with("17.0.2", 17), probe_with("21.0.8", 21)];
        let requirement = JavaRequirement {
            minimum_major_version: 21,
            maximum_major_version: None,
        };
        assert_eq!(
            pick_probe(&probes, &requirement).map(|probe| &probe.version),
            Some(&"21.0.8".to_owned())
        );
    }

    #[test]
    fn pick_probe_keeps_highest_legacy_runtime() {
        let probes = vec![probe_with("8u402", 8), probe_with("8u282", 8)];
        let requirement = JavaRequirement {
            minimum_major_version: 8,
            maximum_major_version: Some(8),
        };
        assert_eq!(
            pick_probe(&probes, &requirement).map(|probe| &probe.version),
            Some(&"8u402".to_owned())
        );
    }

    #[test]
    fn pick_probe_rejects_incompatible() {
        let probes = vec![probe_with("21.0.8", 21)];
        let requirement = JavaRequirement {
            minimum_major_version: 25,
            maximum_major_version: None,
        };
        assert!(pick_probe(&probes, &requirement).is_none());
    }

    fn probe_with(version: &str, major_version: u32) -> JavaProbe {
        JavaProbe {
            executable: PathBuf::from(format!("java-{major_version}")),
            version: version.to_owned(),
            major_version,
        }
    }
}
