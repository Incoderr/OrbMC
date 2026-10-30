//! authlib-injector: the Java agent that sends the game's auth and skin
//! requests to Ely.by instead of Mojang.

use std::io::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::ely::AuthError;
use crate::platform::profile_store;

const LATEST: &str = "https://authlib-injector.yushi.moe/artifact/latest.json";
const FILE_NAME: &str = "authlib-injector.jar";

#[derive(Deserialize)]
struct Artifact {
    download_url: String,
    checksums: Checksums,
}

#[derive(Deserialize)]
struct Checksums {
    sha256: String,
}

/// Path of the agent jar, downloading and verifying it on first use.
/// Blocks; call it from a worker thread.
pub fn authlib_injector() -> Result<PathBuf, AuthError> {
    let directory =
        profile_store::data_directory().map_err(|error| AuthError::Injector(error.to_string()))?;
    let path = directory.join(FILE_NAME);
    if path.is_file() {
        return Ok(path);
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("OrbMC/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .build()?;
    let artifact: Artifact = client.get(LATEST).send()?.error_for_status()?.json()?;
    let bytes = client
        .get(&artifact.download_url)
        .send()?
        .error_for_status()?
        .bytes()?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(artifact.checksums.sha256.trim()) {
        return Err(AuthError::Injector(
            "контрольная сумма файла не совпала".to_owned(),
        ));
    }

    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(&directory)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist(&path).map_err(|error| error.error)?;
        Ok(())
    };
    write().map_err(|error| AuthError::Injector(error.to_string()))?;
    tracing::info!(path = %path.display(), "authlib-injector downloaded");
    Ok(path)
}
