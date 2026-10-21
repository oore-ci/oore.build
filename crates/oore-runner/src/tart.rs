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
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
/// Fixed guest path, so Xcode's DerivedData and Gradle see the same project
/// path on every build.
const GUEST_WORKSPACE_DIR: &str = "oore-workspace";
/// Apple's Virtualization framework runs at most two macOS guests per host.
pub(crate) const MAX_MACOS_VMS: usize = 2;
const BOOT_TIMEOUT: Duration = Duration::from_secs(180);
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

pub(crate) fn tart_version(tart: &Path) -> Option<String> {
    let output = Command::new(tart).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8(output.stdout).ok()?.trim().to_string();
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
/// Android SDK and the Tart Guest Agent are already in it.
pub(crate) const BASE_IMAGE_ENV: &str = "OORE_TART_BASE_IMAGE";
pub(crate) const DEFAULT_BASE_IMAGE: &str = "ghcr.io/cirruslabs/macos-tahoe-xcode:26.5";

pub(crate) fn base_image() -> String {
    std::env::var(BASE_IMAGE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_IMAGE.to_string())
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
    pub config: Option<oore_contract::VmImageConfig>,
}

/// Picks the image for a build. A repository's `oore.image.yaml` makes the
/// image its project's own: the name carries a hash of the project, base,
/// Flutter version and file, so a changed file builds a new image.
pub(crate) fn plan_image(
    template: &str,
    base: &str,
    flutter_version: &str,
    project_id: &str,
    image_config: Option<(&str, oore_contract::VmImageConfig)>,
) -> anyhow::Result<ImagePlan> {
    let name = resolve_image(template, flutter_version)?;
    let Some((raw, config)) = image_config else {
        return Ok(ImagePlan {
            name,
            base: base.to_string(),
            flutter_version: flutter_version.to_string(),
            config: None,
        });
    };
    let base = config.base.clone().unwrap_or_else(|| base.to_string());
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    for part in [project_id, base.as_str(), flutter_version, raw] {
        sha2::Digest::update(&mut hasher, part.as_bytes());
        sha2::Digest::update(&mut hasher, [0]);
    }
    let digest = hex::encode(sha2::Digest::finalize(hasher));
    Ok(ImagePlan {
        name: format!("oore-flutter-{flutter_version}-{}", &digest[..12]),
        base,
        flutter_version: flutter_version.to_string(),
        config: Some(config),
    })
}

const FLUTTER_CHANNELS: [&str; 4] = ["stable", "beta", "main", "master"];

/// Script that turns a base image into one for `plan.flutter_version`.
pub(crate) fn provision_script(plan: &ImagePlan) -> String {
    let version = shell_quote(&plan.flutter_version);
    let checkout = if FLUTTER_CHANNELS.contains(&plan.flutter_version.as_str()) {
        format!("git checkout {version}\ngit pull --ff-only origin {version}")
    } else {
        format!("git fetch --no-tags origin tag {version}\ngit checkout --detach {version}")
    };
    let mut script = format!(
        r#"set -e
if [ ! -d "$HOME/flutter/.git" ]; then
  git clone https://github.com/flutter/flutter.git "$HOME/flutter"
fi
cd "$HOME/flutter"
{checkout}
grep -q 'flutter/bin' "$HOME/.zprofile" 2>/dev/null || echo 'export PATH="$HOME/flutter/bin:$PATH"' >> "$HOME/.zprofile"
export PATH="$HOME/flutter/bin:$PATH"
flutter config --no-analytics
flutter precache --ios --android
yes | flutter doctor --android-licenses > /dev/null || true
"#
    );
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

pub(crate) fn local_image_exists(tart: &Path, name: &str) -> anyhow::Result<bool> {
    let listing = run_tart(tart, &["list", "--source", "local", "--quiet"])?;
    Ok(listing.lines().any(|line| line.trim() == name))
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

pub(crate) fn slot_dir() -> anyhow::Result<PathBuf> {
    let tart_home = match std::env::var_os("TART_HOME") {
        Some(home) => PathBuf::from(home),
        None => PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join(".tart"),
    };
    Ok(tart_home.join("oore-slots"))
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

fn run_tart(tart: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new(tart)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to run tart {}", args.join(" ")))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    anyhow::bail!(
        "tart {} failed: {}",
        args.first().copied().unwrap_or_default(),
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

/// Deletes VMs this runner left behind when it crashed or was killed mid-build.
pub(crate) fn delete_stale_vms(tart: &Path, runner_id: &str) -> anyhow::Result<usize> {
    let prefix = runner_vm_prefix(runner_id);
    let listing = run_tart(tart, &["list", "--source", "local", "--quiet"])?;
    let mut removed = 0;
    for name in listing.lines().map(str::trim) {
        if !name.starts_with(&prefix) {
            continue;
        }
        let _ = run_tart(tart, &["stop", "--timeout", "5", name]);
        run_tart(tart, &["delete", name])?;
        removed += 1;
    }
    Ok(removed)
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

/// A host directory shared into the guest, such as a dependency cache.
pub(crate) struct HostShare {
    pub name: &'static str,
    /// Variable to point at the guest path; without one the share is mounted
    /// at the tool's default location (Xcode's DerivedData).
    pub env: Option<&'static str>,
    pub host_dir: PathBuf,
}

fn share_tag(name: &str) -> String {
    format!("oore-cache-{}", name_component(name))
}

/// Guest path for a share, relative to the guest home.
fn share_guest_dir(share: &HostShare) -> String {
    match share.env {
        Some(_) => format!("oore-cache/{}", name_component(share.name)),
        None => "Library/Developer/Xcode/DerivedData".to_string(),
    }
}

pub(crate) struct TartVm {
    tart: PathBuf,
    name: String,
    run: Option<std::process::Child>,
    run_stderr: Arc<Mutex<String>>,
    guest_workspace: String,
    guest_env: Vec<(String, String)>,
    /// Set once the VM was kept as an image; nothing is deleted then.
    kept: bool,
}

impl TartVm {
    /// Clones `image`, boots it headless with `workspace` shared, and waits
    /// until the guest agent answers and the workspace is mounted. The caller
    /// holds a [`VmSlot`] for as long as the VM lives.
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
        {
            let tart = tart.clone();
            let source = image.to_string();
            let name = name.clone();
            tokio::task::spawn_blocking(move || run_tart(&tart, &["clone", &source, &name]))
                .await
                .context("tart clone task failed")?
                .with_context(|| {
                    format!(
                        "failed to clone VM image {image}; build or pull it on this runner first"
                    )
                })?;
        }

        let mut vm = Self {
            tart,
            name,
            run: None,
            run_stderr: Arc::new(Mutex::new(String::new())),
            guest_workspace: String::new(),
            guest_env: Vec::new(),
            kept: false,
        };
        let mut child = Command::new(&vm.tart)
            .arg("run")
            .arg(&vm.name)
            .args(["--no-graphics", "--no-audio", "--no-clipboard"])
            .args(&dir_args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to start tart run")?;
        if let Some(mut stderr) = child.stderr.take() {
            let sink = vm.run_stderr.clone();
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
        vm.run = Some(child);

        vm.wait_for_agent().await?;
        let home = vm.exec_capture("printf %s \"$HOME\"").await?;
        anyhow::ensure!(
            home.starts_with('/'),
            "VM reported an unusable home directory '{home}'"
        );
        let home = home.trim_end_matches('/');
        if workspace.is_none() {
            vm.guest_workspace = home.to_string();
            return Ok(vm);
        }
        let guest_workspace = format!("{home}/{GUEST_WORKSPACE_DIR}");
        let mut mounts = vec![(WORKSPACE_TAG.to_string(), guest_workspace.clone())];
        for share in shares {
            let guest_dir = format!("{home}/{}", share_guest_dir(share));
            if let Some(env) = share.env {
                vm.guest_env.push((env.to_string(), guest_dir.clone()));
            }
            mounts.push((share_tag(share.name), guest_dir));
        }
        let script = mounts
            .iter()
            .map(|(tag, path)| {
                let path = shell_quote(path);
                format!("mkdir -p {path} && mount_virtiofs {tag} {path}")
            })
            .collect::<Vec<_>>()
            .join(" && ");
        vm.exec_capture(&script)
            .await
            .context("failed to mount the build workspace and caches inside the VM")?;
        vm.guest_workspace = guest_workspace;
        Ok(vm)
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn guest_workspace(&self) -> &str {
        &self.guest_workspace
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
            let ready = tokio::process::Command::new(&self.tart)
                .args(["exec", &self.name, "/usr/bin/true"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await
                .is_ok_and(|status| status.success());
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

    async fn exec_capture(&self, script: &str) -> anyhow::Result<String> {
        let output = tokio::process::Command::new(&self.tart)
            .args(["exec", &self.name, "/bin/sh", "-c", script])
            .stdin(Stdio::null())
            .output()
            .await
            .context("failed to run tart exec")?;
        anyhow::ensure!(
            output.status.success(),
            "command in VM failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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

    /// Feeds `script` to a guest login shell, which picks up the image's PATH
    /// (Flutter, Homebrew, Ruby).
    pub(crate) async fn spawn_script(&self, script: &str) -> anyhow::Result<tokio::process::Child> {
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
}

impl TartVm {
    fn shut_down(&mut self, timeout_secs: &str) {
        let _ = run_tart(&self.tart, &["stop", "--timeout", timeout_secs, &self.name]);
        if let Some(mut run) = self.run.take() {
            if run.try_wait().ok().flatten().is_none() {
                let _ = run.kill();
            }
            let _ = run.wait();
        }
    }

    /// Shuts the guest down cleanly and keeps its disk as the local image `image`.
    pub(crate) fn keep_as(mut self, image: &str) -> anyhow::Result<()> {
        self.shut_down("120");
        run_tart(&self.tart, &["rename", &self.name, image])?;
        self.kept = true;
        Ok(())
    }
}

impl Drop for TartVm {
    fn drop(&mut self) {
        if self.kept {
            return;
        }
        self.shut_down("10");
        if let Err(error) = run_tart(&self.tart, &["delete", &self.name]) {
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
    fn caches_mount_under_home_and_derived_data_at_xcode_default() {
        let pub_cache = HostShare {
            name: "pub",
            env: Some("PUB_CACHE"),
            host_dir: PathBuf::from("/caches/pub"),
        };
        let derived = HostShare {
            name: "derived-data",
            env: None,
            host_dir: PathBuf::from("/caches/derived-data"),
        };
        assert_eq!(share_guest_dir(&pub_cache), "oore-cache/pub");
        assert_eq!(
            share_guest_dir(&derived),
            "Library/Developer/Xcode/DerivedData"
        );
        assert_eq!(share_tag("cocoapods-specs"), "oore-cache-cocoapods-specs");
        // Virtualization.framework caps a virtiofs tag at 36 bytes.
        assert!(share_tag("cocoapods-specs").len() <= 36);
    }

    #[test]
    fn repository_image_config_gets_a_project_scoped_image() {
        let shared = plan_image(DEFAULT_IMAGE_TEMPLATE, "base:1", "3.47.7", "p1", None).unwrap();
        assert_eq!(shared.name, "oore-flutter-3.47.7");
        assert_eq!(shared.base, "base:1");

        let raw = "version: 1\nbrew: [jq]\n";
        let config = oore_contract::parse_vm_image_yaml(raw).unwrap();
        let own = plan_image(
            DEFAULT_IMAGE_TEMPLATE,
            "base:1",
            "3.47.7",
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
            "p2",
            Some((raw, config)),
        )
        .unwrap();
        assert_ne!(own.name, other_project.name);
    }

    #[test]
    fn provision_script_checks_out_tags_and_channels() {
        let mut plan = plan_image(DEFAULT_IMAGE_TEMPLATE, "base", "3.47.7", "p", None).unwrap();
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

    #[test]
    fn slots_cap_concurrent_vms() {
        let dir = std::env::temp_dir().join(format!("oore-slot-test-{}", std::process::id()));
        let first = try_acquire_slot(&dir, 2).unwrap().unwrap();
        let second = try_acquire_slot(&dir, 2).unwrap().unwrap();
        assert_ne!(first.index, second.index);
        assert!(try_acquire_slot(&dir, 2).unwrap().is_none());
        drop(first);
        assert!(try_acquire_slot(&dir, 2).unwrap().is_some());
        drop(second);
        let _ = fs::remove_dir_all(&dir);
    }
}
