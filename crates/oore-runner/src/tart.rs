//! Per-build macOS VMs with Tart (`isolation: vm`).
//!
//! The host runner keeps the checkout, signing and artifact upload. For each
//! build it clones a VM from the image for the build's Flutter version, shares
//! the workspace into it over virtiofs, runs every step through `tart exec`
//! (the Tart Guest Agent, so no SSH credentials), and deletes the VM afterwards.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use anyhow::Context;
use tokio::io::AsyncWriteExt;

/// Oldest Tart release the runner drives. `tart exec` and the guest agent
/// protocol it speaks are only stable from here on.
pub(crate) const PINNED_TART_VERSION: &str = "2.41.1";
const TART_BIN_ENV: &str = "OORE_TART_BIN";
/// Image name or OCI reference; `{flutter}` becomes the build's Flutter version.
pub(crate) const IMAGE_TEMPLATE_ENV: &str = "OORE_TART_IMAGE";
pub(crate) const DEFAULT_IMAGE_TEMPLATE: &str = "oore-flutter-{flutter}";
const WORKSPACE_TAG: &str = "oore-workspace";
/// Where the host workspace share is mounted, relative to the guest home.
const GUEST_SHARE_DIR: &str = "oore-workspace-share";
/// Where the steps run: a copy of the workspace on the guest disk, at a fixed
/// path so Xcode's DerivedData and Gradle see the same project path on every
/// build. Building on the virtiofs share directly is slow, and tools that write
/// a file and read it straight back (Gradle's binary stores, Xcode's build
/// database) do not get consistent results from it.
const GUEST_WORKSPACE_DIR: &str = "oore-workspace";
/// Apple's Virtualization framework runs at most two macOS guests per host.
pub(crate) const MAX_MACOS_VMS: usize = 2;
const BOOT_TIMEOUT: Duration = Duration::from_secs(180);
/// One `tart exec <vm> true` probe while the guest boots. A wedged agent must
/// not hold the boot past [`BOOT_TIMEOUT`].
const BOOT_PROBE_TIMEOUT: Duration = Duration::from_secs(15);
/// Tart queries such as `tart list` and `tart --version` answer in seconds.
const TART_QUERY_TIMEOUT: Duration = Duration::from_secs(60);
/// `tart clone` is an APFS clone of a local disk, but a registry image is
/// copied out of Tart's OCI cache first.
const TART_CLONE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// Short commands in the guest: the home probe, mounts, stopping daemons.
const VM_EXEC_TIMEOUT: Duration = Duration::from_secs(90);
/// Copying the workspace or a cache in, or a cache back out, over virtiofs.
/// Multi-GB Gradle caches and DerivedData take minutes.
const VM_COPY_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const VM_NAME_PREFIX: &str = "oore";
const TART_RUN_STDERR_LIMIT: usize = 16 * 1024;

pub(crate) fn find_tart() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(TART_BIN_ENV) {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    // Launchd services get a minimal PATH, so look in Homebrew's prefixes first.
    let fixed = ["/opt/homebrew/bin/tart", "/usr/local/bin/tart"]
        .into_iter()
        .map(PathBuf::from);
    let from_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .map(|dir| dir.join("tart"));
    fixed.chain(from_path).find(|path| path.is_file())
}

/// Blocks for up to [`TART_QUERY_TIMEOUT`]; call it off the async runtime.
pub(crate) fn tart_version(tart: &Path) -> Option<String> {
    let version = run_tart_blocking(tart, &["--version"], TART_QUERY_TIMEOUT).ok()?;
    let version = version.trim().to_string();
    (!version.is_empty()).then_some(version)
}

fn parse_version(raw: &str) -> Option<(u64, u64, u64)> {
    let mut parts = raw.trim().trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts
        .next()
        .and_then(|patch| patch.split(|c: char| !c.is_ascii_digit()).next())
        .unwrap_or("0")
        .parse()
        .ok()?;
    Some((major, minor, patch))
}

pub(crate) fn version_is_supported(version: &str) -> bool {
    match (parse_version(version), parse_version(PINNED_TART_VERSION)) {
        (Some(found), Some(pinned)) => found >= pinned,
        _ => false,
    }
}

/// Image every built Flutter image starts from: Xcode, CocoaPods, a JDK, the
/// Android SDK and the Tart Guest Agent are already in it. About 69 GB to
/// download.
pub(crate) const BASE_IMAGE_ENV: &str = "OORE_TART_BASE_IMAGE";
pub(crate) const DEFAULT_BASE_IMAGE: &str = "ghcr.io/cirruslabs/macos-tahoe-xcode:26.5";
/// Base for builds that need no Xcode: macOS with Homebrew and the Tart Guest
/// Agent, about 26 GB. The provision script adds a JDK and the Android SDK.
/// Pinned by digest because the repository publishes only `latest`.
pub(crate) const ANDROID_BASE_IMAGE: &str = "ghcr.io/cirruslabs/macos-tahoe-base@sha256:4aaa751fa7538902d73c767f27a066fc87e83f183069da4e4b5022863abd4c77";

/// The base for a build. `OORE_TART_BASE_IMAGE` wins; otherwise only builds
/// for iOS or macOS pay for the Xcode image.
pub(crate) fn base_image(needs_xcode: bool) -> String {
    std::env::var(BASE_IMAGE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if needs_xcode {
                DEFAULT_BASE_IMAGE
            } else {
                ANDROID_BASE_IMAGE
            }
            .to_string()
        })
}

/// OCI references contain a registry path; Tart pulls those itself.
pub(crate) fn is_registry_reference(image: &str) -> bool {
    image.contains('/')
}

/// The image a build runs in, and how to build it when it is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImagePlan {
    pub name: String,
    pub base: String,
    pub flutter_version: String,
    /// Android only: no iOS precache, and the image is named apart from the
    /// Xcode one for the same Flutter version.
    pub android_only: bool,
    pub config: Option<oore_contract::VmImageConfig>,
}

/// Picks the image for a build. A repository's `oore.image.yaml` makes the
/// image its project's own: the name carries a hash of the project, base,
/// Flutter version and file, so a changed file builds a new image.
pub(crate) fn plan_image(
    template: &str,
    base: &str,
    flutter_version: &str,
    android_only: bool,
    project_id: &str,
    image_config: Option<(&str, oore_contract::VmImageConfig)>,
) -> anyhow::Result<ImagePlan> {
    let mut name = resolve_image(template, flutter_version)?;
    let Some((raw, config)) = image_config else {
        if android_only && !is_registry_reference(&name) {
            name.push_str("-android");
        }
        return Ok(ImagePlan {
            name,
            base: base.to_string(),
            flutter_version: flutter_version.to_string(),
            android_only,
            config: None,
        });
    };
    let base = config.base.clone().unwrap_or_else(|| base.to_string());
    let platforms = if android_only { "android" } else { "all" };
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    for part in [project_id, base.as_str(), flutter_version, platforms, raw] {
        sha2::Digest::update(&mut hasher, part.as_bytes());
        sha2::Digest::update(&mut hasher, [0]);
    }
    let digest = hex::encode(sha2::Digest::finalize(hasher));
    Ok(ImagePlan {
        name: format!("oore-flutter-{flutter_version}-{}", &digest[..12]),
        base,
        flutter_version: flutter_version.to_string(),
        android_only,
        config: Some(config),
    })
}

const FLUTTER_CHANNELS: [&str; 4] = ["stable", "beta", "main", "master"];

/// Installs a JDK and the Android SDK into a base without them, and records
/// both in the login profile every build step reads.
const ANDROID_TOOLCHAIN_SCRIPT: &str = r#"if ! /usr/libexec/java_home >/dev/null 2>&1 && [ -z "$JAVA_HOME" ]; then
  brew install openjdk@17
  echo "export JAVA_HOME=\"$(brew --prefix openjdk@17)/libexec/openjdk.jdk/Contents/Home\"" >> "$HOME/.zprofile"
  echo 'export PATH="$JAVA_HOME/bin:$PATH"' >> "$HOME/.zprofile"
fi
if [ -z "$ANDROID_HOME" ] && [ -z "$ANDROID_SDK_ROOT" ]; then
  brew install --cask android-commandlinetools
  echo "export ANDROID_HOME=\"$(brew --prefix)/share/android-commandlinetools\"" >> "$HOME/.zprofile"
  echo 'export PATH="$ANDROID_HOME/platform-tools:$PATH"' >> "$HOME/.zprofile"
fi
source "$HOME/.zprofile"
sdkmanager="$(command -v sdkmanager || echo "${ANDROID_HOME:-$ANDROID_SDK_ROOT}/cmdline-tools/latest/bin/sdkmanager")"
yes | "$sdkmanager" --licenses > /dev/null || true
"$sdkmanager" 'platform-tools' 'platforms;android-36' 'build-tools;36.0.0'
"#;

/// Script that turns a base image into one for `plan.flutter_version`.
pub(crate) fn provision_script(plan: &ImagePlan) -> String {
    let version = shell_quote(&plan.flutter_version);
    let checkout = if FLUTTER_CHANNELS.contains(&plan.flutter_version.as_str()) {
        format!("git checkout {version}\ngit pull --ff-only origin {version}")
    } else {
        format!("git fetch --no-tags origin tag {version}\ngit checkout --detach {version}")
    };
    let precache = if plan.android_only {
        "--android"
    } else {
        "--ios --android"
    };
    let mut script = String::from("set -e\n");
    if plan.android_only {
        // The Xcode image ships a JDK and the Android SDK; the slim base does
        // not. Skip what the base already has, so an overridden base works too.
        script.push_str(ANDROID_TOOLCHAIN_SCRIPT);
    }
    script.push_str(&format!(
        r#"if [ ! -d "$HOME/flutter/.git" ]; then
  git clone https://github.com/flutter/flutter.git "$HOME/flutter"
fi
cd "$HOME/flutter"
{checkout}
grep -q 'flutter/bin' "$HOME/.zprofile" 2>/dev/null || echo 'export PATH="$HOME/flutter/bin:$PATH"' >> "$HOME/.zprofile"
export PATH="$HOME/flutter/bin:$PATH"
flutter config --no-analytics
flutter precache {precache}
yes | flutter doctor --android-licenses > /dev/null || true
"#
    ));
    if let Some(config) = &plan.config {
        if !config.brew.is_empty() {
            let packages = config
                .brew
                .iter()
                .map(|p| shell_quote(p))
                .collect::<Vec<_>>();
            script.push_str(&format!("brew install {}\n", packages.join(" ")));
        }
        if !config.android_packages.is_empty() {
            let packages = config
                .android_packages
                .iter()
                .map(|p| shell_quote(p))
                .collect::<Vec<_>>();
            script.push_str(&format!("yes | sdkmanager {}\n", packages.join(" ")));
        }
        for command in &config.setup {
            script.push_str(&format!("/bin/sh -c {}\n", shell_quote(command)));
        }
    }
    script.push_str("cd \"$HOME\"\nflutter --version\n");
    script
}

async fn local_vms(tart: &Path) -> anyhow::Result<Vec<String>> {
    let listing = run_tart(
        tart,
        &["list", "--source", "local", "--quiet"],
        TART_QUERY_TIMEOUT,
    )
    .await?;
    Ok(listing
        .lines()
        .map(|line| line.trim().to_string())
        .collect())
}

pub(crate) async fn local_image_exists(tart: &Path, name: &str) -> anyhow::Result<bool> {
    Ok(local_vms(tart).await?.iter().any(|line| line == name))
}

/// Whether Tart's OCI cache already holds the registry image `reference`.
pub(crate) async fn registry_image_cached(tart: &Path, reference: &str) -> anyhow::Result<bool> {
    let listing = run_tart(
        tart,
        &["list", "--source", "oci", "--quiet"],
        TART_QUERY_TIMEOUT,
    )
    .await?;
    Ok(listing.lines().any(|line| line.trim() == reference))
}

/// Starts `tart pull` for a registry image. `CI` makes Tart print its
/// percentage on a new line about once a second instead of redrawing one
/// line, so the progress streams into a build log.
pub(crate) fn spawn_pull(tart: &Path, reference: &str) -> anyhow::Result<tokio::process::Child> {
    tokio::process::Command::new(tart)
        .args(["pull", reference])
        .env("CI", "true")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("failed to start tart pull")
}

/// Serialises image builds across this host's runners, so two builds that need
/// the same missing image build it once.
pub(crate) fn try_lock_image(dir: &Path, image: &str) -> anyhow::Result<Option<fs::File>> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("image-{}.lock", name_component(image)));
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(error)) => {
            Err(error).with_context(|| format!("failed to lock {}", path.display()))
        }
    }
}

/// Maps the build's Flutter version to a Tart image through the template.
pub(crate) fn resolve_image(template: &str, flutter_version: &str) -> anyhow::Result<String> {
    anyhow::ensure!(
        !flutter_version.is_empty()
            && flutter_version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')),
        "Flutter version '{flutter_version}' cannot name a VM image; use a version such as 3.47.7"
    );
    let template = template.trim();
    anyhow::ensure!(
        !template.is_empty(),
        "{IMAGE_TEMPLATE_ENV} must not be empty"
    );
    Ok(template.replace("{flutter}", flutter_version))
}

pub(crate) fn image_template() -> String {
    std::env::var(IMAGE_TEMPLATE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_IMAGE_TEMPLATE.to_string())
}

fn name_component(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

fn runner_vm_prefix(runner_id: &str) -> String {
    format!("{VM_NAME_PREFIX}-{}-", name_component(runner_id))
}

pub(crate) fn vm_name(runner_id: &str, build_id: &str) -> String {
    format!(
        "{}{}",
        runner_vm_prefix(runner_id),
        name_component(build_id)
    )
}

/// A held macOS VM slot. Slots are lock files shared by every runner process
/// of this user on the host, so together they never start more than
/// [`MAX_MACOS_VMS`] guests.
pub(crate) struct VmSlot {
    _lock: fs::File,
    pub(crate) index: usize,
}

fn tart_home() -> anyhow::Result<PathBuf> {
    Ok(match std::env::var_os("TART_HOME") {
        Some(home) => PathBuf::from(home),
        None => PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join(".tart"),
    })
}

pub(crate) fn slot_dir() -> anyhow::Result<PathBuf> {
    Ok(tart_home()?.join("oore-slots"))
}

pub(crate) fn try_acquire_slot(dir: &Path, slots: usize) -> anyhow::Result<Option<VmSlot>> {
    fs::create_dir_all(dir)
        .with_context(|| format!("failed to create VM slot directory {}", dir.display()))?;
    for index in 0..slots {
        let path = dir.join(format!("macos-{index}.lock"));
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .with_context(|| format!("failed to open VM slot {}", path.display()))?;
        match file.try_lock() {
            Ok(()) => return Ok(Some(VmSlot { _lock: file, index })),
            Err(fs::TryLockError::WouldBlock) => continue,
            Err(fs::TryLockError::Error(error)) => {
                return Err(error).with_context(|| format!("failed to lock {}", path.display()));
            }
        }
    }
    Ok(None)
}

/// Runs `tart` with `args`, killing it when it outlives `timeout`.
async fn run_tart(tart: &Path, args: &[&str], timeout: Duration) -> anyhow::Result<String> {
    let output = tokio::process::Command::new(tart)
        .args(args)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(timeout, output)
        .await
        .map_err(|_| tart_timed_out(args, timeout))?
        .with_context(|| format!("failed to run tart {}", args.join(" ")))?;
    tart_result(args, output.status, &output.stdout, &output.stderr)
}

/// [`run_tart`] for code that cannot await, such as `Drop`.
fn run_tart_blocking(tart: &Path, args: &[&str], timeout: Duration) -> anyhow::Result<String> {
    let mut child = Command::new(tart)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run tart {}", args.join(" ")))?;
    let stdout = read_in_background(child.stdout.take());
    let stderr = read_in_background(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(tart_timed_out(args, timeout));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    // A process tart left behind may still hold a pipe open; do not wait on it.
    let stdout = stdout
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_default();
    let stderr = stderr
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_default();
    tart_result(args, status, &stdout, &stderr)
}

fn read_in_background(pipe: Option<impl Read + Send + 'static>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    if let Some(mut pipe) = pipe {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = pipe.read_to_end(&mut buffer);
            let _ = tx.send(buffer);
        });
    }
    rx
}

fn tart_timed_out(args: &[&str], timeout: Duration) -> anyhow::Error {
    anyhow::anyhow!(
        "tart {} did not finish within {}s and was stopped",
        args.first().copied().unwrap_or_default(),
        timeout.as_secs()
    )
}

fn tart_result(
    args: &[&str],
    status: std::process::ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
) -> anyhow::Result<String> {
    if status.success() {
        return Ok(String::from_utf8_lossy(stdout).into_owned());
    }
    anyhow::bail!(
        "tart {} failed: {}",
        args.first().copied().unwrap_or_default(),
        String::from_utf8_lossy(stderr).trim()
    )
}

/// Mounts each virtiofs `(tag, guest path)` share. `tart exec` runs without a
/// login shell, so its PATH has no `/sbin` and the mount tool needs its full path.
fn mount_script(mounts: &[(String, String)]) -> String {
    mounts
        .iter()
        .map(|(tag, path)| {
            let path = shell_quote(path);
            format!("mkdir -p {path} && /sbin/mount_virtiofs {tag} {path}")
        })
        .collect::<Vec<_>>()
        .join(" && ")
}

/// Copies the tree at `from` into `to`, creating `to` if needed. Files that
/// only exist in `to` are kept. rsync's partial-transfer statuses (23 when an
/// attribute could not be set, 24 when a file vanished mid-copy) are accepted:
/// the share cannot take every attribute, and a build tool's daemon may still
/// be writing. A missing artifact is caught by whoever reads the copy.
fn copy_tree_script(from: &str, to: &str, excludes: &[&str]) -> String {
    let excludes = excludes
        .iter()
        .map(|pattern| format!(" --exclude {}", shell_quote(pattern)))
        .collect::<String>();
    let from = shell_quote(from);
    let to = shell_quote(to);
    format!(
        "mkdir -p {to} && {{ /usr/bin/rsync -a{excludes} {from}/ {to}/; s=$?; [ $s -eq 0 ] || [ $s -eq 23 ] || [ $s -eq 24 ]; }}"
    )
}

/// Copies the host workspace share into the guest build directory.
fn workspace_in_script(share: &str, workspace: &str) -> String {
    copy_tree_script(share, workspace, &[])
}

/// Copies everything the steps produced back to the share, where the host
/// collects artifacts and test reports. The build does not change `.git`.
fn workspace_out_script(workspace: &str, share: &str) -> String {
    copy_tree_script(workspace, share, &[".git"])
}

/// Gradle reads its temp and binary store files back in ways virtiofs does not
/// keep consistent ("Problems reading data from Binary store"), so a VM build
/// runs Gradle from a home on the guest disk. The parts of the home that only
/// ever gain complete entries are copied in from the share and back out after:
/// downloaded dependencies, transformed artifacts, the build cache and wrapper
/// distributions.
const GRADLE_SHARE: &str = "gradle";
const GUEST_GRADLE_HOME: &str = ".gradle";
const GRADLE_SYNCED_DIRS: [&str; 5] = [
    "caches/modules-2",
    "caches/transforms-*",
    "caches/jars-*",
    "caches/build-cache-1",
    "wrapper/dists",
];

/// Copies the reusable parts of the Gradle home at `from` into `to`. The
/// directory names are globs, as Gradle versions its cache directories.
fn gradle_sync_script(from: &str, to: &str) -> String {
    let to = shell_quote(to);
    format!(
        "cd {} 2>/dev/null || exit 0; for dir in {}; do [ -d \"$dir\" ] || continue; \
mkdir -p {to}/\"$dir\" && {{ /usr/bin/rsync -a --exclude '*.lock' --exclude '*.lck' \"$dir\"/ {to}/\"$dir\"/; s=$?; [ $s -eq 0 ] || [ $s -eq 23 ] || [ $s -eq 24 ]; }} || exit 1; done",
        shell_quote(from),
        GRADLE_SYNCED_DIRS.join(" "),
    )
}

/// Gives the guest Gradle home the host's `gradle.properties`, as a host
/// build gets, and turns Gradle's build cache on unless that file decides.
/// Every build starts from a fresh checkout, so task outputs are only reused
/// through the build cache.
fn gradle_properties_script(shared: &str, local: &str) -> String {
    let source = shell_quote(&format!("{shared}/gradle.properties"));
    let target = shell_quote(&format!("{local}/gradle.properties"));
    format!(
        "mkdir -p {local} && if [ -f {source} ]; then cp {source} {target}; fi && {{ grep -qs '^[[:space:]]*org\\.gradle\\.caching[[:space:]]*[=:]' {target} || echo 'org.gradle.caching=true' >> {target}; }}",
        local = shell_quote(local),
    )
}

/// Xcode's DerivedData, kept per project on the host. Its build database is
/// SQLite, which needs locking the share does not give, so Xcode works in its
/// default folder on the guest disk and the folder is copied in and back out.
/// Swift packages Xcode resolved (`SourcePackages`) come back with it, so a
/// build does not clone them again.
const DERIVED_DATA_SHARE: &str = "derived-data";
const GUEST_DERIVED_DATA: &str = "Library/Developer/Xcode/DerivedData";

/// The saved DerivedData lives in one of two folders under the share, and
/// `current` names the complete one. A save mirrors the guest's folder into
/// the other one and only then switches `current`, so a save that is cut off
/// leaves the last complete copy in place. Mirroring over the older copy
/// keeps a save incremental.
const DERIVED_DATA_CURRENT: &str = "current";

/// Saves the guest's DerivedData at `from` to the share at `to`. Also drops
/// what the share held before it had two folders.
fn derived_data_save_script(from: &str, to: &str) -> String {
    let from = shell_quote(from);
    let to = shell_quote(to);
    format!(
        "[ -d {from} ] || exit 0; cur=$(cat {to}/{DERIVED_DATA_CURRENT} 2>/dev/null); if [ \"$cur\" = a ]; then next=b; else next=a; fi; \
mkdir -p {to}/\"$next\" && {{ /usr/bin/rsync -a --delete {from}/ {to}/\"$next\"/; s=$?; [ $s -eq 0 ] || [ $s -eq 23 ] || [ $s -eq 24 ]; }} || exit 1; \
printf %s \"$next\" > {to}/.{DERIVED_DATA_CURRENT}.tmp && mv {to}/.{DERIVED_DATA_CURRENT}.tmp {to}/{DERIVED_DATA_CURRENT} || exit 1; \
for entry in {to}/* {to}/.[!.]*; do case \"${{entry##*/}}\" in a|b|{DERIVED_DATA_CURRENT}|'.[!.]*'|'*') ;; *) rm -rf \"$entry\";; esac; done"
    )
}

/// Copies the complete saved DerivedData at `from` to the guest folder `to`,
/// printing `restored` when there was one.
fn derived_data_restore_script(from: &str, to: &str) -> String {
    let from = shell_quote(from);
    let to = shell_quote(to);
    format!(
        "cur=$(cat {from}/{DERIVED_DATA_CURRENT} 2>/dev/null); case \"$cur\" in a|b) ;; *) exit 0;; esac; [ -d {from}/\"$cur\" ] || exit 0; \
mkdir -p {to} && {{ /usr/bin/rsync -a --delete {from}/\"$cur\"/ {to}/; s=$?; [ $s -eq 0 ] || [ $s -eq 23 ] || [ $s -eq 24 ]; }} && echo restored"
    )
}

/// Android SDK packages a build installs (Gradle fetches the platforms, build
/// tools, NDK and CMake a project asks for) would land on the guest disk and
/// be downloaded again by the next build. Packages the image does not have are
/// saved to the share by `sdkmanager` path, such as `platforms/android-34`,
/// and copied into later VMs. Each package is copied under a hidden name and
/// renamed when complete, so an interrupted copy is never taken for a package.
const ANDROID_SDK_SHARE: &str = "android-sdk";
const ANDROID_SDK_PARENTS: &str = "platforms build-tools ndk cmake";
/// Lists the image's own packages, so only packages a build added are saved.
const ANDROID_SDK_BASELINE: &str = ".oore-android-sdk-image";

/// Copies `$src` to `$parent/$name` under `dest_root` atomically.
fn android_package_copy(dest_root: &str) -> String {
    format!(
        "tmp={dest_root}/\"$parent/.$name.partial\"; rm -rf \"$tmp\" && mkdir -p {dest_root}/\"$parent\" && {{ /usr/bin/rsync -a \"$src\"/ \"$tmp\"/; s=$?; [ $s -eq 0 ] || [ $s -eq 23 ] || [ $s -eq 24 ]; }} && mv \"$tmp\" {dest_root}/\"$parent/$name\""
    )
}

/// Records the image's packages, then copies in every saved package the image
/// lacks. Prints the `sdkmanager` name of each restored package.
fn android_sdk_in_script(share: &str, sdk: &str, baseline: &str) -> String {
    let share = shell_quote(share);
    let sdk = shell_quote(sdk);
    format!(
        "cd {sdk} || exit 0; : > {baseline}; for parent in {ANDROID_SDK_PARENTS}; do for p in \"$parent\"/*; do [ -d \"$p\" ] && echo \"$p\" >> {baseline}; done; for src in {share}/\"$parent\"/*; do [ -d \"$src\" ] || continue; name=\"${{src##*/}}\"; [ -e \"$parent/$name\" ] && continue; {} && echo \"$parent;$name\"; done; done; exit 0",
        android_package_copy(&sdk),
        baseline = shell_quote(baseline),
    )
}

/// Saves every package the image did not have and the share does not hold yet.
fn android_sdk_out_script(sdk: &str, share: &str, baseline: &str) -> String {
    let share = shell_quote(share);
    let sdk = shell_quote(sdk);
    format!(
        "cd {sdk} && [ -f {baseline} ] || exit 0; for parent in {ANDROID_SDK_PARENTS}; do for src in \"$parent\"/*; do [ -d \"$src\" ] || continue; name=\"${{src##*/}}\"; grep -qxF \"$src\" {baseline} && continue; [ -e {share}/\"$src\" ] && continue; {} || exit 1; done; done",
        android_package_copy(&share),
        baseline = shell_quote(baseline),
    )
}

/// What one sweep for leftover VMs did.
#[derive(Debug, Default)]
pub(crate) struct VmSweep {
    pub removed: usize,
    pub errors: Vec<String>,
}

/// Deletes VMs this runner left behind: after a crash, a kill mid-build, or a
/// teardown whose delete failed. One VM that cannot be deleted does not keep
/// the others. Run it only while this runner has no build.
pub(crate) async fn delete_stale_vms(tart: &Path, runner_id: &str) -> anyhow::Result<VmSweep> {
    let prefix = runner_vm_prefix(runner_id);
    let mut sweep = VmSweep::default();
    for name in local_vms(tart).await? {
        if !name.starts_with(&prefix) {
            continue;
        }
        let _ = run_tart(tart, &["stop", "--timeout", "5", &name], TART_QUERY_TIMEOUT).await;
        match run_tart(tart, &["delete", &name], TART_QUERY_TIMEOUT).await {
            Ok(_) => sweep.removed += 1,
            Err(error) => sweep.errors.push(format!("{name}: {error:#}")),
        }
    }
    Ok(sweep)
}

/// Local images the runner builds; only these are pruned.
const BUILT_IMAGE_PREFIX: &str = "oore-flutter-";
/// Days an image the runner built may go unused before it is deleted.
pub(crate) const IMAGE_MAX_AGE_ENV: &str = "OORE_TART_IMAGE_MAX_AGE_DAYS";
const DEFAULT_IMAGE_MAX_AGE_DAYS: u64 = 30;

/// How long a built image may go unused; `None` turns pruning off.
pub(crate) fn image_max_age() -> Option<Duration> {
    let days = std::env::var(IMAGE_MAX_AGE_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_IMAGE_MAX_AGE_DAYS);
    (days > 0).then(|| Duration::from_secs(days * 24 * 60 * 60))
}

fn image_use_path(dir: &Path, image: &str) -> PathBuf {
    dir.join(format!("image-{}.used", name_component(image)))
}

fn read_image_use(dir: &Path, image: &str) -> Option<i64> {
    fs::read_to_string(image_use_path(dir, image))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn write_image_use(dir: &Path, image: &str, now: i64) -> anyhow::Result<()> {
    fs::create_dir_all(dir)?;
    let path = image_use_path(dir, image);
    fs::write(&path, now.to_string()).with_context(|| format!("failed to write {}", path.display()))
}

/// Records that a build is about to use `image`, so pruning keeps it. Waits
/// briefly for the image's lock, so a prune that is deleting the image right
/// now finishes first and the build then finds the image missing and builds
/// it again. An image build holds the same lock for much longer; then the
/// image does not exist yet and there is nothing to prune.
pub(crate) async fn record_image_use(dir: &Path, image: &str, now: i64) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let _lock = loop {
        if let Some(lock) = try_lock_image(dir, image)? {
            break Some(lock);
        }
        if tokio::time::Instant::now() >= deadline {
            break None;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    write_image_use(dir, image, now)
}

/// What one prune of unused images did.
#[derive(Debug, Default)]
pub(crate) struct ImagePrune {
    pub deleted: Vec<String>,
    pub errors: Vec<String>,
    /// The images the runner built that remain, with their size on disk.
    pub kept_bytes: u64,
    pub kept: usize,
}

/// Deletes local images the runner built that no build used for `max_age`.
/// An image with no record yet starts its clock now. Images a build is
/// creating are skipped through their lock.
pub(crate) async fn prune_unused_images(
    tart: &Path,
    dir: &Path,
    vms_dir: Option<&Path>,
    max_age: Option<Duration>,
    now: i64,
) -> anyhow::Result<ImagePrune> {
    let mut prune = ImagePrune::default();
    for image in local_vms(tart).await? {
        if !image.starts_with(BUILT_IMAGE_PREFIX) {
            continue;
        }
        let mut delete = false;
        if let Some(max_age) = max_age
            && let Some(_lock) = try_lock_image(dir, &image)?
        {
            match read_image_use(dir, &image) {
                Some(used) => {
                    delete = now.saturating_sub(used) > max_age.as_secs() as i64;
                }
                None => write_image_use(dir, &image, now)?,
            }
            if delete {
                match run_tart(tart, &["delete", &image], TART_QUERY_TIMEOUT).await {
                    Ok(_) => {
                        let _ = fs::remove_file(image_use_path(dir, &image));
                        prune.deleted.push(image.clone());
                    }
                    Err(error) => {
                        delete = false;
                        prune.errors.push(format!("{image}: {error:#}"));
                    }
                }
            }
        }
        if !delete {
            prune.kept += 1;
            if let Some(vms_dir) = vms_dir {
                prune.kept_bytes += crate::caches::allocated_size(&vms_dir.join(&image));
            }
        }
    }
    Ok(prune)
}

/// Where Tart keeps local VMs and images.
pub(crate) fn local_vms_dir() -> Option<PathBuf> {
    tart_home().ok().map(|home| home.join("vms"))
}

/// Single-quotes `value` for a POSIX shell.
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn is_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The script fed to the guest login shell on stdin. Values travel over the
/// agent's channel only, so secrets never land on the shared disk or in argv.
pub(crate) fn guest_step_script(
    guest_workspace: &str,
    command: &str,
    env: &[(String, String)],
) -> anyhow::Result<String> {
    let mut script = String::from("set -e\n");
    script.push_str(&format!("cd {}\n", shell_quote(guest_workspace)));
    for (key, value) in env {
        anyhow::ensure!(is_env_key(key), "invalid environment variable name '{key}'");
        script.push_str(&format!("export {key}={}\n", shell_quote(value)));
    }
    // Same shell semantics as host mode, which runs `/bin/sh -c`.
    script.push_str(&format!("exec /bin/sh -c {}\n", shell_quote(command)));
    Ok(script)
}

/// Wraps a script fed to a shell on stdin in one brace group, so the shell
/// reads all of it before running any of it, and its commands read
/// /dev/null. Otherwise a command that reads stdin, such as `brew install`,
/// swallows the rest of the script and the shell exits 0 at EOF.
fn stdin_script(script: &str) -> String {
    format!("{{\n{script}\n}} </dev/null\n")
}

/// A host directory shared into the guest, such as a dependency cache.
pub(crate) struct HostShare {
    pub name: &'static str,
    /// Variable to point at the guest path. Shares without one are copied to
    /// where the tool looks (DerivedData, Android SDK packages).
    pub env: Option<&'static str>,
    pub host_dir: PathBuf,
}

fn share_tag(name: &str) -> String {
    format!("oore-cache-{}", name_component(name))
}

/// Guest path for a share, relative to the guest home.
fn share_guest_dir(share: &HostShare) -> String {
    format!("oore-cache/{}", name_component(share.name))
}

/// Gradle and Kotlin daemons a build started outlive its steps and keep
/// writing the Gradle home. The brackets keep the patterns from matching the
/// stopping script itself.
const GRADLE_DAEMON_PATTERN: &str = "[G]radleDaemon|[K]otlinCompileDaemon";

/// Stops the processes whose command line matches `pattern` and fails if they
/// do not exit, so a cache is saved only once nothing writes to it. Prints
/// how many were running.
fn stop_processes_script(pattern: &str) -> String {
    format!(
        "p={}; n=$(pgrep -f \"$p\" | wc -l | tr -d ' '); [ \"$n\" -eq 0 ] && {{ echo 0; exit 0; }}; \
pkill -TERM -f \"$p\"; i=0; while pgrep -f \"$p\" >/dev/null && [ $i -lt 30 ]; do sleep 1; i=$((i+1)); done; \
pkill -KILL -f \"$p\"; sleep 1; if pgrep -f \"$p\" >/dev/null; then exit 1; fi; echo \"$n\"",
        shell_quote(pattern)
    )
}

pub(crate) struct TartVm {
    tart: PathBuf,
    name: String,
    run: Option<std::process::Child>,
    run_stderr: Arc<Mutex<String>>,
    guest_workspace: String,
    /// The workspace share's mount point; empty without a workspace.
    guest_share: String,
    guest_env: Vec<(String, String)>,
    /// The Gradle share and the guest-local Gradle home, saved by `save_caches`.
    gradle_sync: Option<(String, String)>,
    /// The DerivedData share and Xcode's folder, saved by `save_caches`.
    derived_data_sync: Option<(String, String)>,
    /// The Android SDK share, the guest SDK and its baseline, saved by `save_caches`.
    android_sdk_sync: Option<(String, String, String)>,
    /// Build log lines on what the guest got back from earlier builds.
    cache_notes: Vec<String>,
    /// Set once the VM was deleted or kept as an image; `Drop` does nothing then.
    released: bool,
    /// Bounds each copy into or out of the guest.
    copy_timeout: Duration,
}

impl TartVm {
    /// Clones `image`, boots it headless with `workspace` shared, and waits
    /// until the guest agent answers and the workspace is mounted. The caller
    /// holds a [`VmSlot`] for as long as the VM lives, and ends the VM with
    /// [`TartVm::delete`] or [`TartVm::keep_as`].
    pub(crate) async fn start(
        tart: PathBuf,
        image: &str,
        name: String,
        workspace: Option<&Path>,
        shares: &[HostShare],
    ) -> anyhow::Result<Self> {
        let mut dir_args = Vec::new();
        if let Some(workspace) = workspace {
            let workspace = workspace
                .to_str()
                .context("build workspace path is not valid UTF-8")?;
            dir_args.push(format!("--dir={workspace}:tag={WORKSPACE_TAG}"));
        }
        for share in shares {
            let host_dir = share
                .host_dir
                .to_str()
                .context("cache directory path is not valid UTF-8")?;
            fs::create_dir_all(&share.host_dir)
                .with_context(|| format!("failed to create {host_dir}"))?;
            dir_args.push(format!("--dir={host_dir}:tag={}", share_tag(share.name)));
        }
        for arg in &dir_args {
            // Tart splits `--dir` on ':'.
            anyhow::ensure!(
                arg.matches(':').count() == 1,
                "{arg} cannot be shared into a VM because its path contains ':'"
            );
        }
        run_tart(&tart, &["clone", image, &name], TART_CLONE_TIMEOUT)
            .await
            .with_context(|| {
                format!("failed to clone VM image {image}; build or pull it on this runner first")
            })?;

        let mut vm = Self {
            tart,
            name,
            run: None,
            run_stderr: Arc::new(Mutex::new(String::new())),
            guest_workspace: String::new(),
            guest_share: String::new(),
            guest_env: Vec::new(),
            gradle_sync: None,
            derived_data_sync: None,
            android_sdk_sync: None,
            cache_notes: Vec::new(),
            released: false,
            copy_timeout: VM_COPY_TIMEOUT,
        };
        match vm.boot(&dir_args, workspace.is_some(), shares).await {
            Ok(()) => Ok(vm),
            Err(error) => {
                if let Err(delete_error) = vm.delete().await {
                    eprintln!("Warning: {delete_error:#}");
                }
                Err(error)
            }
        }
    }

    async fn boot(
        &mut self,
        dir_args: &[String],
        has_workspace: bool,
        shares: &[HostShare],
    ) -> anyhow::Result<()> {
        let mut child = Command::new(&self.tart)
            .arg("run")
            .arg(&self.name)
            .args(["--no-graphics", "--no-audio", "--no-clipboard"])
            .args(dir_args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to start tart run")?;
        if let Some(mut stderr) = child.stderr.take() {
            let sink = self.run_stderr.clone();
            std::thread::spawn(move || {
                let mut buffer = [0_u8; 4096];
                while let Ok(read) = stderr.read(&mut buffer) {
                    if read == 0 {
                        break;
                    }
                    if let Ok(mut text) = sink.lock()
                        && text.len() < TART_RUN_STDERR_LIMIT
                    {
                        text.push_str(&String::from_utf8_lossy(&buffer[..read]));
                    }
                }
            });
        }
        self.run = Some(child);

        self.wait_for_agent().await?;
        let home = self
            .exec_capture("printf %s \"$HOME\"", VM_EXEC_TIMEOUT)
            .await?;
        anyhow::ensure!(
            home.starts_with('/'),
            "VM reported an unusable home directory '{home}'"
        );
        let home = home.trim_end_matches('/');
        if !has_workspace {
            self.guest_workspace = home.to_string();
            return Ok(());
        }
        let guest_workspace = format!("{home}/{GUEST_WORKSPACE_DIR}");
        let guest_share = format!("{home}/{GUEST_SHARE_DIR}");
        let mut mounts = vec![(WORKSPACE_TAG.to_string(), guest_share.clone())];
        let mut gradle_dirs = None;
        let mut derived_data_share = None;
        let mut android_sdk_share = None;
        for share in shares {
            let guest_dir = format!("{home}/{}", share_guest_dir(share));
            match share.name {
                DERIVED_DATA_SHARE => derived_data_share = Some(guest_dir.clone()),
                ANDROID_SDK_SHARE => android_sdk_share = Some(guest_dir.clone()),
                _ => {}
            }
            if let Some(env) = share.env {
                let value = if share.name == GRADLE_SHARE {
                    let local = format!("{home}/{GUEST_GRADLE_HOME}");
                    gradle_dirs = Some((guest_dir.clone(), local.clone()));
                    local
                } else {
                    guest_dir.clone()
                };
                self.guest_env.push((env.to_string(), value));
            }
            mounts.push((share_tag(share.name), guest_dir));
        }
        self.exec_capture(&mount_script(&mounts), VM_EXEC_TIMEOUT)
            .await
            .context("failed to mount the build workspace and caches inside the VM")?;
        self.exec_capture(
            &workspace_in_script(&guest_share, &guest_workspace),
            self.copy_timeout,
        )
        .await
        .context("failed to copy the workspace into the VM")?;
        if let Some((shared, local)) = gradle_dirs {
            self.exec_capture(&gradle_sync_script(&shared, &local), self.copy_timeout)
                .await
                .context("failed to copy the Gradle cache into the VM")?;
            self.exec_capture(&gradle_properties_script(&shared, &local), VM_EXEC_TIMEOUT)
                .await
                .context("failed to write gradle.properties inside the VM")?;
            self.gradle_sync = Some((shared, local));
        }
        if let Some(shared) = derived_data_share {
            let local = format!("{home}/{GUEST_DERIVED_DATA}");
            let restored = self
                .exec_capture(
                    &derived_data_restore_script(&shared, &local),
                    self.copy_timeout,
                )
                .await
                .context("failed to copy DerivedData into the VM")?;
            self.cache_notes.push(if restored.trim().is_empty() {
                "  derived data: none saved for this project yet; Xcode's is kept when the VM stops"
                    .to_string()
            } else {
                "  derived data: restored this project's DerivedData and Swift packages".to_string()
            });
            self.derived_data_sync = Some((shared, local));
        }
        if let Some(shared) = android_sdk_share {
            // The image sets the SDK location in the login profile.
            let sdk = self
                .exec_capture(
                    "/bin/zsh -l -c 'printf \"\\n%s\" \"${ANDROID_HOME:-$ANDROID_SDK_ROOT}\"'",
                    VM_EXEC_TIMEOUT,
                )
                .await
                .unwrap_or_default();
            let sdk = sdk.lines().last().unwrap_or_default().trim().to_string();
            if sdk.starts_with('/') {
                let baseline = format!("{home}/{ANDROID_SDK_BASELINE}");
                let restored = self
                    .exec_capture(
                        &android_sdk_in_script(&shared, &sdk, &baseline),
                        self.copy_timeout,
                    )
                    .await
                    .context("failed to copy Android SDK packages into the VM")?;
                let restored = restored
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>();
                if !restored.is_empty() {
                    self.cache_notes
                        .push(format!("  android sdk: restored {}", restored.join(", ")));
                }
                self.android_sdk_sync = Some((shared, sdk, baseline));
            }
        }
        self.guest_workspace = guest_workspace;
        self.guest_share = guest_share;
        Ok(())
    }

    /// Copies what the steps wrote in the guest workspace back to the host
    /// workspace. Runs after every step, so the host sees artifacts, archives
    /// and test reports as each step finishes.
    pub(crate) async fn sync_workspace_out(&self) -> anyhow::Result<()> {
        self.exec_capture(
            &workspace_out_script(&self.guest_workspace, &self.guest_share),
            self.copy_timeout,
        )
        .await
        .context("failed to copy the workspace out of the VM")?;
        Ok(())
    }

    /// Empties `dir` in the guest, creating it if needed.
    pub(crate) async fn reset_guest_dir(&self, dir: &str) -> anyhow::Result<()> {
        let dir = shell_quote(dir);
        self.exec_capture(&format!("rm -rf {dir} && mkdir -p {dir}"), VM_EXEC_TIMEOUT)
            .await?;
        Ok(())
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn guest_workspace(&self) -> &str {
        &self.guest_workspace
    }

    pub(crate) fn cache_notes(&self) -> &[String] {
        &self.cache_notes
    }

    /// Guest paths for the shared caches, keyed by the variable that names them.
    pub(crate) fn guest_env(&self) -> &[(String, String)] {
        &self.guest_env
    }

    fn run_exit_message(&mut self) -> Option<String> {
        let status = self.run.as_mut()?.try_wait().ok()??;
        let stderr = self
            .run_stderr
            .lock()
            .map(|text| text.trim().to_string())
            .unwrap_or_default();
        Some(format!("tart run exited with {status}: {stderr}"))
    }

    async fn wait_for_agent(&mut self) -> anyhow::Result<()> {
        let deadline = tokio::time::Instant::now() + BOOT_TIMEOUT;
        loop {
            if let Some(message) = self.run_exit_message() {
                anyhow::bail!("VM {} stopped while booting: {message}", self.name);
            }
            let probe = deadline
                .saturating_duration_since(tokio::time::Instant::now())
                .min(BOOT_PROBE_TIMEOUT);
            let ready = run_tart(&self.tart, &["exec", &self.name, "/usr/bin/true"], probe)
                .await
                .is_ok();
            if ready {
                return Ok(());
            }
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "VM {} did not answer within {}s; check that the image runs the Tart Guest Agent",
                self.name,
                BOOT_TIMEOUT.as_secs()
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    async fn exec_capture(&self, script: &str, timeout: Duration) -> anyhow::Result<String> {
        run_tart(
            &self.tart,
            &["exec", &self.name, "/bin/sh", "-c", script],
            timeout,
        )
        .await
        .map_err(|error| anyhow::anyhow!("command in VM failed: {error:#}"))
    }

    /// Spawns one build step in the guest. Output streams like a host step.
    pub(crate) async fn spawn_step(
        &self,
        command: &str,
        env: &[(String, String)],
    ) -> anyhow::Result<tokio::process::Child> {
        let script = guest_step_script(&self.guest_workspace, command, env)?;
        self.spawn_script(&script).await
    }

    /// Runs `script` in a guest login shell and collects its output instead
    /// of streaming it, for steps whose output must not reach the build log.
    pub(crate) async fn run_script(&self, script: &str) -> anyhow::Result<std::process::Output> {
        let child = self.spawn_script(script).await?;
        child
            .wait_with_output()
            .await
            .context("failed to wait for tart exec")
    }

    /// Feeds `script` to a guest login shell, which picks up the image's PATH
    /// (Flutter, Homebrew, Ruby).
    pub(crate) async fn spawn_script(&self, script: &str) -> anyhow::Result<tokio::process::Child> {
        let script = stdin_script(script);
        let mut child = tokio::process::Command::new(&self.tart)
            .args(["exec", "-i", &self.name, "/bin/zsh", "-l", "-s"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("failed to start tart exec")?;
        let mut stdin = child.stdin.take().context("tart exec has no stdin")?;
        stdin.write_all(script.as_bytes()).await?;
        stdin.shutdown().await?;
        drop(stdin);
        Ok(child)
    }

    /// Whether [`TartVm::save_caches`] has anything to save.
    pub(crate) fn has_cache_saves(&self) -> bool {
        self.gradle_sync.is_some()
            || self.derived_data_sync.is_some()
            || self.android_sdk_sync.is_some()
    }

    /// Copies the guest's Gradle cache, DerivedData and new Android SDK
    /// packages back to the host. Each copy is bounded, and the Gradle cache is
    /// saved only after the guest's Gradle daemons exited. Returns a build log
    /// line for each cache. Skip it for a canceled build, whose tools may have
    /// stopped mid-write.
    pub(crate) async fn save_caches(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some((shared, local)) = self.gradle_sync.take() {
            match self
                .exec_capture(
                    &stop_processes_script(GRADLE_DAEMON_PATTERN),
                    VM_EXEC_TIMEOUT,
                )
                .await
            {
                Ok(stopped) => {
                    let stopped = stopped.trim();
                    let note = if stopped == "0" {
                        String::new()
                    } else {
                        format!(", after stopping {stopped} Gradle daemon(s)")
                    };
                    lines.push(
                        self.save_cache("gradle cache", &gradle_sync_script(&local, &shared))
                            .await
                            + &note,
                    );
                }
                Err(error) => lines.push(format!(
                    "  gradle cache: not saved, the build's Gradle daemons did not stop: {error:#}"
                )),
            }
        }
        if let Some((shared, local)) = self.derived_data_sync.take() {
            lines.push(
                self.save_cache("derived data", &derived_data_save_script(&local, &shared))
                    .await,
            );
        }
        if let Some((shared, sdk, baseline)) = self.android_sdk_sync.take() {
            lines.push(
                self.save_cache(
                    "android sdk",
                    &android_sdk_out_script(&sdk, &shared, &baseline),
                )
                .await,
            );
        }
        lines
    }

    async fn save_cache(&self, what: &str, script: &str) -> String {
        let started = Instant::now();
        let result = self.exec_capture(script, self.copy_timeout).await;
        let seconds = started.elapsed().as_secs();
        match result {
            Ok(_) => format!("  {what}: saved in {seconds}s"),
            Err(error) => format!("  {what}: not saved after {seconds}s: {error:#}"),
        }
    }

    async fn stop(&mut self, timeout_secs: u64) {
        let timeout = timeout_secs.to_string();
        let _ = run_tart(
            &self.tart,
            &["stop", "--timeout", &timeout, &self.name],
            Duration::from_secs(timeout_secs) + TART_QUERY_TIMEOUT,
        )
        .await;
        if let Some(mut run) = self.run.take() {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
            while run.try_wait().ok().flatten().is_none() && tokio::time::Instant::now() < deadline
            {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            if run.try_wait().ok().flatten().is_none() {
                let _ = run.kill();
            }
            let _ = run.wait();
        }
    }

    /// Stops the VM and deletes it, each step bounded. A VM whose delete fails
    /// is left for the runner's next sweep.
    pub(crate) async fn delete(mut self) -> anyhow::Result<()> {
        self.released = true;
        self.stop(10).await;
        run_tart(&self.tart, &["delete", &self.name], TART_QUERY_TIMEOUT)
            .await
            .with_context(|| format!("failed to delete VM {}", self.name))?;
        Ok(())
    }

    /// Shuts the guest down cleanly and keeps its disk as the local image `image`.
    pub(crate) async fn keep_as(mut self, image: &str) -> anyhow::Result<()> {
        self.stop(120).await;
        run_tart(
            &self.tart,
            &["rename", &self.name, image],
            TART_QUERY_TIMEOUT,
        )
        .await?;
        self.released = true;
        Ok(())
    }
}

/// The last resort when a VM is dropped without [`TartVm::delete`], such as a
/// build future dropped on cancel: no caches are saved, and stopping and
/// deleting are bounded so a wedged Tart cannot hold the runner.
impl Drop for TartVm {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        let _ = run_tart_blocking(
            &self.tart,
            &["stop", "--timeout", "5", &self.name],
            Duration::from_secs(30),
        );
        if let Some(mut run) = self.run.take() {
            let _ = run.kill();
            let _ = run.wait();
        }
        if let Err(error) =
            run_tart_blocking(&self.tart, &["delete", &self.name], Duration::from_secs(60))
        {
            eprintln!("Warning: failed to delete VM {}: {error:#}", self.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_image_from_flutter_version() {
        assert_eq!(
            resolve_image(DEFAULT_IMAGE_TEMPLATE, "3.47.7").unwrap(),
            "oore-flutter-3.47.7"
        );
        assert_eq!(
            resolve_image("ghcr.io/acme/flutter:{flutter}-xcode26", "3.47.7").unwrap(),
            "ghcr.io/acme/flutter:3.47.7-xcode26"
        );
        assert_eq!(
            resolve_image("fixed-image", "stable").unwrap(),
            "fixed-image"
        );
        assert!(resolve_image(DEFAULT_IMAGE_TEMPLATE, "3.47.7; rm -rf /").is_err());
        assert!(resolve_image(DEFAULT_IMAGE_TEMPLATE, "").is_err());
    }

    #[test]
    fn accepts_pinned_tart_release_or_newer() {
        assert!(version_is_supported("2.41.1"));
        assert!(version_is_supported("2.42.0"));
        assert!(version_is_supported("3.0.0"));
        assert!(!version_is_supported("2.41.0"));
        assert!(!version_is_supported("2.9.9"));
        assert!(!version_is_supported("garbage"));
    }

    #[test]
    fn vm_names_are_scoped_to_the_runner() {
        let name = vm_name("runner-1", "build/../2");
        assert_eq!(name, "oore-runner-1-build2");
        assert!(name.starts_with(&runner_vm_prefix("runner-1")));
        assert!(!name.starts_with(&runner_vm_prefix("runner-10")));
    }

    #[test]
    fn guest_script_quotes_values_and_rejects_bad_keys() {
        let script = guest_step_script(
            "/Users/admin/oore-workspace",
            "flutter build apk --release",
            &[("TOKEN".to_string(), "it's $secret".to_string())],
        )
        .unwrap();
        assert_eq!(
            script,
            "set -e\ncd '/Users/admin/oore-workspace'\nexport TOKEN='it'\\''s $secret'\nexec /bin/sh -c 'flutter build apk --release'\n"
        );
        assert!(guest_step_script("/w", "true", &[("A B".to_string(), "x".to_string())]).is_err());
    }

    #[test]
    fn caches_mount_under_oore_cache() {
        let pub_cache = HostShare {
            name: "pub",
            env: Some("PUB_CACHE"),
            host_dir: PathBuf::from("/caches/pub"),
        };
        let derived = HostShare {
            name: DERIVED_DATA_SHARE,
            env: None,
            host_dir: PathBuf::from("/caches/derived-data/p1"),
        };
        assert_eq!(share_guest_dir(&pub_cache), "oore-cache/pub");
        assert_eq!(share_guest_dir(&derived), "oore-cache/derived-data");
        assert_eq!(share_tag("cocoapods-specs"), "oore-cache-cocoapods-specs");
        // Virtualization.framework caps a virtiofs tag at 36 bytes.
        assert!(share_tag("cocoapods-specs").len() <= 36);
    }

    #[test]
    fn repository_image_config_gets_a_project_scoped_image() {
        let shared = plan_image(
            DEFAULT_IMAGE_TEMPLATE,
            "base:1",
            "3.47.7",
            false,
            "p1",
            None,
        )
        .unwrap();
        assert_eq!(shared.name, "oore-flutter-3.47.7");
        assert_eq!(shared.base, "base:1");

        let raw = "version: 1\nbrew: [jq]\n";
        let config = oore_contract::parse_vm_image_yaml(raw).unwrap();
        let own = plan_image(
            DEFAULT_IMAGE_TEMPLATE,
            "base:1",
            "3.47.7",
            false,
            "p1",
            Some((raw, config.clone())),
        )
        .unwrap();
        assert!(own.name.starts_with("oore-flutter-3.47.7-"));
        assert_eq!(own.name.len(), "oore-flutter-3.47.7-".len() + 12);
        let other_project = plan_image(
            DEFAULT_IMAGE_TEMPLATE,
            "base:1",
            "3.47.7",
            false,
            "p2",
            Some((raw, config.clone())),
        )
        .unwrap();
        assert_ne!(own.name, other_project.name);
        let android = plan_image(
            DEFAULT_IMAGE_TEMPLATE,
            "base:1",
            "3.47.7",
            true,
            "p1",
            Some((raw, config)),
        )
        .unwrap();
        assert_ne!(own.name, android.name);
    }

    #[test]
    fn mounts_use_the_full_mount_virtiofs_path() {
        let script = mount_script(&[
            (
                "oore-workspace".to_string(),
                "/Users/admin/oore-workspace".to_string(),
            ),
            (
                "oore-cache-pub".to_string(),
                "/Users/admin/oore-cache/pub".to_string(),
            ),
        ]);
        assert_eq!(
            script,
            "mkdir -p '/Users/admin/oore-workspace' && /sbin/mount_virtiofs oore-workspace '/Users/admin/oore-workspace' && mkdir -p '/Users/admin/oore-cache/pub' && /sbin/mount_virtiofs oore-cache-pub '/Users/admin/oore-cache/pub'"
        );
    }

    #[test]
    fn gradle_cache_syncs_complete_entries_only() {
        let script = gradle_sync_script("/s", "/l");
        assert!(script.starts_with("cd '/s' 2>/dev/null || exit 0; for dir in caches/modules-2 caches/transforms-* caches/jars-* caches/build-cache-1 wrapper/dists; do"));
        assert!(script.contains("--exclude '*.lock' --exclude '*.lck'"));
        assert!(!script.contains("caches/8."));

        let properties = gradle_properties_script("/s", "/l");
        assert!(properties.contains("cp '/s/gradle.properties' '/l/gradle.properties'"));
        assert!(properties.contains("echo 'org.gradle.caching=true' >> '/l/gradle.properties'"));
    }

    fn run_sh(script: &str) -> std::process::Output {
        Command::new("/bin/sh")
            .args(["-c", script])
            .output()
            .unwrap()
    }

    /// Runs the copy scripts against temp directories where rsync sits at the
    /// macOS path the scripts call.
    #[test]
    fn cache_scripts_round_trip() {
        if !Path::new("/usr/bin/rsync").is_file() {
            return;
        }
        let root = std::env::temp_dir().join(format!("oore-tart-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path = |rel: &str| root.join(rel).to_str().unwrap().to_string();
        let write = |rel: &str, body: &str| {
            let file = root.join(rel);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, body).unwrap();
        };

        // Gradle: complete entries go both ways, locks and per-version stores do not.
        write("gradle-share/caches/modules-2/files-2.1/a.jar", "a");
        write("gradle-share/caches/modules-2/modules-2.lock", "");
        write(
            "gradle-share/gradle.properties",
            "systemProp.https.proxyHost=proxy\n",
        );
        assert!(
            run_sh(&gradle_sync_script(
                &path("gradle-share"),
                &path("gradle-local")
            ))
            .status
            .success()
        );
        assert!(
            root.join("gradle-local/caches/modules-2/files-2.1/a.jar")
                .is_file()
        );
        assert!(
            !root
                .join("gradle-local/caches/modules-2/modules-2.lock")
                .exists()
        );
        assert!(
            run_sh(&gradle_properties_script(
                &path("gradle-share"),
                &path("gradle-local")
            ))
            .status
            .success()
        );
        assert_eq!(
            fs::read_to_string(root.join("gradle-local/gradle.properties")).unwrap(),
            "systemProp.https.proxyHost=proxy\norg.gradle.caching=true\n"
        );
        write("gradle-local/caches/transforms-4/abc/result.jar", "t");
        write("gradle-local/caches/build-cache-1/0123", "c");
        write("gradle-local/caches/8.14/fileHashes/fileHashes.bin", "h");
        assert!(
            run_sh(&gradle_sync_script(
                &path("gradle-local"),
                &path("gradle-share")
            ))
            .status
            .success()
        );
        assert!(
            root.join("gradle-share/caches/transforms-4/abc/result.jar")
                .is_file()
        );
        assert!(
            root.join("gradle-share/caches/build-cache-1/0123")
                .is_file()
        );
        assert!(!root.join("gradle-share/caches/8.14").exists());

        // A project that turns the build cache off keeps it off.
        write(
            "gradle-share/gradle.properties",
            "org.gradle.caching=false\n",
        );
        assert!(
            run_sh(&gradle_properties_script(
                &path("gradle-share"),
                &path("gradle-local")
            ))
            .status
            .success()
        );
        assert_eq!(
            fs::read_to_string(root.join("gradle-local/gradle.properties")).unwrap(),
            "org.gradle.caching=false\n"
        );

        // Android SDK: only packages the image lacked are saved, and restored later.
        write("sdk/platforms/android-36/package.xml", "image");
        let baseline = path("baseline");
        let first = run_sh(&android_sdk_in_script(
            &path("sdk-share"),
            &path("sdk"),
            &baseline,
        ));
        assert!(first.status.success());
        assert!(String::from_utf8_lossy(&first.stdout).trim().is_empty());
        write("sdk/platforms/android-34/package.xml", "34");
        write("sdk/cmake/3.22.1/bin/cmake", "cmake");
        assert!(
            run_sh(&android_sdk_out_script(
                &path("sdk"),
                &path("sdk-share"),
                &baseline
            ))
            .status
            .success()
        );
        assert!(
            root.join("sdk-share/platforms/android-34/package.xml")
                .is_file()
        );
        assert!(root.join("sdk-share/cmake/3.22.1/bin/cmake").is_file());
        assert!(!root.join("sdk-share/platforms/android-36").exists());

        fs::remove_dir_all(root.join("sdk")).unwrap();
        write("sdk/platforms/android-36/package.xml", "image");
        write("sdk-share/ndk/.27.0.partial/half", "x");
        let second = run_sh(&android_sdk_in_script(
            &path("sdk-share"),
            &path("sdk"),
            &baseline,
        ));
        assert!(second.status.success());
        let mut restored = String::from_utf8_lossy(&second.stdout)
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();
        restored.sort();
        assert_eq!(restored, ["cmake;3.22.1", "platforms;android-34"]);
        assert!(root.join("sdk/platforms/android-34/package.xml").is_file());
        assert!(!root.join("sdk/ndk").exists());
        assert!(
            fs::read_dir(root.join("sdk/platforms"))
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().starts_with('.'))
        );

        // DerivedData mirrors, so files Xcode dropped leave the saved copy too.
        write("dd-share/Runner-old/legacy", "l");
        write("dd-local/Runner-abc/Build/old.o", "o");
        write(
            "dd-local/Runner-abc/SourcePackages/checkouts/firebase/x",
            "f",
        );
        let save = derived_data_save_script(&path("dd-local"), &path("dd-share"));
        let restore = derived_data_restore_script(&path("dd-share"), &path("dd-restored"));
        assert!(run_sh(&save).status.success());
        assert_eq!(
            fs::read_to_string(root.join("dd-share/current")).unwrap(),
            "a"
        );
        // The copy from before the two folders is dropped.
        assert!(!root.join("dd-share/Runner-old").exists());
        fs::remove_file(root.join("dd-local/Runner-abc/Build/old.o")).unwrap();
        assert!(run_sh(&save).status.success());
        assert_eq!(
            fs::read_to_string(root.join("dd-share/current")).unwrap(),
            "b"
        );
        let restored = run_sh(&restore);
        assert!(restored.status.success());
        assert_eq!(String::from_utf8_lossy(&restored.stdout).trim(), "restored");
        assert!(
            root.join("dd-restored/Runner-abc/SourcePackages/checkouts/firebase/x")
                .is_file()
        );
        assert!(!root.join("dd-restored/Runner-abc/Build/old.o").exists());

        // A save that fails part way keeps the last complete copy current.
        fs::remove_dir_all(root.join("dd-share/a")).unwrap();
        write("dd-share/a", "not a folder");
        write("dd-local/Runner-abc/Build/new.o", "n");
        assert!(!run_sh(&save).status.success());
        assert_eq!(
            fs::read_to_string(root.join("dd-share/current")).unwrap(),
            "b"
        );
        fs::remove_dir_all(root.join("dd-restored")).unwrap();
        assert!(run_sh(&restore).status.success());
        assert!(
            root.join("dd-restored/Runner-abc/SourcePackages/checkouts/firebase/x")
                .is_file()
        );
        assert!(!root.join("dd-restored/Runner-abc/Build/new.o").exists());

        // Nothing saved yet: nothing restored, and a missing folder saves nothing.
        let empty = run_sh(&derived_data_restore_script(
            &path("dd-none"),
            &path("dd-restored-none"),
        ));
        assert!(empty.status.success());
        assert!(String::from_utf8_lossy(&empty.stdout).trim().is_empty());
        assert!(
            run_sh(&derived_data_save_script(
                &path("missing"),
                &path("dd-none")
            ))
            .status
            .success()
        );
        assert!(!root.join("dd-none").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stdin_scripts_run_past_commands_that_read_stdin() {
        use std::io::Write as _;
        let script = "set -e\ncat >/dev/null\necho after\nfalse\necho unreachable\n";
        let mut child = std::process::Command::new("/bin/sh")
            .arg("-s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin_script(script).as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), "after\n");
        assert!(!output.status.success());
    }

    #[test]
    fn workspace_is_copied_in_whole_and_out_without_git() {
        assert_eq!(
            workspace_in_script(
                "/Users/admin/oore-workspace-share",
                "/Users/admin/oore-workspace"
            ),
            "mkdir -p '/Users/admin/oore-workspace' && { /usr/bin/rsync -a '/Users/admin/oore-workspace-share'/ '/Users/admin/oore-workspace'/; s=$?; [ $s -eq 0 ] || [ $s -eq 23 ] || [ $s -eq 24 ]; }"
        );
        assert_eq!(
            workspace_out_script("/w", "/s"),
            "mkdir -p '/s' && { /usr/bin/rsync -a --exclude '.git' '/w'/ '/s'/; s=$?; [ $s -eq 0 ] || [ $s -eq 23 ] || [ $s -eq 24 ]; }"
        );
    }

    #[test]
    fn android_only_builds_skip_the_xcode_image() {
        let plan = plan_image(DEFAULT_IMAGE_TEMPLATE, "slim", "3.47.7", true, "p", None).unwrap();
        assert_eq!(plan.name, "oore-flutter-3.47.7-android");
        let script = provision_script(&plan);
        assert!(script.contains("brew install --cask android-commandlinetools"));
        assert!(script.contains("flutter precache --android\n"));
        assert!(!script.contains("--ios"));

        let prebuilt = plan_image(
            "ghcr.io/acme/flutter:{flutter}",
            "slim",
            "3.47.7",
            true,
            "p",
            None,
        )
        .unwrap();
        assert_eq!(prebuilt.name, "ghcr.io/acme/flutter:3.47.7");

        let xcode = plan_image(DEFAULT_IMAGE_TEMPLATE, "base", "3.47.7", false, "p", None).unwrap();
        assert!(!provision_script(&xcode).contains("android-commandlinetools"));
    }

    #[test]
    fn provision_script_checks_out_tags_and_channels() {
        let mut plan =
            plan_image(DEFAULT_IMAGE_TEMPLATE, "base", "3.47.7", false, "p", None).unwrap();
        let script = provision_script(&plan);
        assert!(
            script.contains(
                "git fetch --no-tags origin tag '3.47.7'\ngit checkout --detach '3.47.7'"
            )
        );
        assert!(script.contains("flutter precache --ios --android"));

        plan.flutter_version = "stable".to_string();
        plan.config = Some(
            oore_contract::parse_vm_image_yaml(
                "version: 1\nbrew: [jq]\nandroid_packages: ['platforms;android-36']\nsetup: ['echo hi']\n",
            )
            .unwrap(),
        );
        let script = provision_script(&plan);
        assert!(script.contains("git checkout 'stable'\ngit pull --ff-only origin 'stable'"));
        assert!(script.contains("brew install 'jq'\n"));
        assert!(script.contains("yes | sdkmanager 'platforms;android-36'\n"));
        assert!(script.contains("/bin/sh -c 'echo hi'\n"));
    }

    /// A stand-in `tart` that logs its arguments, lists `vms`, fails to
    /// delete `stuck` and hangs on `exec`.
    fn fake_tart(dir: &Path, vms: &[&str], stuck: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        fs::create_dir_all(dir).unwrap();
        let tart = dir.join("tart");
        let log = dir.join("calls.log");
        fs::write(
            &tart,
            format!(
                "#!/bin/sh\necho \"$*\" >> {log}\ncase \"$1\" in\n  list) printf '%s\\n' {vms} ;;\n  delete) [ \"$2\" = {stuck} ] && {{ echo locked >&2; exit 1; }} ;;\n  exec) exec sleep 600 ;;\nesac\nexit 0\n",
                log = shell_quote(log.to_str().unwrap()),
                vms = vms.iter().map(|vm| shell_quote(vm)).collect::<Vec<_>>().join(" "),
                stuck = shell_quote(stuck),
            ),
        )
        .unwrap();
        fs::set_permissions(&tart, fs::Permissions::from_mode(0o755)).unwrap();
        tart
    }

    fn fake_tart_calls(dir: &Path) -> Vec<String> {
        fs::read_to_string(dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn stopping_daemons_waits_for_them_to_exit() {
        // A name no other process has, so the test stops only its own.
        let name = format!("OoreTestDaemon{}", std::process::id());
        let pattern = format!("[{}]{}", &name[..1], &name[1..]);
        let script = stop_processes_script(&pattern);
        // A shell whose `$0`, and so its command line, carries the name. A
        // copied system binary would not do: macOS kills a platform binary
        // run from outside its install path.
        let mut child = Command::new("/bin/sh")
            .args(["-c", "sleep 60; :", &name])
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !run_sh(&format!("pgrep -f {}", shell_quote(&pattern)))
            .status
            .success()
        {
            assert!(Instant::now() < deadline, "the test daemon never showed up");
            std::thread::sleep(Duration::from_millis(50));
        }
        let stopped = run_sh(&script);
        assert!(stopped.status.success());
        assert_eq!(String::from_utf8_lossy(&stopped.stdout).trim(), "1");
        assert!(!child.wait().unwrap().success());
        let none = run_sh(&script);
        assert!(none.status.success());
        assert_eq!(String::from_utf8_lossy(&none.stdout).trim(), "0");
    }

    #[tokio::test]
    async fn sweep_keeps_deleting_after_one_delete_fails() {
        let dir = std::env::temp_dir().join(format!("oore-tart-sweep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let tart = fake_tart(
            &dir,
            &[
                "oore-r1-a",
                "oore-r1-stuck",
                "oore-r1-b",
                "oore-r10-other",
                "oore-flutter-3.47.7",
            ],
            "oore-r1-stuck",
        );
        let sweep = delete_stale_vms(&tart, "r1").await.unwrap();
        assert_eq!(sweep.removed, 2);
        assert_eq!(sweep.errors.len(), 1);
        assert!(sweep.errors[0].starts_with("oore-r1-stuck: "));
        let deletes = fake_tart_calls(&dir)
            .into_iter()
            .filter(|call| call.starts_with("delete "))
            .collect::<Vec<_>>();
        assert_eq!(
            deletes,
            [
                "delete oore-r1-a",
                "delete oore-r1-stuck",
                "delete oore-r1-b"
            ]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn teardown_is_bounded_when_the_guest_agent_hangs() {
        let dir = std::env::temp_dir().join(format!("oore-tart-hang-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let tart = fake_tart(&dir, &[], "");
        let mut vm = TartVm {
            tart,
            name: "oore-r1-build".to_string(),
            run: None,
            run_stderr: Arc::new(Mutex::new(String::new())),
            guest_workspace: String::new(),
            guest_share: String::new(),
            guest_env: Vec::new(),
            gradle_sync: None,
            derived_data_sync: Some(("/share".to_string(), "/local".to_string())),
            android_sdk_sync: None,
            cache_notes: Vec::new(),
            released: false,
            copy_timeout: Duration::from_secs(1),
        };
        let started = Instant::now();
        let lines = vm.save_caches().await;
        vm.delete().await.unwrap();
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("  derived data: not saved after 1s: "));
        assert!(lines[0].contains("did not finish within 1s"));
        assert!(
            fake_tart_calls(&dir)
                .iter()
                .any(|call| call == "delete oore-r1-build")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn prunes_built_images_unused_for_the_max_age() {
        let dir = std::env::temp_dir().join(format!("oore-tart-prune-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let tart = fake_tart(
            &dir,
            &[
                "oore-flutter-3.41.0",
                "oore-flutter-3.47.7",
                "oore-flutter-3.44.0",
                "oore-flutter-3.40.0",
                "my-golden-image",
            ],
            "",
        );
        let day = 24 * 60 * 60;
        let now = 100 * day;
        let max_age = Some(Duration::from_secs(30 * day as u64));
        write_image_use(&dir, "oore-flutter-3.41.0", now - 31 * day).unwrap();
        write_image_use(&dir, "oore-flutter-3.47.7", now - day).unwrap();
        write_image_use(&dir, "oore-flutter-3.40.0", now - 90 * day).unwrap();
        // A build is creating or using 3.40.0 right now.
        let _busy = try_lock_image(&dir, "oore-flutter-3.40.0")
            .unwrap()
            .unwrap();

        let prune = prune_unused_images(&tart, &dir, None, max_age, now)
            .await
            .unwrap();
        assert_eq!(prune.deleted, ["oore-flutter-3.41.0"]);
        assert_eq!(prune.kept, 3);
        assert!(read_image_use(&dir, "oore-flutter-3.41.0").is_none());
        // An image with no record starts its clock.
        assert_eq!(read_image_use(&dir, "oore-flutter-3.44.0"), Some(now));
        let deletes = fake_tart_calls(&dir)
            .into_iter()
            .filter(|call| call.starts_with("delete "))
            .collect::<Vec<_>>();
        assert_eq!(deletes, ["delete oore-flutter-3.41.0"]);

        // A recorded use keeps the image; no max age keeps every image.
        record_image_use(&dir, "oore-flutter-3.44.0", now + 40 * day)
            .await
            .unwrap();
        let later = prune_unused_images(&tart, &dir, None, None, now + 365 * day)
            .await
            .unwrap();
        assert!(later.deleted.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn slots_cap_concurrent_vms() {
        let dir = std::env::temp_dir().join(format!("oore-slot-test-{}", std::process::id()));
        let first = try_acquire_slot(&dir, 2).unwrap().unwrap();
        let second = try_acquire_slot(&dir, 2).unwrap().unwrap();
        assert_ne!(first.index, second.index);
        assert!(try_acquire_slot(&dir, 2).unwrap().is_none());
        drop(first);
        // A child forked by a concurrent test inherits the lock's descriptor
        // until it execs, so the released slot can stay held for a moment.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while try_acquire_slot(&dir, 2).unwrap().is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "the released slot stayed locked"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        drop(second);
        let _ = fs::remove_dir_all(&dir);
    }
}
