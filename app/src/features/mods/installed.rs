use crate::platform::{mod_files, profile_store};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};

/// Mod index of a build, inside its launcher data directory.
const MANIFEST: &str = "mods.json";
/// Where older launcher versions kept the index: inside the mods directory.
const LEGACY_MANIFEST: &str = ".launcher-mods.json";
/// Suffix that turns a mod file invisible to the game and the scanner while
/// keeping it on disk (`mod.jar` → `mod.jar.disabled`).
const DISABLED_SUFFIX: &str = ".disabled";
/// Directory with icons extracted from mod jars, inside the build data
/// directory; the manifest stores icon paths relative to that directory.
const ICON_CACHE: &str = "mod-icons";
const LEGACY_ICON_CACHE: &str = ".launcher-icons";

/// Where a build keeps its mod jars, and where the launcher keeps its own
/// index of them. The index lives outside the game directory so the build
/// folder only holds what the game itself uses.
#[derive(Clone, Debug)]
pub struct ModsLocation {
    pub mods: PathBuf,
    pub index: PathBuf,
}

impl ModsLocation {
    fn manifest(&self) -> PathBuf {
        self.index.join(MANIFEST)
    }
}

/// Moves an index and icon cache left inside the mods directory by older
/// launcher versions into the build data directory.
pub fn migrate_legacy_index(location: &ModsLocation) -> std::io::Result<()> {
    use crate::platform::instance_layout;

    let legacy_icons = location.mods.join(LEGACY_ICON_CACHE);
    if legacy_icons.is_dir() {
        let icons = location.index.join(ICON_CACHE);
        std::fs::create_dir_all(&icons)?;
        for entry in std::fs::read_dir(&legacy_icons)? {
            let entry = entry?;
            instance_layout::move_file(&entry.path(), &icons.join(entry.file_name()))?;
        }
        std::fs::remove_dir_all(&legacy_icons)?;
    }
    let legacy_manifest = location.mods.join(LEGACY_MANIFEST);
    if !legacy_manifest.is_file() {
        return Ok(());
    }
    if !location.manifest().exists() {
        let mut records: Vec<Record> = mod_files::read_json(&legacy_manifest)?;
        let legacy_prefix = format!("{LEGACY_ICON_CACHE}/");
        for record in &mut records {
            if let Some(path) = &record.icon_path
                && let Some(file) = path.strip_prefix(&legacy_prefix)
            {
                record.icon_path = Some(format!("{ICON_CACHE}/{file}"));
            }
        }
        profile_store::write_json_atomically(&location.manifest(), &records)?;
    }
    std::fs::remove_file(legacy_manifest)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Record {
    filename: String,
    sha1: String,
    projects: Vec<String>,
    /// Display name, author, version label and icon URL shown on the build
    /// page. Legacy manifests without these fields still load.
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    icon_url: Option<String>,
    /// Provider icon of the mod, cached inside the mods directory
    /// (`<index>/<ICON_CACHE>/<sha1>.png`) when the icon lives in the jar itself.
    #[serde(default)]
    icon_path: Option<String>,
}

/// Display metadata recorded together with an install, so the build page can
/// show the catalog picture of a mod without refetching the providers.
#[derive(Clone, Debug, Default)]
pub struct InstalledMeta {
    pub name: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Inventory {
    pub projects: HashSet<String>,
    pub warning: Option<String>,
}

#[derive(Debug, Default)]
pub struct InstalledState {
    inventories: HashMap<crate::features::instances::InstanceId, Inventory>,
    generations: HashMap<crate::features::instances::InstanceId, u64>,
    loading: HashSet<crate::features::instances::InstanceId>,
    /// Builds (project ids) whose download is currently in flight.
    pending: HashSet<String>,
}

impl InstalledState {
    pub fn contains(&self, profile: crate::features::instances::InstanceId, project: &str) -> bool {
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
    pub fn begin(&mut self, profile: crate::features::instances::InstanceId) -> u64 {
        let generation = self.generations.entry(profile).or_default();
        *generation = generation.wrapping_add(1);
        self.loading.insert(profile);
        *generation
    }
    pub fn finish(
        &mut self,
        profile: crate::features::instances::InstanceId,
        generation: u64,
        inventory: Inventory,
    ) {
        if self.generations.get(&profile) == Some(&generation) {
            self.inventories.insert(profile, inventory);
            self.loading.remove(&profile);
        }
    }
    pub fn mark_installed(
        &mut self,
        profile: crate::features::instances::InstanceId,
        project: String,
    ) {
        self.begin(profile); // Invalidate any scan started before the download completed.
        self.loading.remove(&profile);
        self.inventories
            .entry(profile)
            .or_default()
            .projects
            .insert(project);
    }
    pub fn is_loading(&self, profile: crate::features::instances::InstanceId) -> bool {
        self.loading.contains(&profile)
    }
    pub fn warning(&self, profile: crate::features::instances::InstanceId) -> Option<&str> {
        self.inventories
            .get(&profile)
            .and_then(|inventory| inventory.warning.as_deref())
    }
}

/// Every project id ever recorded for the build. Used to skip already
/// installed projects when resolving dependencies.
pub fn recorded_projects(location: &ModsLocation) -> HashSet<String> {
    match mod_files::read_json::<Vec<Record>>(&location.manifest()) {
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
    pub disabled: bool,
    pub name: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub icon_url: Option<String>,
    /// Absolute path of an icon extracted from the jar itself, ready for
    /// `img()`; empty when the icon comes from the provider (`icon_url`).
    pub icon_path: Option<PathBuf>,
}

/// Reads the manifest records of a mods directory, tolerating its absence.
fn read_records(location: &ModsLocation) -> Vec<Record> {
    mod_files::read_json(&location.manifest()).unwrap_or_default()
}

/// Lists every recorded mod file plus its on-disk enabled state.
pub fn list_entries(location: &ModsLocation) -> Vec<InstalledModEntry> {
    let mut entries: Vec<InstalledModEntry> = read_records(location)
        .into_iter()
        .map(|record| InstalledModEntry {
            filename: record.filename.clone(),
            disabled: is_disabled(&location.mods, &record.filename),
            name: record.name,
            author: record.author,
            version: record.version,
            icon_url: record.icon_url,
            // Resolved now, so a moved data directory keeps working: the
            // manifest stores the path relative to the index directory.
            icon_path: record
                .icon_path
                .as_deref()
                .map(|relative| location.index.join(relative)),
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

/// Records an installed file with the display metadata captured at install
/// time. A second `record_install` for the same filename replaces its entry.
pub fn record_install(
    location: &ModsLocation,
    project: String,
    filename: &str,
    meta: &InstalledMeta,
) -> Result<(), String> {
    let sha1 = mc_launcher_core::io::hash::sha1_file(location.mods.join(filename))
        .map_err(|e| e.to_string())?;
    let mut records: Vec<Record> =
        mod_files::read_json(&location.manifest()).map_err(|e| e.to_string())?;
    records.retain(|record| record.filename != filename);
    records.push(Record {
        filename: filename.into(),
        sha1,
        projects: vec![project],
        name: meta.name.clone(),
        author: meta.author.clone(),
        version: meta.version.clone(),
        icon_url: meta.icon_url.clone(),
        icon_path: None,
    });
    profile_store::write_json_atomically(&location.manifest(), &records).map_err(|e| e.to_string())
}

/// Deletes every mod file linked to `project` from the mods directory and
/// removes their records from the manifest. Files that fail to delete (for
/// example, a running game keeps a lock on them) stay in the manifest.
/// Returns the removed filenames.
/// Removes every mod file whose record matches, then rewrites the manifest.
fn uninstall_records(
    location: &ModsLocation,
    matches: impl Fn(&Record) -> bool,
) -> Result<Vec<String>, String> {
    let records: Vec<Record> =
        mod_files::read_json(&location.manifest()).map_err(|e| e.to_string())?;
    let mut removed = Vec::new();
    let mut kept = Vec::with_capacity(records.len());
    let mut failure = None;
    for record in records {
        if !matches(&record) {
            kept.push(record);
            continue;
        }
        match remove_record_file(&location.mods, &record.filename) {
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
        profile_store::write_json_atomically(&location.manifest(), &kept)
            .map_err(|e| e.to_string())?;
    }
    match failure {
        Some(message) => Err(message),
        None => Ok(removed),
    }
}

/// Removes the files recorded for a project id — or, when given a file name,
/// that single file. Hand-installed mods (no catalog project) are removed by
/// their file name.
pub fn uninstall(location: &ModsLocation, spec: &str) -> Result<Vec<String>, String> {
    uninstall_records(location, |record| {
        record.filename == spec || record.projects.iter().any(|project| project == spec)
    })
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

pub fn scan(location: &ModsLocation, api_key: Option<&str>) -> Inventory {
    let directory = location.mods.as_path();
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
    let cached: Vec<Record> = match mod_files::read_json(&location.manifest()) {
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
        .user_agent("orbmc/0.1")
        .build();
    if let Ok(client) = client {
        // Hashes are file identities; filenames and save paths never leave the computer.
        // Records without a version label reuse the same lookup, so a scan
        // also fills the display version of legacy or unknown files.
        let hashes: Vec<_> = records
            .iter()
            .filter(|r| {
                r.version.is_none()
                    || r.projects.is_empty()
                    || !r.projects.iter().any(|id| id.starts_with("modrinth:"))
            })
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
                                if record.version.is_none() {
                                    record.version = version.version_number.clone();
                                }
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
                                        if record.version.is_none() {
                                            // Compacted against the project
                                            // name after the meta lookup.
                                            record.version = found.file.display_name.clone();
                                        }
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
        enrich_metadata(&mut records, &client, api_key, &mut warnings);
    } else {
        warnings.push("Не удалось создать клиент для определения модов".into());
    }
    // Whatever the providers could not resolve, the mod jars themselves
    // usually carry. Pure IO, so it runs even when the network is down.
    enrich_from_jar(&mut records, location);
    for record in &mut records {
        record.projects.sort();
        record.projects.dedup();
    }
    if let Err(error) = profile_store::write_json_atomically(&location.manifest(), &records) {
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
    #[serde(default)]
    version_number: Option<String>,
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
    #[serde(default)]
    display_name: Option<String>,
}
#[derive(Deserialize)]
struct FileHash {
    algo: u8,
    value: String,
}

/// Locator files inside a mod jar, in preference order: modern Fabric,
/// Quilt, NeoForge/Forge descriptors and the legacy Forge `mcmod.info`.
const JAR_METADATA_FILES: [&str; 5] = [
    "fabric.mod.json",
    "quilt.mod.json",
    "META-INF/neoforge.mods.toml",
    "META-INF/mods.toml",
    "mcmod.info",
];

/// Display metadata of one mod as written inside its own jar.
#[derive(Clone, Debug, Default)]
struct JarMeta {
    name: Option<String>,
    author: Option<String>,
    version: Option<String>,
    /// Jar-relative icon path, or a provider URL.
    icon: Option<String>,
}

/// Fills whatever the providers could not resolve from the metadata the mod
/// itself carries — the same jar parsing the Modrinth app falls back to for
/// hand-installed mods. Pure file IO, works fully offline, and skips records
/// that are already complete.
fn enrich_from_jar(records: &mut [Record], location: &ModsLocation) {
    for record in records.iter_mut() {
        let complete = record.name.is_some()
            && record.author.is_some()
            && record.version.is_some()
            && (record.icon_url.is_some() || record.icon_path.is_some());
        if complete {
            continue;
        }
        let jar = location.mods.join(&record.filename);
        apply_jar_meta(record, &location.index, &jar);
    }
}

/// Fills the empty fields of one record from the metadata inside its jar.
fn apply_jar_meta(record: &mut Record, index: &Path, jar: &Path) {
    let Some(meta) = read_jar_meta(jar) else {
        return;
    };
    if record.name.is_none() {
        record.name = meta.name;
    }
    if record.author.is_none() {
        record.author = meta.author;
    }
    if record.version.is_none() {
        record.version = meta.version;
    }
    if record.icon_url.is_some() || record.icon_path.is_some() {
        return;
    }
    let Some(spec) = meta.icon else {
        return;
    };
    if spec.starts_with("http://") || spec.starts_with("https://") {
        // Some projects ship the icon as a hosted image already.
        record.icon_url = Some(spec);
    } else {
        record.icon_path = extract_jar_icon(jar, &spec, index, &record.sha1);
    }
}

/// Reads and parses the first locator file that yields anything.
fn read_jar_meta(jar: &Path) -> Option<JarMeta> {
    let entries = mc_launcher_core::io::archive::read_zip_entries(jar, &JAR_METADATA_FILES).ok()?;
    for (name, bytes) in entries {
        let meta = if name.ends_with("fabric.mod.json") {
            parse_fabric_meta(&bytes)
        } else if name.ends_with("quilt.mod.json") {
            parse_quilt_meta(&bytes)
        } else if name.ends_with("mods.toml") {
            parse_forgelle_meta(&String::from_utf8(bytes).ok()?.replace("\r\n", "\n"))
        } else {
            parse_mcmod_info(&bytes)
        };
        if let Some(meta) = meta.filter(|meta| meta.name.is_some() || meta.icon.is_some()) {
            return Some(meta);
        }
    }
    None
}

/// Writes an icon from the jar into the launcher icon cache and returns the
/// manifest-relative path of the copy.
///
/// The file is keyed by the jar's sha1, so re-scans never rewrite the same
/// icon and two copies of one jar share one cached file.
fn extract_jar_icon(jar: &Path, entry: &str, index: &Path, sha1: &str) -> Option<String> {
    let extract =
        mc_launcher_core::io::archive::read_zip_entries(jar, std::slice::from_ref(&entry))
            .ok()
            .and_then(|found| found.into_iter().next());
    let Some((resolved, bytes)) = extract else {
        tracing::debug!(jar = %jar.display(), entry = %entry, "no icon entry in the mod jar");
        return None;
    };
    let Some(extension) = image_extension(&bytes) else {
        tracing::debug!(jar = %jar.display(), entry = %resolved, "unsupported icon format in the mod jar");
        return None;
    };
    // Keep the cache sane; real icons are far below this.
    if bytes.len() > 4 * 1024 * 1024 {
        return None;
    }
    let file_name = format!("{sha1}.{extension}");
    let target = index.join(ICON_CACHE).join(&file_name);
    if !target.exists() {
        std::fs::create_dir_all(target.parent()?).ok()?;
        std::fs::write(&target, &bytes).ok()?;
    }
    Some(format!("{ICON_CACHE}/{file_name}"))
}

/// Detects the icon format by leading magic bytes.
fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else if bytes.starts_with(b"GIF8") {
        Some("gif")
    } else if bytes.starts_with(b"BM") {
        Some("bmp")
    } else {
        None
    }
}

/// `fabric.mod.json`: authors are strings or `{ "name": ... }` objects, and
/// `icon` is a jar-relative path or a URL.
fn parse_fabric_meta(bytes: &[u8]) -> Option<JarMeta> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    Some(JarMeta {
        name: value
            .get("name")
            .and_then(value_str)
            .filter(|value| known_label(value)),
        author: value.get("authors").and_then(first_author),
        version: value
            .get("version")
            .and_then(value_str)
            .filter(|value| known_label(value)),
        icon: value
            .get("icon")
            .and_then(value_str)
            .filter(|value| known_label(value)),
    })
}

/// `quilt.mod.json`: everything is nested under `quilt_loader`, the human
/// fields under its `metadata`, authors live in a contributors map.
fn parse_quilt_meta(bytes: &[u8]) -> Option<JarMeta> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let loader = value.get("quilt_loader")?;
    let metadata = loader.get("metadata");
    Some(JarMeta {
        name: metadata
            .and_then(|metadata| metadata.get("name"))
            .and_then(value_str),
        version: loader
            .get("version")
            .and_then(value_str)
            .filter(|value| known_label(value)),
        author: metadata
            .and_then(|metadata| metadata.get("contributors"))
            .and_then(first_contributor),
        icon: metadata
            .and_then(|metadata| metadata.get("icon"))
            .and_then(value_str)
            .filter(|value| known_label(value)),
    })
}

/// Legacy Forge `mcmod.info`: a bare list or nested in `modList`.
fn parse_mcmod_info(bytes: &[u8]) -> Option<JarMeta> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let list = value
        .as_array()
        .or_else(|| value.get("modList").and_then(|e| e.as_array()))?;
    let first = list.first()?;
    Some(JarMeta {
        name: first
            .get("name")
            .and_then(value_str)
            .filter(|value| known_label(value)),
        version: first
            .get("version")
            .and_then(value_str)
            .filter(|value| known_label(value)),
        author: first.get("authorList").and_then(first_author),
        icon: first
            .get("logoFile")
            .and_then(value_str)
            .filter(|value| known_label(value)),
    })
}

/// `META-INF/{neoforge.,}mods.toml`: only the `[[mods]]` section matters
/// here. A tiny parser instead of a toml dependency — display files are
/// `key = "value"` lines, and anything unparseable degrades to no metadata.
fn parse_forgelle_meta(source: &str) -> Option<JarMeta> {
    let mut meta = JarMeta::default();
    let mut in_mods = false;
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_mods = line == "[[mods]]";
            continue;
        }
        if !in_mods {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "displayName" | "name" => {
                meta.name = toml_string(value).filter(|value| known_label(value))
            }
            "version" => meta.version = toml_string(value).filter(|value| known_label(value)),
            // `authors = ["A", "B"]` and `authors = "A, B"` both occur.
            "authors" | "author" => {
                // `authors = "A, B"` is a common single-string form too.
                let joined = toml_string(value).or_else(|| toml_first_array_entry(value));
                meta.author = joined.map(|joined| match joined.split(',').next() {
                    Some(first) => first.trim().to_owned(),
                    None => joined,
                });
            }
            "logoFile" | "logo" | "logoBlurFile" | "icon" => {
                meta.icon = toml_string(value).filter(|value| known_label(value))
            }
            _ => {}
        }
    }
    let usable = meta.name.is_some() || meta.icon.is_some();
    usable.then_some(meta)
}

/// Forge templates versions as `${file.jarVersion}` and leaves that literal
/// in jars whose build failed to expand it; such labels are useless.
fn known_label(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && !(trimmed.starts_with("${") && trimmed.ends_with('}'))
}

fn value_str(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// `authors = ["A", "B"]` / `["A", { "name": "B" }]` / `"A, B"`.
fn first_author(value: &serde_json::Value) -> Option<String> {
    let authors = match value {
        serde_json::Value::Array(items) => items,
        serde_json::Value::String(text) if !text.trim().is_empty() => {
            return Some(
                text.split(',')
                    .next()
                    .map(str::trim)
                    .unwrap_or(text)
                    .to_owned(),
            );
        }
        _ => return None,
    };
    authors.iter().find_map(|item| match item {
        serde_json::Value::String(text) if !text.trim().is_empty() => Some(
            text.split(',')
                .next()
                .map(str::trim)
                .unwrap_or(text)
                .to_owned(),
        ),
        item => item
            .get("name")
            .and_then(value_str)
            .filter(|value| known_label(value)),
    })
}

/// Quilt contributors map names to roles; the owner of the project wins.
fn first_contributor(value: &serde_json::Value) -> Option<String> {
    let contributors = value.as_object()?;
    let picks: Vec<&String> = contributors
        .iter()
        .filter(|(_, role)| {
            let role = role.to_string();
            role.contains("Owner") || role.contains("Author") || role.contains("Maintainer")
        })
        .map(|(name, _)| name)
        .collect();
    let name = picks
        .first()
        .copied()
        .or_else(|| contributors.keys().next())?;
    (!name.trim().is_empty()).then(|| name.to_owned())
}

fn toml_string(value: &str) -> Option<String> {
    let value = value.trim();
    // Arrays are handled by `toml_first_array_entry`.
    if value.starts_with('[') || value.starts_with('{') {
        return None;
    }
    for quote in ['"', '\''] {
        if let Some(rest) = value.strip_prefix(quote) {
            let mut text = String::new();
            let mut chars = rest.chars();
            while let Some(char) = chars.next() {
                match char {
                    '\\' if quote == '"' => match chars.next() {
                        Some(escaped) => text.push(match escaped {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        }),
                        None => break,
                    },
                    _ if char == quote => return Some(text),
                    char => text.push(char),
                }
            }
            return Some(text).filter(|value| known_label(value));
        }
    }
    // Unquoted scalar (a bare version number): strip the inline comment.
    let bare = value
        .split_once(" #")
        .map_or(value, |(bare, _)| bare)
        .trim();
    (!bare.is_empty())
        .then(|| bare.to_owned())
        .filter(|value| known_label(value))
}

fn toml_first_array_entry(value: &str) -> Option<String> {
    toml_string(
        value
            .trim()
            .strip_prefix('[')?
            .split(',')
            .next()?
            .trim_end_matches(']'),
    )
}

/// Provider project details used to fill the display metadata of records
/// that predate metadata recording (or were added into the mods folder
/// manually).
#[derive(Clone, Debug, Default)]
struct ProjectMeta {
    name: Option<String>,
    author: Option<String>,
    icon_url: Option<String>,
}

/// One Modrinth search hit reduced to the useful display fields.
#[derive(Deserialize)]
struct MetaHit {
    #[serde(default)]
    project_id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    icon_url: Option<String>,
}
#[derive(Deserialize)]
struct MetaHits {
    #[serde(default)]
    hits: Vec<MetaHit>,
}

/// One CurseForge project of a batched `/v1/mods` response.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetaMod {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    authors: Vec<MetaAuthor>,
    #[serde(default)]
    logo: Option<MetaLogo>,
}
#[derive(Deserialize)]
struct MetaAuthor {
    name: String,
}
#[derive(Deserialize)]
struct MetaLogo {
    #[serde(default)]
    url: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetaMods {
    #[serde(default)]
    data: Vec<MetaMod>,
}

/// Fills missing display metadata by batch-querying the providers for the
/// recorded project ids, then compacts curseforge version labels against the
/// freshly learned project names.
///
/// Mods already carrying metadata contribute no requests, so steady-state
/// scans stay network-free on this path.
fn enrich_metadata(
    records: &mut [Record],
    client: &reqwest::blocking::Client,
    curseforge_api_key: Option<&str>,
    warnings: &mut Vec<String>,
) {
    let mut missing: Vec<String> = records
        .iter()
        .filter(|r| r.name.is_none() || r.author.is_none() || r.icon_url.is_none())
        .flat_map(|r| r.projects.iter().cloned())
        .collect();
    missing.sort();
    missing.dedup();

    let mut known: HashMap<String, ProjectMeta> = HashMap::new();
    if !missing.is_empty() {
        fetch_modrinth_meta(&mut known, client, &missing, warnings);
        fetch_curseforge_meta(&mut known, client, curseforge_api_key, &missing, warnings);
    }

    for record in records.iter_mut() {
        for project in &record.projects {
            let Some(meta) = known.get(project) else {
                continue;
            };
            record.name = record.name.clone().or_else(|| meta.name.clone());
            record.author = record.author.clone().or_else(|| meta.author.clone());
            record.icon_url = record.icon_url.clone().or_else(|| meta.icon_url.clone());
        }
    }
    apply_version_labels(records);
}

/// Modrinth batch lookup through the search endpoint: one request per 100
/// project ids returns the title, author and icon of every miss.
fn fetch_modrinth_meta(
    known: &mut HashMap<String, ProjectMeta>,
    client: &reqwest::blocking::Client,
    missing: &[String],
    warnings: &mut Vec<String>,
) {
    let ids: Vec<&str> = missing
        .iter()
        .filter_map(|id| id.strip_prefix("modrinth:"))
        .filter(|id| !id.is_empty())
        .collect();
    let ids_per_chunk = 100;
    for chunk in ids.chunks(ids_per_chunk) {
        let facets: Vec<String> = chunk.iter().map(|id| format!("project_id:{id}")).collect();
        let facets = serde_json::json!([facets]).to_string();
        let url = format!(
            "https://api.modrinth.com/v2/search?limit={}&facets={}",
            chunk.len(),
            super::catalog::encode_query_component(&facets)
        );
        let result = client
            .get(&url)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.text());
        match result {
            Ok(body) => match serde_json::from_str::<MetaHits>(&body) {
                Ok(hits) => {
                    for hit in hits.hits {
                        if hit.project_id.trim().is_empty() {
                            continue;
                        }
                        known.insert(
                            format!("modrinth:{}", hit.project_id.trim()),
                            ProjectMeta {
                                name: hit.title,
                                author: hit.author,
                                icon_url: hit.icon_url,
                            },
                        );
                    }
                }
                Err(error) => warnings.push(format!("Modrinth: {error}")),
            },
            Err(error) => warnings.push(format!("Modrinth: {error}")),
        }
    }
}

/// CurseForge batch lookup, one request per 100 mod ids.
fn fetch_curseforge_meta(
    known: &mut HashMap<String, ProjectMeta>,
    client: &reqwest::blocking::Client,
    curseforge_api_key: Option<&str>,
    missing: &[String],
    warnings: &mut Vec<String>,
) {
    let ids: Vec<u64> = missing
        .iter()
        .filter_map(|id| id.strip_prefix("curseforge:"))
        .filter_map(|id| id.parse::<u64>().ok())
        .collect();
    if ids.is_empty() {
        return;
    }
    let Some(api_key) = curseforge_api_key.filter(|key| !key.is_empty()) else {
        return;
    };
    for chunk in ids.chunks(100) {
        let result = client
            .post("https://api.curseforge.com/v1/mods")
            .header("x-api-key", api_key)
            .header("Content-Type", "application/json")
            .body(serde_json::json!({"modIds": chunk}).to_string())
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.text());
        match result {
            Ok(body) => match serde_json::from_str::<MetaMods>(&body) {
                Ok(found) => {
                    for project in found.data {
                        known.insert(
                            format!("curseforge:{}", project.id),
                            ProjectMeta {
                                name: project.name,
                                author: project
                                    .authors
                                    .into_iter()
                                    .map(|author| author.name)
                                    .next(),
                                icon_url: project.logo.and_then(|logo| logo.url),
                            },
                        );
                    }
                }
                Err(error) => warnings.push(format!("CurseForge: {error}")),
            },
            Err(error) => warnings.push(format!("CurseForge: {error}")),
        }
    }
}

/// Rewrites curseforge version labels without the project name prefix when
/// the name is known ("YACL-3.9.7+26.1" → "3.9.7+26.1").
fn apply_version_labels(records: &mut [Record]) {
    for record in records.iter_mut() {
        let Some(raw) = record.version.clone() else {
            continue;
        };
        // Modrinth versions are already clean version numbers; curseforge
        // labels are often "Project Name-1.2.3" and read badly in the list.
        if record
            .projects
            .iter()
            .any(|project| project.starts_with("curseforge:"))
        {
            record.version = Some(super::catalog::compact_version(
                record.name.as_deref(),
                &raw,
            ));
        }
    }
}

fn reconcile(files: &[mod_files::ModFile], cached: &[Record]) -> Vec<Record> {
    files
        .iter()
        .map(|file| {
            // A matching cached record restores everything: recorded
            // projects plus the display metadata shown on the build page.
            let previous = cached
                .iter()
                .find(|record| record.filename == file.filename && record.sha1 == file.sha1);
            Record {
                filename: file.filename.clone(),
                sha1: file.sha1.clone(),
                projects: previous
                    .map(|record| record.projects.clone())
                    .unwrap_or_default(),
                name: previous.and_then(|record| record.name.clone()),
                author: previous.and_then(|record| record.author.clone()),
                version: previous.and_then(|record| record.version.clone()),
                icon_url: previous.and_then(|record| record.icon_url.clone()),
                icon_path: previous.and_then(|record| record.icon_path.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps the index in a separate directory, as builds do.
    fn location(directory: &tempfile::TempDir) -> ModsLocation {
        ModsLocation {
            mods: directory.path().into(),
            index: directory.path().join("index"),
        }
    }
    #[test]
    fn installation_survives_restart_but_removed_and_changed_files_are_not_installed() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("example.jar");
        std::fs::write(&file, b"first version").unwrap();
        record_install(
            &location(&directory),
            "modrinth:example".into(),
            "example.jar",
            &Default::default(),
        )
        .unwrap();
        let cached: Vec<Record> = mod_files::read_json(&location(&directory).manifest()).unwrap();
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
        let first = crate::features::instances::InstanceId::new();
        let second = crate::features::instances::InstanceId::new();
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
        record_install(
            &location(&directory),
            "modrinth:a".into(),
            "a.jar",
            &Default::default(),
        )
        .unwrap();

        // Disable: the .jar disappears, the .disabled variant appears and
        // list_entries reports the mod as disabled.
        set_disabled(directory.path(), "a.jar", true).unwrap();
        assert!(!directory.path().join("a.jar").exists());
        assert!(directory.path().join("a.jar.disabled").exists());
        let entries = list_entries(&location(&directory));
        assert_eq!(entries.len(), 1);
        assert!(entries[0].disabled);

        // A scan must keep the record of the disabled mod: the project stays
        // "installed" (disabled is a file state, not an uninstall).
        let scanned = scan(&location(&directory), None);
        assert!(scanned.projects.contains("modrinth:a"));
        let records: Vec<Record> = mod_files::read_json(&location(&directory).manifest()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].filename, "a.jar");
        assert_eq!(records[0].projects, vec!["modrinth:a".to_owned()]);

        // Re-enable: the file is back with the original name.
        set_disabled(directory.path(), "a.jar", false).unwrap();
        assert!(directory.path().join("a.jar").exists());
        let scanned = scan(&location(&directory), None);
        assert!(scanned.projects.contains("modrinth:a"));
    }

    #[test]
    fn uninstall_removes_both_enabled_and_disabled_variants() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.jar"), b"a").unwrap();
        record_install(
            &location(&directory),
            "modrinth:a".into(),
            "a.jar",
            &Default::default(),
        )
        .unwrap();
        set_disabled(directory.path(), "a.jar", true).unwrap();
        uninstall(&location(&directory), "modrinth:a").unwrap();
        assert!(!directory.path().join("a.jar.disabled").exists());
        let records: Vec<Record> = mod_files::read_json(&location(&directory).manifest()).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn recorded_projects_lists_every_project_of_a_build() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.jar"), b"a").unwrap();
        record_install(
            &location(&directory),
            "modrinth:a".into(),
            "a.jar",
            &Default::default(),
        )
        .unwrap();
        std::fs::write(directory.path().join("b.jar"), b"b").unwrap();
        record_install(
            &location(&directory),
            "curseforge:2".into(),
            "b.jar",
            &Default::default(),
        )
        .unwrap();
        let projects = recorded_projects(&location(&directory));
        assert_eq!(
            projects,
            HashSet::from(["modrinth:a".to_owned(), "curseforge:2".to_owned()])
        );
    }

    #[test]
    fn uninstall_removes_hand_added_mods_by_their_file_name() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("hand.jar"), b"hand").unwrap();
        // A record with no catalog projects: mods added into the folder.
        let manifest = vec![bare_record(
            "hand.jar",
            "cccccccccccccccccccccccccccccccccccccccc",
        )];
        profile_store::write_json_atomically(&location(&directory).manifest(), &manifest).unwrap();

        let removed = uninstall(&location(&directory), "hand.jar").unwrap();

        assert_eq!(removed, ["hand.jar"]);
        assert!(!directory.path().join("hand.jar").exists());
        // The record disappears with the file.
        assert!(
            uninstall(&location(&directory), "hand.jar")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn uninstall_removes_only_the_requested_project() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.jar"), b"a").unwrap();
        std::fs::write(directory.path().join("b.jar"), b"b").unwrap();
        record_install(
            &location(&directory),
            "modrinth:a".into(),
            "a.jar",
            &Default::default(),
        )
        .unwrap();
        record_install(
            &location(&directory),
            "modrinth:b".into(),
            "b.jar",
            &Default::default(),
        )
        .unwrap();

        let removed = uninstall(&location(&directory), "modrinth:a").unwrap();

        assert_eq!(removed, ["a.jar"]);
        assert!(!directory.path().join("a.jar").exists());
        assert!(directory.path().join("b.jar").exists());
        let records: Vec<Record> = mod_files::read_json(&location(&directory).manifest()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].projects, ["modrinth:b"]);
    }

    #[test]
    fn entries_carry_the_recorded_display_metadata() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.jar"), b"a").unwrap();
        record_install(
            &location(&directory),
            "curseforge:7".into(),
            "a.jar",
            &InstalledMeta {
                name: Some("Example".into()),
                author: Some("Kevin".into()),
                version: Some("Example-1.2.3".into()),
                icon_url: Some("https://cdn.example/a.png".into()),
            },
        )
        .unwrap();

        let entries = list_entries(&location(&directory));

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name.as_deref(), Some("Example"));
        assert_eq!(entries[0].author.as_deref(), Some("Kevin"));
        assert_eq!(
            entries[0].icon_url.as_deref(),
            Some("https://cdn.example/a.png")
        );
    }

    #[test]
    fn legacy_manifests_without_metadata_still_load() {
        let directory = tempfile::tempdir().unwrap();
        let legacy = "[{\"filename\":\"a.jar\",\"sha1\":\"d6a1b6d4b3ffd4b05b0f1902686d7c3a52942e37\",\"projects\":[\"modrinth:a\"]}]";
        std::fs::create_dir_all(location(&directory).index).unwrap();
        std::fs::write(location(&directory).manifest(), legacy).unwrap();

        let entries = list_entries(&location(&directory));

        assert_eq!(entries.len(), 1);
        assert!(entries[0].name.is_none());
        assert!(entries[0].author.is_none());
        assert!(entries[0].version.is_none());
    }

    #[test]
    fn version_label_compaction_strips_the_project_name_prefix() {
        assert_eq!(
            super::super::catalog::compact_version(
                Some("YetAnotherConfigLib (YACL)"),
                "YetAnotherConfigLib (YACL)-3.9.7+26.1-neoforge"
            ),
            "3.9.7+26.1-neoforge"
        );
        assert_eq!(
            super::super::catalog::compact_version(None, "15.0.0+26.1.2"),
            "15.0.0+26.1.2"
        );
        // Unrelated prefixes survive untouched.
        assert_eq!(
            super::super::catalog::compact_version(Some("Curios API"), "KotlinLangForge-2.15"),
            "KotlinLangForge-2.15"
        );
    }

    /// Builds a mod jar with the given entries so the jar parsers run against
    /// real zip archives, not just strings.
    fn write_test_jar(path: &Path, entries: &[(&str, &[u8])]) {
        use std::io::Write as _;
        let file = std::fs::File::create(path).expect("create test jar");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, options).expect("start entry");
            zip.write_all(bytes).expect("write entry");
        }
        zip.finish().expect("finish test jar");
    }

    const ICON_PNG: &[u8] = b"\x89PNG\r\n\x1a\nexample icon bytes";

    fn bare_record(filename: &str, sha1: &str) -> Record {
        Record {
            filename: filename.into(),
            sha1: sha1.into(),
            projects: Vec::new(),
            name: None,
            author: None,
            version: None,
            icon_url: None,
            icon_path: None,
        }
    }

    #[test]
    fn fabric_jar_metadata_fills_missing_fields_and_caches_the_icon() {
        let directory = tempfile::tempdir().unwrap();
        write_test_jar(
            &directory.path().join("example.jar"),
            &[
                (
                    "fabric.mod.json",
                    br#"{"schemaVersion":1,"id":"example","version":"5.4.9+1.21.1","name":"Example Mod","authors":[{"name":"Kevin"},{"name":"Fred"}],"icon":"assets/example.png"}"#,
                ),
                ("assets/example.png", ICON_PNG),
            ],
        );
        let mut records = vec![bare_record(
            "example.jar",
            "9dd9f8f33e54556ab8fcb4a070ff2df2c6dc27dd",
        )];

        enrich_from_jar(&mut records, &location(&directory));

        let record = &records[0];
        assert_eq!(record.name.as_deref(), Some("Example Mod"));
        assert_eq!(record.author.as_deref(), Some("Kevin"));
        assert_eq!(record.version.as_deref(), Some("5.4.9+1.21.1"));
        let cached = record.icon_path.as_deref().expect("icon extracted");
        assert!(cached.starts_with("mod-icons/"));
        assert!(cached.ends_with(".png"));
        assert!(location(&directory).index.join(cached).is_file());
    }

    #[test]
    fn forgelle_jar_metadata_names_the_mod_and_rejects_the_version_placeholder() {
        let directory = tempfile::tempdir().unwrap();
        write_test_jar(
            &directory.path().join("legacy.jar"),
            &[
                (
                    "META-INF/mods.toml",
                    b"modLoader=\"javafml\"\nloaderVersion=\"[46,)\"\n\n[[mods]]\nmodId=\"legacy\"\nversion=\"${file.jarVersion}\"\ndisplayName=\"Curios API\"\nlogoFile=\"legacy_logo.png\"\nauthors=\"TheIllusiveC4, FancyGuy\"\n",
                ),
                ("legacy_logo.png", ICON_PNG),
            ],
        );
        let mut records = vec![bare_record(
            "legacy.jar",
            "5aa9c27b8e4a070ff2df2c6dc27dd0000000000",
        )];

        enrich_from_jar(&mut records, &location(&directory));

        let record = &records[0];
        assert_eq!(record.name.as_deref(), Some("Curios API"));
        // `${file.jarVersion}` never survives the build — no fake version.
        assert_eq!(record.version, None);
        assert_eq!(record.author.as_deref(), Some("TheIllusiveC4"));
        assert!(record.icon_path.is_some());
    }

    #[test]
    fn quilt_and_mcmod_info_parsers_read_their_dialects() {
        let quilt = serde_json::json!({
            "schema_version": 1,
            "quilt_loader": {
                "id": "example",
                "version": "3.1.0",
                "metadata": {
                    "name": "Quilt Mod",
                    "contributors": {"Alice": "Owner", "Bob": "Contributor"},
                    "icon": "assets/q.png"
                }
            }
        });
        let quilt = parse_quilt_meta(serde_json::to_vec(&quilt).unwrap().as_slice()).unwrap();
        assert_eq!(quilt.name.as_deref(), Some("Quilt Mod"));
        assert_eq!(quilt.author.as_deref(), Some("Alice"));
        assert_eq!(quilt.version.as_deref(), Some("3.1.0"));
        assert_eq!(quilt.icon.as_deref(), Some("assets/q.png"));

        let legacy = parse_mcmod_info(
            br#"{"modList":[{"modid":"old","name":"Old Mod","version":"1.0","authorList":["Steve","Fred"],"logoFile":"logo.png"}]}"#,
        )
        .unwrap();
        assert_eq!(legacy.name.as_deref(), Some("Old Mod"));
        assert_eq!(legacy.author.as_deref(), Some("Steve"));
        assert_eq!(legacy.icon.as_deref(), Some("logo.png"));
    }

    #[test]
    fn jar_icon_urls_go_to_the_provider_field_not_the_cache() {
        let directory = tempfile::tempdir().unwrap();
        write_test_jar(
            &directory.path().join("hosted.jar"),
            &[(
                "fabric.mod.json",
                br#"{"id":"hosted","name":"Hosted Icon Mod","version":"1.0","icon":"https://cdn.example/hosted.png"}"#,
            )],
        );
        let mut records = vec![bare_record(
            "hosted.jar",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )];

        enrich_from_jar(&mut records, &location(&directory));

        assert_eq!(
            records[0].icon_url.as_deref(),
            Some("https://cdn.example/hosted.png")
        );
        assert_eq!(records[0].icon_path, None);
        assert!(!location(&directory).index.join(ICON_CACHE).exists());
    }

    #[test]
    fn legacy_index_moves_out_of_the_mods_directory() {
        let directory = tempfile::tempdir().unwrap();
        let mut record = bare_record("a.jar", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        record.icon_path = Some(format!("{LEGACY_ICON_CACHE}/aaaa.png"));
        profile_store::write_json_atomically(&directory.path().join(LEGACY_MANIFEST), &[record])
            .unwrap();
        std::fs::create_dir_all(directory.path().join(LEGACY_ICON_CACHE)).unwrap();
        std::fs::write(
            directory.path().join(LEGACY_ICON_CACHE).join("aaaa.png"),
            ICON_PNG,
        )
        .unwrap();

        migrate_legacy_index(&location(&directory)).unwrap();

        assert!(!directory.path().join(LEGACY_MANIFEST).exists());
        assert!(!directory.path().join(LEGACY_ICON_CACHE).exists());
        let entries = list_entries(&location(&directory));
        let icon = entries[0].icon_path.as_deref().expect("icon kept");
        assert!(icon.is_file());
        assert!(icon.starts_with(location(&directory).index.join(ICON_CACHE)));
    }
}
