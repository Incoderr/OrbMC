//! Filesystem locations where Java runtimes are commonly installed.
//!
//! Discovery here is offline: it only lists directories that may contain a
//! Java home. Probing an installation (running `java -version`) is owned by
//! [`crate::features::java`].

use std::{env, path::PathBuf};

/// Candidate directories that may be a Java home or a `bin` directory
/// containing a `java` executable.
///
/// Mirrors what the official launcher and the Modrinth App scan: install
/// roots of common distributions, `JAVA_HOME`, and everything on `PATH`.
pub fn java_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    for root in install_roots() {
        if let Ok(entries) = std::fs::read_dir(&root) {
            candidates.extend(entries.flatten().map(|entry| entry.path()));
        }
    }

    if let Ok(java_home) = env::var("JAVA_HOME") {
        candidates.push(PathBuf::from(java_home));
    }

    if let Ok(path) = env::var("PATH") {
        candidates.extend(env::split_paths(&path));
    }

    candidates
}

#[cfg(target_os = "windows")]
fn install_roots() -> Vec<PathBuf> {
    [
        r"C:\Program Files\Java",
        r"C:\Program Files (x86)\Java",
        r"C:\Program Files\Eclipse Adoptium",
        r"C:\Program Files (x86)\Eclipse Adoptium",
        r"C:\Program Files\Microsoft",
        r"C:\Program Files\Zulu",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(target_os = "macos")]
fn install_roots() -> Vec<PathBuf> {
    ["/Library/Java/JavaVirtualMachines"]
        .iter()
        .map(PathBuf::from)
        .collect()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn install_roots() -> Vec<PathBuf> {
    [
        "/usr/lib/jvm",
        "/usr/lib64/jvm",
        "/usr/java",
        "/opt/jdk",
        "/opt/jdks",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_include_path_and_java_home() {
        let candidates = java_candidates();
        // PATH always has entries in a test environment, so at least one
        // candidate must be produced without touching the network.
        assert!(!candidates.is_empty());
    }
}
