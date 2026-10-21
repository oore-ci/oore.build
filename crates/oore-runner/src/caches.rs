//! Runner-local dependency caches.
//!
//! Every build starts from a fresh clone, so the runner keeps the package
//! manager caches itself: one host directory per tool, handed to build steps
//! through the tool's own environment variable. The directories live outside
//! any build workspace and contain no symlinks, so VM isolation can share them
//! into a guest unchanged (see [`RunnerCaches::mounts`]).
//!
//! Lockfile hashes decide whether a build is a hit: a key that a previous
//! successful build recorded means the same dependency set is already on disk.
//! After each build the runner measures the caches and evicts the least
//! recently used entries until the total is under the cap.

use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const CACHE_DIR_ENV: &str = "OORE_RUNNER_CACHE_DIR";
pub(crate) const CACHE_MAX_GB_ENV: &str = "OORE_RUNNER_CACHE_MAX_GB";
const DEFAULT_MAX_BYTES: u64 = 50 * GIB;
const GIB: u64 = 1024 * 1024 * 1024;
const STATE_FILE: &str = "state.json";
/// Eviction stops below this share of the cap so the next build does not
/// immediately evict again.
const EVICTION_TARGET_PERCENT: u64 = 90;
const KEYS_PER_KIND: usize = 64;
const LOCKFILE_SCAN_DEPTH: usize = 5;
const LOCKFILE_SKIP_DIRS: [&str; 10] = [
    ".git",
    ".dart_tool",
    ".gradle",
    ".symlinks",
    "build",
    "DerivedData",
    "ephemeral",
    "node_modules",
    "Pods",
    ".fvm",
];

/// One host directory a build uses as a cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheMount {
    pub name: &'static str,
    /// The variable that points the tool at the directory. DerivedData and
    /// Android SDK packages have none; a VM copies those where the tool looks.
    pub env: Option<&'static str>,
    pub host_dir: PathBuf,
}

struct CacheDirSpec {
    name: &'static str,
    env: Option<&'static str>,
    /// Subtrees whose entries at the given depth are evicted as one unit.
    /// Anything outside them is counted but never evicted.
    units: &'static [(&'static str, usize)],
    stops_gradle: bool,
}

const CACHE_DIRS: [CacheDirSpec; 6] = [
    CacheDirSpec {
        name: "pub",
        env: Some("PUB_CACHE"),
        units: &[("hosted", 2), ("hosted-hashes", 2), ("git", 1)],
        stops_gradle: false,
    },
    CacheDirSpec {
        name: "gradle",
        env: Some("GRADLE_USER_HOME"),
        units: &[
            ("caches", 1),
            ("wrapper/dists", 1),
            ("jdks", 1),
            ("daemon", 1),
            ("native", 1),
        ],
        stops_gradle: true,
    },
    CacheDirSpec {
        name: "cocoapods",
        env: Some("CP_CACHE_DIR"),
        units: &[("Pods", 2)],
        stops_gradle: false,
    },
    CacheDirSpec {
        name: "cocoapods-specs",
        env: Some("CP_HOME_DIR"),
        units: &[("repos", 1)],
        stops_gradle: false,
    },
    CacheDirSpec {
        name: "derived-data",
        env: None,
        units: &[(".", 1)],
        stops_gradle: false,
    },
    // Android SDK packages VM builds installed; a host build installs into
    // the host's SDK, which persists on its own.
    CacheDirSpec {
        name: "android-sdk",
        env: None,
        units: &[
            ("platforms", 1),
            ("build-tools", 1),
            ("ndk", 1),
            ("cmake", 1),
        ],
        stops_gradle: false,
    },
];

/// The lockfiles that key each dependency cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockKind {
    Pub,
    Gradle,
    CocoaPods,
}

impl LockKind {
    const ALL: [LockKind; 3] = [LockKind::Pub, LockKind::Gradle, LockKind::CocoaPods];

    fn name(self) -> &'static str {
        match self {
            LockKind::Pub => "pub",
            LockKind::Gradle => "gradle",
            LockKind::CocoaPods => "cocoapods",
        }
    }

    fn lockfile_label(self) -> &'static str {
        match self {
            LockKind::Pub => "pubspec.lock",
            LockKind::Gradle => "Gradle wrapper",
            LockKind::CocoaPods => "Podfile.lock",
        }
    }

    fn matches(self, file_name: &str) -> bool {
        match self {
            LockKind::Pub => file_name == "pubspec.lock",
            LockKind::Gradle => matches!(
                file_name,
                "gradle-wrapper.properties"
                    | "libs.versions.toml"
                    | "settings.gradle"
                    | "settings.gradle.kts"
            ),
            LockKind::CocoaPods => file_name == "Podfile.lock",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CacheUsage {
    pub name: String,
    pub bytes: u64,
}

/// What the runner reports to the backend in `capabilities.caches`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CacheStats {
    pub total_bytes: u64,
    pub max_bytes: u64,
    pub measured_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purged_at: Option<i64>,
    #[serde(default)]
    pub evicted_bytes: u64,
    pub kinds: Vec<CacheUsage>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheState {
    #[serde(default)]
    keys: BTreeMap<String, BTreeMap<String, i64>>,
    #[serde(default)]
    purged_at: Option<i64>,
    #[serde(default)]
    stats: Option<CacheStats>,
}

/// The lockfile keys of one build, captured right after checkout.
#[derive(Debug, Default)]
pub(crate) struct CacheObservation {
    keys: Vec<(LockKind, String)>,
    pub_packages: Vec<(String, String)>,
    /// Lines for the build log, one per cache.
    pub lines: Vec<String>,
}

pub(crate) struct RunnerCaches {
    root: PathBuf,
    max_bytes: u64,
    /// Xcode's default DerivedData folder. Host builds leave DerivedData
    /// there; folders whose workspace is one of this runner's are ours.
    xcode_derived_data: Option<PathBuf>,
    /// The Gradle home builds used before the runner owned one. Its
    /// `gradle.properties` often carries proxy and JVM settings the host
    /// needs, so builds keep getting it.
    host_gradle_home: Option<PathBuf>,
    workspace_marker: String,
}

impl RunnerCaches {
    /// Resolves and creates the cache root for a runner.
    pub(crate) fn for_runner(runner_id: &str, workspace_prefix: &str) -> anyhow::Result<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let root = match std::env::var_os(CACHE_DIR_ENV).filter(|value| !value.is_empty()) {
            Some(path) => PathBuf::from(path),
            None => home
                .as_deref()
                .context("HOME is not set, so the runner cannot place its caches")?
                .join("Library/Caches/build.oore/runner-caches")
                .join(runner_cache_id(runner_id)),
        };
        let max_bytes = std::env::var(CACHE_MAX_GB_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|gb| *gb > 0)
            .map_or(DEFAULT_MAX_BYTES, |gb| gb.saturating_mul(GIB));
        let xcode_derived_data = home
            .as_ref()
            .map(|home| home.join("Library/Developer/Xcode/DerivedData"));
        let mut caches = Self::new(root, max_bytes, xcode_derived_data, workspace_prefix)?;
        caches.host_gradle_home = std::env::var_os("GRADLE_USER_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.map(|home| home.join(".gradle")));
        Ok(caches)
    }

    fn new(
        root: PathBuf,
        max_bytes: u64,
        xcode_derived_data: Option<PathBuf>,
        workspace_prefix: &str,
    ) -> anyhow::Result<Self> {
        create_private_dir_all(&root)?;
        let metadata = fs::symlink_metadata(&root)?;
        anyhow::ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "runner cache root {} is not a directory",
            root.display()
        );
        for spec in &CACHE_DIRS {
            create_private_dir_all(&root.join(spec.name))?;
        }
        Ok(Self {
            root,
            max_bytes,
            xcode_derived_data,
            host_gradle_home: None,
            workspace_marker: format!("/{workspace_prefix}"),
        })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Every cache directory, for sharing into a VM.
    pub(crate) fn mounts(&self) -> Vec<CacheMount> {
        CACHE_DIRS
            .iter()
            .map(|spec| CacheMount {
                name: spec.name,
                env: spec.env,
                host_dir: self.root.join(spec.name),
            })
            .collect()
    }

    /// The environment that points build steps at the host cache directories.
    pub(crate) fn step_env(&self) -> Vec<(String, String)> {
        self.mounts()
            .into_iter()
            .filter_map(|mount| {
                mount.env.map(|env| {
                    (
                        env.to_string(),
                        mount.host_dir.to_string_lossy().into_owned(),
                    )
                })
            })
            .collect()
    }

    /// The last measurement, so a restarted runner reports sizes at once.
    pub(crate) fn last_stats(&self) -> Option<CacheStats> {
        self.load_state().stats
    }

    pub(crate) fn purged_at(&self) -> Option<i64> {
        self.load_state().purged_at
    }

    /// Hashes the workspace lockfiles and describes each cache as a hit or miss.
    pub(crate) fn observe(&self, workspace: &Path) -> CacheObservation {
        let state = self.load_state();
        let lockfiles = find_lockfiles(workspace);
        let mut observation = CacheObservation::default();
        let size = state
            .stats
            .as_ref()
            .map(|stats| {
                format!(
                    " ({} of {} used)",
                    format_bytes(stats.total_bytes),
                    format_bytes(self.max_bytes)
                )
            })
            .unwrap_or_default();
        observation.lines.push(format!(
            "Dependency caches at {}{size}",
            self.root.display()
        ));
        match self.sync_host_gradle_properties() {
            Ok(Some(source)) => observation.lines.push(format!(
                "  gradle: using settings from {}",
                source.display()
            )),
            Ok(None) => {}
            Err(error) => observation.lines.push(format!(
                "  gradle: could not copy host gradle.properties: {error}"
            )),
        }

        for kind in LockKind::ALL {
            let files: Vec<&(String, PathBuf)> = lockfiles
                .iter()
                .filter(|(relative, _)| kind.matches(file_name_of(relative)))
                .collect();
            if files.is_empty() {
                observation.lines.push(format!(
                    "  {}: not used (no {})",
                    kind.name(),
                    kind.lockfile_label()
                ));
                continue;
            }
            let key = lockfile_key(&files);
            let hit = state
                .keys
                .get(kind.name())
                .is_some_and(|keys| keys.contains_key(&key));
            let short = &key[..12];
            observation.lines.push(if hit {
                format!(
                    "  {}: hit ({} {short}), dependencies are already cached",
                    kind.name(),
                    kind.lockfile_label()
                )
            } else {
                format!(
                    "  {}: miss ({} {short}), dependencies will be downloaded and cached",
                    kind.name(),
                    kind.lockfile_label()
                )
            });
            if kind == LockKind::Pub {
                for (_, path) in &files {
                    if let Ok(raw) = fs::read_to_string(path) {
                        observation.pub_packages.extend(hosted_pub_packages(&raw));
                    }
                }
            }
            observation.keys.push((kind, key));
        }

        let derived_data_hit = self
            .owned_xcode_derived_data()
            .iter()
            .any(|(_, workspace_path)| workspace_path.starts_with(&*workspace.to_string_lossy()));
        if workspace.join("ios").is_dir() || workspace.join("macos").is_dir() {
            observation.lines.push(format!(
                "  derived data: {}",
                if derived_data_hit {
                    "hit, Xcode reuses this project's DerivedData"
                } else {
                    "miss, Xcode starts a new DerivedData folder for this project"
                }
            ));
        }
        observation
    }

    /// Copies the host's `gradle.properties` into the runner's Gradle home so
    /// proxy (`systemProp.*`) and JVM settings still apply. A copy, not a
    /// symlink, so the directory stays self-contained for VM mounts.
    fn sync_host_gradle_properties(&self) -> std::io::Result<Option<PathBuf>> {
        let target = self.root.join("gradle/gradle.properties");
        let source = self
            .host_gradle_home
            .as_ref()
            .map(|home| home.join("gradle.properties"))
            .filter(|source| *source != target);
        let content = match source.as_deref().map(fs::read) {
            Some(Ok(content)) => content,
            Some(Err(error)) if error.kind() != ErrorKind::NotFound => return Err(error),
            _ => {
                remove_path(&target)?;
                return Ok(None);
            }
        };
        if fs::read(&target).ok().as_deref() != Some(content.as_slice()) {
            let temporary = self.root.join("gradle/gradle.properties.tmp");
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(&content)?;
            file.sync_all()?;
            fs::rename(&temporary, &target)?;
        }
        Ok(source)
    }

    /// Records a finished build. Keys are kept only for successful builds, so a
    /// hit always means a build with these lockfiles has completed before.
    pub(crate) fn record(&self, observation: &CacheObservation, succeeded: bool, now: i64) {
        if !succeeded {
            return;
        }
        let mut state = self.load_state();
        for (kind, key) in &observation.keys {
            let keys = state.keys.entry(kind.name().to_string()).or_default();
            keys.insert(key.clone(), now);
            while keys.len() > KEYS_PER_KIND {
                let Some(oldest) = keys
                    .iter()
                    .min_by_key(|(_, used)| **used)
                    .map(|(key, _)| key.clone())
                else {
                    break;
                };
                keys.remove(&oldest);
            }
        }
        if let Err(error) = self.save_state(&state) {
            eprintln!("Warning: could not save runner cache state: {error:#}");
        }
        // Package directories are the eviction units of the pub cache; mark the
        // ones this build used so eviction sees them as recent.
        let pub_root = self.root.join("pub/hosted");
        if let Ok(hosts) = fs::read_dir(&pub_root) {
            for host in hosts.flatten() {
                for (name, version) in &observation.pub_packages {
                    let dir = host.path().join(format!("{name}-{version}"));
                    if let Ok(file) = fs::File::open(&dir) {
                        let _ = file.set_modified(std::time::SystemTime::now());
                    }
                }
            }
        }
    }

    /// Measures every cache, evicts least recently used entries above the cap,
    /// and stores the result.
    pub(crate) fn maintain(&self, now: i64) -> CacheStats {
        let mut units = Vec::new();
        let mut kinds = Vec::new();
        let mut total = 0u64;
        for spec in &CACHE_DIRS {
            let dir = self.root.join(spec.name);
            let (bytes, kind_units) = measure_cache_dir(&dir, spec.units);
            total += bytes;
            kinds.push(CacheUsage {
                name: spec.name.to_string(),
                bytes,
            });
            units.extend(kind_units.into_iter().map(|unit| (spec, unit)));
        }
        let mut xcode_bytes = 0;
        for (folder, _) in self.owned_xcode_derived_data() {
            let usage = measure(&folder);
            xcode_bytes += usage.bytes;
            units.push((
                &CACHE_DIRS[4],
                Unit {
                    path: folder,
                    bytes: usage.bytes,
                    last_used: usage.newest,
                },
            ));
        }
        if let Some(derived) = kinds.iter_mut().find(|kind| kind.name == "derived-data") {
            derived.bytes += xcode_bytes;
        }
        total += xcode_bytes;

        let mut evicted = 0u64;
        if total > self.max_bytes {
            let target = self.max_bytes / 100 * EVICTION_TARGET_PERCENT;
            units.sort_by_key(|(_, unit)| unit.last_used);
            let mut gradle_stopped = false;
            for (spec, unit) in units {
                if total <= target {
                    break;
                }
                if spec.stops_gradle && !gradle_stopped {
                    stop_gradle_daemons(&self.root.join("gradle"));
                    gradle_stopped = true;
                }
                if remove_path(&unit.path).is_ok() {
                    total = total.saturating_sub(unit.bytes);
                    evicted += unit.bytes;
                    if let Some(kind) = kinds.iter_mut().find(|kind| kind.name == spec.name) {
                        kind.bytes = kind.bytes.saturating_sub(unit.bytes);
                    }
                }
            }
        }

        let mut state = self.load_state();
        let stats = CacheStats {
            total_bytes: total,
            max_bytes: self.max_bytes,
            measured_at: now,
            purged_at: state.purged_at,
            evicted_bytes: evicted,
            kinds,
        };
        state.stats = Some(stats.clone());
        if let Err(error) = self.save_state(&state) {
            eprintln!("Warning: could not save runner cache state: {error:#}");
        }
        stats
    }

    /// Deletes every cached dependency and forgets every key.
    pub(crate) fn purge(&self, requested_at: i64, now: i64) -> anyhow::Result<CacheStats> {
        stop_gradle_daemons(&self.root.join("gradle"));
        for spec in &CACHE_DIRS {
            let dir = self.root.join(spec.name);
            remove_path(&dir)?;
            create_private_dir_all(&dir)?;
        }
        for (folder, _) in self.owned_xcode_derived_data() {
            remove_path(&folder)?;
        }
        let state = CacheState {
            purged_at: Some(requested_at),
            ..CacheState::default()
        };
        self.save_state(&state)?;
        Ok(self.maintain(now))
    }

    fn owned_xcode_derived_data(&self) -> Vec<(PathBuf, String)> {
        let Some(root) = self.xcode_derived_data.as_deref() else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(root) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if !fs::symlink_metadata(&path).ok()?.is_dir() {
                    return None;
                }
                let workspace = derived_data_workspace_path(&path.join("info.plist"))?;
                workspace
                    .contains(&self.workspace_marker)
                    .then_some((path, workspace))
            })
            .collect()
    }

    fn load_state(&self) -> CacheState {
        fs::read(self.root.join(STATE_FILE))
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default()
    }

    fn save_state(&self, state: &CacheState) -> anyhow::Result<()> {
        let path = self.root.join(STATE_FILE);
        let temporary = self.root.join(format!("{STATE_FILE}.tmp"));
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(state)?)?;
        file.sync_all()?;
        fs::rename(&temporary, &path)?;
        Ok(())
    }
}

fn runner_cache_id(runner_id: &str) -> String {
    let digest = Sha256::digest(runner_id.as_bytes());
    hex::encode(&digest[..8])
}

fn create_private_dir_all(path: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn file_name_of(relative: &str) -> &str {
    relative.rsplit('/').next().unwrap_or(relative)
}

/// Lockfiles in the checkout, as (path relative to the workspace, full path).
fn find_lockfiles(workspace: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let mut pending = vec![(workspace.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if file_type.is_dir() {
                if depth < LOCKFILE_SCAN_DEPTH && !LOCKFILE_SKIP_DIRS.contains(&name.as_ref()) {
                    pending.push((path, depth + 1));
                }
            } else if file_type.is_file()
                && LockKind::ALL.iter().any(|kind| kind.matches(&name))
                && let Ok(relative) = path.strip_prefix(workspace)
            {
                found.push((relative.to_string_lossy().replace('\\', "/"), path));
            }
        }
    }
    found.sort();
    found
}

fn lockfile_key(files: &[&(String, PathBuf)]) -> String {
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(path).unwrap_or_default());
        hasher.update([0]);
    }
    hex::encode(&hasher.finalize()[..16])
}

/// Hosted package names and versions from a `pubspec.lock`.
fn hosted_pub_packages(raw: &str) -> Vec<(String, String)> {
    let mut packages = Vec::new();
    let mut in_packages = false;
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut hosted = false;
    let mut flush = |name: &mut Option<String>, version: &mut Option<String>, hosted: &mut bool| {
        if let (Some(name), Some(version), true) = (name.take(), version.take(), *hosted) {
            packages.push((name, version));
        }
        *hosted = false;
    };
    for line in raw.lines() {
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if indent == 0 {
            flush(&mut name, &mut version, &mut hosted);
            in_packages = trimmed == "packages:";
            continue;
        }
        if !in_packages {
            continue;
        }
        if indent == 2 {
            flush(&mut name, &mut version, &mut hosted);
            name = trimmed
                .strip_suffix(':')
                .map(|value| value.trim_matches('"').to_string());
        } else if indent == 4 {
            if let Some(value) = trimmed.strip_prefix("version:") {
                version = Some(value.trim().trim_matches('"').to_string());
            } else if let Some(value) = trimmed.strip_prefix("source:") {
                hosted = value.trim() == "hosted";
            }
        }
    }
    flush(&mut name, &mut version, &mut hosted);
    packages
}

/// The `WorkspacePath` Xcode records in a DerivedData folder's `info.plist`.
fn derived_data_workspace_path(info_plist: &Path) -> Option<String> {
    let raw = fs::read_to_string(info_plist).ok()?;
    let after_key = raw.split("<key>WorkspacePath</key>").nth(1)?;
    let value = after_key
        .split("<string>")
        .nth(1)?
        .split("</string>")
        .next()?;
    Some(value.trim().to_string())
}

#[derive(Debug, Default, Clone, Copy)]
struct Usage {
    bytes: u64,
    newest: i64,
}

#[derive(Debug)]
struct Unit {
    path: PathBuf,
    bytes: u64,
    last_used: i64,
}

fn modified_unix(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs() as i64)
}

fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        metadata.blocks() * 512
    }
    #[cfg(not(unix))]
    {
        metadata.len()
    }
}

/// Disk usage and newest modification time of a path, without following symlinks.
fn measure(path: &Path) -> Usage {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Usage::default();
    };
    let mut usage = Usage {
        bytes: allocated_bytes(&metadata),
        newest: modified_unix(&metadata),
    };
    if metadata.is_dir()
        && let Ok(entries) = fs::read_dir(path)
    {
        for entry in entries.flatten() {
            let child = measure(&entry.path());
            usage.bytes += child.bytes;
            usage.newest = usage.newest.max(child.newest);
        }
    }
    usage
}

/// Total usage of one cache directory plus its evictable units.
fn measure_cache_dir(dir: &Path, unit_roots: &[(&str, usize)]) -> (u64, Vec<Unit>) {
    let mut units = Vec::new();
    for (relative, depth) in unit_roots {
        collect_units(&dir.join(relative), *depth, &mut units);
    }
    (measure(dir).bytes, units)
}

fn collect_units(dir: &Path, depth: usize, units: &mut Vec<Unit>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_dir = entry.file_type().is_ok_and(|file_type| file_type.is_dir());
        if depth > 1 && is_dir {
            collect_units(&path, depth - 1, units);
        } else {
            let usage = measure(&path);
            units.push(Unit {
                path,
                bytes: usage.bytes,
                last_used: usage.newest,
            });
        }
    }
}

/// Gradle daemons keep cache files open, so stop the ones started against this
/// cache before deleting from it. Their log names carry the process id.
fn stop_gradle_daemons(gradle_home: &Path) {
    let Ok(versions) = fs::read_dir(gradle_home.join("daemon")) else {
        return;
    };
    for version in versions.flatten() {
        let Ok(logs) = fs::read_dir(version.path()) else {
            continue;
        };
        for log in logs.flatten() {
            let name = log.file_name();
            let Some(pid) = name
                .to_str()
                .and_then(|name| name.strip_prefix("daemon-"))
                .and_then(|rest| rest.strip_suffix(".out.log"))
                .and_then(|pid| pid.parse::<i32>().ok())
                .filter(|pid| *pid > 1)
            else {
                continue;
            };
            let is_gradle_daemon = std::process::Command::new("/bin/ps")
                .args(["-p", &pid.to_string(), "-o", "command="])
                .output()
                .ok()
                .is_some_and(|output| {
                    String::from_utf8_lossy(&output.stdout).contains("GradleDaemon")
                });
            if is_gradle_daemon {
                // SAFETY: plain signal delivery to a process this user owns.
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
            }
        }
    }
}

pub(crate) fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caches(dir: &Path, max_bytes: u64) -> RunnerCaches {
        RunnerCaches::new(
            dir.join("caches"),
            max_bytes,
            Some(dir.join("DerivedData")),
            "oore-build-0123456789abcdef-",
        )
        .unwrap()
    }

    fn write(path: &Path, content: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "oore-cache-test-{name}-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn step_env_points_each_tool_at_its_own_directory() {
        let dir = temp_dir("env");
        let caches = caches(&dir, GIB);
        let env: BTreeMap<_, _> = caches.step_env().into_iter().collect();
        let root = dir.join("caches");
        assert_eq!(env["PUB_CACHE"], root.join("pub").to_string_lossy());
        assert_eq!(
            env["GRADLE_USER_HOME"],
            root.join("gradle").to_string_lossy()
        );
        assert_eq!(
            env["CP_CACHE_DIR"],
            root.join("cocoapods").to_string_lossy()
        );
        assert_eq!(
            env["CP_HOME_DIR"],
            root.join("cocoapods-specs").to_string_lossy()
        );
        assert_eq!(env.len(), 4);
        assert!(
            caches
                .mounts()
                .iter()
                .all(|mount| mount.host_dir.is_dir() && mount.host_dir.starts_with(&root))
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn second_build_with_the_same_lockfiles_is_a_hit() {
        let dir = temp_dir("hit");
        let caches = caches(&dir, GIB);
        let workspace = dir.join("workspace");
        write(&workspace.join("pubspec.lock"), b"packages:\n");
        write(
            &workspace.join("android/gradle/wrapper/gradle-wrapper.properties"),
            b"distributionUrl=gradle-8.10-all.zip\n",
        );

        let first = caches.observe(&workspace);
        assert!(
            first
                .lines
                .iter()
                .any(|line| line.starts_with("  pub: miss"))
        );
        assert!(
            first
                .lines
                .iter()
                .any(|line| line.starts_with("  gradle: miss"))
        );
        assert!(
            first
                .lines
                .iter()
                .any(|line| line == "  cocoapods: not used (no Podfile.lock)")
        );

        caches.record(&first, false, 10);
        let after_failure = caches.observe(&workspace);
        assert!(
            after_failure
                .lines
                .iter()
                .any(|line| line.starts_with("  pub: miss"))
        );

        caches.record(&after_failure, true, 20);
        let second = caches.observe(&workspace);
        assert!(
            second
                .lines
                .iter()
                .any(|line| line.starts_with("  pub: hit"))
        );
        assert!(
            second
                .lines
                .iter()
                .any(|line| line.starts_with("  gradle: hit"))
        );

        write(&workspace.join("pubspec.lock"), b"packages:\n  args:\n");
        let changed = caches.observe(&workspace);
        assert!(
            changed
                .lines
                .iter()
                .any(|line| line.starts_with("  pub: miss"))
        );
        assert!(
            changed
                .lines
                .iter()
                .any(|line| line.starts_with("  gradle: hit"))
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lockfiles_in_build_outputs_are_ignored() {
        let dir = temp_dir("skip");
        write(&dir.join("app/pubspec.lock"), b"a");
        write(&dir.join("build/pubspec.lock"), b"b");
        write(&dir.join("ios/Pods/Podfile.lock"), b"c");
        write(&dir.join("ios/Podfile.lock"), b"d");
        let found: Vec<String> = find_lockfiles(&dir)
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        assert_eq!(found, ["app/pubspec.lock", "ios/Podfile.lock"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hosted_packages_are_read_from_pubspec_lock() {
        let lock = r#"# Generated by pub
packages:
  args:
    dependency: transitive
    description:
      name: args
      url: "https://pub.dev"
    source: hosted
    version: "2.5.0"
  flutter:
    dependency: "direct main"
    description: flutter
    source: sdk
    version: "0.0.0"
sdks:
  dart: ">=3.4.0 <4.0.0"
"#;
        assert_eq!(
            hosted_pub_packages(lock),
            [("args".to_string(), "2.5.0".to_string())]
        );
    }

    #[test]
    fn eviction_drops_least_recently_used_entries_first() {
        let dir = temp_dir("evict");
        let cap = 3 * 1024 * 1024 / 2;
        let caches = caches(&dir, cap);
        let hosted = dir.join("caches/pub/hosted/pub.dev");
        let old = hosted.join("old-1.0.0");
        let fresh = hosted.join("fresh-1.0.0");
        write(&old.join("lib.dart"), &vec![1u8; 1024 * 1024]);
        write(&fresh.join("lib.dart"), &vec![2u8; 1024 * 1024]);
        let past = std::time::SystemTime::now() - std::time::Duration::from_secs(86_400);
        for path in [old.join("lib.dart"), old.clone()] {
            fs::File::open(&path).unwrap().set_modified(past).unwrap();
        }

        let stats = caches.maintain(100);
        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(stats.evicted_bytes > 0);
        assert!(stats.total_bytes <= cap);
        assert_eq!(caches.last_stats(), Some(stats));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn purge_clears_caches_keys_and_owned_derived_data() {
        let dir = temp_dir("purge");
        let caches = caches(&dir, GIB);
        let workspace = dir.join("oore-build-0123456789abcdef-aaaa");
        write(&workspace.join("pubspec.lock"), b"packages:\n");
        let observation = caches.observe(&workspace);
        caches.record(&observation, true, 1);
        write(&dir.join("caches/gradle/caches/modules-2/a.jar"), b"jar");

        let ours = dir.join("DerivedData/Runner-abc");
        write(
            &ours.join("info.plist"),
            format!(
                "<dict><key>WorkspacePath</key><string>{}/ios/Runner.xcworkspace</string></dict>",
                workspace.display()
            )
            .as_bytes(),
        );
        let theirs = dir.join("DerivedData/Personal-def");
        write(
            &theirs.join("info.plist"),
            b"<dict><key>WorkspacePath</key><string>/Users/me/app/ios/Runner.xcworkspace</string></dict>",
        );

        let stats = caches.purge(42, 50).unwrap();
        assert_eq!(stats.purged_at, Some(42));
        assert_eq!(caches.purged_at(), Some(42));
        assert!(!dir.join("caches/gradle/caches").exists());
        assert!(dir.join("caches/gradle").is_dir());
        assert!(!ours.exists());
        assert!(theirs.exists());
        let after = caches.observe(&workspace);
        assert!(
            after
                .lines
                .iter()
                .any(|line| line.starts_with("  pub: miss"))
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn host_gradle_properties_follow_builds_into_the_runner_gradle_home() {
        let dir = temp_dir("gradle-props");
        let mut caches = caches(&dir, GIB);
        let host = dir.join("host-gradle");
        caches.host_gradle_home = Some(host.clone());
        let workspace = dir.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let mirrored = dir.join("caches/gradle/gradle.properties");

        let props = b"systemProp.https.proxyHost=proxy.office\nsystemProp.https.proxyPort=3128\n";
        write(&host.join("gradle.properties"), props);
        let observation = caches.observe(&workspace);
        assert_eq!(fs::read(&mirrored).unwrap(), props);
        assert!(
            observation
                .lines
                .iter()
                .any(|line| line.starts_with("  gradle: using settings from"))
        );
        assert!(
            !fs::symlink_metadata(&mirrored)
                .unwrap()
                .file_type()
                .is_symlink()
        );

        fs::remove_file(host.join("gradle.properties")).unwrap();
        caches.observe(&workspace);
        assert!(!mirrored.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bytes_are_formatted_for_the_build_log() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(3 * GIB / 2), "1.5 GB");
    }
}
