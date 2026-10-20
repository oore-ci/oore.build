use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::net::IpAddr;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use base64::Engine as _;
use oore_contract::{
    AndroidSigningBuildType, BuildPlatform, BuildStatus, ClaimJobRequest, ClaimJobResponse,
    ClaimedJob, CompleteArtifactRequest, CompleteArtifactResponse, JobStatusResponse,
    PipelineCommandStages, PipelineEnvVar, PipelineExecutionConfig, PlatformBuildArgs,
    PlatformBuildCommands, RUNNER_PROTOCOL_VERSION, RunnerAndroidSigningProfile,
    RunnerAndroidSigningResponse, RunnerIosSigningBundle, RunnerIosSigningResponse, StepResult,
    artifact_pattern_matches, is_valid_pipeline_env_key, parse_repository_pipeline_yaml,
    validate_artifact_pattern, validate_repository_config_path,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncReadExt};
use zeroize::Zeroize;

const AUTO_CONFIG_PATHS: [&str; 2] = [".oore.yaml", ".oore.yml"];
const OORE_ANDROID_KEYSTORE_PATH_ENV: &str = "OORE_ANDROID_KEYSTORE_PATH";
const OORE_ANDROID_KEYSTORE_B64_ENV: &str = "OORE_ANDROID_KEYSTORE_BASE64";
const OORE_ANDROID_KEYSTORE_PASSWORD_ENV: &str = "OORE_ANDROID_KEYSTORE_PASSWORD";
const OORE_ANDROID_KEY_ALIAS_ENV: &str = "OORE_ANDROID_KEY_ALIAS";
const OORE_ANDROID_KEY_PASSWORD_ENV: &str = "OORE_ANDROID_KEY_PASSWORD";
const OORE_ANDROID_KEY_PROPERTIES_PATH_ENV: &str = "OORE_ANDROID_KEY_PROPERTIES_PATH";
const MANAGED_ANDROID_SIGNING_ENV_KEYS: [&str; 6] = [
    OORE_ANDROID_KEYSTORE_PATH_ENV,
    OORE_ANDROID_KEYSTORE_B64_ENV,
    OORE_ANDROID_KEYSTORE_PASSWORD_ENV,
    OORE_ANDROID_KEY_ALIAS_ENV,
    OORE_ANDROID_KEY_PASSWORD_ENV,
    OORE_ANDROID_KEY_PROPERTIES_PATH_ENV,
];
const ANDROID_SIGNER_STORE_PASSWORD_ENV: &str = "OORE_SIGNER_STORE_PASSWORD";
const ANDROID_SIGNER_KEY_PASSWORD_ENV: &str = "OORE_SIGNER_KEY_PASSWORD";
const IOS_SIGNING_DIR: &str = ".oore/ios-signing";
const IOS_CLEANUP_JOURNAL: &str = ".oore/ios-signing/cleanup-journal.json";
const BUILD_WORKSPACE_PREFIX: &str = "oore-build";
const RUNNER_WORKSPACE_ROOT_NAME: &str = "oore-runner-workspaces";
const LEGACY_RECONCILIATION_MARKER: &str = ".legacy-workspaces-reconciled-v1";
const LEGACY_BUILD_WORKSPACE_ROOT: &str = "/tmp/oore-builds.noindex";
const SPOTLIGHT_NO_INDEX_SENTINEL: &str = ".metadata_never_index";
pub const RUNNER_SERVICE_ACK_FILE: &str = "runner-service-ack.json";
pub const RUNNER_SERVICE_ACK_PATH_ENV: &str = "OORE_RUNNER_SERVICE_ACK_PATH";
pub const RUNNER_SERVICE_ACK_MAX_AGE_SECS: u64 = 75;
const RUNNER_SERVICE_ACK_SCHEMA_VERSION: u32 = 1;
const MANAGED_RUNNER_SERVICE_LABEL: &str = "build.oore.oore-runner";
// ponytail: fixed three-check grace; move it into the runner protocol if deployments need tuning.
const MAX_CONSECUTIVE_AUTHORITY_FAILURES: u8 = 3;
const MANAGED_FVM_VERSION: &str = "4.1.2";
const MANAGED_FVM_ARM64_SHA256: &str =
    "0b2a146986c51f06331f135f0bdf2a202eb57f55d7edd420c9078e8520e4c033";
const MANAGED_FVM_X64_SHA256: &str =
    "7bbfcb6883ea67ce532163704f5625eba7ecf340084be707cde71a28fefff1d8";
const MANAGED_FVM_MAX_ARCHIVE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunnerConfig {
    pub runner_id: String,
    pub runner_token: String,
    pub daemon_url: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RunnerServiceAck {
    pub schema_version: u32,
    pub pid: u32,
    pub runner_id: String,
    pub daemon_url_sha256: String,
    pub executable_identity: String,
    pub version: String,
    pub protocol_version: u32,
    pub acknowledged_at: i64,
}

/// Reject runner control-plane URLs that could expose the bearer token or job
/// traffic over a cleartext network connection. HTTP remains available only
/// for a daemon addressed by a literal loopback IP.
pub fn require_safe_daemon_url(raw_url: &str) -> anyhow::Result<()> {
    let url = reqwest::Url::parse(raw_url).context("invalid daemon URL")?;
    match url.scheme() {
        "https" => Ok(()),
        "http" => {
            let host = url.host_str().context("daemon URL must include a host")?;
            let ip = host
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .map_err(|_| {
                    anyhow::anyhow!(
                        "cleartext daemon URLs require a literal loopback IP; use HTTPS for {host}"
                    )
                })?;
            if ip.is_loopback() {
                Ok(())
            } else {
                anyhow::bail!("cleartext daemon URLs are allowed only for literal loopback IPs")
            }
        }
        scheme => anyhow::bail!(
            "daemon URL must use HTTPS (or HTTP for a literal loopback IP), not {scheme}"
        ),
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn try_mark_no_spotlight_index(path: &Path) {
    let sentinel = path.join(SPOTLIGHT_NO_INDEX_SENTINEL);
    if sentinel.exists() {
        return;
    }
    if let Err(err) = fs::write(&sentinel, b"") {
        eprintln!(
            "Warning: failed to write Spotlight no-index marker {}: {}",
            sentinel.display(),
            err
        );
    }
}

fn runner_workspace_prefix(runner_id: &str) -> String {
    let digest = Sha256::digest(runner_id.as_bytes());
    format!("{BUILD_WORKSPACE_PREFIX}-{}-", hex::encode(&digest[..8]))
}

fn is_runner_workspace_name(file_name: &str) -> bool {
    let Some(suffix) = file_name.strip_prefix(&format!("{BUILD_WORKSPACE_PREFIX}-")) else {
        return false;
    };
    let Some((runner_hash, random_suffix)) = suffix.split_once('-') else {
        return false;
    };
    runner_hash.len() == 16
        && runner_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        && random_suffix.len() == 32
        && random_suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn create_private_workspace_in(parent: &Path, runner_id: &str) -> std::io::Result<PathBuf> {
    let prefix = runner_workspace_prefix(runner_id);
    for _ in 0..16 {
        let mut random = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut random);
        let path = parent.join(format!("{prefix}{}", hex::encode(random)));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        ErrorKind::AlreadyExists,
        "failed to allocate a unique runner workspace",
    ))
}

fn prepare_runner_workspace_root() -> anyhow::Result<PathBuf> {
    let path = std::env::temp_dir().join(RUNNER_WORKSPACE_ROOT_NAME);
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }

    let metadata = fs::symlink_metadata(&path)?;
    anyhow::ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "runner workspace root {} is not a trusted directory",
        path.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        anyhow::ensure!(
            metadata.uid() == current_uid()? && metadata.permissions().mode() & 0o077 == 0,
            "runner workspace root {} is not a private owned directory",
            path.display()
        );
    }
    fs::canonicalize(&path).context("failed to resolve runner workspace root")
}

fn repository_shell_command(script: &str, workspace: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(script)
        .current_dir(workspace)
        .env_remove(RUNNER_SERVICE_ACK_PATH_ENV);
    if let Some(install_root) = bundled_fvm_install_root() {
        configure_bundled_fvm_environment(&mut command, &install_root);
    }
    command
}

fn bundled_fvm_install_root() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    oore_install_root_for_executable(&executable)
        .filter(|install_root| managed_fvm_is_available(install_root))
}

fn oore_install_root_for_executable(executable: &Path) -> Option<PathBuf> {
    let bin = executable.parent()?;
    if bin.file_name()? != "bin" {
        return None;
    }
    let install_root = bin.parent()?;
    install_root
        .join("VERSION")
        .is_file()
        .then(|| install_root.to_path_buf())
}

fn managed_fvm_is_available(install_root: &Path) -> bool {
    install_root.join("bin/fvm").is_file() && install_root.join("libexec/fvm/fvm").is_file()
}

fn managed_fvm_download(arch: &str) -> anyhow::Result<(String, &'static str)> {
    let (asset_arch, expected_sha256) = match arch {
        "aarch64" => ("arm64", MANAGED_FVM_ARM64_SHA256),
        "x86_64" => ("x64", MANAGED_FVM_X64_SHA256),
        other => anyhow::bail!("Oore-managed FVM is not available for architecture {other}"),
    };
    Ok((
        format!(
            "https://github.com/conceptadev/fvm/releases/download/{MANAGED_FVM_VERSION}/fvm-{MANAGED_FVM_VERSION}-macos-{asset_arch}.tar.gz"
        ),
        expected_sha256,
    ))
}

async fn install_managed_fvm_archive(
    install_root: &Path,
    archive_bytes: &[u8],
    expected_sha256: &str,
) -> anyhow::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    anyhow::ensure!(
        archive_bytes.len() <= MANAGED_FVM_MAX_ARCHIVE_BYTES,
        "managed FVM archive exceeds the {} byte limit",
        MANAGED_FVM_MAX_ARCHIVE_BYTES
    );
    let actual_sha256 = hex::encode(Sha256::digest(archive_bytes));
    anyhow::ensure!(
        actual_sha256 == expected_sha256,
        "managed FVM archive checksum mismatch"
    );

    let mut random = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut random);
    let staging = install_root.join(format!(
        ".fvm-bootstrap-{}-{}",
        std::process::id(),
        hex::encode(random)
    ));
    let mut staging_builder = fs::DirBuilder::new();
    staging_builder.mode(0o700);
    staging_builder
        .create(&staging)
        .with_context(|| format!("failed to create {}", staging.display()))?;

    let result: anyhow::Result<()> = async {
        let archive = staging.join("fvm.tar.gz");
        write_private_file(&archive, archive_bytes)
            .with_context(|| format!("failed to write {}", archive.display()))?;
        let output = tokio::process::Command::new("/usr/bin/tar")
            .args(["-xzf"])
            .arg(&archive)
            .args(["-C"])
            .arg(&staging)
            .output()
            .await
            .context("failed to extract managed FVM archive")?;
        anyhow::ensure!(
            output.status.success(),
            "failed to extract managed FVM archive: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );

        let extracted = staging.join("fvm");
        let payload = extracted.join("fvm");
        let payload_metadata = fs::symlink_metadata(&payload)
            .with_context(|| format!("managed FVM archive is missing {}", payload.display()))?;
        anyhow::ensure!(
            payload_metadata.file_type().is_file(),
            "managed FVM payload is not a regular file"
        );
        fs::set_permissions(&payload, fs::Permissions::from_mode(0o755))?;

        let libexec = install_root.join("libexec");
        fs::create_dir_all(&libexec)?;
        let destination = libexec.join("fvm");
        match fs::remove_dir_all(&destination) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        fs::rename(&extracted, &destination).with_context(|| {
            format!(
                "failed to install managed FVM payload at {}",
                destination.display()
            )
        })?;
        fs::File::open(&libexec)?.sync_all()?;

        let bin = install_root.join("bin");
        fs::create_dir_all(&bin)?;
        let launcher = bin.join(format!(
            ".fvm.{}-{}.tmp",
            std::process::id(),
            hex::encode(random)
        ));
        write_private_file(
            &launcher,
            b"#!/bin/sh\nset -eu\nroot=\"$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\"\nexec \"$root/libexec/fvm/fvm\" \"$@\"\n",
        )?;
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755))?;
        fs::rename(&launcher, bin.join("fvm"))?;
        fs::File::open(&bin)?.sync_all()?;
        Ok(())
    }
    .await;

    if let Err(cleanup_error) = fs::remove_dir_all(&staging)
        && cleanup_error.kind() != ErrorKind::NotFound
        && result.is_ok()
    {
        return Err(cleanup_error).context("failed to clean up managed FVM bootstrap");
    }
    result
}

async fn ensure_managed_fvm(client: &reqwest::Client) -> anyhow::Result<bool> {
    let executable = std::env::current_exe().context("failed to locate the Oore runner")?;
    let Some(install_root) = oore_install_root_for_executable(&executable) else {
        return Ok(false);
    };
    if managed_fvm_is_available(&install_root) {
        return Ok(false);
    }

    let (url, expected_sha256) = managed_fvm_download(std::env::consts::ARCH)?;
    let response = client
        .get(&url)
        .send()
        .await
        .context("failed to download Oore-managed FVM")?
        .error_for_status()
        .context("Oore-managed FVM download failed")?;
    if let Some(length) = response.content_length() {
        anyhow::ensure!(
            length <= MANAGED_FVM_MAX_ARCHIVE_BYTES as u64,
            "managed FVM archive exceeds the {} byte limit",
            MANAGED_FVM_MAX_ARCHIVE_BYTES
        );
    }
    let bytes = response
        .bytes()
        .await
        .context("failed to read Oore-managed FVM download")?;
    install_managed_fvm_archive(&install_root, &bytes, expected_sha256).await?;
    Ok(true)
}

fn configure_bundled_fvm_environment(command: &mut tokio::process::Command, install_root: &Path) {
    let mut paths = vec![install_root.join("bin")];
    if let Some(current) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&current));
    }
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
    command.env(
        "FVM_CACHE_PATH",
        install_root.join("toolchains").join("flutter"),
    );
}

fn write_private_file(path: &Path, content: &[u8]) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    if let Err(error) = file.write_all(content).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

pub fn runner_version_for_executable(executable: &Path) -> String {
    executable
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("VERSION"))
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

pub async fn detect_capabilities() -> serde_json::Value {
    let os_version = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let xcode_version = std::process::Command::new("xcodebuild")
        .arg("-version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .unwrap_or_default();

    let arch = std::env::consts::ARCH.to_string();
    let version = std::env::current_exe()
        .map(|path| runner_version_for_executable(&path))
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());

    serde_json::json!({
        "os": "macos",
        "os_version": os_version,
        "arch": arch,
        "xcode_version": xcode_version,
        "version": version,
        "protocol_version": RUNNER_PROTOCOL_VERSION,
    })
}

pub fn get_hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub const RUNNER_RELEASE_MARKER_FILE: &str = "RUNNER_RELEASE";

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExecutableIdentity {
    device: u64,
    inode: u64,
    length: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
}

fn executable_identity(path: &Path) -> anyhow::Result<ExecutableIdentity> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to inspect runner executable {}", path.display()))?;
    anyhow::ensure!(
        metadata.is_file(),
        "runner executable is not a regular file"
    );

    Ok(ExecutableIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        length: metadata.len(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
    })
}

impl ExecutableIdentity {
    fn encode_fields(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            self.device, self.inode, self.length, self.modified_seconds, self.modified_nanoseconds
        )
    }

    fn encode(&self) -> String {
        format!("v1:{}", self.encode_fields())
    }

    fn decode(raw: &str) -> anyhow::Result<Self> {
        let values = raw.trim().split(':').collect::<Vec<_>>();
        anyhow::ensure!(
            values.len() == 6 && values[0] == "v1",
            "invalid runner release marker"
        );
        Ok(Self {
            device: values[1].parse().context("invalid marker device")?,
            inode: values[2].parse().context("invalid marker inode")?,
            length: values[3].parse().context("invalid marker length")?,
            modified_seconds: values[4]
                .parse()
                .context("invalid marker modification time")?,
            modified_nanoseconds: values[5]
                .parse()
                .context("invalid marker modification nanoseconds")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunnerReleaseMarker {
    generation: Option<String>,
    executable: ExecutableIdentity,
}

impl RunnerReleaseMarker {
    fn new(executable: ExecutableIdentity) -> Self {
        let mut generation = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut generation);
        Self {
            generation: Some(hex::encode(generation)),
            executable,
        }
    }

    fn encode(&self) -> String {
        match self.generation.as_deref() {
            Some(generation) => {
                format!("v2:{generation}:{}", self.executable.encode_fields())
            }
            None => self.executable.encode(),
        }
    }

    fn decode(raw: &str) -> anyhow::Result<Self> {
        let raw = raw.trim();
        if raw.starts_with("v1:") {
            return Ok(Self {
                generation: None,
                executable: ExecutableIdentity::decode(raw)?,
            });
        }

        let mut values = raw.splitn(3, ':');
        anyhow::ensure!(values.next() == Some("v2"), "invalid runner release marker");
        let generation = values.next().context("missing marker generation")?;
        anyhow::ensure!(
            generation.len() == 32 && generation.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid marker generation"
        );
        let identity = values
            .next()
            .context("missing marker executable identity")?;
        Ok(Self {
            generation: Some(generation.to_string()),
            executable: ExecutableIdentity::decode(&format!("v1:{identity}"))?,
        })
    }
}

pub fn runner_executable_identity_marker(path: &Path) -> anyhow::Result<String> {
    Ok(executable_identity(path)?.encode())
}

fn runner_service_ack_path(raw_path: Option<OsString>) -> anyhow::Result<Option<PathBuf>> {
    let Some(raw_path) = raw_path else {
        return Ok(None);
    };
    anyhow::ensure!(
        !raw_path.is_empty(),
        "{RUNNER_SERVICE_ACK_PATH_ENV} must not be empty"
    );
    let path = PathBuf::from(raw_path);
    anyhow::ensure!(
        path.is_absolute(),
        "{RUNNER_SERVICE_ACK_PATH_ENV} must be an absolute path"
    );
    Ok(Some(path))
}

fn runner_service_ack_path_from_env() -> anyhow::Result<Option<PathBuf>> {
    runner_service_ack_path(std::env::var_os(RUNNER_SERVICE_ACK_PATH_ENV))
}

pub fn runner_daemon_url_fingerprint(daemon_url: &str) -> String {
    hex::encode(Sha256::digest(daemon_url.as_bytes()))
}

fn runner_service_ack_for(
    config: &RunnerConfig,
    daemon_url: &str,
    executable: &Path,
    acknowledged_at: i64,
) -> anyhow::Result<RunnerServiceAck> {
    Ok(RunnerServiceAck {
        schema_version: RUNNER_SERVICE_ACK_SCHEMA_VERSION,
        pid: std::process::id(),
        runner_id: config.runner_id.clone(),
        daemon_url_sha256: runner_daemon_url_fingerprint(daemon_url),
        executable_identity: runner_executable_identity_marker(executable)?,
        version: runner_version_for_executable(executable),
        protocol_version: RUNNER_PROTOCOL_VERSION,
        acknowledged_at,
    })
}

fn refreshed_runner_service_ack(
    template: &RunnerServiceAck,
    acknowledged_at: i64,
) -> RunnerServiceAck {
    let mut ack = template.clone();
    ack.acknowledged_at = acknowledged_at;
    ack
}

fn write_runner_service_ack(path: &Path, ack: &RunnerServiceAck) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("runner service acknowledgement path has no parent")?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create runner acknowledgement directory {}",
            parent.display()
        )
    })?;

    let mut random = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut random);
    let staged = parent.join(format!(
        ".{}.{}-{}.tmp",
        RUNNER_SERVICE_ACK_FILE,
        std::process::id(),
        hex::encode(random)
    ));
    let bytes = serde_json::to_vec(ack).context("failed to serialize runner acknowledgement")?;
    if let Err(error) = write_private_file(&staged, &bytes).and_then(|()| fs::rename(&staged, path))
    {
        let _ = fs::remove_file(&staged);
        return Err(error).with_context(|| {
            format!(
                "failed to publish runner service acknowledgement {}",
                path.display()
            )
        });
    }
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn clear_runner_service_ack(path: &Path) -> anyhow::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to clear runner service acknowledgement {}",
                path.display()
            )
        }),
    }
}

fn clear_runner_service_ack_if_owned(path: &Path, pid: u32) {
    let owned = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<RunnerServiceAck>(&bytes).ok())
        .is_some_and(|ack| ack.pid == pid);
    if owned {
        let _ = fs::remove_file(path);
    }
}

pub fn verify_runner_service_ack(
    path: &Path,
    config: &RunnerConfig,
    executable: &Path,
    expected_pid: u32,
    not_before: Option<i64>,
    max_age: Duration,
) -> anyhow::Result<RunnerServiceAck> {
    #[cfg(unix)]
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let metadata = fs::symlink_metadata(path).with_context(|| {
        format!(
            "runner has not acknowledged the backend at {}",
            path.display()
        )
    })?;
    anyhow::ensure!(
        metadata.file_type().is_file(),
        "runner service acknowledgement is not a regular file"
    );
    #[cfg(unix)]
    anyhow::ensure!(
        metadata.uid() == current_uid()? && metadata.permissions().mode() & 0o777 == 0o600,
        "runner service acknowledgement is not a private current-user file"
    );

    let ack: RunnerServiceAck = serde_json::from_slice(&fs::read(path).with_context(|| {
        format!(
            "failed to read runner service acknowledgement {}",
            path.display()
        )
    })?)
    .context("runner service acknowledgement is invalid")?;
    anyhow::ensure!(
        ack.schema_version == RUNNER_SERVICE_ACK_SCHEMA_VERSION,
        "runner service acknowledgement schema is unsupported"
    );
    anyhow::ensure!(
        ack.pid == expected_pid,
        "runner has not acknowledged from the active service process"
    );
    anyhow::ensure!(
        ack.runner_id == config.runner_id,
        "runner acknowledgement belongs to a different runner"
    );
    anyhow::ensure!(
        ack.daemon_url_sha256 == runner_daemon_url_fingerprint(&config.daemon_url),
        "runner acknowledgement belongs to a different backend"
    );
    anyhow::ensure!(
        ack.executable_identity == runner_executable_identity_marker(executable)?,
        "runner acknowledgement belongs to a different executable"
    );
    anyhow::ensure!(
        ack.version == runner_version_for_executable(executable),
        "runner acknowledgement belongs to a different release"
    );
    anyhow::ensure!(
        ack.protocol_version == RUNNER_PROTOCOL_VERSION,
        "runner acknowledgement uses a different protocol"
    );
    if let Some(not_before) = not_before {
        anyhow::ensure!(
            ack.acknowledged_at >= not_before,
            "runner acknowledgement predates this service start"
        );
    }
    let now = now_unix();
    anyhow::ensure!(
        ack.acknowledged_at <= now.saturating_add(30),
        "runner acknowledgement timestamp is in the future"
    );
    anyhow::ensure!(
        now.saturating_sub(ack.acknowledged_at) <= max_age.as_secs() as i64,
        "runner has not acknowledged the backend recently"
    );
    Ok(ack)
}

pub fn runner_release_marker(path: &Path) -> anyhow::Result<String> {
    Ok(RunnerReleaseMarker::new(executable_identity(path)?).encode())
}

fn read_runner_release_marker(path: &Path) -> anyhow::Result<Option<RunnerReleaseMarker>> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to read runner release marker {}", path.display())
            });
        }
    };
    RunnerReleaseMarker::decode(&raw)
        .with_context(|| format!("invalid runner release marker {}", path.display()))
        .map(Some)
}

struct RunnerReleaseWatch {
    marker_path: PathBuf,
    started_from: ExecutableIdentity,
    observed_commit: Option<RunnerReleaseMarker>,
}

impl RunnerReleaseWatch {
    fn for_current_executable() -> anyhow::Result<Self> {
        let executable_path =
            std::env::current_exe().context("failed to locate the running runner executable")?;
        let install_root = executable_path
            .parent()
            .and_then(Path::parent)
            .context("runner executable path has no install root")?;
        let marker_path = install_root.join(RUNNER_RELEASE_MARKER_FILE);
        Self::for_paths(executable_path, marker_path)
    }

    fn for_paths(executable_path: PathBuf, marker_path: PathBuf) -> anyhow::Result<Self> {
        let started_from = executable_identity(&executable_path)?;
        let observed_commit = match read_runner_release_marker(&marker_path) {
            Ok(commit) => commit,
            Err(error) => {
                eprintln!("Warning: could not read the committed runner release marker: {error:#}");
                None
            }
        };
        Ok(Self {
            marker_path,
            started_from,
            observed_commit,
        })
    }

    fn replacement_committed(&mut self) -> anyhow::Result<bool> {
        let Some(committed) = read_runner_release_marker(&self.marker_path)? else {
            return Ok(false);
        };
        let commit_changed = self.observed_commit.as_ref() != Some(&committed);
        self.observed_commit = Some(committed.clone());
        Ok(commit_changed && committed.executable != self.started_from)
    }
}

fn runner_should_retire(watch: &mut RunnerReleaseWatch) -> bool {
    match watch.replacement_committed() {
        Ok(retire) => retire,
        Err(error) => {
            eprintln!("Warning: could not check for a committed runner update: {error:#}");
            false
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AndroidSigningInputs {
    keystore_bytes: Vec<u8>,
    keystore_password: String,
    key_alias: String,
    key_password: String,
}

impl Drop for AndroidSigningInputs {
    fn drop(&mut self) {
        self.keystore_bytes.zeroize();
        self.keystore_password.zeroize();
        self.key_alias.zeroize();
        self.key_password.zeroize();
    }
}

fn decode_base64_keystore(value: &str) -> anyhow::Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(value))
        .map_err(|e| {
            anyhow::anyhow!("invalid base64 value in {OORE_ANDROID_KEYSTORE_B64_ENV}: {e}")
        })
}

fn android_signing_prepared_marker(source: &str, variant: AndroidSigningBuildType) -> String {
    format!(
        "[oore-signing] {}",
        serde_json::json!({
            "event": "android_signing_reserved",
            "source": source,
            "variant": match variant {
                AndroidSigningBuildType::Debug => "debug",
                AndroidSigningBuildType::Release => "release",
            },
            "delivery": "runner_owned_post_build_signer",
        })
    )
}

fn is_android_flutter_build_command(command: &str) -> bool {
    let trimmed = command.trim_start();
    trimmed.starts_with("flutter build apk")
        || trimmed.starts_with("fvm flutter build apk")
        || trimmed.starts_with("flutter build appbundle")
        || trimmed.starts_with("fvm flutter build appbundle")
}

fn android_signing_variant_for_command(command: &str) -> Option<AndroidSigningBuildType> {
    if !is_android_flutter_build_command(command) {
        return None;
    }
    if command.contains("--debug") {
        Some(AndroidSigningBuildType::Debug)
    } else {
        Some(AndroidSigningBuildType::Release)
    }
}

fn determine_android_signing_variant(
    build_commands: &[String],
) -> anyhow::Result<Option<AndroidSigningBuildType>> {
    let mut current: Option<AndroidSigningBuildType> = None;
    for command in build_commands {
        let Some(variant) = android_signing_variant_for_command(command) else {
            continue;
        };
        match current {
            None => current = Some(variant),
            Some(existing) if existing == variant => {}
            Some(_) => {
                anyhow::bail!(
                    "mixed Android build variants detected in one build (debug and release). Use one variant per pipeline run."
                );
            }
        }
    }
    Ok(current)
}

fn signing_inputs_from_runner_profile(
    profile: &RunnerAndroidSigningProfile,
) -> anyhow::Result<AndroidSigningInputs> {
    let keystore_bytes = decode_base64_keystore(&profile.keystore_base64)?;
    if keystore_bytes.is_empty() {
        anyhow::bail!("runner signing profile keystore is empty");
    }
    Ok(AndroidSigningInputs {
        keystore_bytes,
        keystore_password: profile.store_password.clone(),
        key_alias: profile.key_alias.clone(),
        key_password: profile.key_password.clone(),
    })
}

fn zeroize_ios_signing_bundle(bundle: &mut RunnerIosSigningBundle) {
    bundle.p12_base64.zeroize();
    bundle.p12_password.zeroize();
    for profile in &mut bundle.provisioning_profiles {
        profile.profile_base64.zeroize();
    }
}

async fn fetch_job_android_signing(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    signing_token: &str,
) -> anyhow::Result<Option<RunnerAndroidSigningResponse>> {
    let resp = client
        .get(format!(
            "{}/v1/runners/{}/jobs/{}/android-signing",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .header("x-oore-signing-token", signing_token)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Android signing lookup failed: {}", resp.status());
    }

    let payload: RunnerAndroidSigningResponse = resp.json().await?;
    Ok(Some(payload))
}

fn select_runner_signing_profile(
    response: &RunnerAndroidSigningResponse,
    variant: AndroidSigningBuildType,
) -> Option<&RunnerAndroidSigningProfile> {
    match variant {
        AndroidSigningBuildType::Debug => response.debug.as_ref(),
        AndroidSigningBuildType::Release => response.release.as_ref(),
    }
}

struct PrivateSigningDirectory {
    path: PathBuf,
}

impl Drop for PrivateSigningDirectory {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn android_artifact_extension(command: &str) -> Option<&'static str> {
    if !is_android_flutter_build_command(command) {
        None
    } else if command.split_whitespace().any(|part| part == "appbundle") {
        Some("aab")
    } else {
        Some("apk")
    }
}

#[derive(Debug, Clone)]
struct AndroidSigningToolchain {
    apksigner: PathBuf,
    jarsigner: PathBuf,
    zip: PathBuf,
    java_home: Option<PathBuf>,
}

#[derive(Debug)]
struct ResolvedAndroidJava {
    home: PathBuf,
    jarsigner: PathBuf,
}

fn canonical_file(path: &Path) -> Option<PathBuf> {
    if !path.is_file() {
        return None;
    }
    fs::canonicalize(path).ok()
}

fn fixed_system_executable(name: &str) -> Option<PathBuf> {
    ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
        .into_iter()
        .find_map(|root| canonical_file(&Path::new(root).join(name)))
}

fn find_apksigner() -> PathBuf {
    let mut roots = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(PathBuf::from))
        .collect::<Vec<_>>();
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Library/Android/sdk"));
    }

    for root in roots {
        let build_tools = root.join("build-tools");
        let mut candidates = fs::read_dir(build_tools)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("apksigner"))
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        candidates.sort();
        while let Some(path) = candidates.pop() {
            let Some(apksigner) = canonical_file(&path) else {
                continue;
            };
            return apksigner;
        }
    }
    fixed_system_executable("apksigner").unwrap_or_else(|| PathBuf::from("apksigner"))
}

fn resolve_android_signing_java(home: &Path) -> Option<ResolvedAndroidJava> {
    let home = fs::canonicalize(home).ok()?;
    canonical_file(&home.join("bin/java"))?;
    let jarsigner = canonical_file(&home.join("bin/jarsigner"))?;
    Some(ResolvedAndroidJava { home, jarsigner })
}

fn find_android_signing_java() -> Option<ResolvedAndroidJava> {
    if let Some(path) = std::env::var_os("JAVA_HOME").map(PathBuf::from)
        && let Some(java) = resolve_android_signing_java(&path)
    {
        return Some(java);
    }

    #[cfg(target_os = "macos")]
    {
        let mut application_roots = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            application_roots.push(PathBuf::from(home).join("Applications"));
        }
        application_roots.push(PathBuf::from("/Applications"));
        for root in application_roots {
            for app in ["Android Studio.app", "Android Studio Preview.app"] {
                let app_root = root.join(app);
                let home = app_root.join("Contents/jbr/Contents/Home");
                if let Some(java) = resolve_android_signing_java(&home) {
                    return Some(java);
                }
            }
        }

        if let Ok(output) = Command::new("/usr/libexec/java_home").output()
            && output.status.success()
        {
            let home = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
            if let Some(java) = resolve_android_signing_java(&home) {
                return Some(java);
            }
        }
    }

    None
}

impl AndroidSigningToolchain {
    fn discover() -> Self {
        let apksigner = find_apksigner();
        let java = find_android_signing_java();
        let (java_home, jarsigner) = if let Some(java) = java {
            (Some(java.home), java.jarsigner)
        } else {
            (
                None,
                fixed_system_executable("jarsigner").unwrap_or_else(|| PathBuf::from("jarsigner")),
            )
        };
        Self {
            apksigner,
            jarsigner,
            zip: fixed_system_executable("zip").unwrap_or_else(|| PathBuf::from("zip")),
            java_home,
        }
    }

    fn system_path() -> anyhow::Result<OsString> {
        std::env::join_paths(
            ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
                .into_iter()
                .map(PathBuf::from),
        )
        .context("invalid fixed system PATH")
    }

    fn signer_path(&self) -> anyhow::Result<OsString> {
        let mut paths = Vec::new();
        if let Some(java_home) = &self.java_home {
            paths.push(java_home.join("bin"));
        }
        paths.extend(
            ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
                .into_iter()
                .map(PathBuf::from),
        );
        std::env::join_paths(paths).context("invalid fixed Android signer PATH")
    }
}

fn run_android_signer_command(
    toolchain: &AndroidSigningToolchain,
    program: &Path,
    args: &[String],
    inputs: &AndroidSigningInputs,
    action: &str,
) -> anyhow::Result<()> {
    let mut command = Command::new(program);
    command
        .args(args)
        .env(ANDROID_SIGNER_STORE_PASSWORD_ENV, &inputs.keystore_password)
        .env(ANDROID_SIGNER_KEY_PASSWORD_ENV, &inputs.key_password)
        .env("PATH", toolchain.signer_path()?);
    if let Some(java_home) = &toolchain.java_home {
        command.env("JAVA_HOME", java_home);
    } else {
        command.env_remove("JAVA_HOME");
    }
    let output = command.output().map_err(|error| {
        anyhow::anyhow!("failed to {action} with {}: {error}", program.display())
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!("failed to {action}: {stderr}");
    }
    Ok(())
}

fn strip_android_bundle_signatures(
    toolchain: &AndroidSigningToolchain,
    artifact: &Path,
) -> anyhow::Result<()> {
    let output = Command::new(&toolchain.zip)
        .args([
            "-d",
            artifact.to_str().unwrap_or_default(),
            "META-INF/*.SF",
            "META-INF/*.RSA",
            "META-INF/*.DSA",
            "META-INF/*.EC",
            "META-INF/MANIFEST.MF",
        ])
        .env("PATH", AndroidSigningToolchain::system_path()?)
        .env_remove(ANDROID_SIGNER_STORE_PASSWORD_ENV)
        .env_remove(ANDROID_SIGNER_KEY_PASSWORD_ENV)
        .output()
        .with_context(|| {
            format!(
                "failed to strip existing AAB signatures with {}",
                toolchain.zip.display()
            )
        })?;
    if !output.status.success() && output.status.code() != Some(12) {
        anyhow::bail!(
            "failed to strip existing AAB signatures: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn scrub_managed_runner_env(command: &mut tokio::process::Command) {
    for key in MANAGED_ANDROID_SIGNING_ENV_KEYS {
        command.env_remove(key);
    }
    command.env_remove(RUNNER_SERVICE_ACK_PATH_ENV);
}

fn android_artifacts_for_signing(
    workspace: &Path,
    extension: &str,
) -> anyhow::Result<Vec<PathBuf>> {
    let outputs = workspace.join("build").join("app").join("outputs");
    let mut artifacts = walk_artifact_candidates(&outputs)
        .into_iter()
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
        .collect::<Vec<_>>();
    artifacts.sort();
    if artifacts.is_empty() {
        anyhow::bail!(
            "no .{extension} artifact was produced under {}",
            outputs.display()
        );
    }
    Ok(artifacts)
}

fn sign_android_artifacts(
    workspace: &Path,
    signing_workspace: &Path,
    command: &str,
    inputs: &AndroidSigningInputs,
    toolchain: &AndroidSigningToolchain,
) -> anyhow::Result<Vec<PathBuf>> {
    let extension = android_artifact_extension(command)
        .ok_or_else(|| anyhow::anyhow!("unsupported Android signing command"))?;
    let artifacts = android_artifacts_for_signing(workspace, extension)?;
    let signing_dir = create_private_workspace_in(signing_workspace, "android-signer")?;
    let _cleanup = PrivateSigningDirectory {
        path: signing_dir.clone(),
    };
    let keystore_path = signing_dir.join("managed-keystore.jks");
    write_private_file(&keystore_path, &inputs.keystore_bytes)?;
    for (index, artifact) in artifacts.iter().enumerate() {
        let signed_artifact = signing_dir.join(format!("signed-{index}.{extension}"));
        if extension == "apk" {
            run_android_signer_command(
                toolchain,
                &toolchain.apksigner,
                &[
                    "sign".to_string(),
                    "--ks".to_string(),
                    keystore_path.display().to_string(),
                    "--ks-key-alias".to_string(),
                    inputs.key_alias.clone(),
                    "--ks-pass".to_string(),
                    format!("env:{ANDROID_SIGNER_STORE_PASSWORD_ENV}"),
                    "--key-pass".to_string(),
                    format!("env:{ANDROID_SIGNER_KEY_PASSWORD_ENV}"),
                    "--out".to_string(),
                    signed_artifact.display().to_string(),
                    artifact.display().to_string(),
                ],
                inputs,
                "sign Android APK",
            )?;
            run_android_signer_command(
                toolchain,
                &toolchain.apksigner,
                &[
                    "verify".to_string(),
                    "--verbose".to_string(),
                    "--print-certs".to_string(),
                    signed_artifact.display().to_string(),
                ],
                inputs,
                "verify Android APK signature",
            )?;
        } else {
            fs::copy(artifact, &signed_artifact)?;
            strip_android_bundle_signatures(toolchain, &signed_artifact)?;
            run_android_signer_command(
                toolchain,
                &toolchain.jarsigner,
                &[
                    "-keystore".to_string(),
                    keystore_path.display().to_string(),
                    "-storepass:env".to_string(),
                    ANDROID_SIGNER_STORE_PASSWORD_ENV.to_string(),
                    "-keypass:env".to_string(),
                    ANDROID_SIGNER_KEY_PASSWORD_ENV.to_string(),
                    signed_artifact.display().to_string(),
                    inputs.key_alias.clone(),
                ],
                inputs,
                "sign Android App Bundle",
            )?;
            run_android_signer_command(
                toolchain,
                &toolchain.jarsigner,
                &[
                    "-verify".to_string(),
                    "-strict".to_string(),
                    signed_artifact.display().to_string(),
                ],
                inputs,
                "verify Android App Bundle signature",
            )?;
        }

        fs::copy(&signed_artifact, artifact).map_err(|error| {
            anyhow::anyhow!(
                "failed to replace Android artifact {}: {error}",
                artifact.display()
            )
        })?;
    }
    Ok(artifacts)
}

#[derive(Debug, Clone)]
struct IosSigningMaterialization {
    keychain_path: PathBuf,
    export_options_plist_path: PathBuf,
    bundle_profile_mapping: Vec<(String, String)>,
    bundle_profile_paths: Vec<(String, PathBuf)>,
    effective_export_method: String,
    signing_identity_sha1: String,
    signing_identity_name: Option<String>,
}

#[derive(Debug, Clone)]
struct IosAppMetadata {
    bundle_identifier: String,
    display_name: String,
    version: String,
    build_number: String,
}

#[derive(Debug)]
struct SignedIosArchive {
    ipa_path: PathBuf,
    app: IosAppMetadata,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct IosCleanupJournal {
    keychain_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original_default_keychain: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    original_keychains: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    installed_profiles: Vec<PathBuf>,
}

struct IosSigningCleanup {
    journal_path: Option<PathBuf>,
    journal: IosCleanupJournal,
}

impl IosSigningCleanup {
    fn cleanup(&mut self) -> anyhow::Result<()> {
        if self.journal_path.is_none() {
            return Ok(());
        }
        cleanup_ios_signing_state(&self.journal)?;
        let journal_path = self
            .journal_path
            .take()
            .expect("journal path checked before cleanup");
        match fs::remove_file(&journal_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => {
                self.journal_path = Some(journal_path.clone());
                Err(anyhow::anyhow!(
                    "failed to remove iOS cleanup journal {}: {error}",
                    journal_path.display()
                ))
            }
        }
    }
}

impl Drop for IosSigningCleanup {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            eprintln!("Warning: failed to clean up iOS signing state: {error:#}");
        }
    }
}

fn run_security_command(args: &[&str]) -> anyhow::Result<String> {
    run_security_command_with_strings(
        &args
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    )
}

fn require_ios_signing_user_session() -> anyhow::Result<()> {
    // SAFETY: `geteuid` accepts no arguments and has no failure state.
    let uid = unsafe { libc::geteuid() };
    let output = Command::new("/bin/launchctl")
        .args(["print", &format!("gui/{uid}")])
        .output()
        .context("failed to inspect the runner account login session")?;
    anyhow::ensure!(
        output.status.success(),
        "iOS signing requires an active macOS login session. Log into the runner account and retry the build"
    );
    if is_managed_system_runner() {
        active_gui_session_process_id(uid)?;
    }
    Ok(())
}

fn is_managed_system_runner() -> bool {
    std::env::var_os("XPC_SERVICE_NAME").is_some_and(|name| name == MANAGED_RUNNER_SERVICE_LABEL)
}

fn active_gui_session_process_id(uid: u32) -> anyhow::Result<u32> {
    let output = Command::new("/usr/bin/pgrep")
        .args(["-u", &uid.to_string(), "-x", "Dock"])
        .output()
        .context("failed to inspect the runner account GUI session")?;
    let pid = String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.trim().parse::<u32>().ok())
        .filter(|pid| *pid > 0);
    pid.ok_or_else(|| {
        anyhow::anyhow!(
            "iOS signing requires an active macOS login session. Log into the runner account and retry the build"
        )
    })
}

fn ios_signing_command(program: &str, args: &[String]) -> Command {
    let mut command = Command::new(program);
    command.args(args);
    command
}

fn run_security_command_with_strings(args: &[String]) -> anyhow::Result<String> {
    let output = ios_signing_command("/usr/bin/security", args)
        .output()
        .map_err(|e| anyhow::anyhow!("failed to execute security command: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("security command failed: {stderr}");
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn cleanup_ios_signing_state(journal: &IosCleanupJournal) -> anyhow::Result<()> {
    let mut errors = Vec::new();
    let keychain_path = journal.keychain_path.display().to_string();
    if let Some(original_default_keychain) = &journal.original_default_keychain {
        match run_security_command(&["default-keychain", "-d", "user"]) {
            Ok(output)
                if parse_keychain_list(&output).into_iter().next().as_deref()
                    == Some(keychain_path.as_str()) =>
            {
                if let Err(error) = run_security_command_with_strings(&[
                    "default-keychain".to_string(),
                    "-d".to_string(),
                    "user".to_string(),
                    "-s".to_string(),
                    original_default_keychain.clone(),
                ]) {
                    errors.push(format!("failed to restore default keychain: {error:#}"));
                }
            }
            Ok(_) => {}
            Err(error) => errors.push(format!("failed to inspect default keychain: {error:#}")),
        }

        match run_security_command(&["list-keychains", "-d", "user"]) {
            Ok(output) => {
                let current_keychains = parse_keychain_list(&output);
                if current_keychains.iter().any(|path| path == &keychain_path)
                    && let Err(error) = run_security_command_with_strings(
                        &[
                            "list-keychains".to_string(),
                            "-d".to_string(),
                            "user".to_string(),
                            "-s".to_string(),
                        ]
                        .into_iter()
                        .chain(
                            current_keychains
                                .into_iter()
                                .filter(|path| path != &keychain_path),
                        )
                        .collect::<Vec<_>>(),
                    )
                {
                    errors.push(format!("failed to restore keychain search list: {error:#}"));
                }
            }
            Err(error) => errors.push(format!("failed to inspect keychain search list: {error:#}")),
        }
    }

    if journal.keychain_path.exists() {
        if let Err(error) =
            run_security_command_with_strings(&["delete-keychain".to_string(), keychain_path])
        {
            errors.push(format!("failed to delete build keychain: {error:#}"));
        }
        match fs::remove_file(&journal.keychain_path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => errors.push(format!(
                "failed to remove iOS signing keychain {}: {error}",
                journal.keychain_path.display()
            )),
        }
    }

    for profile in &journal.installed_profiles {
        match fs::remove_file(profile) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => errors.push(format!(
                "failed to remove installed provisioning profile {}: {error}",
                profile.display()
            )),
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(errors.join("; "))
    }
}

fn write_ios_cleanup_journal(path: &Path, journal: &IosCleanupJournal) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(journal)?;
    let temporary_path = path.with_extension("tmp");
    write_private_file(&temporary_path, &bytes).map_err(|error| {
        anyhow::anyhow!(
            "failed to write iOS cleanup journal {}: {error}",
            temporary_path.display()
        )
    })?;
    fs::rename(&temporary_path, path).map_err(|error| {
        anyhow::anyhow!(
            "failed to publish iOS cleanup journal {}: {error}",
            path.display()
        )
    })?;
    if let Some(parent) = path.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn parse_keychain_list(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            if let Some((_, rest)) = trimmed.split_once('"')
                && let Some((path, _)) = rest.split_once('"')
            {
                return Some(path.to_string());
            }
            Some(trimmed.to_string())
        })
        .collect()
}

fn is_missing_legacy_oore_keychain(path: &str) -> bool {
    let path = Path::new(path);
    if path.exists() {
        return false;
    }

    let relative = path
        .strip_prefix("/private/tmp")
        .or_else(|_| path.strip_prefix("/tmp"));
    let Ok(relative) = relative else {
        return false;
    };
    let components = relative.components().collect::<Vec<_>>();
    components.len() == 5
        && matches!(
            components[0],
            Component::Normal(name) if name == "oore-builds" || name == "oore-builds.noindex"
        )
        && matches!(components[1], Component::Normal(_))
        && matches!(components[2], Component::Normal(name) if name == ".oore")
        && matches!(components[3], Component::Normal(name) if name == "ios-signing")
        && matches!(
            components[4],
            Component::Normal(name) if name == "oore-ci-build.keychain-db"
        )
}

#[cfg(target_os = "macos")]
fn remove_missing_legacy_oore_keychains_from_search_list() -> anyhow::Result<usize> {
    let current = parse_keychain_list(&run_security_command(&["list-keychains", "-d", "user"])?);
    let retained = current
        .iter()
        .filter(|path| !is_missing_legacy_oore_keychain(path))
        .cloned()
        .collect::<Vec<_>>();
    let removed = current.len().saturating_sub(retained.len());
    if removed == 0 {
        return Ok(0);
    }
    anyhow::ensure!(
        !retained.is_empty(),
        "refusing to replace the user keychain search list with an empty list"
    );

    let args = [
        "list-keychains".to_string(),
        "-d".to_string(),
        "user".to_string(),
        "-s".to_string(),
    ]
    .into_iter()
    .chain(retained)
    .collect::<Vec<_>>();
    run_security_command_with_strings(&args)?;
    Ok(removed)
}

#[cfg(not(target_os = "macos"))]
fn remove_missing_legacy_oore_keychains_from_search_list() -> anyhow::Result<usize> {
    Ok(0)
}

fn parse_distribution_certificate(raw: &str) -> Option<(String, String)> {
    raw.split("SHA-256 hash:").find_map(|block| {
        let name = block
            .lines()
            .find_map(|line| line.trim().strip_prefix("\"alis\"<blob>=\""))?
            .strip_suffix('"')?;
        if !name.contains("Distribution") {
            return None;
        }
        let sha1 = block
            .lines()
            .find_map(|line| line.trim().strip_prefix("SHA-1 hash:"))?
            .trim();
        (sha1.len() == 40 && sha1.chars().all(|ch| ch.is_ascii_hexdigit()))
            .then(|| (sha1.to_string(), name.to_string()))
    })
}

fn random_password_hex() -> String {
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn choose_ios_export_method(help_text: &str) -> &'static str {
    if help_text.to_ascii_lowercase().contains("release-testing") {
        "release-testing"
    } else {
        "ad-hoc"
    }
}

fn resolve_ios_export_method() -> String {
    match Command::new("xcodebuild").arg("-help").output() {
        Ok(output) if output.status.success() => {
            choose_ios_export_method(&String::from_utf8_lossy(&output.stdout)).to_string()
        }
        _ => "ad-hoc".to_string(),
    }
}

fn decode_runner_b64(value: &str, field_name: &str) -> anyhow::Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(value))
        .map_err(|e| anyhow::anyhow!("invalid base64 value for {field_name}: {e}"))
}

fn safe_ios_signing_filename(
    raw: &str,
    fallback: &str,
    field_name: &str,
) -> anyhow::Result<String> {
    let trimmed = raw.trim();
    let candidate = if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    };
    let mut components = Path::new(candidate).components();
    let is_single_normal_component =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();

    if !is_single_normal_component || candidate.contains('/') || candidate.contains('\\') {
        anyhow::bail!("{field_name} must be a filename, not a path");
    }

    Ok(candidate.to_string())
}

fn write_export_options_plist(
    output_path: &Path,
    team_id: &str,
    method: &str,
    mapping: &[(String, String)],
    signing_identity_sha1: Option<&str>,
) -> anyhow::Result<()> {
    let mut provisioning_dict = String::new();
    for (bundle_id, profile) in mapping {
        provisioning_dict.push_str("    <key>");
        provisioning_dict.push_str(bundle_id);
        provisioning_dict.push_str("</key>\n");
        provisioning_dict.push_str("    <string>");
        provisioning_dict.push_str(profile);
        provisioning_dict.push_str("</string>\n");
    }

    let signing_cert_entry = signing_identity_sha1
        .map(|sha1| format!("  <key>signingCertificate</key>\n  <string>{sha1}</string>\n"))
        .unwrap_or_default();

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>method</key>
  <string>{method}</string>
  <key>signingStyle</key>
  <string>manual</string>
  <key>teamID</key>
  <string>{team_id}</string>
  <key>destination</key>
  <string>export</string>
{signing_cert_entry}  <key>provisioningProfiles</key>
  <dict>
{provisioning_dict}  </dict>
</dict>
</plist>
"#
    );
    fs::write(output_path, plist).map_err(|e| {
        anyhow::anyhow!(
            "failed to write export options plist {}: {e}",
            output_path.display()
        )
    })
}

fn install_ios_signing_bundle(
    signing_workspace: &Path,
    bundle: &RunnerIosSigningBundle,
) -> anyhow::Result<(IosSigningMaterialization, IosSigningCleanup)> {
    install_ios_signing_bundle_with_session_check(
        signing_workspace,
        bundle,
        require_ios_signing_user_session,
    )
}

fn install_ios_signing_bundle_with_session_check(
    signing_workspace: &Path,
    bundle: &RunnerIosSigningBundle,
    require_session: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<(IosSigningMaterialization, IosSigningCleanup)> {
    require_session()?;
    if bundle.team_id.trim().is_empty() {
        anyhow::bail!("iOS signing bundle team_id is empty");
    }
    if bundle.p12_base64.trim().is_empty() {
        anyhow::bail!("iOS signing bundle p12 payload is empty");
    }
    if bundle.p12_password.trim().is_empty() {
        anyhow::bail!("iOS signing bundle p12 password is empty");
    }
    if bundle.provisioning_profiles.is_empty() {
        anyhow::bail!("iOS signing bundle has no provisioning profiles");
    }

    let signing_dir = signing_workspace.join(IOS_SIGNING_DIR);
    fs::create_dir_all(&signing_dir).map_err(|e| {
        anyhow::anyhow!(
            "failed to create iOS signing working directory {}: {e}",
            signing_dir.display()
        )
    })?;

    let p12_filename =
        safe_ios_signing_filename(&bundle.p12_filename, "distribution.p12", "p12_filename")?;
    let p12_path = signing_dir.join(p12_filename);
    let p12_bytes = decode_runner_b64(&bundle.p12_base64, "p12")?;
    if p12_bytes.is_empty() {
        anyhow::bail!("decoded iOS p12 is empty");
    }
    fs::write(&p12_path, p12_bytes)
        .map_err(|e| anyhow::anyhow!("failed to write p12 {}: {e}", p12_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&p12_path, fs::Permissions::from_mode(0o600));
    }

    let profile_work_dir = signing_dir.join("profiles");
    fs::create_dir_all(&profile_work_dir).map_err(|e| {
        anyhow::anyhow!(
            "failed to create profile work directory {}: {e}",
            profile_work_dir.display()
        )
    })?;

    let keychain_password = random_password_hex();
    let keychain_path = signing_dir.join("oore-ci-build.keychain-db");
    let keychain_path_str = keychain_path.display().to_string();
    let mut prepared_profiles = Vec::new();
    for profile in &bundle.provisioning_profiles {
        if profile.bundle_id.trim().is_empty() {
            anyhow::bail!("iOS signing bundle has profile with empty bundle_id");
        }
        let profile_bytes = decode_runner_b64(
            &profile.profile_base64,
            &format!("provisioning profile '{}'", profile.bundle_id),
        )?;
        if profile_bytes.is_empty() {
            anyhow::bail!(
                "decoded provisioning profile '{}' is empty",
                profile.bundle_id
            );
        }

        let fallback_profile_name = format!("{}.mobileprovision", profile.bundle_id);
        let work_file_name = safe_ios_signing_filename(
            &profile.profile_filename,
            &fallback_profile_name,
            "profile_filename",
        )?;
        let work_path = profile_work_dir.join(work_file_name);
        write_private_file(&work_path, &profile_bytes).map_err(|error| {
            anyhow::anyhow!("failed to write profile {}: {error}", work_path.display())
        })?;

        let profile_ref = profile
            .profile_uuid
            .clone()
            .or_else(|| profile.profile_name.clone())
            .unwrap_or_else(|| hex::encode(Sha256::digest(&profile_bytes)));
        prepared_profiles.push((profile.bundle_id.clone(), profile_ref, work_path));
    }

    let mut journal = IosCleanupJournal {
        keychain_path: keychain_path.clone(),
        original_default_keychain: None,
        original_keychains: Vec::new(),
        installed_profiles: Vec::new(),
    };
    let journal_path = signing_workspace.join(IOS_CLEANUP_JOURNAL);
    write_ios_cleanup_journal(&journal_path, &journal)?;

    let install_result: anyhow::Result<IosSigningMaterialization> = (|| {
        let mut bundle_profile_mapping = Vec::new();
        let mut bundle_profile_paths = Vec::new();
        for (bundle_id, profile_ref, work_path) in &prepared_profiles {
            bundle_profile_mapping.push((bundle_id.clone(), profile_ref.clone()));
            bundle_profile_paths.push((bundle_id.clone(), work_path.clone()));
        }

        run_security_command_with_strings(&[
            "create-keychain".to_string(),
            "-p".to_string(),
            keychain_password.clone(),
            keychain_path_str.clone(),
        ])?;

        run_security_command_with_strings(&[
            "set-keychain-settings".to_string(),
            "-lut".to_string(),
            "21600".to_string(),
            keychain_path_str.clone(),
        ])?;
        run_security_command_with_strings(&[
            "unlock-keychain".to_string(),
            "-p".to_string(),
            keychain_password.clone(),
            keychain_path_str.clone(),
        ])?;
        run_security_command_with_strings(&[
            "set-keychain-settings".to_string(),
            "-lut".to_string(),
            "21600".to_string(),
            keychain_path_str.clone(),
        ])?;

        let original_default_keychain =
            parse_keychain_list(&run_security_command(&["default-keychain", "-d", "user"])?)
                .into_iter()
                .next()
                .context("user keychain domain has no default keychain")?;
        let original_keychains =
            parse_keychain_list(&run_security_command(&["list-keychains", "-d", "user"])?);
        anyhow::ensure!(
            !original_keychains.is_empty(),
            "user keychain search list is empty"
        );
        journal.original_default_keychain = Some(original_default_keychain);
        journal.original_keychains = original_keychains.clone();
        write_ios_cleanup_journal(&journal_path, &journal)?;
        run_security_command_with_strings(
            &[
                "list-keychains".to_string(),
                "-d".to_string(),
                "user".to_string(),
                "-s".to_string(),
                keychain_path_str.clone(),
            ]
            .into_iter()
            .chain(original_keychains)
            .collect::<Vec<_>>(),
        )?;
        run_security_command_with_strings(&[
            "default-keychain".to_string(),
            "-d".to_string(),
            "user".to_string(),
            "-s".to_string(),
            keychain_path_str.clone(),
        ])?;

        run_security_command_with_strings(&ios_keychain_import_arguments(
            &p12_path,
            &keychain_path,
            &bundle.p12_password,
        ))?;

        run_security_command_with_strings(&[
            "set-key-partition-list".to_string(),
            "-S".to_string(),
            "apple-tool:,apple:,codesign:".to_string(),
            "-s".to_string(),
            "-k".to_string(),
            keychain_password.clone(),
            keychain_path_str.clone(),
        ])?;

        run_security_command_with_strings(&[
            "find-key".to_string(),
            "-s".to_string(),
            "-t".to_string(),
            "private".to_string(),
            keychain_path_str.clone(),
        ])
        .map_err(|error| {
            anyhow::anyhow!("no sign-capable private key found after importing p12: {error}")
        })?;

        let certificates = run_security_command_with_strings(&[
            "find-certificate".to_string(),
            "-a".to_string(),
            "-Z".to_string(),
            keychain_path_str.clone(),
        ])?;
        let (signing_identity_sha1, signing_identity_name) =
            parse_distribution_certificate(&certificates).ok_or_else(|| {
                anyhow::anyhow!(
                    "no Apple Distribution certificate found after importing p12:\n{}",
                    certificates.trim()
                )
            })?;

        let signing_preflight_path = signing_dir.join("oore-signing-preflight");
        fs::copy("/usr/bin/true", &signing_preflight_path).map_err(|error| {
            anyhow::anyhow!("failed to prepare iOS signing identity preflight: {error}")
        })?;
        run_ios_signing_tool(
            "/usr/bin/codesign",
            vec![
                "--force".to_string(),
                "--sign".to_string(),
                signing_identity_sha1.clone(),
                "--keychain".to_string(),
                keychain_path_str.clone(),
                "--timestamp=none".to_string(),
                signing_preflight_path.display().to_string(),
            ],
            "use the imported iOS signing identity from the build keychain",
        )?;
        run_ios_signing_tool(
            "/usr/bin/codesign",
            vec![
                "--verify".to_string(),
                "--strict".to_string(),
                signing_preflight_path.display().to_string(),
            ],
            "verify imported iOS signing identity access",
        )?;
        let _ = fs::remove_file(signing_preflight_path);

        let effective_export_method = resolve_ios_export_method();
        let export_options_plist_path = signing_dir.join("ExportOptions.plist");
        write_export_options_plist(
            &export_options_plist_path,
            bundle.team_id.trim(),
            &effective_export_method,
            &bundle_profile_mapping,
            Some(&signing_identity_sha1),
        )?;

        Ok(IosSigningMaterialization {
            keychain_path: keychain_path.clone(),
            export_options_plist_path,
            bundle_profile_mapping,
            bundle_profile_paths,
            effective_export_method,
            signing_identity_sha1,
            signing_identity_name: Some(signing_identity_name),
        })
    })();

    match install_result {
        Ok(materialization) => Ok((
            materialization,
            IosSigningCleanup {
                journal_path: Some(journal_path),
                journal,
            },
        )),
        Err(err) => {
            match cleanup_ios_signing_state(&journal)
                .and_then(|()| fs::remove_file(&journal_path).map_err(anyhow::Error::from))
            {
                Ok(()) => Err(err),
                Err(cleanup_error) => Err(err.context(format!(
                    "iOS signing cleanup was deferred for startup reconciliation: {cleanup_error:#}"
                ))),
            }
        }
    }
}

fn ios_keychain_import_arguments(
    p12_path: &Path,
    keychain_path: &Path,
    password: &str,
) -> Vec<String> {
    vec![
        "import".to_string(),
        p12_path.display().to_string(),
        "-f".to_string(),
        "pkcs12".to_string(),
        "-k".to_string(),
        keychain_path.display().to_string(),
        "-P".to_string(),
        password.to_string(),
        "-T".to_string(),
        "/usr/bin/codesign".to_string(),
    ]
}

fn run_ios_signing_tool(program: &str, args: Vec<String>, action: &str) -> anyhow::Result<String> {
    let output = ios_signing_command(program, &args)
        .output()
        .map_err(|error| anyhow::anyhow!("failed to {action}: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let mut detail = if stderr.is_empty() { stdout } else { stderr };
        if detail.contains("errSecInternalComponent")
            || detail.contains("User interaction is not allowed")
        {
            detail.push_str(
                "; the build keychain could not authorize non-interactive signing. Re-import the signing credential in Oore and retry the build",
            );
        }
        anyhow::bail!("failed to {action}: {detail}");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn collect_paths_with_extension(
    root: &Path,
    extension: &str,
    output: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(root)
        .map_err(|error| anyhow::anyhow!("failed to inspect {}: {error}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if matches!(
                path.file_name().and_then(|value| value.to_str()),
                Some(".git" | ".dart_tool" | "Pods")
            ) {
                continue;
            }
            if path.extension().and_then(|value| value.to_str()) == Some(extension) {
                output.push(path.clone());
            }
            collect_paths_with_extension(&path, extension, output)?;
        }
    }
    Ok(())
}

fn find_newest_path_with_extension(root: &Path, extension: &str) -> anyhow::Result<PathBuf> {
    let mut candidates = Vec::new();
    collect_paths_with_extension(root, extension, &mut candidates)?;
    candidates
        .into_iter()
        .max_by_key(|path| {
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .ok()
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no .{extension} bundle was produced under {}",
                root.display()
            )
        })
}

fn find_direct_path_with_extension(root: &Path, extension: &str) -> anyhow::Result<PathBuf> {
    fs::read_dir(root)
        .map_err(|error| anyhow::anyhow!("failed to inspect {}: {error}", root.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir() && path.extension().and_then(|value| value.to_str()) == Some(extension)
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no direct .{extension} bundle was produced under {}",
                root.display()
            )
        })
}

fn read_apple_bundle_identifier(bundle_path: &Path) -> anyhow::Result<String> {
    run_ios_signing_tool(
        "/usr/libexec/PlistBuddy",
        vec![
            "-c".to_string(),
            "Print :CFBundleIdentifier".to_string(),
            bundle_path.join("Info.plist").display().to_string(),
        ],
        &format!("read bundle identifier from {}", bundle_path.display()),
    )
}

fn read_apple_bundle_value(bundle_path: &Path, key: &str) -> Option<String> {
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args([
            "-c".to_string(),
            format!("Print :{key}"),
            bundle_path.join("Info.plist").display().to_string(),
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn read_ios_app_metadata(bundle_path: &Path) -> anyhow::Result<IosAppMetadata> {
    let bundle_identifier = read_apple_bundle_identifier(bundle_path)?;
    let display_name = read_apple_bundle_value(bundle_path, "CFBundleDisplayName")
        .or_else(|| read_apple_bundle_value(bundle_path, "CFBundleName"))
        .or_else(|| {
            bundle_path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_string)
        })
        .ok_or_else(|| anyhow::anyhow!("app bundle is missing a display name"))?;
    let version = read_apple_bundle_value(bundle_path, "CFBundleShortVersionString")
        .ok_or_else(|| anyhow::anyhow!("app bundle is missing CFBundleShortVersionString"))?;
    let build_number = read_apple_bundle_value(bundle_path, "CFBundleVersion")
        .ok_or_else(|| anyhow::anyhow!("app bundle is missing CFBundleVersion"))?;

    Ok(IosAppMetadata {
        bundle_identifier,
        display_name,
        version,
        build_number,
    })
}

fn profile_path_for_bundle<'a>(
    materialization: &'a IosSigningMaterialization,
    bundle_id: &str,
) -> anyhow::Result<&'a Path> {
    materialization
        .bundle_profile_paths
        .iter()
        .find(|(candidate, _)| candidate == bundle_id)
        .map(|(_, path)| path.as_path())
        .ok_or_else(|| {
            anyhow::anyhow!("no stored provisioning profile matches bundle ID {bundle_id}")
        })
}

fn extract_profile_entitlements(profile_path: &Path, output_path: &Path) -> anyhow::Result<()> {
    let decoded_path = output_path.with_extension("profile.plist");
    run_ios_signing_tool(
        "/usr/bin/security",
        vec![
            "cms".to_string(),
            "-D".to_string(),
            "-i".to_string(),
            profile_path.display().to_string(),
            "-o".to_string(),
            decoded_path.display().to_string(),
        ],
        &format!("decode provisioning profile {}", profile_path.display()),
    )?;
    run_ios_signing_tool(
        "/usr/bin/plutil",
        vec![
            "-extract".to_string(),
            "Entitlements".to_string(),
            "xml1".to_string(),
            "-o".to_string(),
            output_path.display().to_string(),
            decoded_path.display().to_string(),
        ],
        &format!("extract entitlements from {}", profile_path.display()),
    )?;
    let _ = fs::remove_file(decoded_path);
    Ok(())
}

fn codesign_path(
    path: &Path,
    materialization: &IosSigningMaterialization,
    entitlements: Option<&Path>,
) -> anyhow::Result<()> {
    let mut args = vec![
        "--force".to_string(),
        "--sign".to_string(),
        materialization.signing_identity_sha1.clone(),
        "--keychain".to_string(),
        materialization.keychain_path.display().to_string(),
        "--timestamp=none".to_string(),
    ];
    if let Some(entitlements) = entitlements {
        args.push("--entitlements".to_string());
        args.push(entitlements.display().to_string());
    }
    args.push(path.display().to_string());
    run_ios_signing_tool(
        "/usr/bin/codesign",
        args,
        &format!("sign {}", path.display()),
    )?;
    Ok(())
}

fn collect_nested_code(root: &Path, output: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in fs::read_dir(root)
        .map_err(|error| anyhow::anyhow!("failed to inspect {}: {error}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_nested_code(&path, output)?;
            if matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("framework" | "xpc")
            ) {
                output.push(path);
            }
        } else if path.extension().and_then(|value| value.to_str()) == Some("dylib") {
            output.push(path);
        }
    }
    Ok(())
}

fn collect_provisioned_bundles_deepest_first(app: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut bundles = Vec::new();
    collect_paths_with_extension(app, "app", &mut bundles)?;
    collect_paths_with_extension(app, "appex", &mut bundles)?;
    bundles.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    bundles.push(app.to_path_buf());
    Ok(bundles)
}

fn provision_and_sign_bundle(
    bundle: &Path,
    entitlements_dir: &Path,
    materialization: &IosSigningMaterialization,
) -> anyhow::Result<()> {
    let bundle_id = read_apple_bundle_identifier(bundle)?;
    let profile = profile_path_for_bundle(materialization, &bundle_id)?;
    fs::copy(profile, bundle.join("embedded.mobileprovision"))
        .map_err(|error| anyhow::anyhow!("failed to embed profile for {bundle_id}: {error}"))?;
    let entitlements = entitlements_dir.join(format!(
        "{}.plist",
        xcode_build_setting_identifier(&bundle_id)
    ));
    extract_profile_entitlements(profile, &entitlements)?;
    codesign_path(bundle, materialization, Some(&entitlements))
}

fn manually_sign_ios_archive(
    workspace: &Path,
    materialization: &IosSigningMaterialization,
) -> anyhow::Result<SignedIosArchive> {
    let archive = find_newest_path_with_extension(workspace, "xcarchive")?;
    let applications = archive.join("Products").join("Applications");
    let app = find_direct_path_with_extension(&applications, "app")?;
    let entitlements_dir = materialization
        .export_options_plist_path
        .parent()
        .unwrap_or(workspace)
        .join("entitlements");
    fs::create_dir_all(&entitlements_dir)?;

    let mut nested_code = Vec::new();
    collect_nested_code(&app, &mut nested_code)?;
    nested_code.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in nested_code {
        codesign_path(&path, materialization, None)?;
    }

    for bundle in collect_provisioned_bundles_deepest_first(&app)? {
        provision_and_sign_bundle(&bundle, &entitlements_dir, materialization)?;
    }

    run_ios_signing_tool(
        "/usr/bin/codesign",
        vec![
            "--verify".to_string(),
            "--deep".to_string(),
            "--strict".to_string(),
            "--verbose=2".to_string(),
            app.display().to_string(),
        ],
        &format!("verify signed app {}", app.display()),
    )?;

    let app_metadata = read_ios_app_metadata(&app)?;

    let package_root = entitlements_dir.join("package");
    let payload = package_root.join("Payload");
    if package_root.exists() {
        fs::remove_dir_all(&package_root)?;
    }
    fs::create_dir_all(&payload)?;
    let packaged_app = payload.join(
        app.file_name()
            .ok_or_else(|| anyhow::anyhow!("archive app path has no filename"))?,
    );
    run_ios_signing_tool(
        "/usr/bin/ditto",
        vec![
            app.display().to_string(),
            packaged_app.display().to_string(),
        ],
        "copy signed app into IPA payload",
    )?;

    let ios_build_dir = archive
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow::anyhow!("archive path is missing its iOS build directory"))?;
    let ipa_dir = ios_build_dir.join("ipa");
    fs::create_dir_all(&ipa_dir)?;
    let ipa_path = ipa_dir.join(format!(
        "{}.ipa",
        app.file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Runner")
    ));
    if ipa_path.exists() {
        fs::remove_file(&ipa_path)?;
    }
    run_ios_signing_tool(
        "/usr/bin/ditto",
        vec![
            "-c".to_string(),
            "-k".to_string(),
            "--sequesterRsrc".to_string(),
            "--keepParent".to_string(),
            payload.display().to_string(),
            ipa_path.display().to_string(),
        ],
        "package signed IPA",
    )?;
    fs::metadata(&ipa_path)
        .map_err(|error| anyhow::anyhow!("signed IPA was not created: {error}"))?;
    Ok(SignedIosArchive {
        ipa_path,
        app: app_metadata,
    })
}

fn ios_signing_prepared_marker(
    source: &str,
    bundle: &RunnerIosSigningBundle,
    materialization: &IosSigningMaterialization,
) -> String {
    format!(
        "[oore-signing] {}",
        serde_json::json!({
            "event": "ios_signing_prepared",
            "source": source,
            "mode": match bundle.mode {
                oore_contract::IosSigningMode::Manual => "manual",
                oore_contract::IosSigningMode::Api => "api",
                oore_contract::IosSigningMode::Hybrid => "hybrid",
            },
            "team_id": bundle.team_id,
            "profiles_count": bundle.provisioning_profiles.len(),
            "export_options_plist_path": materialization.export_options_plist_path,
            "effective_export_method": materialization.effective_export_method,
            "signing_identity": materialization.signing_identity_name,
            "profile_mapping": materialization.bundle_profile_mapping,
        })
    )
}

fn is_ios_flutter_build_command(command: &str) -> bool {
    let args: Vec<&str> = command.split_whitespace().collect();
    let has_flutter_target = |target: &str| -> bool {
        args.windows(3)
            .any(|window| window == ["flutter", "build", target])
            || args
                .windows(4)
                .any(|window| window == ["fvm", "flutter", "build", target])
    };
    has_flutter_target("ios") || has_flutter_target("ipa")
}

fn contains_flutter_build_target(args: &[String], target: &str) -> bool {
    args.windows(3)
        .any(|window| window == ["flutter", "build", target])
        || args
            .windows(4)
            .any(|window| window == ["fvm", "flutter", "build", target])
}

fn rewrite_flutter_ios_target_to_ipa(args: &mut [String]) -> bool {
    let mut rewritten = false;
    for i in 0..args.len() {
        if i + 2 < args.len()
            && args[i] == "flutter"
            && args[i + 1] == "build"
            && args[i + 2] == "ios"
        {
            args[i + 2] = "ipa".to_string();
            rewritten = true;
        }
        if i + 3 < args.len()
            && args[i] == "fvm"
            && args[i + 1] == "flutter"
            && args[i + 2] == "build"
            && args[i + 3] == "ios"
        {
            args[i + 3] = "ipa".to_string();
            rewritten = true;
        }
    }
    rewritten
}

fn adapt_ios_command_for_signing(
    command: &str,
    _export_options_plist: &Path,
) -> anyhow::Result<String> {
    if !is_ios_flutter_build_command(command) {
        return Ok(command.to_string());
    }

    let mut args: Vec<String> = command
        .split_whitespace()
        .map(|part| part.to_string())
        .collect();
    if args.len() < 3 {
        return Ok(command.to_string());
    }

    if args.iter().any(|arg| arg == "--simulator") {
        anyhow::bail!(
            "iOS signing is enabled, but command uses --simulator which cannot produce installable ad-hoc IPA"
        );
    }

    let mut filtered_args = Vec::with_capacity(args.len());
    let mut remove_next_value = false;
    for arg in args {
        if remove_next_value {
            remove_next_value = false;
            continue;
        }
        if arg == "--export-method" || arg == "--export-options-plist" {
            remove_next_value = true;
            continue;
        }
        if arg == "--codesign"
            || arg == "--no-codesign"
            || arg.starts_with("--export-method=")
            || arg.starts_with("--export-options-plist=")
        {
            continue;
        }
        filtered_args.push(arg);
    }
    args = filtered_args;

    let rewrote_ios_target = rewrite_flutter_ios_target_to_ipa(&mut args);
    if !rewrote_ios_target && !contains_flutter_build_target(&args, "ipa") {
        anyhow::bail!(
            "iOS signing is enabled, but command did not contain a Flutter iOS build target that can be rewritten to ipa export"
        );
    }

    // Xcode's certificate discovery is tied to a GUI security session on some
    // headless macOS runners. Build an unsigned archive first; Oore then embeds
    // the stored profiles and signs every nested bundle directly with the exact
    // managed identity and temporary keychain.
    args.push("--no-codesign".to_string());

    Ok(args.join(" "))
}

fn xcode_build_setting_identifier(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn normalize_stage_command_for_execution(
    stage_name: &str,
    command: &str,
    dart_define_file: Option<&str>,
    ios_export_options_plist: Option<&Path>,
) -> anyhow::Result<(String, bool)> {
    let mut normalized_command = normalize_legacy_env_syntax(command);

    if stage_name == "build"
        && let Some(define_file) = dart_define_file
        && is_flutter_build_command(&normalized_command)
    {
        normalized_command = with_dart_define_file(&normalized_command, define_file);
    }

    let mut ios_signing_command_applied = false;
    if stage_name == "build"
        && let Some(export_plist) = ios_export_options_plist
        && is_ios_flutter_build_command(&normalized_command)
    {
        normalized_command = adapt_ios_command_for_signing(&normalized_command, export_plist)?;
        ios_signing_command_applied = true;
    }

    Ok((normalized_command, ios_signing_command_applied))
}

async fn fetch_job_ios_signing(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    signing_token: &str,
) -> anyhow::Result<Option<RunnerIosSigningResponse>> {
    let resp = client
        .get(format!(
            "{}/v1/runners/{}/jobs/{}/ios-signing",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .header("x-oore-signing-token", signing_token)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if body.trim().is_empty() {
            anyhow::bail!("iOS signing lookup failed: {status}");
        }
        anyhow::bail!("iOS signing lookup failed: {status} {body}");
    }

    let payload: RunnerIosSigningResponse = resp.json().await?;
    Ok(Some(payload))
}

fn validate_ios_cleanup_journal(
    workspace: &Path,
    journal: &IosCleanupJournal,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        journal.keychain_path
            == workspace
                .join(IOS_SIGNING_DIR)
                .join("oore-ci-build.keychain-db"),
        "iOS cleanup journal keychain path is outside its workspace"
    );
    let profile_root = PathBuf::from(
        std::env::var("HOME")
            .map_err(|_| anyhow::anyhow!("HOME environment variable is not set"))?,
    )
    .join("Library/MobileDevice/Provisioning Profiles");
    anyhow::ensure!(
        journal.installed_profiles.iter().all(|path| {
            path.parent() == Some(profile_root.as_path())
                && path.extension().and_then(|extension| extension.to_str())
                    == Some("mobileprovision")
        }),
        "iOS cleanup journal contains an invalid provisioning profile path"
    );
    match &journal.original_default_keychain {
        Some(default_keychain) => anyhow::ensure!(
            journal.original_keychains.contains(default_keychain),
            "iOS cleanup journal has no original default keychain"
        ),
        None => anyhow::ensure!(
            journal.original_keychains.is_empty() && journal.installed_profiles.is_empty(),
            "iOS cleanup journal mixes legacy global state with headless signing state"
        ),
    }
    Ok(())
}

fn ensure_legacy_workspace_has_no_residue(path: &Path) -> anyhow::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "legacy runner workspace {} is not a trusted directory; remove it before starting the runner",
        path.display()
    );
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_name() != SPOTLIGHT_NO_INDEX_SENTINEL {
            anyhow::bail!(
                "legacy runner workspace {} contains unreconciled build state; clean it before starting the runner",
                path.display()
            );
        }
    }
    Ok(())
}

#[cfg(unix)]
fn current_uid() -> anyhow::Result<u32> {
    // SAFETY: `geteuid` has no arguments, pointer requirements, or failure state.
    Ok(unsafe { libc::geteuid() })
}

#[cfg(unix)]
fn legacy_reconciliation_complete(root: &Path) -> anyhow::Result<bool> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

    let mut root_options = OpenOptions::new();
    root_options
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW);
    let root = root_options.open(root)?;
    let mut marker = match openat_no_follow(&root, LEGACY_RECONCILIATION_MARKER, false) {
        Ok(marker) => marker,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).context("failed to open legacy reconciliation marker"),
    };
    let metadata = marker.metadata()?;
    anyhow::ensure!(
        metadata.uid() == current_uid()?
            && metadata.is_file()
            && metadata.permissions().mode() & 0o077 == 0,
        "legacy reconciliation marker is not a private owned file"
    );
    let mut content = Vec::new();
    marker.read_to_end(&mut content)?;
    anyhow::ensure!(
        content == b"complete\n",
        "legacy reconciliation marker has invalid content"
    );
    Ok(true)
}

#[cfg(not(unix))]
fn legacy_reconciliation_complete(root: &Path) -> anyhow::Result<bool> {
    let marker = root.join(LEGACY_RECONCILIATION_MARKER);
    let metadata = match fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "legacy reconciliation marker is not a regular file"
    );
    anyhow::ensure!(
        fs::read(&marker)? == b"complete\n",
        "legacy reconciliation marker has invalid content"
    );
    Ok(true)
}

struct OpenIosCleanupJournal {
    journal: IosCleanupJournal,
    #[cfg(unix)]
    directory: fs::File,
    #[cfg(not(unix))]
    path: PathBuf,
}

impl OpenIosCleanupJournal {
    fn remove(self) -> anyhow::Result<()> {
        #[cfg(unix)]
        {
            use std::ffi::CString;
            use std::os::fd::AsRawFd;

            let name = CString::new("cleanup-journal.json").expect("static filename");
            // SAFETY: The directory descriptor is live, and `name` is a valid C string.
            let result = unsafe { libc::unlinkat(self.directory.as_raw_fd(), name.as_ptr(), 0) };
            if result == 0 {
                return Ok(());
            }
            let error = std::io::Error::last_os_error();
            if error.kind() == ErrorKind::NotFound {
                Ok(())
            } else {
                Err(error.into())
            }
        }
        #[cfg(not(unix))]
        {
            match fs::remove_file(self.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
            }
        }
    }
}

#[cfg(unix)]
fn openat_no_follow(parent: &fs::File, name: &str, directory: bool) -> std::io::Result<fs::File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};

    let name = CString::new(name).expect("static path component");
    let mut flags = libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW;
    if directory {
        flags |= libc::O_DIRECTORY;
    }
    // SAFETY: The parent descriptor is live, and `name` is a valid C string.
    let descriptor = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if descriptor < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: `openat` returned a new owned descriptor, which this `File` now owns.
        Ok(unsafe { fs::File::from_raw_fd(descriptor) })
    }
}

#[cfg(unix)]
fn open_ios_cleanup_journal(workspace: &Path) -> anyhow::Result<Option<OpenIosCleanupJournal>> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

    let mut root_options = OpenOptions::new();
    root_options
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW);
    let root = root_options.open(workspace).with_context(|| {
        format!(
            "failed to open runner workspace without following links: {}",
            workspace.display()
        )
    })?;
    let root_metadata = root.metadata()?;
    anyhow::ensure!(
        root_metadata.uid() == current_uid()?
            && root_metadata.is_dir()
            && root_metadata.permissions().mode() & 0o077 == 0,
        "runner workspace {} is not a private owned directory",
        workspace.display()
    );

    let oore_dir = match openat_no_follow(&root, ".oore", true) {
        Ok(directory) => directory,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("failed to open runner metadata directory"),
    };
    let signing_dir = match openat_no_follow(&oore_dir, "ios-signing", true) {
        Ok(directory) => directory,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("failed to open iOS signing journal directory"),
    };
    let mut journal_file = match openat_no_follow(&signing_dir, "cleanup-journal.json", false) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("failed to open iOS cleanup journal"),
    };
    let metadata = journal_file.metadata()?;
    anyhow::ensure!(
        metadata.uid() == current_uid()?
            && metadata.is_file()
            && metadata.permissions().mode() & 0o077 == 0,
        "iOS cleanup journal is not a private owned file"
    );
    let mut bytes = Vec::new();
    journal_file.read_to_end(&mut bytes)?;
    let journal = serde_json::from_slice(&bytes).context("failed to parse iOS cleanup journal")?;
    Ok(Some(OpenIosCleanupJournal {
        journal,
        directory: signing_dir,
    }))
}

#[cfg(not(unix))]
fn open_ios_cleanup_journal(workspace: &Path) -> anyhow::Result<Option<OpenIosCleanupJournal>> {
    let metadata_dir = workspace.join(".oore");
    let signing_dir = metadata_dir.join("ios-signing");
    for directory in [&metadata_dir, &signing_dir] {
        let metadata = match fs::symlink_metadata(directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        anyhow::ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "iOS signing journal path contains a link"
        );
    }
    let path = signing_dir.join("cleanup-journal.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "iOS cleanup journal is not a regular file"
    );
    let journal = serde_json::from_slice(&fs::read(&path)?)?;
    Ok(Some(OpenIosCleanupJournal { journal, path }))
}

fn reconcile_stale_workspaces_with(
    parent: &Path,
    mut reconcile_journal: impl FnMut(&Path, &IosCleanupJournal) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    #[cfg(unix)]
    let uid = current_uid()?;

    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !is_runner_workspace_name(file_name) {
            continue;
        }

        let workspace = entry.path();
        let metadata = fs::symlink_metadata(&workspace)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if metadata.uid() != uid {
                continue;
            }
            anyhow::ensure!(
                metadata.is_dir() && metadata.permissions().mode() & 0o077 == 0,
                "runner workspace {} is not a private directory",
                workspace.display()
            );
        }
        #[cfg(not(unix))]
        anyhow::ensure!(
            metadata.is_dir(),
            "runner workspace {} is not a directory",
            workspace.display()
        );

        if let Some(journal) = open_ios_cleanup_journal(&workspace)? {
            reconcile_journal(&workspace, &journal.journal)?;
            journal.remove()?;
        }
        fs::remove_dir_all(&workspace).with_context(|| {
            format!(
                "failed to remove stale runner workspace {}",
                workspace.display()
            )
        })?;
    }
    Ok(())
}

fn reconcile_stale_runner_mutations() -> anyhow::Result<()> {
    // ponytail: one process per runner account; add a process lease if replicas become supported.
    ensure_legacy_workspace_has_no_residue(Path::new(LEGACY_BUILD_WORKSPACE_ROOT))?;
    let runner_root = prepare_runner_workspace_root()?;
    reconcile_stale_workspaces_with(&runner_root, |workspace, journal| {
        validate_ios_cleanup_journal(workspace, journal)?;
        cleanup_ios_signing_state(journal)
    })?;
    if !legacy_reconciliation_complete(&runner_root)? {
        // One-time migration for generations created before the private runner root existed.
        reconcile_stale_workspaces_with(&std::env::temp_dir(), |workspace, journal| {
            validate_ios_cleanup_journal(workspace, journal)?;
            cleanup_ios_signing_state(journal)
        })?;
        write_private_file(
            &runner_root.join(LEGACY_RECONCILIATION_MARKER),
            b"complete\n",
        )?;
    }
    Ok(())
}

#[derive(Debug)]
struct RunnerControlPlaneRejected {
    operation: &'static str,
    status: reqwest::StatusCode,
}

impl std::fmt::Display for RunnerControlPlaneRejected {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "runner {operation} was rejected by the backend ({status})",
            operation = self.operation,
            status = self.status
        )
    }
}

impl std::error::Error for RunnerControlPlaneRejected {}

fn runner_response_is_terminal(status: reqwest::StatusCode) -> bool {
    status.is_client_error()
        && !matches!(
            status,
            reqwest::StatusCode::REQUEST_TIMEOUT | reqwest::StatusCode::TOO_MANY_REQUESTS
        )
}

fn terminal_runner_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<RunnerControlPlaneRejected>().is_some()
}

async fn send_runner_heartbeat(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    capabilities: &serde_json::Value,
) -> anyhow::Result<()> {
    let response = client
        .post(format!(
            "{}/v1/runners/{}/heartbeat",
            daemon_url, config.runner_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&serde_json::json!({ "status": "online", "capabilities": capabilities }))
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .context("runner heartbeat could not reach the backend")?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    if runner_response_is_terminal(status) {
        return Err(anyhow::Error::new(RunnerControlPlaneRejected {
            operation: "heartbeat",
            status,
        }));
    }
    anyhow::bail!("runner heartbeat was unavailable ({status})")
}

async fn establish_runner_heartbeat(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    capabilities: &serde_json::Value,
) -> anyhow::Result<()> {
    let delays = [
        Duration::ZERO,
        Duration::from_millis(500),
        Duration::from_secs(1),
    ];
    let mut last_error = None;
    for delay in delays {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        match send_runner_heartbeat(client, daemon_url, config, capabilities).await {
            Ok(()) => return Ok(()),
            Err(error) if terminal_runner_error(&error) => return Err(error),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("runner heartbeat failed"))
        .context("backend did not acknowledge runner startup"))
}

struct RunnerServiceAckGuard {
    path: Option<PathBuf>,
    pid: u32,
}

impl Drop for RunnerServiceAckGuard {
    fn drop(&mut self) {
        if let Some(path) = self.path.as_deref() {
            clear_runner_service_ack_if_owned(path, self.pid);
        }
    }
}

pub async fn run_runner_forever(
    config: RunnerConfig,
    daemon_url_override: Option<String>,
) -> anyhow::Result<()> {
    let daemon_url = daemon_url_override.unwrap_or(config.daemon_url.clone());
    require_safe_daemon_url(&daemon_url)?;
    let client = reqwest::Client::new();
    let mut release_watch = RunnerReleaseWatch::for_current_executable()?;
    let executable = std::env::current_exe().context("failed to locate the runner executable")?;
    let service_ack_path = runner_service_ack_path_from_env()?;
    // Capture process identity before doing any network work. An updater can
    // atomically replace the executable path while this process is still
    // finishing a build; later heartbeats must not let the old process claim
    // the replacement binary's identity or version.
    let service_ack_template = service_ack_path
        .as_ref()
        .map(|_| runner_service_ack_for(&config, &daemon_url, &executable, now_unix()))
        .transpose()?;
    if let Some(path) = service_ack_path.as_deref() {
        clear_runner_service_ack(path)?;
    }
    let _service_ack_guard = RunnerServiceAckGuard {
        path: service_ack_path.clone(),
        pid: std::process::id(),
    };

    match remove_missing_legacy_oore_keychains_from_search_list() {
        Ok(removed) if removed > 0 => {
            println!("Removed {removed} stale Oore build keychain entries");
        }
        Ok(_) => {}
        Err(error) => {
            eprintln!("Warning: could not remove stale Oore build keychain entries: {error:#}");
        }
    }

    reconcile_stale_runner_mutations()
        .context("failed to reconcile stale runner state before startup")?;

    println!("Starting runner '{}' ({})", config.name, config.runner_id);
    println!("Connecting to: {}", daemon_url);

    let capabilities = detect_capabilities().await;

    // Do not enter the claim loop or advertise service readiness until the
    // backend has authenticated this exact runner/config/release.
    establish_runner_heartbeat(&client, &daemon_url, &config, &capabilities).await?;
    if let (Some(path), Some(template)) =
        (service_ack_path.as_deref(), service_ack_template.as_ref())
    {
        write_runner_service_ack(path, &refreshed_runner_service_ack(template, now_unix()))?;
    }

    let hb_client = client.clone();
    let hb_url = daemon_url.clone();
    let hb_config = config.clone();
    let hb_capabilities = capabilities.clone();
    let hb_ack_path = service_ack_path;
    let hb_ack_template = service_ack_template;
    let (fatal_tx, mut fatal_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            match send_runner_heartbeat(&hb_client, &hb_url, &hb_config, &hb_capabilities).await {
                Ok(()) => {
                    if let (Some(path), Some(template)) =
                        (hb_ack_path.as_deref(), hb_ack_template.as_ref())
                    {
                        match write_runner_service_ack(
                            path,
                            &refreshed_runner_service_ack(template, now_unix()),
                        ) {
                            Ok(()) => {}
                            Err(error) => {
                                clear_runner_service_ack_if_owned(path, std::process::id());
                                let _ = fatal_tx.send(format!(
                                    "failed to publish authenticated runner readiness: {error:#}"
                                ));
                                return;
                            }
                        }
                    }
                }
                Err(error) => {
                    if let Some(path) = hb_ack_path.as_deref() {
                        clear_runner_service_ack_if_owned(path, std::process::id());
                    }
                    if terminal_runner_error(&error) {
                        let _ = fatal_tx.send(error.to_string());
                        return;
                    }
                    eprintln!("Runner heartbeat is temporarily unavailable: {error:#}");
                }
            }
        }
    });

    loop {
        tokio::select! {
            fatal = fatal_rx.recv() => {
                match fatal {
                    Some(error) => anyhow::bail!(error),
                    None => anyhow::bail!("runner heartbeat monitor stopped unexpectedly"),
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
        }
        if runner_should_retire(&mut release_watch) {
            println!(
                "A committed runner update is ready; no build is active, exiting for a clean restart"
            );
            return Ok(());
        }
        if let Err(error) = reconcile_stale_runner_mutations() {
            eprintln!("Refusing to claim work until stale runner state is cleaned: {error:#}");
            tokio::time::sleep(Duration::from_secs(10)).await;
            continue;
        }
        match claim_and_execute(&client, &daemon_url, &config).await {
            Ok(_executed) => {}
            Err(error) if terminal_runner_error(&error) => return Err(error),
            Err(e) => {
                eprintln!("Error during claim/execute: {}", e);
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
    }
}

async fn claim_and_execute(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
) -> anyhow::Result<bool> {
    let resp = client
        .post(format!(
            "{}/v1/runners/{}/claim",
            daemon_url, config.runner_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&ClaimJobRequest {
            protocol_version: RUNNER_PROTOCOL_VERSION,
        })
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        if runner_response_is_terminal(status) {
            return Err(anyhow::Error::new(RunnerControlPlaneRejected {
                operation: "claim",
                status,
            }));
        }
        anyhow::bail!("Claim request failed: {status}");
    }

    let claim: ClaimJobResponse = resp.json().await?;
    let job = match claim.job {
        Some(j) => j,
        None => return Ok(false),
    };

    println!(
        "Claimed build {} (#{}) for project {}",
        job.build_id, job.build_number, job.project_id
    );

    report_status(
        client,
        daemon_url,
        config,
        StatusReport {
            build_id: job.build_id.as_str(),
            status: "running",
            exit_code: None,
            error_message: None,
            steps: &[],
        },
    )
    .await?;

    let (steps, result) = execute_build(&job, client, daemon_url, config).await;

    match result {
        Ok(()) => {
            report_status(
                client,
                daemon_url,
                config,
                StatusReport {
                    build_id: job.build_id.as_str(),
                    status: "succeeded",
                    exit_code: Some(0),
                    error_message: None,
                    steps: &steps,
                },
            )
            .await?;
            println!("Build {} succeeded", job.build_id);
        }
        Err(e) => {
            if e.downcast_ref::<BuildTerminated>().is_some() {
                println!(
                    "Build {} was externally terminated, skipping status report",
                    job.build_id
                );
            } else {
                report_status(
                    client,
                    daemon_url,
                    config,
                    StatusReport {
                        build_id: job.build_id.as_str(),
                        status: "failed",
                        exit_code: Some(1),
                        error_message: Some(e.to_string()),
                        steps: &steps,
                    },
                )
                .await?;
                eprintln!("Build {} failed: {}", job.build_id, e);
            }
        }
    }

    Ok(true)
}

struct WorkspaceCleanup {
    path: PathBuf,
}

impl Drop for WorkspaceCleanup {
    fn drop(&mut self) {
        if self.path.join(IOS_CLEANUP_JOURNAL).exists() {
            eprintln!(
                "Warning: retaining workspace {} for iOS signing reconciliation",
                self.path.display()
            );
            return;
        }
        if self.path.exists()
            && let Err(e) = fs::remove_dir_all(&self.path)
        {
            eprintln!(
                "Warning: failed to clean up workspace {}: {}",
                self.path.display(),
                e
            );
        }
    }
}

#[derive(Debug)]
struct BuildTerminated {
    status: String,
}

impl std::fmt::Display for BuildTerminated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "build was externally terminated (status: {})",
            self.status
        )
    }
}

impl std::error::Error for BuildTerminated {}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FvmRcConfig {
    flutter: String,
}

const DEFAULT_MANAGED_FLUTTER_VERSION: &str = "stable";

#[derive(Debug, Clone)]
struct ResolvedExecutionPlan {
    stage_commands: PipelineCommandStages,
    artifact_patterns: Vec<String>,
    env: Vec<PipelineEnvVar>,
    source: String,
}

fn validate_command_list(path: &str, commands: &[String]) -> anyhow::Result<Vec<String>> {
    let mut cleaned = Vec::with_capacity(commands.len());
    for (idx, command) in commands.iter().enumerate() {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            anyhow::bail!("{path}[{idx}] must not be empty");
        }
        cleaned.push(trimmed.to_string());
    }
    Ok(cleaned)
}

fn normalize_flutter_version(
    value: Option<&str>,
    field_path: &str,
) -> anyhow::Result<Option<String>> {
    match value {
        None => Ok(None),
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                anyhow::bail!("{field_path} must not be empty");
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

fn read_fvmrc_version(workspace: &Path) -> anyhow::Result<Option<String>> {
    let fvmrc_path = workspace.join(".fvmrc");
    if !fvmrc_path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&fvmrc_path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", fvmrc_path.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("{} is empty", fvmrc_path.display());
    }

    if trimmed.starts_with('{') {
        let parsed: FvmRcConfig = serde_json::from_str(trimmed).map_err(|e| {
            anyhow::anyhow!(
                "failed to parse {} as JSON object with 'flutter': {e}",
                fvmrc_path.display()
            )
        })?;
        return normalize_flutter_version(Some(parsed.flutter.as_str()), ".fvmrc.flutter");
    }

    if trimmed.starts_with('"') {
        let parsed: String = serde_json::from_str(trimmed).map_err(|e| {
            anyhow::anyhow!(
                "failed to parse {} as JSON string: {e}",
                fvmrc_path.display()
            )
        })?;
        return normalize_flutter_version(Some(parsed.as_str()), ".fvmrc");
    }

    normalize_flutter_version(Some(trimmed), ".fvmrc")
}

fn maybe_wrap_with_fvm(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed == "flutter" {
        return "fvm flutter".to_string();
    }
    if trimmed == "dart" {
        return "fvm dart".to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("flutter ") {
        return format!("fvm flutter {rest}");
    }
    if let Some(rest) = trimmed.strip_prefix("dart ") {
        return format!("fvm dart {rest}");
    }
    trimmed.to_string()
}

fn apply_fvm_wrappers(stage_commands: PipelineCommandStages) -> PipelineCommandStages {
    PipelineCommandStages {
        pre_build: stage_commands
            .pre_build
            .into_iter()
            .map(|cmd| maybe_wrap_with_fvm(&cmd))
            .collect(),
        build: stage_commands
            .build
            .into_iter()
            .map(|cmd| maybe_wrap_with_fvm(&cmd))
            .collect(),
        post_build: stage_commands
            .post_build
            .into_iter()
            .map(|cmd| maybe_wrap_with_fvm(&cmd))
            .collect(),
    }
}

fn command_uses_flutter_toolchain(command: &str) -> bool {
    let trimmed = command.trim();
    maybe_wrap_with_fvm(trimmed) != trimmed
        || trimmed == "fvm flutter"
        || trimmed == "fvm dart"
        || trimmed.starts_with("fvm flutter ")
        || trimmed.starts_with("fvm dart ")
}

fn apply_managed_flutter_toolchain(
    stage_commands: PipelineCommandStages,
    version: &str,
) -> PipelineCommandStages {
    let requires_flutter = stage_commands
        .pre_build
        .iter()
        .chain(&stage_commands.build)
        .chain(&stage_commands.post_build)
        .any(|command| command_uses_flutter_toolchain(command));
    if !requires_flutter {
        return stage_commands;
    }

    let mut stage_commands = apply_fvm_wrappers(stage_commands);
    stage_commands
        .pre_build
        .insert(0, format!("fvm use {version} --force --skip-pub-get"));
    stage_commands
}

fn validate_artifact_patterns(patterns: &[String]) -> anyhow::Result<Vec<String>> {
    let mut cleaned = Vec::with_capacity(patterns.len());
    for (idx, pattern) in patterns.iter().enumerate() {
        let trimmed = pattern.trim();
        validate_artifact_pattern(trimmed)
            .map_err(|error| anyhow::anyhow!("artifacts.patterns[{idx}] {error}"))?;
        cleaned.push(trimmed.to_string());
    }
    Ok(cleaned)
}

fn validate_platform_command(
    command: &Option<String>,
    path: &str,
) -> anyhow::Result<Option<String>> {
    match command {
        None => Ok(None),
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                anyhow::bail!("{path} must not be empty when provided");
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

fn validate_env_vars(env: &[PipelineEnvVar]) -> anyhow::Result<Vec<PipelineEnvVar>> {
    let mut validated = Vec::with_capacity(env.len());
    let mut seen = std::collections::HashSet::new();
    for (idx, pair) in env.iter().enumerate() {
        let key = pair.key.trim();
        let value = pair.value.trim();
        if key.is_empty() {
            anyhow::bail!("env[{idx}].key must not be empty");
        }
        if !is_valid_pipeline_env_key(key) {
            anyhow::bail!("env[{idx}].key must match [A-Za-z_][A-Za-z0-9_]*");
        }
        if !seen.insert(key.to_string()) {
            anyhow::bail!("env contains duplicate key '{key}'");
        }
        validated.push(PipelineEnvVar {
            key: key.to_string(),
            value: value.to_string(),
        });
    }
    Ok(validated)
}

fn normalize_execution_config(
    config: PipelineExecutionConfig,
) -> anyhow::Result<PipelineExecutionConfig> {
    if config.platforms.is_empty() {
        anyhow::bail!("platforms must include at least one target");
    }

    let commands = PipelineCommandStages {
        pre_build: validate_command_list("commands.pre_build", &config.commands.pre_build)?,
        build: validate_command_list("commands.build", &config.commands.build)?,
        post_build: validate_command_list("commands.post_build", &config.commands.post_build)?,
    };
    let platform_build_args = PlatformBuildArgs {
        android: validate_command_list(
            "platform_build_args.android",
            &config.platform_build_args.android,
        )?,
        ios: validate_command_list("platform_build_args.ios", &config.platform_build_args.ios)?,
        macos: validate_command_list(
            "platform_build_args.macos",
            &config.platform_build_args.macos,
        )?,
    };
    let platform_commands = PlatformBuildCommands {
        android: validate_platform_command(
            &config.platform_commands.android,
            "platform_commands.android",
        )?,
        ios: validate_platform_command(&config.platform_commands.ios, "platform_commands.ios")?,
        macos: validate_platform_command(
            &config.platform_commands.macos,
            "platform_commands.macos",
        )?,
    };
    let env = validate_env_vars(&config.env)?;
    let artifact_patterns = validate_artifact_patterns(&config.artifact_patterns)?;
    let flutter_version =
        normalize_flutter_version(config.flutter_version.as_deref(), "flutter_version")?;

    Ok(PipelineExecutionConfig {
        platforms: config.platforms,
        flutter_version,
        commands,
        platform_build_args,
        platform_commands,
        env,
        artifact_patterns,
    })
}

fn build_default_command_with_args(base: &str, args: &[String]) -> String {
    if args.is_empty() {
        return base.to_string();
    }
    format!("{base} {}", args.join(" "))
}

fn default_platform_command(
    platform: &BuildPlatform,
    overrides: &PlatformBuildCommands,
    args: &PlatformBuildArgs,
) -> String {
    match platform {
        BuildPlatform::Android => overrides.android.clone().unwrap_or_else(|| {
            build_default_command_with_args("flutter build apk --release", &args.android)
        }),
        BuildPlatform::Ios => overrides.ios.clone().unwrap_or_else(|| {
            build_default_command_with_args("flutter build ios --release --no-codesign", &args.ios)
        }),
        BuildPlatform::Macos => overrides.macos.clone().unwrap_or_else(|| {
            build_default_command_with_args("flutter build macos --release", &args.macos)
        }),
    }
}

fn materialize_stage_commands(
    config: &PipelineExecutionConfig,
    include_default_platform_commands: bool,
) -> PipelineCommandStages {
    let mut pre_build = Vec::new();
    if include_default_platform_commands && !config.platforms.is_empty() {
        pre_build.push("flutter pub get".to_string());
    }
    pre_build.extend(config.commands.pre_build.clone());

    let mut build = if include_default_platform_commands {
        config
            .platforms
            .iter()
            .map(|platform| {
                default_platform_command(
                    platform,
                    &config.platform_commands,
                    &config.platform_build_args,
                )
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    build.extend(config.commands.build.clone());

    PipelineCommandStages {
        pre_build,
        build,
        post_build: config.commands.post_build.clone(),
    }
}

fn load_ui_execution_config(
    snapshot: &serde_json::Value,
) -> anyhow::Result<PipelineExecutionConfig> {
    let Some(raw) = snapshot.get("ui_execution_config") else {
        return Ok(PipelineExecutionConfig::default());
    };
    let parsed: PipelineExecutionConfig = serde_json::from_value(raw.clone())
        .map_err(|e| anyhow::anyhow!("Invalid ui_execution_config in snapshot: {e}"))?;
    normalize_execution_config(parsed)
}

fn apply_run_platform_selection(
    mut config: PipelineExecutionConfig,
    snapshot: &serde_json::Value,
) -> anyhow::Result<PipelineExecutionConfig> {
    let Some(raw) = snapshot.get("selected_platforms") else {
        return Ok(config);
    };
    if raw.is_null() {
        return Ok(config);
    }

    let selected: Vec<BuildPlatform> = serde_json::from_value(raw.clone())
        .map_err(|error| anyhow::anyhow!("Invalid selected_platforms in snapshot: {error}"))?;
    if selected.is_empty() {
        anyhow::bail!("selected_platforms must include at least one target");
    }
    let has_duplicate = selected
        .iter()
        .enumerate()
        .any(|(index, platform)| selected[..index].contains(platform));
    if has_duplicate
        || selected
            .iter()
            .any(|platform| !config.platforms.contains(platform))
    {
        anyhow::bail!("selected_platforms must be unique and configured by the workflow");
    }
    if selected.len() < config.platforms.len() && !config.commands.build.is_empty() {
        anyhow::bail!(
            "Per-run platform selection requires platform_commands or default platform commands; shared commands.build cannot be filtered safely"
        );
    }

    config.platforms = selected;
    Ok(config)
}

fn resolve_execution_plan(
    workspace: &Path,
    snapshot: &serde_json::Value,
) -> anyhow::Result<ResolvedExecutionPlan> {
    let config_path_explicit = snapshot
        .get("config_path_explicit")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let snapshot_config_path = snapshot
        .get("config_path")
        .and_then(|v| v.as_str())
        .unwrap_or(".oore.yaml")
        .trim()
        .to_string();

    let candidate_paths: Vec<String> = if config_path_explicit {
        validate_repository_config_path(&snapshot_config_path)
            .map_err(|error| anyhow::anyhow!("Invalid explicit repository config path: {error}"))?;
        vec![snapshot_config_path]
    } else {
        AUTO_CONFIG_PATHS.iter().map(|p| p.to_string()).collect()
    };

    let fvmrc_version = read_fvmrc_version(workspace)?;
    let canonical_workspace = workspace
        .canonicalize()
        .map_err(|error| anyhow::anyhow!("Failed to resolve build workspace: {error}"))?;

    for rel_path in &candidate_paths {
        let full_path = workspace.join(rel_path);
        if !full_path.exists() {
            continue;
        }

        let canonical_path = full_path.canonicalize().map_err(|error| {
            anyhow::anyhow!(
                "Failed to resolve repository config file {}: {error}",
                full_path.display()
            )
        })?;
        if !canonical_path.starts_with(&canonical_workspace) {
            anyhow::bail!(
                "Repository config path resolves outside the build workspace: {}",
                full_path.display()
            );
        }

        let content = fs::read_to_string(&full_path).map_err(|e| {
            anyhow::anyhow!("Failed to read config file {}: {e}", full_path.display())
        })?;
        let file_config = parse_repository_pipeline_yaml(&content).map_err(|e| {
            anyhow::anyhow!("Invalid pipeline config in {}: {}", full_path.display(), e)
        })?;
        let file_config = apply_run_platform_selection(file_config, snapshot)?;
        let resolved_flutter_version = fvmrc_version
            .or_else(|| file_config.flutter_version.clone())
            .unwrap_or_else(|| DEFAULT_MANAGED_FLUTTER_VERSION.to_string());
        let include_defaults = file_config.commands.build.is_empty();
        let stage_commands = apply_managed_flutter_toolchain(
            materialize_stage_commands(&file_config, include_defaults),
            &resolved_flutter_version,
        );

        return Ok(ResolvedExecutionPlan {
            stage_commands,
            artifact_patterns: file_config.artifact_patterns,
            env: file_config.env,
            source: format!("file:{}", full_path.display()),
        });
    }

    let fallback = apply_run_platform_selection(load_ui_execution_config(snapshot)?, snapshot)?;
    let resolved_flutter_version = fvmrc_version
        .or_else(|| fallback.flutter_version.clone())
        .unwrap_or_else(|| DEFAULT_MANAGED_FLUTTER_VERSION.to_string());
    let stage_commands = apply_managed_flutter_toolchain(
        materialize_stage_commands(&fallback, true),
        &resolved_flutter_version,
    );
    Ok(ResolvedExecutionPlan {
        stage_commands,
        artifact_patterns: fallback.artifact_patterns,
        env: fallback.env,
        source: "ui_fallback".to_string(),
    })
}

#[derive(Default)]
struct BuildAuthorityState {
    consecutive_failures: AtomicU8,
}

impl BuildAuthorityState {
    fn confirmed_active(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    fn transient_failure(&self) -> anyhow::Result<()> {
        // `fetch_update` is deprecated in favor of `try_update` from Rust 1.99; keep the old
        // name until local toolchains catch up with CI's stable.
        #[allow(deprecated)]
        let failures = self
            .consecutive_failures
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_add(1))
            })
            .unwrap_or(u8::MAX)
            .saturating_add(1);
        if failures >= MAX_CONSECUTIVE_AUTHORITY_FAILURES {
            return Err(BuildTerminated {
                status: "controller_unavailable".to_string(),
            }
            .into());
        }
        Ok(())
    }
}

fn authority_loss(status: reqwest::StatusCode) -> anyhow::Error {
    let status = match status {
        reqwest::StatusCode::UNAUTHORIZED => "runner_unauthorized".to_string(),
        reqwest::StatusCode::FORBIDDEN => "assignment_lost".to_string(),
        reqwest::StatusCode::NOT_FOUND => "build_missing".to_string(),
        status => format!("protocol_rejected_{status}"),
    };
    BuildTerminated { status }.into()
}

fn is_transient_authority_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error()
        || matches!(
            status,
            reqwest::StatusCode::REQUEST_TIMEOUT | reqwest::StatusCode::TOO_MANY_REQUESTS
        )
}

async fn check_build_active(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    authority: &BuildAuthorityState,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!(
            "{}/v1/runners/{}/jobs/{}",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&ClaimJobRequest {
            protocol_version: RUNNER_PROTOCOL_VERSION,
        })
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let status_resp: JobStatusResponse = r.json().await?;
            let status: BuildStatus = status_resp
                .status
                .parse()
                .map_err(|e: String| anyhow::anyhow!(e))?;

            if status.is_terminal() {
                return Err(BuildTerminated {
                    status: status_resp.status,
                }
                .into());
            }
            authority.confirmed_active();
            Ok(())
        }
        Ok(response) if is_transient_authority_status(response.status()) => {
            authority.transient_failure()
        }
        Ok(response) => Err(authority_loss(response.status())),
        Err(_) => authority.transient_failure(),
    }
}

async fn poll_cancellation(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    authority: Arc<BuildAuthorityState>,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        if check_build_active(client, daemon_url, config, build_id, &authority)
            .await
            .is_err()
        {
            return;
        }
    }
}

#[derive(Debug, Clone)]
struct CheckoutInvocation {
    preview_command: String,
    shell_script: String,
    env: Vec<(String, String)>,
}

fn build_checkout_invocation(
    repo_url: &str,
    commit_sha: Option<&str>,
    branch: Option<&str>,
) -> anyhow::Result<CheckoutInvocation> {
    if let Some(sha) = commit_sha {
        return Ok(CheckoutInvocation {
            preview_command: format!(
                "git fetch --depth 1 <repo> {sha} && git checkout FETCH_HEAD && \
                 git submodule sync --recursive && git submodule update --init --recursive"
            ),
            shell_script: r#"set -eu
git init
git fetch --depth 1 "$OORE_REPO" "$OORE_SHA"
git checkout FETCH_HEAD
echo "[oore-checkout] syncing submodules (recursive)"
if ! git submodule sync --recursive; then
  echo "[oore-checkout] submodule sync failed" >&2
  exit 91
fi
echo "[oore-checkout] updating submodules (init + recursive)"
if ! git submodule update --init --recursive; then
  echo "[oore-checkout] submodule update failed" >&2
  exit 92
fi
"#
            .to_string(),
            env: vec![
                ("OORE_REPO".to_string(), repo_url.to_string()),
                ("OORE_SHA".to_string(), sha.to_string()),
            ],
        });
    }

    if let Some(branch) = branch {
        return Ok(CheckoutInvocation {
            preview_command: format!(
                "git clone --depth 1 --branch {branch} <repo> . && \
                 git submodule sync --recursive && git submodule update --init --recursive"
            ),
            shell_script: r#"set -eu
git clone --depth 1 --branch "$OORE_BRANCH" "$OORE_REPO" .
echo "[oore-checkout] syncing submodules (recursive)"
if ! git submodule sync --recursive; then
  echo "[oore-checkout] submodule sync failed" >&2
  exit 91
fi
echo "[oore-checkout] updating submodules (init + recursive)"
if ! git submodule update --init --recursive; then
  echo "[oore-checkout] submodule update failed" >&2
  exit 92
fi
"#
            .to_string(),
            env: vec![
                ("OORE_REPO".to_string(), repo_url.to_string()),
                ("OORE_BRANCH".to_string(), branch.to_string()),
            ],
        });
    }

    anyhow::bail!("Build has neither commit_sha nor branch — cannot checkout source")
}

fn add_checkout_proxy_config(
    checkout: &mut CheckoutInvocation,
    proxy_url: &str,
    runner_token: &str,
) {
    checkout.env.extend([
        ("GIT_CONFIG_COUNT".to_string(), "1".to_string()),
        (
            "GIT_CONFIG_KEY_0".to_string(),
            format!("http.{proxy_url}.extraHeader"),
        ),
        (
            "GIT_CONFIG_VALUE_0".to_string(),
            format!("Authorization: Bearer {runner_token}"),
        ),
    ]);
}

fn snapshot_requests_ios(snapshot: &serde_json::Value) -> bool {
    let platforms = snapshot
        .get("selected_platforms")
        .filter(|value| !value.is_null())
        .or_else(|| {
            snapshot
                .get("ui_execution_config")
                .and_then(|config| config.get("platforms"))
        });
    platforms
        .and_then(serde_json::Value::as_array)
        .is_some_and(|platforms| platforms.iter().any(|platform| platform == "ios"))
}

async fn execute_build(
    job: &ClaimedJob,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
) -> (Vec<StepResult>, anyhow::Result<()>) {
    // Pin signer executables before any repository-controlled checkout or build stage runs.
    let android_signing_toolchain = AndroidSigningToolchain::discover();
    let runner_workspace_root = match prepare_runner_workspace_root() {
        Ok(root) => root,
        Err(error) => return (vec![], Err(error)),
    };
    let workspace = match create_private_workspace_in(&runner_workspace_root, &config.runner_id) {
        Ok(workspace) => workspace,
        Err(error) => return (vec![], Err(error.into())),
    };
    try_mark_no_spotlight_index(&workspace);

    let _cleanup = WorkspaceCleanup {
        path: workspace.clone(),
    };
    let signing_workspace =
        match create_private_workspace_in(&runner_workspace_root, &config.runner_id) {
            Ok(workspace) => workspace,
            Err(error) => return (vec![], Err(error.into())),
        };
    try_mark_no_spotlight_index(&signing_workspace);
    let _signing_cleanup = WorkspaceCleanup {
        path: signing_workspace.clone(),
    };
    let authority = Arc::new(BuildAuthorityState::default());

    let snapshot = &job.config_snapshot;
    let mut steps = Vec::new();
    let mut log_seq: i64 = 0;
    let mut ios_signing_source: Option<&str> = None;
    let mut ios_signing_bundle: Option<RunnerIosSigningBundle> = None;
    let ios_signing_checked_before_checkout = snapshot_requests_ios(snapshot);

    if let Err(e) = check_build_active(client, daemon_url, config, &job.build_id, &authority).await
    {
        return (steps, Err(e));
    }

    if ios_signing_checked_before_checkout {
        match fetch_job_ios_signing(
            client,
            daemon_url,
            config,
            &job.build_id,
            &job.signing_token,
        )
        .await
        {
            Ok(Some(server_payload)) => {
                if let Some(mut bundle) = server_payload.bundle {
                    if let Err(error) = require_ios_signing_user_session() {
                        zeroize_ios_signing_bundle(&mut bundle);
                        return (steps, Err(error));
                    }
                    let signing_source = match bundle.mode {
                        oore_contract::IosSigningMode::Manual => "manual",
                        oore_contract::IosSigningMode::Api => "api",
                        oore_contract::IosSigningMode::Hybrid => "hybrid",
                    };
                    ios_signing_bundle = Some(bundle);
                    ios_signing_source = Some(signing_source);
                    println!("Reserved iOS signing bundle before repository checkout");
                }
            }
            Ok(None) => {}
            Err(error) => {
                return (
                    steps,
                    Err(anyhow::anyhow!(
                        "Failed to load iOS signing bundle before checkout. Aborting to avoid unsigned iOS artifacts: {error}"
                    )),
                );
            }
        }
    }

    let repo_url = snapshot
        .get("repo_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if repo_url.is_empty() {
        return (
            steps,
            Err(anyhow::anyhow!(
                "Build config snapshot has no repo_url — cannot checkout source"
            )),
        );
    }

    let proxy_path = snapshot.get("checkout_proxy_path").and_then(|v| v.as_str());
    let effective_repo_url = proxy_path
        .map(|path| format!("{}{}", daemon_url.trim_end_matches('/'), path))
        .unwrap_or_else(|| repo_url.to_string());

    let start = now_unix();
    let mut checkout = match build_checkout_invocation(
        &effective_repo_url,
        job.commit_sha.as_deref(),
        job.branch.as_deref(),
    ) {
        Ok(checkout) => checkout,
        Err(e) => return (steps, Err(e)),
    };
    if proxy_path.is_some() {
        add_checkout_proxy_config(&mut checkout, &effective_repo_url, &config.runner_token);
    }

    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        &mut log_seq,
        "stdout",
        &step_start_marker("checkout", &checkout.preview_command),
    )
    .await;
    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        &mut log_seq,
        "stdout",
        &format!("$ {}", checkout.preview_command),
    )
    .await;

    let mut checkout_child = repository_shell_command(&checkout.shell_script, &workspace);
    checkout_child
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    for (key, value) in &checkout.env {
        checkout_child.env(key, value);
    }
    scrub_managed_runner_env(&mut checkout_child);

    let child = match checkout_child.spawn() {
        Ok(c) => c,
        Err(e) => return (steps, Err(e.into())),
    };

    let clone_status = run_and_stream(
        child,
        client,
        daemon_url,
        config,
        &job.build_id,
        &mut log_seq,
        poll_cancellation(client, daemon_url, config, &job.build_id, authority.clone()),
    )
    .await;

    let finished = now_unix();
    match clone_status {
        None => {
            steps.push(StepResult {
                name: "checkout".to_string(),
                status: "failed".to_string(),
                exit_code: None,
                started_at: start,
                finished_at: finished,
                duration_ms: (finished - start) * 1000,
            });
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                "stdout",
                &step_end_marker("checkout", "canceled", None),
            )
            .await;
            return (
                steps,
                Err(BuildTerminated {
                    status: "canceled".to_string(),
                }
                .into()),
            );
        }
        Some(status) => {
            let exit_code = status.code();
            let success = exit_code == Some(0);
            steps.push(StepResult {
                name: "checkout".to_string(),
                status: if success { "succeeded" } else { "failed" }.to_string(),
                exit_code,
                started_at: start,
                finished_at: finished,
                duration_ms: (finished - start) * 1000,
            });
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                "stdout",
                &step_end_marker(
                    "checkout",
                    if success { "succeeded" } else { "failed" },
                    exit_code,
                ),
            )
            .await;
            if !success {
                return (steps, Err(anyhow::anyhow!("Git checkout failed")));
            }
        }
    }

    let execution_plan = match resolve_execution_plan(&workspace, snapshot) {
        Ok(plan) => plan,
        Err(e) => return (steps, Err(e)),
    };
    let requires_managed_fvm = execution_plan
        .stage_commands
        .pre_build
        .iter()
        .chain(&execution_plan.stage_commands.build)
        .chain(&execution_plan.stage_commands.post_build)
        .any(|command| command_uses_flutter_toolchain(command));
    if requires_managed_fvm && bundled_fvm_install_root().is_none() {
        let _ = append_runner_log_line(
            client,
            daemon_url,
            config,
            &job.build_id,
            &mut log_seq,
            "stdout",
            "Preparing Oore-managed Flutter tooling for this runner...",
        )
        .await;
        match ensure_managed_fvm(client).await {
            Ok(true) => {
                let _ = append_runner_log_line(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    &mut log_seq,
                    "stdout",
                    "Oore-managed Flutter tooling is ready.",
                )
                .await;
            }
            Ok(false) => {}
            Err(error) => {
                return (
                    steps,
                    Err(error.context(
                        "Failed to prepare Oore-managed Flutter tooling; retry the build",
                    )),
                );
            }
        }
    }

    let mut signing_source: Option<&str> = None;
    let mut signing_variant: Option<AndroidSigningBuildType> = None;
    let mut signing_inputs: Option<AndroidSigningInputs> = None;
    let build_commands = execution_plan.stage_commands.build.as_slice();
    match determine_android_signing_variant(build_commands) {
        Ok(Some(variant)) => {
            signing_variant = Some(variant);
            match fetch_job_android_signing(
                client,
                daemon_url,
                config,
                &job.build_id,
                &job.signing_token,
            )
            .await
            {
                Ok(Some(server_profiles)) => {
                    if let Some(profile) = select_runner_signing_profile(&server_profiles, variant)
                    {
                        match signing_inputs_from_runner_profile(profile) {
                            Ok(inputs) => {
                                signing_inputs = Some(inputs);
                                signing_source = Some("pipeline_profile");
                                println!(
                                    "Reserved Android signing profile for runner-owned post-build signing ({variant:?})"
                                );
                            }
                            Err(e) => return (steps, Err(e)),
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    return (
                        steps,
                        Err(anyhow::anyhow!(
                            "Failed to load Android signing profile for this build: {e}"
                        )),
                    );
                }
            }
        }
        Ok(None) => {}
        Err(e) => return (steps, Err(e)),
    }

    if !ios_signing_checked_before_checkout
        && build_commands
            .iter()
            .any(|command| is_ios_flutter_build_command(command))
    {
        match fetch_job_ios_signing(
            client,
            daemon_url,
            config,
            &job.build_id,
            &job.signing_token,
        )
        .await
        {
            Ok(Some(server_payload)) => {
                if let Some(mut bundle) = server_payload.bundle {
                    if let Err(error) = require_ios_signing_user_session() {
                        zeroize_ios_signing_bundle(&mut bundle);
                        return (steps, Err(error));
                    }
                    let signing_source = match bundle.mode {
                        oore_contract::IosSigningMode::Manual => "manual",
                        oore_contract::IosSigningMode::Api => "api",
                        oore_contract::IosSigningMode::Hybrid => "hybrid",
                    };
                    ios_signing_bundle = Some(bundle);
                    ios_signing_source = Some(signing_source);
                    println!("Reserved iOS signing bundle for runner-owned post-build signing");
                }
            }
            Ok(None) => {}
            Err(e) => {
                return (
                    steps,
                    Err(anyhow::anyhow!(
                        "Failed to load iOS signing bundle for this build. Aborting to avoid unsigned iOS artifacts: {e}"
                    )),
                );
            }
        }
    }

    println!("Using pipeline config source: {}", execution_plan.source);
    let mut step_env: Vec<(String, String)> = vec![
        ("PROJECT_ID".to_string(), job.project_id.clone()),
        ("PIPELINE_ID".to_string(), job.pipeline_id.clone()),
        ("BUILD_ID".to_string(), job.build_id.clone()),
        (
            "PROJECT_BUILD_NUMBER".to_string(),
            job.build_number.to_string(),
        ),
        ("BUILD_NUMBER".to_string(), job.build_number.to_string()),
    ];
    if let Some(branch) = &job.branch {
        step_env.push(("BRANCH".to_string(), branch.clone()));
    }
    if let Some(commit_sha) = &job.commit_sha {
        step_env.push(("COMMIT_SHA".to_string(), commit_sha.clone()));
    }
    for pair in &execution_plan.env {
        step_env.push((pair.key.clone(), pair.value.clone()));
    }

    if execution_plan.env.iter().all(|pair| pair.key != "CI") {
        step_env.push(("CI".to_string(), "true".to_string()));
    }

    if let (Some(source), Some(variant), Some(_)) =
        (signing_source, signing_variant, &signing_inputs)
    {
        let _ = append_runner_log_line(
            client,
            daemon_url,
            config,
            &job.build_id,
            &mut log_seq,
            "stdout",
            &android_signing_prepared_marker(source, variant),
        )
        .await;
    }

    // Flutter expects dart-defines to be materialized in a file. When pipeline
    // env vars are provided, create a temporary .env and pass it to flutter build.
    let dart_define_file = if execution_plan.env.is_empty() {
        None
    } else {
        let mut content = String::new();
        for pair in &execution_plan.env {
            content.push_str(&pair.key);
            content.push('=');
            content.push_str(&pair.value);
            content.push('\n');
        }
        let define_file_path = workspace.join(".env");
        if let Err(e) = write_private_file(&define_file_path, content.as_bytes()) {
            return (
                steps,
                Err(anyhow::anyhow!(
                    "Failed to write dart define file {}: {e}",
                    define_file_path.display()
                )),
            );
        }
        Some(".env".to_string())
    };

    let mut ios_signing_command_applied = false;
    let ios_signing_expected = ios_signing_bundle.is_some();
    let mut ios_artifact_metadata: Option<serde_json::Value> = None;
    for (stage_name, commands) in [
        (
            "pre_build",
            execution_plan.stage_commands.pre_build.as_slice(),
        ),
        ("build", execution_plan.stage_commands.build.as_slice()),
        (
            "post_build",
            execution_plan.stage_commands.post_build.as_slice(),
        ),
    ] {
        for (index, command) in commands.iter().enumerate() {
            if let Err(e) =
                check_build_active(client, daemon_url, config, &job.build_id, &authority).await
            {
                return (steps, Err(e));
            }

            let step_name = format!("{stage_name}-{}", index + 1);
            let start = now_unix();
            let (normalized_command, command_applied) = match normalize_stage_command_for_execution(
                stage_name,
                command,
                dart_define_file.as_deref(),
                ios_signing_bundle.as_ref().map(|_| Path::new("")),
            ) {
                Ok(value) => value,
                Err(e) => return (steps, Err(e)),
            };
            ios_signing_command_applied |= command_applied;
            let command_preview = render_command_preview(&normalized_command, &step_env);

            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                "stdout",
                &step_start_marker(&step_name, &normalized_command),
            )
            .await;
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                "stdout",
                &format!("$ {command_preview}"),
            )
            .await;
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                "stdout",
                &render_step_env_preview(&step_env),
            )
            .await;

            let mut step_cmd = repository_shell_command(&normalized_command, &workspace);
            step_cmd
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            for (key, value) in &step_env {
                step_cmd.env(key, value);
            }
            scrub_managed_runner_env(&mut step_cmd);
            let child = match step_cmd.spawn() {
                Ok(c) => c,
                Err(e) => return (steps, Err(e.into())),
            };

            let step_status = run_and_stream(
                child,
                client,
                daemon_url,
                config,
                &job.build_id,
                &mut log_seq,
                poll_cancellation(client, daemon_url, config, &job.build_id, authority.clone()),
            )
            .await;

            let finished = now_unix();
            match step_status {
                None => {
                    steps.push(StepResult {
                        name: step_name,
                        status: "failed".to_string(),
                        exit_code: None,
                        started_at: start,
                        finished_at: finished,
                        duration_ms: (finished - start) * 1000,
                    });
                    let _ = append_runner_log_line(
                        client,
                        daemon_url,
                        config,
                        &job.build_id,
                        &mut log_seq,
                        "stdout",
                        &step_end_marker(&format!("{stage_name}-{}", index + 1), "canceled", None),
                    )
                    .await;
                    return (
                        steps,
                        Err(BuildTerminated {
                            status: "canceled".to_string(),
                        }
                        .into()),
                    );
                }
                Some(status) => {
                    let exit_code = status.code().unwrap_or(-1);
                    steps.push(StepResult {
                        name: step_name,
                        status: if exit_code == 0 {
                            "succeeded"
                        } else {
                            "failed"
                        }
                        .to_string(),
                        exit_code: Some(exit_code),
                        started_at: start,
                        finished_at: finished,
                        duration_ms: (finished - start) * 1000,
                    });
                    let step_name = format!("{stage_name}-{}", index + 1);
                    let _ = append_runner_log_line(
                        client,
                        daemon_url,
                        config,
                        &job.build_id,
                        &mut log_seq,
                        "stdout",
                        &step_end_marker(
                            &step_name,
                            if exit_code == 0 {
                                "succeeded"
                            } else {
                                "failed"
                            },
                            Some(exit_code),
                        ),
                    )
                    .await;
                    if exit_code != 0 {
                        let err = if exit_code == 127 {
                            anyhow::anyhow!(
                                "Step failed with exit code 127 (command not found). \
Install required tooling (for example Flutter/FVM) or override build commands. Command: {}",
                                command_preview
                            )
                        } else {
                            anyhow::anyhow!("Step failed with exit code {}", exit_code)
                        };
                        return (steps, Err(err));
                    }
                    if stage_name == "build"
                        && android_artifact_extension(command).is_some()
                        && let Some(inputs) = signing_inputs.as_ref()
                    {
                        let signing_step_name = "android-sign";
                        let signing_started = now_unix();
                        let _ = append_runner_log_line(
                            client,
                            daemon_url,
                            config,
                            &job.build_id,
                            &mut log_seq,
                            "stdout",
                            &step_start_marker(
                                signing_step_name,
                                "Sign Android artifact with managed credentials",
                            ),
                        )
                        .await;
                        let signing_result = sign_android_artifacts(
                            &workspace,
                            &signing_workspace,
                            command,
                            inputs,
                            &android_signing_toolchain,
                        );
                        let signing_finished = now_unix();
                        let signing_succeeded = signing_result.is_ok();
                        steps.push(StepResult {
                            name: signing_step_name.to_string(),
                            status: if signing_succeeded {
                                "succeeded"
                            } else {
                                "failed"
                            }
                            .to_string(),
                            exit_code: if signing_succeeded { Some(0) } else { Some(1) },
                            started_at: signing_started,
                            finished_at: signing_finished,
                            duration_ms: (signing_finished - signing_started) * 1000,
                        });
                        match signing_result {
                            Ok(artifacts) => {
                                let _ = append_runner_log_line(
                                    client,
                                    daemon_url,
                                    config,
                                    &job.build_id,
                                    &mut log_seq,
                                    "stdout",
                                    &format!(
                                        "[oore-signing] Signed and verified {} Android artifact(s)",
                                        artifacts.len()
                                    ),
                                )
                                .await;
                            }
                            Err(error) => {
                                let _ = append_runner_log_line(
                                    client,
                                    daemon_url,
                                    config,
                                    &job.build_id,
                                    &mut log_seq,
                                    "stderr",
                                    &format!("[oore-signing] {error:#}"),
                                )
                                .await;
                                return (steps, Err(error));
                            }
                        }
                        let _ = append_runner_log_line(
                            client,
                            daemon_url,
                            config,
                            &job.build_id,
                            &mut log_seq,
                            "stdout",
                            &step_end_marker(signing_step_name, "succeeded", Some(0)),
                        )
                        .await;
                    }
                    if command_applied {
                        let Some(bundle) = ios_signing_bundle.as_ref() else {
                            return (
                                steps,
                                Err(anyhow::anyhow!(
                                    "iOS signing command ran without a managed signing bundle"
                                )),
                            );
                        };
                        let (materialization, mut cleanup) =
                            match install_ios_signing_bundle(&signing_workspace, bundle) {
                                Ok(prepared) => prepared,
                                Err(error) => return (steps, Err(error)),
                            };
                        let _ = append_runner_log_line(
                            client,
                            daemon_url,
                            config,
                            &job.build_id,
                            &mut log_seq,
                            "stdout",
                            &ios_signing_prepared_marker(
                                ios_signing_source.unwrap_or("pipeline_profile"),
                                bundle,
                                &materialization,
                            ),
                        )
                        .await;
                        let signing_step_name = "ios-sign";
                        let signing_started = now_unix();
                        let _ = append_runner_log_line(
                            client,
                            daemon_url,
                            config,
                            &job.build_id,
                            &mut log_seq,
                            "stdout",
                            &step_start_marker(
                                signing_step_name,
                                "Sign and package iOS archive with managed credentials",
                            ),
                        )
                        .await;
                        let signing_result =
                            manually_sign_ios_archive(&workspace, &materialization);
                        let signing_finished = now_unix();
                        let signing_succeeded = signing_result.is_ok();
                        steps.push(StepResult {
                            name: signing_step_name.to_string(),
                            status: if signing_succeeded {
                                "succeeded"
                            } else {
                                "failed"
                            }
                            .to_string(),
                            exit_code: if signing_succeeded { Some(0) } else { Some(1) },
                            started_at: signing_started,
                            finished_at: signing_finished,
                            duration_ms: (signing_finished - signing_started) * 1000,
                        });
                        match signing_result {
                            Ok(signed_archive) => {
                                let _ = append_runner_log_line(
                                    client,
                                    daemon_url,
                                    config,
                                    &job.build_id,
                                    &mut log_seq,
                                    "stdout",
                                    &format!(
                                        "[oore-signing] Signed IPA created at {}",
                                        signed_archive.ipa_path.display()
                                    ),
                                )
                                .await;
                                let mut p12_bytes =
                                    match decode_runner_b64(&bundle.p12_base64, "p12") {
                                        Ok(bytes) => bytes,
                                        Err(error) => return (steps, Err(error)),
                                    };
                                let certificate_fingerprint =
                                    hex::encode(Sha256::digest(&p12_bytes));
                                p12_bytes.zeroize();
                                ios_artifact_metadata = Some(serde_json::json!({
                                    "ios_app": {
                                        "bundle_identifier": signed_archive.app.bundle_identifier,
                                        "display_name": signed_archive.app.display_name,
                                        "version": signed_archive.app.version,
                                        "build_number": signed_archive.app.build_number,
                                    },
                                    "ios_signing": {
                                        "source": ios_signing_source.unwrap_or("pipeline_profile"),
                                        "mode": match bundle.mode {
                                            oore_contract::IosSigningMode::Manual => "manual",
                                            oore_contract::IosSigningMode::Api => "api",
                                            oore_contract::IosSigningMode::Hybrid => "hybrid",
                                        },
                                        "team_id": bundle.team_id,
                                        "bundle_ids": bundle
                                            .provisioning_profiles
                                            .iter()
                                            .map(|profile| profile.bundle_id.clone())
                                            .collect::<Vec<_>>(),
                                        "profile_uuid_map": bundle
                                            .provisioning_profiles
                                            .iter()
                                            .filter_map(|profile| profile.profile_uuid.as_ref().map(|uuid| (profile.bundle_id.clone(), uuid.clone())))
                                            .collect::<Vec<_>>(),
                                        "certificate_fingerprint": certificate_fingerprint,
                                        "effective_export_method": materialization.effective_export_method,
                                    }
                                }));
                            }
                            Err(error) => {
                                let _ = append_runner_log_line(
                                    client,
                                    daemon_url,
                                    config,
                                    &job.build_id,
                                    &mut log_seq,
                                    "stderr",
                                    &format!("[oore-signing] {error:#}"),
                                )
                                .await;
                                let _ = append_runner_log_line(
                                    client,
                                    daemon_url,
                                    config,
                                    &job.build_id,
                                    &mut log_seq,
                                    "stdout",
                                    &step_end_marker(signing_step_name, "failed", Some(1)),
                                )
                                .await;
                                return (steps, Err(error));
                            }
                        }
                        if let Err(error) = cleanup.cleanup() {
                            return (steps, Err(error));
                        }
                        let _ = append_runner_log_line(
                            client,
                            daemon_url,
                            config,
                            &job.build_id,
                            &mut log_seq,
                            "stdout",
                            &step_end_marker(signing_step_name, "succeeded", Some(0)),
                        )
                        .await;
                    }
                }
            }
        }
        if stage_name == "build" {
            signing_inputs.take();
            if let Some(mut bundle) = ios_signing_bundle.take() {
                zeroize_ios_signing_bundle(&mut bundle);
            }
        }
    }

    if ios_signing_expected && !ios_signing_command_applied {
        return (
            steps,
            Err(anyhow::anyhow!(
                "iOS signing bundle was provided, but no Flutter iOS build command was executed"
            )),
        );
    }

    let artifacts_started = now_unix();
    let artifact_result = scan_and_upload_artifacts(
        workspace.as_path(),
        client,
        daemon_url,
        config,
        &job.build_id,
        &execution_plan.artifact_patterns,
        ios_artifact_metadata.as_ref(),
    )
    .await;
    let artifacts_finished = now_unix();
    steps.push(StepResult {
        name: "artifacts".to_string(),
        status: if artifact_result.is_ok() {
            "succeeded"
        } else {
            "failed"
        }
        .to_string(),
        exit_code: artifact_result.as_ref().err().map(|_| 1),
        started_at: artifacts_started,
        finished_at: artifacts_finished,
        duration_ms: (artifacts_finished - artifacts_started) * 1000,
    });
    if let Err(error) = artifact_result {
        return (steps, Err(error));
    }

    (steps, Ok(()))
}

fn step_start_marker(name: &str, command: &str) -> String {
    format!(
        "[oore-step] {}",
        serde_json::json!({
            "event": "start",
            "name": name,
            "command": command,
        })
    )
}

fn is_sensitive_env_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    upper.contains("SECRET")
        || upper.contains("TOKEN")
        || upper.contains("PASSWORD")
        || upper.contains("CREDENTIAL")
        || upper.contains("PRIVATE")
        || upper.contains("AUTH")
        || upper.contains("P12")
        || upper.contains("PROVISION")
        || upper.contains("KEYCHAIN")
}

fn preview_env_value(key: &str, value: &str) -> String {
    if is_sensitive_env_key(key) {
        if value.is_empty() {
            String::new()
        } else {
            "***".to_string()
        }
    } else {
        value.to_string()
    }
}

fn render_step_env_preview(env: &[(String, String)]) -> String {
    if env.is_empty() {
        return "# env: (none)".to_string();
    }

    let mut parts = Vec::new();
    for (idx, (key, value)) in env.iter().enumerate() {
        if idx >= 20 {
            parts.push(format!("...(+{} more)", env.len() - 20));
            break;
        }
        let preview = preview_env_value(key, value);
        if preview.is_empty() {
            parts.push(format!("{key}="));
        } else {
            parts.push(format!("{key}={preview}"));
        }
    }

    format!("# env: {}", parts.join(" "))
}

fn normalize_legacy_env_syntax(command: &str) -> String {
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0usize;
    let mut out = String::with_capacity(command.len());

    while i < chars.len() {
        if chars[i] == '$' && i + 3 < chars.len() && chars[i + 1] == '(' && chars[i + 2] == '$' {
            let mut j = i + 3;
            if j < chars.len() && (chars[j] == '_' || chars[j].is_ascii_alphabetic()) {
                j += 1;
                while j < chars.len() && (chars[j] == '_' || chars[j].is_ascii_alphanumeric()) {
                    j += 1;
                }
                if j < chars.len() && chars[j] == ')' {
                    let var_name: String = chars[(i + 3)..j].iter().collect();
                    out.push('$');
                    out.push_str(&var_name);
                    i = j + 1;
                    continue;
                }
            }
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

fn lookup_env_value<'a>(env: &'a [(String, String)], key: &str) -> Option<&'a str> {
    env.iter()
        .find(|(k, _)| k == key)
        .map(|(_, value)| value.as_str())
}

fn render_command_preview(command: &str, env: &[(String, String)]) -> String {
    let normalized = normalize_legacy_env_syntax(command);
    let chars: Vec<char> = normalized.chars().collect();
    let mut i = 0usize;
    let mut out = String::with_capacity(normalized.len());

    while i < chars.len() {
        if chars[i] == '$' {
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                let mut j = i + 2;
                while j < chars.len() && chars[j] != '}' {
                    j += 1;
                }
                if j < chars.len() && chars[j] == '}' {
                    let key: String = chars[(i + 2)..j].iter().collect();
                    if !key.is_empty() {
                        if let Some(value) = lookup_env_value(env, &key) {
                            out.push_str(&preview_env_value(&key, value));
                        } else {
                            out.push('$');
                            out.push('{');
                            out.push_str(&key);
                            out.push('}');
                        }
                        i = j + 1;
                        continue;
                    }
                }
            } else if i + 1 < chars.len()
                && (chars[i + 1] == '_' || chars[i + 1].is_ascii_alphabetic())
            {
                let mut j = i + 2;
                while j < chars.len() && (chars[j] == '_' || chars[j].is_ascii_alphanumeric()) {
                    j += 1;
                }
                let key: String = chars[(i + 1)..j].iter().collect();
                if let Some(value) = lookup_env_value(env, &key) {
                    out.push_str(&preview_env_value(&key, value));
                } else {
                    out.push('$');
                    out.push_str(&key);
                }
                i = j;
                continue;
            }
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

fn is_flutter_build_command(command: &str) -> bool {
    let args: Vec<&str> = command.split_whitespace().collect();
    args.windows(2).any(|window| window == ["flutter", "build"])
        || args
            .windows(3)
            .any(|window| window == ["fvm", "flutter", "build"])
}

fn with_dart_define_file(command: &str, define_file: &str) -> String {
    if command.contains("--dart-define-from-file=") {
        return command.to_string();
    }
    format!("{command} --dart-define-from-file={define_file}")
}

fn step_end_marker(name: &str, status: &str, exit_code: Option<i32>) -> String {
    format!(
        "[oore-step] {}",
        serde_json::json!({
            "event": "end",
            "name": name,
            "status": status,
            "exit_code": exit_code,
        })
    )
}

async fn append_runner_log_line(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    seq: &mut i64,
    stream: &str,
    content: &str,
) -> anyhow::Result<()> {
    let body = serde_json::json!({
        "chunks": [{
            "sequence": *seq,
            "content": content,
            "stream": stream,
        }],
    });

    let resp = client
        .post(format!(
            "{}/v1/runners/{}/jobs/{}/logs",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("log append failed: {}", resp.status());
    }

    *seq += 1;
    Ok(())
}

fn artifact_type_for_extension(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "apk" => Some("apk"),
        "ipa" => Some("ipa"),
        _ => None,
    }
}

fn walk_artifact_candidates(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    fn walk(dir: &Path, result: &mut Vec<PathBuf>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                let is_hidden_dir = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| name.starts_with('.'));
                if is_hidden_dir {
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) == Some("app") {
                    result.push(path);
                    continue;
                }
                walk(&path, result);
            } else if file_type.is_file() {
                result.push(path);
            }
        }
    }
    walk(dir, &mut result);
    result
}

const ARTIFACT_UPLOAD_CHUNK_BYTES: usize = 64 * 1024;

async fn compute_file_sha256(path: &std::path::Path) -> anyhow::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; ARTIFACT_UPLOAD_CHUNK_BYTES];
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn artifact_upload_body(mut file: tokio::fs::File) -> reqwest::Body {
    reqwest::Body::wrap_stream(async_stream::stream! {
        loop {
            let mut chunk = vec![0u8; ARTIFACT_UPLOAD_CHUNK_BYTES];
            match file.read(&mut chunk).await {
                Ok(0) => break,
                Ok(read) => {
                    chunk.truncate(read);
                    yield Ok::<Vec<u8>, std::io::Error>(chunk);
                }
                Err(error) => {
                    yield Err(error);
                    break;
                }
            }
        }
    })
}

fn runner_artifact_upload_url(daemon_url: &str, upload_url: &str) -> String {
    const LOCAL_UPLOAD_PATH: &str = "/v1/artifacts/local-upload/";
    match upload_url.split_once(LOCAL_UPLOAD_PATH) {
        Some((_, token)) => format!(
            "{}{LOCAL_UPLOAD_PATH}{token}",
            daemon_url.trim_end_matches('/')
        ),
        None => upload_url.to_string(),
    }
}

async fn scan_and_upload_artifacts(
    workspace: &std::path::Path,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    artifact_patterns: &[String],
    ios_metadata: Option<&serde_json::Value>,
) -> anyhow::Result<()> {
    if artifact_patterns.is_empty() {
        return Ok(());
    }

    let mut artifacts: Vec<(PathBuf, String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for path in walk_artifact_candidates(workspace) {
        let relative = path
            .strip_prefix(workspace)
            .ok()
            .and_then(Path::to_str)
            .map(|value| value.replace(std::path::MAIN_SEPARATOR, "/"));
        let Some(relative) = relative else { continue };
        if !artifact_patterns
            .iter()
            .any(|pattern| artifact_pattern_matches(pattern, &relative))
            || !seen.insert(relative)
        {
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) == Some("app") && path.is_dir() {
            let bundle_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("app.app");
            let package_dir = workspace.join(".oore-artifacts");
            fs::create_dir_all(&package_dir)?;
            let package_path = package_dir.join(format!("{bundle_name}.zip"));
            let status = tokio::process::Command::new("ditto")
                .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
                .arg(&path)
                .arg(&package_path)
                .status()
                .await
                .context("failed to package .app bundle with ditto")?;
            if !status.success() {
                anyhow::bail!("failed to package .app bundle {}", path.display());
            }
            artifacts.push((
                package_path,
                "app".to_string(),
                format!("{bundle_name}.zip"),
            ));
        } else {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("artifact")
                .to_string();
            let artifact_type = path
                .extension()
                .and_then(|ext| ext.to_str())
                .and_then(artifact_type_for_extension)
                .unwrap_or("generic")
                .to_string();
            artifacts.push((path, artifact_type, name));
        }
    }

    if artifacts.is_empty() {
        anyhow::bail!(
            "artifact patterns matched no files: {}",
            artifact_patterns.join(", ")
        );
    }

    println!("Found {} artifact(s) to upload", artifacts.len());

    for (path, artifact_type, name) in &artifacts {
        let file_size = i64::try_from(tokio::fs::metadata(path).await?.len())
            .context("artifact size exceeds the supported range")?;
        let checksum = Some(compute_file_sha256(path).await?);

        let metadata = match ios_metadata {
            Some(value) if artifact_type == "ipa" => value.clone(),
            _ => serde_json::json!({}),
        };

        let body = serde_json::json!({
            "name": name,
            "artifact_type": artifact_type,
            "file_size": Some(file_size),
            "checksum": checksum,
            "metadata": metadata,
        });

        let resp = client
            .post(format!(
                "{}/v1/runners/{}/jobs/{}/artifacts",
                daemon_url, config.runner_id, build_id
            ))
            .bearer_auth(&config.runner_token)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("failed to reserve artifact {name}"))?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "artifact reservation failed for {name} (HTTP {})",
                resp.status()
            );
        }

        let create_resp: oore_contract::CreateArtifactResponse = resp.json().await?;
        let artifact_id = create_resp.artifact.id;
        let upload_url = runner_artifact_upload_url(daemon_url, &create_resp.upload_url);

        if upload_url.is_empty() {
            abort_artifact(
                client,
                daemon_url,
                config,
                build_id,
                &artifact_id,
                "artifact storage is not configured",
            )
            .await;
            anyhow::bail!("artifact storage is not configured for {name}");
        }

        let upload = async {
            let file = tokio::fs::File::open(path)
                .await
                .with_context(|| format!("failed to open artifact {name}"))?;
            let response = client
                .put(&upload_url)
                .header(reqwest::header::CONTENT_LENGTH, file_size.to_string())
                .body(artifact_upload_body(file))
                .send()
                .await?;
            if !response.status().is_success() {
                anyhow::bail!("upload returned HTTP {}", response.status());
            }
            let response = client
                .post(format!(
                    "{}/v1/runners/{}/jobs/{}/artifacts/{}/complete",
                    daemon_url, config.runner_id, build_id, artifact_id
                ))
                .bearer_auth(&config.runner_token)
                .json(&CompleteArtifactRequest {
                    error_message: None,
                })
                .send()
                .await?;
            if !response.status().is_success() {
                anyhow::bail!("completion returned HTTP {}", response.status());
            }
            let _: CompleteArtifactResponse = response.json().await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(error) = upload {
            abort_artifact(
                client,
                daemon_url,
                config,
                build_id,
                &artifact_id,
                &error.to_string(),
            )
            .await;
            return Err(error.context(format!("failed to finalize artifact {name}")));
        }
        println!("  Uploaded artifact {}", name);
    }
    Ok(())
}

async fn abort_artifact(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    artifact_id: &str,
    error_message: &str,
) {
    let _ = client
        .post(format!(
            "{}/v1/runners/{}/jobs/{}/artifacts/{}/abort",
            daemon_url, config.runner_id, build_id, artifact_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&CompleteArtifactRequest {
            error_message: Some(error_message.to_string()),
        })
        .send()
        .await;
}

struct StatusReport<'a> {
    build_id: &'a str,
    status: &'a str,
    exit_code: Option<i32>,
    error_message: Option<String>,
    steps: &'a [StepResult],
}

async fn report_status(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    report: StatusReport<'_>,
) -> anyhow::Result<()> {
    let StatusReport {
        build_id,
        status,
        exit_code,
        error_message,
        steps,
    } = report;

    let body = serde_json::json!({
        "status": status,
        "exit_code": exit_code,
        "error_message": error_message,
        "steps": steps,
    });

    let resp = client
        .post(format!(
            "{}/v1/runners/{}/jobs/{}/status",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Status update failed: {}", resp.status());
    }

    Ok(())
}

async fn run_and_stream(
    mut child: tokio::process::Child,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    seq: &mut i64,
    cancel_fut: impl std::future::Future<Output = ()>,
) -> Option<std::process::ExitStatus> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let client_out = client.clone();
    let daemon_out = daemon_url.to_string();
    let config_out_id = config.runner_id.clone();
    let config_out_token = config.runner_token.clone();
    let build_out = build_id.to_string();

    let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<(String, String)>(256);

    if let Some(stdout) = stdout {
        let tx = line_tx.clone();
        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send((line, "stdout".to_string())).await.is_err() {
                    break;
                }
            }
        });
    }

    if let Some(stderr) = stderr {
        let tx = line_tx.clone();
        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send((line, "stderr".to_string())).await.is_err() {
                    break;
                }
            }
        });
    }

    drop(line_tx);

    let upload_client = client_out;
    let upload_daemon = daemon_out;
    let upload_config_id = config_out_id;
    let upload_config_token = config_out_token;
    let upload_build = build_out;
    let seq_start = *seq;
    let upload_handle = tokio::spawn(async move {
        let mut local_seq = seq_start;
        let mut batch = Vec::new();
        let mut interval = tokio::time::interval(Duration::from_millis(500));

        loop {
            tokio::select! {
                line = line_rx.recv() => {
                    match line {
                        Some((content, stream)) => {
                            batch.push(serde_json::json!({
                                "sequence": local_seq,
                                "content": content,
                                "stream": stream,
                            }));
                            local_seq += 1;

                            if batch.len() >= 50 {
                                let body = serde_json::json!({ "chunks": batch });
                                let _ = upload_client
                                    .post(format!(
                                        "{}/v1/runners/{}/jobs/{}/logs",
                                        upload_daemon, upload_config_id, upload_build
                                    ))
                                    .bearer_auth(&upload_config_token)
                                    .json(&body)
                                    .send()
                                    .await;
                                batch = Vec::new();
                            }
                        }
                        None => {
                            if !batch.is_empty() {
                                let body = serde_json::json!({ "chunks": batch });
                                let _ = upload_client
                                    .post(format!(
                                        "{}/v1/runners/{}/jobs/{}/logs",
                                        upload_daemon, upload_config_id, upload_build
                                    ))
                                    .bearer_auth(&upload_config_token)
                                    .json(&body)
                                    .send()
                                    .await;
                            }
                            return local_seq;
                        }
                    }
                }
                _ = interval.tick() => {
                    if !batch.is_empty() {
                        let body = serde_json::json!({ "chunks": batch });
                        let _ = upload_client
                            .post(format!(
                                "{}/v1/runners/{}/jobs/{}/logs",
                                upload_daemon, upload_config_id, upload_build
                            ))
                            .bearer_auth(&upload_config_token)
                            .json(&body)
                            .send()
                            .await;
                        batch = Vec::new();
                    }
                }
            }
        }
    });

    let status = tokio::select! {
        result = child.wait() => {
            result.ok()
        },
        _ = cancel_fut => {
            child.kill().await.ok();
            None
        }
    };

    if let Ok(final_seq) = upload_handle.await {
        *seq = final_seq;
    }

    status
}

#[cfg(test)]
mod tests {
    use super::load_ui_execution_config;
    use serde_json::json;

    #[test]
    fn snapshot_config_preserves_defaults_trimming_and_field_errors() {
        assert!(load_ui_execution_config(&json!({})).is_ok());
        assert!(load_ui_execution_config(&json!({"ui_execution_config": null})).is_err());
        for (group, field) in [
            ("commands", "pre_build"),
            ("commands", "build"),
            ("commands", "post_build"),
            ("platform_build_args", "android"),
            ("platform_build_args", "ios"),
            ("platform_build_args", "macos"),
        ] {
            let mut config = json!({});
            config[group] = json!({});
            config[group][field] = json!(["  first  ", "\tsecond\n"]);
            let snapshot = json!({"ui_execution_config": config});
            let normalized = load_ui_execution_config(&snapshot).unwrap();
            assert_eq!(
                serde_json::to_value(normalized).unwrap()[group][field],
                json!(["first", "second"])
            );
            let mut invalid = snapshot;
            invalid["ui_execution_config"][group][field][1] = json!("  ");
            assert_eq!(
                load_ui_execution_config(&invalid).unwrap_err().to_string(),
                format!("{group}.{field}[1] must not be empty")
            );
        }
        assert_eq!(
            load_ui_execution_config(&json!({"ui_execution_config": {"artifact_patterns": [" "]}}))
                .unwrap_err()
                .to_string(),
            "artifacts.patterns[0] must not be empty"
        );
    }
}
