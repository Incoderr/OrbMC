//! Launcher updates from GitHub Releases.
//!
//! The latest release is compared with the running version. A release can be
//! installed in place when this build knows the update public key and the
//! release carries the executable with its minisign signature; the update is
//! verified before it replaces the launcher and applies on the next start.
//! Otherwise the launcher only points to the release page.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::http_client::{AsyncBody, HttpClient, HttpRequestExt, RedirectPolicy, Request};
use gpui_kit::*;
use serde::Deserialize;

use super::downloads::{DownloadError, FileDownload, Transfer, TransferHandle, download_file};
use super::instances::InstancesState;
use super::settings::SettingsState;
use crate::platform::self_update;

const RELEASES_API: &str = "https://api.github.com/repos/Incoderr/OrbMC/releases/latest";
/// Public key (minisign) that release executables are signed with, baked in
/// at build time. Builds without it never replace themselves.
const UPDATE_PUBLIC_KEY: Option<&str> = option_env!("ORBMC_UPDATE_PUBLIC_KEY");
const MAX_EXECUTABLE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_SIGNATURE_BYTES: usize = 4 * 1024;
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(5);
const RECHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// `major.minor.patch` of a launcher release.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u64, u64, u64);

impl Version {
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Self(0, 0, 0))
    }

    /// Reads `v1.2.3` or `1.2`; pre-release suffixes (`-beta`) are dropped.
    fn parse(text: &str) -> Option<Self> {
        let text = text.trim().trim_start_matches(['v', 'V']);
        let text = text.split(['-', '+']).next()?;
        let mut parts = text.split('.').map(str::parse::<u64>);
        let major = parts.next()?.ok()?;
        let minor = parts.next().unwrap_or(Ok(0)).ok()?;
        let patch = parts.next().unwrap_or(Ok(0)).ok()?;
        parts.next().is_none().then_some(Self(major, minor, patch))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// A file attached to a release.
#[derive(Clone, Debug, PartialEq)]
struct Asset {
    url: String,
    size: u64,
}

/// A release newer than the running launcher.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: Version,
    /// Release page with the notes, for builds that cannot update in place.
    pub page_url: String,
    executable: Option<Asset>,
    signature: Option<Asset>,
}

impl Release {
    /// Whether this build can download, verify and install the release.
    pub fn installable(&self) -> bool {
        update_public_key().is_some()
            && !cfg!(debug_assertions)
            && self.executable.is_some()
            && self.signature.is_some()
    }
}

fn update_public_key() -> Option<&'static str> {
    UPDATE_PUBLIC_KEY
        .map(str::trim)
        .filter(|key| !key.is_empty())
}

#[derive(Clone, Debug, Default)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading {
        version: Version,
        transfer: TransferHandle,
    },
    /// The update replaced the executable; it runs from the next start.
    Ready {
        version: Version,
        executable: PathBuf,
    },
    Failed(String),
}

/// Why checking or installing an update failed. Messages are shown to the user.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Нет связи с GitHub: {0}")]
    Network(String),
    #[error("GitHub ответил HTTP {0}")]
    Http(u16),
    #[error("Непонятный ответ GitHub: {0}")]
    InvalidResponse(String),
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error("Подпись обновления не прошла проверку, файл удалён")]
    Signature,
    #[error("Не удалось заменить лаунчер: {0}")]
    Install(std::io::Error),
}

/// Checks for and installs launcher updates. Shared by the settings page and
/// the sidebar.
pub struct Updater {
    status: UpdateStatus,
    settings: Entity<SettingsState>,
    instances: Entity<InstancesState>,
    _schedule: Task<()>,
}

impl Updater {
    pub fn new(
        settings: Entity<SettingsState>,
        instances: Entity<InstancesState>,
        cx: &mut Context<Self>,
    ) -> Self {
        // Checks shortly after start and then a few times a day.
        let schedule = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FIRST_CHECK_DELAY).await;
            loop {
                if this
                    .update(cx, |this, cx| this.check_automatically(cx))
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(RECHECK_INTERVAL).await;
            }
        });
        Self {
            status: UpdateStatus::Idle,
            settings,
            instances,
            _schedule: schedule,
        }
    }

    pub fn status(&self) -> &UpdateStatus {
        &self.status
    }

    fn set_status(&mut self, status: UpdateStatus, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    fn check_automatically(&mut self, cx: &mut Context<Self>) {
        if self.settings.read(cx).auto_update()
            && matches!(
                self.status,
                UpdateStatus::Idle | UpdateStatus::UpToDate | UpdateStatus::Failed(_)
            )
        {
            self.check(true, cx);
        }
    }

    /// Looks for a newer release; with `install` an installable one is
    /// downloaded right away.
    pub fn check(&mut self, install: bool, cx: &mut Context<Self>) {
        if matches!(
            self.status,
            UpdateStatus::Checking | UpdateStatus::Downloading { .. } | UpdateStatus::Ready { .. }
        ) {
            return;
        }
        self.set_status(UpdateStatus::Checking, cx);
        let client = cx.http_client();
        cx.spawn(async move |this, cx| {
            let result = fetch_latest(client).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(Some(release)) => {
                    tracing::info!(version = %release.version, "launcher update available");
                    let installable = release.installable();
                    this.set_status(UpdateStatus::Available(release), cx);
                    if install && installable {
                        this.install(cx);
                    }
                }
                Ok(None) => this.set_status(UpdateStatus::UpToDate, cx),
                Err(error) => {
                    tracing::warn!(%error, "update check failed");
                    this.set_status(UpdateStatus::Failed(error.to_string()), cx);
                }
            });
        })
        .detach();
    }

    /// Downloads, verifies and installs the available release.
    pub fn install(&mut self, cx: &mut Context<Self>) {
        let UpdateStatus::Available(release) = &self.status else {
            return;
        };
        let (Some(public_key), Some(executable_asset), Some(signature_asset)) = (
            update_public_key(),
            release.executable.clone(),
            release.signature.clone(),
        ) else {
            return;
        };
        if !release.installable() {
            return;
        }
        let executable = match self_update::current_executable() {
            Ok(executable) => executable,
            Err(error) => {
                self.set_status(
                    UpdateStatus::Failed(UpdateError::Install(error).to_string()),
                    cx,
                );
                return;
            }
        };
        let version = release.version;
        let transfer = TransferHandle::default();
        self.set_status(
            UpdateStatus::Downloading {
                version,
                transfer: transfer.clone(),
            },
            cx,
        );

        let client = cx.http_client();
        let transfer = Transfer {
            handle: transfer,
            executor: cx.background_executor().clone(),
        };
        let target = executable.clone();
        let task = cx.background_executor().spawn(async move {
            download_and_install(
                client,
                &executable_asset,
                &signature_asset,
                public_key,
                &target,
                &transfer,
            )
            .await
        });
        // Progress lives in the transfer handle; redraw while it moves.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let downloading = this
                    .update(cx, |this, cx| {
                        cx.notify();
                        matches!(this.status, UpdateStatus::Downloading { .. })
                    })
                    .unwrap_or(false);
                if !downloading {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(()) => {
                    tracing::info!(%version, "launcher update installed");
                    this.set_status(
                        UpdateStatus::Ready {
                            version,
                            executable,
                        },
                        cx,
                    );
                }
                Err(error) => {
                    tracing::warn!(%version, %error, "launcher update failed");
                    this.set_status(UpdateStatus::Failed(error.to_string()), cx);
                }
            });
        })
        .detach();
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if let UpdateStatus::Downloading { transfer, .. } = &self.status {
            transfer.cancel();
            cx.notify();
        }
    }

    /// Restarting would cut off downloads and lose track of running games.
    pub fn can_restart(&self, cx: &App) -> bool {
        let instances = self.instances.read(cx);
        !instances
            .profiles()
            .iter()
            .any(|profile| instances.runtime(&profile.id).is_busy())
    }

    /// Starts the installed update and quits this launcher.
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        let UpdateStatus::Ready { executable, .. } = &self.status else {
            return;
        };
        if !self.can_restart(cx) {
            return;
        }
        match self_update::restart(executable) {
            Ok(()) => cx.quit(),
            Err(error) => {
                tracing::error!(%error, "failed to start the updated launcher");
                self.set_status(
                    UpdateStatus::Failed(UpdateError::Install(error).to_string()),
                    cx,
                );
            }
        }
    }
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// The release if it is newer than `current` and meant for everyone.
fn newer_release(release: GitHubRelease, current: Version) -> Option<Release> {
    if release.draft || release.prerelease {
        return None;
    }
    let version = Version::parse(&release.tag_name)?;
    if version <= current {
        return None;
    }
    let asset = |name: &str| {
        release
            .assets
            .iter()
            .find(|asset| asset.name.eq_ignore_ascii_case(name))
            .filter(|asset| asset.browser_download_url.starts_with("https://"))
            .map(|asset| Asset {
                url: asset.browser_download_url.clone(),
                size: asset.size,
            })
    };
    let executable_name = self_update::RELEASE_ASSET;
    Some(Release {
        version,
        page_url: release.html_url.clone(),
        executable: executable_name.and_then(asset),
        signature: executable_name.and_then(|name| asset(&format!("{name}.minisig"))),
    })
}

async fn fetch_latest(client: Arc<dyn HttpClient>) -> Result<Option<Release>, UpdateError> {
    let request = Request::builder()
        .uri(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        // GitHub answers renamed or moved repositories with a redirect.
        .follow_redirects(RedirectPolicy::FollowAll)
        .body(AsyncBody::empty())
        .map_err(|error| UpdateError::Network(error.to_string()))?;
    let response = client
        .send(request)
        .await
        .map_err(|error| UpdateError::Network(error.to_string()))?;
    let status = response.status();
    // No published releases yet.
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(UpdateError::Http(status.as_u16()));
    }
    let mut body = Vec::new();
    futures::io::AsyncReadExt::read_to_end(&mut response.into_body(), &mut body)
        .await
        .map_err(|error| UpdateError::Network(error.to_string()))?;
    let release: GitHubRelease = serde_json::from_slice(&body)
        .map_err(|error| UpdateError::InvalidResponse(error.to_string()))?;
    Ok(newer_release(release, Version::current()))
}

async fn fetch_signature(client: &Arc<dyn HttpClient>, url: &str) -> Result<String, UpdateError> {
    let request = Request::builder()
        .uri(url)
        .timeout(Duration::from_secs(20))
        .follow_redirects(RedirectPolicy::FollowAll)
        .body(AsyncBody::empty())
        .map_err(|error| UpdateError::Network(error.to_string()))?;
    let response = client
        .send(request)
        .await
        .map_err(|error| UpdateError::Network(error.to_string()))?;
    if !response.status().is_success() {
        return Err(UpdateError::Http(response.status().as_u16()));
    }
    let mut body = Vec::new();
    futures::io::AsyncReadExt::read_to_end(&mut response.into_body(), &mut body)
        .await
        .map_err(|error| UpdateError::Network(error.to_string()))?;
    if body.len() > MAX_SIGNATURE_BYTES {
        return Err(UpdateError::Signature);
    }
    String::from_utf8(body).map_err(|_| UpdateError::Signature)
}

async fn download_and_install(
    client: Arc<dyn HttpClient>,
    executable_asset: &Asset,
    signature_asset: &Asset,
    public_key: &str,
    executable: &Path,
    transfer: &Transfer,
) -> Result<(), UpdateError> {
    let signature = fetch_signature(&client, &signature_asset.url).await?;
    let staging = self_update::staging_path(executable);
    download_file(
        client,
        FileDownload {
            url: &executable_asset.url,
            destination: &staging,
            size: Some(executable_asset.size),
            sha1: None,
            max_size: MAX_EXECUTABLE_BYTES,
        },
        transfer,
    )
    .await?;
    let verified = std::fs::read(&staging)
        .map_err(UpdateError::Install)
        .and_then(|bytes| verify_signature(&bytes, &signature, public_key));
    if let Err(error) = verified {
        let _ = std::fs::remove_file(&staging);
        return Err(error);
    }
    self_update::install(executable, &staging).map_err(|error| {
        let _ = std::fs::remove_file(&staging);
        UpdateError::Install(error)
    })
}

/// Checks a minisign signature. The key may be the whole `.pub` file or just
/// its base64 line.
fn verify_signature(bytes: &[u8], signature: &str, public_key: &str) -> Result<(), UpdateError> {
    let key = minisign_verify::PublicKey::decode(public_key)
        .or_else(|_| minisign_verify::PublicKey::from_base64(public_key))
        .map_err(|error| {
            tracing::error!(%error, "the built-in update key is invalid");
            UpdateError::Signature
        })?;
    let signature =
        minisign_verify::Signature::decode(signature).map_err(|_| UpdateError::Signature)?;
    key.verify(bytes, &signature, false)
        .map_err(|_| UpdateError::Signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn versions_parse_and_compare() {
        assert_eq!(Version::parse("v1.2.3"), Some(Version(1, 2, 3)));
        assert_eq!(Version::parse("0.4"), Some(Version(0, 4, 0)));
        assert_eq!(Version::parse("2.0.0-beta.1"), Some(Version(2, 0, 0)));
        assert_eq!(Version::parse("nightly"), None);
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert!(Version(0, 10, 0) > Version(0, 9, 9));
    }

    fn github_release(tag: &str) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag.into(),
            html_url: "https://github.com/Incoderr/OrbMC/releases/tag/v1".into(),
            draft: false,
            prerelease: false,
            assets: ["orbmc.exe", "orbmc.exe.minisig"]
                .into_iter()
                .map(|name| GitHubAsset {
                    name: name.into(),
                    browser_download_url: format!("https://example.com/{name}"),
                    size: 10,
                })
                .collect(),
        }
    }

    #[test]
    fn only_newer_public_releases_are_offered() {
        let current = Version(0, 2, 0);
        assert!(newer_release(github_release("v0.2.0"), current).is_none());
        assert!(newer_release(github_release("v0.1.9"), current).is_none());
        let mut prerelease = github_release("v0.3.0");
        prerelease.prerelease = true;
        assert!(newer_release(prerelease, current).is_none());

        let release = newer_release(github_release("v0.3.0"), current).unwrap();
        assert_eq!(release.version, Version(0, 3, 0));
        if self_update::RELEASE_ASSET.is_some() {
            assert_eq!(
                release.signature.map(|asset| asset.url),
                Some("https://example.com/orbmc.exe.minisig".to_owned())
            );
        }
    }

    // Made with `rsign generate -W` and `rsign sign -W`, as the release
    // workflow does.
    const TEST_PUBLIC_KEY: &str = "RWR7vKcO0OqoNLlGh8o2CONV8jPBK9YUQU1gkQiq/GPoXqYfxYRiVd7d";
    const TEST_SIGNATURE: &str = "untrusted comment: signature from rsign secret key
RUR7vKcO0OqoNB9+/Zg8m1/Uvtewf8zWnObC7iWbIpMU8iIBhAf7bqC9Erckno6bS6YIpX7B9Ns8DYA0DuMkbMohJP1gj4wQDwU=
trusted comment: OrbMC v9
J93vwTHkkZ03aryFN5w31xFQkzGT33khsvSd7BWFAXvwvTe2Te1scsuPcfStoxInu7CLs1bXxC82tupCMuvXCA==
";

    #[test]
    fn signatures_are_checked() {
        let signed = b"hello update";
        assert!(verify_signature(signed, TEST_SIGNATURE, TEST_PUBLIC_KEY).is_ok());
        let key_file = format!(
            "untrusted comment: minisign public key
{TEST_PUBLIC_KEY}"
        );
        assert!(verify_signature(signed, TEST_SIGNATURE, &key_file).is_ok());
        assert!(verify_signature(b"tampered", TEST_SIGNATURE, TEST_PUBLIC_KEY).is_err());
        assert!(verify_signature(signed, "garbage", TEST_PUBLIC_KEY).is_err());
    }
}
