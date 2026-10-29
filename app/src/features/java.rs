//! Java runtime resolution for launching and installing Minecraft.
//!
//! The flow mirrors the Modrinth App: read the required Java major version
//! from version metadata, probe installed runtimes, and auto-download an
//! official Mojang runtime when nothing suitable is installed. The rules
//! themselves live in [`orbmc_core::minecraft::java`].

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

use mc_launcher_core::{core::version::VersionJson, runtime, types::CallbackDict};
pub use orbmc_core::minecraft::java::JavaRequirement;
use orbmc_core::minecraft::java::{RuntimeMatch, parse_major_version, pick_runtime};

use crate::platform;

/// Reads the Java requirement from version metadata.
pub fn requirement_for(version: &VersionJson) -> JavaRequirement {
    JavaRequirement::from_metadata(
        version
            .java_version
            .as_ref()
            .map(|java| i64::from(java.major_version)),
        version.main_class.as_deref(),
    )
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

/// Why no usable Java runtime could be chosen. The messages are shown to the
/// user as is.
#[derive(Debug, thiserror::Error)]
pub enum JavaError {
    #[error(
        "Не удалось определить версию Java по пути {}. Проверь выбор Java в настройках профиля.",
        path.display()
    )]
    UnreadableCustom { path: PathBuf },
    #[error(
        "Требуется {}, а выбрана Java {} ({}). Обнови Java или выбери подходящую в настройках профиля.",
        requirement.label(),
        found.major_version,
        found.version
    )]
    Mismatch {
        requirement: JavaRequirement,
        found: JavaProbe,
    },
    #[error(
        "Требуется {}, но подходящая Java не найдена{}: {reason}. Установи Java {} и укажи путь к java.exe в настройках профиля.",
        requirement.label(),
        found_list(found),
        requirement.minimum_major_version
    )]
    DownloadFailed {
        requirement: JavaRequirement,
        /// Major versions of the runtimes that were found but do not fit.
        found: Vec<u32>,
        reason: String,
    },
    #[error("Скачанная Java {major_version} не запускается: не удалось определить версию.")]
    BrokenDownload { major_version: u32 },
}

fn found_list(found: &[u32]) -> String {
    if found.is_empty() {
        return String::new();
    }
    let found = found
        .iter()
        .map(|major| format!("Java {major}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(" (найдены: {found})")
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

/// Chooses a Java runtime that satisfies `requirement`.
///
/// An explicitly configured `custom_executable` is honored as long as it can
/// be probed and satisfies the requirement. Otherwise an installed runtime of
/// exactly the declared major version is used; when there is none, the
/// official Mojang runtime for that version (a JRE, the same one the official
/// launcher uses) is downloaded into the launcher's `runtime` directory. A
/// newer compatible runtime is only the fallback when that download fails.
///
/// `on_status` receives progress text for long downloads.
pub fn resolve_java(
    custom_executable: Option<&Path>,
    requirement: &JavaRequirement,
    minecraft_directory: &Path,
    mut on_status: impl FnMut(String),
) -> Result<JavaResolution, JavaError> {
    if let Some(executable) = custom_executable {
        let Some(probe) = probe(executable) else {
            return Err(JavaError::UnreadableCustom {
                path: executable.to_path_buf(),
            });
        };
        if !requirement.is_satisfied_by(probe.major_version) {
            return Err(JavaError::Mismatch {
                requirement: *requirement,
                found: probe,
            });
        }
        return Ok(JavaResolution {
            executable: executable.to_path_buf(),
            probe,
        });
    }

    let probes = installed_probes(minecraft_directory);
    if let Some(matched) = pick_runtime(&probes, requirement, RuntimeMatch::Exact, describe) {
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
        Err(reason) => {
            if let Some(fallback) =
                pick_runtime(&probes, requirement, RuntimeMatch::Compatible, describe)
            {
                tracing::warn!(
                    %reason,
                    java = %fallback.version,
                    "Java download failed; using a newer installed runtime"
                );
                return Ok(JavaResolution {
                    executable: fallback.executable.clone(),
                    probe: fallback.clone(),
                });
            }
            return Err(JavaError::DownloadFailed {
                requirement: *requirement,
                found: probes.iter().map(|probe| probe.major_version).collect(),
                reason,
            });
        }
    };
    let Some(probe) = probe(&executable) else {
        return Err(JavaError::BrokenDownload { major_version });
    };
    tracing::info!(java = %probe.version, "installed Java runtime");
    Ok(JavaResolution { executable, probe })
}

fn describe(probe: &JavaProbe) -> (u32, &str) {
    (probe.major_version, probe.version.as_str())
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn requirement_reads_version_metadata() {
        let version: VersionJson = serde_json::from_value(json!({
            "id": "1.6.4-forge",
            "javaVersion": { "component": "jre-legacy", "majorVersion": 8 },
            "mainClass": "net.minecraft.launchwrapper.Launch"
        }))
        .expect("valid version json");
        let requirement = requirement_for(&version);
        assert_eq!(requirement.label(), "Java 8");
        assert!(!requirement.is_satisfied_by(17));
    }

    #[test]
    fn mismatch_message_names_both_versions() {
        let error = JavaError::Mismatch {
            requirement: JavaRequirement::from_metadata(Some(21), None),
            found: JavaProbe {
                executable: PathBuf::from("java"),
                version: "17.0.2".into(),
                major_version: 17,
            },
        };
        let message = error.to_string();
        assert!(message.contains("Java 21+"));
        assert!(message.contains("Java 17 (17.0.2)"));
    }

    #[test]
    fn download_failure_lists_found_runtimes() {
        let error = JavaError::DownloadFailed {
            requirement: JavaRequirement::from_metadata(Some(25), None),
            found: vec![17, 21],
            reason: "offline".into(),
        };
        assert!(
            error
                .to_string()
                .contains("(найдены: Java 17, Java 21): offline")
        );
    }
}
