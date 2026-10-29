//! The launcher's own log: `<data>/logs/launcher.log`, with the previous
//! run kept as `launcher.old.log`. Without it `tracing` output goes nowhere.

use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;

use super::profile_store;

pub fn log_directory() -> io::Result<PathBuf> {
    Ok(profile_store::data_directory()?.join("logs"))
}

/// Starts writing `tracing` events to the log file. Failing to open the file
/// only loses the log; the launcher keeps working.
pub fn init() {
    let file = log_directory().and_then(|directory| {
        std::fs::create_dir_all(&directory)?;
        let current = directory.join("launcher.log");
        if current.exists() {
            std::fs::rename(&current, directory.join("launcher.old.log"))?;
        }
        File::create(current)
    });
    let file = match file {
        Ok(file) => file,
        Err(error) => {
            eprintln!("OrbMC: cannot open the log file: {error}");
            return;
        }
    };
    let result = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(if cfg!(debug_assertions) {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .with_writer(Mutex::new(file))
        .try_init();
    if let Err(error) = result {
        eprintln!("OrbMC: cannot start logging: {error}");
    }
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "OrbMC started");
}
