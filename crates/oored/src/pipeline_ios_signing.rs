use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path as FsPath, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use base64::Engine as _;
use chrono::{DateTime, NaiveDateTime, Utc};
use oore_contract::{
    ApiError, IosProvisioningProfileInput, IosProvisioningProfileSummary, IosSigningMode,
    ListPipelineIosDevicesResponse, PipelineIosSigningResponse, RegisterIosDeviceRequest,
    RegisterIosDeviceResponse, RegisteredIosDevice, RunnerIosProvisioningProfile,
    RunnerIosSigningBundle, RunnerIosSigningResponse, SyncPipelineIosSigningResponse,
    UpdatePipelineIosSigningRequest,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::apple_api::{self, AppleApiCredentials};
use crate::crypto;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::runners::RunnerAuth;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

pub const MAX_IOS_SIGNING_REQUEST_BYTES: usize = 10 * 1024 * 1024;
const MAX_P12_BYTES: usize = 2 * 1024 * 1024;
const MAX_PROFILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_API_P8_BYTES: usize = 16 * 1024;
const MAX_PROFILE_COUNT: usize = 32;
const MAX_BUNDLE_IDS: usize = 32;
const MAX_TEAM_ID_LEN: usize = 32;
const MAX_DEVICE_NAME_LEN: usize = 128;
const MAX_UDID_LEN: usize = 64;

#[derive(Debug, Clone, FromRow)]
struct IosSigningSettingsRow {
    id: String,
    enabled: i32,
    mode: String,
    team_id: Option<String>,
    export_method: String,
    bundle_ids_json: String,
    p12_filename: Option<String>,
    p12_encrypted: Option<String>,
    p12_password_encrypted: Option<String>,
    p12_fingerprint: Option<String>,
    p12_expires_at: Option<i64>,
    api_key_id: Option<String>,
    api_issuer_id: Option<String>,
    api_private_key_encrypted: Option<String>,
    updated_at: i64,
}

#[derive(Debug, Clone, FromRow)]
struct IosProvisioningProfileRow {
    id: String,
    bundle_id: String,
    profile_filename: Option<String>,
    profile_encrypted: Option<String>,
    profile_uuid: Option<String>,
    profile_name: Option<String>,
    team_id: Option<String>,
    expires_at: Option<i64>,
    checksum: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct IosDeviceRow {
    id: String,
    device_id: Option<String>,
    udid: String,
    name: String,
    platform: String,
    status: String,
    added_at: i64,
    last_synced_at: Option<i64>,
}

#[derive(Debug, Clone)]
struct ParsedProvisioningProfile {
    bundle_id: String,
    profile_uuid: Option<String>,
    profile_name: Option<String>,
    team_id: Option<String>,
    expires_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct ParsedP12Metadata {
    fingerprint: Option<String>,
    expires_at: Option<i64>,
    serial_number: Option<String>,
}

#[derive(Debug, Clone)]
struct GeneratedApiCertificateBundle {
    certificate_id: String,
    certificate_type: Option<String>,
    p12_base64: String,
    p12_password: String,
    metadata: ParsedP12Metadata,
}

fn trim_opt(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn mode_str(mode: IosSigningMode) -> &'static str {
    match mode {
        IosSigningMode::Manual => "manual",
        IosSigningMode::Api => "api",
        IosSigningMode::Hybrid => "hybrid",
    }
}

fn parse_mode(raw: &str) -> IosSigningMode {
    match raw {
        "api" => IosSigningMode::Api,
        "hybrid" => IosSigningMode::Hybrid,
        _ => IosSigningMode::Manual,
    }
}

fn parse_bundle_ids(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn encode_bundle_ids(bundle_ids: &[String]) -> String {
    serde_json::to_string(bundle_ids).unwrap_or_else(|_| "[]".to_string())
}

fn decode_b64(value: &str) -> anyhow::Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(value))
        .map_err(|e| anyhow::anyhow!("invalid base64 payload: {e}"))
}

fn encrypt_opt(value: Option<String>, key: &[u8]) -> anyhow::Result<Option<String>> {
    match value {
        Some(v) => Ok(Some(crypto::encrypt(&v, key)?)),
        None => Ok(None),
    }
}

fn decrypt_opt(value: Option<String>, key: &[u8]) -> anyhow::Result<Option<String>> {
    match value {
        Some(v) => Ok(Some(crypto::decrypt(&v, key)?)),
        None => Ok(None),
    }
}

fn is_valid_team_id(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_TEAM_ID_LEN {
        return false;
    }
    value
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
}

fn is_valid_bundle_id(value: &str) -> bool {
    if value.is_empty() || value.len() > 255 {
        return false;
    }
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' || ch == '*')
}

fn sanitize_bundle_ids(bundle_ids: &[String]) -> anyhow::Result<Vec<String>> {
    if bundle_ids.len() > MAX_BUNDLE_IDS {
        anyhow::bail!("too many bundle identifiers (max {MAX_BUNDLE_IDS})");
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for raw in bundle_ids {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !is_valid_bundle_id(trimmed) {
            anyhow::bail!("invalid bundle identifier '{trimmed}'");
        }
        if seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    Ok(out)
}

fn parse_rfc3339_ts(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc).timestamp())
}

fn parse_bundle_from_application_identifier(value: &str, team_id: Option<&str>) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(team) = team_id {
        let prefix = format!("{team}.");
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            return Some(rest.to_string());
        }
    }
    if let Some((_, rest)) = trimmed.split_once('.') {
        return Some(rest.to_string());
    }
    None
}

async fn parse_provisioning_profile(
    profile_bytes: Vec<u8>,
) -> anyhow::Result<ParsedProvisioningProfile> {
    tokio::task::spawn_blocking(move || {
        let temp_path = std::env::temp_dir().join(format!(
            "oore-ios-profile-{}.mobileprovision",
            Uuid::new_v4()
        ));
        fs::write(&temp_path, &profile_bytes).map_err(|e| {
            anyhow::anyhow!(
                "failed to write temporary provisioning profile {}: {e}",
                temp_path.display()
            )
        })?;

        let cms_output = Command::new("security")
            .arg("cms")
            .arg("-D")
            .arg("-i")
            .arg(&temp_path)
            .output()
            .map_err(|e| anyhow::anyhow!("failed to execute security cms: {e}"))?;
        let _ = fs::remove_file(&temp_path);
        if !cms_output.status.success() {
            let stderr = String::from_utf8_lossy(&cms_output.stderr);
            anyhow::bail!("failed to decode provisioning profile (security cms): {stderr}");
        }

        let mut child = Command::new("plutil")
            .arg("-convert")
            .arg("json")
            .arg("-o")
            .arg("-")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to start plutil: {e}"))?;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(&cms_output.stdout)
                .map_err(|e| anyhow::anyhow!("failed to pipe plist into plutil: {e}"))?;
        }
        let plutil_output = child
            .wait_with_output()
            .map_err(|e| anyhow::anyhow!("failed to wait for plutil: {e}"))?;
        if !plutil_output.status.success() {
            let stderr = String::from_utf8_lossy(&plutil_output.stderr);
            anyhow::bail!("failed to parse provisioning profile plist with plutil: {stderr}");
        }

        let json: serde_json::Value = serde_json::from_slice(&plutil_output.stdout)
            .map_err(|e| anyhow::anyhow!("failed to parse provisioning profile JSON: {e}"))?;

        let profile_uuid = json
            .get("UUID")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        let profile_name = json
            .get("Name")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        let team_id = json
            .get("TeamIdentifier")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        let expires_at = json
            .get("ExpirationDate")
            .and_then(|v| v.as_str())
            .and_then(parse_rfc3339_ts);

        let app_identifier = json
            .get("Entitlements")
            .and_then(|v| v.as_object())
            .and_then(|ent| ent.get("application-identifier"))
            .and_then(|v| v.as_str());
        let bundle_id = app_identifier
            .and_then(|value| parse_bundle_from_application_identifier(value, team_id.as_deref()))
            .ok_or_else(|| {
                anyhow::anyhow!("failed to derive bundle identifier from provisioning profile")
            })?;

        Ok(ParsedProvisioningProfile {
            bundle_id,
            profile_uuid,
            profile_name,
            team_id,
            expires_at,
        })
    })
    .await
    .map_err(|e| anyhow::anyhow!("parse_provisioning_profile task panicked: {e}"))?
}

fn set_strict_permissions(path: &FsPath) {
    #[cfg(unix)]
    {
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
}

fn cleanup_temp_paths(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

fn random_secret_hex() -> String {
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn normalize_serial(value: &str) -> String {
    let filtered: String = value
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .map(|ch| ch.to_ascii_uppercase())
        .collect();
    let trimmed = filtered.trim_start_matches('0');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_openssl_time(value: &str) -> Option<i64> {
    let raw = value.trim();
    if raw.is_empty() {
        return None;
    }

    DateTime::parse_from_str(raw, "%b %e %H:%M:%S %Y %Z")
        .ok()
        .map(|dt| dt.with_timezone(&Utc).timestamp())
        .or_else(|| {
            NaiveDateTime::parse_from_str(raw, "%b %e %H:%M:%S %Y")
                .ok()
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).timestamp())
        })
}

async fn parse_p12_metadata(
    p12_bytes: Vec<u8>,
    p12_password: String,
) -> anyhow::Result<ParsedP12Metadata> {
    tokio::task::spawn_blocking(move || {
        let token = Uuid::new_v4().to_string();
        let p12_path = std::env::temp_dir().join(format!("oore-ios-cert-{token}.p12"));
        let password_path = std::env::temp_dir().join(format!("oore-ios-cert-pass-{token}.txt"));
        let cert_path = std::env::temp_dir().join(format!("oore-ios-cert-{token}.pem"));

        fs::write(&p12_path, &p12_bytes).map_err(|e| {
            anyhow::anyhow!(
                "failed to write temporary p12 bundle '{}': {e}",
                p12_path.display()
            )
        })?;
        fs::write(&password_path, &p12_password).map_err(|e| {
            anyhow::anyhow!(
                "failed to write temporary p12 password '{}': {e}",
                password_path.display()
            )
        })?;
        set_strict_permissions(&p12_path);
        set_strict_permissions(&password_path);

        let passin = format!("file:{}", password_path.display());
        let cert_output = Command::new("openssl")
            .args([
                "pkcs12",
                "-in",
                p12_path.to_string_lossy().as_ref(),
                "-clcerts",
                "-nokeys",
                // Required for reading legacy-encoded (3DES/SHA1) p12 files on OpenSSL 3.x.
                "-legacy",
                "-passin",
                &passin,
            ])
            .output()
            .map_err(|e| anyhow::anyhow!("failed to execute openssl pkcs12: {e}"))?;
        if !cert_output.status.success() {
            let stderr = String::from_utf8_lossy(&cert_output.stderr);
            cleanup_temp_paths(&[p12_path, password_path, cert_path]);
            anyhow::bail!("failed to parse p12 certificate with openssl: {stderr}");
        }
        fs::write(&cert_path, &cert_output.stdout).map_err(|e| {
            anyhow::anyhow!(
                "failed to write temporary certificate '{}': {e}",
                cert_path.display()
            )
        })?;
        set_strict_permissions(&cert_path);

        let metadata_output = Command::new("openssl")
            .args([
                "x509",
                "-in",
                cert_path.to_string_lossy().as_ref(),
                "-noout",
                "-fingerprint",
                "-sha256",
                "-enddate",
                "-serial",
            ])
            .output()
            .map_err(|e| anyhow::anyhow!("failed to execute openssl x509: {e}"))?;
        cleanup_temp_paths(&[p12_path, password_path, cert_path]);
        if !metadata_output.status.success() {
            let stderr = String::from_utf8_lossy(&metadata_output.stderr);
            anyhow::bail!("failed to inspect p12 certificate metadata: {stderr}");
        }

        let stdout = String::from_utf8_lossy(&metadata_output.stdout);
        let mut metadata = ParsedP12Metadata::default();
        for line in stdout.lines() {
            let trimmed = line.trim();
            if let Some((_, value)) = trimmed.split_once("Fingerprint=") {
                metadata.fingerprint = Some(value.replace(':', "").to_uppercase());
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("notAfter=") {
                metadata.expires_at = parse_openssl_time(value);
                continue;
            }
            if let Some(value) = trimmed.strip_prefix("serial=") {
                metadata.serial_number = Some(normalize_serial(value));
            }
        }

        Ok(metadata)
    })
    .await
    .map_err(|e| anyhow::anyhow!("parse_p12_metadata task panicked: {e}"))?
}

async fn verify_p12_keychain_import_compatibility(
    p12_bytes: Vec<u8>,
    p12_password: String,
) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || {
        let token = Uuid::new_v4().to_string();
        let p12_path = std::env::temp_dir().join(format!("oore-ios-import-check-{token}.p12"));
        let keychain_path =
            std::env::temp_dir().join(format!("oore-ios-import-check-{token}.keychain-db"));
        let keychain_password = random_secret_hex();

        fs::write(&p12_path, &p12_bytes).map_err(|e| {
            anyhow::anyhow!(
                "failed to write temporary p12 for keychain compatibility check '{}': {e}",
                p12_path.display()
            )
        })?;
        set_strict_permissions(&p12_path);

        let cleanup = || {
            let _ = Command::new("security")
                .args(["delete-keychain", keychain_path.to_string_lossy().as_ref()])
                .output();
            let _ = fs::remove_file(&keychain_path);
            let _ = fs::remove_file(&p12_path);
        };

        let create_output = Command::new("security")
            .args([
                "create-keychain",
                "-p",
                &keychain_password,
                keychain_path.to_string_lossy().as_ref(),
            ])
            .output()
            .map_err(|e| anyhow::anyhow!("failed to execute security create-keychain: {e}"))?;
        if !create_output.status.success() {
            let stderr = String::from_utf8_lossy(&create_output.stderr);
            cleanup();
            anyhow::bail!(
                "failed to create temporary keychain for iOS signing validation: {stderr}"
            );
        }

        let unlock_output = Command::new("security")
            .args([
                "unlock-keychain",
                "-p",
                &keychain_password,
                keychain_path.to_string_lossy().as_ref(),
            ])
            .output()
            .map_err(|e| anyhow::anyhow!("failed to execute security unlock-keychain: {e}"))?;
        if !unlock_output.status.success() {
            let stderr = String::from_utf8_lossy(&unlock_output.stderr);
            cleanup();
            anyhow::bail!(
                "failed to unlock temporary keychain for iOS signing validation: {stderr}"
            );
        }

        let import_output = Command::new("security")
            .args([
                "import",
                p12_path.to_string_lossy().as_ref(),
                "-k",
                keychain_path.to_string_lossy().as_ref(),
                "-P",
                &p12_password,
                "-T",
                "/usr/bin/codesign",
                "-T",
                "/usr/bin/security",
                "-T",
                "/usr/bin/xcodebuild",
            ])
            .output()
            .map_err(|e| anyhow::anyhow!("failed to execute security import: {e}"))?;

        cleanup();

        if !import_output.status.success() {
            let stderr = String::from_utf8_lossy(&import_output.stderr);
            anyhow::bail!("failed to import p12 into macOS keychain: {stderr}");
        }

        Ok(())
    })
    .await
    .map_err(|e| anyhow::anyhow!("verify_p12_keychain_import_compatibility task panicked: {e}"))?
}

/// Re-export a p12 bundle using legacy 3DES+SHA1 encoding that macOS `security import` supports.
/// Takes the existing p12 bytes and password, extracts key+cert via openssl, then re-exports
/// with `-certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg SHA1`.
/// Returns `(new_p12_bytes, new_password)` on success.
async fn reexport_p12_legacy(
    p12_bytes: Vec<u8>,
    p12_password: String,
) -> anyhow::Result<(Vec<u8>, String)> {
    tokio::task::spawn_blocking(move || reexport_p12_legacy_blocking(&p12_bytes, &p12_password))
        .await
        .map_err(|e| anyhow::anyhow!("reexport_p12_legacy task panicked: {e}"))?
}

fn reexport_p12_legacy_blocking(
    p12_bytes: &[u8],
    p12_password: &str,
) -> anyhow::Result<(Vec<u8>, String)> {
    let token = Uuid::new_v4().to_string();
    let old_p12_path = std::env::temp_dir().join(format!("oore-ios-reexport-old-{token}.p12"));
    let old_pass_path = std::env::temp_dir().join(format!("oore-ios-reexport-oldpass-{token}.txt"));
    let key_path = std::env::temp_dir().join(format!("oore-ios-reexport-key-{token}.pem"));
    let cert_path = std::env::temp_dir().join(format!("oore-ios-reexport-cert-{token}.pem"));
    let new_pass_path = std::env::temp_dir().join(format!("oore-ios-reexport-newpass-{token}.txt"));
    let new_p12_path = std::env::temp_dir().join(format!("oore-ios-reexport-new-{token}.p12"));

    let all_paths = vec![
        old_p12_path.clone(),
        old_pass_path.clone(),
        key_path.clone(),
        cert_path.clone(),
        new_pass_path.clone(),
        new_p12_path.clone(),
    ];

    fs::write(&old_p12_path, p12_bytes)
        .map_err(|e| anyhow::anyhow!("failed to write temp p12 for re-export: {e}"))?;
    fs::write(&old_pass_path, p12_password)
        .map_err(|e| anyhow::anyhow!("failed to write temp p12 password for re-export: {e}"))?;
    set_strict_permissions(&old_p12_path);
    set_strict_permissions(&old_pass_path);

    let passin = format!("file:{}", old_pass_path.display());

    // Extract private key
    let key_out = Command::new("openssl")
        .args([
            "pkcs12",
            "-in",
            old_p12_path.to_string_lossy().as_ref(),
            "-nocerts",
            "-nodes",
            "-legacy",
            "-passin",
            &passin,
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run openssl pkcs12 key extraction: {e}"))?;
    if !key_out.status.success() {
        let stderr = String::from_utf8_lossy(&key_out.stderr);
        cleanup_temp_paths(&all_paths);
        anyhow::bail!("failed to extract private key from p12: {stderr}");
    }
    fs::write(&key_path, &key_out.stdout)
        .map_err(|e| anyhow::anyhow!("failed to write extracted key: {e}"))?;
    set_strict_permissions(&key_path);

    // Extract certificate
    let cert_out = Command::new("openssl")
        .args([
            "pkcs12",
            "-in",
            old_p12_path.to_string_lossy().as_ref(),
            "-clcerts",
            "-nokeys",
            "-legacy",
            "-passin",
            &passin,
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run openssl pkcs12 cert extraction: {e}"))?;
    if !cert_out.status.success() {
        let stderr = String::from_utf8_lossy(&cert_out.stderr);
        cleanup_temp_paths(&all_paths);
        anyhow::bail!("failed to extract certificate from p12: {stderr}");
    }
    fs::write(&cert_path, &cert_out.stdout)
        .map_err(|e| anyhow::anyhow!("failed to write extracted cert: {e}"))?;
    set_strict_permissions(&cert_path);

    // Re-export with legacy encoding
    let new_password = random_secret_hex();
    fs::write(&new_pass_path, &new_password)
        .map_err(|e| anyhow::anyhow!("failed to write new p12 password: {e}"))?;
    set_strict_permissions(&new_pass_path);

    let passout = format!("file:{}", new_pass_path.display());
    let export_out = Command::new("openssl")
        .args([
            "pkcs12",
            "-export",
            "-certpbe",
            "PBE-SHA1-3DES",
            "-keypbe",
            "PBE-SHA1-3DES",
            "-macalg",
            "SHA1",
            "-inkey",
            key_path.to_string_lossy().as_ref(),
            "-in",
            cert_path.to_string_lossy().as_ref(),
            "-name",
            "oore.build iOS Distribution",
            "-out",
            new_p12_path.to_string_lossy().as_ref(),
            "-passout",
            &passout,
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run openssl pkcs12 re-export: {e}"))?;
    if !export_out.status.success() {
        let stderr = String::from_utf8_lossy(&export_out.stderr);
        cleanup_temp_paths(&all_paths);
        anyhow::bail!("failed to re-export p12 with legacy encoding: {stderr}");
    }

    let new_bytes = fs::read(&new_p12_path)
        .map_err(|e| anyhow::anyhow!("failed to read re-exported p12: {e}"))?;
    cleanup_temp_paths(&all_paths);
    Ok((new_bytes, new_password))
}

fn generate_profile_filename(bundle_id: &str, profile_uuid: Option<&String>) -> String {
    if let Some(uuid) = profile_uuid {
        if !uuid.trim().is_empty() {
            return format!("{uuid}.mobileprovision");
        }
    }
    format!("{bundle_id}.mobileprovision")
}

fn parse_rfc3339_opt(value: Option<String>) -> Option<i64> {
    value
        .as_deref()
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|dt| dt.with_timezone(&Utc).timestamp())
}

fn profile_name_for_bundle(bundle_id: &str) -> String {
    let mut compact = bundle_id.replace(|ch: char| !ch.is_ascii_alphanumeric(), "-");
    if compact.len() > 70 {
        compact.truncate(70);
    }
    format!("oore-adhoc-{compact}")
}

async fn generate_api_certificate_bundle(
    client: &reqwest::Client,
    creds: &AppleApiCredentials,
) -> Result<GeneratedApiCertificateBundle, (StatusCode, Json<ApiError>)> {
    let token = Uuid::new_v4().to_string();
    let key_path = std::env::temp_dir().join(format!("oore-ios-key-{token}.pem"));
    let csr_path = std::env::temp_dir().join(format!("oore-ios-csr-{token}.pem"));
    let cert_der_path = std::env::temp_dir().join(format!("oore-ios-cert-{token}.der"));
    let cert_pem_path = std::env::temp_dir().join(format!("oore-ios-cert-{token}.pem"));
    let password_path = std::env::temp_dir().join(format!("oore-ios-p12-pass-{token}.txt"));
    let p12_path = std::env::temp_dir().join(format!("oore-ios-p12-{token}.p12"));

    let cleanup_paths = vec![
        key_path.clone(),
        csr_path.clone(),
        cert_der_path.clone(),
        cert_pem_path.clone(),
        password_path.clone(),
        p12_path.clone(),
    ];

    // Phase 1: Generate key + CSR (blocking openssl commands)
    let csr_content = {
        let key_path = key_path.clone();
        let csr_path = csr_path.clone();
        let cleanup_paths = cleanup_paths.clone();
        tokio::task::spawn_blocking(move || {
            let gen_key = Command::new("openssl")
                .args([
                    "genrsa",
                    "-out",
                    key_path.to_string_lossy().as_ref(),
                    "2048",
                ])
                .output()
                .map_err(|e| {
                    error!(error = %e, "failed to execute openssl genrsa for API certificate");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "ios_signing_error",
                        "Failed to generate iOS certificate key material",
                    )
                })?;
            if !gen_key.status.success() {
                let stderr = String::from_utf8_lossy(&gen_key.stderr);
                cleanup_temp_paths(&cleanup_paths);
                return Err(api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ios_signing_error",
                    format!("Failed to generate iOS private key: {stderr}"),
                ));
            }
            set_strict_permissions(&key_path);

            let gen_csr = Command::new("openssl")
                .args([
                    "req",
                    "-new",
                    "-key",
                    key_path.to_string_lossy().as_ref(),
                    "-subj",
                    "/CN=oore.build iOS Distribution",
                    "-out",
                    csr_path.to_string_lossy().as_ref(),
                ])
                .output()
                .map_err(|e| {
                    error!(error = %e, "failed to execute openssl req for API certificate");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "ios_signing_error",
                        "Failed to generate iOS certificate signing request",
                    )
                })?;
            if !gen_csr.status.success() {
                let stderr = String::from_utf8_lossy(&gen_csr.stderr);
                cleanup_temp_paths(&cleanup_paths);
                return Err(api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ios_signing_error",
                    format!("Failed to generate iOS certificate signing request: {stderr}"),
                ));
            }

            let csr_content = fs::read_to_string(&csr_path).map_err(|e| {
                cleanup_temp_paths(&cleanup_paths);
                error!(error = %e, "failed to read generated iOS CSR");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ios_signing_error",
                    "Failed to read generated iOS certificate signing request",
                )
            })?;
            Ok(csr_content)
        })
        .await
        .map_err(|e| {
            error!(error = %e, "generate_api_certificate_bundle key/CSR task panicked");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ios_signing_error",
                "Failed to generate iOS certificate key material",
            )
        })??
    };

    // Phase 2: Submit CSR to Apple API (async)
    let created_cert =
        apple_api::create_distribution_certificate(client, creds, &csr_content).await?;
    let cert_content = created_cert.certificate_content.clone().ok_or_else(|| {
        cleanup_temp_paths(&cleanup_paths);
        api_err(
            StatusCode::BAD_GATEWAY,
            "apple_api_error",
            "Apple returned certificate without certificate content",
        )
    })?;
    let cert_der = decode_b64(&cert_content).map_err(|e| {
        cleanup_temp_paths(&cleanup_paths);
        error!(error = %e, "failed to decode Apple certificate content");
        api_err(
            StatusCode::BAD_GATEWAY,
            "apple_api_error",
            "Apple returned invalid certificate content",
        )
    })?;

    // Phase 3: Convert cert + export p12 (blocking openssl commands)
    let (mut p12_bytes, mut p12_password) = {
        let key_path = key_path.clone();
        let cert_der_path = cert_der_path.clone();
        let cert_pem_path = cert_pem_path.clone();
        let password_path = password_path.clone();
        let p12_path = p12_path.clone();
        let cleanup_paths = cleanup_paths.clone();
        tokio::task::spawn_blocking(move || {
            fs::write(&cert_der_path, cert_der).map_err(|e| {
                cleanup_temp_paths(&cleanup_paths);
                error!(error = %e, "failed to write temporary Apple certificate DER");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ios_signing_error",
                    "Failed to materialize Apple certificate",
                )
            })?;

            let convert_cert = Command::new("openssl")
                .args([
                    "x509",
                    "-inform",
                    "DER",
                    "-in",
                    cert_der_path.to_string_lossy().as_ref(),
                    "-out",
                    cert_pem_path.to_string_lossy().as_ref(),
                ])
                .output()
                .map_err(|e| {
                    cleanup_temp_paths(&cleanup_paths);
                    error!(error = %e, "failed to execute openssl x509 DER conversion");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "ios_signing_error",
                        "Failed to convert Apple certificate format",
                    )
                })?;
            if !convert_cert.status.success() {
                let stderr = String::from_utf8_lossy(&convert_cert.stderr);
                cleanup_temp_paths(&cleanup_paths);
                return Err(api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ios_signing_error",
                    format!("Failed to convert Apple certificate format: {stderr}"),
                ));
            }

            let p12_password = random_secret_hex();
            fs::write(&password_path, &p12_password).map_err(|e| {
                cleanup_temp_paths(&cleanup_paths);
                error!(error = %e, "failed to write temporary p12 password file");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ios_signing_error",
                    "Failed to prepare p12 export password",
                )
            })?;
            set_strict_permissions(&password_path);

            let passout = format!("file:{}", password_path.display());
            let export_p12 = Command::new("openssl")
                .args([
                    "pkcs12",
                    "-export",
                    // Force legacy PKCS#12 encoding that macOS security import
                    // (SecKeychainItemImport) actually supports.
                    "-certpbe",
                    "PBE-SHA1-3DES",
                    "-keypbe",
                    "PBE-SHA1-3DES",
                    "-macalg",
                    "SHA1",
                    "-inkey",
                    key_path.to_string_lossy().as_ref(),
                    "-in",
                    cert_pem_path.to_string_lossy().as_ref(),
                    "-name",
                    "oore.build iOS Distribution",
                    "-out",
                    p12_path.to_string_lossy().as_ref(),
                    "-passout",
                    &passout,
                ])
                .output()
                .map_err(|e| {
                    cleanup_temp_paths(&cleanup_paths);
                    error!(error = %e, "failed to execute openssl pkcs12 export");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "ios_signing_error",
                        "Failed to export generated iOS certificate bundle",
                    )
                })?;
            if !export_p12.status.success() {
                let stderr = String::from_utf8_lossy(&export_p12.stderr);
                cleanup_temp_paths(&cleanup_paths);
                return Err(api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ios_signing_error",
                    format!("Failed to export generated iOS certificate bundle: {stderr}"),
                ));
            }

            let p12_bytes = fs::read(&p12_path).map_err(|e| {
                cleanup_temp_paths(&cleanup_paths);
                error!(error = %e, "failed to read generated p12");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ios_signing_error",
                    "Failed to load generated iOS certificate bundle",
                )
            })?;
            cleanup_temp_paths(&cleanup_paths);
            Ok((p12_bytes, p12_password))
        })
        .await
        .map_err(|e| {
            error!(error = %e, "generate_api_certificate_bundle export task panicked");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ios_signing_error",
                "Failed to export generated iOS certificate bundle",
            )
        })??
    };

    // Phase 4: Parse p12 metadata (already async via spawn_blocking)
    let mut metadata = parse_p12_metadata(p12_bytes.clone(), p12_password.clone())
        .await
        .map_err(|e| {
            error!(error = %e, "failed to inspect generated p12 metadata");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ios_signing_error",
                "Failed to inspect generated iOS certificate metadata",
            )
        })?;

    // Ensure generated certificate material can actually be imported by macOS security tooling.
    if let Err(err) =
        verify_p12_keychain_import_compatibility(p12_bytes.clone(), p12_password.clone()).await
    {
        warn!(
            certificate_id = %created_cert.certificate_id,
            error = %err,
            "generated API certificate bundle failed keychain import; re-exporting with legacy encoding"
        );
        let (reexported_bytes, reexported_password) = reexport_p12_legacy(
            p12_bytes.clone(),
            p12_password.clone(),
        )
        .await
        .map_err(|e| {
            error!(
                certificate_id = %created_cert.certificate_id,
                error = %e,
                "failed to re-export generated API certificate bundle"
            );
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ios_signing_error",
                "Generated iOS certificate bundle is not compatible with macOS keychain import",
            )
        })?;
        verify_p12_keychain_import_compatibility(
            reexported_bytes.clone(),
            reexported_password.clone(),
        )
        .await
        .map_err(|e| {
            error!(
                certificate_id = %created_cert.certificate_id,
                error = %e,
                "re-exported generated API certificate bundle still failed keychain import"
            );
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ios_signing_error",
                "Generated iOS certificate bundle is not compatible with macOS keychain import",
            )
        })?;
        metadata = parse_p12_metadata(reexported_bytes.clone(), reexported_password.clone())
            .await
            .unwrap_or(metadata);
        p12_bytes = reexported_bytes;
        p12_password = reexported_password;
    }

    let p12_base64 = base64::engine::general_purpose::STANDARD.encode(&p12_bytes);

    Ok(GeneratedApiCertificateBundle {
        certificate_id: created_cert.certificate_id,
        certificate_type: created_cert.certificate_type,
        p12_base64,
        p12_password,
        metadata,
    })
}

async fn ensure_pipeline_exists(pool: &SqlitePool, pipeline_id: &str) -> anyhow::Result<bool> {
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM pipelines WHERE id = ?1")
        .bind(pipeline_id)
        .fetch_optional(pool)
        .await?;
    Ok(exists.is_some())
}

async fn load_ios_settings(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> anyhow::Result<Option<IosSigningSettingsRow>> {
    let row = sqlx::query_as::<_, IosSigningSettingsRow>(
        "SELECT id, enabled, mode, team_id, export_method, bundle_ids_json,
                p12_filename, p12_encrypted, p12_password_encrypted, p12_fingerprint, p12_expires_at,
                api_key_id, api_issuer_id, api_private_key_encrypted, updated_at
         FROM pipeline_ios_signing_settings WHERE pipeline_id = ?1",
    )
    .bind(pipeline_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

async fn load_ios_profiles(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> anyhow::Result<Vec<IosProvisioningProfileRow>> {
    let rows = sqlx::query_as::<_, IosProvisioningProfileRow>(
        "SELECT id, bundle_id, profile_filename, profile_encrypted, profile_uuid,
                profile_name, team_id, expires_at, checksum
         FROM pipeline_ios_provisioning_profiles
         WHERE pipeline_id = ?1 ORDER BY bundle_id ASC",
    )
    .bind(pipeline_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

async fn load_ios_devices(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> anyhow::Result<Vec<IosDeviceRow>> {
    let rows = sqlx::query_as::<_, IosDeviceRow>(
        "SELECT id, device_id, udid, name, platform, status, added_at, last_synced_at
         FROM pipeline_ios_signing_devices
         WHERE pipeline_id = ?1 ORDER BY added_at DESC",
    )
    .bind(pipeline_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

fn profile_row_to_summary(row: &IosProvisioningProfileRow) -> IosProvisioningProfileSummary {
    IosProvisioningProfileSummary {
        bundle_id: row.bundle_id.clone(),
        has_profile: row.profile_encrypted.is_some(),
        profile_filename: row.profile_filename.clone(),
        profile_uuid: row.profile_uuid.clone(),
        profile_name: row.profile_name.clone(),
        team_id: row.team_id.clone(),
        expires_at: row.expires_at,
        checksum: row.checksum.clone(),
    }
}

fn device_row_to_contract(row: IosDeviceRow) -> RegisteredIosDevice {
    RegisteredIosDevice {
        id: row.id,
        device_id: row.device_id,
        udid: row.udid,
        name: row.name,
        platform: row.platform,
        status: row.status,
        added_at: row.added_at,
        last_synced_at: row.last_synced_at,
    }
}

fn default_ios_signing_response(pipeline_id: String) -> PipelineIosSigningResponse {
    PipelineIosSigningResponse {
        pipeline_id,
        enabled: false,
        mode: IosSigningMode::Manual,
        team_id: None,
        export_method: "ad_hoc".to_string(),
        bundle_ids: Vec::new(),
        has_p12: false,
        p12_filename: None,
        p12_fingerprint: None,
        p12_expires_at: None,
        has_p12_password: false,
        has_api_key: false,
        api_key_id: None,
        api_issuer_id: None,
        provisioning_profiles: Vec::new(),
        updated_at: None,
    }
}

async fn build_public_response(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> anyhow::Result<PipelineIosSigningResponse> {
    let settings = load_ios_settings(pool, pipeline_id).await?;
    let profiles = load_ios_profiles(pool, pipeline_id).await?;
    let Some(settings) = settings else {
        return Ok(default_ios_signing_response(pipeline_id.to_string()));
    };

    Ok(PipelineIosSigningResponse {
        pipeline_id: pipeline_id.to_string(),
        enabled: settings.enabled != 0,
        mode: parse_mode(&settings.mode),
        team_id: settings.team_id,
        export_method: settings.export_method,
        bundle_ids: parse_bundle_ids(&settings.bundle_ids_json),
        has_p12: settings.p12_encrypted.is_some(),
        p12_filename: settings.p12_filename,
        p12_fingerprint: settings.p12_fingerprint,
        p12_expires_at: settings.p12_expires_at,
        has_p12_password: settings.p12_password_encrypted.is_some(),
        has_api_key: settings.api_private_key_encrypted.is_some(),
        api_key_id: settings.api_key_id,
        api_issuer_id: settings.api_issuer_id,
        provisioning_profiles: profiles.iter().map(profile_row_to_summary).collect(),
        updated_at: Some(settings.updated_at),
    })
}

fn validate_udid(udid: &str) -> bool {
    if udid.is_empty() || udid.len() > MAX_UDID_LEN {
        return false;
    }
    udid.chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}

fn load_api_credentials(
    settings: &IosSigningSettingsRow,
    encryption_key: &[u8],
) -> anyhow::Result<Option<AppleApiCredentials>> {
    let (Some(key_id), Some(issuer_id), Some(private_key_encrypted)) = (
        settings.api_key_id.clone(),
        settings.api_issuer_id.clone(),
        settings.api_private_key_encrypted.clone(),
    ) else {
        return Ok(None);
    };
    let private_key_pem = crypto::decrypt(&private_key_encrypted, encryption_key)?;
    Ok(Some(AppleApiCredentials {
        key_id,
        issuer_id,
        private_key_pem,
    }))
}

async fn upsert_cached_apple_devices(
    pool: &SqlitePool,
    pipeline_id: &str,
    actor_id: &str,
    devices: &[apple_api::AppleDeviceRecord],
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let existing = load_ios_devices(pool, pipeline_id)
        .await
        .unwrap_or_default();
    let mut existing_by_udid: HashMap<String, IosDeviceRow> = existing
        .into_iter()
        .map(|row| (row.udid.to_uppercase(), row))
        .collect();
    let now = now_unix();

    for remote in devices {
        let udid = remote.udid.to_uppercase();
        if let Some(existing) = existing_by_udid.remove(&udid) {
            sqlx::query(
                "UPDATE pipeline_ios_signing_devices
                 SET device_id = ?1, name = ?2, platform = ?3, status = ?4,
                     last_synced_at = ?5, updated_by = ?6, updated_at = ?5
                 WHERE id = ?7",
            )
            .bind(&remote.device_id)
            .bind(&remote.name)
            .bind(&remote.platform)
            .bind(&remote.status)
            .bind(now)
            .bind(actor_id)
            .bind(existing.id)
            .execute(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to update cached Apple iOS device");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to persist synced iOS device cache",
                )
            })?;
        } else {
            sqlx::query(
                "INSERT INTO pipeline_ios_signing_devices (
                    id, pipeline_id, device_id, udid, name, platform, status,
                    added_at, last_synced_at, created_by, updated_by, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9, ?8, ?8)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(pipeline_id)
            .bind(&remote.device_id)
            .bind(&udid)
            .bind(&remote.name)
            .bind(&remote.platform)
            .bind(&remote.status)
            .bind(now)
            .bind(actor_id)
            .execute(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to insert cached Apple iOS device");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to persist synced iOS device cache",
                )
            })?;
        }
    }

    Ok(())
}

async fn sync_ios_signing_assets(
    pool: &SqlitePool,
    encryption_key: &[u8],
    pipeline_id: &str,
    settings: &IosSigningSettingsRow,
    actor_id: &str,
    creds: &AppleApiCredentials,
) -> Result<(usize, Vec<String>), (StatusCode, Json<ApiError>)> {
    let bundle_ids = parse_bundle_ids(&settings.bundle_ids_json);
    if bundle_ids.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_state",
            "iOS signing sync requires at least one configured bundle identifier",
        ));
    }

    let client = reqwest::Client::new();
    let remote_devices = apple_api::list_devices(&client, creds).await?;

    let bundle_records = apple_api::list_bundle_ids(&client, creds, &bundle_ids).await?;
    let bundle_by_identifier: HashMap<String, apple_api::AppleBundleIdRecord> = bundle_records
        .into_iter()
        .map(|record| (record.identifier.clone(), record))
        .collect();

    let mut missing_bundle_ids = Vec::new();
    for bundle_id in &bundle_ids {
        if !bundle_by_identifier.contains_key(bundle_id) {
            missing_bundle_ids.push(bundle_id.clone());
        }
    }
    if !missing_bundle_ids.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_state",
            format!(
                "These bundle identifiers are not available in Apple Developer account: {}",
                missing_bundle_ids.join(", ")
            ),
        ));
    }

    let mut cert_records = apple_api::list_distribution_certificates(&client, creds).await?;
    let mut warnings = Vec::new();
    let mode = parse_mode(&settings.mode);

    let mut existing_p12 =
        decrypt_opt(settings.p12_encrypted.clone(), encryption_key).map_err(|e| {
            error!(error = %e, "failed to decrypt stored iOS p12 during sync");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "decryption_error",
                "Failed to load stored iOS certificate material",
            )
        })?;
    let mut existing_password =
        decrypt_opt(settings.p12_password_encrypted.clone(), encryption_key).map_err(|e| {
            error!(error = %e, "failed to decrypt stored iOS p12 password during sync");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "decryption_error",
                "Failed to load stored iOS certificate password",
            )
        })?;

    if matches!(mode, IosSigningMode::Api) && (existing_p12.is_some() ^ existing_password.is_some())
    {
        warn!(
            pipeline_id = %pipeline_id,
            "stored API-mode p12 material is incomplete; forcing certificate regeneration"
        );
        warnings.push(
            "Stored API-mode iOS certificate material was incomplete and will be regenerated"
                .to_string(),
        );
        existing_p12 = None;
        existing_password = None;
    }

    let mut p12_was_reexported = false;
    let p12_metadata = match (existing_p12.clone(), existing_password.clone()) {
        (Some(p12_base64), Some(password)) => {
            let p12_bytes = match decode_b64(&p12_base64) {
                Ok(bytes) => bytes,
                Err(e) => {
                    if matches!(mode, IosSigningMode::Api) {
                        warn!(
                            pipeline_id = %pipeline_id,
                            error = %e,
                            "stored API-mode p12 payload is invalid; forcing certificate regeneration"
                        );
                        warnings.push(
                            "Stored API-mode iOS certificate payload was invalid and has been discarded"
                                .to_string(),
                        );
                        existing_p12 = None;
                        existing_password = None;
                        Vec::new()
                    } else {
                        error!(error = %e, "invalid stored iOS p12 base64");
                        return Err(api_err(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "invalid_state",
                            "Stored iOS certificate material is invalid",
                        ));
                    }
                }
            };
            if p12_bytes.is_empty() {
                None
            } else {
                match parse_p12_metadata(p12_bytes.clone(), password.clone()).await {
                    Ok(metadata) => {
                        if matches!(mode, IosSigningMode::Api) {
                            if let Err(e) = verify_p12_keychain_import_compatibility(
                                p12_bytes.clone(),
                                password.clone(),
                            )
                            .await
                            {
                                warn!(
                                    pipeline_id = %pipeline_id,
                                    error = %e,
                                    "stored API-mode p12 fails macOS keychain import; attempting re-export with legacy encoding"
                                );
                                match reexport_p12_legacy(p12_bytes.clone(), password.clone()).await
                                {
                                    Ok((new_bytes, new_password)) => {
                                        if let Err(e2) = verify_p12_keychain_import_compatibility(
                                            new_bytes.clone(),
                                            new_password.clone(),
                                        )
                                        .await
                                        {
                                            warn!(
                                                pipeline_id = %pipeline_id,
                                                error = %e2,
                                                "re-exported p12 still fails keychain import; forcing certificate regeneration"
                                            );
                                            warnings.push(
                                                "Stored API-mode iOS certificate could not be re-encoded for macOS compatibility and will be regenerated"
                                                    .to_string(),
                                            );
                                            existing_p12 = None;
                                            existing_password = None;
                                            None
                                        } else {
                                            let new_b64 = base64::engine::general_purpose::STANDARD
                                                .encode(&new_bytes);
                                            let new_metadata =
                                                parse_p12_metadata(new_bytes, new_password.clone())
                                                    .await
                                                    .unwrap_or(metadata);
                                            existing_p12 = Some(new_b64);
                                            existing_password = Some(new_password);
                                            p12_was_reexported = true;
                                            warnings.push(
                                                "Re-exported stored iOS certificate with macOS-compatible encoding"
                                                    .to_string(),
                                            );
                                            Some(new_metadata)
                                        }
                                    }
                                    Err(e2) => {
                                        warn!(
                                            pipeline_id = %pipeline_id,
                                            error = %e2,
                                            "failed to re-export p12 with legacy encoding; forcing certificate regeneration"
                                        );
                                        warnings.push(
                                            "Stored API-mode iOS certificate uses incompatible encoding and will be regenerated"
                                                .to_string(),
                                        );
                                        existing_p12 = None;
                                        existing_password = None;
                                        None
                                    }
                                }
                            } else {
                                Some(metadata)
                            }
                        } else {
                            Some(metadata)
                        }
                    }
                    Err(e) => {
                        if matches!(mode, IosSigningMode::Api) {
                            warn!(
                                pipeline_id = %pipeline_id,
                                error = %e,
                                "stored API-mode p12/password pair is invalid; forcing certificate regeneration"
                            );
                            warnings.push(
                                "Stored API-mode iOS certificate password did not match certificate payload and has been discarded"
                                    .to_string(),
                            );
                            existing_p12 = None;
                            existing_password = None;
                            None
                        } else {
                            error!(error = %e, "failed to inspect stored iOS p12 metadata");
                            return Err(api_err(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "invalid_state",
                                "Stored iOS certificate metadata could not be inspected",
                            ));
                        }
                    }
                }
            }
        }
        _ => None,
    };

    let p12_serial = p12_metadata
        .as_ref()
        .and_then(|metadata| metadata.serial_number.clone());

    let mut selected_certificate_id = None;
    if let Some(serial) = p12_serial {
        selected_certificate_id = cert_records
            .iter()
            .find(|record| {
                record
                    .serial_number
                    .as_deref()
                    .map(normalize_serial)
                    .as_deref()
                    == Some(serial.as_str())
            })
            .map(|record| record.certificate_id.clone());
    }

    let should_generate_cert = if selected_certificate_id.is_some() {
        false
    } else if existing_p12.is_none() || existing_password.is_none() {
        matches!(mode, IosSigningMode::Api)
    } else if p12_metadata.is_some() {
        matches!(mode, IosSigningMode::Api)
    } else {
        false
    };

    let mut generated_certificate = None;
    if should_generate_cert {
        match generate_api_certificate_bundle(&client, creds).await {
            Ok(bundle) => {
                generated_certificate = Some(bundle);
            }
            Err((status, err)) => {
                if cert_records.is_empty() {
                    return Err((status, err));
                }
                warn!(
                    pipeline_id = %pipeline_id,
                    code = %err.code,
                    error = %err.error,
                    http_status = %status,
                    "failed to generate API-mode iOS certificate bundle; continuing with existing Apple certificate record"
                );
                warnings.push(format!(
                    "Could not generate a new iOS certificate bundle from Apple API ({}) - continuing with existing Apple certificate references",
                    err.error
                ));
            }
        }
    }

    if let Some(generated) = generated_certificate.as_ref() {
        selected_certificate_id = Some(generated.certificate_id.clone());
        cert_records.push(apple_api::AppleCertificateRecord {
            certificate_id: generated.certificate_id.clone(),
            certificate_content: None,
            certificate_type: generated.certificate_type.clone(),
            serial_number: generated.metadata.serial_number.clone(),
            expiration_date: generated.metadata.expires_at.map(|ts| ts.to_string()),
        });
    }

    if selected_certificate_id.is_none() {
        if p12_metadata.is_some() && matches!(mode, IosSigningMode::Hybrid) {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_state",
                "Stored hybrid-mode p12 certificate does not match any Apple IOS_DISTRIBUTION certificate; upload matching material or switch to API mode and sync",
            ));
        }
        if p12_metadata.is_some() && generated_certificate.is_none() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_state",
                "Stored iOS p12 certificate does not match any Apple distribution certificate and no replacement certificate could be generated",
            ));
        }
        selected_certificate_id = cert_records
            .first()
            .map(|record| record.certificate_id.clone());
    }

    let Some(certificate_id) = selected_certificate_id else {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_state",
            "No iOS distribution certificate is available in Apple account for profile generation",
        ));
    };

    if matches!(mode, IosSigningMode::Api)
        && existing_p12.is_none()
        && generated_certificate.is_none()
    {
        warnings.push(
            "No local iOS .p12 certificate bundle is stored for API mode; profile sync can proceed but signed iOS builds may fail until certificate material is available".to_string(),
        );
    }

    let enabled_device_ids: Vec<String> = remote_devices
        .iter()
        .filter(|device| device.status.eq_ignore_ascii_case("ENABLED"))
        .map(|device| device.device_id.clone())
        .collect();
    if enabled_device_ids.is_empty() {
        warnings.push(
            "No enabled Apple devices found; generated profiles may not install on devices"
                .to_string(),
        );
    }

    let mut generated_profiles = Vec::new();
    for bundle_id in &bundle_ids {
        let bundle_record = bundle_by_identifier.get(bundle_id).ok_or_else(|| {
            api_err(
                StatusCode::BAD_REQUEST,
                "invalid_state",
                format!("Bundle identifier '{bundle_id}' is not available in Apple account"),
            )
        })?;
        let profile_name = profile_name_for_bundle(bundle_id);
        let created_profile = apple_api::create_ad_hoc_profile(
            &client,
            creds,
            &profile_name,
            &bundle_record.bundle_id_id,
            std::slice::from_ref(&certificate_id),
            &enabled_device_ids,
        )
        .await?;

        let profile_content = created_profile.profile_content.clone().ok_or_else(|| {
            api_err(
                StatusCode::BAD_GATEWAY,
                "apple_api_error",
                format!(
                    "Apple profile response for bundle '{bundle_id}' did not include profile content"
                ),
            )
        })?;
        let profile_bytes = decode_b64(&profile_content).map_err(|e| {
            error!(error = %e, "failed to decode Apple profile content");
            api_err(
                StatusCode::BAD_GATEWAY,
                "apple_api_error",
                format!("Apple returned invalid profile content for '{bundle_id}'"),
            )
        })?;
        if profile_bytes.is_empty() || profile_bytes.len() > MAX_PROFILE_BYTES {
            return Err(api_err(
                StatusCode::BAD_GATEWAY,
                "apple_api_error",
                format!(
                    "Apple returned profile content for '{bundle_id}' outside allowed size limits"
                ),
            ));
        }

        let parsed_profile = match parse_provisioning_profile(profile_bytes.clone()).await {
            Ok(parsed) => {
                if parsed.bundle_id != *bundle_id && parsed.bundle_id != "*" {
                    return Err(api_err(
                        StatusCode::BAD_GATEWAY,
                        "apple_api_error",
                        format!(
                            "Generated profile bundle mismatch for '{bundle_id}' (profile contains '{}')",
                            parsed.bundle_id
                        ),
                    ));
                }
                Some(parsed)
            }
            Err(e) => {
                warn!(
                    pipeline_id = %pipeline_id,
                    bundle_id = %bundle_id,
                    error = %e,
                    "failed to parse generated provisioning profile with security cms; falling back to Apple API metadata"
                );
                warnings.push(format!(
                    "Generated profile for '{bundle_id}' could not be parsed locally; using Apple API metadata"
                ));
                None
            }
        };

        generated_profiles.push((
            bundle_id.to_string(),
            profile_bytes,
            parsed_profile,
            created_profile,
        ));
    }

    let now = now_unix();
    if let Some(generated) = generated_certificate {
        let p12_encrypted =
            encrypt_opt(Some(generated.p12_base64), encryption_key).map_err(|e| {
                error!(error = %e, "failed to encrypt generated iOS p12");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "encryption_error",
                    "Failed to persist generated iOS certificate",
                )
            })?;
        let p12_password_encrypted = encrypt_opt(Some(generated.p12_password), encryption_key)
            .map_err(|e| {
                error!(error = %e, "failed to encrypt generated iOS p12 password");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "encryption_error",
                    "Failed to persist generated iOS certificate password",
                )
            })?;

        sqlx::query(
            "UPDATE pipeline_ios_signing_settings
             SET p12_filename = ?1,
                 p12_encrypted = ?2,
                 p12_password_encrypted = ?3,
                 p12_fingerprint = ?4,
                 p12_expires_at = ?5,
                 updated_by = ?6,
                 updated_at = ?7
             WHERE id = ?8",
        )
        .bind("ios-distribution-api.p12")
        .bind(p12_encrypted)
        .bind(p12_password_encrypted)
        .bind(generated.metadata.fingerprint)
        .bind(generated.metadata.expires_at)
        .bind(actor_id)
        .bind(now)
        .bind(&settings.id)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to store generated iOS certificate bundle");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to persist generated iOS certificate",
            )
        })?;
        warnings.push("Generated and stored a new iOS distribution certificate bundle from App Store Connect API".to_string());
    } else if p12_was_reexported {
        if let (Some(ref reexported_b64), Some(ref reexported_pass)) =
            (existing_p12.clone(), existing_password.clone())
        {
            let p12_encrypted =
                encrypt_opt(Some(reexported_b64.clone()), encryption_key).map_err(|e| {
                    error!(error = %e, "failed to encrypt re-exported iOS p12");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "encryption_error",
                        "Failed to persist re-exported iOS certificate",
                    )
                })?;
            let p12_password_encrypted = encrypt_opt(Some(reexported_pass.clone()), encryption_key)
                .map_err(|e| {
                    error!(error = %e, "failed to encrypt re-exported iOS p12 password");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "encryption_error",
                        "Failed to persist re-exported iOS certificate password",
                    )
                })?;
            let reexported_bytes = decode_b64(reexported_b64).ok();
            let reexported_fingerprint = reexported_bytes
                .as_ref()
                .map(|b| hex::encode(Sha256::digest(b)));
            sqlx::query(
                "UPDATE pipeline_ios_signing_settings
                 SET p12_encrypted = ?1,
                     p12_password_encrypted = ?2,
                     p12_fingerprint = ?3,
                     updated_at = ?4
                 WHERE id = ?5",
            )
            .bind(p12_encrypted)
            .bind(p12_password_encrypted)
            .bind(reexported_fingerprint)
            .bind(now)
            .bind(&settings.id)
            .execute(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to store re-exported iOS certificate bundle");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to persist re-exported iOS certificate",
                )
            })?;
        }
    }

    for (bundle_id, profile_bytes, parsed_profile, created_profile) in generated_profiles {
        let canonical_b64 = base64::engine::general_purpose::STANDARD.encode(&profile_bytes);
        let encrypted = encrypt_opt(Some(canonical_b64), encryption_key).map_err(|e| {
            error!(error = %e, "failed to encrypt generated iOS profile");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "encryption_error",
                "Failed to encrypt generated iOS provisioning profile",
            )
        })?;
        let checksum = Some(hex::encode(Sha256::digest(&profile_bytes)));

        let existing = sqlx::query_as::<_, IosProvisioningProfileRow>(
            "SELECT id, bundle_id, profile_filename, profile_encrypted, profile_uuid,
                    profile_name, team_id, expires_at, checksum
             FROM pipeline_ios_provisioning_profiles
             WHERE pipeline_id = ?1 AND bundle_id = ?2",
        )
        .bind(pipeline_id)
        .bind(&bundle_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to query iOS profile row during sync");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to persist generated provisioning profile",
            )
        })?;

        let profile_uuid = parsed_profile
            .as_ref()
            .and_then(|profile| profile.profile_uuid.clone())
            .clone()
            .or(created_profile.uuid.clone());
        let profile_name = parsed_profile
            .as_ref()
            .and_then(|profile| profile.profile_name.clone())
            .clone()
            .or(Some(created_profile.name.clone()));
        let expires_at = parsed_profile
            .as_ref()
            .and_then(|profile| profile.expires_at)
            .or(parse_rfc3339_opt(created_profile.expiration_date.clone()));
        let team_id = parsed_profile
            .as_ref()
            .and_then(|profile| profile.team_id.clone())
            .or_else(|| settings.team_id.clone());
        let filename = generate_profile_filename(&bundle_id, profile_uuid.as_ref());

        if let Some(existing) = existing {
            sqlx::query(
                "UPDATE pipeline_ios_provisioning_profiles
                 SET profile_filename = ?1,
                     profile_encrypted = ?2,
                     profile_uuid = ?3,
                     profile_name = ?4,
                     team_id = ?5,
                     expires_at = ?6,
                     checksum = ?7,
                     updated_by = ?8,
                     updated_at = ?9
                 WHERE id = ?10",
            )
            .bind(filename)
            .bind(encrypted)
            .bind(profile_uuid)
            .bind(profile_name)
            .bind(team_id)
            .bind(expires_at)
            .bind(checksum)
            .bind(actor_id)
            .bind(now)
            .bind(existing.id)
            .execute(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to update generated iOS profile");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to persist generated provisioning profile",
                )
            })?;
        } else {
            sqlx::query(
                "INSERT INTO pipeline_ios_provisioning_profiles (
                    id, pipeline_id, bundle_id, profile_filename, profile_encrypted,
                    profile_uuid, profile_name, team_id, expires_at, checksum,
                    created_by, updated_by, created_at, updated_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5,
                    ?6, ?7, ?8, ?9, ?10,
                    ?11, ?11, ?12, ?12
                 )",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(pipeline_id)
            .bind(bundle_id)
            .bind(filename)
            .bind(encrypted)
            .bind(profile_uuid)
            .bind(profile_name)
            .bind(team_id)
            .bind(expires_at)
            .bind(checksum)
            .bind(actor_id)
            .bind(now)
            .execute(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to insert generated iOS profile");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to persist generated provisioning profile",
                )
            })?;
        }
    }

    upsert_cached_apple_devices(pool, pipeline_id, actor_id, &remote_devices).await?;

    Ok((bundle_ids.len(), warnings))
}

pub async fn get_pipeline_ios_signing(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<PipelineIosSigningResponse> {
    check_permission(&state.enforcer, &auth.0.role, "pipelines", "read").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    if !ensure_pipeline_exists(&pool, &pipeline_id)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to verify pipeline");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load iOS signing settings",
            )
        })?
    {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Pipeline not found",
        ));
    }

    let response = build_public_response(&pool, &pipeline_id)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to build iOS signing response");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load iOS signing settings",
            )
        })?;
    Ok(Json(response))
}

pub async fn update_pipeline_ios_signing(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
    Json(req): Json<UpdatePipelineIosSigningRequest>,
) -> ApiResult<PipelineIosSigningResponse> {
    check_permission(&state.enforcer, &auth.0.role, "pipelines", "write").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    if !ensure_pipeline_exists(&pool, &pipeline_id)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to verify pipeline");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to save iOS signing settings",
            )
        })?
    {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Pipeline not found",
        ));
    }

    let now = now_unix();
    let settings_existing = load_ios_settings(&pool, &pipeline_id).await.map_err(|e| {
        error!(error = %e, "failed to load existing iOS signing settings");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save iOS signing settings",
        )
    })?;

    let bundle_ids = sanitize_bundle_ids(&req.bundle_ids).map_err(|e| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_bundle_ids",
            format!("Invalid bundle identifiers: {e}"),
        )
    })?;

    let team_id = trim_opt(req.team_id.clone());
    if req.enabled {
        let Some(team_id_value) = team_id.as_deref() else {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "team_id is required when iOS signing is enabled",
            ));
        };
        if !is_valid_team_id(team_id_value) {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "team_id must contain uppercase letters and digits only",
            ));
        }
    }

    let mut p12_filename = settings_existing
        .as_ref()
        .and_then(|r| r.p12_filename.clone());
    let mut p12_encrypted = settings_existing
        .as_ref()
        .and_then(|r| r.p12_encrypted.clone());
    let mut p12_password_encrypted = settings_existing
        .as_ref()
        .and_then(|r| r.p12_password_encrypted.clone());
    let mut p12_fingerprint = settings_existing
        .as_ref()
        .and_then(|r| r.p12_fingerprint.clone());
    let mut p12_expires_at = settings_existing.as_ref().and_then(|r| r.p12_expires_at);
    let mut api_key_id = settings_existing
        .as_ref()
        .and_then(|r| r.api_key_id.clone());
    let mut api_issuer_id = settings_existing
        .as_ref()
        .and_then(|r| r.api_issuer_id.clone());
    let mut api_private_key_encrypted = settings_existing
        .as_ref()
        .and_then(|r| r.api_private_key_encrypted.clone());
    let mut uploaded_p12_bytes: Option<Vec<u8>> = None;
    let mut uploaded_p12_password: Option<String> = None;

    if let Some(cert) = req.certificate.clone() {
        if let Some(filename) = trim_opt(cert.p12_filename) {
            p12_filename = Some(filename);
        }
        if let Some(p12_payload) = trim_opt(cert.p12_base64) {
            let p12_bytes = decode_b64(&p12_payload).map_err(|_| {
                api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_certificate",
                    "Invalid base64 for iOS .p12 certificate",
                )
            })?;
            if p12_bytes.is_empty() || p12_bytes.len() > MAX_P12_BYTES {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_certificate",
                    format!("iOS .p12 must be between 1 and {MAX_P12_BYTES} bytes"),
                ));
            }
            let canonical_b64 = base64::engine::general_purpose::STANDARD.encode(&p12_bytes);
            uploaded_p12_bytes = Some(p12_bytes.clone());
            p12_encrypted =
                encrypt_opt(Some(canonical_b64), &state.encryption_key).map_err(|e| {
                    error!(error = %e, "failed to encrypt iOS p12");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "encryption_error",
                        "Failed to encrypt iOS certificate",
                    )
                })?;
            p12_fingerprint = Some(hex::encode(Sha256::digest(&p12_bytes)));
            if p12_filename.is_none() {
                p12_filename = Some("ios-distribution.p12".to_string());
            }
        }
        if let Some(p12_password) = trim_opt(cert.p12_password) {
            uploaded_p12_password = Some(p12_password.clone());
            p12_password_encrypted = encrypt_opt(Some(p12_password), &state.encryption_key)
                .map_err(|e| {
                    error!(error = %e, "failed to encrypt iOS p12 password");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "encryption_error",
                        "Failed to encrypt iOS certificate password",
                    )
                })?;
        }
    }

    if let Some(p12_bytes) = uploaded_p12_bytes.as_ref() {
        let metadata_password = if let Some(password) = uploaded_p12_password.clone() {
            Some(password)
        } else {
            decrypt_opt(p12_password_encrypted.clone(), &state.encryption_key).map_err(|e| {
                error!(error = %e, "failed to decrypt iOS p12 password for metadata parsing");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "decryption_error",
                    "Failed to inspect iOS certificate metadata",
                )
            })?
        };

        if let Some(password) = metadata_password {
            let metadata = parse_p12_metadata(p12_bytes.clone(), password)
                .await
                .map_err(|e| {
                    api_err(
                        StatusCode::BAD_REQUEST,
                        "invalid_certificate",
                        format!(
                            "Unable to inspect iOS .p12 certificate with provided password: {e}"
                        ),
                    )
                })?;
            if let Some(fingerprint) = metadata.fingerprint {
                p12_fingerprint = Some(fingerprint);
            }
            p12_expires_at = metadata.expires_at.or(p12_expires_at);
        }
    }

    if let Some(api) = req.api_credentials.clone() {
        if let Some(key_id) = trim_opt(api.key_id) {
            api_key_id = Some(key_id);
        }
        if let Some(issuer_id) = trim_opt(api.issuer_id) {
            api_issuer_id = Some(issuer_id);
        }
        if let Some(private_key_payload) = trim_opt(api.private_key_base64) {
            let private_key_bytes = decode_b64(&private_key_payload).map_err(|_| {
                api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_api_key",
                    "Invalid base64 for App Store Connect private key",
                )
            })?;
            if private_key_bytes.is_empty() || private_key_bytes.len() > MAX_API_P8_BYTES {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_api_key",
                    format!("API private key must be between 1 and {MAX_API_P8_BYTES} bytes"),
                ));
            }
            let private_key_pem = String::from_utf8(private_key_bytes).map_err(|_| {
                api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_api_key",
                    "App Store Connect private key must be UTF-8 PEM",
                )
            })?;
            api_private_key_encrypted = encrypt_opt(Some(private_key_pem), &state.encryption_key)
                .map_err(|e| {
                error!(error = %e, "failed to encrypt App Store Connect private key");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "encryption_error",
                    "Failed to encrypt App Store Connect private key",
                )
            })?;
        }
    }

    if req.enabled {
        if bundle_ids.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "incomplete_signing_profile",
                "Enabled iOS signing requires at least one bundle identifier",
            ));
        }
        match req.mode {
            IosSigningMode::Manual => {
                if p12_encrypted.is_none()
                    || p12_filename.is_none()
                    || p12_password_encrypted.is_none()
                {
                    return Err(api_err(
                        StatusCode::BAD_REQUEST,
                        "incomplete_signing_profile",
                        "Manual iOS signing mode requires p12 file, filename, and p12 password",
                    ));
                }
            }
            IosSigningMode::Api => {
                if api_key_id.is_none()
                    || api_issuer_id.is_none()
                    || api_private_key_encrypted.is_none()
                {
                    return Err(api_err(
                        StatusCode::BAD_REQUEST,
                        "incomplete_signing_profile",
                        "API mode requires App Store Connect API credentials",
                    ));
                }
            }
            IosSigningMode::Hybrid => {
                if p12_encrypted.is_none()
                    || p12_filename.is_none()
                    || p12_password_encrypted.is_none()
                {
                    return Err(api_err(
                        StatusCode::BAD_REQUEST,
                        "incomplete_signing_profile",
                        "Hybrid iOS signing mode requires manual p12 certificate material",
                    ));
                }
                if api_key_id.is_none()
                    || api_issuer_id.is_none()
                    || api_private_key_encrypted.is_none()
                {
                    return Err(api_err(
                        StatusCode::BAD_REQUEST,
                        "incomplete_signing_profile",
                        "Hybrid mode requires App Store Connect API credentials",
                    ));
                }
            }
        }
    }

    let settings_id = settings_existing
        .as_ref()
        .map(|r| r.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    if settings_existing.is_some() {
        sqlx::query(
            "UPDATE pipeline_ios_signing_settings
             SET enabled = ?1,
                 mode = ?2,
                 team_id = ?3,
                 export_method = 'ad_hoc',
                 bundle_ids_json = ?4,
                 p12_filename = ?5,
                 p12_encrypted = ?6,
                 p12_password_encrypted = ?7,
                 p12_fingerprint = ?8,
                 p12_expires_at = ?9,
                 api_key_id = ?10,
                 api_issuer_id = ?11,
                 api_private_key_encrypted = ?12,
                 updated_by = ?13,
                 updated_at = ?14
             WHERE id = ?15",
        )
        .bind(if req.enabled { 1 } else { 0 })
        .bind(mode_str(req.mode))
        .bind(team_id.clone())
        .bind(encode_bundle_ids(&bundle_ids))
        .bind(p12_filename.clone())
        .bind(p12_encrypted.clone())
        .bind(p12_password_encrypted.clone())
        .bind(p12_fingerprint.clone())
        .bind(p12_expires_at)
        .bind(api_key_id.clone())
        .bind(api_issuer_id.clone())
        .bind(api_private_key_encrypted.clone())
        .bind(&auth.0.user_id)
        .bind(now)
        .bind(&settings_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to update pipeline iOS signing settings");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to save iOS signing settings",
            )
        })?;
    } else {
        sqlx::query(
            "INSERT INTO pipeline_ios_signing_settings (
                id, pipeline_id, enabled, mode, team_id, export_method, bundle_ids_json,
                p12_filename, p12_encrypted, p12_password_encrypted, p12_fingerprint, p12_expires_at,
                api_key_id, api_issuer_id, api_private_key_encrypted,
                created_by, updated_by, created_at, updated_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, 'ad_hoc', ?6,
                ?7, ?8, ?9, ?10, ?11,
                ?12, ?13, ?14,
                ?15, ?15, ?16, ?16
             )",
        )
        .bind(&settings_id)
        .bind(&pipeline_id)
        .bind(if req.enabled { 1 } else { 0 })
        .bind(mode_str(req.mode))
        .bind(team_id.clone())
        .bind(encode_bundle_ids(&bundle_ids))
        .bind(p12_filename.clone())
        .bind(p12_encrypted.clone())
        .bind(p12_password_encrypted.clone())
        .bind(p12_fingerprint.clone())
        .bind(p12_expires_at)
        .bind(api_key_id.clone())
        .bind(api_issuer_id.clone())
        .bind(api_private_key_encrypted.clone())
        .bind(&auth.0.user_id)
        .bind(now)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to insert pipeline iOS signing settings");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to save iOS signing settings",
            )
        })?;
    }

    if req.provisioning_profiles.len() > MAX_PROFILE_COUNT {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_profiles",
            format!("Too many provisioning profiles (max {MAX_PROFILE_COUNT})"),
        ));
    }

    if !req.provisioning_profiles.is_empty() {
        for input in req.provisioning_profiles {
            upsert_profile(
                &pool,
                &state.encryption_key,
                &pipeline_id,
                &auth.0.user_id,
                input,
            )
            .await?;
        }
    }

    if req.enabled && matches!(req.mode, IosSigningMode::Manual) {
        let profile_rows = load_ios_profiles(&pool, &pipeline_id).await.map_err(|e| {
            error!(error = %e, "failed to load iOS provisioning profiles for validation");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to validate iOS signing settings",
            )
        })?;
        let available_bundles: HashSet<String> = profile_rows
            .iter()
            .filter(|row| row.profile_encrypted.is_some())
            .map(|row| row.bundle_id.clone())
            .collect();
        for bundle_id in &bundle_ids {
            if !available_bundles.contains(bundle_id) {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "incomplete_signing_profile",
                    format!(
                        "Manual iOS signing mode requires provisioning profile for bundle '{bundle_id}'"
                    ),
                ));
            }
        }
    }

    let details = serde_json::json!({
        "pipeline_id": &pipeline_id,
        "updated_by": &auth.0.email,
        "enabled": req.enabled,
        "mode": mode_str(req.mode),
        "bundle_ids": bundle_ids,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "pipeline_ios_signing_updated",
        "pipeline",
        Some(&pipeline_id),
        Some(&details),
    )
    .await;

    info!(
        pipeline_id = %pipeline_id,
        actor = %auth.0.email,
        "updated pipeline iOS signing settings"
    );

    let response = build_public_response(&pool, &pipeline_id)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to build iOS signing response");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load iOS signing settings",
            )
        })?;
    Ok(Json(response))
}

async fn upsert_profile(
    pool: &SqlitePool,
    encryption_key: &[u8],
    pipeline_id: &str,
    actor_id: &str,
    input: IosProvisioningProfileInput,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let bundle_id = input.bundle_id.trim().to_string();
    if bundle_id.is_empty() || !is_valid_bundle_id(&bundle_id) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_profiles",
            format!("Invalid bundle identifier '{bundle_id}'"),
        ));
    }
    let Some(profile_payload) = trim_opt(input.profile_base64) else {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_profiles",
            format!("Provisioning profile payload is required for bundle '{bundle_id}'"),
        ));
    };

    let profile_bytes = decode_b64(&profile_payload).map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_profiles",
            format!("Invalid base64 provisioning profile for bundle '{bundle_id}'"),
        )
    })?;
    if profile_bytes.is_empty() || profile_bytes.len() > MAX_PROFILE_BYTES {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_profiles",
            format!(
                "Provisioning profile for bundle '{bundle_id}' must be between 1 and {MAX_PROFILE_BYTES} bytes"
            ),
        ));
    }

    let parsed = parse_provisioning_profile(profile_bytes.clone())
        .await
        .map_err(|e| {
            api_err(
                StatusCode::BAD_REQUEST,
                "invalid_profiles",
                format!("Invalid provisioning profile for bundle '{bundle_id}': {e}"),
            )
        })?;
    if parsed.bundle_id != bundle_id && parsed.bundle_id != "*" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_profiles",
            format!(
                "Provisioning profile bundle mismatch for '{bundle_id}'; profile contains '{}'",
                parsed.bundle_id
            ),
        ));
    }

    let canonical_b64 = base64::engine::general_purpose::STANDARD.encode(&profile_bytes);
    let encrypted = encrypt_opt(Some(canonical_b64), encryption_key).map_err(|e| {
        error!(error = %e, "failed to encrypt iOS provisioning profile");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "encryption_error",
            "Failed to encrypt provisioning profile",
        )
    })?;
    let checksum = Some(hex::encode(Sha256::digest(&profile_bytes)));
    let filename = trim_opt(input.profile_filename)
        .or_else(|| {
            parsed
                .profile_uuid
                .clone()
                .map(|uuid| format!("{uuid}.mobileprovision"))
        })
        .or_else(|| Some(format!("{bundle_id}.mobileprovision")));

    let existing = sqlx::query_as::<_, IosProvisioningProfileRow>(
        "SELECT id, bundle_id, profile_filename, profile_encrypted, profile_uuid,
                profile_name, team_id, expires_at, checksum
         FROM pipeline_ios_provisioning_profiles
         WHERE pipeline_id = ?1 AND bundle_id = ?2",
    )
    .bind(pipeline_id)
    .bind(&bundle_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to query existing iOS profile row");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save provisioning profile",
        )
    })?;

    let now = now_unix();
    if let Some(existing) = existing {
        sqlx::query(
            "UPDATE pipeline_ios_provisioning_profiles
             SET profile_filename = ?1,
                 profile_encrypted = ?2,
                 profile_uuid = ?3,
                 profile_name = ?4,
                 team_id = ?5,
                 expires_at = ?6,
                 checksum = ?7,
                 updated_by = ?8,
                 updated_at = ?9
             WHERE id = ?10",
        )
        .bind(filename)
        .bind(encrypted)
        .bind(parsed.profile_uuid)
        .bind(parsed.profile_name)
        .bind(parsed.team_id)
        .bind(parsed.expires_at)
        .bind(checksum)
        .bind(actor_id)
        .bind(now)
        .bind(existing.id)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to update iOS provisioning profile row");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to save provisioning profile",
            )
        })?;
    } else {
        sqlx::query(
            "INSERT INTO pipeline_ios_provisioning_profiles (
                id, pipeline_id, bundle_id, profile_filename, profile_encrypted,
                profile_uuid, profile_name, team_id, expires_at, checksum,
                created_by, updated_by, created_at, updated_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9, ?10,
                ?11, ?11, ?12, ?12
             )",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(pipeline_id)
        .bind(bundle_id)
        .bind(filename)
        .bind(encrypted)
        .bind(parsed.profile_uuid)
        .bind(parsed.profile_name)
        .bind(parsed.team_id)
        .bind(parsed.expires_at)
        .bind(checksum)
        .bind(actor_id)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to insert iOS provisioning profile row");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to save provisioning profile",
            )
        })?;
    }

    Ok(())
}

pub async fn list_pipeline_ios_devices(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<ListPipelineIosDevicesResponse> {
    check_permission(&state.enforcer, &auth.0.role, "pipelines", "read").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    if !ensure_pipeline_exists(&pool, &pipeline_id)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to verify pipeline");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load iOS devices",
            )
        })?
    {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Pipeline not found",
        ));
    }

    let devices = load_ios_devices(&pool, &pipeline_id).await.map_err(|e| {
        error!(error = %e, "failed to query iOS devices");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load iOS devices",
        )
    })?;
    Ok(Json(ListPipelineIosDevicesResponse {
        pipeline_id,
        devices: devices.into_iter().map(device_row_to_contract).collect(),
    }))
}

pub async fn register_pipeline_ios_device(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
    Json(req): Json<RegisterIosDeviceRequest>,
) -> ApiResult<RegisterIosDeviceResponse> {
    check_permission(&state.enforcer, &auth.0.role, "pipelines", "write").await?;

    let udid = req.udid.trim().to_uppercase();
    let name = req.name.trim().to_string();
    let platform = req.platform.unwrap_or_else(|| "IOS".to_string());

    if !validate_udid(&udid) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "udid must be alphanumeric and 1-64 characters",
        ));
    }
    if name.is_empty() || name.len() > MAX_DEVICE_NAME_LEN {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            format!("name must be between 1 and {MAX_DEVICE_NAME_LEN} characters"),
        ));
    }

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    if !ensure_pipeline_exists(&pool, &pipeline_id)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to verify pipeline");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to register iOS device",
            )
        })?
    {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Pipeline not found",
        ));
    }

    let settings = load_ios_settings(&pool, &pipeline_id).await.map_err(|e| {
        error!(error = %e, "failed to load iOS signing settings");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to register iOS device",
        )
    })?;
    let Some(settings) = settings else {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_state",
            "iOS signing is not configured for this pipeline",
        ));
    };
    let creds = load_api_credentials(&settings, &state.encryption_key).map_err(|e| {
        error!(error = %e, "failed to decode App Store Connect API credentials");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "decryption_error",
            "Failed to decrypt App Store Connect credentials",
        )
    })?;
    let Some(creds) = creds else {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_state",
            "App Store Connect API credentials are required to register devices",
        ));
    };

    let client = reqwest::Client::new();
    let remote = apple_api::register_device(&client, &creds, &udid, &name, &platform).await?;

    let now = now_unix();
    let existing = sqlx::query_as::<_, IosDeviceRow>(
        "SELECT id, device_id, udid, name, platform, status, added_at, last_synced_at
         FROM pipeline_ios_signing_devices WHERE pipeline_id = ?1 AND udid = ?2",
    )
    .bind(&pipeline_id)
    .bind(&udid)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to query existing iOS device row");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to register iOS device",
        )
    })?;

    let row_id = existing
        .as_ref()
        .map(|r| r.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if existing.is_some() {
        sqlx::query(
            "UPDATE pipeline_ios_signing_devices
             SET device_id = ?1, name = ?2, platform = ?3, status = ?4,
                 last_synced_at = ?5, updated_by = ?6, updated_at = ?5
             WHERE id = ?7",
        )
        .bind(&remote.device_id)
        .bind(&remote.name)
        .bind(&remote.platform)
        .bind(&remote.status)
        .bind(now)
        .bind(&auth.0.user_id)
        .bind(&row_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to update iOS device row");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to register iOS device",
            )
        })?;
    } else {
        sqlx::query(
            "INSERT INTO pipeline_ios_signing_devices (
                id, pipeline_id, device_id, udid, name, platform, status,
                added_at, last_synced_at, created_by, updated_by, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9, ?8, ?8)",
        )
        .bind(&row_id)
        .bind(&pipeline_id)
        .bind(&remote.device_id)
        .bind(&udid)
        .bind(&remote.name)
        .bind(&remote.platform)
        .bind(&remote.status)
        .bind(now)
        .bind(&auth.0.user_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to insert iOS device row");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to register iOS device",
            )
        })?;
    }

    let details = serde_json::json!({
        "pipeline_id": &pipeline_id,
        "udid": &udid,
        "device_name": &remote.name,
        "platform": &remote.platform,
        "actor": &auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "pipeline_ios_device_registered",
        "pipeline",
        Some(&pipeline_id),
        Some(&details),
    )
    .await;

    let mut profile_sync_triggered = false;
    if matches!(
        parse_mode(&settings.mode),
        IosSigningMode::Api | IosSigningMode::Hybrid
    ) {
        match sync_ios_signing_assets(
            &pool,
            &state.encryption_key,
            &pipeline_id,
            &settings,
            &auth.0.user_id,
            &creds,
        )
        .await
        {
            Ok((updated_profiles, sync_warnings)) => {
                profile_sync_triggered = true;
                let details = serde_json::json!({
                    "pipeline_id": &pipeline_id,
                    "actor": &auth.0.email,
                    "trigger": "device_register",
                    "updated_profiles": updated_profiles,
                    "warnings_count": sync_warnings.len(),
                })
                .to_string();
                let _ = write_audit_log(
                    &pool,
                    Some(&auth.0.user_id),
                    "pipeline_ios_signing_sync",
                    "pipeline",
                    Some(&pipeline_id),
                    Some(&details),
                )
                .await;
            }
            Err((status, err)) => {
                warn!(
                    pipeline_id = %pipeline_id,
                    actor = %auth.0.email,
                    code = %err.code,
                    error = %err.error,
                    http_status = %status,
                    "iOS profile sync after device registration failed"
                );
            }
        }
    }

    let device = RegisteredIosDevice {
        id: row_id,
        device_id: Some(remote.device_id),
        udid,
        name: remote.name,
        platform: remote.platform,
        status: remote.status,
        added_at: existing.map(|r| r.added_at).unwrap_or(now),
        last_synced_at: Some(now),
    };
    Ok(Json(RegisterIosDeviceResponse {
        pipeline_id,
        device,
        profile_sync_triggered,
    }))
}

pub async fn sync_pipeline_ios_signing(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<SyncPipelineIosSigningResponse> {
    check_permission(&state.enforcer, &auth.0.role, "pipelines", "write").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    if !ensure_pipeline_exists(&pool, &pipeline_id)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to verify pipeline");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to sync iOS signing settings",
            )
        })?
    {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Pipeline not found",
        ));
    }

    let settings = load_ios_settings(&pool, &pipeline_id).await.map_err(|e| {
        error!(error = %e, "failed to load iOS settings for sync");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to sync iOS signing settings",
        )
    })?;
    let Some(settings) = settings else {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_state",
            "iOS signing is not configured for this pipeline",
        ));
    };
    let creds = load_api_credentials(&settings, &state.encryption_key).map_err(|e| {
        error!(error = %e, "failed to decode App Store Connect credentials");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "decryption_error",
            "Failed to decrypt App Store Connect credentials",
        )
    })?;
    let Some(creds) = creds else {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_state",
            "App Store Connect API credentials are required to sync iOS signing",
        ));
    };
    let bundle_ids = parse_bundle_ids(&settings.bundle_ids_json);
    let (updated_profiles, warnings) = sync_ios_signing_assets(
        &pool,
        &state.encryption_key,
        &pipeline_id,
        &settings,
        &auth.0.user_id,
        &creds,
    )
    .await?;

    let details = serde_json::json!({
        "pipeline_id": &pipeline_id,
        "actor": &auth.0.email,
        "bundle_ids": &bundle_ids,
        "updated_profiles": updated_profiles,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "pipeline_ios_signing_sync",
        "pipeline",
        Some(&pipeline_id),
        Some(&details),
    )
    .await;

    Ok(Json(SyncPipelineIosSigningResponse {
        pipeline_id,
        ok: true,
        updated_profiles,
        synced_bundle_ids: bundle_ids,
        warnings,
    }))
}

pub async fn get_job_ios_signing(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
) -> ApiResult<RunnerIosSigningResponse> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let build_row = sqlx::query("SELECT pipeline_id, runner_id FROM builds WHERE id = ?1")
        .bind(&job_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load build for iOS signing lookup");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load build iOS signing settings",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;

    let assigned_runner: Option<String> = sqlx::Row::get(&build_row, "runner_id");
    if assigned_runner.as_deref() != Some(&runner_id) {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "This build is not assigned to your runner",
        ));
    }
    let pipeline_id: String = sqlx::Row::get(&build_row, "pipeline_id");

    let settings = load_ios_settings(&pool, &pipeline_id).await.map_err(|e| {
        error!(error = %e, "failed to load iOS signing settings for runner");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load iOS signing settings",
        )
    })?;
    let Some(settings) = settings else {
        return Ok(Json(RunnerIosSigningResponse { bundle: None }));
    };
    if settings.enabled == 0 {
        return Ok(Json(RunnerIosSigningResponse { bundle: None }));
    }

    let team_id = settings.team_id.clone().ok_or_else(|| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "incomplete_signing_profile",
            "iOS signing is enabled but team_id is missing",
        )
    })?;
    let p12_filename = settings.p12_filename.clone().ok_or_else(|| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "incomplete_signing_profile",
            "iOS signing is enabled but p12 filename is missing (run iOS signing sync)",
        )
    })?;
    let p12_base64 = decrypt_opt(settings.p12_encrypted.clone(), &state.encryption_key)
        .map_err(|e| {
            error!(error = %e, "failed to decrypt iOS p12");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "decryption_error",
                "Failed to load iOS certificate",
            )
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "incomplete_signing_profile",
                "iOS signing is enabled but p12 payload is missing (run iOS signing sync)",
            )
        })?;
    let p12_password = decrypt_opt(
        settings.p12_password_encrypted.clone(),
        &state.encryption_key,
    )
    .map_err(|e| {
        error!(error = %e, "failed to decrypt iOS p12 password");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "decryption_error",
            "Failed to load iOS certificate password",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "incomplete_signing_profile",
            "iOS signing is enabled but p12 password is missing (run iOS signing sync)",
        )
    })?;

    let p12_bytes = decode_b64(&p12_base64).map_err(|e| {
        error!(error = %e, "stored iOS p12 payload is invalid base64");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "incomplete_signing_profile",
            "Stored iOS signing certificate payload is invalid; run iOS signing sync",
        )
    })?;
    parse_p12_metadata(p12_bytes, p12_password.clone())
        .await
        .map_err(|e| {
            error!(
                error = %e,
                "stored iOS p12/password pair is invalid during runner bundle assembly"
            );
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "incomplete_signing_profile",
                "Stored iOS signing certificate password does not match certificate payload; run iOS signing sync",
            )
        })?;

    let profile_rows = load_ios_profiles(&pool, &pipeline_id).await.map_err(|e| {
        error!(error = %e, "failed to load iOS profiles for runner");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load iOS provisioning profiles",
        )
    })?;
    if profile_rows.is_empty() {
        return Err(api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "incomplete_signing_profile",
            "iOS signing is enabled but no provisioning profiles are configured",
        ));
    }

    let mut provisioning_profiles = Vec::with_capacity(profile_rows.len());
    for row in profile_rows {
        let profile_filename = row.profile_filename.unwrap_or_else(|| {
            row.profile_uuid
                .clone()
                .map(|uuid| format!("{uuid}.mobileprovision"))
                .unwrap_or_else(|| format!("{}.mobileprovision", row.bundle_id))
        });
        let profile_base64 = decrypt_opt(row.profile_encrypted.clone(), &state.encryption_key)
            .map_err(|e| {
                error!(error = %e, "failed to decrypt iOS provisioning profile");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "decryption_error",
                    "Failed to load iOS provisioning profile",
                )
            })?
            .ok_or_else(|| {
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "incomplete_signing_profile",
                    format!(
                        "iOS provisioning profile payload is missing for bundle '{}'",
                        row.bundle_id
                    ),
                )
            })?;
        provisioning_profiles.push(RunnerIosProvisioningProfile {
            bundle_id: row.bundle_id,
            profile_filename,
            profile_base64,
            profile_uuid: row.profile_uuid,
            profile_name: row.profile_name,
        });
    }

    let bundle = RunnerIosSigningBundle {
        enabled: true,
        mode: parse_mode(&settings.mode),
        team_id,
        export_method: settings.export_method,
        p12_filename,
        p12_base64,
        p12_password,
        provisioning_profiles,
    };

    Ok(Json(RunnerIosSigningResponse {
        bundle: Some(bundle),
    }))
}
