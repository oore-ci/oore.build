use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
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
    artifact_pattern_matches, parse_repository_pipeline_yaml, validate_artifact_pattern,
    validate_repository_config_path,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::io::AsyncBufReadExt;
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
const LEGACY_BUILD_WORKSPACE_ROOT: &str = "/tmp/oore-builds.noindex";
const SPOTLIGHT_NO_INDEX_SENTINEL: &str = ".metadata_never_index";
// ponytail: fixed three-check grace; move it into the runner protocol if deployments need tuning.
const MAX_CONSECUTIVE_AUTHORITY_FAILURES: u8 = 3;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunnerConfig {
    pub runner_id: String,
    pub runner_token: String,
    pub daemon_url: String,
    pub name: String,
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
        .ok()
        .and_then(|path| path.parent()?.parent().map(|root| root.join("VERSION")))
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    serde_json::json!({
        "os": "macos",
        "os_version": os_version,
        "arch": arch,
        "xcode_version": xcode_version,
        "version": version,
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

fn find_apksigner() -> PathBuf {
    for root in ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(PathBuf::from))
    {
        let build_tools = root.join("build-tools");
        let mut candidates = fs::read_dir(build_tools)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("apksigner"))
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        candidates.sort();
        if let Some(path) = candidates.pop() {
            return path;
        }
    }
    PathBuf::from("apksigner")
}

fn run_android_signer_command(
    program: &Path,
    args: &[String],
    inputs: &AndroidSigningInputs,
    action: &str,
) -> anyhow::Result<()> {
    let output = Command::new(program)
        .args(args)
        .env(ANDROID_SIGNER_STORE_PASSWORD_ENV, &inputs.keystore_password)
        .env(ANDROID_SIGNER_KEY_PASSWORD_ENV, &inputs.key_password)
        .output()
        .map_err(|error| anyhow::anyhow!("failed to {action}: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!("failed to {action}: {stderr}");
    }
    Ok(())
}

fn scrub_managed_android_signing_env(command: &mut tokio::process::Command) {
    for key in MANAGED_ANDROID_SIGNING_ENV_KEYS {
        command.env_remove(key);
    }
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
    command: &str,
    inputs: &AndroidSigningInputs,
) -> anyhow::Result<Vec<PathBuf>> {
    let extension = android_artifact_extension(command)
        .ok_or_else(|| anyhow::anyhow!("unsupported Android signing command"))?;
    let artifacts = android_artifacts_for_signing(workspace, extension)?;
    let signing_dir = create_private_workspace_in(&std::env::temp_dir(), "android-signer")?;
    let _cleanup = PrivateSigningDirectory {
        path: signing_dir.clone(),
    };
    let keystore_path = signing_dir.join("managed-keystore.jks");
    write_private_file(&keystore_path, &inputs.keystore_bytes)?;
    for (index, artifact) in artifacts.iter().enumerate() {
        let signed_artifact = signing_dir.join(format!("signed-{index}.{extension}"));
        if extension == "apk" {
            let apksigner = find_apksigner();
            run_android_signer_command(
                &apksigner,
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
                &apksigner,
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
            let strip = Command::new("zip")
                .args([
                    "-d",
                    signed_artifact.to_str().unwrap_or_default(),
                    "META-INF/*.SF",
                    "META-INF/*.RSA",
                    "META-INF/*.DSA",
                    "META-INF/*.EC",
                    "META-INF/MANIFEST.MF",
                ])
                .output()
                .context("failed to strip existing AAB signatures")?;
            if !strip.status.success() && strip.status.code() != Some(12) {
                anyhow::bail!(
                    "failed to strip existing AAB signatures: {}",
                    String::from_utf8_lossy(&strip.stderr).trim()
                );
            }
            run_android_signer_command(
                Path::new("jarsigner"),
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
                Path::new("jarsigner"),
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
    original_default_keychain: String,
    original_keychains: Vec<String>,
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
    let output = Command::new("/usr/bin/security")
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("failed to execute security command: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("security command failed: {stderr}");
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_security_command_with_strings(args: &[String]) -> anyhow::Result<String> {
    let output = Command::new("/usr/bin/security")
        .args(args)
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
                journal.original_default_keychain.clone(),
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

    if journal.keychain_path.exists() {
        if let Err(error) = run_security_command_with_strings(&[
            "delete-keychain".to_string(),
            keychain_path.clone(),
        ]) {
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
    workspace: &Path,
    bundle: &RunnerIosSigningBundle,
) -> anyhow::Result<(IosSigningMaterialization, IosSigningCleanup)> {
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

    let signing_dir = workspace.join(IOS_SIGNING_DIR);
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

    let home = std::env::var("HOME")
        .map_err(|_| anyhow::anyhow!("HOME environment variable is not set"))?;
    let installed_profiles_dir = PathBuf::from(home)
        .join("Library")
        .join("MobileDevice")
        .join("Provisioning Profiles");
    fs::create_dir_all(&installed_profiles_dir).map_err(|e| {
        anyhow::anyhow!(
            "failed to create provisioning profiles directory {}: {e}",
            installed_profiles_dir.display()
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
        let installed_path = installed_profiles_dir.join(format!("{profile_ref}.mobileprovision"));
        prepared_profiles.push((
            profile.bundle_id.clone(),
            profile_ref,
            work_path,
            installed_path,
            profile_bytes,
        ));
    }

    let original_keychains =
        parse_keychain_list(&run_security_command(&["list-keychains", "-d", "user"])?);
    let original_default_keychain =
        parse_keychain_list(&run_security_command(&["default-keychain", "-d", "user"])?)
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no default user keychain is configured"))?;
    let journal = IosCleanupJournal {
        keychain_path: keychain_path.clone(),
        original_default_keychain,
        original_keychains,
        installed_profiles: prepared_profiles
            .iter()
            .map(|(_, _, _, installed_path, _)| installed_path.clone())
            .collect(),
    };
    let journal_path = workspace.join(IOS_CLEANUP_JOURNAL);
    write_ios_cleanup_journal(&journal_path, &journal)?;

    let install_result: anyhow::Result<IosSigningMaterialization> = (|| {
        let mut bundle_profile_mapping = Vec::new();
        let mut bundle_profile_paths = Vec::new();
        for (bundle_id, profile_ref, work_path, installed_path, profile_bytes) in &prepared_profiles
        {
            write_private_file(installed_path, profile_bytes).map_err(|e| {
                anyhow::anyhow!(
                    "failed to install provisioning profile {}: {e}",
                    installed_path.display()
                )
            })?;
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

        // Match Codemagic's proven keychain layout: keep the user's normal
        // keychains available for Apple's public trust chain and append the
        // isolated build keychain that owns the private signing identity.
        let mut build_keychain_search_list = vec![
            "list-keychains".to_string(),
            "-d".to_string(),
            "user".to_string(),
            "-s".to_string(),
        ];
        build_keychain_search_list.extend(journal.original_keychains.iter().cloned());
        build_keychain_search_list.push(keychain_path_str.clone());
        run_security_command_with_strings(&build_keychain_search_list)?;
        run_security_command_with_strings(&[
            "default-keychain".to_string(),
            "-d".to_string(),
            "user".to_string(),
            "-s".to_string(),
            keychain_path_str.clone(),
        ])?;

        run_security_command_with_strings(&[
            "import".to_string(),
            p12_path.display().to_string(),
            "-f".to_string(),
            "pkcs12".to_string(),
            "-k".to_string(),
            keychain_path_str.clone(),
            "-P".to_string(),
            bundle.p12_password.clone(),
            "-A".to_string(),
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

fn run_ios_signing_tool(program: &str, args: Vec<String>, action: &str) -> anyhow::Result<String> {
    let output = Command::new(program)
        .args(&args)
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
                "; the runner is outside an interactive macOS login session. Register it as an external runner and run `oore runner install-service`",
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

    let mut app_extensions = Vec::new();
    collect_paths_with_extension(&app, "appex", &mut app_extensions)?;

    let mut nested_code = Vec::new();
    collect_nested_code(&app, &mut nested_code)?;
    nested_code.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in nested_code {
        codesign_path(&path, materialization, None)?;
    }

    app_extensions.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for extension in &app_extensions {
        let bundle_id = read_apple_bundle_identifier(extension)?;
        let profile = profile_path_for_bundle(materialization, &bundle_id)?;
        fs::copy(profile, extension.join("embedded.mobileprovision"))
            .map_err(|error| anyhow::anyhow!("failed to embed profile for {bundle_id}: {error}"))?;
        let entitlements = entitlements_dir.join(format!(
            "{}.plist",
            xcode_build_setting_identifier(&bundle_id)
        ));
        extract_profile_entitlements(profile, &entitlements)?;
        codesign_path(extension, materialization, Some(&entitlements))?;
    }

    let app_bundle_id = read_apple_bundle_identifier(&app)?;
    let app_profile = profile_path_for_bundle(materialization, &app_bundle_id)?;
    fs::copy(app_profile, app.join("embedded.mobileprovision"))
        .map_err(|error| anyhow::anyhow!("failed to embed profile for {app_bundle_id}: {error}"))?;
    let app_entitlements = entitlements_dir.join(format!(
        "{}.plist",
        xcode_build_setting_identifier(&app_bundle_id)
    ));
    extract_profile_entitlements(app_profile, &app_entitlements)?;
    codesign_path(&app, materialization, Some(&app_entitlements))?;

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
    anyhow::ensure!(
        journal
            .original_keychains
            .contains(&journal.original_default_keychain),
        "iOS cleanup journal has no original default keychain"
    );
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
    let output = Command::new("/usr/bin/id").arg("-u").output()?;
    anyhow::ensure!(output.status.success(), "failed to determine runner uid");
    String::from_utf8(output.stdout)?
        .trim()
        .parse()
        .context("invalid uid returned by /usr/bin/id")
}

fn reconcile_stale_workspaces_with(
    parent: &Path,
    runner_id: &str,
    mut reconcile_journal: impl FnMut(&Path, &IosCleanupJournal) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let prefix = runner_workspace_prefix(runner_id);
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
        let Some(random_suffix) = file_name.strip_prefix(&prefix) else {
            continue;
        };
        if random_suffix.len() != 32 || !random_suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
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

        let journal_path = workspace.join(IOS_CLEANUP_JOURNAL);
        if journal_path.exists() {
            let journal: IosCleanupJournal =
                serde_json::from_slice(&fs::read(&journal_path).with_context(|| {
                    format!(
                        "failed to read iOS cleanup journal {}",
                        journal_path.display()
                    )
                })?)
                .with_context(|| {
                    format!(
                        "failed to parse iOS cleanup journal {}",
                        journal_path.display()
                    )
                })?;
            reconcile_journal(&workspace, &journal)?;
            fs::remove_file(&journal_path)?;
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

fn reconcile_stale_runner_mutations(runner_id: &str) -> anyhow::Result<()> {
    // ponytail: one process per runner ID; add a process lease if replicas become supported.
    ensure_legacy_workspace_has_no_residue(Path::new(LEGACY_BUILD_WORKSPACE_ROOT))?;
    reconcile_stale_workspaces_with(&std::env::temp_dir(), runner_id, |workspace, journal| {
        validate_ios_cleanup_journal(workspace, journal)?;
        cleanup_ios_signing_state(journal)
    })
}

pub async fn run_runner_forever(
    config: RunnerConfig,
    daemon_url_override: Option<String>,
) -> anyhow::Result<()> {
    let daemon_url = daemon_url_override.unwrap_or(config.daemon_url.clone());
    require_safe_daemon_url(&daemon_url)?;
    let client = reqwest::Client::new();

    reconcile_stale_runner_mutations(&config.runner_id)
        .context("failed to reconcile stale runner state before startup")?;

    println!("Starting runner '{}' ({})", config.name, config.runner_id);
    println!("Connecting to: {}", daemon_url);

    let capabilities = detect_capabilities().await;

    // Send one heartbeat immediately so runner status appears online
    // without waiting for the first interval tick.
    let _ = client
        .post(format!(
            "{}/v1/runners/{}/heartbeat",
            daemon_url, config.runner_id
        ))
        .bearer_auth(&config.runner_token)
        .json(&serde_json::json!({ "status": "online", "capabilities": capabilities }))
        .send()
        .await;

    let hb_client = client.clone();
    let hb_url = daemon_url.clone();
    let hb_token = config.runner_token.clone();
    let hb_runner_id = config.runner_id.clone();
    let hb_capabilities = capabilities.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let _ = hb_client
                .post(format!("{}/v1/runners/{}/heartbeat", hb_url, hb_runner_id))
                .bearer_auth(&hb_token)
                .json(&serde_json::json!({ "status": "online", "capabilities": hb_capabilities }))
                .send()
                .await;
        }
    });

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        if let Err(error) = reconcile_stale_runner_mutations(&config.runner_id) {
            eprintln!("Refusing to claim work until stale runner state is cleaned: {error:#}");
            tokio::time::sleep(Duration::from_secs(10)).await;
            continue;
        }
        match claim_and_execute(&client, &daemon_url, &config).await {
            Ok(_executed) => {}
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
        anyhow::bail!("Claim request failed: {}", resp.status());
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

#[derive(Debug, Clone)]
struct ResolvedExecutionPlan {
    stage_commands: PipelineCommandStages,
    artifact_patterns: Vec<String>,
    env: Vec<PipelineEnvVar>,
    source: String,
}

fn validate_command_list(stage: &str, commands: &[String]) -> anyhow::Result<Vec<String>> {
    let mut cleaned = Vec::with_capacity(commands.len());
    for (idx, command) in commands.iter().enumerate() {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            anyhow::bail!("commands.{stage}[{idx}] must not be empty");
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

fn validate_artifact_patterns(patterns: &[String]) -> anyhow::Result<Vec<String>> {
    let mut cleaned = Vec::with_capacity(patterns.len());
    for (idx, pattern) in patterns.iter().enumerate() {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            anyhow::bail!("artifacts.patterns[{idx}] must not be empty");
        }
        validate_artifact_pattern(trimmed)
            .map_err(|error| anyhow::anyhow!("artifacts.patterns[{idx}] {error}"))?;
        cleaned.push(trimmed.to_string());
    }
    Ok(cleaned)
}

fn validate_platform_args(args: &[String], path: &str) -> anyhow::Result<Vec<String>> {
    let mut cleaned = Vec::with_capacity(args.len());
    for (idx, arg) in args.iter().enumerate() {
        let trimmed = arg.trim();
        if trimmed.is_empty() {
            anyhow::bail!("{path}[{idx}] must not be empty");
        }
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

fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != '_' && !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
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
        if !is_valid_env_key(key) {
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
        pre_build: validate_command_list("pre_build", &config.commands.pre_build)?,
        build: validate_command_list("build", &config.commands.build)?,
        post_build: validate_command_list("post_build", &config.commands.post_build)?,
    };
    let platform_build_args = PlatformBuildArgs {
        android: validate_platform_args(
            &config.platform_build_args.android,
            "platform_build_args.android",
        )?,
        ios: validate_platform_args(&config.platform_build_args.ios, "platform_build_args.ios")?,
        macos: validate_platform_args(
            &config.platform_build_args.macos,
            "platform_build_args.macos",
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

fn parse_repo_config_file(raw: &str) -> anyhow::Result<PipelineExecutionConfig> {
    parse_repository_pipeline_yaml(raw).map_err(anyhow::Error::msg)
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
    let raw = snapshot.get("ui_execution_config");
    if raw.is_none() {
        return Ok(PipelineExecutionConfig::default());
    }
    let parsed: PipelineExecutionConfig = serde_json::from_value(raw.cloned().unwrap_or_default())
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
        let file_config = parse_repo_config_file(&content).map_err(|e| {
            anyhow::anyhow!("Invalid pipeline config in {}: {}", full_path.display(), e)
        })?;
        let file_config = apply_run_platform_selection(file_config, snapshot)?;
        let resolved_flutter_version = fvmrc_version
            .clone()
            .or_else(|| file_config.flutter_version.clone());
        let include_defaults = file_config.commands.build.is_empty();
        let mut stage_commands = materialize_stage_commands(&file_config, include_defaults);
        if let Some(version) = resolved_flutter_version {
            stage_commands = apply_fvm_wrappers(stage_commands);
            stage_commands
                .pre_build
                .insert(0, format!("fvm use {version} --force"));
        }

        return Ok(ResolvedExecutionPlan {
            stage_commands,
            artifact_patterns: file_config.artifact_patterns,
            env: file_config.env,
            source: format!("file:{}", full_path.display()),
        });
    }

    let fallback = apply_run_platform_selection(load_ui_execution_config(snapshot)?, snapshot)?;
    let resolved_flutter_version = fvmrc_version.or_else(|| fallback.flutter_version.clone());
    let mut stage_commands = materialize_stage_commands(&fallback, true);
    if let Some(version) = resolved_flutter_version {
        stage_commands = apply_fvm_wrappers(stage_commands);
        stage_commands
            .pre_build
            .insert(0, format!("fvm use {version} --force"));
    }
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

async fn execute_build(
    job: &ClaimedJob,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
) -> (Vec<StepResult>, anyhow::Result<()>) {
    let workspace = match create_private_workspace_in(&std::env::temp_dir(), &config.runner_id) {
        Ok(workspace) => workspace,
        Err(error) => return (vec![], Err(error.into())),
    };
    try_mark_no_spotlight_index(&workspace);

    let _cleanup = WorkspaceCleanup {
        path: workspace.clone(),
    };
    let authority = Arc::new(BuildAuthorityState::default());

    let snapshot = &job.config_snapshot;
    let mut steps = Vec::new();
    let mut log_seq: i64 = 0;

    if let Err(e) = check_build_active(client, daemon_url, config, &job.build_id, &authority).await
    {
        return (steps, Err(e));
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

    let mut checkout_child = tokio::process::Command::new("sh");
    checkout_child
        .arg("-c")
        .arg(&checkout.shell_script)
        .current_dir(&workspace)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    for (key, value) in &checkout.env {
        checkout_child.env(key, value);
    }

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

    let mut signing_source: Option<&str> = None;
    let mut signing_variant: Option<AndroidSigningBuildType> = None;
    let mut signing_inputs: Option<AndroidSigningInputs> = None;
    let mut ios_signing_source: Option<&str> = None;
    let mut ios_signing_bundle: Option<RunnerIosSigningBundle> = None;
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

    if build_commands
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
                if let Some(bundle) = server_payload.bundle {
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

            let mut step_cmd = tokio::process::Command::new("sh");
            step_cmd
                .arg("-c")
                .arg(&normalized_command)
                .current_dir(&workspace)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            for (key, value) in &step_env {
                step_cmd.env(key, value);
            }
            scrub_managed_android_signing_env(&mut step_cmd);
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
                        let signing_result = sign_android_artifacts(&workspace, command, inputs);
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
                            match install_ios_signing_bundle(workspace.as_path(), bundle) {
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

fn compute_file_sha256(path: &std::path::Path) -> anyhow::Result<String> {
    use std::io::Read;
    let file = fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
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
        let file_size = fs::metadata(path).map(|m| m.len() as i64).ok();
        let checksum = Some(compute_file_sha256(path)?);

        let metadata = match ios_metadata {
            Some(value) if artifact_type == "ipa" => value.clone(),
            _ => serde_json::json!({}),
        };

        let body = serde_json::json!({
            "name": name,
            "artifact_type": artifact_type,
            "file_size": file_size,
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
            let bytes = tokio::fs::read(path).await?;
            let response = client.put(&upload_url).body(bytes).send().await?;
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
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[derive(Debug, Clone)]
    struct NestedSubmoduleFixture {
        root_repo: PathBuf,
        child_repo: PathBuf,
        default_branch: String,
        head_sha: String,
    }

    fn temp_workspace() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = TEMP_WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "oore-runner-test-{}-{nanos}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[tokio::test]
    async fn reported_capabilities_include_runner_version() {
        let capabilities = detect_capabilities().await;
        assert!(
            capabilities
                .get("version")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|version| !version.is_empty())
        );
    }

    #[test]
    fn daemon_url_requires_https_except_literal_loopback() {
        for allowed in [
            "https://ci.example.com",
            "https://127.0.0.1:8787",
            "http://127.0.0.1:8787",
            "http://[::1]:8787",
        ] {
            require_safe_daemon_url(allowed).expect(allowed);
        }

        for rejected in [
            "http://localhost:8787",
            "http://192.0.2.10:8787",
            "http://[2001:db8::10]:8787",
            "ftp://127.0.0.1:8787",
            "not-a-url",
        ] {
            assert!(
                require_safe_daemon_url(rejected).is_err(),
                "unexpectedly allowed {rejected}"
            );
        }
    }

    #[tokio::test]
    async fn runner_runtime_rejects_cleartext_remote_override_before_connecting() {
        let config = RunnerConfig {
            runner_id: "runner-test".to_string(),
            runner_token: "secret".to_string(),
            daemon_url: "https://daemon.example".to_string(),
            name: "test".to_string(),
        };
        let error = run_runner_forever(config, Some("http://192.0.2.10:8787".to_string()))
            .await
            .expect_err("remote cleartext override must fail");
        assert!(error.to_string().contains("cleartext daemon URLs"));
    }

    #[test]
    fn authority_loss_stops_definitive_revocations_and_bounds_outages() {
        for (status, expected) in [
            (reqwest::StatusCode::UNAUTHORIZED, "runner_unauthorized"),
            (reqwest::StatusCode::FORBIDDEN, "assignment_lost"),
            (reqwest::StatusCode::NOT_FOUND, "build_missing"),
        ] {
            let error = authority_loss(status);
            assert_eq!(
                error
                    .downcast_ref::<BuildTerminated>()
                    .expect("authority loss must terminate the build")
                    .status,
                expected
            );
        }

        let authority = BuildAuthorityState::default();
        assert!(authority.transient_failure().is_ok());
        assert!(authority.transient_failure().is_ok());
        authority.confirmed_active();
        assert!(authority.transient_failure().is_ok());
        assert!(authority.transient_failure().is_ok());
        let error = authority
            .transient_failure()
            .expect_err("the transient grace budget must be finite");
        assert_eq!(
            error
                .downcast_ref::<BuildTerminated>()
                .expect("grace exhaustion must terminate the build")
                .status,
            "controller_unavailable"
        );
        assert!(is_transient_authority_status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(!is_transient_authority_status(
            reqwest::StatusCode::BAD_REQUEST
        ));
    }

    #[cfg(unix)]
    #[test]
    fn private_workspace_ignores_legacy_symlink_and_excludes_other_users() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let parent = temp_workspace();
        let attacker_target = parent.join("attacker-target");
        fs::create_dir(&attacker_target).expect("create synthetic symlink target");
        symlink(&attacker_target, parent.join("oore-builds.noindex"))
            .expect("create legacy workspace symlink");

        let first = create_private_workspace_in(&parent, "runner-security-test")
            .expect("create first private workspace");
        let second = create_private_workspace_in(&parent, "runner-security-test")
            .expect("create second private workspace");
        fs::write(first.join("marker"), b"runner-owned").expect("write workspace marker");

        assert_ne!(first, second);
        assert_eq!(
            fs::metadata(&first)
                .expect("stat private workspace")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert!(!attacker_target.join("marker").exists());
        cleanup_workspace(&parent);
    }

    #[cfg(unix)]
    #[test]
    fn legacy_and_journal_paths_fail_closed_before_cleanup() {
        use std::os::unix::fs::symlink;

        let parent = temp_workspace();
        let legacy = parent.join("oore-builds.noindex");
        fs::create_dir(&legacy).expect("create clean legacy root");
        fs::write(legacy.join(SPOTLIGHT_NO_INDEX_SENTINEL), b"").expect("write legacy sentinel");
        ensure_legacy_workspace_has_no_residue(&legacy).expect("clean legacy root remains valid");
        fs::create_dir(legacy.join("stale-build")).expect("create stale legacy build");
        assert!(ensure_legacy_workspace_has_no_residue(&legacy).is_err());
        fs::remove_dir_all(&legacy).expect("remove legacy fixture");
        let target = parent.join("legacy-target");
        fs::create_dir(&target).expect("create legacy symlink target");
        symlink(&target, &legacy).expect("create legacy symlink");
        assert!(ensure_legacy_workspace_has_no_residue(&legacy).is_err());

        let workspace = parent.join("workspace");
        let profile_root = PathBuf::from(std::env::var("HOME").expect("HOME is set"))
            .join("Library/MobileDevice/Provisioning Profiles");
        let valid = IosCleanupJournal {
            keychain_path: workspace
                .join(IOS_SIGNING_DIR)
                .join("oore-ci-build.keychain-db"),
            original_default_keychain: "/placeholder/login.keychain-db".to_string(),
            original_keychains: vec!["/placeholder/login.keychain-db".to_string()],
            installed_profiles: vec![profile_root.join("placeholder.mobileprovision")],
        };
        validate_ios_cleanup_journal(&workspace, &valid).expect("valid journal paths");
        let mut escaped = valid;
        escaped.installed_profiles = vec![profile_root.join("../escaped.mobileprovision")];
        assert!(validate_ios_cleanup_journal(&workspace, &escaped).is_err());
        cleanup_workspace(&parent);
    }

    #[cfg(unix)]
    #[test]
    fn private_file_replaces_public_files_and_symlinks_at_mode_0600() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

        let workspace = temp_workspace();
        let env_path = workspace.join(".env");
        fs::write(&env_path, b"PUBLIC_PLACEHOLDER=1\n").expect("write public placeholder");
        fs::set_permissions(&env_path, fs::Permissions::from_mode(0o644))
            .expect("set permissive fixture mode");
        let original_inode = fs::metadata(&env_path).expect("stat public fixture").ino();

        write_private_file(&env_path, b"PIPELINE_PLACEHOLDER=2\n")
            .expect("replace with private environment file");
        let metadata = fs::metadata(&env_path).expect("stat private environment file");
        assert_ne!(metadata.ino(), original_inode);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(
            fs::read_to_string(&env_path).expect("read private environment file"),
            "PIPELINE_PLACEHOLDER=2\n"
        );

        let symlink_target = workspace.join("unrelated");
        fs::write(&symlink_target, b"UNCHANGED").expect("write symlink target");
        fs::remove_file(&env_path).expect("remove first private file");
        symlink(&symlink_target, &env_path).expect("replace env with symlink fixture");
        write_private_file(&env_path, b"PIPELINE_PLACEHOLDER=3\n")
            .expect("replace symlink without following it");
        assert_eq!(
            fs::read_to_string(&symlink_target).expect("read untouched target"),
            "UNCHANGED"
        );
        assert!(
            !fs::symlink_metadata(&env_path)
                .expect("stat replaced symlink")
                .file_type()
                .is_symlink()
        );
        cleanup_workspace(&workspace);
    }

    #[cfg(unix)]
    #[test]
    fn stale_signing_journal_blocks_then_reconciles_before_later_work() {
        use std::os::unix::fs::PermissionsExt;

        let parent = temp_workspace();
        let runner_id = "runner-reconciliation-test";
        let workspace =
            create_private_workspace_in(&parent, runner_id).expect("create stale workspace");
        fs::create_dir_all(workspace.join(IOS_SIGNING_DIR)).expect("create signing directory");
        let owned_profile = parent.join("generation-a.mobileprovision");
        let unrelated = parent.join("generation-b.mobileprovision");
        fs::write(&owned_profile, b"GENERATION_A").expect("write owned placeholder");
        fs::write(&unrelated, b"GENERATION_B").expect("write unrelated placeholder");
        let journal = IosCleanupJournal {
            keychain_path: workspace
                .join(IOS_SIGNING_DIR)
                .join("oore-ci-build.keychain-db"),
            original_default_keychain: "/placeholder/login.keychain-db".to_string(),
            original_keychains: vec!["/placeholder/login.keychain-db".to_string()],
            installed_profiles: vec![owned_profile.clone()],
        };
        let journal_path = workspace.join(IOS_CLEANUP_JOURNAL);
        write_ios_cleanup_journal(&journal_path, &journal).expect("write durable journal");
        assert_eq!(
            fs::metadata(&journal_path)
                .expect("stat cleanup journal")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        drop(WorkspaceCleanup {
            path: workspace.clone(),
        });
        assert!(workspace.exists(), "journal must preserve cleanup evidence");

        let blocked = reconcile_stale_workspaces_with(&parent, runner_id, |_, _| {
            anyhow::bail!("synthetic cleanup failure")
        });
        assert!(blocked.is_err());
        assert!(workspace.exists());
        assert!(journal_path.exists());

        reconcile_stale_workspaces_with(&parent, runner_id, |seen_workspace, seen_journal| {
            assert_eq!(seen_workspace, workspace);
            assert_eq!(seen_journal.installed_profiles, vec![owned_profile.clone()]);
            fs::remove_file(&owned_profile)?;
            Ok(())
        })
        .expect("reconcile stale generation");
        assert!(!workspace.exists());
        assert!(!owned_profile.exists());
        assert_eq!(
            fs::read_to_string(&unrelated).expect("read newer generation placeholder"),
            "GENERATION_B"
        );
        cleanup_workspace(&parent);
    }

    #[test]
    fn local_artifact_uploads_use_the_runner_daemon_url() {
        assert_eq!(
            runner_artifact_upload_url(
                "http://127.0.0.1:8787",
                "https://ci.example.com/v1/artifacts/local-upload/token"
            ),
            "http://127.0.0.1:8787/v1/artifacts/local-upload/token"
        );
        assert_eq!(
            runner_artifact_upload_url(
                "http://127.0.0.1:8787",
                "https://s3.example.com/bucket/artifact?signature=abc"
            ),
            "https://s3.example.com/bucket/artifact?signature=abc"
        );
    }

    fn cleanup_workspace(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    fn run_git(
        cwd: &Path,
        args: &[&str],
        extra_env: &[(&str, &str)],
    ) -> anyhow::Result<std::process::Output> {
        let mut cmd = Command::new("git");
        cmd.current_dir(cwd).args(args);
        for (key, value) in extra_env {
            cmd.env(key, value);
        }
        let output = cmd.output().map_err(|e| {
            anyhow::anyhow!("failed to run git {:?} in {}: {e}", args, cwd.display())
        })?;
        Ok(output)
    }

    fn git_expect_success(cwd: &Path, args: &[&str], extra_env: &[(&str, &str)]) {
        let output = run_git(cwd, args, extra_env).expect("git command should run");
        if output.status.success() {
            return;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "git command failed in {}: git {:?}\n{}",
            cwd.display(),
            args,
            stderr
        );
    }

    fn git_stdout(cwd: &Path, args: &[&str]) -> String {
        let output = run_git(cwd, args, &[]).expect("git command should run");
        assert!(output.status.success(), "git {:?} failed", args);
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn init_repo(path: &Path) {
        fs::create_dir_all(path).expect("create repo directory");
        git_expect_success(path, &["init", "--quiet"], &[]);
        git_expect_success(
            path,
            &["config", "user.email", "runner-tests@example.com"],
            &[],
        );
        git_expect_success(path, &["config", "user.name", "oore-runner-tests"], &[]);
    }

    fn add_commit(path: &Path, file_rel: &str, content: &str, message: &str) {
        let file_path = path.join(file_rel);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(&file_path, content).expect("write file");
        git_expect_success(path, &["add", "."], &[]);
        git_expect_success(path, &["commit", "--quiet", "-m", message], &[]);
    }

    fn create_nested_submodule_fixture(base: &Path) -> NestedSubmoduleFixture {
        let grandchild_repo = base.join("grandchild-repo");
        init_repo(&grandchild_repo);
        add_commit(
            &grandchild_repo,
            "README.md",
            "nested-submodule-grandchild\n",
            "init grandchild",
        );

        let child_repo = base.join("child-repo");
        init_repo(&child_repo);
        add_commit(&child_repo, "README.md", "child\n", "init child");
        let grandchild_path = grandchild_repo
            .to_str()
            .expect("grandchild path must be UTF-8")
            .to_string();
        git_expect_success(
            &child_repo,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &grandchild_path,
                "deps/grandchild",
            ],
            &[],
        );
        git_expect_success(
            &child_repo,
            &["commit", "--quiet", "-am", "add nested submodule"],
            &[],
        );

        let root_repo = base.join("root-repo");
        init_repo(&root_repo);
        add_commit(&root_repo, "README.md", "root\n", "init root");
        let child_path = child_repo
            .to_str()
            .expect("child path must be UTF-8")
            .to_string();
        git_expect_success(
            &root_repo,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &child_path,
                "deps/child",
            ],
            &[],
        );
        git_expect_success(
            &root_repo,
            &["commit", "--quiet", "-am", "add child submodule"],
            &[],
        );

        let default_branch = git_stdout(&root_repo, &["rev-parse", "--abbrev-ref", "HEAD"]);
        let head_sha = git_stdout(&root_repo, &["rev-parse", "HEAD"]);

        NestedSubmoduleFixture {
            root_repo,
            child_repo,
            default_branch,
            head_sha,
        }
    }

    fn run_checkout_for_test(
        checkout: &CheckoutInvocation,
        workspace: &Path,
    ) -> std::process::Output {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(&checkout.shell_script)
            .current_dir(workspace)
            // Local fixture repos use file:// transport for submodule resolution.
            .env("GIT_ALLOW_PROTOCOL", "file:git:http:https:ssh");
        for (key, value) in &checkout.env {
            command.env(key, value);
        }
        command.output().expect("checkout command should run")
    }

    #[test]
    fn checkout_invocation_includes_recursive_submodule_commands() {
        let commit =
            build_checkout_invocation("https://example.com/repo.git", Some("abc123"), None)
                .expect("commit checkout invocation");
        assert!(
            commit
                .preview_command
                .contains("git submodule sync --recursive")
        );
        assert!(
            commit
                .preview_command
                .contains("git submodule update --init --recursive")
        );
        assert!(
            commit
                .shell_script
                .contains("[oore-checkout] updating submodules (init + recursive)")
        );

        let branch = build_checkout_invocation("https://example.com/repo.git", None, Some("main"))
            .expect("branch checkout invocation");
        assert!(
            branch
                .preview_command
                .contains("git clone --depth 1 --branch main")
        );
        assert!(
            branch
                .preview_command
                .contains("git submodule update --init --recursive")
        );
    }

    #[test]
    fn checkout_proxy_scopes_runner_token_to_daemon_url() {
        let mut checkout =
            build_checkout_invocation("http://127.0.0.1:8787/proxy/repo.git", None, Some("main"))
                .expect("checkout invocation");
        add_checkout_proxy_config(
            &mut checkout,
            "http://127.0.0.1:8787/proxy/repo.git",
            "runner-secret",
        );

        assert!(checkout.env.contains(&(
            "GIT_CONFIG_KEY_0".to_string(),
            "http.http://127.0.0.1:8787/proxy/repo.git.extraHeader".to_string(),
        )));
        assert!(
            !checkout
                .env
                .iter()
                .any(|(_, value)| value == "http.extraHeader")
        );
        assert!(!checkout.preview_command.contains("runner-secret"));
        assert!(!checkout.shell_script.contains("runner-secret"));
    }

    #[test]
    fn checkout_branch_materializes_nested_submodules() {
        let fixture_root = temp_workspace();
        let fixture = create_nested_submodule_fixture(&fixture_root);
        let workspace = fixture_root.join("checkout-branch");
        fs::create_dir_all(&workspace).expect("create checkout workspace");

        let checkout = build_checkout_invocation(
            fixture.root_repo.to_str().expect("root path"),
            None,
            Some(&fixture.default_branch),
        )
        .expect("build checkout invocation");
        let output = run_checkout_for_test(&checkout, &workspace);
        assert!(
            output.status.success(),
            "checkout failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(workspace.join("deps/child/README.md").exists());
        assert!(
            workspace
                .join("deps/child/deps/grandchild/README.md")
                .exists()
        );
        cleanup_workspace(&fixture_root);
    }

    #[test]
    fn checkout_sha_materializes_nested_submodules() {
        let fixture_root = temp_workspace();
        let fixture = create_nested_submodule_fixture(&fixture_root);
        add_commit(
            &fixture.root_repo,
            "AFTER_PIN.md",
            "created after the build was queued\n",
            "advance branch after pin",
        );
        let workspace = fixture_root.join("checkout-sha");
        fs::create_dir_all(&workspace).expect("create checkout workspace");

        let checkout = build_checkout_invocation(
            fixture.root_repo.to_str().expect("root path"),
            Some(&fixture.head_sha),
            None,
        )
        .expect("build checkout invocation");
        let output = run_checkout_for_test(&checkout, &workspace);
        assert!(
            output.status.success(),
            "checkout failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(workspace.join("deps/child/README.md").exists());
        assert!(!workspace.join("AFTER_PIN.md").exists());
        assert!(
            workspace
                .join("deps/child/deps/grandchild/README.md")
                .exists()
        );
        cleanup_workspace(&fixture_root);
    }

    #[test]
    fn checkout_surfaces_explicit_marker_when_submodule_update_fails() {
        let fixture_root = temp_workspace();
        let fixture = create_nested_submodule_fixture(&fixture_root);
        let broken_url = "/tmp/definitely-missing-oore-submodule";
        let gitmodules = fixture.root_repo.join(".gitmodules");
        let original_gitmodules = fs::read_to_string(&gitmodules).expect("read .gitmodules");
        let updated_gitmodules = original_gitmodules
            .replace(fixture.child_repo.to_str().expect("child path"), broken_url);
        fs::write(&gitmodules, updated_gitmodules).expect("write .gitmodules");
        git_expect_success(&fixture.root_repo, &["add", ".gitmodules"], &[]);
        git_expect_success(
            &fixture.root_repo,
            &["commit", "--quiet", "-m", "break submodule url"],
            &[],
        );

        let workspace = fixture_root.join("checkout-broken-submodule");
        fs::create_dir_all(&workspace).expect("create checkout workspace");
        let checkout = build_checkout_invocation(
            fixture.root_repo.to_str().expect("root path"),
            None,
            Some(&fixture.default_branch),
        )
        .expect("build checkout invocation");
        let output = run_checkout_for_test(&checkout, &workspace);
        assert!(!output.status.success(), "checkout unexpectedly succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("[oore-checkout] submodule update failed"));
        cleanup_workspace(&fixture_root);
    }

    #[test]
    fn repository_stages_scrub_all_managed_android_signing_environment() {
        assert_eq!(MANAGED_ANDROID_SIGNING_ENV_KEYS.len(), 6);
        for required in [
            OORE_ANDROID_KEYSTORE_PATH_ENV,
            OORE_ANDROID_KEYSTORE_B64_ENV,
            OORE_ANDROID_KEYSTORE_PASSWORD_ENV,
            OORE_ANDROID_KEY_ALIAS_ENV,
            OORE_ANDROID_KEY_PASSWORD_ENV,
            OORE_ANDROID_KEY_PROPERTIES_PATH_ENV,
        ] {
            assert!(MANAGED_ANDROID_SIGNING_ENV_KEYS.contains(&required));
        }
    }

    #[tokio::test]
    async fn repository_child_process_cannot_inherit_managed_signing_values() {
        let mut command = tokio::process::Command::new("sh");
        command.arg("-c").arg("env");
        for key in MANAGED_ANDROID_SIGNING_ENV_KEYS {
            command.env(key, "managed-secret");
        }
        scrub_managed_android_signing_env(&mut command);
        let output = command.output().await.expect("run scrubbed child");
        assert!(output.status.success());
        let environment = String::from_utf8_lossy(&output.stdout);
        for key in MANAGED_ANDROID_SIGNING_ENV_KEYS {
            assert!(!environment.contains(&format!("{key}=")));
        }
    }

    #[test]
    fn android_signing_marker_exposes_no_reusable_authority() {
        let marker =
            android_signing_prepared_marker("pipeline_profile", AndroidSigningBuildType::Release);
        assert!(marker.contains("runner_owned_post_build_signer"));
        assert!(!marker.contains("password"));
        assert!(!marker.contains("keystore"));
        assert!(!marker.contains("key.properties"));
    }

    #[test]
    fn runner_profile_decodes_only_into_runner_owned_inputs() {
        let profile = RunnerAndroidSigningProfile {
            build_type: AndroidSigningBuildType::Release,
            enabled: true,
            keystore_filename: "release.jks".to_string(),
            keystore_base64: "ZmFrZS1rZXlzdG9yZS1ieXRlcw==".to_string(),
            store_password: "store-pass".to_string(),
            key_alias: "upload".to_string(),
            key_password: "key-pass".to_string(),
        };
        let inputs = signing_inputs_from_runner_profile(&profile).expect("runner inputs");
        assert_eq!(inputs.keystore_bytes, b"fake-keystore-bytes");
        assert_eq!(inputs.keystore_password, "store-pass");
    }

    #[test]
    fn android_signing_command_detection_covers_flutter_android_targets() {
        assert!(is_android_flutter_build_command(
            "flutter build apk --release"
        ));
        assert!(is_android_flutter_build_command(
            "fvm flutter build appbundle --release"
        ));
        assert!(!is_android_flutter_build_command(
            "flutter build ios --release"
        ));
    }

    #[test]
    fn android_signer_covers_every_split_artifact() {
        let workspace = temp_workspace();
        let outputs = workspace.join("build/app/outputs/flutter-apk");
        fs::create_dir_all(&outputs).expect("create Android outputs");
        for filename in ["app-arm64-v8a-release.apk", "app-x86_64-release.apk"] {
            fs::write(outputs.join(filename), b"unsigned").expect("write split APK");
        }
        fs::write(outputs.join("ignored.aab"), b"unsigned").expect("write other artifact");

        let artifacts =
            android_artifacts_for_signing(&workspace, "apk").expect("discover every split APK");
        assert_eq!(artifacts.len(), 2);
        assert!(
            artifacts
                .iter()
                .all(|path| path.extension().and_then(|value| value.to_str()) == Some("apk"))
        );
        cleanup_workspace(&workspace);
    }

    #[test]
    fn determines_android_signing_variant_from_commands() {
        let release = determine_android_signing_variant(&[
            "flutter build apk --release".to_string(),
            "echo done".to_string(),
        ])
        .expect("variant");
        assert_eq!(release, Some(AndroidSigningBuildType::Release));

        let debug = determine_android_signing_variant(&["flutter build apk --debug".to_string()])
            .expect("variant");
        assert_eq!(debug, Some(AndroidSigningBuildType::Debug));
    }

    #[test]
    fn rejects_mixed_android_signing_variants() {
        let err = determine_android_signing_variant(&[
            "flutter build apk --release".to_string(),
            "flutter build apk --debug".to_string(),
        ])
        .expect_err("mixed variant must fail");
        assert!(err.to_string().contains("mixed Android build variants"));
    }

    #[test]
    fn autodetect_prefers_dot_oore_yaml_over_yml() {
        let workspace = temp_workspace();
        fs::write(
            workspace.join(".oore.yaml"),
            "version: 1\nplatforms: [ios]\ncommands:\n  pre_build: []\n  build: []\n  post_build: []\nartifacts:\n  patterns: [\"*.ipa\"]\n",
        )
        .expect("write yaml");
        fs::write(
            workspace.join(".oore.yml"),
            "version: 1\nplatforms: [android]\ncommands:\n  pre_build: []\n  build: []\n  post_build: []\nartifacts:\n  patterns: [\"*.apk\"]\n",
        )
        .expect("write yml");

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "config_path": ".oore.yml",
            "ui_execution_config": {
                "platforms": ["android"],
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve plan");
        assert_eq!(
            plan.stage_commands.build,
            vec!["flutter build ios --release --no-codesign".to_string()]
        );
        assert_eq!(plan.artifact_patterns, vec!["*.ipa".to_string()]);

        cleanup_workspace(&workspace);
    }

    #[test]
    fn explicit_path_wins_over_auto_detect() {
        let workspace = temp_workspace();
        fs::create_dir_all(workspace.join("ci")).expect("mkdir ci");
        fs::write(
            workspace.join("ci/pipeline.yml"),
            "version: 1\nplatforms: [macos]\ncommands:\n  pre_build: []\n  build: []\n  post_build: []\nartifacts:\n  patterns: [\"*.zip\"]\n",
        )
        .expect("write explicit");
        fs::write(
            workspace.join(".oore.yaml"),
            "version: 1\nplatforms: [android]\ncommands:\n  pre_build: []\n  build: []\n  post_build: []\nartifacts:\n  patterns: [\"*.apk\"]\n",
        )
        .expect("write auto");

        let snapshot = serde_json::json!({
            "config_path_explicit": true,
            "config_path": "ci/pipeline.yml",
            "ui_execution_config": {
                "platforms": ["android"],
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve plan");
        assert_eq!(
            plan.stage_commands.build,
            vec!["flutter build macos --release".to_string()]
        );
        assert_eq!(plan.artifact_patterns, vec!["*.zip".to_string()]);

        cleanup_workspace(&workspace);
    }

    #[test]
    fn explicit_config_path_must_stay_within_workspace() {
        let workspace = temp_workspace();
        let snapshot = serde_json::json!({
            "config_path_explicit": true,
            "config_path": "../outside.oore.yaml",
        });

        let error = resolve_execution_plan(&workspace, &snapshot).expect_err("path must fail");
        assert!(
            error
                .to_string()
                .contains("Invalid explicit repository config path")
        );

        cleanup_workspace(&workspace);
    }

    #[cfg(unix)]
    #[test]
    fn repository_config_symlink_must_stay_within_workspace() {
        let workspace = temp_workspace();
        let outside = temp_workspace();
        let outside_config = outside.join(".oore.yaml");
        fs::write(
            &outside_config,
            "version: 1\nplatforms: [android]\nartifacts:\n  patterns: [\"*.apk\"]\n",
        )
        .expect("write outside config");
        std::os::unix::fs::symlink(&outside_config, workspace.join(".oore.yaml"))
            .expect("link outside config");

        let error = resolve_execution_plan(&workspace, &serde_json::json!({}))
            .expect_err("symlink escape must fail");
        assert!(
            error
                .to_string()
                .contains("resolves outside the build workspace")
        );

        cleanup_workspace(&workspace);
        cleanup_workspace(&outside);
    }

    #[test]
    fn invalid_yaml_fails_without_fallback() {
        let workspace = temp_workspace();
        fs::write(
            workspace.join(".oore.yaml"),
            "version: 1\nplatforms: [android]\ncommands:\n  bad_stage: [\"echo no\"]\nartifacts:\n  patterns: [\"*.apk\"]\n",
        )
        .expect("write invalid yaml");

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "ui_execution_config": {
                "platforms": ["android"],
                "commands": { "pre_build": [], "build": ["echo fallback"], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let error = resolve_execution_plan(&workspace, &snapshot).expect_err("should fail");
        assert!(error.to_string().contains("Invalid pipeline config"));

        cleanup_workspace(&workspace);
    }

    #[test]
    fn missing_file_uses_ui_fallback_config() {
        let workspace = temp_workspace();

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "ui_execution_config": {
                "platforms": ["android", "macos"],
                "commands": { "pre_build": ["echo pre"], "build": ["echo custom"], "post_build": ["echo post"] },
                "artifact_patterns": ["*.apk", "*.zip"]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve plan");
        assert_eq!(plan.source, "ui_fallback");
        assert_eq!(
            plan.stage_commands.pre_build,
            vec!["flutter pub get".to_string(), "echo pre".to_string()]
        );
        assert_eq!(
            plan.stage_commands.build,
            vec![
                "flutter build apk --release".to_string(),
                "flutter build macos --release".to_string(),
                "echo custom".to_string(),
            ]
        );
        assert_eq!(
            plan.stage_commands.post_build,
            vec!["echo post".to_string()]
        );

        cleanup_workspace(&workspace);
    }

    #[test]
    fn repo_file_custom_build_commands_override_default_platform_commands() {
        let workspace = temp_workspace();
        fs::write(
            workspace.join(".oore.yaml"),
            "version: 1\nplatforms: [android]\ncommands:\n  pre_build: [\"echo pre\"]\n  build: [\"echo custom-build\"]\n  post_build: [\"echo post\"]\nartifacts:\n  patterns: [\"*.txt\"]\n",
        )
        .expect("write repo config");

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "ui_execution_config": {
                "platforms": ["android"],
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve plan");
        assert_eq!(
            plan.source,
            format!("file:{}", workspace.join(".oore.yaml").display())
        );
        assert_eq!(plan.stage_commands.pre_build, vec!["echo pre".to_string()]);
        assert_eq!(
            plan.stage_commands.build,
            vec!["echo custom-build".to_string()]
        );
        assert_eq!(
            plan.stage_commands.post_build,
            vec!["echo post".to_string()]
        );
        assert_eq!(plan.artifact_patterns, vec!["*.txt".to_string()]);

        cleanup_workspace(&workspace);
    }

    #[test]
    fn fvmrc_flutter_version_is_applied_to_flutter_commands() {
        let workspace = temp_workspace();
        fs::write(workspace.join(".fvmrc"), "{ \"flutter\": \"3.24.0\" }\n").expect("write .fvmrc");

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "ui_execution_config": {
                "platforms": ["android"],
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve plan");
        assert_eq!(
            plan.stage_commands.pre_build,
            vec![
                "fvm use 3.24.0 --force".to_string(),
                "fvm flutter pub get".to_string(),
            ]
        );
        assert_eq!(
            plan.stage_commands.build,
            vec!["fvm flutter build apk --release".to_string()]
        );

        cleanup_workspace(&workspace);
    }

    #[test]
    fn ui_flutter_version_used_when_fvmrc_absent() {
        let workspace = temp_workspace();

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "ui_execution_config": {
                "platforms": ["android"],
                "flutter_version": "3.22.3",
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve plan");
        assert_eq!(
            plan.stage_commands.pre_build,
            vec![
                "fvm use 3.22.3 --force".to_string(),
                "fvm flutter pub get".to_string(),
            ]
        );
        assert_eq!(
            plan.stage_commands.build,
            vec!["fvm flutter build apk --release".to_string()]
        );

        cleanup_workspace(&workspace);
    }

    #[test]
    fn invalid_fvmrc_fails_resolution() {
        let workspace = temp_workspace();
        fs::write(workspace.join(".fvmrc"), "{ }\n").expect("write invalid .fvmrc");

        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "ui_execution_config": {
                "platforms": ["android"],
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["*.apk"]
            }
        });

        let error = resolve_execution_plan(&workspace, &snapshot).expect_err("should fail");
        assert!(error.to_string().contains(".fvmrc"));

        cleanup_workspace(&workspace);
    }

    #[test]
    fn ui_fallback_accepts_workspace_relative_artifact_patterns() {
        let workspace = temp_workspace();
        let snapshot = serde_json::json!({
            "config_path_explicit": false,
            "ui_execution_config": {
                "platforms": ["android", "ios"],
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": [
                    "build/app/outputs/bundle/release/*.aab",
                    "build/ios/ipa/*.ipa"
                ]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve UI fallback");
        assert_eq!(
            plan.artifact_patterns,
            vec![
                "build/app/outputs/bundle/release/*.aab".to_string(),
                "build/ios/ipa/*.ipa".to_string(),
            ]
        );

        cleanup_workspace(&workspace);
    }

    #[test]
    fn multi_platform_default_command_order_is_sequential() {
        let config = PipelineExecutionConfig {
            platforms: vec![
                BuildPlatform::Android,
                BuildPlatform::Ios,
                BuildPlatform::Macos,
            ],
            flutter_version: None,
            commands: PipelineCommandStages::default(),
            platform_build_args: PlatformBuildArgs::default(),
            platform_commands: PlatformBuildCommands::default(),
            env: Vec::new(),
            artifact_patterns: vec!["*.apk".to_string()],
        };
        let commands = materialize_stage_commands(&config, true);
        assert_eq!(
            commands.build,
            vec![
                "flutter build apk --release".to_string(),
                "flutter build ios --release --no-codesign".to_string(),
                "flutter build macos --release".to_string(),
            ]
        );
    }

    #[test]
    fn per_run_platform_selection_filters_repository_platform_commands() {
        let workspace = temp_workspace();
        fs::write(
            workspace.join(".oore.yml"),
            "version: 1\nplatforms: [android, ios]\nplatform_commands:\n  android: flutter build apk --release --target lib/main_adhoc.dart\n  ios: flutter build ipa --release --export-method ad-hoc --target lib/main_adhoc.dart\nartifacts:\n  patterns: [\"build/**/*.apk\", \"build/**/*.ipa\"]\n",
        )
        .expect("write combined workflow");
        let snapshot = serde_json::json!({
            "config_path_explicit": true,
            "config_path": ".oore.yml",
            "selected_platforms": ["ios"],
            "ui_execution_config": {
                "platforms": ["android", "ios"],
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["build/**/*.apk", "build/**/*.ipa"]
            }
        });

        let plan = resolve_execution_plan(&workspace, &snapshot).expect("resolve plan");
        assert_eq!(
            plan.stage_commands.build,
            vec![
                "flutter build ipa --release --export-method ad-hoc --target lib/main_adhoc.dart"
                    .to_string()
            ]
        );
        cleanup_workspace(&workspace);
    }

    #[test]
    fn per_run_platform_selection_rejects_shared_build_commands() {
        let workspace = temp_workspace();
        fs::write(
            workspace.join(".oore.yml"),
            "version: 1\nplatforms: [android, ios]\ncommands:\n  build: [\"flutter build ipa --release\"]\nartifacts:\n  patterns: [\"build/**/*.ipa\"]\n",
        )
        .expect("write workflow");
        let snapshot = serde_json::json!({
            "config_path_explicit": true,
            "config_path": ".oore.yml",
            "selected_platforms": ["ios"]
        });

        let error = resolve_execution_plan(&workspace, &snapshot).expect_err("must reject");
        assert!(
            error
                .to_string()
                .contains("shared commands.build cannot be filtered safely")
        );
        cleanup_workspace(&workspace);
    }

    #[test]
    fn defaults_run_before_custom_build_commands() {
        let config = PipelineExecutionConfig {
            platforms: vec![BuildPlatform::Android],
            flutter_version: None,
            commands: PipelineCommandStages {
                pre_build: Vec::new(),
                build: vec!["echo custom-build".to_string()],
                post_build: Vec::new(),
            },
            platform_build_args: PlatformBuildArgs::default(),
            platform_commands: PlatformBuildCommands::default(),
            env: Vec::new(),
            artifact_patterns: vec!["*.apk".to_string()],
        };

        let commands = materialize_stage_commands(&config, true);
        assert_eq!(
            commands.build,
            vec![
                "flutter build apk --release".to_string(),
                "echo custom-build".to_string(),
            ]
        );
    }

    #[test]
    fn platform_build_args_are_appended_to_default_commands() {
        let config = PipelineExecutionConfig {
            platforms: vec![BuildPlatform::Android],
            flutter_version: None,
            commands: PipelineCommandStages::default(),
            platform_build_args: PlatformBuildArgs {
                android: vec![
                    "--dart-define-from-file=config/dev.json".to_string(),
                    "--build-number=$PROJECT_BUILD_NUMBER".to_string(),
                ],
                ios: Vec::new(),
                macos: Vec::new(),
            },
            platform_commands: PlatformBuildCommands::default(),
            env: Vec::new(),
            artifact_patterns: vec!["*.apk".to_string()],
        };

        let commands = materialize_stage_commands(&config, true);
        assert_eq!(
            commands.build,
            vec![
                "flutter build apk --release --dart-define-from-file=config/dev.json --build-number=$PROJECT_BUILD_NUMBER".to_string()
            ]
        );
    }

    #[test]
    fn platform_command_override_takes_precedence_over_default_and_args() {
        let config = PipelineExecutionConfig {
            platforms: vec![BuildPlatform::Android],
            flutter_version: None,
            commands: PipelineCommandStages::default(),
            platform_build_args: PlatformBuildArgs {
                android: vec!["--build-number=42".to_string()],
                ios: Vec::new(),
                macos: Vec::new(),
            },
            platform_commands: PlatformBuildCommands {
                android: Some("flutter build appbundle --release".to_string()),
                ios: None,
                macos: None,
            },
            env: Vec::new(),
            artifact_patterns: vec!["*.apk".to_string()],
        };

        let commands = materialize_stage_commands(&config, true);
        assert_eq!(
            commands.build,
            vec!["flutter build appbundle --release".to_string()]
        );
    }

    #[test]
    fn env_keys_must_be_unique_and_valid() {
        let config = PipelineExecutionConfig {
            platforms: vec![BuildPlatform::Android],
            flutter_version: None,
            commands: PipelineCommandStages::default(),
            platform_build_args: PlatformBuildArgs::default(),
            platform_commands: PlatformBuildCommands::default(),
            env: vec![
                PipelineEnvVar {
                    key: "PROJECT_BUILD_NUMBER".to_string(),
                    value: "1".to_string(),
                },
                PipelineEnvVar {
                    key: "PROJECT_BUILD_NUMBER".to_string(),
                    value: "2".to_string(),
                },
            ],
            artifact_patterns: vec!["*.apk".to_string()],
        };

        let err = normalize_execution_config(config).expect_err("duplicate env keys should fail");
        assert!(err.to_string().contains("duplicate key"));
    }

    #[test]
    fn normalizes_legacy_command_substitution_syntax() {
        let normalized = normalize_legacy_env_syntax(
            "flutter build apk --build-number=$($PROJECT_BUILD_NUMBER)",
        );
        assert_eq!(
            normalized,
            "flutter build apk --build-number=$PROJECT_BUILD_NUMBER"
        );
    }

    #[test]
    fn command_preview_expands_env_and_masks_sensitive_values() {
        let env = vec![
            ("PROJECT_BUILD_NUMBER".to_string(), "42".to_string()),
            ("API_TOKEN".to_string(), "secret-123".to_string()),
        ];
        let preview = render_command_preview(
            "flutter build apk --build-number=$PROJECT_BUILD_NUMBER --dart-define=API_TOKEN=$API_TOKEN",
            &env,
        );
        assert!(preview.contains("--build-number=42"));
        assert!(preview.contains("API_TOKEN=***"));
        assert!(!preview.contains("secret-123"));
    }

    #[test]
    fn appends_dart_define_file_to_flutter_build_commands() {
        let command = "fvm flutter build apk --release";
        assert!(is_flutter_build_command(command));
        let updated = with_dart_define_file(command, ".env");
        assert_eq!(
            updated,
            "fvm flutter build apk --release --dart-define-from-file=.env"
        );
    }

    #[test]
    fn detects_wrapped_flutter_build_commands() {
        assert!(is_flutter_build_command(
            "cd app && flutter build ios --release --no-codesign"
        ));
        assert!(is_flutter_build_command(
            "env FOO=bar fvm flutter build ipa --release"
        ));
    }

    #[test]
    fn does_not_duplicate_existing_dart_define_file_arg() {
        let command = "flutter build ios --release --dart-define-from-file=.env";
        let updated = with_dart_define_file(command, ".env");
        assert_eq!(updated, command);
    }

    #[test]
    fn chooses_release_testing_export_method_when_available() {
        let help = "Available options: app-store-connect, release-testing, enterprise";
        assert_eq!(choose_ios_export_method(help), "release-testing");
    }

    #[test]
    fn ios_signing_filename_must_be_single_path_component() {
        assert_eq!(
            safe_ios_signing_filename(" dist.p12 ", "fallback.p12", "p12_filename")
                .expect("valid filename"),
            "dist.p12"
        );
        assert_eq!(
            safe_ios_signing_filename("", "fallback.p12", "p12_filename")
                .expect("fallback filename"),
            "fallback.p12"
        );

        for unsafe_name in [
            "../dist.p12",
            "/tmp/dist.p12",
            "profiles/dist.mobileprovision",
            "profiles\\dist.mobileprovision",
            ".",
            "..",
        ] {
            assert!(
                safe_ios_signing_filename(unsafe_name, "fallback.p12", "profile_filename").is_err(),
                "{unsafe_name} should be rejected"
            );
        }
    }

    #[test]
    fn falls_back_to_ad_hoc_alias_when_release_testing_absent() {
        let help = "Available options: app-store, ad-hoc, enterprise";
        assert_eq!(choose_ios_export_method(help), "ad-hoc");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reads_install_metadata_from_signed_app_info_plist() {
        let workspace = temp_workspace();
        let app = workspace.join("Kite.app");
        fs::create_dir_all(&app).expect("create app bundle");
        fs::write(
            app.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.example.kite</string>
<key>CFBundleDisplayName</key><string>Kite QA</string>
<key>CFBundleShortVersionString</key><string>3.2.1</string>
<key>CFBundleVersion</key><string>42</string>
</dict></plist>"#,
        )
        .expect("write Info.plist");

        let metadata = read_ios_app_metadata(&app).expect("read app metadata");
        assert_eq!(metadata.bundle_identifier, "com.example.kite");
        assert_eq!(metadata.display_name, "Kite QA");
        assert_eq!(metadata.version, "3.2.1");
        assert_eq!(metadata.build_number, "42");
        cleanup_workspace(&workspace);
    }

    #[test]
    fn adapts_flutter_build_ios_to_signed_ipa_export() {
        let export_plist = PathBuf::from("/tmp/ExportOptions.plist");
        let command = "flutter build ios --release --no-codesign";
        let adapted =
            adapt_ios_command_for_signing(command, export_plist.as_path()).expect("adapt");
        assert!(adapted.starts_with("flutter build ipa --release"));
        assert!(adapted.contains("--no-codesign"));
        assert!(!adapted.contains("--export-options-plist"));
    }

    #[test]
    fn detects_ios_build_commands_when_wrapped() {
        assert!(is_ios_flutter_build_command(
            "cd app && flutter build ios --release --no-codesign"
        ));
        assert!(is_ios_flutter_build_command(
            "env FOO=bar fvm flutter build ipa --release"
        ));
    }

    #[test]
    fn adapts_wrapped_flutter_ios_commands() {
        let export_plist = PathBuf::from("/tmp/ExportOptions.plist");
        let command = "cd app && flutter build ios --release --no-codesign";
        let adapted =
            adapt_ios_command_for_signing(command, export_plist.as_path()).expect("adapt");
        assert!(adapted.contains("flutter build ipa --release"));
        assert!(adapted.contains("--no-codesign"));
        assert!(!adapted.contains("--export-options-plist"));
    }

    #[test]
    fn normalizes_build_stage_wrapped_ios_command_with_signing_and_define_file() {
        let export_plist = PathBuf::from("/tmp/ExportOptions.plist");
        let command = "cd app && flutter build ios --release --no-codesign";
        let (normalized, signing_applied) = normalize_stage_command_for_execution(
            "build",
            command,
            Some(".env"),
            Some(export_plist.as_path()),
        )
        .expect("normalize");

        assert!(signing_applied);
        assert!(normalized.contains("flutter build ipa --release"));
        assert!(normalized.contains("--no-codesign"));
        assert!(normalized.contains("--dart-define-from-file=.env"));
        assert!(!normalized.contains("--export-options-plist"));
    }

    #[test]
    fn does_not_apply_ios_signing_rewrite_outside_build_stage() {
        let export_plist = PathBuf::from("/tmp/ExportOptions.plist");
        let command = "cd app && flutter build ios --release --no-codesign";
        let (normalized, signing_applied) = normalize_stage_command_for_execution(
            "pre_build",
            command,
            Some(".env"),
            Some(export_plist.as_path()),
        )
        .expect("normalize");

        assert!(!signing_applied);
        assert_eq!(normalized, command);
    }

    #[test]
    fn normalizes_wrapped_ipa_command_and_marks_signing_applied() {
        let export_plist = PathBuf::from("/tmp/ExportOptions.plist");
        let command = "env FOO=bar fvm flutter build ipa --release --export-method ad-hoc";
        let (normalized, signing_applied) = normalize_stage_command_for_execution(
            "build",
            command,
            None,
            Some(export_plist.as_path()),
        )
        .expect("normalize");

        assert!(signing_applied);
        assert!(normalized.contains("fvm flutter build ipa --release"));
        assert!(!normalized.contains("--export-method"));
        assert!(normalized.contains("--no-codesign"));
        assert!(!normalized.contains("--export-options-plist"));
    }

    #[test]
    fn replaces_pipeline_export_options_with_oore_signed_export() {
        let export_plist = PathBuf::from("/tmp/OoreExportOptions.plist");
        let command =
            "fvm flutter build ipa --release --codesign --export-options-plist /tmp/repo.plist";
        let adapted =
            adapt_ios_command_for_signing(command, export_plist.as_path()).expect("adapt");

        assert!(!adapted.contains("/tmp/repo.plist"));
        assert!(!adapted.contains("--codesign"));
        assert!(adapted.contains("--no-codesign"));
        assert!(!adapted.contains("--export-options-plist"));
    }

    #[test]
    fn converts_bundle_ids_to_xcode_build_setting_identifiers() {
        assert_eq!(
            xcode_build_setting_identifier("com.example.app-share"),
            "com_example_app_share"
        );
    }

    #[test]
    fn selects_only_the_direct_app_from_an_xcarchive() {
        let workspace = temp_workspace();
        let applications = workspace.join("Products/Applications");
        let app = applications.join("Runner.app");
        let nested_app = app.join("Watch/Companion.app");
        fs::create_dir_all(&nested_app).expect("create archive apps");

        assert_eq!(
            find_direct_path_with_extension(&applications, "app").expect("find main app"),
            app
        );
        cleanup_workspace(&workspace);
    }

    #[test]
    fn build_stage_ios_simulator_command_fails_when_signing_enabled() {
        let export_plist = PathBuf::from("/tmp/ExportOptions.plist");
        let command = "flutter build ios --simulator";
        let error = normalize_stage_command_for_execution(
            "build",
            command,
            None,
            Some(export_plist.as_path()),
        )
        .expect_err("simulator must fail");

        assert!(error.to_string().contains("--simulator"));
    }

    #[test]
    fn parses_imported_distribution_certificate() {
        let output = r#"
SHA-256 hash: 7C16B3FEEB8C0DD77E73D2714F4E63C4F4EBB57CDAF72F11F121AF610AB1647F
SHA-1 hash: 0ADDF2727054A792183CF51F72B687DCA1D35C6B
keychain: "/tmp/oore-ci-build.keychain-db"
attributes:
    "alis"<blob>="Apple Distribution: Zerodha Broking Limited (843ED8PUW8)"
"#;
        assert_eq!(
            parse_distribution_certificate(output),
            Some((
                "0ADDF2727054A792183CF51F72B687DCA1D35C6B".to_string(),
                "Apple Distribution: Zerodha Broking Limited (843ED8PUW8)".to_string()
            ))
        );
        assert_eq!(parse_distribution_certificate("no certificates"), None);
    }

    #[test]
    fn parses_default_keychain_path_for_restore() {
        assert_eq!(
            parse_keychain_list("    \"/Users/runner/Library/Keychains/login.keychain-db\"\n"),
            vec!["/Users/runner/Library/Keychains/login.keychain-db"]
        );
    }
}
