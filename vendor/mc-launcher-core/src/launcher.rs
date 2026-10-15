//! High-level launcher facade.
//!
//! [`Launcher`] owns a Minecraft directory and coordinates the common workflow:
//! install a profile, load its merged version metadata, and build a Java
//! command from that metadata.

use std::path::{Path, PathBuf};

use crate::{
    command::builder::{build_launch_command, LaunchCommand, LaunchOptions},
    core::version::VersionJson,
    install::{
        client::{
            fetch_vanilla_version, install_version_files_controlled, load_version_json,
            write_version_json,
        },
        loader::{write_loader_profile, InstallerInvocation},
        request::{InstallRequest, InstallResult},
    },
    loader::{
        common::{LoaderSpec, LoaderVersion},
        LoaderKind,
    },
    net::{
        control::DownloadControl,
        download::{execute_plan_controlled, DownloadPlan, DownloadTask},
    },
    progress::{InstallStage, ProgressEvent, ProgressReporter},
    LauncherError, Result,
};

/// Facade for installing and launching Minecraft profiles inside one directory.
///
/// A `Launcher` is cheap to clone and only stores the root Minecraft directory.
/// The directory is expected to follow the standard launcher layout with
/// `versions`, `libraries`, `assets`, and `runtime` children as needed.
#[derive(Debug, Clone)]
pub struct Launcher {
    minecraft_dir: PathBuf,
}

impl Launcher {
    /// Creates a launcher rooted at the given Minecraft directory.
    ///
    /// The directory is created lazily by install operations.
    pub fn new(minecraft_dir: impl Into<PathBuf>) -> Self {
        Self {
            minecraft_dir: minecraft_dir.into(),
        }
    }

    /// Returns the Minecraft directory managed by this launcher.
    pub fn minecraft_dir(&self) -> &Path {
        &self.minecraft_dir
    }

    /// Installs a vanilla or loader-backed Minecraft profile.
    ///
    /// This is a convenience wrapper around [`Launcher::install_with_progress`]
    /// that ignores progress events.
    ///
    /// # Errors
    ///
    /// Returns [`LauncherError`] for network, filesystem, metadata, checksum, or
    /// loader-installer failures.
    pub fn install(&self, request: InstallRequest) -> Result<InstallResult> {
        let mut reporter = |_event: ProgressEvent| {};
        self.install_with_progress(request, &mut reporter)
    }

    /// Installs a profile and reports progress as tasks are processed.
    ///
    /// Vanilla, Fabric, and Quilt installs are handled with Rust-native
    /// metadata planning. Legacy Forge profiles are installed from their
    /// embedded version metadata; modern Forge and NeoForge installers run
    /// with `java`.
    ///
    /// The install cannot be paused or cancelled; see
    /// [`Launcher::install_with_java_progress`].
    ///
    /// # Errors
    ///
    /// Returns [`LauncherError`] for invalid versions, failed downloads,
    /// checksum mismatches, unsafe paths, or loader installer failures.
    pub fn install_with_progress(
        &self,
        request: InstallRequest,
        reporter: &mut dyn ProgressReporter,
    ) -> Result<InstallResult> {
        self.install_with_java_progress(request, None, reporter, &DownloadControl::new())
    }

    /// Installs a profile that can be paused and cancelled through `control`.
    ///
    /// Uses the supplied Java executable for loader installers.
    ///
    /// # Errors
    ///
    /// Returns [`LauncherError`] for invalid versions, failed downloads,
    /// checksum mismatches, unsafe paths, or loader installer failures, and
    /// [`LauncherError::InstallCancelled`] after cancellation.
    pub fn install_with_java_progress(
        &self,
        request: InstallRequest,
        java_executable: Option<PathBuf>,
        reporter: &mut dyn ProgressReporter,
        control: &DownloadControl,
    ) -> Result<InstallResult> {
        control.checkpoint()?;
        if let Some(loader) = request.loader.clone() {
            match loader {
                LoaderSpec::Fabric { version } => {
                    self.install_vanilla_version(&request.minecraft_version, reporter, control)?;
                    let loader_version = resolve_fabric_loader_version(version)?;
                    let profile = crate::loader::fabric::fetch_profile(
                        &request.minecraft_version,
                        &loader_version,
                    )?;
                    let version_id = version_id(&profile, "loader profile")?.to_string();
                    write_loader_profile(&self.minecraft_dir, &profile)?;
                    let merged = self.load_version(&version_id)?;
                    install_version_files_controlled(
                        &merged,
                        &self.minecraft_dir,
                        reporter,
                        control,
                    )?;
                    return Ok(InstallResult { version_id });
                }
                LoaderSpec::Quilt { version } => {
                    self.install_vanilla_version(&request.minecraft_version, reporter, control)?;
                    let loader_version = resolve_quilt_loader_version(version)?;
                    let profile = crate::loader::quilt::fetch_profile(
                        &request.minecraft_version,
                        &loader_version,
                    )?;
                    let version_id = version_id(&profile, "loader profile")?.to_string();
                    write_loader_profile(&self.minecraft_dir, &profile)?;
                    let merged = self.load_version(&version_id)?;
                    install_version_files_controlled(
                        &merged,
                        &self.minecraft_dir,
                        reporter,
                        control,
                    )?;
                    return Ok(InstallResult { version_id });
                }
                LoaderSpec::Forge { version } => {
                    self.install_vanilla_version(&request.minecraft_version, reporter, control)?;
                    let loader_version =
                        resolve_forge_loader_version(&request.minecraft_version, version)?;
                    let installer_path = download_forge_installer(
                        &self.minecraft_dir,
                        &request.minecraft_version,
                        &loader_version,
                        control,
                    )?;
                    if let Some(version_id) = crate::install::forge::install_legacy_forge_client(
                        &self.minecraft_dir,
                        &installer_path,
                        &request.minecraft_version,
                        reporter,
                        control,
                    )? {
                        return Ok(InstallResult { version_id });
                    }
                    run_loader_installer_controlled(
                        &InstallerInvocation {
                            loader: LoaderKind::Forge,
                            java_executable: java_executable
                                .clone()
                                .unwrap_or_else(|| PathBuf::from("java")),
                            installer_path,
                            minecraft_dir: self.minecraft_dir.clone(),
                        },
                        reporter,
                        control,
                    )?;
                    let version_id = find_installed_forge_version_id(
                        &self.minecraft_dir,
                        &request.minecraft_version,
                        &loader_version,
                    )?;
                    let merged = self.load_version(&version_id)?;
                    install_version_files_controlled(
                        &merged,
                        &self.minecraft_dir,
                        reporter,
                        control,
                    )?;
                    return Ok(InstallResult { version_id });
                }
                LoaderSpec::NeoForge { version } => {
                    self.install_vanilla_version(&request.minecraft_version, reporter, control)?;
                    let loader_version =
                        resolve_neoforge_loader_version(&request.minecraft_version, version)?;
                    let installer_path = download_installer(
                        &self.minecraft_dir,
                        "neoforge",
                        &loader_version,
                        &crate::loader::neoforge::installer_url(&loader_version),
                        control,
                    )?;
                    run_loader_installer_controlled(
                        &InstallerInvocation {
                            loader: LoaderKind::NeoForge,
                            java_executable: java_executable
                                .clone()
                                .unwrap_or_else(|| PathBuf::from("java")),
                            installer_path,
                            minecraft_dir: self.minecraft_dir.clone(),
                        },
                        reporter,
                        control,
                    )?;
                    let version_id = crate::loader::neoforge::neoforge_installed_version_id(
                        &request.minecraft_version,
                        &loader_version,
                    );
                    let merged = self.load_version(&version_id)?;
                    install_version_files_controlled(
                        &merged,
                        &self.minecraft_dir,
                        reporter,
                        control,
                    )?;
                    return Ok(InstallResult { version_id });
                }
            }
        }

        self.install_vanilla_version(&request.minecraft_version, reporter, control)?;
        Ok(InstallResult {
            version_id: request.minecraft_version,
        })
    }

    /// Builds a Java launch command from already-loaded version metadata.
    ///
    /// Call [`Launcher::load_version`] after installation to obtain merged
    /// metadata for profiles that inherit from a parent version.
    ///
    /// # Errors
    ///
    /// Returns [`LauncherError`] if required metadata is missing or cannot be
    /// converted into classpath and argument values.
    pub fn build_launch_command_from_version(
        &self,
        version: &VersionJson,
        options: LaunchOptions,
    ) -> Result<LaunchCommand> {
        build_launch_command(version, self.minecraft_dir.clone(), options)
    }

    /// Loads and merges a version JSON from `<minecraft_dir>/versions`.
    ///
    /// If the profile declares `inheritsFrom`, parent metadata is loaded and
    /// merged before the result is returned.
    ///
    /// # Errors
    ///
    /// Returns [`LauncherError`] if the profile or any parent cannot be read or
    /// parsed.
    pub fn load_version(&self, version_id: &str) -> Result<VersionJson> {
        load_version_json(&self.minecraft_dir, version_id)
    }

    fn install_vanilla_version(
        &self,
        version_id: &str,
        reporter: &mut dyn ProgressReporter,
        control: &DownloadControl,
    ) -> Result<()> {
        control.checkpoint()?;
        reporter.report(ProgressEvent::StageStarted {
            stage: InstallStage::ResolveVersion,
        });
        let version = fetch_vanilla_version(version_id)?;
        write_version_json(&self.minecraft_dir, &version)?;
        install_version_files_controlled(&version, &self.minecraft_dir, reporter, control)
    }
}

/// Runs a loader installer after a final cancel checkpoint.
fn run_loader_installer_controlled(
    invocation: &InstallerInvocation,
    reporter: &mut dyn ProgressReporter,
    control: &DownloadControl,
) -> Result<()> {
    control.checkpoint()?;
    reporter.report(ProgressEvent::StageStarted {
        stage: InstallStage::LoaderInstall,
    });
    crate::install::loader::run_loader_installer(invocation)
}

fn version_id<'a>(version: &'a VersionJson, context: &str) -> Result<&'a str> {
    version
        .id
        .as_deref()
        .ok_or_else(|| LauncherError::MissingField {
            context: context.to_string(),
            field: "id".to_string(),
        })
}
fn resolve_fabric_loader_version(version: LoaderVersion) -> Result<String> {
    match version {
        LoaderVersion::Exact(version) => Ok(version),
        LoaderVersion::Latest | LoaderVersion::LatestStable => {
            let versions = crate::loader::fabric::list_loader_versions()?;
            Ok(crate::loader::fabric::latest_stable_loader(&versions)?
                .version
                .clone())
        }
    }
}

fn resolve_quilt_loader_version(version: LoaderVersion) -> Result<String> {
    match version {
        LoaderVersion::Exact(version) => Ok(version),
        LoaderVersion::Latest | LoaderVersion::LatestStable => {
            let versions = crate::loader::quilt::list_loader_versions()?;
            Ok(crate::loader::quilt::latest_loader(&versions)?
                .version
                .clone())
        }
    }
}

fn resolve_forge_loader_version(minecraft_version: &str, version: LoaderVersion) -> Result<String> {
    match version {
        LoaderVersion::Exact(version) => Ok(version),
        LoaderVersion::Latest | LoaderVersion::LatestStable => {
            let versions = crate::loader::forge::list_forge_versions()?;
            Ok(
                crate::loader::forge::latest_for_minecraft(&versions, minecraft_version)?
                    .to_string(),
            )
        }
    }
}

fn resolve_neoforge_loader_version(
    minecraft_version: &str,
    version: LoaderVersion,
) -> Result<String> {
    match version {
        LoaderVersion::Exact(version) => Ok(version),
        LoaderVersion::Latest | LoaderVersion::LatestStable => {
            let versions = crate::loader::neoforge::list_neoforge_versions()?;
            Ok(
                crate::loader::neoforge::latest_for_minecraft(&versions, minecraft_version)?
                    .to_string(),
            )
        }
    }
}

fn download_installer(
    minecraft_dir: &Path,
    loader_name: &str,
    loader_version: &str,
    url: &str,
    control: &DownloadControl,
) -> Result<PathBuf> {
    let destination = minecraft_dir
        .join("versions")
        .join(".installers")
        .join(format!("{loader_name}-{loader_version}-installer.jar"));
    let plan = DownloadPlan {
        tasks: vec![DownloadTask {
            url: url.to_string(),
            destination: destination.clone(),
            checksum: None,
            size: None,
            label: format!("{loader_name} installer {loader_version}"),
        }],
    };
    let mut reporter = |_event: ProgressEvent| {};
    execute_plan_controlled(&plan, &mut reporter, control)?;
    Ok(destination)
}

fn download_forge_installer(
    minecraft_dir: &Path,
    minecraft_version: &str,
    forge_version: &str,
    control: &DownloadControl,
) -> Result<PathBuf> {
    let url = crate::loader::forge::installer_url(forge_version);
    match download_installer(minecraft_dir, "forge", forge_version, &url, control) {
        Ok(path) => Ok(path),
        Err(LauncherError::Network { source })
            if source.status().is_some_and(|status| status.as_u16() == 404) =>
        {
            let suffix = format!("-{minecraft_version}");
            if forge_version.ends_with(&suffix) {
                return Err(LauncherError::Network { source });
            }

            // Some older Forge builds publish Maven artifacts with the game
            // version repeated at the end of the Maven version coordinate.
            let maven_version = format!("{forge_version}{suffix}");
            let url = crate::loader::forge::installer_url(&maven_version);
            download_installer(minecraft_dir, "forge", forge_version, &url, control)
        }
        Err(error) => Err(error),
    }
}

fn find_installed_forge_version_id(
    minecraft_dir: &Path,
    minecraft_version: &str,
    forge_version: &str,
) -> Result<String> {
    let standard_id = crate::loader::forge::forge_installed_version_id(forge_version)?;
    let standard_json = minecraft_dir
        .join("versions")
        .join(&standard_id)
        .join(format!("{standard_id}.json"));
    if standard_json.is_file() {
        return Ok(standard_id);
    }

    let version_prefix = format!("{minecraft_version}-");
    let forge_revision = forge_version
        .strip_prefix(&version_prefix)
        .unwrap_or(forge_version)
        .to_ascii_lowercase();
    let minecraft_prefix = format!("{}-", minecraft_version.to_ascii_lowercase());
    let versions_dir = minecraft_dir.join("versions");
    for entry in std::fs::read_dir(&versions_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let version_id = entry.file_name().to_string_lossy().into_owned();
        let normalized_id = version_id.to_ascii_lowercase();
        if !normalized_id.starts_with(&minecraft_prefix)
            || !normalized_id.contains("forge")
            || !normalized_id.contains(&forge_revision)
        {
            continue;
        }

        let version_json_path = entry.path().join(format!("{version_id}.json"));
        let version_json: VersionJson = match std::fs::read(&version_json_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if version_json.id.as_deref() == Some(version_id.as_str()) {
            return Ok(version_id);
        }
    }

    Err(LauncherError::Other {
        message: format!(
            "Forge installer finished but did not create a version profile for Minecraft {minecraft_version} / Forge {forge_version}"
        ),
    })
}
