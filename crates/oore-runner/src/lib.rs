use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use oore_contract::{
    AndroidSigningBuildType, BuildPlatform, BuildStatus, ClaimJobResponse, ClaimedJob,
    JobStatusResponse, PipelineCommandStages, PipelineEnvVar, PipelineExecutionConfig,
    PlatformBuildArgs, PlatformBuildCommands, RunnerAndroidSigningProfile,
    RunnerAndroidSigningResponse, RunnerIosSigningBundle, RunnerIosSigningResponse, StepResult,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::io::AsyncBufReadExt;

const AUTO_CONFIG_PATHS: [&str; 2] = [".oore.yaml", ".oore.yml"];
const OORE_ANDROID_KEYSTORE_PATH_ENV: &str = "OORE_ANDROID_KEYSTORE_PATH";
const OORE_ANDROID_KEYSTORE_B64_ENV: &str = "OORE_ANDROID_KEYSTORE_BASE64";
const OORE_ANDROID_KEYSTORE_PASSWORD_ENV: &str = "OORE_ANDROID_KEYSTORE_PASSWORD";
const OORE_ANDROID_KEY_ALIAS_ENV: &str = "OORE_ANDROID_KEY_ALIAS";
const OORE_ANDROID_KEY_PASSWORD_ENV: &str = "OORE_ANDROID_KEY_PASSWORD";
const OORE_ANDROID_KEY_PROPERTIES_PATH_ENV: &str = "OORE_ANDROID_KEY_PROPERTIES_PATH";
const IOS_SIGNING_DIR: &str = ".oore/ios-signing";
const BUILD_WORKSPACE_ROOT: &str = "/tmp/oore-builds.noindex";
const SPOTLIGHT_NO_INDEX_SENTINEL: &str = ".metadata_never_index";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunnerConfig {
    pub runner_id: String,
    pub runner_token: String,
    pub daemon_url: String,
    pub name: String,
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

    serde_json::json!({
        "os": "macos",
        "os_version": os_version,
        "arch": arch,
        "xcode_version": xcode_version,
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct AndroidSigningEnv {
    keystore_path: Option<String>,
    keystore_b64: Option<String>,
    keystore_password: Option<String>,
    key_alias: Option<String>,
    key_password: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AndroidSigningInputs {
    keystore_bytes: Vec<u8>,
    keystore_password: String,
    key_alias: String,
    key_password: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AndroidSigningMaterialization {
    keystore_path: PathBuf,
    key_properties_path: PathBuf,
    keystore_overwrote_existing: bool,
    key_properties_overwrote_existing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AndroidSigningPreparation {
    inputs: AndroidSigningInputs,
    materialization: AndroidSigningMaterialization,
}

fn trim_to_opt(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn collect_android_signing_env(read_env: impl Fn(&str) -> Option<String>) -> AndroidSigningEnv {
    AndroidSigningEnv {
        keystore_path: trim_to_opt(read_env(OORE_ANDROID_KEYSTORE_PATH_ENV)),
        keystore_b64: trim_to_opt(read_env(OORE_ANDROID_KEYSTORE_B64_ENV)),
        keystore_password: trim_to_opt(read_env(OORE_ANDROID_KEYSTORE_PASSWORD_ENV)),
        key_alias: trim_to_opt(read_env(OORE_ANDROID_KEY_ALIAS_ENV)),
        key_password: trim_to_opt(read_env(OORE_ANDROID_KEY_PASSWORD_ENV)),
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

fn require_signing_field(value: Option<String>, env_key: &str) -> anyhow::Result<String> {
    value.ok_or_else(|| anyhow::anyhow!("missing required environment variable {env_key}"))
}

fn resolve_android_signing_inputs(
    env: &AndroidSigningEnv,
) -> anyhow::Result<Option<AndroidSigningInputs>> {
    let any_present = env.keystore_path.is_some()
        || env.keystore_b64.is_some()
        || env.keystore_password.is_some()
        || env.key_alias.is_some()
        || env.key_password.is_some();

    if !any_present {
        return Ok(None);
    }

    let keystore_bytes = if let Some(path_raw) = &env.keystore_path {
        let path = PathBuf::from(path_raw);
        fs::read(&path).map_err(|e| {
            anyhow::anyhow!(
                "failed to read keystore from {} ({}): {e}",
                OORE_ANDROID_KEYSTORE_PATH_ENV,
                path.display()
            )
        })?
    } else {
        let b64 = require_signing_field(env.keystore_b64.clone(), OORE_ANDROID_KEYSTORE_B64_ENV)?;
        decode_base64_keystore(&b64)?
    };

    if keystore_bytes.is_empty() {
        anyhow::bail!("resolved keystore file is empty");
    }

    Ok(Some(AndroidSigningInputs {
        keystore_bytes,
        keystore_password: require_signing_field(
            env.keystore_password.clone(),
            OORE_ANDROID_KEYSTORE_PASSWORD_ENV,
        )?,
        key_alias: require_signing_field(env.key_alias.clone(), OORE_ANDROID_KEY_ALIAS_ENV)?,
        key_password: require_signing_field(
            env.key_password.clone(),
            OORE_ANDROID_KEY_PASSWORD_ENV,
        )?,
    }))
}

fn android_signing_prepared_marker(
    source: &str,
    variant: AndroidSigningBuildType,
    prep: &AndroidSigningPreparation,
) -> String {
    format!(
        "[oore-signing] {}",
        serde_json::json!({
            "event": "android_signing_prepared",
            "source": source,
            "variant": match variant {
                AndroidSigningBuildType::Debug => "debug",
                AndroidSigningBuildType::Release => "release",
            },
            "key_properties_path": prep.materialization.key_properties_path,
            "keystore_path": prep.materialization.keystore_path,
            "key_properties_overwrote_existing": prep.materialization.key_properties_overwrote_existing,
            "keystore_overwrote_existing": prep.materialization.keystore_overwrote_existing,
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

fn requires_android_signing(build_commands: &[String]) -> bool {
    build_commands
        .iter()
        .any(|command| is_android_flutter_build_command(command))
}

fn materialize_android_signing_files(
    workspace: &Path,
    inputs: &AndroidSigningInputs,
) -> anyhow::Result<AndroidSigningMaterialization> {
    let android_dir = workspace.join("android");
    if !android_dir.is_dir() {
        anyhow::bail!(
            "Android signing configuration was provided, but no android/ directory exists in repository"
        );
    }

    let app_dir = android_dir.join("app");
    fs::create_dir_all(&app_dir).map_err(|e| {
        anyhow::anyhow!(
            "failed to prepare Android app directory {}: {e}",
            app_dir.display()
        )
    })?;

    let keystore_file_name = "oore-upload-keystore.jks";
    let keystore_path = app_dir.join(keystore_file_name);
    let keystore_overwrote_existing = keystore_path.exists();
    fs::write(&keystore_path, &inputs.keystore_bytes).map_err(|e| {
        anyhow::anyhow!("failed to write keystore {}: {e}", keystore_path.display())
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        let _ = fs::set_permissions(&keystore_path, perms);
    }

    let key_properties_path = android_dir.join("key.properties");
    let key_properties_overwrote_existing = key_properties_path.exists();
    let key_properties = format!(
        "storePassword={}\nkeyPassword={}\nkeyAlias={}\nstoreFile={}\n",
        inputs.keystore_password, inputs.key_password, inputs.key_alias, keystore_file_name
    );
    fs::write(&key_properties_path, key_properties).map_err(|e| {
        anyhow::anyhow!(
            "failed to write Android key.properties {}: {e}",
            key_properties_path.display()
        )
    })?;

    Ok(AndroidSigningMaterialization {
        keystore_path,
        key_properties_path,
        keystore_overwrote_existing,
        key_properties_overwrote_existing,
    })
}

fn prepare_android_signing_if_configured(
    workspace: &Path,
    build_commands: &[String],
) -> anyhow::Result<Option<AndroidSigningPreparation>> {
    if !requires_android_signing(build_commands) {
        return Ok(None);
    }

    let env = collect_android_signing_env(|key| std::env::var(key).ok());
    let Some(inputs) = resolve_android_signing_inputs(&env)? else {
        return Ok(None);
    };

    let materialization = materialize_android_signing_files(workspace, &inputs)?;
    Ok(Some(AndroidSigningPreparation {
        inputs,
        materialization,
    }))
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

async fn fetch_job_android_signing(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
) -> anyhow::Result<Option<RunnerAndroidSigningResponse>> {
    let resp = client
        .get(format!(
            "{}/v1/runners/{}/jobs/{}/android-signing",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
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

#[derive(Debug, Clone)]
struct IosSigningMaterialization {
    p12_path: PathBuf,
    export_options_plist_path: PathBuf,
    bundle_profile_mapping: Vec<(String, String)>,
    effective_export_method: String,
    signing_identity_sha1: Option<String>,
}

struct IosSigningCleanup {
    keychain_path: PathBuf,
    original_keychains: Vec<String>,
    installed_profiles: Vec<PathBuf>,
}

impl Drop for IosSigningCleanup {
    fn drop(&mut self) {
        cleanup_ios_signing_state(
            Some(&self.keychain_path),
            &self.original_keychains,
            &self.installed_profiles,
        );
    }
}

fn run_security_command(args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("security")
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
    let output = Command::new("security")
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("failed to execute security command: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("security command failed: {stderr}");
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn cleanup_ios_signing_state(
    keychain_path: Option<&Path>,
    original_keychains: &[String],
    installed_profiles: &[PathBuf],
) {
    for profile in installed_profiles {
        let _ = fs::remove_file(profile);
    }

    if let Some(path) = keychain_path {
        if !original_keychains.is_empty() {
            let _ = run_security_command_with_strings(
                &[
                    "list-keychains".to_string(),
                    "-d".to_string(),
                    "user".to_string(),
                    "-s".to_string(),
                ]
                .into_iter()
                .chain(original_keychains.iter().cloned())
                .collect::<Vec<_>>(),
            );
        }

        let keychain_str = path.display().to_string();
        let _ = run_security_command_with_strings(&[
            "delete-keychain".to_string(),
            keychain_str.clone(),
        ]);
        let _ = fs::remove_file(path);
    }
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

fn parse_codesigning_identity_hashes(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let (_, rest) = trimmed.split_once(')')?;
            let candidate = rest.split_whitespace().next()?;
            if candidate.len() == 40 && candidate.chars().all(|ch| ch.is_ascii_hexdigit()) {
                Some(candidate.to_string())
            } else {
                None
            }
        })
        .collect()
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

    let p12_path = signing_dir.join(if bundle.p12_filename.trim().is_empty() {
        "distribution.p12"
    } else {
        bundle.p12_filename.trim()
    });
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

    let mut installed_profiles = Vec::new();
    let keychain_password = random_password_hex();
    let keychain_path = signing_dir.join("oore-ci-build.keychain-db");
    let keychain_path_str = keychain_path.display().to_string();
    let mut keychain_created = false;
    let mut original_keychains = Vec::new();

    let install_result: anyhow::Result<IosSigningMaterialization> = (|| {
        let mut bundle_profile_mapping = Vec::new();
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

            let work_file_name = if profile.profile_filename.trim().is_empty() {
                format!("{}.mobileprovision", profile.bundle_id)
            } else {
                profile.profile_filename.trim().to_string()
            };
            let work_path = profile_work_dir.join(work_file_name);
            fs::write(&work_path, &profile_bytes).map_err(|e| {
                anyhow::anyhow!("failed to write profile {}: {e}", work_path.display())
            })?;

            let profile_ref = profile
                .profile_uuid
                .clone()
                .or_else(|| profile.profile_name.clone())
                .unwrap_or_else(|| hex::encode(Sha256::digest(&profile_bytes)));

            let installed_name = format!("{profile_ref}.mobileprovision");
            let installed_path = installed_profiles_dir.join(installed_name);
            fs::write(&installed_path, &profile_bytes).map_err(|e| {
                anyhow::anyhow!(
                    "failed to install provisioning profile {}: {e}",
                    installed_path.display()
                )
            })?;
            installed_profiles.push(installed_path);
            bundle_profile_mapping.push((profile.bundle_id.clone(), profile_ref));
        }

        run_security_command_with_strings(&[
            "create-keychain".to_string(),
            "-p".to_string(),
            keychain_password.clone(),
            keychain_path_str.clone(),
        ])?;
        keychain_created = true;

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

        let original_keychain_output = run_security_command(&["list-keychains", "-d", "user"])?;
        original_keychains = parse_keychain_list(&original_keychain_output);

        let mut list_args = vec![
            "list-keychains".to_string(),
            "-d".to_string(),
            "user".to_string(),
            "-s".to_string(),
            keychain_path_str.clone(),
        ];
        list_args.extend(original_keychains.clone());
        run_security_command_with_strings(&list_args)?;

        run_security_command_with_strings(&[
            "import".to_string(),
            p12_path.display().to_string(),
            "-k".to_string(),
            keychain_path_str.clone(),
            "-P".to_string(),
            bundle.p12_password.clone(),
            "-T".to_string(),
            "/usr/bin/codesign".to_string(),
            "-T".to_string(),
            "/usr/bin/security".to_string(),
            "-T".to_string(),
            "/usr/bin/xcodebuild".to_string(),
        ])?;

        run_security_command_with_strings(&[
            "set-key-partition-list".to_string(),
            "-S".to_string(),
            "apple-tool:,apple:".to_string(),
            "-k".to_string(),
            keychain_password.clone(),
            keychain_path_str.clone(),
        ])?;

        let identity_output = run_security_command_with_strings(&[
            "find-identity".to_string(),
            "-v".to_string(),
            "-p".to_string(),
            "codesigning".to_string(),
            keychain_path_str.clone(),
        ])?;
        let identity_hashes = parse_codesigning_identity_hashes(&identity_output);
        if identity_hashes.is_empty() {
            anyhow::bail!("no valid code signing identity found after importing p12");
        }

        let signing_identity_sha1 = identity_hashes.first().cloned();
        let effective_export_method = resolve_ios_export_method();
        let export_options_plist_path = signing_dir.join("ExportOptions.plist");
        write_export_options_plist(
            &export_options_plist_path,
            bundle.team_id.trim(),
            &effective_export_method,
            &bundle_profile_mapping,
            signing_identity_sha1.as_deref(),
        )?;

        Ok(IosSigningMaterialization {
            p12_path: p12_path.clone(),
            export_options_plist_path,
            bundle_profile_mapping,
            effective_export_method,
            signing_identity_sha1,
        })
    })();

    match install_result {
        Ok(materialization) => Ok((
            materialization,
            IosSigningCleanup {
                keychain_path,
                original_keychains,
                installed_profiles,
            },
        )),
        Err(err) => {
            cleanup_ios_signing_state(
                if keychain_created {
                    Some(&keychain_path)
                } else {
                    None
                },
                &original_keychains,
                &installed_profiles,
            );
            Err(err)
        }
    }
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
    export_options_plist: &Path,
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

    args.retain(|arg| arg != "--no-codesign");
    args.retain(|arg| !arg.starts_with("--export-method="));

    let rewrote_ios_target = rewrite_flutter_ios_target_to_ipa(&mut args);
    if !rewrote_ios_target && !contains_flutter_build_target(&args, "ipa") {
        anyhow::bail!(
            "iOS signing is enabled, but command did not contain a Flutter iOS build target that can be rewritten to ipa export"
        );
    }

    let has_export_plist = args
        .iter()
        .any(|arg| arg.starts_with("--export-options-plist="));
    if !has_export_plist {
        args.push(format!(
            "--export-options-plist={}",
            export_options_plist.display()
        ));
    }

    Ok(args.join(" "))
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
) -> anyhow::Result<Option<RunnerIosSigningResponse>> {
    let resp = client
        .get(format!(
            "{}/v1/runners/{}/jobs/{}/ios-signing",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
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

pub async fn run_runner_forever(
    config: RunnerConfig,
    daemon_url_override: Option<String>,
) -> anyhow::Result<()> {
    let daemon_url = daemon_url_override.unwrap_or(config.daemon_url.clone());
    let client = reqwest::Client::new();

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
struct RepoPipelineConfig {
    version: u32,
    platforms: Vec<BuildPlatform>,
    #[serde(default)]
    flutter_version: Option<String>,
    #[serde(default)]
    commands: RepoCommandStages,
    #[serde(default)]
    platform_build_args: RepoPlatformBuildArgs,
    #[serde(default)]
    platform_commands: RepoPlatformBuildCommands,
    #[serde(default)]
    env: Vec<RepoEnvVar>,
    #[serde(default)]
    artifacts: RepoArtifacts,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoCommandStages {
    #[serde(default)]
    pre_build: Vec<String>,
    #[serde(default)]
    build: Vec<String>,
    #[serde(default)]
    post_build: Vec<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoArtifacts {
    #[serde(default)]
    patterns: Vec<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoPlatformBuildArgs {
    #[serde(default)]
    android: Vec<String>,
    #[serde(default)]
    ios: Vec<String>,
    #[serde(default)]
    macos: Vec<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoPlatformBuildCommands {
    #[serde(default)]
    android: Option<String>,
    #[serde(default)]
    ios: Option<String>,
    #[serde(default)]
    macos: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RepoEnvVar {
    key: String,
    value: String,
}

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
        if !trimmed.starts_with("*.") || trimmed.chars().any(char::is_whitespace) {
            anyhow::bail!("artifacts.patterns[{idx}] must be extension globs like '*.apk'");
        }
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
    let parsed: RepoPipelineConfig =
        serde_yaml::from_str(raw).map_err(|e| anyhow::anyhow!("YAML parse error: {e}"))?;

    if parsed.version != 1 {
        anyhow::bail!("unsupported config version {}, expected 1", parsed.version);
    }

    normalize_execution_config(PipelineExecutionConfig {
        platforms: parsed.platforms,
        flutter_version: parsed.flutter_version,
        commands: PipelineCommandStages {
            pre_build: parsed.commands.pre_build,
            build: parsed.commands.build,
            post_build: parsed.commands.post_build,
        },
        platform_build_args: PlatformBuildArgs {
            android: parsed.platform_build_args.android,
            ios: parsed.platform_build_args.ios,
            macos: parsed.platform_build_args.macos,
        },
        platform_commands: PlatformBuildCommands {
            android: parsed.platform_commands.android,
            ios: parsed.platform_commands.ios,
            macos: parsed.platform_commands.macos,
        },
        env: parsed
            .env
            .into_iter()
            .map(|entry| PipelineEnvVar {
                key: entry.key,
                value: entry.value,
            })
            .collect(),
        artifact_patterns: parsed.artifacts.patterns,
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
    let raw = snapshot.get("ui_execution_config");
    if raw.is_none() {
        return Ok(PipelineExecutionConfig::default());
    }
    let parsed: PipelineExecutionConfig = serde_json::from_value(raw.cloned().unwrap_or_default())
        .map_err(|e| anyhow::anyhow!("Invalid ui_execution_config in snapshot: {e}"))?;
    normalize_execution_config(parsed)
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
        vec![snapshot_config_path]
    } else {
        AUTO_CONFIG_PATHS.iter().map(|p| p.to_string()).collect()
    };

    let fvmrc_version = read_fvmrc_version(workspace)?;

    for rel_path in &candidate_paths {
        let full_path = workspace.join(rel_path);
        if !full_path.exists() {
            continue;
        }

        let content = fs::read_to_string(&full_path).map_err(|e| {
            anyhow::anyhow!("Failed to read config file {}: {e}", full_path.display())
        })?;
        let file_config = parse_repo_config_file(&content).map_err(|e| {
            anyhow::anyhow!("Invalid pipeline config in {}: {}", full_path.display(), e)
        })?;
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

    let fallback = load_ui_execution_config(snapshot)?;
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

async fn check_build_active(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
) -> anyhow::Result<()> {
    let resp = client
        .get(format!(
            "{}/v1/runners/{}/jobs/{}",
            daemon_url, config.runner_id, build_id
        ))
        .bearer_auth(&config.runner_token)
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
            Ok(())
        }
        Ok(_) | Err(_) => Ok(()),
    }
}

async fn poll_cancellation(
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        if check_build_active(client, daemon_url, config, build_id)
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

async fn execute_build(
    job: &ClaimedJob,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
) -> (Vec<StepResult>, anyhow::Result<()>) {
    let workspace_root = PathBuf::from(BUILD_WORKSPACE_ROOT);
    if let Err(e) = fs::create_dir_all(&workspace_root) {
        return (vec![], Err(e.into()));
    }
    try_mark_no_spotlight_index(&workspace_root);

    let workspace = workspace_root.join(&job.build_id);
    if let Err(e) = fs::create_dir_all(&workspace) {
        return (vec![], Err(e.into()));
    }

    let _cleanup = WorkspaceCleanup {
        path: workspace.clone(),
    };

    let snapshot = &job.config_snapshot;
    let mut steps = Vec::new();
    let mut log_seq: i64 = 0;

    if let Err(e) = check_build_active(client, daemon_url, config, &job.build_id).await {
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

    let start = now_unix();
    let checkout =
        match build_checkout_invocation(repo_url, job.commit_sha.as_deref(), job.branch.as_deref())
        {
            Ok(checkout) => checkout,
            Err(e) => return (steps, Err(e)),
        };

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
        poll_cancellation(client, daemon_url, config, &job.build_id),
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
    let mut signing_preparation: Option<AndroidSigningPreparation> = None;
    let mut ios_signing_source: Option<&str> = None;
    let mut ios_signing_bundle: Option<RunnerIosSigningBundle> = None;
    let mut ios_signing_materialization: Option<IosSigningMaterialization> = None;
    let mut ios_signing_cleanup: Option<IosSigningCleanup> = None;
    let build_commands = execution_plan.stage_commands.build.as_slice();
    match determine_android_signing_variant(build_commands) {
        Ok(Some(variant)) => {
            signing_variant = Some(variant);
            match fetch_job_android_signing(client, daemon_url, config, &job.build_id).await {
                Ok(Some(server_profiles)) => {
                    if let Some(profile) = select_runner_signing_profile(&server_profiles, variant)
                    {
                        match signing_inputs_from_runner_profile(profile) {
                            Ok(inputs) => {
                                let materialization = match materialize_android_signing_files(
                                    workspace.as_path(),
                                    &inputs,
                                ) {
                                    Ok(materialization) => materialization,
                                    Err(e) => return (steps, Err(e)),
                                };
                                signing_preparation = Some(AndroidSigningPreparation {
                                    inputs,
                                    materialization,
                                });
                                signing_source = Some("pipeline_profile");
                                println!(
                                    "Prepared Android signing files from pipeline profile ({variant:?})"
                                );
                            }
                            Err(e) => return (steps, Err(e)),
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("Warning: failed to fetch pipeline Android signing profile: {e}");
                }
            }

            if signing_preparation.is_none() {
                match prepare_android_signing_if_configured(workspace.as_path(), build_commands) {
                    Ok(Some(prep)) => {
                        signing_preparation = Some(prep);
                        signing_source = Some("environment");
                        println!(
                            "Prepared Android signing files from environment fallback (OORE_ANDROID_* vars)"
                        );
                    }
                    Ok(None) => {}
                    Err(e) => return (steps, Err(e)),
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
        match fetch_job_ios_signing(client, daemon_url, config, &job.build_id).await {
            Ok(Some(server_payload)) => {
                if let Some(bundle) = server_payload.bundle {
                    match install_ios_signing_bundle(workspace.as_path(), &bundle) {
                        Ok((materialization, cleanup)) => {
                            let signing_source = match bundle.mode {
                                oore_contract::IosSigningMode::Manual => "manual",
                                oore_contract::IosSigningMode::Api => "api",
                                oore_contract::IosSigningMode::Hybrid => "hybrid",
                            };
                            ios_signing_bundle = Some(bundle);
                            ios_signing_materialization = Some(materialization);
                            ios_signing_cleanup = Some(cleanup);
                            ios_signing_source = Some(signing_source);
                            println!(
                                "Prepared iOS signing keychain/profiles from pipeline profile"
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

    if let Some(prep) = &signing_preparation {
        step_env.push((
            OORE_ANDROID_KEYSTORE_PATH_ENV.to_string(),
            prep.materialization.keystore_path.display().to_string(),
        ));
        step_env.push((
            OORE_ANDROID_KEY_PROPERTIES_PATH_ENV.to_string(),
            prep.materialization
                .key_properties_path
                .display()
                .to_string(),
        ));
        step_env.push((
            OORE_ANDROID_KEYSTORE_PASSWORD_ENV.to_string(),
            prep.inputs.keystore_password.clone(),
        ));
        step_env.push((
            OORE_ANDROID_KEY_ALIAS_ENV.to_string(),
            prep.inputs.key_alias.clone(),
        ));
        step_env.push((
            OORE_ANDROID_KEY_PASSWORD_ENV.to_string(),
            prep.inputs.key_password.clone(),
        ));
    }

    if let (Some(source), Some(variant), Some(prep)) =
        (signing_source, signing_variant, &signing_preparation)
    {
        let _ = append_runner_log_line(
            client,
            daemon_url,
            config,
            &job.build_id,
            &mut log_seq,
            "stdout",
            &android_signing_prepared_marker(source, variant, prep),
        )
        .await;
    }

    if let (Some(source), Some(bundle), Some(materialization)) = (
        ios_signing_source,
        ios_signing_bundle.as_ref(),
        ios_signing_materialization.as_ref(),
    ) {
        // Pin the exact signing identity via environment variable so xcodebuild doesn't
        // accidentally pick a different distribution certificate from the user's login keychain.
        if let Some(ref sha1) = materialization.signing_identity_sha1 {
            step_env.push(("CODE_SIGN_IDENTITY".to_string(), sha1.clone()));
        }
        let _ = append_runner_log_line(
            client,
            daemon_url,
            config,
            &job.build_id,
            &mut log_seq,
            "stdout",
            &ios_signing_prepared_marker(source, bundle, materialization),
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
        if let Err(e) = fs::write(&define_file_path, content) {
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
            if let Err(e) = check_build_active(client, daemon_url, config, &job.build_id).await {
                return (steps, Err(e));
            }

            let step_name = format!("{stage_name}-{}", index + 1);
            let start = now_unix();
            let (normalized_command, command_applied) = match normalize_stage_command_for_execution(
                stage_name,
                command,
                dart_define_file.as_deref(),
                ios_signing_materialization
                    .as_ref()
                    .map(|materialization| materialization.export_options_plist_path.as_path()),
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
                poll_cancellation(client, daemon_url, config, &job.build_id),
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
                }
            }
        }
    }

    if ios_signing_materialization.is_some() && !ios_signing_command_applied {
        return (
            steps,
            Err(anyhow::anyhow!(
                "iOS signing bundle was provided, but no Flutter iOS build command was executed"
            )),
        );
    }

    let ios_artifact_metadata = ios_signing_bundle
        .as_ref()
        .zip(ios_signing_materialization.as_ref())
        .map(|(bundle, materialization)| {
            serde_json::json!({
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
                        .filter_map(|profile| {
                            profile
                                .profile_uuid
                                .as_ref()
                                .map(|uuid| (profile.bundle_id.clone(), uuid.clone()))
                        })
                        .collect::<Vec<_>>(),
                    "certificate_fingerprint": hex::encode(Sha256::digest(
                        fs::read(&materialization.p12_path).unwrap_or_default()
                    )),
                    "effective_export_method": materialization.effective_export_method,
                }
            })
        });

    scan_and_upload_artifacts(
        workspace.as_path(),
        client,
        daemon_url,
        config,
        &job.build_id,
        &execution_plan.artifact_patterns,
        ios_artifact_metadata.as_ref(),
    )
    .await;

    drop(ios_signing_cleanup);

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

fn walk_dir_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    fn walk(dir: &std::path::Path, result: &mut Vec<PathBuf>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let is_hidden_dir = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| name.starts_with('.'));
                if is_hidden_dir {
                    continue;
                }
                walk(&path, result);
            } else if path.is_file() {
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

async fn scan_and_upload_artifacts(
    workspace: &std::path::Path,
    client: &reqwest::Client,
    daemon_url: &str,
    config: &RunnerConfig,
    build_id: &str,
    artifact_patterns: &[String],
    ios_metadata: Option<&serde_json::Value>,
) {
    let all_files = walk_dir_files(workspace);

    let custom_extensions: Vec<String> = artifact_patterns
        .iter()
        .filter_map(|pat| pat.strip_prefix("*."))
        .map(|ext| ext.to_lowercase())
        .collect();

    let mut artifacts: Vec<(PathBuf, String)> = Vec::new();

    for path in &all_files {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if let Some(art_type) = artifact_type_for_extension(ext) {
                artifacts.push((path.clone(), art_type.to_string()));
            } else if custom_extensions.contains(&ext.to_lowercase()) {
                artifacts.push((path.clone(), "generic".to_string()));
            }
        }
    }

    if artifacts.is_empty() {
        return;
    }

    println!("Found {} artifact(s) to upload", artifacts.len());

    for (path, artifact_type) in &artifacts {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let file_size = fs::metadata(path).map(|m| m.len() as i64).ok();

        let checksum = match compute_file_sha256(path) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("Warning: failed to compute checksum for {}: {}", name, e);
                None
            }
        };

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

        let resp = match client
            .post(format!(
                "{}/v1/runners/{}/jobs/{}/artifacts",
                daemon_url, config.runner_id, build_id
            ))
            .bearer_auth(&config.runner_token)
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Warning: failed to register artifact {}: {}", name, e);
                continue;
            }
        };

        if !resp.status().is_success() {
            eprintln!(
                "Warning: artifact registration failed for {} (HTTP {})",
                name,
                resp.status()
            );
            continue;
        }

        let create_resp: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "Warning: failed to parse artifact response for {}: {}",
                    name, e
                );
                continue;
            }
        };

        let upload_url = create_resp
            .get("upload_url")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if upload_url.is_empty() {
            println!(
                "  Registered artifact {} (no S3 upload URL — storage not configured)",
                name
            );
            continue;
        }

        match tokio::fs::read(path).await {
            Ok(bytes) => match client.put(upload_url).body(bytes).send().await {
                Ok(r) if r.status().is_success() => {
                    println!("  Uploaded artifact {}", name);
                }
                Ok(r) => {
                    eprintln!(
                        "Warning: S3 upload failed for {} (HTTP {})",
                        name,
                        r.status()
                    );
                }
                Err(e) => {
                    eprintln!("Warning: S3 upload failed for {}: {}", name, e);
                }
            },
            Err(e) => {
                eprintln!("Warning: failed to read artifact file {}: {}", name, e);
            }
        }
    }
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

    fn signing_env_from_pairs(pairs: &[(&str, &str)]) -> AndroidSigningEnv {
        use std::collections::HashMap;
        let mut values = HashMap::new();
        for (key, value) in pairs {
            values.insert((*key).to_string(), (*value).to_string());
        }
        collect_android_signing_env(|key| values.get(key).cloned())
    }

    #[test]
    fn android_signing_env_empty_is_noop() {
        let env = signing_env_from_pairs(&[]);
        let inputs = resolve_android_signing_inputs(&env).expect("resolve");
        assert!(inputs.is_none());
    }

    #[test]
    fn android_signing_requires_all_required_fields() {
        let env = signing_env_from_pairs(&[
            (OORE_ANDROID_KEYSTORE_B64_ENV, "ZmFrZS1rZXlzdG9yZQ=="),
            (OORE_ANDROID_KEYSTORE_PASSWORD_ENV, "store-pass"),
        ]);
        let err = resolve_android_signing_inputs(&env).expect_err("missing env must fail");
        assert!(err.to_string().contains(OORE_ANDROID_KEY_ALIAS_ENV));
    }

    #[test]
    fn android_signing_materializes_keystore_and_key_properties() {
        let workspace = temp_workspace();
        fs::create_dir_all(workspace.join("android/app")).expect("mkdir android/app");
        let env = signing_env_from_pairs(&[
            (
                OORE_ANDROID_KEYSTORE_B64_ENV,
                "ZmFrZS1rZXlzdG9yZS1ieXRlcw==",
            ),
            (OORE_ANDROID_KEYSTORE_PASSWORD_ENV, "store-pass"),
            (OORE_ANDROID_KEY_ALIAS_ENV, "upload"),
            (OORE_ANDROID_KEY_PASSWORD_ENV, "key-pass"),
        ]);

        let inputs = resolve_android_signing_inputs(&env)
            .expect("resolve")
            .expect("inputs");
        materialize_android_signing_files(&workspace, &inputs).expect("materialize");

        let keystore_path = workspace.join("android/app/oore-upload-keystore.jks");
        let key_properties_path = workspace.join("android/key.properties");
        let key_properties = fs::read_to_string(&key_properties_path).expect("read key.properties");

        assert!(keystore_path.exists());
        assert!(key_properties.contains("storePassword=store-pass"));
        assert!(key_properties.contains("keyPassword=key-pass"));
        assert!(key_properties.contains("keyAlias=upload"));
        assert!(key_properties.contains("storeFile=oore-upload-keystore.jks"));

        cleanup_workspace(&workspace);
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
    fn falls_back_to_ad_hoc_alias_when_release_testing_absent() {
        let help = "Available options: app-store, ad-hoc, enterprise";
        assert_eq!(choose_ios_export_method(help), "ad-hoc");
    }

    #[test]
    fn adapts_flutter_build_ios_to_signed_ipa_command() {
        let export_plist = PathBuf::from("/tmp/ExportOptions.plist");
        let command = "flutter build ios --release --no-codesign";
        let adapted =
            adapt_ios_command_for_signing(command, export_plist.as_path()).expect("adapt");
        assert!(adapted.starts_with("flutter build ipa --release"));
        assert!(!adapted.contains("--no-codesign"));
        assert!(adapted.contains("--export-options-plist=/tmp/ExportOptions.plist"));
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
        assert!(!adapted.contains("--no-codesign"));
        assert!(adapted.contains("--export-options-plist=/tmp/ExportOptions.plist"));
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
        assert!(!normalized.contains("--no-codesign"));
        assert!(normalized.contains("--dart-define-from-file=.env"));
        assert!(normalized.contains("--export-options-plist=/tmp/ExportOptions.plist"));
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
        let command = "env FOO=bar fvm flutter build ipa --release";
        let (normalized, signing_applied) = normalize_stage_command_for_execution(
            "build",
            command,
            None,
            Some(export_plist.as_path()),
        )
        .expect("normalize");

        assert!(signing_applied);
        assert!(normalized.contains("fvm flutter build ipa --release"));
        assert!(normalized.contains("--export-options-plist=/tmp/ExportOptions.plist"));
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
    fn parses_valid_codesigning_identity_hashes() {
        let output = r#"
  1) 0123456789ABCDEF0123456789ABCDEF01234567 "Apple Distribution: Example (TEAM1234)"
     1 valid identities found
"#;
        let hashes = parse_codesigning_identity_hashes(output);
        assert_eq!(hashes.len(), 1);
        assert_eq!(hashes[0], "0123456789ABCDEF0123456789ABCDEF01234567");

        let none = parse_codesigning_identity_hashes("  0 valid identities found");
        assert!(none.is_empty());
    }
}
