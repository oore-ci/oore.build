use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::net::IpAddr;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub mod store_version;

use anyhow::Context;
use base64::Engine as _;
use caches::{CacheObservation, CacheStats, RunnerCaches};
use oore_contract::{
    AndroidSigningBuildType, BuildIsolation, BuildPlatform, BuildStatus, ClaimJobRequest,
    ClaimJobResponse, ClaimedJob, CompleteArtifactRequest, CompleteArtifactResponse,
    IosExportMethod, JobStatusResponse, PUBLISH_APP_STORE_STEP, PUBLISH_PLAY_STEP,
    PipelineCommandStages, PipelineEnvVar, PipelineExecutionConfig, PlanRequest, PlanStep,
    PlatformBuildArgs, PlatformBuildCommands, RUNNER_PROTOCOL_VERSION, RunnerAndroidSigningProfile,
    RunnerAndroidSigningResponse, RunnerHeartbeatResponse, RunnerIosSigningBundle,
    RunnerIosSigningResponse, RunnerJobSecretsResponse, RunnerPublishPlan, RunnerPublishResult,
    RunnerRetryArtifactsResponse, StepPhase, StepResult, artifact_pattern_matches,
    is_valid_pipeline_env_key, missing_runner_labels, normalize_runner_labels,
    parse_repository_config, pipeline_config::select_v1_platforms, plan_execution_config,
    replace_secret_references, store_version_code, validate_artifact_pattern,
    validate_build_number_offset, validate_repository_config_path,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncReadExt};
use zeroize::Zeroize;

mod app_store_upload;
mod caches;
mod metrics;
mod publish;
mod tart;
mod test_reports;

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
/// How long a build keeps running while the backend cannot confirm it, for
/// example through a backend restart.
const CONTROLLER_GRACE: Duration = Duration::from_secs(300);
/// `BuildTerminated` status for a build stopped after `CONTROLLER_GRACE`.
const CONTROLLER_UNAVAILABLE: &str = "controller_unavailable";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest wait for the next bytes of any response, so a half-open
/// connection fails instead of hanging the runner.
const READ_TIMEOUT: Duration = Duration::from_secs(120);
/// Whole-request bound for the small JSON calls to the backend.
const CONTROL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// How long the runner keeps trying to deliver a build's final status.
const FINAL_REPORT_WINDOW: Duration = Duration::from_secs(600);
/// How long it keeps trying to say a claimed build started. Shorter than the
/// backend's 5 minute lease, after which the build is queued again.
const RUNNING_REPORT_WINDOW: Duration = Duration::from_secs(120);
/// Longest a Google Play upload may run before the runner stops it.
const PLAY_PUBLISH_TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// How long a canceled step's processes get to exit after SIGTERM.
const STEP_TERM_GRACE: Duration = Duration::from_secs(5);
/// How long a step's output may stay quiet after the step exits before the
/// runner stops reading it, in case a background process it left behind
/// still holds the pipes. `STEP_OUTPUT_LIMIT` bounds the whole wait.
const STEP_OUTPUT_GRACE: Duration = Duration::from_secs(5);
const STEP_OUTPUT_LIMIT: Duration = Duration::from_secs(60);

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

/// A build workspace whose path is the same for every build of a project on
/// this runner, so Xcode keeps one DerivedData folder per project instead of
/// starting a new one per build. Falls back to a random name if a previous
/// workspace for the project is still present.
fn create_project_workspace_in(
    parent: &Path,
    runner_id: &str,
    project_id: &str,
) -> std::io::Result<PathBuf> {
    let digest = Sha256::digest(project_id.as_bytes());
    let path = parent.join(format!(
        "{}{}",
        runner_workspace_prefix(runner_id),
        hex::encode(&digest[..16])
    ));
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(&path) {
        Ok(()) => Ok(path),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            create_private_workspace_in(parent, runner_id)
        }
        Err(error) => Err(error),
    }
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
        .env_remove(RUNNER_SERVICE_ACK_PATH_ENV)
        // Its own process group, so canceling the step reaches everything
        // the script started, not just the shell.
        .process_group(0);
    command
}

/// Stops a step: SIGTERM to its process group, then SIGKILL to whatever is
/// left once `STEP_TERM_GRACE` passes. A child that leads no group of its
/// own is killed directly.
async fn stop_process_group(child: &mut tokio::process::Child) {
    let group = child.id().and_then(|pid| libc::pid_t::try_from(pid).ok());
    // SAFETY: killpg only sends a signal. The child leads its group when it
    // was spawned with process_group(0); otherwise no group has its id and
    // the call fails harmlessly.
    let signal_group =
        |signal| group.is_some_and(|pgid| unsafe { libc::killpg(pgid, signal) } == 0);
    if signal_group(libc::SIGTERM) {
        let _ = tokio::time::timeout(STEP_TERM_GRACE, child.wait()).await;
        signal_group(libc::SIGKILL);
    }
    child.kill().await.ok();
}

/// Waits for every task in `handles`.
async fn join_all(handles: Vec<tokio::task::JoinHandle<()>>) {
    for handle in handles {
        let _ = handle.await;
    }
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

    let tart_version = tart::find_tart()
        .and_then(|tart| tart::tart_version(&tart))
        .unwrap_or_default();

    serde_json::json!({
        "os": std::env::consts::OS,
        "os_version": os_version,
        "arch": arch,
        "xcode_version": xcode_version,
        "tart_version": tart_version,
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
    trimmed.starts_with("flutter build apk") || trimmed.starts_with("flutter build appbundle")
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
) -> anyhow::Result<String> {
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
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() {
        anyhow::bail!(
            "failed to {action}: {}",
            android_signer_failure_detail(&stdout, &output.stderr)
        );
    }
    Ok(stdout)
}

/// jarsigner and apksigner print most diagnostics to stdout, so failures need
/// both streams to be actionable.
fn android_signer_failure_detail(stdout: &str, stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let detail = [stderr.trim(), stdout.trim()]
        .into_iter()
        .filter(|stream| !stream.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if detail.is_empty() {
        "signer exited without output".to_string()
    } else {
        detail
    }
}

/// Checks `jarsigner -verify` output. `-strict` is deliberately not used:
/// Android upload keys are self-signed and AABs are not timestamped, which
/// `-strict` reports as errors. Plain `-verify` still exits non-zero on digest
/// or signature mismatches, but exits zero for an unsigned jar, so the output
/// must confirm every entry was verified.
fn check_android_bundle_verification(stdout: &str) -> anyhow::Result<()> {
    let verified = stdout
        .lines()
        .any(|line| line.trim().starts_with("jar verified"));
    if !verified || stdout.contains("unsigned entries") {
        anyhow::bail!(
            "failed to verify Android App Bundle signature: {}",
            android_signer_failure_detail(stdout, &[])
        );
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
            let verification = run_android_signer_command(
                toolchain,
                &toolchain.jarsigner,
                &["-verify".to_string(), signed_artifact.display().to_string()],
                inputs,
                "verify Android App Bundle signature",
            )?;
            check_android_bundle_verification(&verification)?;
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

/// Asks the host's `flutter`, found through the same PATH and env the
/// pipeline steps get, which framework version it is. `Err` carries why it
/// could not tell.
async fn host_flutter_version(
    workspace: &Path,
    step_env: &[(String, String)],
) -> Result<String, String> {
    let mut command = repository_shell_command("flutter --version --machine", workspace);
    command
        .envs(step_env.iter().map(|(key, value)| (key, value)))
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(300), command.output())
        .await
        .map_err(|_| "flutter --version timed out after 5 minutes".to_string())?
        .map_err(|error| format!("failed to run flutter --version: {error}"))?;
    if output.status.code() == Some(127) {
        return Err("no flutter on the runner's PATH".to_string());
    }
    if !output.status.success() {
        return Err(format!(
            "flutter --version failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_flutter_machine_version(&String::from_utf8_lossy(&output.stdout)).ok_or_else(|| {
        "could not read frameworkVersion from flutter --version --machine".to_string()
    })
}

/// `flutter --version --machine` prints JSON, possibly after first-run
/// banners, so read the outermost object.
fn parse_flutter_machine_version(stdout: &str) -> Option<String> {
    let start = stdout.find('{')?;
    let end = stdout.rfind('}')?;
    let value: serde_json::Value = serde_json::from_str(stdout.get(start..=end)?).ok()?;
    value.get("frameworkVersion")?.as_str().map(str::to_string)
}

/// Host builds cannot switch Flutter, so a pinned version (from `.fvmrc` or
/// the pipeline) must be the one the host has installed. A channel such as
/// `stable` is not checked, since a host checkout of a tag reports no
/// channel. `Ok` and `Err` both carry the line for the build log.
fn check_host_flutter(requested: &str, found: Result<String, String>) -> Result<String, String> {
    // FVM writes `3.47.7@beta` for a release from a channel.
    let pinned = requested.split('@').next().unwrap_or(requested).trim();
    let pinned = pinned.strip_prefix('v').unwrap_or(pinned);
    let is_version = pinned.starts_with(|c: char| c.is_ascii_digit());
    match (is_version, found) {
        (true, Ok(found)) if found == pinned => Ok(format!(
            "Host build: using the runner's Flutter {found}, matching the requested {requested}"
        )),
        (true, Ok(found)) => Err(format!(
            "Host build needs Flutter {pinned}, but the runner's flutter is {found}. Install {pinned} on the runner host (see Runner prerequisites in the docs) or set isolation: vm"
        )),
        (true, Err(reason)) => Err(format!(
            "Host build needs Flutter {pinned}, but {reason}. Install {pinned} on the runner host (see Runner prerequisites in the docs) or set isolation: vm"
        )),
        (false, Ok(found)) => Ok(format!(
            "Host build: using the runner's Flutter {found} for {requested}; pin a version in .fvmrc to enforce one"
        )),
        (false, Err(reason)) => Ok(format!(
            "Host build: could not check the runner's Flutter ({reason})"
        )),
    }
}

/// The guest-side equivalent of [`sign_android_artifacts`]: the same
/// apksigner and jarsigner steps, run with the VM image's Android SDK and JDK
/// so the host needs neither. The script travels over the guest agent's
/// channel and writes the keystore only to a private directory on the VM's
/// own disk, never to the shared workspace.
fn android_vm_signing_script(
    guest_artifacts: &[String],
    extension: &str,
    inputs: &AndroidSigningInputs,
) -> String {
    use tart::shell_quote as q;
    let keystore = base64::engine::general_purpose::STANDARD.encode(&inputs.keystore_bytes);
    let mut script = format!(
        r#"set -e
umask 077
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
printf %s {keystore} | base64 --decode > "$dir/keystore.jks"
export {ANDROID_SIGNER_STORE_PASSWORD_ENV}={store}
export {ANDROID_SIGNER_KEY_PASSWORD_ENV}={key}
apksigner="$(ls -1 "${{ANDROID_HOME:-$HOME/android-sdk}}"/build-tools/*/apksigner 2>/dev/null | sort | tail -n 1)"
"#,
        keystore = q(&keystore),
        store = q(&inputs.keystore_password),
        key = q(&inputs.key_password),
    );
    let alias = q(&inputs.key_alias);
    for (index, artifact) in guest_artifacts.iter().enumerate() {
        let artifact = q(artifact);
        let signed = format!("\"$dir/signed-{index}.{extension}\"");
        if extension == "apk" {
            script.push_str(&format!(
                r#"[ -n "$apksigner" ] || {{ echo "apksigner is missing from the VM image's Android SDK" >&2; exit 1; }}
"$apksigner" sign --ks "$dir/keystore.jks" --ks-key-alias {alias} --ks-pass env:{ANDROID_SIGNER_STORE_PASSWORD_ENV} --key-pass env:{ANDROID_SIGNER_KEY_PASSWORD_ENV} --out {signed} {artifact}
"$apksigner" verify --verbose --print-certs {signed} >&2
"#
            ));
        } else {
            script.push_str(&format!(
                r#"cp {artifact} {signed}
zip -d {signed} 'META-INF/*.SF' 'META-INF/*.RSA' 'META-INF/*.DSA' 'META-INF/*.EC' 'META-INF/MANIFEST.MF' > /dev/null || [ $? -eq 12 ]
jarsigner -keystore "$dir/keystore.jks" -storepass:env {ANDROID_SIGNER_STORE_PASSWORD_ENV} -keypass:env {ANDROID_SIGNER_KEY_PASSWORD_ENV} {signed} {alias} >&2
verified="$(jarsigner -verify {signed})" || {{ printf '%s\n' "$verified" >&2; exit 1; }}
if ! printf '%s\n' "$verified" | grep -q '^ *jar verified' || printf '%s\n' "$verified" | grep -q 'unsigned entries'; then
  printf 'failed to verify Android App Bundle signature: %s\n' "$verified" >&2
  exit 1
fi
"#
            ));
        }
        script.push_str(&format!("cp {signed} {artifact}\n"));
    }
    script
}

async fn sign_android_artifacts_in_vm(
    vm: &tart::TartVm,
    workspace: &Path,
    command: &str,
    inputs: &AndroidSigningInputs,
) -> anyhow::Result<Vec<PathBuf>> {
    let extension = android_artifact_extension(command)
        .ok_or_else(|| anyhow::anyhow!("unsupported Android signing command"))?;
    let artifacts = android_artifacts_for_signing(workspace, extension)?;
    let guest_artifacts = artifacts
        .iter()
        .map(|artifact| {
            let relative = artifact
                .strip_prefix(workspace)
                .context("Android artifact is outside the build workspace")?;
            Ok(format!("{}/{}", vm.guest_workspace(), relative.display()))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut script = android_vm_signing_script(&guest_artifacts, extension, inputs);
    let output = vm.run_script(&script).await;
    script.zeroize();
    let output = output?;
    anyhow::ensure!(
        output.status.success(),
        "failed to sign Android artifacts in the VM ({}): {}",
        output.status,
        android_signer_failure_detail(&String::from_utf8_lossy(&output.stdout), &output.stderr)
    );
    vm.sync_workspace_out().await?;
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
    require_gui_login_session("iOS signing")
}

fn require_gui_login_session(purpose: &str) -> anyhow::Result<()> {
    // SAFETY: `geteuid` accepts no arguments and has no failure state.
    let uid = unsafe { libc::geteuid() };
    let output = Command::new("/bin/launchctl")
        .args(["print", &format!("gui/{uid}")])
        .output()
        .context("failed to inspect the runner account login session")?;
    anyhow::ensure!(
        output.status.success(),
        "{purpose} requires an active macOS login session. Log into the runner account and retry the build"
    );
    if is_managed_system_runner() {
        active_gui_session_process_id(uid, purpose)?;
    }
    Ok(())
}

fn is_managed_system_runner() -> bool {
    std::env::var_os("XPC_SERVICE_NAME").is_some_and(|name| name == MANAGED_RUNNER_SERVICE_LABEL)
}

fn active_gui_session_process_id(uid: u32, purpose: &str) -> anyhow::Result<u32> {
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
            "{purpose} requires an active macOS login session. Log into the runner account and retry the build"
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

/// Xcode 15.3 renamed `ad-hoc` to `release-testing` and `app-store` to
/// `app-store-connect`; older toolchains only know the original names.
fn choose_ios_export_method(help_text: &str, export_method: IosExportMethod) -> &'static str {
    let help_text = help_text.to_ascii_lowercase();
    match export_method {
        IosExportMethod::AdHoc if help_text.contains("release-testing") => "release-testing",
        IosExportMethod::AdHoc => "ad-hoc",
        IosExportMethod::AppStore if help_text.contains("app-store-connect") => "app-store-connect",
        IosExportMethod::AppStore => "app-store",
    }
}

fn resolve_ios_export_method(export_method: IosExportMethod) -> String {
    let help_text = match Command::new("xcodebuild").arg("-help").output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
        _ => String::new(),
    };
    choose_ios_export_method(&help_text, export_method).to_string()
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

        let effective_export_method = resolve_ios_export_method(bundle.export_method);
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
    };
    has_flutter_target("ios") || has_flutter_target("ipa")
}

fn contains_flutter_build_target(args: &[String], target: &str) -> bool {
    args.windows(3)
        .any(|window| window == ["flutter", "build", target])
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

/// Cache sizes for the heartbeat and the purge the backend last asked for.
#[derive(Debug, Default)]
struct CacheReport {
    stats: Option<CacheStats>,
    purge_requested_at: Option<i64>,
    vm_images: Option<VmImageUsage>,
}

/// The VM images this runner built that are on disk, outside the cache cap.
#[derive(Debug, Clone, serde::Serialize)]
struct VmImageUsage {
    count: usize,
    bytes: u64,
}

type SharedCacheReport = Arc<std::sync::Mutex<CacheReport>>;

fn heartbeat_capabilities(
    capabilities: &serde_json::Value,
    cache_report: &SharedCacheReport,
) -> serde_json::Value {
    let mut capabilities = capabilities.clone();
    let (stats, vm_images) = cache_report
        .lock()
        .map(|report| (report.stats.clone(), report.vm_images.clone()))
        .unwrap_or_default();
    if let (Some(object), Some(stats)) = (capabilities.as_object_mut(), stats)
        && let Ok(value) = serde_json::to_value(stats)
    {
        object.insert("caches".to_string(), value);
    }
    if let (Some(object), Some(vm_images)) = (capabilities.as_object_mut(), vm_images)
        && let Ok(value) = serde_json::to_value(vm_images)
    {
        object.insert("vm_images".to_string(), value);
    }
    capabilities
}

/// The runner's HTTP client. Uploads can take long, so it bounds connecting
/// and each wait for response bytes rather than whole requests; small JSON
/// calls add `CONTROL_REQUEST_TIMEOUT`.
fn runner_http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .build()
        .context("failed to build the runner HTTP client")
}

async fn send_runner_heartbeat(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    capabilities: &serde_json::Value,
    cache_report: &SharedCacheReport,
) -> anyhow::Result<()> {
    let capabilities = heartbeat_capabilities(capabilities, cache_report);
    let cache_bytes = cache_report
        .lock()
        .ok()
        .and_then(|report| report.stats.as_ref().map(|stats| stats.total_bytes));
    let metrics = metrics::sample_runner_metrics(&std::env::temp_dir(), cache_bytes);
    let response = client
        .post(format!(
            "{}/v1/runners/{}/heartbeat",
            daemon_url, config.runner_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&serde_json::json!({
            "status": "online",
            "capabilities": capabilities,
            "metrics": metrics,
        }))
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .context("runner heartbeat could not reach the backend")?;
    let status = response.status();
    if status.is_success() {
        // Older backends answer 204 with no body; that means no purge request.
        let purge_requested_at = response
            .json::<RunnerHeartbeatResponse>()
            .await
            .ok()
            .and_then(|body| body.cache_purge_requested_at);
        if let Ok(mut report) = cache_report.lock() {
            report.purge_requested_at = purge_requested_at;
        }
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
    cache_report: &SharedCacheReport,
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
        match send_runner_heartbeat(client, daemon_url, config, capabilities, cache_report).await {
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
    let client = runner_http_client()?;
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
    // Sweeps leftover VMs before the first claim.
    let mut vm_upkeep = tart::find_tart().map(VmUpkeep::new);

    println!("Starting runner '{}' ({})", config.name, config.runner_id);
    println!("Connecting to: {}", daemon_url);

    let capabilities = detect_capabilities().await;

    let cache_report = SharedCacheReport::default();
    let caches = match RunnerCaches::for_runner(
        &config.runner_id,
        &runner_workspace_prefix(&config.runner_id),
    ) {
        Ok(caches) => {
            println!("Dependency caches: {}", caches.root().display());
            if let Ok(mut report) = cache_report.lock() {
                report.stats = caches.last_stats();
            }
            Some(Arc::new(caches))
        }
        Err(error) => {
            eprintln!("Warning: dependency caches are disabled: {error:#}");
            None
        }
    };

    // Do not enter the claim loop or advertise service readiness until the
    // backend has authenticated this exact runner/config/release.
    establish_runner_heartbeat(&client, &daemon_url, &config, &capabilities, &cache_report).await?;
    if let (Some(path), Some(template)) =
        (service_ack_path.as_deref(), service_ack_template.as_ref())
    {
        write_runner_service_ack(path, &refreshed_runner_service_ack(template, now_unix()))?;
    }

    let hb_client = client.clone();
    let hb_url = daemon_url.clone();
    let hb_config = config.clone();
    let hb_capabilities = capabilities.clone();
    let hb_cache_report = cache_report.clone();
    let hb_ack_path = service_ack_path;
    let hb_ack_template = service_ack_template;
    let (fatal_tx, mut fatal_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            match send_runner_heartbeat(
                &hb_client,
                &hb_url,
                &hb_config,
                &hb_capabilities,
                &hb_cache_report,
            )
            .await
            {
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

    if let Some(caches) = &caches {
        run_cache_maintenance(caches.clone(), cache_report.clone(), None).await;
    }

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
        if let Some(caches) = &caches {
            purge_caches_if_requested(caches, &cache_report).await;
        }
        if let Some(upkeep) = vm_upkeep.as_mut() {
            upkeep.run(&config.runner_id, &cache_report).await;
        }
        match claim_and_execute(
            &client,
            &daemon_url,
            &config,
            caches.as_ref(),
            &cache_report,
        )
        .await
        {
            Ok(executed) => {
                // A build whose VM could not be deleted leaves it behind.
                if executed && let Some(upkeep) = vm_upkeep.as_mut() {
                    upkeep.sweep_now();
                }
            }
            Err(error) if terminal_runner_error(&error) => return Err(error),
            Err(e) => {
                eprintln!("Error during claim/execute: {}", e);
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
    }
}

/// Idle-time VM housekeeping: deletes VMs this runner left behind and prunes
/// the VM images it built that no build used for a while. Runs only between
/// builds, when this runner has no VM of its own.
struct VmUpkeep {
    tart: PathBuf,
    next_sweep: tokio::time::Instant,
    next_prune: tokio::time::Instant,
}

impl VmUpkeep {
    const SWEEP_INTERVAL: Duration = Duration::from_secs(10 * 60);
    const PRUNE_INTERVAL: Duration = Duration::from_secs(60 * 60);

    fn new(tart: PathBuf) -> Self {
        let now = tokio::time::Instant::now();
        Self {
            tart,
            next_sweep: now,
            next_prune: now,
        }
    }

    fn sweep_now(&mut self) {
        self.next_sweep = tokio::time::Instant::now();
    }

    async fn run(&mut self, runner_id: &str, cache_report: &SharedCacheReport) {
        let now = tokio::time::Instant::now();
        if now >= self.next_sweep {
            self.next_sweep = now + Self::SWEEP_INTERVAL;
            match tart::delete_stale_vms(&self.tart, runner_id).await {
                Ok(sweep) => {
                    if sweep.removed > 0 {
                        println!("Deleted {} leftover build VM(s)", sweep.removed);
                    }
                    for error in sweep.errors {
                        eprintln!("Warning: could not delete leftover build VM {error}");
                    }
                }
                Err(error) => eprintln!("Warning: could not list leftover build VMs: {error:#}"),
            }
        }
        if now >= self.next_prune {
            self.next_prune = now + Self::PRUNE_INTERVAL;
            let Ok(dir) = tart::slot_dir() else {
                return;
            };
            match tart::prune_unused_images(
                &self.tart,
                &dir,
                tart::local_vms_dir().as_deref(),
                tart::image_max_age(),
                now_unix(),
            )
            .await
            {
                Ok(prune) => {
                    for image in &prune.deleted {
                        println!("Deleted VM image {image}, which no build used recently");
                    }
                    for error in &prune.errors {
                        eprintln!("Warning: could not delete unused VM image {error}");
                    }
                    if let Ok(mut report) = cache_report.lock() {
                        report.vm_images = Some(VmImageUsage {
                            count: prune.kept,
                            bytes: prune.kept_bytes,
                        });
                    }
                }
                Err(error) => eprintln!("Warning: could not prune VM images: {error:#}"),
            }
        }
    }
}

/// Records the finished build's lockfile keys, then measures the caches and
/// evicts above the cap. It finishes before the next claim, because eviction
/// must never delete from a cache a build is using.
async fn run_cache_maintenance(
    caches: Arc<RunnerCaches>,
    cache_report: SharedCacheReport,
    finished_build: Option<(CacheObservation, bool)>,
) {
    let maintenance = tokio::task::spawn_blocking(move || {
        if let Some((observation, succeeded)) = finished_build {
            caches.record(&observation, succeeded, now_unix());
        }
        let stats = caches.maintain(now_unix());
        if stats.evicted_bytes > 0 {
            println!(
                "Evicted {} of least recently used dependency caches",
                caches::format_bytes(stats.evicted_bytes)
            );
        }
        if let Ok(mut report) = cache_report.lock() {
            report.stats = Some(stats);
        }
    });
    if let Err(error) = maintenance.await {
        eprintln!("Warning: dependency cache maintenance stopped: {error}");
    }
}

async fn purge_caches_if_requested(caches: &Arc<RunnerCaches>, cache_report: &SharedCacheReport) {
    let Some(requested_at) = cache_report
        .lock()
        .ok()
        .and_then(|report| report.purge_requested_at)
    else {
        return;
    };
    if caches
        .purged_at()
        .is_some_and(|purged_at| purged_at >= requested_at)
    {
        return;
    }
    println!("Purging dependency caches at {}", caches.root().display());
    let purge_caches = caches.clone();
    match tokio::task::spawn_blocking(move || purge_caches.purge(requested_at, now_unix())).await {
        Ok(Ok(stats)) => {
            if let Ok(mut report) = cache_report.lock() {
                report.stats = Some(stats);
            }
            println!("Dependency caches purged");
        }
        Ok(Err(error)) => eprintln!("Warning: dependency cache purge failed: {error:#}"),
        Err(error) => eprintln!("Warning: dependency cache purge stopped: {error}"),
    }
}

async fn claim_and_execute(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    caches: Option<&Arc<RunnerCaches>>,
    cache_report: &SharedCacheReport,
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
        .timeout(CONTROL_REQUEST_TIMEOUT)
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

    report_status_with_retry(
        client,
        daemon_url,
        config,
        &StatusReport {
            build_id: job.build_id.as_str(),
            status: "running",
            exit_code: None,
            error_message: None,
            steps: &[],
        },
        RetryPolicy::within(RUNNING_REPORT_WINDOW),
    )
    .await?;

    let mut cache_observation = None;
    let (steps, result) = execute_build(
        &job,
        client,
        daemon_url,
        config,
        caches.map(Arc::as_ref),
        &mut cache_observation,
    )
    .await;
    let finished_build = cache_observation.map(|observation| (observation, result.is_ok()));

    let final_report = match &result {
        Ok(()) => Some(StatusReport {
            build_id: job.build_id.as_str(),
            status: "succeeded",
            exit_code: Some(0),
            error_message: None,
            steps: &steps,
        }),
        Err(e) => match e.downcast_ref::<BuildTerminated>() {
            Some(terminated) if terminated.status == CONTROLLER_UNAVAILABLE => Some(StatusReport {
                build_id: job.build_id.as_str(),
                status: "failed",
                exit_code: Some(1),
                error_message: Some(format!(
                    "The runner lost contact with the backend for more than {} minutes and stopped the build",
                    CONTROLLER_GRACE.as_secs() / 60
                )),
                steps: &steps,
            }),
            Some(_) => {
                println!(
                    "Build {} was externally terminated, skipping status report",
                    job.build_id
                );
                None
            }
            None => Some(StatusReport {
                build_id: job.build_id.as_str(),
                status: "failed",
                exit_code: Some(1),
                error_message: Some(e.to_string()),
                steps: &steps,
            }),
        },
    };
    let reported = match &final_report {
        Some(report) => {
            report_status_with_retry(
                client,
                daemon_url,
                config,
                report,
                RetryPolicy::within(FINAL_REPORT_WINDOW),
            )
            .await
        }
        None => Ok(()),
    };
    match (&final_report, &reported) {
        (Some(report), Ok(())) if report.status == "succeeded" => {
            println!("Build {} succeeded", job.build_id)
        }
        (Some(report), Ok(())) => eprintln!(
            "Build {} failed: {}",
            job.build_id,
            report.error_message.as_deref().unwrap_or_default()
        ),
        _ => {}
    }

    // After the status report, so measuring the caches never delays it, and
    // whatever its outcome.
    if let Some(caches) = caches {
        run_cache_maintenance(caches.clone(), cache_report.clone(), finished_build).await;
    }
    reported?;

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

/// Flutter version a build asks for when neither `.fvmrc` nor the pipeline pins one.
const DEFAULT_FLUTTER_VERSION: &str = "stable";

#[derive(Debug, Clone)]
struct ResolvedExecutionPlan {
    /// Named steps in run order: pre-build, then build, then post-build.
    steps: Vec<PlanStep>,
    artifact_patterns: Vec<String>,
    env: Vec<PipelineEnvVar>,
    /// `OORE_SECRET_<NAME>` variables backing `${{ secrets.NAME }}` in commands.
    secret_env: Vec<(String, String)>,
    /// Env keys whose values came from a secret.
    secret_env_keys: std::collections::HashSet<String>,
    build_number_offset: i64,
    isolation: BuildIsolation,
    /// Whether the build targets iOS or macOS. Only those need the Xcode VM
    /// image; Android-only builds use a base about a third its size.
    needs_xcode: bool,
    /// From `.fvmrc`, then the pipeline config, then `stable`. Picks the VM
    /// image; host builds check that the runner's Flutter matches it.
    flutter_version: String,
    source: String,
}

fn secret_env_var(name: &str) -> String {
    format!("OORE_SECRET_{name}")
}

fn lookup_secret<'a>(
    secrets: &'a std::collections::BTreeMap<String, String>,
    name: &str,
    path: &str,
) -> anyhow::Result<&'a str> {
    secrets.get(name).map(String::as_str).ok_or_else(|| {
        anyhow::anyhow!(
            "{path} references secret '{name}', which is not defined for this pipeline, its project or the instance"
        )
    })
}

/// Resolve `${{ secrets.NAME }}`. Env values receive the value itself. Commands
/// receive a quoted-safe `${OORE_SECRET_NAME}` shell reference instead, so a value
/// is never spliced into shell source.
fn apply_secret_references(
    mut plan: ResolvedExecutionPlan,
    secrets: &std::collections::BTreeMap<String, String>,
) -> anyhow::Result<ResolvedExecutionPlan> {
    for (index, pair) in plan.env.iter_mut().enumerate() {
        let path = format!("env[{index}].value");
        let mut missing = None;
        let resolved = replace_secret_references(&pair.value, |name| {
            match lookup_secret(secrets, name, &path) {
                Ok(value) => value.to_string(),
                Err(error) => {
                    missing.get_or_insert(error);
                    String::new()
                }
            }
        })
        .map_err(|error| anyhow::anyhow!("{path} {error}"))?;
        if let Some(error) = missing {
            return Err(error);
        }
        if resolved != pair.value {
            plan.secret_env_keys.insert(pair.key.clone());
            pair.value = resolved;
        }
    }

    let mut referenced = std::collections::BTreeSet::new();
    for step in &mut plan.steps {
        let path = step.source.clone();
        let mut names = Vec::new();
        step.run = replace_secret_references(&step.run, |name| {
            names.push(name.to_string());
            format!("${{{}}}", secret_env_var(name))
        })
        .map_err(|error| anyhow::anyhow!("{path} {error}"))?;
        for name in names {
            lookup_secret(secrets, &name, &path)?;
            referenced.insert(name);
        }
    }
    plan.secret_env = referenced
        .into_iter()
        .map(|name| {
            let value = secrets[&name].clone();
            (secret_env_var(&name), value)
        })
        .collect();
    Ok(plan)
}

/// Replaces secret values in log text with `***`.
#[derive(Clone, Default)]
struct SecretMasker {
    patterns: Arc<Vec<String>>,
}

impl SecretMasker {
    fn new<'a>(values: impl Iterator<Item = &'a str>) -> Self {
        let mut patterns = std::collections::BTreeSet::new();
        for value in values {
            if value.is_empty() {
                continue;
            }
            patterns.insert(value.to_string());
            // Multi-line values (JSON keys, PEM blocks) are printed line by line.
            if value.contains('\n') {
                for line in value.lines().map(str::trim) {
                    if line.len() >= 8 {
                        patterns.insert(line.to_string());
                    }
                }
            }
        }
        let mut patterns: Vec<String> = patterns.into_iter().collect();
        patterns.sort_by_key(|pattern| std::cmp::Reverse(pattern.len()));
        Self {
            patterns: Arc::new(patterns),
        }
    }

    fn mask(&self, text: &str) -> String {
        let mut masked = text.to_string();
        for pattern in self.patterns.iter() {
            if masked.contains(pattern.as_str()) {
                masked = masked.replace(pattern.as_str(), "***");
            }
        }
        masked
    }
}

/// Log sequence position plus the masker applied to every uploaded line.
#[derive(Default)]
struct BuildLogCursor {
    seq: i64,
    masker: SecretMasker,
}

async fn fetch_job_secrets(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    signing_token: &str,
) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    let resp = client
        .get(format!(
            "{}/v1/runners/{}/jobs/{}/secrets",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .header("x-oore-signing-token", signing_token)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("secret lookup failed: {}", resp.status());
    }

    let payload: RunnerJobSecretsResponse = resp.json().await?;
    Ok(payload
        .secrets
        .into_iter()
        .map(|secret| (secret.name, secret.value))
        .collect())
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

/// The version name from `pubspec.yaml` (`version: 1.4.0+12` gives `1.4.0`),
/// when the workspace root has one.
fn read_pubspec_version_name(workspace: &Path) -> Option<String> {
    let raw = fs::read_to_string(workspace.join("pubspec.yaml")).ok()?;
    pubspec_version_name(&raw)
}

fn pubspec_version_name(raw: &str) -> Option<String> {
    let value = raw.lines().find_map(|line| line.strip_prefix("version:"))?;
    let value = value.split(" #").next()?.trim().trim_matches(['"', '\'']);
    let name = value.split('+').next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
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
    validate_build_number_offset(config.build_number_offset)
        .map_err(|error| anyhow::anyhow!("build_number_offset {error}"))?;
    let runs_on = normalize_runner_labels(&config.runs_on)
        .map_err(|error| anyhow::anyhow!("runs_on {error}"))?;

    Ok(PipelineExecutionConfig {
        platforms: config.platforms,
        flutter_version,
        commands,
        platform_build_args,
        platform_commands,
        env,
        artifact_patterns,
        build_number_offset: config.build_number_offset,
        runs_on,
        isolation: config.isolation,
    })
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

/// The run's platform selection from the snapshot, when it made one.
fn selected_platforms(snapshot: &serde_json::Value) -> anyhow::Result<Option<Vec<BuildPlatform>>> {
    match snapshot.get("selected_platforms") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(raw) => serde_json::from_value(raw.clone())
            .map(Some)
            .map_err(|error| anyhow::anyhow!("Invalid selected_platforms in snapshot: {error}")),
    }
}

/// The scheduler matched this runner against the pipeline's `runs_on`. A
/// repository file can ask for more, so stop before any build command runs on
/// a runner that lacks those labels.
fn require_runner_labels(
    runs_on: &[String],
    snapshot: &serde_json::Value,
    config_path: &str,
) -> anyhow::Result<()> {
    let Some(runner_labels) = snapshot.get("runner_labels").and_then(|v| v.as_array()) else {
        return Ok(());
    };
    let runner_labels: Vec<String> = runner_labels
        .iter()
        .filter_map(|label| label.as_str().map(str::to_string))
        .collect();
    let missing = missing_runner_labels(runs_on, &runner_labels);
    if !missing.is_empty() {
        anyhow::bail!(
            "{config_path} sets runs_on [{}], but this runner lacks [{}]. Set the same runs_on on the pipeline so the scheduler picks a matching runner",
            runs_on.join(", "),
            missing.join(", ")
        );
    }
    Ok(())
}

fn resolve_execution_plan(
    workspace: &Path,
    snapshot: &serde_json::Value,
    build_play_bundle: bool,
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
        let file_config = parse_repository_config(&content).map_err(|e| {
            anyhow::anyhow!("Invalid pipeline config in {}: {}", full_path.display(), e)
        })?;
        require_runner_labels(file_config.runs_on(), snapshot, rel_path)?;
        let selected = selected_platforms(snapshot)?;
        let plan = file_config
            .plan(PlanRequest {
                // The pipeline's v2 workflow, when it names one.
                workflow: snapshot.get("workflow").and_then(|v| v.as_str()),
                selected_platforms: selected.as_deref(),
                build_play_bundle,
            })
            .map_err(|e| anyhow::anyhow!("{rel_path}: {e}"))?;
        let resolved_flutter_version = fvmrc_version
            .or_else(|| plan.flutter_version.clone())
            .unwrap_or_else(|| DEFAULT_FLUTTER_VERSION.to_string());

        return Ok(ResolvedExecutionPlan {
            needs_xcode: needs_xcode(&plan.platforms),
            steps: plan.steps,
            artifact_patterns: plan.artifact_patterns,
            env: plan.env,
            secret_env: Vec::new(),
            secret_env_keys: Default::default(),
            build_number_offset: plan.build_number_offset,
            isolation: plan.isolation,
            flutter_version: resolved_flutter_version,
            source: format!("file:{}", full_path.display()),
        });
    }

    let mut fallback = load_ui_execution_config(snapshot)?;
    if let Some(selected) = selected_platforms(snapshot)? {
        fallback = select_v1_platforms(fallback, &selected).map_err(anyhow::Error::msg)?;
    }
    let resolved_flutter_version = fvmrc_version
        .or_else(|| fallback.flutter_version.clone())
        .unwrap_or_else(|| DEFAULT_FLUTTER_VERSION.to_string());
    let plan = plan_execution_config(&fallback, true, build_play_bundle);
    Ok(ResolvedExecutionPlan {
        needs_xcode: needs_xcode(&plan.platforms),
        steps: plan.steps,
        artifact_patterns: plan.artifact_patterns,
        env: plan.env,
        secret_env: Vec::new(),
        secret_env_keys: Default::default(),
        build_number_offset: plan.build_number_offset,
        isolation: plan.isolation,
        flutter_version: resolved_flutter_version,
        source: "ui_fallback".to_string(),
    })
}

/// What the runner last heard from the backend about the build it runs.
struct BuildAuthorityState {
    last_confirmed: Mutex<Instant>,
    grace: Duration,
    /// Why the build must stop, once the runner knows.
    terminated: Mutex<Option<String>>,
}

impl Default for BuildAuthorityState {
    fn default() -> Self {
        Self::with_grace(CONTROLLER_GRACE)
    }
}

impl BuildAuthorityState {
    fn with_grace(grace: Duration) -> Self {
        Self {
            last_confirmed: Mutex::new(Instant::now()),
            grace,
            terminated: Mutex::new(None),
        }
    }

    fn confirmed_active(&self) {
        if let Ok(mut last) = self.last_confirmed.lock() {
            *last = Instant::now();
        }
    }

    /// The backend could not answer. The build carries on until the backend
    /// has been unreachable for the whole grace window.
    fn transient_failure(&self) -> anyhow::Result<()> {
        let elapsed = self
            .last_confirmed
            .lock()
            .map(|last| last.elapsed())
            .unwrap_or(Duration::MAX);
        if elapsed >= self.grace {
            return Err(BuildTerminated {
                status: CONTROLLER_UNAVAILABLE.to_string(),
            }
            .into());
        }
        Ok(())
    }

    fn record(&self, result: &anyhow::Result<()>) {
        if let Err(error) = result
            && let Some(terminated) = error.downcast_ref::<BuildTerminated>()
            && let Ok(mut reason) = self.terminated.lock()
        {
            reason.get_or_insert_with(|| terminated.status.clone());
        }
    }

    /// The error for a step stopped by `poll_cancellation`.
    fn terminated(&self) -> anyhow::Error {
        let status = self
            .terminated
            .lock()
            .ok()
            .and_then(|reason| reason.clone())
            .unwrap_or_else(|| "canceled".to_string());
        BuildTerminated { status }.into()
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
        .timeout(CONTROL_REQUEST_TIMEOUT)
        .send()
        .await;

    let result = match resp {
        Ok(r) if r.status().is_success() => match r.json::<JobStatusResponse>().await {
            Ok(status_resp) => {
                let status: BuildStatus = status_resp
                    .status
                    .parse()
                    .map_err(|e: String| anyhow::anyhow!(e))?;
                if status.is_terminal() {
                    Err(BuildTerminated {
                        status: status_resp.status,
                    }
                    .into())
                } else {
                    authority.confirmed_active();
                    Ok(())
                }
            }
            // A body cut off mid-read is the connection's fault, not the build's.
            Err(_) => authority.transient_failure(),
        },
        Ok(response) if is_transient_authority_status(response.status()) => {
            authority.transient_failure()
        }
        Ok(response) => Err(authority_loss(response.status())),
        Err(_) => authority.transient_failure(),
    };
    authority.record(&result);
    result
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

fn needs_xcode(platforms: &[BuildPlatform]) -> bool {
    platforms
        .iter()
        .any(|platform| matches!(platform, BuildPlatform::Ios | BuildPlatform::Macos))
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
    caches: Option<&RunnerCaches>,
    cache_observation: &mut Option<CacheObservation>,
) -> (Vec<StepResult>, anyhow::Result<()>) {
    let mut log_seq = BuildLogCursor::default();
    let mut build_vm = None;
    let (steps, result) = execute_build_steps(
        job,
        client,
        daemon_url,
        config,
        caches,
        cache_observation,
        &mut log_seq,
        &mut build_vm,
    )
    .await;
    // A build that ended before its last step still has its VM. A canceled
    // or timed-out build may have stopped tools mid-write, so its caches are
    // not saved.
    if let Some(vm) = build_vm.take() {
        let terminated = result
            .as_ref()
            .err()
            .is_some_and(|error| error.downcast_ref::<BuildTerminated>().is_some());
        shutdown_build_vm(
            vm,
            !terminated,
            client,
            daemon_url,
            config,
            &job.build_id,
            &mut log_seq,
        )
        .await;
    }
    // Error text reaches the build record and the runner log, so mask it too.
    let result = result.map_err(|error| {
        if error.downcast_ref::<BuildTerminated>().is_some() {
            error
        } else {
            anyhow::anyhow!(log_seq.masker.mask(&format!("{error:#}")))
        }
    });
    (steps, result)
}

/// Where a reused artifact goes in the workspace: where a build leaves it, so
/// the uploads find it the same way. `None` for an artifact no upload reads.
fn retry_artifact_path(name: &str, artifact_type: &str) -> Option<String> {
    if name.is_empty()
        || name.starts_with('.')
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        return None;
    }
    if name.to_ascii_lowercase().ends_with(".aab") {
        Some(format!("build/app/outputs/bundle/release/{name}"))
    } else if name == "mapping.txt" {
        Some("build/app/outputs/mapping/release/mapping.txt".to_string())
    } else if artifact_type == "ipa" {
        Some(format!("build/ios/ipa/{name}"))
    } else {
        None
    }
}

/// Downloads a stored artifact to `dest` and checks it against the checksum
/// recorded when it was uploaded.
async fn download_retry_artifact(
    client: &reqwest::Client,
    daemon_url: &str,
    download_url: &str,
    dest: &Path,
    checksum: Option<&str>,
) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;

    // Local storage hands out daemon-relative URLs.
    let url = if download_url.starts_with('/') {
        format!("{}{download_url}", daemon_url.trim_end_matches('/'))
    } else {
        download_url.to_string()
    };
    let mut resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("download failed (HTTP {})", resp.status());
    }
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::File::create(dest).await?;
    let mut hasher = Sha256::new();
    while let Some(chunk) = resp.chunk().await? {
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    let actual = hex::encode(hasher.finalize());
    if let Some(expected) = checksum
        && !expected.eq_ignore_ascii_case(&actual)
    {
        anyhow::bail!("checksum mismatch: expected {expected}, got {actual}");
    }
    Ok(())
}

async fn fetch_job_retry_artifacts(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    signing_token: &str,
) -> anyhow::Result<RunnerRetryArtifactsResponse> {
    let resp = client
        .get(format!(
            "{}/v1/runners/{}/jobs/{}/retry-artifacts",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .header("x-oore-signing-token", signing_token)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("artifact lookup failed: {}", resp.status());
    }
    Ok(resp.json().await?)
}

/// The retry job's private directories.
struct RetryWorkspace<'a> {
    workspace: &'a Path,
    signing_workspace: &'a Path,
    authority: &'a Arc<BuildAuthorityState>,
}

/// Runs only the retried store uploads, with the source build's stored
/// artifacts instead of a build. The artifacts are stored again as this
/// build's own, so its uploads and releases point at what it published.
#[allow(clippy::too_many_arguments)]
async fn execute_step_retry(
    job: &ClaimedJob,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    log_seq: &mut BuildLogCursor,
    dirs: RetryWorkspace<'_>,
    retry_steps: &[String],
    steps: &mut Vec<StepResult>,
) -> anyhow::Result<()> {
    let RetryWorkspace {
        workspace,
        signing_workspace,
        authority,
    } = dirs;
    let secrets = fetch_job_secrets(
        client,
        daemon_url,
        config,
        &job.build_id,
        &job.signing_token,
    )
    .await
    .map_err(|error| anyhow::anyhow!("Failed to load secrets for this build: {error}"))?;
    log_seq.masker = SecretMasker::new(secrets.values().map(String::as_str));
    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
        "stdout",
        &format!(
            "[oore-retry] Retrying {} with the source build's stored artifacts. Build steps do not run again.",
            retry_steps.join(", ")
        ),
    )
    .await;

    let publish_plan = fetch_job_publish_plan(
        client,
        daemon_url,
        config,
        &job.build_id,
        &job.signing_token,
    )
    .await
    .map_err(|error| anyhow::anyhow!("Failed to load publish settings for this build: {error}"))?;
    for step in retry_steps {
        let enabled = match step.as_str() {
            PUBLISH_PLAY_STEP => publish_plan.google_play.is_some(),
            PUBLISH_APP_STORE_STEP => publish_plan.app_store.is_some(),
            _ => false,
        };
        if !enabled {
            anyhow::bail!(
                "{step} cannot run: publishing to that store is no longer turned on for this pipeline"
            );
        }
    }

    // Release notes kept in the repository are read from the build's commit.
    let needs_source = retry_steps.iter().any(|step| step == PUBLISH_PLAY_STEP)
        && publish_plan.google_play.as_ref().is_some_and(|play| {
            play.release_notes == oore_contract::GooglePlayReleaseNotesSource::RepoFiles
        });
    if needs_source {
        run_checkout(
            job, client, daemon_url, config, log_seq, workspace, authority, steps,
        )
        .await?;
    }

    let step_name = "artifacts";
    let started = now_unix();
    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
        "stdout",
        &step_start_marker(step_name, "Reuse the source build's stored artifacts"),
    )
    .await;
    let mut ipa: Option<(PathBuf, serde_json::Value)> = None;
    let restore = async {
        let listing = fetch_job_retry_artifacts(
            client,
            daemon_url,
            config,
            &job.build_id,
            &job.signing_token,
        )
        .await?;
        let mut restored = Vec::new();
        for artifact in listing.artifacts {
            let Some(relative) = retry_artifact_path(&artifact.name, &artifact.artifact_type)
            else {
                continue;
            };
            let dest = workspace.join(&relative);
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                log_seq,
                "stdout",
                &format!("[oore-retry] Downloading {}", artifact.name),
            )
            .await;
            download_retry_artifact(
                client,
                daemon_url,
                &artifact.download_url,
                &dest,
                artifact.checksum.as_deref(),
            )
            .await
            .with_context(|| format!("failed to download {}", artifact.name))?;
            if artifact.artifact_type == "ipa" {
                ipa = Some((dest, artifact.metadata));
            }
            restored.push(relative);
        }
        if restored.is_empty() {
            anyhow::bail!("the source build has no stored artifacts these uploads can use");
        }
        scan_and_upload_artifacts(
            workspace,
            client,
            daemon_url,
            config,
            &job.build_id,
            &restored,
            ipa.as_ref().map(|(_, metadata)| metadata),
        )
        .await
    };
    let restore: anyhow::Result<()> = tokio::select! {
        result = restore => result,
        _ = poll_cancellation(client, daemon_url, config, &job.build_id, authority.clone()) => {
            Err(authority.terminated())
        }
    };
    let finished = now_unix();
    let succeeded = restore.is_ok();
    steps.push(StepResult {
        name: step_name.to_string(),
        status: if succeeded { "succeeded" } else { "failed" }.to_string(),
        exit_code: Some(if succeeded { 0 } else { 1 }),
        started_at: started,
        finished_at: finished,
        duration_ms: (finished - started) * 1000,
    });
    if let Err(error) = &restore {
        let _ = append_runner_log_line(
            client,
            daemon_url,
            config,
            &job.build_id,
            log_seq,
            "stderr",
            &format!("[oore-retry] {error:#}"),
        )
        .await;
    }
    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
        "stdout",
        &step_end_marker(
            step_name,
            if succeeded { "succeeded" } else { "failed" },
            Some(if succeeded { 0 } else { 1 }),
        ),
    )
    .await;
    restore?;

    run_publish_steps(
        job,
        client,
        daemon_url,
        config,
        log_seq,
        PublishInputs {
            workspace,
            signing_workspace,
            secrets: &secrets,
            signed_ipa_path: ipa.as_ref().map(|(path, _)| path.as_path()),
            ios_artifact_metadata: ipa.as_ref().map(|(_, metadata)| metadata),
            only: Some(retry_steps),
            authority,
        },
        steps,
    )
    .await
}

/// Clones the build's commit into `workspace` as the `checkout` step.
#[allow(clippy::too_many_arguments)]
async fn run_checkout(
    job: &ClaimedJob,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    log_seq: &mut BuildLogCursor,
    workspace: &Path,
    authority: &Arc<BuildAuthorityState>,
    steps: &mut Vec<StepResult>,
) -> anyhow::Result<()> {
    let snapshot = &job.config_snapshot;
    let repo_url = snapshot
        .get("repo_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if repo_url.is_empty() {
        anyhow::bail!("Build config snapshot has no repo_url — cannot checkout source");
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
        Err(e) => return Err(e),
    };
    if proxy_path.is_some() {
        add_checkout_proxy_config(&mut checkout, &effective_repo_url, &config.runner_token);
    }

    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
        "stdout",
        &step_start_marker("checkout", &checkout.preview_command),
    )
    .await;
    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
        "stdout",
        &format!("$ {}", checkout.preview_command),
    )
    .await;

    let mut checkout_child = repository_shell_command(&checkout.shell_script, workspace);
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
        Err(e) => return Err(e.into()),
    };

    let clone_status = run_and_stream(
        child,
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
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
                log_seq,
                "stdout",
                &step_end_marker("checkout", "canceled", None),
            )
            .await;
            return Err(authority.terminated());
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
                log_seq,
                "stdout",
                &step_end_marker(
                    "checkout",
                    if success { "succeeded" } else { "failed" },
                    exit_code,
                ),
            )
            .await;
            if !success {
                anyhow::bail!("Git checkout failed");
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn execute_build_steps(
    job: &ClaimedJob,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    caches: Option<&RunnerCaches>,
    cache_observation: &mut Option<CacheObservation>,
    log_seq: &mut BuildLogCursor,
    build_vm: &mut Option<BuildVm>,
) -> (Vec<StepResult>, anyhow::Result<()>) {
    // Pin signer executables before any repository-controlled checkout or build stage runs.
    let android_signing_toolchain = AndroidSigningToolchain::discover();
    let runner_workspace_root = match prepare_runner_workspace_root() {
        Ok(root) => root,
        Err(error) => return (vec![], Err(error)),
    };
    let workspace = match create_project_workspace_in(
        &runner_workspace_root,
        &config.runner_id,
        &job.project_id,
    ) {
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
    let mut ios_signing_source: Option<&str> = None;
    let mut ios_signing_bundle: Option<RunnerIosSigningBundle> = None;
    let ios_signing_checked_before_checkout = snapshot_requests_ios(snapshot);

    if let Err(e) = check_build_active(client, daemon_url, config, &job.build_id, &authority).await
    {
        return (steps, Err(e));
    }

    if let Some(retry_steps) = job.retry_steps.as_deref() {
        let result = execute_step_retry(
            job,
            client,
            daemon_url,
            config,
            log_seq,
            RetryWorkspace {
                workspace: &workspace,
                signing_workspace: &signing_workspace,
                authority: &authority,
            },
            retry_steps,
            &mut steps,
        )
        .await;
        return (steps, result);
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

    let secrets = match fetch_job_secrets(
        client,
        daemon_url,
        config,
        &job.build_id,
        &job.signing_token,
    )
    .await
    {
        Ok(secrets) => secrets,
        Err(error) => {
            return (
                steps,
                Err(anyhow::anyhow!(
                    "Failed to load secrets for this build: {error}"
                )),
            );
        }
    };
    log_seq.masker = SecretMasker::new(secrets.values().map(String::as_str));

    if let Err(error) = run_checkout(
        job, client, daemon_url, config, log_seq, &workspace, &authority, &mut steps,
    )
    .await
    {
        return (steps, Err(error));
    }

    if let Some(caches) = caches {
        let observation = caches.observe(&workspace);
        for line in &observation.lines {
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                log_seq,
                "stdout",
                line,
            )
            .await;
        }
        *cache_observation = Some(observation);
    }

    // A failed lookup here is not fatal: the publish step reloads the plan
    // and reports the error where the user looks for it.
    let build_play_bundle = fetch_job_publish_plan(
        client,
        daemon_url,
        config,
        &job.build_id,
        &job.signing_token,
    )
    .await
    .is_ok_and(|plan| plan.google_play.is_some());
    let execution_plan = match resolve_execution_plan(&workspace, snapshot, build_play_bundle)
        .and_then(|plan| apply_secret_references(plan, &secrets))
    {
        Ok(plan) => plan,
        Err(e) => return (steps, Err(e)),
    };
    let version_code =
        match store_version_code(job.build_number, execution_plan.build_number_offset) {
            Ok(code) => code,
            Err(error) => {
                return (
                    steps,
                    Err(anyhow::anyhow!("Invalid store version code: {error}")),
                );
            }
        };
    let version_name = read_pubspec_version_name(&workspace);

    let mut signing_source: Option<&str> = None;
    let mut signing_variant: Option<AndroidSigningBuildType> = None;
    let mut signing_inputs: Option<AndroidSigningInputs> = None;
    let build_commands: Vec<String> = execution_plan
        .steps
        .iter()
        .filter(|step| step.phase == StepPhase::Build)
        .map(|step| step.run.clone())
        .collect();
    match determine_android_signing_variant(&build_commands) {
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
        ("OORE_VERSION_CODE".to_string(), version_code.to_string()),
    ];
    // Before pipeline env, so a pipeline can still point a tool elsewhere.
    if let Some(caches) = caches {
        step_env.extend(caches.step_env());
    }
    println!(
        "Store version code {version_code} (build {} + offset {})",
        job.build_number, execution_plan.build_number_offset
    );
    if let Some(name) = &version_name {
        step_env.push(("OORE_VERSION_NAME".to_string(), name.clone()));
    }
    if let Some(branch) = &job.branch {
        step_env.push(("BRANCH".to_string(), branch.clone()));
    }
    if let Some(commit_sha) = &job.commit_sha {
        step_env.push(("COMMIT_SHA".to_string(), commit_sha.clone()));
    }
    for pair in &execution_plan.env {
        step_env.push((pair.key.clone(), pair.value.clone()));
    }
    for (key, value) in &execution_plan.secret_env {
        step_env.push((key.clone(), value.clone()));
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
            log_seq,
            "stdout",
            &android_signing_prepared_marker(source, variant),
        )
        .await;
    }

    // Flutter expects dart-defines to be materialized in a file. When pipeline
    // env vars are provided, create a temporary .env and pass it to flutter build.
    // Secret-backed values stay out of the define file so they are never
    // compiled into the app; commands can still pass them explicitly.
    let dart_defines: Vec<&PipelineEnvVar> = execution_plan
        .env
        .iter()
        .filter(|pair| !execution_plan.secret_env_keys.contains(&pair.key))
        .collect();
    let dart_define_file = if dart_defines.is_empty() {
        None
    } else {
        let mut content = String::new();
        for pair in dart_defines {
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

    let vm = match execution_plan.isolation {
        BuildIsolation::Host => {
            let started = now_unix();
            let found = host_flutter_version(&workspace, &step_env).await;
            let check = check_host_flutter(&execution_plan.flutter_version, found);
            let message = match &check {
                Ok(message) | Err(message) => message.clone(),
            };
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                log_seq,
                if check.is_ok() { "stdout" } else { "stderr" },
                &message,
            )
            .await;
            if check.is_err() {
                let finished = now_unix();
                steps.push(StepResult {
                    name: "flutter-version".to_string(),
                    status: "failed".to_string(),
                    exit_code: Some(1),
                    started_at: started,
                    finished_at: finished,
                    duration_ms: (finished - started) * 1000,
                });
                return (steps, Err(anyhow::anyhow!(message)));
            }
            None
        }
        BuildIsolation::Vm => {
            let started = now_unix();
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                log_seq,
                "stdout",
                &step_start_marker("vm-start", "Start a clean Tart VM for this build"),
            )
            .await;
            let shares = caches
                .map(|caches| {
                    caches
                        .mounts()
                        .into_iter()
                        .map(|mount| tart::HostShare {
                            name: mount.name,
                            env: mount.env,
                            // Every VM build runs from the same guest path, so
                            // Xcode names DerivedData the same for every
                            // project; keep one saved copy per project.
                            host_dir: if mount.name == "derived-data" {
                                mount.host_dir.join(hex::encode(
                                    &Sha256::digest(job.project_id.as_bytes())[..16],
                                ))
                            } else {
                                mount.host_dir
                            },
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let result = start_build_vm(
                job,
                client,
                daemon_url,
                config,
                log_seq,
                &authority,
                &workspace,
                &shares,
                &execution_plan.flutter_version,
                execution_plan.needs_xcode,
            )
            .await;
            let finished = now_unix();
            let succeeded = result.is_ok();
            steps.push(StepResult {
                name: "vm-start".to_string(),
                status: if succeeded { "succeeded" } else { "failed" }.to_string(),
                exit_code: Some(if succeeded { 0 } else { 1 }),
                started_at: started,
                finished_at: finished,
                duration_ms: (finished - started) * 1000,
            });
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                log_seq,
                "stdout",
                &step_end_marker(
                    "vm-start",
                    if succeeded { "succeeded" } else { "failed" },
                    Some(if succeeded { 0 } else { 1 }),
                ),
            )
            .await;
            match result {
                Ok(vm) => {
                    // Point the cache variables at the guest mounts, leaving
                    // any value a pipeline set for itself alone.
                    if let Some(caches) = caches {
                        let host_env = caches.step_env();
                        for (key, value) in step_env.iter_mut() {
                            let is_host_cache =
                                host_env.iter().any(|(k, v)| k == key && v == value);
                            if let Some((_, guest)) =
                                vm.vm.guest_env().iter().find(|(k, _)| k == key)
                                && is_host_cache
                            {
                                *value = guest.clone();
                            }
                        }
                    }
                    Some(vm)
                }
                Err(error) => return (steps, Err(error)),
            }
        }
    };

    // The caller ends the VM if a step returns early.
    *build_vm = vm;
    let vm = build_vm.as_ref();

    // A VM build's steps write reports in the guest workspace; the copy back
    // after each step brings them here.
    let test_reports_dir = workspace.join(test_reports::TEST_REPORTS_SUBDIR);
    if let Err(error) = fs::create_dir_all(&test_reports_dir) {
        return (
            steps,
            Err(anyhow::anyhow!(
                "Failed to create the test reports directory: {error}"
            )),
        );
    }
    let guest_workspace = vm.as_ref().map(|vm| PathBuf::from(vm.vm.guest_workspace()));
    let step_test_reports_dir = guest_workspace
        .as_deref()
        .unwrap_or(&workspace)
        .join(test_reports::TEST_REPORTS_SUBDIR);
    // The guest workspace was copied in before the host directory existed.
    if let Some(vm) = vm
        && let Err(error) = vm
            .vm
            .reset_guest_dir(&step_test_reports_dir.display().to_string())
            .await
    {
        return (
            steps,
            Err(error.context("Failed to create the test reports directory in the VM")),
        );
    }
    step_env.push((
        test_reports::TEST_REPORTS_DIR_ENV.to_string(),
        step_test_reports_dir.display().to_string(),
    ));
    let mut report_path_prefixes = vec![workspace.clone()];
    report_path_prefixes.extend(guest_workspace);

    let mut ios_signing_command_applied = false;
    let ios_signing_expected = ios_signing_bundle.is_some();
    let mut ios_artifact_metadata: Option<serde_json::Value> = None;
    let mut signed_ipa_path: Option<PathBuf> = None;
    for phase in StepPhase::ALL {
        let stage_name = phase.as_str();
        let phase_steps: Vec<PlanStep> = execution_plan
            .steps
            .iter()
            .filter(|step| step.phase == phase)
            .cloned()
            .collect();
        for plan_step in &phase_steps {
            let command = &plan_step.run;
            if let Err(e) =
                check_build_active(client, daemon_url, config, &job.build_id, &authority).await
            {
                return (steps, Err(e));
            }

            let step_name = plan_step.name.clone();
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
            let normalized_command = test_reports::with_test_file_reporter(
                &normalized_command,
                &step_test_reports_dir.join(format!("{}.json", report_file_stem(&step_name))),
            );
            let command_preview = render_command_preview(&normalized_command, &step_env);

            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                log_seq,
                "stdout",
                &step_start_marker(&step_name, &normalized_command),
            )
            .await;
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                log_seq,
                "stdout",
                &format!("$ {command_preview}"),
            )
            .await;
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                log_seq,
                "stdout",
                &render_step_env_preview(&step_env),
            )
            .await;

            let spawned = match &vm {
                Some(vm) => vm.vm.spawn_step(&normalized_command, &step_env).await,
                None => {
                    let mut step_cmd = repository_shell_command(&normalized_command, &workspace);
                    step_cmd
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .kill_on_drop(true);
                    for (key, value) in &step_env {
                        step_cmd.env(key, value);
                    }
                    scrub_managed_runner_env(&mut step_cmd);
                    step_cmd.spawn().map_err(anyhow::Error::from)
                }
            };
            let child = match spawned {
                Ok(c) => c,
                Err(e) => return (steps, Err(e)),
            };

            let step_status = run_and_stream(
                child,
                client,
                daemon_url,
                config,
                &job.build_id,
                log_seq,
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
                        log_seq,
                        "stdout",
                        &step_end_marker(&plan_step.name, "canceled", None),
                    )
                    .await;
                    return (steps, Err(authority.terminated()));
                }
                Some(status) => {
                    let exit_code = status.code().unwrap_or(-1);
                    // The host collects test reports, archives and artifacts
                    // from its workspace, so bring the guest's changes over
                    // before anything reads it.
                    let sync_error = match &vm {
                        Some(vm) => vm.vm.sync_workspace_out().await.err(),
                        None => None,
                    };
                    if let Some(report) = plan_step.test_report.as_deref()
                        && let Err(error) = copy_declared_test_report(
                            &workspace,
                            report,
                            &test_reports_dir,
                            &step_name,
                        )
                    {
                        let _ = append_runner_log_line(
                            client,
                            daemon_url,
                            config,
                            &job.build_id,
                            log_seq,
                            "stderr",
                            &format!("[oore-tests] {error}"),
                        )
                        .await;
                    }
                    report_step_tests(
                        client,
                        daemon_url,
                        config,
                        &job.build_id,
                        log_seq,
                        &test_reports_dir,
                        &report_path_prefixes,
                        &step_name,
                    )
                    .await;
                    // The host removed the reports it read; remove the guest's
                    // too, or the next step's copy back reports them again.
                    if let Some(vm) = vm
                        && let Err(error) = vm
                            .vm
                            .reset_guest_dir(&step_test_reports_dir.display().to_string())
                            .await
                    {
                        let _ = append_runner_log_line(
                            client,
                            daemon_url,
                            config,
                            &job.build_id,
                            log_seq,
                            "stderr",
                            &format!(
                                "[oore-tests] Could not clear the VM's test reports: {error:#}"
                            ),
                        )
                        .await;
                    }
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
                    let step_name = plan_step.name.clone();
                    let _ = append_runner_log_line(
                        client,
                        daemon_url,
                        config,
                        &job.build_id,
                        log_seq,
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
                        let err = if exit_code == 127 && vm.is_some() {
                            anyhow::anyhow!(
                                "Step failed with exit code 127 (command not found). \
The VM image for Flutter {} does not provide this command. Command: {}",
                                execution_plan.flutter_version,
                                command_preview
                            )
                        } else if exit_code == 127 {
                            anyhow::anyhow!(
                                "Step failed with exit code 127 (command not found). \
Install the required tooling on the runner, set isolation: vm, or override build commands. Command: {}",
                                command_preview
                            )
                        } else {
                            anyhow::anyhow!("Step failed with exit code {}", exit_code)
                        };
                        return (steps, Err(err));
                    }
                    if let Some(error) = sync_error {
                        return (steps, Err(error));
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
                            log_seq,
                            "stdout",
                            &step_start_marker(
                                signing_step_name,
                                "Sign Android artifact with managed credentials",
                            ),
                        )
                        .await;
                        let signing_result = match &vm {
                            Some(vm) => {
                                sign_android_artifacts_in_vm(&vm.vm, &workspace, command, inputs)
                                    .await
                            }
                            None => sign_android_artifacts(
                                &workspace,
                                &signing_workspace,
                                command,
                                inputs,
                                &android_signing_toolchain,
                            ),
                        };
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
                                    log_seq,
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
                                    log_seq,
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
                            log_seq,
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
                            log_seq,
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
                            log_seq,
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
                                    log_seq,
                                    "stdout",
                                    &format!(
                                        "[oore-signing] Signed IPA created at {}",
                                        signed_archive.ipa_path.display()
                                    ),
                                )
                                .await;
                                signed_ipa_path = Some(signed_archive.ipa_path.clone());
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
                                    log_seq,
                                    "stderr",
                                    &format!("[oore-signing] {error:#}"),
                                )
                                .await;
                                let _ = append_runner_log_line(
                                    client,
                                    daemon_url,
                                    config,
                                    &job.build_id,
                                    log_seq,
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
                            log_seq,
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

    // Every step has run; free the VM slot before the upload.
    if let Some(vm) = build_vm.take() {
        shutdown_build_vm(vm, true, client, daemon_url, config, &job.build_id, log_seq).await;
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

    // Publishing runs only after every build, signing and artifact step
    // succeeded; any earlier failure has already returned.
    let result = run_publish_steps(
        job,
        client,
        daemon_url,
        config,
        log_seq,
        PublishInputs {
            workspace: &workspace,
            signing_workspace: &signing_workspace,
            secrets: &secrets,
            signed_ipa_path: signed_ipa_path.as_deref(),
            ios_artifact_metadata: ios_artifact_metadata.as_ref(),
            only: None,
            authority: &authority,
        },
        &mut steps,
    )
    .await;
    (steps, result)
}

/// What the store uploads read: the build's outputs and the job's secrets.
struct PublishInputs<'a> {
    workspace: &'a Path,
    signing_workspace: &'a Path,
    secrets: &'a std::collections::BTreeMap<String, String>,
    signed_ipa_path: Option<&'a Path>,
    ios_artifact_metadata: Option<&'a serde_json::Value>,
    /// Runs only these upload steps, for a step retry. `None` runs every
    /// upload the pipeline turns on.
    only: Option<&'a [String]>,
    authority: &'a Arc<BuildAuthorityState>,
}

/// Runs the store uploads the pipeline turns on, stopping at the first failure.
async fn run_publish_steps(
    job: &ClaimedJob,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    log_seq: &mut BuildLogCursor,
    inputs: PublishInputs<'_>,
    steps: &mut Vec<StepResult>,
) -> anyhow::Result<()> {
    let PublishInputs {
        workspace,
        signing_workspace,
        secrets,
        signed_ipa_path,
        ios_artifact_metadata,
        only,
        authority,
    } = inputs;
    let runs = |step: &str| only.is_none_or(|only| only.iter().any(|name| name == step));
    let publish_plan = match fetch_job_publish_plan(
        client,
        daemon_url,
        config,
        &job.build_id,
        &job.signing_token,
    )
    .await
    {
        Ok(plan) => plan,
        Err(error) => {
            anyhow::bail!("Failed to load publish settings for this build: {error}");
        }
    };
    if let Some(play) = publish_plan
        .google_play
        .as_ref()
        .filter(|_| runs(PUBLISH_PLAY_STEP))
    {
        let step_name = PUBLISH_PLAY_STEP;
        let started = now_unix();
        let _ = append_runner_log_line(
            client,
            daemon_url,
            config,
            &job.build_id,
            log_seq,
            "stdout",
            &step_start_marker(step_name, "Upload the App Bundle to Google Play"),
        )
        .await;
        let mut lines = Vec::new();
        let result = publish::publish_to_google_play(
            client,
            workspace,
            secrets,
            publish_plan.changelog.as_deref(),
            play,
            &mut lines,
            publish::PublishGate {
                stopped: async {
                    tokio::select! {
                        _ = poll_cancellation(client, daemon_url, config, &job.build_id, authority.clone()) => {
                            authority.terminated()
                        }
                        _ = tokio::time::sleep(PLAY_PUBLISH_TIMEOUT) => anyhow::anyhow!(
                            "Google Play upload timed out after {} minutes",
                            PLAY_PUBLISH_TIMEOUT.as_secs() / 60
                        ),
                    }
                },
                still_active: check_build_active(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    authority,
                ),
            },
        )
        .await;
        for line in lines {
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                log_seq,
                "stdout",
                &line,
            )
            .await;
        }
        let finished = now_unix();
        steps.push(StepResult {
            name: step_name.to_string(),
            status: if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            }
            .to_string(),
            exit_code: Some(if result.is_ok() { 0 } else { 1 }),
            started_at: started,
            finished_at: finished,
            duration_ms: (finished - started) * 1000,
        });
        match result {
            Ok(published) => {
                if let Err(error) = report_publish_result(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    &job.signing_token,
                    &published,
                    &PUBLISH_REPORT_DELAYS,
                )
                .await
                {
                    let _ = append_runner_log_line(
                        client,
                        daemon_url,
                        config,
                        &job.build_id,
                        log_seq,
                        "stderr",
                        &format!(
                            "[oore-publish] Warning: the build is on Google Play, but Oore could not \
                             record the release, so it will not track or roll it out: {error:#}"
                        ),
                    )
                    .await;
                }
                let _ = append_runner_log_line(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    log_seq,
                    "stdout",
                    &step_end_marker(step_name, "succeeded", Some(0)),
                )
                .await;
            }
            Err(error) => {
                let _ = append_runner_log_line(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    log_seq,
                    "stderr",
                    &format!("[oore-publish] {error:#}"),
                )
                .await;
                let _ = append_runner_log_line(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    log_seq,
                    "stdout",
                    &step_end_marker(step_name, "failed", Some(1)),
                )
                .await;
                return Err(error.context("publish-play failed"));
            }
        }
    }

    if let Some(app_store) = publish_plan
        .app_store
        .as_ref()
        .filter(|_| runs(PUBLISH_APP_STORE_STEP))
    {
        let step_name = PUBLISH_APP_STORE_STEP;
        let started = now_unix();
        let _ = append_runner_log_line(
            client,
            daemon_url,
            config,
            &job.build_id,
            log_seq,
            "stdout",
            &step_start_marker(step_name, "Upload the IPA to App Store Connect"),
        )
        .await;
        let result = match (signed_ipa_path, ios_artifact_metadata) {
            (Some(ipa), Some(metadata)) => {
                let key_dir = signing_workspace.join(".oore").join("asc-keys");
                // Once altool starts, Apple may get the build; don't start
                // it for a build that has already stopped.
                match check_build_active(client, daemon_url, config, &job.build_id, authority)
                    .await
                {
                    Err(error) => Err(error.context(
                        "The build stopped before the App Store Connect upload started; nothing was uploaded",
                    )),
                    Ok(()) => tokio::select! {
                        uploaded = app_store_upload::upload_ipa(ipa, &app_store.credentials, &key_dir) => {
                            uploaded.and_then(|message| {
                                app_store_upload::publish_result(metadata, &app_store.config)
                                    .map(|result| (message, result))
                            })
                        }
                        _ = poll_cancellation(client, daemon_url, config, &job.build_id, authority.clone()) => {
                            Err(authority.terminated().context(
                                "The build stopped during the App Store Connect upload, so the upload was stopped",
                            ))
                        }
                    },
                }
            }
            _ => Err(anyhow::anyhow!(
                "App Store publishing is on, but this build produced no signed App Store IPA"
            )),
        };
        let finished = now_unix();
        steps.push(StepResult {
            name: step_name.to_string(),
            status: if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            }
            .to_string(),
            exit_code: Some(if result.is_ok() { 0 } else { 1 }),
            started_at: started,
            finished_at: finished,
            duration_ms: (finished - started) * 1000,
        });
        match result {
            Ok((message, published)) => {
                let _ = append_runner_log_line(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    log_seq,
                    "stdout",
                    &format!("[oore-publish] {message}"),
                )
                .await;
                if let Err(error) = report_publish_result(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    &job.signing_token,
                    &published,
                    &PUBLISH_REPORT_DELAYS,
                )
                .await
                {
                    let _ = append_runner_log_line(
                        client,
                        daemon_url,
                        config,
                        &job.build_id,
                        log_seq,
                        "stderr",
                        &format!(
                            "[oore-publish] Warning: the build is on App Store Connect, but Oore could not \
                             record the release, so it will not track or roll it out: {error:#}"
                        ),
                    )
                    .await;
                }
                let _ = append_runner_log_line(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    log_seq,
                    "stdout",
                    &step_end_marker(step_name, "succeeded", Some(0)),
                )
                .await;
            }
            Err(error) => {
                let _ = append_runner_log_line(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    log_seq,
                    "stderr",
                    &format!("[oore-publish] {error:#}"),
                )
                .await;
                let _ = append_runner_log_line(
                    client,
                    daemon_url,
                    config,
                    &job.build_id,
                    log_seq,
                    "stdout",
                    &step_end_marker(step_name, "failed", Some(1)),
                )
                .await;
                return Err(error.context("publish-app-store failed"));
            }
        }
    }

    Ok(())
}

/// The build's VM and the host slot it occupies. The VM is declared first so
/// it is deleted before the slot is released.
struct BuildVm {
    vm: tart::TartVm,
    _slot: tart::VmSlot,
}

/// Saves the VM's caches back to the host unless `save_caches` is false,
/// deletes the VM, and only then frees its slot. Every step is bounded.
async fn shutdown_build_vm(
    build_vm: BuildVm,
    save_caches: bool,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    log_seq: &mut BuildLogCursor,
) {
    let BuildVm {
        mut vm,
        _slot: slot,
    } = build_vm;
    if save_caches && vm.has_cache_saves() {
        let _ = append_runner_log_line(
            client,
            daemon_url,
            config,
            build_id,
            log_seq,
            "stdout",
            &format!("Saving caches from VM {} to this runner", vm.name()),
        )
        .await;
        for line in vm.save_caches().await {
            let _ = append_runner_log_line(
                client, daemon_url, config, build_id, log_seq, "stdout", &line,
            )
            .await;
        }
    }
    let name = vm.name().to_string();
    if let Err(error) = vm.delete().await {
        eprintln!("Warning: {error:#}; the runner retries before its next build");
        let _ = append_runner_log_line(
            client,
            daemon_url,
            config,
            build_id,
            log_seq,
            "stderr",
            &format!("Could not delete VM {name}; the runner retries before its next build"),
        )
        .await;
    }
    drop(slot);
}

/// Reads the repository's optional `oore.image.yaml`.
fn read_vm_image_config(
    workspace: &Path,
) -> anyhow::Result<Option<(String, oore_contract::VmImageConfig)>> {
    let path = workspace.join(oore_contract::VM_IMAGE_CONFIG_PATH);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let config = oore_contract::parse_vm_image_yaml(&raw)
        .map_err(|error| anyhow::anyhow!("Invalid {}: {error}", path.display()))?;
    Ok(Some((raw, config)))
}

/// Waits for one of the host's macOS VM slots, builds the image if this host
/// does not have it yet, then boots the build's VM.
#[allow(clippy::too_many_arguments)]
async fn start_build_vm(
    job: &ClaimedJob,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    log_seq: &mut BuildLogCursor,
    authority: &Arc<BuildAuthorityState>,
    workspace: &Path,
    shares: &[tart::HostShare],
    flutter_version: &str,
    needs_xcode: bool,
) -> anyhow::Result<BuildVm> {
    let tart_bin = tart::find_tart().with_context(|| {
        format!(
            "This pipeline runs in a VM (isolation: vm), but Tart is not installed on this runner; install Tart {} or newer",
            tart::PINNED_TART_VERSION
        )
    })?;
    let tart_version = {
        let tart_bin = tart_bin.clone();
        tokio::task::spawn_blocking(move || tart::tart_version(&tart_bin))
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    };
    anyhow::ensure!(
        tart::version_is_supported(&tart_version),
        "Tart {} or newer is required for VM builds; this runner has '{}'",
        tart::PINNED_TART_VERSION,
        tart_version
    );
    // Virtualization.framework needs the runner account's GUI session, the
    // same one iOS signing uses; a bare launchd daemon context cannot boot VMs.
    require_gui_login_session("Running builds in a VM")?;
    let image_config = read_vm_image_config(workspace)?;
    let plan = tart::plan_image(
        &tart::image_template(),
        &tart::base_image(needs_xcode),
        flutter_version,
        !needs_xcode,
        &job.project_id,
        image_config
            .as_ref()
            .map(|(raw, config)| (raw.as_str(), config.clone())),
    )?;
    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
        "stdout",
        &format!(
            "Flutter {flutter_version} uses VM image {} (Tart {tart_version}){}",
            plan.name,
            if plan.config.is_some() {
                format!(", customised by {}", oore_contract::VM_IMAGE_CONFIG_PATH)
            } else {
                String::new()
            }
        ),
    )
    .await;

    let slot_dir = tart::slot_dir()?;
    let mut waiting_logged = false;
    let slot = loop {
        if let Some(slot) = tart::try_acquire_slot(&slot_dir, tart::MAX_MACOS_VMS)? {
            break slot;
        }
        if !waiting_logged {
            waiting_logged = true;
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                log_seq,
                "stdout",
                &format!(
                    "Waiting for a macOS VM slot; this host runs at most {} at a time",
                    tart::MAX_MACOS_VMS
                ),
            )
            .await;
        }
        check_build_active(client, daemon_url, config, &job.build_id, authority).await?;
        tokio::time::sleep(Duration::from_secs(5)).await;
    };

    let name = tart::vm_name(&config.runner_id, &job.build_id);
    if tart::is_registry_reference(&plan.name) {
        pull_registry_image(
            job, client, daemon_url, config, log_seq, authority, &tart_bin, &plan.name,
        )
        .await?;
    } else {
        // Before the image is checked, so pruning cannot delete it under us.
        if let Err(error) = tart::record_image_use(&slot_dir, &plan.name, now_unix()).await {
            eprintln!(
                "Warning: could not record the use of VM image {}: {error:#}",
                plan.name
            );
        }
        ensure_vm_image(
            job, client, daemon_url, config, log_seq, authority, &tart_bin, &slot_dir, &plan, &name,
        )
        .await?;
    }
    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
        "stdout",
        &format!(
            "Cloning {} into {name} (VM slot {})",
            plan.name,
            slot.index + 1
        ),
    )
    .await;
    // Dropping the start on cancel deletes the half-started VM.
    let vm = tokio::select! {
        vm = tart::TartVm::start(tart_bin, &plan.name, name, Some(workspace), shares) => vm?,
        () = poll_cancellation(client, daemon_url, config, &job.build_id, authority.clone()) => {
            return Err(authority.terminated());
        }
    };
    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
        "stdout",
        &format!(
            "VM {} is ready; the workspace was copied to {}",
            vm.name(),
            vm.guest_workspace()
        ),
    )
    .await;
    for line in vm.cache_notes() {
        let _ = append_runner_log_line(
            client,
            daemon_url,
            config,
            &job.build_id,
            log_seq,
            "stdout",
            line,
        )
        .await;
    }
    Ok(BuildVm { vm, _slot: slot })
}

/// Downloads a registry image into Tart's cache unless it is already there,
/// streaming Tart's progress into the build log. Without this, the clone that
/// follows would download tens of gigabytes with no output and no way to cancel.
#[allow(clippy::too_many_arguments)]
async fn pull_registry_image(
    job: &ClaimedJob,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    log_seq: &mut BuildLogCursor,
    authority: &Arc<BuildAuthorityState>,
    tart_bin: &Path,
    reference: &str,
) -> anyhow::Result<()> {
    if tart::registry_image_cached(tart_bin, reference).await? {
        return Ok(());
    }
    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
        "stdout",
        &format!("Downloading {reference}; Tart reports its progress below"),
    )
    .await;
    let child = tart::spawn_pull(tart_bin, reference)?;
    let status = run_and_stream(
        child,
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
        poll_cancellation(client, daemon_url, config, &job.build_id, authority.clone()),
    )
    .await;
    match status {
        None => Err(authority.terminated()),
        Some(status) if !status.success() => {
            anyhow::bail!("Downloading {reference} failed with {status}; see the log above")
        }
        Some(_) => Ok(()),
    }
}

/// Builds `plan.name` from its base image unless this host already has it.
/// Runs inside the build's own VM slot.
#[allow(clippy::too_many_arguments)]
async fn ensure_vm_image(
    job: &ClaimedJob,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    log_seq: &mut BuildLogCursor,
    authority: &Arc<BuildAuthorityState>,
    tart_bin: &Path,
    lock_dir: &Path,
    plan: &tart::ImagePlan,
    vm_name: &str,
) -> anyhow::Result<()> {
    let mut waiting_logged = false;
    let _image_lock = loop {
        if tart::local_image_exists(tart_bin, &plan.name).await? {
            return Ok(());
        }
        if let Some(lock) = tart::try_lock_image(lock_dir, &plan.name)? {
            break lock;
        }
        if !waiting_logged {
            waiting_logged = true;
            let _ = append_runner_log_line(
                client,
                daemon_url,
                config,
                &job.build_id,
                log_seq,
                "stdout",
                &format!(
                    "Another build is creating VM image {}; waiting for it",
                    plan.name
                ),
            )
            .await;
        }
        check_build_active(client, daemon_url, config, &job.build_id, authority).await?;
        tokio::time::sleep(Duration::from_secs(10)).await;
    };
    // The build holding the lock before us may have just finished it.
    if tart::local_image_exists(tart_bin, &plan.name).await? {
        return Ok(());
    }

    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
        "stdout",
        &format!(
            "This runner has no VM image {}; building it from {}. The first build of a new image downloads the base and Flutter and can take an hour or more",
            plan.name, plan.base
        ),
    )
    .await;
    if tart::is_registry_reference(&plan.base) {
        pull_registry_image(
            job, client, daemon_url, config, log_seq, authority, tart_bin, &plan.base,
        )
        .await?;
    }
    let builder = tokio::select! {
        builder = tart::TartVm::start(
            tart_bin.to_path_buf(),
            &plan.base,
            format!("{vm_name}-image"),
            None,
            &[],
        ) => builder.with_context(|| format!("failed to start base image {}", plan.base))?,
        () = poll_cancellation(client, daemon_url, config, &job.build_id, authority.clone()) => {
            return Err(authority.terminated());
        }
    };
    let provisioned = async {
        let child = builder.spawn_script(&tart::provision_script(plan)).await?;
        let status = run_and_stream(
            child,
            client,
            daemon_url,
            config,
            &job.build_id,
            log_seq,
            poll_cancellation(client, daemon_url, config, &job.build_id, authority.clone()),
        )
        .await;
        match status {
            None => {
                return Err(authority.terminated());
            }
            Some(status) if !status.success() => {
                anyhow::bail!(
                    "Building VM image {} failed with {status}; see the log above",
                    plan.name
                );
            }
            Some(_) => {}
        }
        // Check Flutter the way build steps find it, so an image that cannot
        // run Flutter is never cached.
        let check = builder
            .run_script("exec /bin/sh -c 'flutter --version'")
            .await?;
        anyhow::ensure!(
            check.status.success(),
            "Building VM image {} finished, but build steps cannot run flutter in it ({}): {}",
            plan.name,
            check.status,
            String::from_utf8_lossy(&check.stderr).trim()
        );
        Ok(())
    }
    .await;
    if let Err(error) = provisioned {
        if let Err(delete_error) = builder.delete().await {
            eprintln!("Warning: {delete_error:#}");
        }
        return Err(error);
    }
    builder
        .keep_as(&plan.name)
        .await
        .context("saving the VM image failed")?;
    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        &job.build_id,
        log_seq,
        "stdout",
        &format!("VM image {} is ready and cached on this runner", plan.name),
    )
    .await;
    Ok(())
}

async fn fetch_job_publish_plan(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    signing_token: &str,
) -> anyhow::Result<RunnerPublishPlan> {
    let resp = client
        .get(format!(
            "{}/v1/runners/{}/jobs/{}/publish",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .header("x-oore-signing-token", signing_token)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("publish settings lookup failed: {}", resp.status());
    }
    Ok(resp.json().await?)
}

/// Waits between attempts to record a store upload: about two minutes in all,
/// enough to ride out a daemon restart.
const PUBLISH_REPORT_DELAYS: [Duration; 5] = [
    Duration::ZERO,
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(20),
    Duration::from_secs(40),
];
const PUBLISH_REPORT_TIMEOUT: Duration = Duration::from_secs(30);

/// Tell the daemon about a finished store upload so it records the release.
/// The daemon ignores repeats, so this retries until the daemon answers or
/// rejects the report.
async fn report_publish_result(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    signing_token: &str,
    result: &RunnerPublishResult,
    delays: &[Duration],
) -> anyhow::Result<()> {
    let mut last_error = None;
    for delay in delays {
        if !delay.is_zero() {
            tokio::time::sleep(*delay).await;
        }
        let sent = client
            .post(format!(
                "{}/v1/runners/{}/jobs/{}/publish-results",
                daemon_url, config.runner_id, build_id
            ))
            .bearer_auth(&config.runner_token)
            .header("x-oore-signing-token", signing_token)
            .timeout(PUBLISH_REPORT_TIMEOUT)
            .json(result)
            .send()
            .await;
        let resp = match sent {
            Ok(resp) => resp,
            Err(error) => {
                last_error = Some(anyhow::Error::new(error));
                continue;
            }
        };
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let body = resp.text().await.unwrap_or_default();
        let error = anyhow::anyhow!("the daemon answered {status}: {body}");
        if runner_response_is_terminal(status) {
            return Err(error);
        }
        last_error = Some(error);
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("the publish result was not sent")))
}

/// Test report file name for a step. Step names can hold spaces and
/// punctuation; v1 names (`pre_build-1`) pass through unchanged.
fn report_file_stem(step_name: &str) -> String {
    step_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Copy the report a v2 step declares in `reports.tests` into the reports
/// directory the runner reads after every step.
fn copy_declared_test_report(
    workspace: &Path,
    report: &str,
    reports_dir: &Path,
    step_name: &str,
) -> Result<(), String> {
    let source = workspace.join(report);
    if !source.is_file() {
        return Err(format!(
            "step '{step_name}' declares reports.tests {report}, but the file was not written"
        ));
    }
    let inside_workspace = match (source.canonicalize(), workspace.canonicalize()) {
        (Ok(source), Ok(workspace)) => source.starts_with(workspace),
        _ => false,
    };
    if !inside_workspace {
        return Err(format!(
            "step '{step_name}' declares reports.tests {report}, which resolves outside the workspace"
        ));
    }
    // The collector skips larger reports anyway; do not copy them first.
    let size = fs::metadata(&source).map(|meta| meta.len()).unwrap_or(0);
    if size > test_reports::MAX_REPORT_FILE_BYTES {
        return Err(format!(
            "step '{step_name}' declares reports.tests {report}, which is larger than {} MiB and is skipped",
            test_reports::MAX_REPORT_FILE_BYTES / (1024 * 1024)
        ));
    }
    let extension = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("json");
    fs::create_dir_all(reports_dir)
        .and_then(|_| {
            fs::copy(
                &source,
                reports_dir.join(format!(
                    "{}-declared.{extension}",
                    report_file_stem(step_name)
                )),
            )
        })
        .map(|_| ())
        .map_err(|error| format!("could not read reports.tests {report}: {error}"))
}

/// Parse the reports a step wrote and send them to the daemon. Test reporting
/// never fails the build; problems are logged instead.
#[allow(clippy::too_many_arguments)]
async fn report_step_tests(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    log_seq: &mut BuildLogCursor,
    reports_dir: &Path,
    workspaces: &[PathBuf],
    step_name: &str,
) {
    let masker = &log_seq.masker;
    let Some(report) =
        test_reports::collect_test_reports(reports_dir, workspaces, step_name, |text| {
            masker.mask(text)
        })
    else {
        return;
    };
    let _ = append_runner_log_line(
        client,
        daemon_url,
        config,
        build_id,
        log_seq,
        "stdout",
        &test_reports::summary_line(&report),
    )
    .await;
    let result = client
        .post(format!(
            "{}/v1/runners/{}/jobs/{}/test-results",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&report)
        .send()
        .await
        .map_err(anyhow::Error::from)
        .and_then(|resp| {
            if resp.status().is_success() {
                Ok(())
            } else {
                Err(anyhow::anyhow!("{}", resp.status()))
            }
        });
    if let Err(error) = result {
        let _ = append_runner_log_line(
            client,
            daemon_url,
            config,
            build_id,
            log_seq,
            "stderr",
            &format!("[oore-tests] Could not upload test results: {error}"),
        )
        .await;
    }
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
    cursor: &mut BuildLogCursor,
    stream: &str,
    content: &str,
) -> anyhow::Result<()> {
    let body = serde_json::json!({
        "chunks": [{
            "sequence": cursor.seq,
            "content": cursor.masker.mask(content),
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
        .timeout(CONTROL_REQUEST_TIMEOUT)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("log append failed: {}", resp.status());
    }

    cursor.seq += 1;
    Ok(())
}

fn artifact_type_for_extension(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "apk" => Some("apk"),
        "ipa" => Some("ipa"),
        "aab" => Some("aab"),
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

        let resp = reserve_artifact(client, daemon_url, config, build_id, name, body).await?;

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

/// Reserves an artifact upload. A daemon older than the `aab` artifact type
/// rejects it with `invalid_artifact_type`; the bundle is then reserved once
/// more as `generic`, which is how those daemons stored App Bundles.
async fn reserve_artifact(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    name: &str,
    mut body: serde_json::Value,
) -> anyhow::Result<reqwest::Response> {
    let url = format!(
        "{}/v1/runners/{}/jobs/{}/artifacts",
        daemon_url, config.runner_id, build_id
    );
    let mut fell_back = false;
    loop {
        let resp = client
            .post(&url)
            .bearer_auth(&config.runner_token)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("failed to reserve artifact {name}"))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        if !fell_back
            && status == reqwest::StatusCode::BAD_REQUEST
            && body["artifact_type"] == "aab"
        {
            let code = resp
                .json::<oore_contract::ApiError>()
                .await
                .map(|error| error.code)
                .unwrap_or_default();
            if code == "invalid_artifact_type" {
                println!(
                    "  Daemon does not accept the aab artifact type yet; uploading {name} as generic"
                );
                body["artifact_type"] = serde_json::Value::from("generic");
                fell_back = true;
                continue;
            }
        }
        anyhow::bail!("artifact reservation failed for {name} (HTTP {status})");
    }
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
    report: &StatusReport<'_>,
) -> anyhow::Result<()> {
    let body = serde_json::json!({
        "status": report.status,
        "exit_code": report.exit_code,
        "error_message": report.error_message,
        "steps": report.steps,
    });

    let resp = client
        .post(format!(
            "{}/v1/runners/{}/jobs/{}/status",
            daemon_url, config.runner_id, report.build_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&body)
        .timeout(CONTROL_REQUEST_TIMEOUT)
        .send()
        .await?;

    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    if is_transient_authority_status(status) {
        anyhow::bail!("Status update failed: {status}");
    }
    Err(StatusReportRejected(status).into())
}

/// The backend refused a status report, for example because the build
/// already ended or moved to another runner. Retrying cannot help.
#[derive(Debug)]
struct StatusReportRejected(reqwest::StatusCode);

impl std::fmt::Display for StatusReportRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Status update was rejected: {}", self.0)
    }
}

impl std::error::Error for StatusReportRejected {}

#[derive(Debug, Clone, Copy)]
struct RetryPolicy {
    first_delay: Duration,
    max_delay: Duration,
    window: Duration,
}

impl RetryPolicy {
    fn within(window: Duration) -> Self {
        Self {
            first_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            window,
        }
    }
}

/// Sends a status report until the backend takes it, the backend refuses
/// it, or `policy.window` runs out. A lost final report would leave the build
/// running until it timed out.
async fn report_status_with_retry(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    report: &StatusReport<'_>,
    policy: RetryPolicy,
) -> anyhow::Result<()> {
    let started = Instant::now();
    let mut delay = policy.first_delay;
    loop {
        let error = match report_status(client, daemon_url, config, report).await {
            Ok(()) => return Ok(()),
            Err(error) if error.downcast_ref::<StatusReportRejected>().is_some() => {
                return Err(error);
            }
            Err(error) => error,
        };
        if started.elapsed() + delay > policy.window {
            return Err(error.context(format!(
                "gave up reporting build {} as {}",
                report.build_id, report.status
            )));
        }
        eprintln!(
            "Could not report build {} as {} ({error:#}); retrying in {}s",
            report.build_id,
            report.status,
            delay.as_secs_f32()
        );
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(policy.max_delay);
    }
}

async fn run_and_stream(
    mut child: tokio::process::Child,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    cursor: &mut BuildLogCursor,
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

    let mut readers = Vec::new();
    // When each reader last saw a line, in milliseconds since `opened`.
    let opened = Instant::now();
    let last_line_ms = Arc::new(AtomicU64::new(0));
    if let Some(stdout) = stdout {
        let tx = line_tx.clone();
        let last_line_ms = last_line_ms.clone();
        readers.push(tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                last_line_ms.fetch_max(opened.elapsed().as_millis() as u64, Ordering::Relaxed);
                if tx.send((line, "stdout".to_string())).await.is_err() {
                    break;
                }
            }
        }));
    }

    if let Some(stderr) = stderr {
        let tx = line_tx.clone();
        let last_line_ms = last_line_ms.clone();
        readers.push(tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                last_line_ms.fetch_max(opened.elapsed().as_millis() as u64, Ordering::Relaxed);
                if tx.send((line, "stderr".to_string())).await.is_err() {
                    break;
                }
            }
        }));
    }

    drop(line_tx);

    let upload_client = client_out;
    let upload_daemon = daemon_out;
    let upload_config_id = config_out_id;
    let upload_config_token = config_out_token;
    let upload_build = build_out;
    let seq_start = cursor.seq;
    let masker = cursor.masker.clone();
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
                                "content": masker.mask(&content),
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
                                    .timeout(CONTROL_REQUEST_TIMEOUT)
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
                                    .timeout(CONTROL_REQUEST_TIMEOUT)
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
                            .timeout(CONTROL_REQUEST_TIMEOUT)
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
            stop_process_group(&mut child).await;
            None
        }
    };

    // A background process the step started can keep the pipes open after
    // the step exits. Keep reading while output still arrives, then stop.
    let exited = Instant::now();
    let abort_readers: Vec<_> = readers.iter().map(|reader| reader.abort_handle()).collect();
    let readers_done = join_all(readers);
    tokio::pin!(readers_done);
    loop {
        if tokio::time::timeout(Duration::from_millis(250), &mut readers_done)
            .await
            .is_ok()
        {
            break;
        }
        let last_line = opened + Duration::from_millis(last_line_ms.load(Ordering::Relaxed));
        if last_line.max(exited).elapsed() >= STEP_OUTPUT_GRACE
            || exited.elapsed() >= STEP_OUTPUT_LIMIT
        {
            for reader in &abort_readers {
                reader.abort();
            }
            break;
        }
    }

    if let Ok(final_seq) = upload_handle.await {
        cursor.seq = final_seq;
    }

    status
}

#[cfg(test)]
mod step_retry_tests {
    use super::{artifact_type_for_extension, retry_artifact_path};

    #[test]
    fn artifact_types_follow_the_file_extension() {
        assert_eq!(artifact_type_for_extension("apk"), Some("apk"));
        assert_eq!(artifact_type_for_extension("ipa"), Some("ipa"));
        assert_eq!(artifact_type_for_extension("aab"), Some("aab"));
        assert_eq!(artifact_type_for_extension("AAB"), Some("aab"));
        assert_eq!(artifact_type_for_extension("txt"), None);
    }

    #[test]
    fn reused_artifacts_go_where_a_build_leaves_them() {
        assert_eq!(
            retry_artifact_path("app-release.aab", "generic").as_deref(),
            Some("build/app/outputs/bundle/release/app-release.aab")
        );
        assert_eq!(
            retry_artifact_path("app-release.aab", "aab").as_deref(),
            Some("build/app/outputs/bundle/release/app-release.aab")
        );
        assert_eq!(
            retry_artifact_path("mapping.txt", "generic").as_deref(),
            Some("build/app/outputs/mapping/release/mapping.txt")
        );
        assert_eq!(
            retry_artifact_path("Runner.ipa", "ipa").as_deref(),
            Some("build/ios/ipa/Runner.ipa")
        );
        assert_eq!(retry_artifact_path("app-release.apk", "apk"), None);
        for unsafe_name in ["../app.aab", "a/b.aab", ".hidden.aab", ""] {
            assert_eq!(retry_artifact_path(unsafe_name, "generic"), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        choose_ios_export_method, load_ui_execution_config, pubspec_version_name,
        require_runner_labels, resolve_execution_plan,
    };
    use oore_contract::{BuildIsolation, IosExportMethod, StepPhase};
    use serde_json::json;

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn plan_commands(plan: &super::ResolvedExecutionPlan, phase: StepPhase) -> Vec<String> {
        plan.steps
            .iter()
            .filter(|step| step.phase == phase)
            .map(|step| step.run.clone())
            .collect()
    }

    #[test]
    fn v2_files_run_named_steps_through_the_same_plan() {
        let workspace = TempDir::new("oore-v2-plan-test");
        std::fs::write(
            workspace.path().join(".oore.yaml"),
            "version: 2\napp:\n  flutter: 3.47.7\ntargets:\n  android:\n    build: flutter build appbundle --release\n  ios:\n    build: flutter build ipa --release\nsteps:\n  - name: Get packages\n    run: flutter pub get\n  - build\n  - name: Notify\n    run: echo ${{ secrets.HOOK }}\nenv:\n  CHANNEL: beta\n",
        )
        .unwrap();

        let plan = resolve_execution_plan(workspace.path(), &json!({}), true).unwrap();
        let names: Vec<(&str, StepPhase)> = plan
            .steps
            .iter()
            .map(|step| (step.name.as_str(), step.phase))
            .collect();
        assert_eq!(
            names,
            [
                ("Get packages", StepPhase::PreBuild),
                ("Build Android", StepPhase::Build),
                ("Build iOS", StepPhase::Build),
                ("Notify", StepPhase::PostBuild),
            ]
        );
        // v2 build commands are complete; a Play upload adds no hidden bundle step.
        assert_eq!(
            plan_commands(&plan, StepPhase::Build),
            [
                "flutter build appbundle --release",
                "flutter build ipa --release"
            ]
        );
        assert_eq!(plan.flutter_version, "3.47.7");
        assert!(plan.needs_xcode);
        assert_eq!(plan.env[0].key, "CHANNEL");

        let secrets = std::collections::BTreeMap::from([("HOOK".to_string(), "x".to_string())]);
        let resolved = super::apply_secret_references(plan, &secrets).unwrap();
        assert_eq!(resolved.steps[3].run, "echo ${OORE_SECRET_HOOK}");

        let android_only = resolve_execution_plan(
            workspace.path(),
            &json!({ "selected_platforms": ["android"] }),
            false,
        )
        .unwrap();
        assert_eq!(android_only.steps.len(), 3);
        assert!(!android_only.needs_xcode);

        std::fs::write(
            workspace.path().join(".oore.yaml"),
            "version: 2\ntargets:\n  android:\n    build: a\nworkflows:\n  adhoc: {}\n  release: {}\n",
        )
        .unwrap();
        let error = resolve_execution_plan(workspace.path(), &json!({}), false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("choose one for the pipeline"), "{error}");
        let release =
            resolve_execution_plan(workspace.path(), &json!({ "workflow": "release" }), false)
                .unwrap();
        assert_eq!(release.steps[0].name, "Build Android");
    }

    #[test]
    fn declared_test_reports_are_copied_for_collection() {
        let workspace = TempDir::new("oore-declared-report-ws");
        let reports = TempDir::new("oore-declared-report-out");
        std::fs::create_dir_all(workspace.path().join("build")).unwrap();
        std::fs::write(workspace.path().join("build/tests.json"), "{}").unwrap();

        super::copy_declared_test_report(
            workspace.path(),
            "build/tests.json",
            reports.path(),
            "Unit tests",
        )
        .unwrap();
        assert!(reports.path().join("Unit-tests-declared.json").is_file());

        let error = super::copy_declared_test_report(
            workspace.path(),
            "build/missing.xml",
            reports.path(),
            "Unit tests",
        )
        .unwrap_err();
        assert!(error.contains("was not written"), "{error}");

        // Oversized reports are skipped, not copied. A sparse file keeps the
        // test cheap.
        std::fs::File::create(workspace.path().join("build/huge.json"))
            .unwrap()
            .set_len(super::test_reports::MAX_REPORT_FILE_BYTES + 1)
            .unwrap();
        let error = super::copy_declared_test_report(
            workspace.path(),
            "build/huge.json",
            reports.path(),
            "Huge",
        )
        .unwrap_err();
        assert!(
            error.contains("larger than 64 MiB and is skipped"),
            "{error}"
        );
        assert!(!reports.path().join("Huge-declared.json").exists());
        assert_eq!(super::report_file_stem("pre_build-1"), "pre_build-1");
        assert_eq!(
            super::report_file_stem("Build iOS (pro)"),
            "Build-iOS--pro-"
        );
    }

    #[test]
    fn host_flutter_must_match_a_pinned_version() {
        use super::{check_host_flutter, parse_flutter_machine_version};
        let machine = "Welcome to Flutter!\n{\n  \"frameworkVersion\": \"3.47.7\",\n  \"channel\": \"stable\"\n}\n";
        assert_eq!(
            parse_flutter_machine_version(machine).as_deref(),
            Some("3.47.7")
        );
        assert_eq!(parse_flutter_machine_version("no json"), None);

        assert!(check_host_flutter("3.47.7", Ok("3.47.7".into())).is_ok());
        assert!(check_host_flutter("v3.47.7", Ok("3.47.7".into())).is_ok());
        assert!(check_host_flutter("3.47.7@stable", Ok("3.47.7".into())).is_ok());
        let wrong = check_host_flutter("3.47.7", Ok("3.44.0".into())).unwrap_err();
        assert!(wrong.contains("needs Flutter 3.47.7") && wrong.contains("3.44.0"));
        let missing = check_host_flutter("3.47.7", Err("no flutter on the runner's PATH".into()))
            .unwrap_err();
        assert!(missing.contains("no flutter on the runner's PATH"));

        assert!(check_host_flutter("stable", Ok("3.44.0".into())).is_ok());
        assert!(
            check_host_flutter("stable", Err("no flutter on the runner's PATH".into())).is_ok()
        );
    }

    #[test]
    fn vm_android_signing_keeps_secrets_quoted_and_off_the_workspace() {
        let inputs = super::AndroidSigningInputs {
            keystore_bytes: b"keystore".to_vec(),
            keystore_password: "p'ss".to_string(),
            key_alias: "upload".to_string(),
            key_password: "k$y".to_string(),
        };
        let script = super::android_vm_signing_script(
            &[
                "/Users/admin/oore-workspace/build/app/outputs/flutter-apk/app-release.apk"
                    .to_string(),
            ],
            "apk",
            &inputs,
        );
        assert!(
            script.contains("printf %s 'a2V5c3RvcmU=' | base64 --decode > \"$dir/keystore.jks\"")
        );
        assert!(script.contains("export OORE_SIGNER_STORE_PASSWORD='p'\\''ss'"));
        assert!(script.contains("export OORE_SIGNER_KEY_PASSWORD='k$y'"));
        assert!(script.contains("--out \"$dir/signed-0.apk\" '/Users/admin/oore-workspace/build/app/outputs/flutter-apk/app-release.apk'"));
        assert!(script.contains("cp \"$dir/signed-0.apk\" '/Users/admin/oore-workspace/"));

        let bundle = super::android_vm_signing_script(
            &["/w/build/app/outputs/bundle/release/app-release.aab".to_string()],
            "aab",
            &inputs,
        );
        assert!(bundle.contains("jarsigner -keystore \"$dir/keystore.jks\""));
        assert!(bundle.contains("verified=\"$(jarsigner -verify \"$dir/signed-0.aab\")\""));
        assert!(!bundle.contains("-strict"));
    }

    #[test]
    fn repository_runs_on_must_match_the_claiming_runner() {
        let snapshot = serde_json::json!({ "runner_labels": ["arm64", "macos", "xcode-16"] });
        assert!(require_runner_labels(&["macos".into()], &snapshot, ".oore.yaml").is_ok());
        let error = require_runner_labels(&["linux".into()], &snapshot, ".oore.yaml")
            .unwrap_err()
            .to_string();
        assert!(error.contains("lacks [linux]"), "{error}");
        // Older backends do not send runner labels.
        assert!(
            require_runner_labels(&["linux".into()], &serde_json::json!({}), ".oore.yaml").is_ok()
        );
    }

    #[test]
    fn play_publishing_adds_an_app_bundle_to_the_default_android_build() {
        let workspace =
            std::env::temp_dir().join(format!("oore-play-bundle-test-{}", std::process::id()));
        std::fs::create_dir_all(&workspace).unwrap();
        let snapshot = json!({
            "ui_execution_config": {
                "platforms": ["android"],
                "platform_build_args": { "android": ["--split-per-abi", "--flavor=prod"] }
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot, true).unwrap();
        assert_eq!(
            plan_commands(&plan, StepPhase::Build),
            vec![
                "flutter build apk --release --split-per-abi --flavor=prod",
                "flutter build appbundle --release --flavor=prod",
            ]
        );

        let plan = resolve_execution_plan(&workspace, &snapshot, false).unwrap();
        assert_eq!(
            plan_commands(&plan, StepPhase::Build),
            vec!["flutter build apk --release --split-per-abi --flavor=prod"]
        );

        // An explicit Android command is the user's choice; leave it alone.
        let snapshot = json!({
            "ui_execution_config": {
                "platforms": ["android"],
                "platform_commands": { "android": "flutter build apk --release" }
            }
        });
        let plan = resolve_execution_plan(&workspace, &snapshot, true).unwrap();
        assert_eq!(
            plan_commands(&plan, StepPhase::Build),
            vec!["flutter build apk --release"]
        );
        std::fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn flutter_commands_run_unwrapped_and_fvmrc_picks_the_version() {
        let workspace =
            std::env::temp_dir().join(format!("oore-vm-plan-test-{}", std::process::id()));
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join(".fvmrc"), r#"{"flutter": "3.47.7"}"#).unwrap();
        std::fs::write(
            workspace.join(".oore.yaml"),
            "version: 1\nplatforms: [android]\nisolation: vm\n",
        )
        .unwrap();

        let plan = resolve_execution_plan(&workspace, &json!({}), false).unwrap();
        assert_eq!(plan.isolation, BuildIsolation::Vm);
        assert_eq!(plan.flutter_version, "3.47.7");
        assert_eq!(
            plan_commands(&plan, StepPhase::PreBuild),
            vec!["flutter pub get"]
        );
        assert_eq!(
            plan_commands(&plan, StepPhase::Build),
            vec!["flutter build apk --release"]
        );

        std::fs::write(
            workspace.join(".oore.yaml"),
            "version: 1\nplatforms: [android]\n",
        )
        .unwrap();
        let plan = resolve_execution_plan(&workspace, &json!({}), false).unwrap();
        assert_eq!(plan.isolation, BuildIsolation::Host);
        assert_eq!(plan.flutter_version, "3.47.7");
        assert_eq!(
            plan_commands(&plan, StepPhase::PreBuild),
            vec!["flutter pub get"]
        );

        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[test]
    fn export_method_follows_pipeline_and_xcode_naming() {
        let current = "-exportArchive ... release-testing, app-store-connect, debugging";
        assert_eq!(
            choose_ios_export_method(current, IosExportMethod::AdHoc),
            "release-testing"
        );
        assert_eq!(
            choose_ios_export_method(current, IosExportMethod::AppStore),
            "app-store-connect"
        );
        assert_eq!(
            choose_ios_export_method("", IosExportMethod::AdHoc),
            "ad-hoc"
        );
        assert_eq!(
            choose_ios_export_method("", IosExportMethod::AppStore),
            "app-store"
        );
    }

    #[test]
    fn pubspec_version_name_drops_build_number() {
        assert_eq!(
            pubspec_version_name("name: app\nversion: 1.4.0+12\n").as_deref(),
            Some("1.4.0")
        );
        assert_eq!(
            pubspec_version_name("version: \"2.0.1\" # release\n").as_deref(),
            Some("2.0.1")
        );
        assert_eq!(
            pubspec_version_name("dependencies:\n  version: 1.0.0\n"),
            None
        );
    }

    #[test]
    fn ui_config_rejects_negative_build_number_offset() {
        let error =
            load_ui_execution_config(&json!({"ui_execution_config": {"build_number_offset": -3}}))
                .unwrap_err()
                .to_string();
        assert!(
            error.starts_with("build_number_offset must be between 0"),
            "{error}"
        );
    }

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

#[cfg(test)]
mod secret_tests {
    use std::collections::BTreeMap;

    use super::*;

    fn plan(env: &[(&str, &str)], build: &[&str]) -> ResolvedExecutionPlan {
        ResolvedExecutionPlan {
            steps: build
                .iter()
                .enumerate()
                .map(|(index, command)| PlanStep {
                    name: format!("build-{}", index + 1),
                    phase: StepPhase::Build,
                    run: command.to_string(),
                    source: format!("commands.build[{index}]"),
                    platform: None,
                    flavor: None,
                    test_report: None,
                })
                .collect(),
            artifact_patterns: vec![],
            env: env
                .iter()
                .map(|(key, value)| PipelineEnvVar {
                    key: key.to_string(),
                    value: value.to_string(),
                })
                .collect(),
            secret_env: vec![],
            secret_env_keys: Default::default(),
            build_number_offset: 0,
            isolation: BuildIsolation::Host,
            needs_xcode: false,
            flutter_version: "stable".to_string(),
            source: "test".to_string(),
        }
    }

    fn secrets() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("API_TOKEN".to_string(), "s3cr3t-token".to_string()),
            ("UNUSED".to_string(), "never-exposed".to_string()),
        ])
    }

    #[test]
    fn env_values_resolve_and_commands_use_shell_references() {
        let resolved = apply_secret_references(
            plan(
                &[
                    ("AUTH", "Bearer ${{ secrets.API_TOKEN }}"),
                    ("FLAVOR", "prod"),
                ],
                &["curl -H \"X-Token: ${{secrets.API_TOKEN}}\" https://example.test"],
            ),
            &secrets(),
        )
        .unwrap();

        assert_eq!(resolved.env[0].value, "Bearer s3cr3t-token");
        assert_eq!(resolved.env[1].value, "prod");
        assert!(resolved.secret_env_keys.contains("AUTH"));
        assert!(!resolved.secret_env_keys.contains("FLAVOR"));
        assert_eq!(
            resolved.steps[0].run,
            "curl -H \"X-Token: ${OORE_SECRET_API_TOKEN}\" https://example.test"
        );
        assert_eq!(
            resolved.secret_env,
            vec![(
                "OORE_SECRET_API_TOKEN".to_string(),
                "s3cr3t-token".to_string()
            )]
        );
    }

    #[test]
    fn unknown_secret_fails_before_any_step_runs() {
        let error =
            apply_secret_references(plan(&[], &["echo ${{ secrets.MISSING }}"]), &secrets())
                .err()
                .unwrap()
                .to_string();
        assert!(error.contains("commands.build[0]"), "{error}");
        assert!(error.contains("'MISSING'"), "{error}");
    }

    #[test]
    fn masker_hides_whole_values_and_lines_of_multiline_values() {
        let json = "{\n  \"private_key\": \"-----BEGIN PRIVATE KEY-----abc\",\n  \"x\": 1\n}";
        let masker = SecretMasker::new(["s3cr3t-token", json, ""].into_iter());
        assert_eq!(masker.mask("token=s3cr3t-token done"), "token=*** done");
        assert_eq!(
            masker.mask("  \"private_key\": \"-----BEGIN PRIVATE KEY-----abc\","),
            "  ***"
        );
        assert_eq!(masker.mask("nothing to hide"), "nothing to hide");
    }

    #[test]
    fn bundle_verification_requires_every_entry_to_be_signed() {
        let self_signed = "jar verified.\n\nWarning: \nThis jar contains entries whose signer certificate is self-signed.";
        assert!(check_android_bundle_verification(self_signed).is_ok());
        let unsigned = check_android_bundle_verification("no manifest.\n\njar is unsigned.")
            .unwrap_err()
            .to_string();
        assert!(unsigned.contains("jar is unsigned"), "{unsigned}");
        let partial = "jar verified.\n\nWarning: \nThis jar contains unsigned entries which have not been integrity-checked.";
        assert!(check_android_bundle_verification(partial).is_err());
        assert_eq!(
            android_signer_failure_detail("", b"  "),
            "signer exited without output"
        );
        assert_eq!(android_signer_failure_detail("out", b"err\n"), "err\nout");
    }

    #[test]
    fn app_bundles_sign_with_self_signed_upload_keys() {
        let toolchain = AndroidSigningToolchain::discover();
        let Some(keytool) = toolchain
            .java_home
            .as_ref()
            .map(|home| home.join("bin/keytool"))
            .or_else(|| fixed_system_executable("keytool"))
            .filter(|path| path.is_file())
        else {
            eprintln!("skipping: keytool is not installed");
            return;
        };
        if !toolchain.jarsigner.is_file() || !toolchain.zip.is_file() {
            eprintln!("skipping: jarsigner or zip is not installed");
            return;
        }
        // macOS ships /usr/bin/keytool as a stub that fails without a JDK.
        if !Command::new(&keytool)
            .arg("-help")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            eprintln!("skipping: keytool has no Java runtime");
            return;
        }

        let root = std::env::temp_dir().join(format!(
            "oore-aab-signing-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bundle_dir = root.join("workspace/build/app/outputs/bundle/release");
        fs::create_dir_all(&bundle_dir).unwrap();
        let keystore = root.join("upload.jks");
        let status = Command::new(&keytool)
            .args(["-genkeypair", "-keystore"])
            .arg(&keystore)
            .args([
                "-storepass",
                "store-secret",
                "-keypass",
                "store-secret",
                "-alias",
                "upload",
                "-keyalg",
                "RSA",
                "-keysize",
                "2048",
                "-validity",
                "10000",
                "-dname",
                "CN=Upload",
            ])
            .env_remove("JAVA_TOOL_OPTIONS")
            .output()
            .unwrap();
        assert!(status.status.success(), "{status:?}");

        fs::write(root.join("base.txt"), "module").unwrap();
        let bundle = bundle_dir.join("app-release.aab");
        let zipped = Command::new(&toolchain.zip)
            .current_dir(&root)
            .arg("-q")
            .arg(&bundle)
            .arg("base.txt")
            .output()
            .unwrap();
        assert!(zipped.status.success(), "{zipped:?}");

        let inputs = AndroidSigningInputs {
            keystore_bytes: fs::read(&keystore).unwrap(),
            keystore_password: "store-secret".into(),
            key_alias: "upload".into(),
            key_password: "store-secret".into(),
        };
        let signed = sign_android_artifacts(
            &root.join("workspace"),
            &root,
            "flutter build appbundle --release",
            &inputs,
            &toolchain,
        )
        .unwrap();
        assert_eq!(signed, vec![bundle.clone()]);

        let output = Command::new(&toolchain.jarsigner)
            .arg("-verify")
            .arg(&bundle)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{stdout}");
        assert!(stdout.contains("self-signed"), "{stdout}");
        fs::remove_dir_all(&root).ok();
    }
}

#[cfg(test)]
mod publish_report_tests {
    use std::time::Duration;

    use oore_contract::{PublishStore, RunnerPublishResult};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::{RunnerConfig, report_publish_result};

    /// Answers each connection with the next status and counts requests.
    async fn daemon(statuses: &'static [u16]) -> (String, tokio::task::JoinHandle<usize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let mut served = 0;
            for status in statuses {
                let Ok(Ok((mut socket, _))) =
                    tokio::time::timeout(Duration::from_millis(500), listener.accept()).await
                else {
                    break;
                };
                let mut buf = vec![0u8; 8192];
                let _ = socket.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{{}}"
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                served += 1;
            }
            served
        });
        (url, handle)
    }

    async fn report(url: &str) -> anyhow::Result<()> {
        let config = RunnerConfig {
            runner_id: "r1".into(),
            runner_token: "t".into(),
            daemon_url: url.into(),
            name: "runner".into(),
        };
        let result = RunnerPublishResult {
            store: PublishStore::GooglePlay,
            track: "production".into(),
            package_name: "com.example.app".into(),
            version_code: 7,
            version_name: None,
            rollout_fraction: Some(0.1),
            status: "in_progress".into(),
        };
        report_publish_result(
            &reqwest::Client::new(),
            url,
            &config,
            "b1",
            "signing",
            &result,
            &[
                Duration::ZERO,
                Duration::from_millis(10),
                Duration::from_millis(10),
            ],
        )
        .await
    }

    #[tokio::test]
    async fn retries_until_the_daemon_records_the_release() {
        let (url, daemon) = daemon(&[503, 500, 200]).await;
        report(&url).await.unwrap();
        assert_eq!(daemon.await.unwrap(), 3);
    }

    #[tokio::test]
    async fn stops_when_the_daemon_rejects_the_report() {
        let (url, daemon) = daemon(&[403, 200]).await;
        assert!(report(&url).await.is_err());
        assert_eq!(daemon.await.unwrap(), 1);
    }

    #[tokio::test]
    async fn gives_up_after_the_last_attempt() {
        let (url, daemon) = daemon(&[500, 500, 500, 200]).await;
        let error = report(&url).await.unwrap_err();
        assert!(format!("{error:#}").contains("500"), "{error:#}");
        assert_eq!(daemon.await.unwrap(), 3);
    }
}

#[cfg(test)]
mod build_lifecycle_tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    fn test_config(daemon_url: &str) -> RunnerConfig {
        RunnerConfig {
            runner_id: "r1".to_string(),
            runner_token: "token".to_string(),
            daemon_url: daemon_url.to_string(),
            name: "test".to_string(),
        }
    }

    /// Answers one request per connection with each status in turn and
    /// returns how many requests it answered.
    async fn backend(statuses: Vec<u16>) -> (String, tokio::task::JoinHandle<usize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for (served, status) in statuses.iter().enumerate() {
                let Ok(Ok((mut socket, _))) =
                    tokio::time::timeout(Duration::from_secs(5), listener.accept()).await
                else {
                    return served;
                };
                let mut buf = vec![0u8; 16 * 1024];
                let _ = socket.read(&mut buf).await.unwrap();
                let body = r#"{"status":"running"}"#;
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            statuses.len()
        });
        (url, server)
    }

    fn quick_retries() -> RetryPolicy {
        RetryPolicy {
            first_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(20),
            window: Duration::from_secs(5),
        }
    }

    fn succeeded(build_id: &str) -> StatusReport<'_> {
        StatusReport {
            build_id,
            status: "succeeded",
            exit_code: Some(0),
            error_message: None,
            steps: &[],
        }
    }

    #[tokio::test]
    async fn the_final_status_survives_two_backend_errors() {
        let (url, server) = backend(vec![503, 503, 200]).await;
        let config = test_config(&url);
        report_status_with_retry(
            &runner_http_client().unwrap(),
            &url,
            &config,
            &succeeded("b1"),
            quick_retries(),
        )
        .await
        .unwrap();
        assert_eq!(server.await.unwrap(), 3);
    }

    #[tokio::test]
    async fn a_rejected_status_report_is_not_retried() {
        let (url, server) = backend(vec![409, 200]).await;
        let config = test_config(&url);
        let error = report_status_with_retry(
            &runner_http_client().unwrap(),
            &url,
            &config,
            &succeeded("b1"),
            quick_retries(),
        )
        .await
        .unwrap_err();
        assert!(error.downcast_ref::<StatusReportRejected>().is_some());
        // The server waits for a second request that never comes.
        assert_eq!(server.await.unwrap(), 1);
    }

    #[tokio::test]
    async fn a_short_backend_outage_does_not_stop_the_build() {
        let (url, _server) = backend(vec![503, 503, 200, 503]).await;
        let config = test_config(&url);
        let client = runner_http_client().unwrap();
        let authority = BuildAuthorityState::with_grace(Duration::from_millis(300));

        for _ in 0..3 {
            check_build_active(&client, &url, &config, "b1", &authority)
                .await
                .unwrap();
        }
        // Past the grace window without word from the backend, the build stops
        // and says why.
        tokio::time::sleep(Duration::from_millis(400)).await;
        let error = check_build_active(&client, &url, &config, "b1", &authority)
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<BuildTerminated>().unwrap().status,
            CONTROLLER_UNAVAILABLE
        );
        assert_eq!(
            authority
                .terminated()
                .downcast_ref::<BuildTerminated>()
                .unwrap()
                .status,
            CONTROLLER_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn a_refused_connection_counts_toward_the_grace_window() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let config = test_config(&url);
        let client = runner_http_client().unwrap();
        let authority = BuildAuthorityState::with_grace(Duration::from_secs(60));
        check_build_active(&client, &url, &config, "b1", &authority)
            .await
            .unwrap();
        assert_eq!(
            authority
                .terminated()
                .downcast_ref::<BuildTerminated>()
                .unwrap()
                .status,
            "canceled"
        );
    }

    #[tokio::test]
    async fn a_lost_assignment_stops_the_build_at_once() {
        let (url, _server) = backend(vec![403]).await;
        let config = test_config(&url);
        let authority = BuildAuthorityState::default();
        check_build_active(
            &runner_http_client().unwrap(),
            &url,
            &config,
            "b1",
            &authority,
        )
        .await
        .unwrap_err();
        assert_eq!(
            authority
                .terminated()
                .downcast_ref::<BuildTerminated>()
                .unwrap()
                .status,
            "assignment_lost"
        );
    }

    fn process_is_gone(pid: &str) -> bool {
        // Reparented children may linger as zombies until init reaps them.
        let output = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", pid])
            .output()
            .unwrap();
        let stat = String::from_utf8_lossy(&output.stdout);
        stat.trim().is_empty() || stat.trim().starts_with('Z')
    }

    async fn read_pid(path: &Path) -> String {
        for _ in 0..100 {
            if let Ok(pid) = std::fs::read_to_string(path)
                && !pid.trim().is_empty()
            {
                return pid.trim().to_string();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the step never wrote its background pid");
    }

    fn spawn_step(script: &str, workspace: &Path) -> tokio::process::Child {
        let mut command = repository_shell_command(script, workspace);
        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        command.spawn().unwrap()
    }

    fn unreachable_daemon() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        url
    }

    #[tokio::test]
    async fn canceling_a_step_stops_everything_it_started() {
        let workspace = tempfile_dir("cancel");
        let child = spawn_step(
            "sleep 600 & echo $! > bg.pid; echo started; sleep 600",
            &workspace,
        );
        let url = unreachable_daemon();
        let config = test_config(&url);
        let mut cursor = BuildLogCursor::default();
        let started = Instant::now();
        let status = run_and_stream(
            child,
            &runner_http_client().unwrap(),
            &url,
            &config,
            "b1",
            &mut cursor,
            async {
                read_pid(&workspace.join("bg.pid")).await;
            },
        )
        .await;
        assert!(status.is_none());
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        let pid = read_pid(&workspace.join("bg.pid")).await;
        let mut gone = false;
        for _ in 0..50 {
            if process_is_gone(&pid) {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(gone, "background process {pid} survived the cancel");
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[tokio::test]
    async fn a_background_process_holding_the_output_does_not_block_the_runner() {
        let workspace = tempfile_dir("orphan");
        let child = spawn_step("sleep 600 & echo $! > bg.pid; echo done", &workspace);
        let url = unreachable_daemon();
        let config = test_config(&url);
        let mut cursor = BuildLogCursor::default();
        let started = Instant::now();
        let status = run_and_stream(
            child,
            &runner_http_client().unwrap(),
            &url,
            &config,
            "b1",
            &mut cursor,
            std::future::pending(),
        )
        .await;
        assert!(status.unwrap().success());
        assert!(
            started.elapsed() < STEP_OUTPUT_GRACE + Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        let pid = read_pid(&workspace.join("bg.pid")).await;
        // SAFETY: kill only sends a signal to the test's own background process.
        unsafe { libc::kill(pid.parse().unwrap(), libc::SIGKILL) };
        let _ = std::fs::remove_dir_all(&workspace);
    }

    fn tempfile_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "oore-runner-test-{name}-{}-{}",
            std::process::id(),
            rand::random::<u32>()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

#[cfg(test)]
mod artifact_reservation_tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::{RunnerConfig, reserve_artifact};

    /// Answers each connection with the next (status, body) and returns the
    /// request bodies it received.
    async fn daemon(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, tokio::task::JoinHandle<Vec<serde_json::Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let mut bodies = Vec::new();
            for (status, body) in responses {
                let Ok(Ok((mut socket, _))) =
                    tokio::time::timeout(Duration::from_millis(500), listener.accept()).await
                else {
                    break;
                };
                let mut request = Vec::new();
                let mut buf = [0u8; 4096];
                let request_body = loop {
                    let read = socket.read(&mut buf).await.unwrap();
                    request.extend_from_slice(&buf[..read]);
                    let text = String::from_utf8_lossy(&request).to_string();
                    if let Some((head, rest)) = text.split_once("\r\n\r\n") {
                        let length = head
                            .lines()
                            .find_map(|line| {
                                let (key, value) = line.split_once(':')?;
                                key.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().ok())?
                            })
                            .unwrap_or(0);
                        if rest.len() >= length || read == 0 {
                            break rest.to_string();
                        }
                    } else if read == 0 {
                        break String::new();
                    }
                };
                bodies.push(serde_json::from_str(&request_body).unwrap());
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            bodies
        });
        (url, handle)
    }

    async fn reserve(url: &str, artifact_type: &str) -> anyhow::Result<reqwest::Response> {
        let config = RunnerConfig {
            runner_id: "r1".into(),
            runner_token: "t".into(),
            daemon_url: url.into(),
            name: "runner".into(),
        };
        let body = serde_json::json!({ "name": "app.aab", "artifact_type": artifact_type });
        reserve_artifact(&reqwest::Client::new(), url, &config, "b1", "app.aab", body).await
    }

    const INVALID_TYPE: &str =
        r#"{"code":"invalid_artifact_type","error":"artifact_type must be one of: apk, ipa"}"#;

    #[tokio::test]
    async fn an_older_daemon_gets_app_bundles_as_generic() {
        let (url, server) = daemon(vec![(400, INVALID_TYPE), (201, "{}")]).await;
        assert!(reserve(&url, "aab").await.unwrap().status().is_success());
        let bodies = server.await.unwrap();
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0]["artifact_type"], "aab");
        assert_eq!(bodies[1]["artifact_type"], "generic");
        assert_eq!(bodies[1]["name"], "app.aab");
    }

    #[tokio::test]
    async fn falls_back_only_once_and_only_for_app_bundles() {
        let (url, server) =
            daemon(vec![(400, INVALID_TYPE), (400, INVALID_TYPE), (201, "{}")]).await;
        assert!(reserve(&url, "aab").await.is_err());
        assert_eq!(server.await.unwrap().len(), 2);

        let (url, server) = daemon(vec![(400, INVALID_TYPE), (201, "{}")]).await;
        assert!(reserve(&url, "apk").await.is_err());
        assert_eq!(server.await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn other_rejections_are_not_retried() {
        let body = r#"{"code":"invalid_name","error":"bad name"}"#;
        let (url, server) = daemon(vec![(400, body), (201, "{}")]).await;
        assert!(reserve(&url, "aab").await.is_err());
        assert_eq!(server.await.unwrap().len(), 1);
    }
}
