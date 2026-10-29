//! Archive extraction helpers.

use std::{
    fs::{self, File},
    io::{self, Read},
    path::Path,
};

use zip::ZipArchive;

use crate::{io::paths::safe_join, Result};

/// Extracts a ZIP archive while rejecting entries that escape the destination.
///
/// # Errors
///
/// Returns [`crate::LauncherError`] if the archive cannot be read, a file cannot
/// be written, or an entry path is unsafe.
pub fn extract_zip_safely(zip_path: impl AsRef<Path>, destination: impl AsRef<Path>) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let destination = destination.as_ref();
    fs::create_dir_all(destination)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(enclosed) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            let unsafe_name = entry.name().to_string();
            safe_join(destination, unsafe_name)?;
            continue;
        };
        let output = safe_join(destination, enclosed)?;

        if entry.is_dir() {
            fs::create_dir_all(&output)?;
            continue;
        }

        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output_file = File::create(output)?;
        io::copy(&mut entry, &mut output_file)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    /// Builds a real zip so the tests exercise the actual ZipArchive code
    /// path, including the "stored" entry format Minecraft jars use.
    fn write_fixture(path: &Path) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("fabric.mod.json", options)
            .expect("start fabric.mod.json");
        zip.write_all(b"{\"name\":\"Example\"}").expect("write json");
        zip.start_file("ui/icon.png", options).expect("start icon");
        zip.write_all(b"icon bytes").expect("write icon");
        zip.start_file("META-INF/mods.toml", options)
            .expect("start toml");
        zip.write_all(b"modLoader=\"javafml\"").expect("write toml");
        zip.finish().expect("finish zip");
    }

    #[test]
    fn read_zip_entries_returns_the_requested_files_and_skips_missing_ones() {
        let directory = std::env::temp_dir()
            .join(format!("rs-mc-zip-entries-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("create fixture directory");
        let archive = directory.join("sample.jar");
        write_fixture(&archive);

        let found = read_zip_entries(
            &archive,
            &["fabric.mod.json", "ui/icon.png", "absent.json"],
        )
        .expect("read entries");

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, "fabric.mod.json");
        assert_eq!(found[0].1, b"{\"name\":\"Example\"}");
        assert_eq!(found[1].0, "ui/icon.png");
        assert_eq!(found[1].1, b"icon bytes");

        std::fs::remove_dir_all(&directory).expect("remove fixture directory");
    }

    #[test]
    fn read_zip_entries_falls_back_to_loosely_matching_names() {
        let directory = std::env::temp_dir()
            .join(format!("rs-mc-zip-loose-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("create fixture directory");
        let archive = directory.join("loose.jar");
        {
            let file = File::create(&archive).expect("create fixture jar");
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("Assets/Logo.PNG", options).expect("start file");
            zip.write_all(b"icon bytes").expect("write icon");
            zip.start_file("meta-inf/mods.toml", options).expect("start toml");
            zip.write_all(b"modLoader=\"javafml\"").expect("write toml");
            zip.finish().expect("finish zip");
        }

        let found =
            read_zip_entries(&archive, &["META-INF/mods.toml", "assets/logo.png", "logo.png"])
                .expect("read entries");

        // Case-insensitive full path, folder-insensitive and bare file name
        // requests all resolve to the same icon entry.
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].0, "meta-inf/mods.toml");
        assert_eq!(found[0].1, b"modLoader=\"javafml\"");
        assert_eq!(found[1].0, "Assets/Logo.PNG");
        assert_eq!(found[2].0, "Assets/Logo.PNG");
        assert_eq!(found[1].1, found[2].1);

        std::fs::remove_dir_all(&directory).expect("remove fixture directory");
    }
}

/// Reads the requested entries of a ZIP archive without extracting the rest.
///
/// Returns the entries found in the archive in the order of `names`, one
/// archive open per call. Missing names are simply skipped, so callers can
/// probe several candidates at once (e.g. the locator files of a mod jar).
///
/// Jar writers are inconsistent with character case and folder prefixes, so
/// a name without an exact match falls back to the first entry that equals it
/// case-insensitively in full — or, failing that, by the bare file name.
pub fn read_zip_entries(
    zip_path: impl AsRef<Path>,
    names: &[&str],
) -> Result<Vec<(String, Vec<u8>)>> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut found = Vec::new();
    for name in names {
        let file_name = name.rsplit(['/', '\\']).next().unwrap_or(name);
        // Step 1: resolve which stored entry the name refers to (immutable
        // scans only). Jar writers are inconsistent with character case and
        // folder prefixes, so fall back to the first entry matching the name
        // in full — or just its bare file name — case-insensitively.
        let resolved = if archive.file_names().any(|entry| entry == *name) {
            Some(name.to_string())
        } else {
            archive
                .file_names()
                .find(|entry| {
                    entry.eq_ignore_ascii_case(name)
                        || entry
                            .rsplit(['/', '\\'])
                            .next()
                            .is_some_and(|entry_name| {
                                entry_name.eq_ignore_ascii_case(file_name)
                            })
                })
                .map(str::to_owned)
        };
        let Some(resolved) = resolved else {
            continue;
        };
        // Step 2: read the resolved entry bytes.
        let Ok(mut entry) = archive.by_name(&resolved) else {
            continue;
        };
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        found.push((resolved, bytes));
    }
    Ok(found)
}
