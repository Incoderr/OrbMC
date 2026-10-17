use crate::platform::{mod_files, profile_store};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    time::Duration,
};

const MANIFEST: &str = ".launcher-mods.json";
/// Suffix that turns a mod file invisible to the game and the scanner while
/// keeping it on disk (`mod.jar` → `mod.jar.disabled`).
const DISABLED_SUFFIX: &str = ".disabled";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Record {
    filename: String,
    sha1: String,
    projects: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Inventory {
    pub projects: HashSet<String>,
    pub warning: Option<String>,
}

#[derive(Debug, Default)]
pub struct InstalledState {
    inventories: HashMap<uuid::Uuid, Inventory>,
    generations: HashMap<uuid::Uuid, u64>,
    loading: HashSet<uuid::Uuid>,
    /// Builds (project ids) whose download is currently in flight.
    pending: HashSet<String>,
}

impl InstalledState {
    pub fn contains(&self, profile: uuid::Uuid, project: &str) -> bool {
        self.inventories
            .get(&profile)
            .is_some_and(|inventory| inventory.projects.contains(project))
    }
    /// Marks a build's download as in flight. Only the item that owns the
    /// download changes appearance; other list items stay untouched.
    pub fn begin_install(&mut self, project: &str) {
        self.pending.insert(project.to_owned());
    }
    pub fn finish_install(&mut self, project: &str) {
        self.pending.remove(project);
    }
    pub fn is_installing(&self, project: &str) -> bool {
        self.pending.contains(project)
    }
    pub fn begin(&mut self, profile: uuid::Uuid) -> u64 {
        let generation = self.generations.entry(profile).or_default();
        *generation = generation.wrapping_add(1);
        self.loading.insert(profile);
        *generation
    }
    pub fn finish(&mut self, profile: uuid::Uuid, generation: u64, inventory: Inventory) {
        if self.generations.get(&profile) == Some(&generation) {
            self.inventories.insert(profile, inventory);
            self.loading.remove(&profile);
        }
    }
    pub fn mark_installed(&mut self, profile: uuid::Uuid, project: String) {
        self.begin(profile); // Invalidate any scan started before the download completed.
        self.loading.remove(&profile);
        self.inventories
            .entry(profile)
            .or_default()
            .projects
            .insert(project);
    }
    pub fn is_loading(&self, profile: uuid::Uuid) -> bool {
        self.loading.contains(&profile)
    }
    pub fn warning(&self, profile: uuid::Uuid) -> Option<&str> {
        self.inventories
            .get(&profile)
            .and_then(|inventory| inventory.warning.as_deref())
    }
}

/// Every project id ever recorded for the build. Used to skip already
/// installed projects when resolving dependencies.
pub fn recorded_projects(directory: &Path) -> HashSet<String> {
    match mod_files::read_json::<Vec<Record>>(&directory.join(MANIFEST)) {
        Ok(records) => records
            .into_iter()
            .flat_map(|record| record.projects)
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "failed to read mod installation records");
            HashSet::new()
        }
    }
}

/// One recorded mod file of a build, as shown on the build page.
#[derive(Clone, Debug)]
pub struct InstalledModEntry {
    pub filename: String,
    pub projects: Vec<String>,
    pub disabled: bool,
}

/// Wrapper of `record.projects` for listing entries.
fn read_records(directory: &Path) -> Vec<Record> {
    mod_files::read_json(&directory.join(MANIFEST)).unwrap_or_default()
}

/// Lists every recorded mod file plus its on-disk enabled state.
pub fn list_entries(directory: &Path) -> Vec<InstalledModEntry> {
    let mut entries: Vec<InstalledModEntry> = read_records(directory)
        .into_iter()
        .map(|record| InstalledModEntry {
            filename: record.filename.clone(),
            projects: record.projects,
            disabled: is_disabled(directory, &record.filename),
        })
        .collect();
    entries.sort_by(|left, right| left.filename.cmp(&right.filename));
    entries.dedup_by(|left, right| left.filename == right.filename);
    entries
}

/// Whether `filename` currently sits with the `.disabled` suffix on disk.
fn is_disabled(directory: &Path, filename: &str) -> bool {
    !directory.join(filename).exists()
        && directory
            .join(format!("{filename}{DISABLED_SUFFIX}"))
            .exists()
}

/// Enables or disables a mod file by renaming it on disk. The game and the
/// library scanner only pick up plain `.jar` files, so a `.jar.disabled`
/// file stays available for re-enabling without any download.
pub fn set_disabled(directory: &Path, filename: &str, disabled: bool) -> Result<(), String> {
    let enabled_path = directory.join(filename);
    let disabled_path = directory.join(format!("{filename}{DISABLED_SUFFIX}"));
    let (from, to) = if disabled {
        (enabled_path, disabled_path)
    } else {
        (disabled_path, enabled_path)
    };
    if from == to || !from.exists() {
        return Err(format!("Мод файл не найден: {filename}"));
    }
    if to.exists() {
        return Err(format!("Целевой файл уже существует: {}", to.display()));
    }
    std::fs::rename(&from, &to).map_err(|error| {
        tracing::warn!(%filename, %error, "failed to toggle mod file state");
        format!("Не удалось изменить состояние мода: {error}")
    })
}

pub fn record_install(directory: &Path, project: String, filename: &str) -> Result<(), String> {
    let sha1 = mc_launcher_core::io::hash::sha1_file(directory.join(filename))
        .map_err(|e| e.to_string())?;
    let mut records: Vec<Record> =
        mod_files::read_json(&directory.join(MANIFEST)).map_err(|e| e.to_string())?;
    records.retain(|record| record.filename != filename);
    records.push(Record {
        filename: filename.into(),
        sha1,
        projects: vec![project],
    });
    profile_store::write_json_atomically(&directory.join(MANIFEST), &records)
        .map_err(|e| e.to_string())
}

/// Deletes every mod file linked to `project` from the mods directory and
/// removes their records from the manifest. Files that fail to delete (for
/// example, a running game keeps a lock on them) stay in the manifest.
/// Returns the removed filenames.
pub fn uninstall(directory: &Path, project: &str) -> Result<Vec<String>, String> {
    let records: Vec<Record> =
        mod_files::read_json(&directory.join(MANIFEST)).map_err(|e| e.to_string())?;
    let mut removed = Vec::new();
    let mut kept = Vec::with_capacity(records.len());
    let mut failure = None;
    for record in records {
        if !record.projects.iter().any(|id| id == project) {
            kept.push(record);
            continue;
        }
        match remove_record_file(directory, &record.filename) {
            Ok(()) => removed.push(record.filename),
            Err(error) => {
                if failure.is_none() {
                    failure = Some(format!("Не удалось удалить файл {}", record.filename));
                }
                tracing::warn!(filename = %record.filename, %error, "failed to delete mod file");
                kept.push(record);
            }
        }
    }
    if !removed.is_empty() {
        profile_store::write_json_atomically(&directory.join(MANIFEST), &kept)
            .map_err(|e| e.to_string())?;
    }
    match failure {
        Some(message) => Err(message),
        None => Ok(removed),
    }
}

/// Filenames of disabled mods in `directory`, without the `.disabled` suffix.
fn disabled_filenames(directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !entry.file_type().ok()?.is_file() {
                return None;
            }
            let name = entry.file_name().to_str()?.to_owned();
            name.strip_suffix(DISABLED_SUFFIX).map(str::to_owned)
        })
        .collect()
}

/// Deletes the file of a record: a plain `.jar` or its `.disabled` variant.
/// A missing file is not an error — the record just stops referring to it.
fn remove_record_file(directory: &Path, filename: &str) -> Result<(), String> {
    let plain = directory.join(filename);
    match std::fs::remove_file(&plain) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let disabled = directory.join(format!("{filename}{DISABLED_SUFFIX}"));
            match std::fs::remove_file(&disabled) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.to_string()),
            }
        }
        Err(error) => Err(error.to_string()),
    }
}

pub fn scan(directory: &Path, api_key: Option<&str>) -> Inventory {
    let files = match mod_files::scan(directory) {
        Ok(files) => files,
        Err(error) => {
            return Inventory {
                warning: Some(format!("Не удалось проверить моды: {error}")),
                ..Default::default()
            };
        }
    };
    if files.is_empty() && disabled_filenames(directory).is_empty() {
        return Inventory::default();
    }
    let mut warnings = Vec::new();
    let cached: Vec<Record> = match mod_files::read_json(&directory.join(MANIFEST)) {
        Ok(records) => records,
        Err(error) => {
            warnings.push(format!("Не удалось прочитать индекс модов: {error}"));
            Vec::new()
        }
    };
    let mut records = reconcile(&files, &cached);
    // Disabled mods are skipped by the scanner but stay installed: keep
    // their records so re-enabling restores a fully identified file.
    let disabled = disabled_filenames(directory);
    for record in &cached {
        if disabled.iter().any(|filename| filename == &record.filename)
            && !records.iter().any(|kept| kept.filename == record.filename)
        {
            records.push(record.clone());
        }
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("rs-mc-launcher/0.1")
        .build();
    if let Ok(client) = client {
        // Hashes are file identities; filenames and save paths never leave the computer.
        let hashes: Vec<_> = records
            .iter()
            .filter(|r| !r.projects.iter().any(|id| id.starts_with("modrinth:")))
            .map(|r| r.sha1.clone())
            .collect();
        for hashes in hashes.chunks(100) {
            let result = client
                .post("https://api.modrinth.com/v2/version_files")
                .header("Content-Type", "application/json")
                .body(serde_json::json!({"hashes":hashes,"algorithm":"sha1"}).to_string())
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.text());
            match result {
                Ok(body) => match serde_json::from_str::<HashMap<String, IdentifiedVersion>>(&body)
                {
                    Ok(matches) => {
                        for record in &mut records {
                            if let Some(version) = matches.get(&record.sha1) {
                                record
                                    .projects
                                    .push(format!("modrinth:{}", version.project_id));
                            }
                        }
                    }
                    Err(error) => warnings.push(format!("Modrinth: {error}")),
                },
                Err(error) => warnings.push(format!("Modrinth: {error}")),
            }
        }
        if let Some(api_key) = api_key.filter(|key| !key.is_empty()) {
            let fingerprints: Vec<_> = files.iter().map(|file| file.fingerprint).collect();
            for batch in fingerprints.chunks(100) {
                let result = client
                    .post("https://api.curseforge.com/v1/fingerprints/432")
                    .header("x-api-key", api_key)
                    .header("Content-Type", "application/json")
                    .body(serde_json::json!({"fingerprints":batch}).to_string())
                    .send()
                    .and_then(|r| r.error_for_status())
                    .and_then(|r| r.text());
                match result {
                    Ok(body) => match serde_json::from_str::<FingerprintResponse>(&body) {
                        Ok(response) => {
                            for found in response.data.exact_matches {
                                for (file, record) in files.iter().zip(&mut records) {
                                    // SHA-1 guards against 32-bit fingerprint collisions.
                                    if found.file.hashes.iter().any(|hash| {
                                        hash.algo == 1
                                            && hash.value.eq_ignore_ascii_case(&file.sha1)
                                    }) {
                                        record
                                            .projects
                                            .push(format!("curseforge:{}", found.file.mod_id));
                                    }
                                }
                            }
                        }
                        Err(error) => warnings.push(format!("CurseForge: {error}")),
                    },
                    Err(error) => warnings.push(format!("CurseForge: {error}")),
                }
            }
        }
    } else {
        warnings.push("Не удалось создать клиент для определения модов".into());
    }
    for record in &mut records {
        record.projects.sort();
        record.projects.dedup();
    }
    if let Err(error) = profile_store::write_json_atomically(&directory.join(MANIFEST), &records) {
        warnings.push(format!("Не удалось сохранить индекс модов: {error}"));
    }
    let unknown = records
        .iter()
        .filter(|record| record.projects.is_empty())
        .count();
    if unknown > 0 {
        warnings.push(format!("Не удалось определить {unknown} файлов"));
    }
    Inventory {
        projects: records
            .into_iter()
            .flat_map(|record| record.projects)
            .collect(),
        warning: (!warnings.is_empty()).then(|| warnings.join("; ")),
    }
}

#[derive(Deserialize)]
struct IdentifiedVersion {
    project_id: String,
}
#[derive(Deserialize)]
struct FingerprintResponse {
    data: FingerprintData,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FingerprintData {
    exact_matches: Vec<FingerprintMatch>,
}
#[derive(Deserialize)]
struct FingerprintMatch {
    file: FingerprintFile,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FingerprintFile {
    mod_id: u64,
    hashes: Vec<FileHash>,
}
#[derive(Deserialize)]
struct FileHash {
    algo: u8,
    value: String,
}

fn reconcile(files: &[mod_files::ModFile], cached: &[Record]) -> Vec<Record> {
    files
        .iter()
        .map(|file| {
            let projects = cached
                .iter()
                .find(|record| record.filename == file.filename && record.sha1 == file.sha1)
                .map(|record| record.projects.clone())
                .unwrap_or_default();
            Record {
                filename: file.filename.clone(),
                sha1: file.sha1.clone(),
                projects,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installation_survives_restart_but_removed_and_changed_files_are_not_installed() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("example.jar");
        std::fs::write(&file, b"first version").unwrap();
        record_install(directory.path(), "modrinth:example".into(), "example.jar").unwrap();
        let cached: Vec<Record> = mod_files::read_json(&directory.path().join(MANIFEST)).unwrap();
        let scanned = mod_files::scan(directory.path()).unwrap();
        assert_eq!(
            reconcile(&scanned, &cached)[0].projects,
            ["modrinth:example"]
        );
        std::fs::write(&file, b"unrelated replacement").unwrap();
        assert!(
            reconcile(&mod_files::scan(directory.path()).unwrap(), &cached)[0]
                .projects
                .is_empty()
        );
        std::fs::remove_file(file).unwrap();
        assert!(reconcile(&mod_files::scan(directory.path()).unwrap(), &cached).is_empty());
    }
    #[test]
    fn late_scans_cannot_erase_a_new_install_and_profiles_remain_independent() {
        let first = uuid::Uuid::new_v4();
        let second = uuid::Uuid::new_v4();
        let mut state = InstalledState::default();
        let old = state.begin(first);
        state.mark_installed(first, "modrinth:example".into());
        state.finish(first, old, Inventory::default());
        assert!(state.contains(first, "modrinth:example"));
        assert!(!state.contains(second, "modrinth:example"));
    }

    #[test]
    fn disabling_keeps_the_record_and_reenabling_restores_the_file() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.jar"), b"a").unwrap();
        record_install(directory.path(), "modrinth:a".into(), "a.jar").unwrap();

        // Disable: the .jar disappears, the .disabled variant appears and
        // list_entries reports the mod as disabled.
        set_disabled(directory.path(), "a.jar", true).unwrap();
        assert!(!directory.path().join("a.jar").exists());
        assert!(directory.path().join("a.jar.disabled").exists());
        let entries = list_entries(directory.path());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].disabled);
        assert_eq!(entries[0].projects, vec!["modrinth:a".to_owned()]);

        // A scan must keep the record of the disabled mod: the project stays
        // "installed" (disabled is a file state, not an uninstall).
        let scanned = scan(directory.path(), None);
        assert!(scanned.projects.contains("modrinth:a"));
        let records: Vec<Record> = mod_files::read_json(&directory.path().join(MANIFEST)).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].filename, "a.jar");
        assert_eq!(records[0].projects, vec!["modrinth:a".to_owned()]);

        // Re-enable: the file is back with the original name.
        set_disabled(directory.path(), "a.jar", false).unwrap();
        assert!(directory.path().join("a.jar").exists());
        let scanned = scan(directory.path(), None);
        assert!(scanned.projects.contains("modrinth:a"));
    }

    #[test]
    fn uninstall_removes_both_enabled_and_disabled_variants() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.jar"), b"a").unwrap();
        record_install(directory.path(), "modrinth:a".into(), "a.jar").unwrap();
        set_disabled(directory.path(), "a.jar", true).unwrap();
        uninstall(directory.path(), "modrinth:a").unwrap();
        assert!(!directory.path().join("a.jar.disabled").exists());
        let records: Vec<Record> = mod_files::read_json(&directory.path().join(MANIFEST)).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn recorded_projects_lists_every_project_of_a_build() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.jar"), b"a").unwrap();
        record_install(directory.path(), "modrinth:a".into(), "a.jar").unwrap();
        std::fs::write(directory.path().join("b.jar"), b"b").unwrap();
        record_install(directory.path(), "curseforge:2".into(), "b.jar").unwrap();
        let projects = recorded_projects(directory.path());
        assert_eq!(
            projects,
            HashSet::from(["modrinth:a".to_owned(), "curseforge:2".to_owned()])
        );
    }

    #[test]
    fn uninstall_removes_only_the_requested_project() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.jar"), b"a").unwrap();
        std::fs::write(directory.path().join("b.jar"), b"b").unwrap();
        record_install(directory.path(), "modrinth:a".into(), "a.jar").unwrap();
        record_install(directory.path(), "modrinth:b".into(), "b.jar").unwrap();

        let removed = uninstall(directory.path(), "modrinth:a").unwrap();

        assert_eq!(removed, ["a.jar"]);
        assert!(!directory.path().join("a.jar").exists());
        assert!(directory.path().join("b.jar").exists());
        let records: Vec<Record> = mod_files::read_json(&directory.path().join(MANIFEST)).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].projects, ["modrinth:b"]);
    }
}
