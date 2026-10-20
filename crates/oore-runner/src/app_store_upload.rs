//! Uploads a signed App Store IPA to App Store Connect with `xcrun altool`
//! and the pipeline's App Store Connect API key. The key is written to a
//! private directory only for the duration of the upload.

use std::fs;
use std::path::{Path, PathBuf};

use oore_contract::RunnerAppStoreUploadCredentials;
use serde_json::Value;

/// `altool` looks for `AuthKey_<key id>.p8` in this directory.
const API_PRIVATE_KEYS_DIR_ENV: &str = "API_PRIVATE_KEYS_DIR";

fn is_valid_key_id(key_id: &str) -> bool {
    !key_id.is_empty()
        && key_id.len() <= 32
        && key_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

fn is_valid_issuer_id(issuer_id: &str) -> bool {
    !issuer_id.is_empty()
        && issuer_id.len() <= 64
        && issuer_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

/// Removes the written API key when the upload ends, whatever the outcome.
struct PrivateKeyFile(PathBuf);

impl Drop for PrivateKeyFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn write_private_key(
    key_dir: &Path,
    credentials: &RunnerAppStoreUploadCredentials,
) -> anyhow::Result<PrivateKeyFile> {
    fs::create_dir_all(key_dir).map_err(|error| {
        anyhow::anyhow!(
            "failed to create App Store Connect key directory {}: {error}",
            key_dir.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(key_dir, fs::Permissions::from_mode(0o700))?;
    }
    let path = key_dir.join(format!("AuthKey_{}.p8", credentials.key_id));
    crate::write_private_file(&path, credentials.private_key_pem.as_bytes())
        .map_err(|error| anyhow::anyhow!("failed to write App Store Connect API key: {error}"))?;
    Ok(PrivateKeyFile(path))
}

fn altool_upload_args(ipa: &Path, credentials: &RunnerAppStoreUploadCredentials) -> Vec<String> {
    vec![
        "altool".to_string(),
        "--upload-app".to_string(),
        "--type".to_string(),
        "ios".to_string(),
        "--file".to_string(),
        ipa.display().to_string(),
        "--apiKey".to_string(),
        credentials.key_id.clone(),
        "--apiIssuer".to_string(),
        credentials.issuer_id.clone(),
        "--output-format".to_string(),
        "json".to_string(),
    ]
}

/// Pulls Apple's error messages out of `altool --output-format json` output,
/// falling back to the raw text when the output is not JSON.
fn summarize_altool_failure(stdout: &str, stderr: &str) -> String {
    let parsed = serde_json::from_str::<Value>(stdout.trim()).ok();
    let messages: Vec<String> = parsed
        .as_ref()
        .and_then(|value| value.get("product-errors"))
        .and_then(Value::as_array)
        .map(|errors| {
            errors
                .iter()
                .filter_map(|error| {
                    let message = error.get("message").and_then(Value::as_str)?;
                    let reason = error
                        .get("userInfo")
                        .and_then(|info| info.get("NSLocalizedFailureReason"))
                        .and_then(Value::as_str)
                        .filter(|reason| !message.contains(reason));
                    Some(match reason {
                        Some(reason) => format!("{message} {reason}"),
                        None => message.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if !messages.is_empty() {
        return messages.join("; ");
    }
    let fallback = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if fallback.is_empty() {
        "altool exited without output".to_string()
    } else {
        fallback.chars().take(2000).collect()
    }
}

/// Uploads `ipa` and returns Apple's success message.
pub(crate) async fn upload_ipa(
    ipa: &Path,
    credentials: &RunnerAppStoreUploadCredentials,
    key_dir: &Path,
) -> anyhow::Result<String> {
    anyhow::ensure!(
        is_valid_key_id(&credentials.key_id),
        "App Store Connect API key ID is invalid"
    );
    anyhow::ensure!(
        is_valid_issuer_id(&credentials.issuer_id),
        "App Store Connect issuer ID is invalid"
    );
    anyhow::ensure!(
        !credentials.private_key_pem.trim().is_empty(),
        "App Store Connect API private key is empty"
    );
    let _key = write_private_key(key_dir, credentials)?;
    let output = tokio::process::Command::new("xcrun")
        .args(altool_upload_args(ipa, credentials))
        .env(API_PRIVATE_KEYS_DIR_ENV, key_dir)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|error| anyhow::anyhow!("failed to run xcrun altool: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        anyhow::bail!(
            "App Store Connect upload failed: {}",
            summarize_altool_failure(&stdout, &stderr)
        );
    }
    let message = serde_json::from_str::<Value>(stdout.trim())
        .ok()
        .and_then(|value| {
            value
                .get("success-message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "Uploaded to App Store Connect".to_string());
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> RunnerAppStoreUploadCredentials {
        RunnerAppStoreUploadCredentials {
            key_id: "ABC123DEF4".to_string(),
            issuer_id: "69a6de7e-1111-47e3-e053-5b8c7c11a4d1".to_string(),
            private_key_pem: "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n"
                .to_string(),
        }
    }

    #[test]
    fn upload_args_never_contain_the_private_key() {
        let args = altool_upload_args(Path::new("/tmp/Runner.ipa"), &credentials());
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--apiKey", "ABC123DEF4"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--file", "/tmp/Runner.ipa"])
        );
        assert!(args.iter().all(|arg| !arg.contains("PRIVATE KEY")));
    }

    #[test]
    fn rejects_key_ids_that_could_escape_the_key_directory() {
        assert!(is_valid_key_id("ABC123DEF4"));
        assert!(!is_valid_key_id("../ABC"));
        assert!(!is_valid_key_id(""));
        assert!(is_valid_issuer_id("69a6de7e-1111-47e3-e053-5b8c7c11a4d1"));
        assert!(!is_valid_issuer_id("issuer id"));
    }

    #[test]
    fn private_key_is_removed_after_upload_scope() {
        let dir = std::env::temp_dir().join(format!("oore-asc-key-test-{}", std::process::id()));
        let key = write_private_key(&dir, &credentials()).expect("write key");
        let path = key.0.clone();
        assert!(path.ends_with("AuthKey_ABC123DEF4.p8"));
        assert!(path.exists());
        drop(key);
        assert!(!path.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn summarizes_altool_product_errors() {
        let stdout = r#"{"tool-version":"8.0","product-errors":[{"message":"Validation failed","userInfo":{"NSLocalizedFailureReason":"The bundle version must be higher than the previously uploaded version."}}]}"#;
        assert_eq!(
            summarize_altool_failure(stdout, ""),
            "Validation failed The bundle version must be higher than the previously uploaded version."
        );
        assert_eq!(summarize_altool_failure("", "boom"), "boom");
        assert_eq!(
            summarize_altool_failure("", ""),
            "altool exited without output"
        );
    }
}
