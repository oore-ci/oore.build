//! Uploads a signed App Store IPA to App Store Connect with `xcrun altool`
//! and the pipeline's App Store Connect API key. The key is written to a
//! private directory only for the duration of the upload.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use oore_contract::{
    AppStoreDestination, AppStorePublishConfig, PublishStore, RunnerAppStoreUploadCredentials,
    RunnerPublishResult,
};
use serde_json::Value;

/// `altool` looks for `AuthKey_<key id>.p8` in this directory.
const API_PRIVATE_KEYS_DIR_ENV: &str = "API_PRIVATE_KEYS_DIR";
/// Longest an altool upload may run before the runner stops it.
const ALTOOL_TIMEOUT: Duration = Duration::from_secs(45 * 60);
const ALTOOL_RETRY_DELAY: Duration = Duration::from_secs(10);

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

/// The last JSON document in altool's output. altool can print log lines
/// before its `--output-format json` result, so parse from the end.
fn altool_result(stdout: &str) -> Option<Value> {
    let lines: Vec<&str> = stdout.lines().collect();
    (0..lines.len())
        .rev()
        .find_map(|start| serde_json::from_str::<Value>(&lines[start..].join("\n")).ok())
        .filter(Value::is_object)
}

/// Apple's product errors from altool's result. altool sometimes reports them
/// with exit status 0, so an upload is only a success without any.
fn altool_product_errors(result: Option<&Value>) -> Vec<String> {
    product_errors(result)
        .into_iter()
        .map(|(_, text)| text)
        .collect()
}

/// Each product error's bare message and its text with Apple's reasons.
/// App Store Connect's validation errors (409) often carry only "Validation
/// failed" at the top level and put the reason in `detail` or inside
/// `NSUnderlyingError`.
fn product_errors(result: Option<&Value>) -> Vec<(String, String)> {
    let Some(errors) = result
        .and_then(|value| value.get("product-errors"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    errors
        .iter()
        .filter_map(|error| {
            let message = error.get("message").and_then(Value::as_str)?.trim();
            let mut details = Vec::new();
            if let Some(info) = error.get("userInfo") {
                collect_error_details(info, &mut details);
            }
            let mut text = message.to_string();
            for detail in details {
                if text.contains(&detail) {
                    continue;
                }
                if text == message && detail.contains(message) {
                    text = detail;
                } else {
                    text = format!("{text} {detail}");
                }
            }
            Some((message.to_string(), text))
        })
        .collect()
}

/// The keys under which altool and App Store Connect put human-readable
/// reasons, in the order they read best.
const ERROR_DETAIL_KEYS: [&str; 4] = [
    "NSLocalizedFailureReason",
    "detail",
    "NSLocalizedRecoverySuggestion",
    "NSLocalizedDescription",
];

fn collect_error_details(info: &Value, details: &mut Vec<String>) {
    for key in ERROR_DETAIL_KEYS {
        if let Some(text) = info.get(key).and_then(Value::as_str) {
            push_detail(details, text);
        }
    }
    match info.get("NSUnderlyingError") {
        Some(Value::String(text)) => {
            for key in ERROR_DETAIL_KEYS {
                if let Some(text) = described_error_value(text, key) {
                    push_detail(details, &text);
                }
            }
        }
        Some(underlying @ Value::Object(_)) => {
            collect_error_details(underlying.get("userInfo").unwrap_or(underlying), details);
        }
        _ => {}
    }
}

fn push_detail(details: &mut Vec<String>, text: &str) {
    let text = text.trim();
    if !text.is_empty() && !details.iter().any(|known| known.contains(text)) {
        details.retain(|known| !text.contains(known.as_str()));
        details.push(text.to_string());
    }
}

/// A value from an NSError description such as
/// `Error Domain=IrisAPI Code=-19241 "Validation failed" UserInfo={status=409, detail=Invalid Bundle. ..., NSLocalizedDescription=Validation failed}`.
/// Values are unquoted, so each runs to the next `, name=` or the closing brace.
fn described_error_value(description: &str, key: &str) -> Option<String> {
    let start = description.find("UserInfo={")? + "UserInfo={".len();
    let user_info = &description[start..];
    let user_info = &user_info[..user_info.rfind('}').unwrap_or(user_info.len())];
    let starts_entry = |text: &str| {
        text.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
        })
    };
    let mut entries = Vec::new();
    let mut entry_start = 0;
    for (index, _) in user_info.match_indices(", ") {
        if index >= entry_start && starts_entry(&user_info[index + 2..]) {
            entries.push(&user_info[entry_start..index]);
            entry_start = index + 2;
        }
    }
    entries.push(&user_info[entry_start..]);
    entries.into_iter().find_map(|entry| {
        let (name, value) = entry.split_once('=')?;
        let value = value.trim().trim_matches('"').trim();
        (name.trim() == key && !value.is_empty()).then(|| value.to_string())
    })
}

/// Some Xcode versions make altool print every environment variable as
/// `ENV: {"NAME"=>"value", ...}` on failure; keep only the names.
fn hide_environment_values(output: &str) -> String {
    let Some(start) = output.find("ENV: {") else {
        return output.to_string();
    };
    let body_start = start + "ENV: {".len();
    let Some(length) = output[body_start..].find('}') else {
        return output[..start].to_string();
    };
    let names: Vec<&str> = output[body_start..body_start + length]
        .split("=>")
        .filter_map(|part| part.rsplit(['{', ',']).next())
        .map(|name| name.trim().trim_matches('"'))
        .filter(|name| !name.is_empty() && !name.contains(' '))
        .collect();
    let hidden: Vec<String> = names
        .iter()
        .map(|name| format!("\"{name}\"=>\"...\""))
        .collect();
    format!(
        "{}ENV: {{{}}}{}",
        &output[..start],
        hidden.join(", "),
        &output[body_start + length + 1..]
    )
}

/// Failures Codemagic's cli-tools retries: Apple's authentication service
/// hiccups (-19209), invalid server responses and timeouts.
fn is_transient_altool_failure(output: &str) -> bool {
    is_pre_upload_failure(output)
        || output.contains("server returned an invalid response")
        || output.contains("The request timed out")
}

/// altool failed to authenticate, before sending the IPA.
fn is_pre_upload_failure(output: &str) -> bool {
    output.contains("Unable to authenticate") && output.contains("-19209")
}

/// Apple already has a build with this build number.
fn is_duplicate_upload(output: &str) -> bool {
    output.contains("Redundant Binary Upload")
        || output.contains("already uploaded a build with build number")
}

/// Kills a process group when dropped, so altool cannot outlive its upload
/// when the step is canceled or times out.
struct ProcessGroupGuard(Option<libc::pid_t>);

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if let Some(pgid) = self.0 {
            // SAFETY: killpg only sends a signal to the group altool leads.
            unsafe { libc::killpg(pgid, libc::SIGKILL) };
        }
    }
}

/// Runs altool in its own process group and waits at most `limit`.
async fn run_altool(
    program: &Path,
    args: Vec<String>,
    key_dir: &Path,
    limit: Duration,
) -> anyhow::Result<std::process::Output> {
    let child = tokio::process::Command::new(program)
        .args(args)
        .env(API_PRIVATE_KEYS_DIR_ENV, key_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| anyhow::anyhow!("failed to run xcrun altool: {error}"))?;
    let mut group = ProcessGroupGuard(child.id().and_then(|pid| libc::pid_t::try_from(pid).ok()));
    match tokio::time::timeout(limit, child.wait_with_output()).await {
        Ok(output) => {
            group.0 = None;
            output.map_err(|error| anyhow::anyhow!("failed to run xcrun altool: {error}"))
        }
        Err(_) => anyhow::bail!(
            "App Store Connect upload timed out after {} minutes. It may still have reached Apple, so check App Store Connect before retrying.",
            limit.as_secs() / 60
        ),
    }
}

/// Pulls Apple's error messages out of `altool --output-format json` output,
/// falling back to the raw text when the output is not JSON.
fn summarize_altool_failure(stdout: &str, stderr: &str) -> String {
    let result = altool_result(stdout);
    let errors = product_errors(result.as_ref());
    if !errors.is_empty() {
        let summary = errors
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        if errors.iter().any(|(message, text)| message != text) {
            return summary;
        }
        // A bare "Validation failed" says nothing; show everything altool
        // returned so Apple's reason is in the log wherever it put it.
        let mut raw = result.map(|value| value.to_string()).unwrap_or_default();
        if !stderr.trim().is_empty() {
            raw = format!("{raw} {}", stderr.trim());
        }
        let raw: String = hide_environment_values(&raw).chars().take(4000).collect();
        return format!("{summary}. altool returned: {raw}");
    }
    let fallback = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if fallback.is_empty() {
        "altool exited without output".to_string()
    } else {
        hide_environment_values(fallback)
            .chars()
            .take(2000)
            .collect()
    }
}

/// Uploads `ipa` and returns Apple's success message.
pub(crate) async fn upload_ipa(
    ipa: &Path,
    credentials: &RunnerAppStoreUploadCredentials,
    key_dir: &Path,
) -> anyhow::Result<String> {
    upload_ipa_with(
        Path::new("xcrun"),
        ipa,
        credentials,
        key_dir,
        ALTOOL_TIMEOUT,
        ALTOOL_RETRY_DELAY,
    )
    .await
}

async fn upload_ipa_with(
    program: &Path,
    ipa: &Path,
    credentials: &RunnerAppStoreUploadCredentials,
    key_dir: &Path,
    limit: Duration,
    retry_delay: Duration,
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
    // Whether an earlier attempt may have delivered the IPA before failing.
    let mut maybe_uploaded = false;
    let mut attempt = 1;
    loop {
        let output = run_altool(
            program,
            altool_upload_args(ipa, credentials),
            key_dir,
            limit,
        )
        .await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let result = altool_result(&stdout);
        if output.status.success() && altool_product_errors(result.as_ref()).is_empty() {
            return Ok(result
                .as_ref()
                .and_then(|value| value.get("success-message"))
                .and_then(Value::as_str)
                .unwrap_or("Uploaded to App Store Connect")
                .to_string());
        }
        let combined = format!("{stdout}\n{stderr}");
        if maybe_uploaded && is_duplicate_upload(&combined) {
            return Ok(
                "Uploaded to App Store Connect. The first attempt's reply was lost, and Apple already has this build"
                    .to_string(),
            );
        }
        if attempt == 1 && is_transient_altool_failure(&combined) {
            maybe_uploaded = !is_pre_upload_failure(&combined);
            attempt += 1;
            tokio::time::sleep(retry_delay).await;
            continue;
        }
        anyhow::bail!(
            "App Store Connect upload failed: {}",
            summarize_altool_failure(&stdout, &stderr)
        );
    }
}

/// The publish result for an uploaded IPA, from the app identity the runner
/// recorded when it signed the archive.
pub(crate) fn publish_result(
    ios_metadata: &Value,
    config: &AppStorePublishConfig,
) -> anyhow::Result<RunnerPublishResult> {
    let field = |key: &str| {
        ios_metadata
            .pointer(&format!("/ios_app/{key}"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("signed IPA metadata has no {key}"))
    };
    let build_number = field("build_number")?;
    // CFBundleVersion may be dotted (1.0.3); its leading number orders builds.
    let version_code = build_number
        .parse::<i64>()
        .ok()
        .or_else(|| build_number.split('.').next()?.parse().ok())
        .unwrap_or(0);
    Ok(RunnerPublishResult {
        store: PublishStore::AppStore,
        track: match config.destination {
            AppStoreDestination::Testflight => config.testflight_groups.join(", "),
            AppStoreDestination::AppStore => "app_store".to_string(),
        },
        package_name: field("bundle_identifier")?,
        version_code,
        version_name: Some(field("version")?),
        rollout_fraction: None,
        status: "uploaded".to_string(),
        testflight_groups: match config.destination {
            AppStoreDestination::Testflight => Some(config.testflight_groups.clone()),
            AppStoreDestination::AppStore => None,
        },
    })
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
    fn publish_result_uses_signed_app_identity() {
        let metadata = serde_json::json!({
            "ios_app": { "bundle_identifier": "com.example.app", "version": "1.2.0", "build_number": "42" }
        });
        let config = AppStorePublishConfig {
            enabled: true,
            testflight_groups: vec!["QA".to_string(), "Beta".to_string()],
            ..AppStorePublishConfig::default()
        };
        let result = publish_result(&metadata, &config).unwrap();
        assert_eq!(result.store, PublishStore::AppStore);
        assert_eq!(result.track, "QA, Beta");
        assert_eq!(
            result.testflight_groups,
            Some(vec!["QA".to_string(), "Beta".to_string()])
        );
        assert_eq!(result.package_name, "com.example.app");
        assert_eq!(result.version_code, 42);
        assert_eq!(result.version_name.as_deref(), Some("1.2.0"));
        assert!(publish_result(&serde_json::json!({}), &config).is_err());
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

    #[test]
    fn product_errors_after_log_lines_count_as_failure() {
        let stdout = "2026-10-21 Running altool...\n{\n  \"product-errors\": [{\"message\": \"Redundant Binary Upload.\", \"userInfo\": {\"NSLocalizedFailureReason\": \"You've already uploaded a build with build number '21'.\"}}]\n}\n";
        let result = altool_result(stdout);
        assert_eq!(
            altool_product_errors(result.as_ref()),
            vec![
                "Redundant Binary Upload. You've already uploaded a build with build number '21'."
                    .to_string()
            ]
        );
        let ok = altool_result("note: uploading\n{\"success-message\": \"No errors uploading\"}");
        assert!(altool_product_errors(ok.as_ref()).is_empty());
        assert_eq!(ok.unwrap()["success-message"], "No errors uploading");
    }

    #[test]
    fn validation_reasons_inside_the_underlying_error_are_shown() {
        let stdout = r#"{"tool-version":"8.0","product-errors":[{"code":-19241,"message":"Validation failed","userInfo":{"NSLocalizedDescription":"Validation failed","NSLocalizedFailureReason":"Validation failed","NSUnderlyingError":"Error Domain=IrisAPI Code=-19241 \"Validation failed\" UserInfo={status=409, detail=Invalid Bundle. The bundle Runner.app does not support the minimum OS Version specified in the Info.plist, id=8a1c, code=STATE_ERROR.VALIDATION_ERROR, title=Validation failed, NSLocalizedFailureReason=Invalid Bundle. The bundle Runner.app does not support the minimum OS Version specified in the Info.plist, NSLocalizedDescription=Validation failed}","iris-code":"STATE_ERROR.VALIDATION_ERROR"}}]}"#;
        assert_eq!(
            summarize_altool_failure(stdout, ""),
            "Validation failed Invalid Bundle. The bundle Runner.app does not support the minimum OS Version specified in the Info.plist"
        );
    }

    #[test]
    fn validation_detail_in_user_info_is_shown() {
        let stdout = r#"{"product-errors":[{"message":"Validation failed","userInfo":{"detail":"The provided entity includes an attribute with a value that has already been used.","status":"409"}}]}"#;
        assert_eq!(
            summarize_altool_failure(stdout, ""),
            "Validation failed The provided entity includes an attribute with a value that has already been used."
        );
    }

    #[test]
    fn a_bare_validation_failure_shows_what_altool_returned() {
        let stdout = r#"{"product-errors":[{"message":"Validation failed","userInfo":{"status":"409","iris-code":"STATE_ERROR.VALIDATION_ERROR"}}]}"#;
        let summary = summarize_altool_failure(stdout, "*** Error: Validation failed (409)");
        assert!(
            summary.starts_with("Validation failed. altool returned: {"),
            "{summary}"
        );
        assert!(
            summary.contains("STATE_ERROR.VALIDATION_ERROR"),
            "{summary}"
        );
        assert!(
            summary.ends_with("*** Error: Validation failed (409)"),
            "{summary}"
        );
    }

    #[test]
    fn reads_values_from_error_descriptions() {
        let description = "Error Domain=IrisAPI Code=-19241 \"Validation failed\" UserInfo={status=409, detail=Invalid Bundle, a, b=c. Fix it., NSLocalizedDescription=Validation failed}";
        assert_eq!(
            described_error_value(description, "detail").as_deref(),
            Some("Invalid Bundle, a")
        );
        assert_eq!(
            described_error_value(description, "NSLocalizedDescription").as_deref(),
            Some("Validation failed")
        );
        assert_eq!(described_error_value(description, "missing"), None);
        assert_eq!(described_error_value("no user info", "detail"), None);
    }

    #[test]
    fn environment_values_are_hidden_from_failures() {
        let output = r#"ipatool failed ENV: {"OORE_TOKEN"=>"secret1", "HOME"=>"/Users/x"} done"#;
        let hidden = hide_environment_values(output);
        assert!(!hidden.contains("secret1"), "{hidden}");
        assert!(hidden.contains(r#""OORE_TOKEN"=>"...""#), "{hidden}");
        assert!(hidden.ends_with(" done"), "{hidden}");
    }

    fn fake_xcrun(dir: &Path, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(dir).unwrap();
        let path = dir.join("xcrun");
        fs::write(
            &path,
            format!("#!/bin/sh\ncd \"$(dirname \"$0\")\"\n{script}\n"),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "oore-altool-{name}-{}-{}",
            std::process::id(),
            rand::random::<u32>()
        ))
    }

    #[tokio::test]
    async fn a_hung_upload_is_stopped_and_its_key_removed() {
        let dir = test_dir("hang");
        let program = fake_xcrun(&dir, "echo $$ > altool.pid\nexec sleep 600");
        let key_dir = dir.join("keys");
        let error = upload_ipa_with(
            &program,
            Path::new("/tmp/Runner.ipa"),
            &credentials(),
            &key_dir,
            Duration::from_millis(500),
            Duration::ZERO,
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(
            error.starts_with("App Store Connect upload timed out"),
            "{error}"
        );
        assert!(!key_dir.join("AuthKey_ABC123DEF4.p8").exists());
        let pid = fs::read_to_string(dir.join("altool.pid")).unwrap();
        let pid: libc::pid_t = pid.trim().parse().unwrap();
        let mut gone = false;
        for _ in 0..50 {
            let stat = std::process::Command::new("ps")
                .args(["-o", "stat=", "-p", &pid.to_string()])
                .output()
                .unwrap();
            let stat = String::from_utf8_lossy(&stat.stdout);
            if stat.trim().is_empty() || stat.trim().starts_with('Z') {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(gone, "altool {pid} outlived its timeout");
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_lost_reply_followed_by_a_duplicate_counts_as_uploaded() {
        let dir = test_dir("lost-reply");
        let program = fake_xcrun(
            &dir,
            r#"if [ -f first ]; then
  echo '{"product-errors":[{"message":"Redundant Binary Upload. You have already uploaded a build with build number 42."}]}'
  exit 1
fi
touch first
echo 'The request timed out.' >&2
exit 1"#,
        );
        let message = upload_ipa_with(
            &program,
            Path::new("/tmp/Runner.ipa"),
            &credentials(),
            &dir.join("keys"),
            Duration::from_secs(10),
            Duration::ZERO,
        )
        .await
        .unwrap();
        assert!(
            message.contains("Apple already has this build"),
            "{message}"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_duplicate_after_an_auth_failure_is_a_real_failure() {
        let dir = test_dir("auth");
        let program = fake_xcrun(
            &dir,
            r#"if [ -f first ]; then
  echo '{"product-errors":[{"message":"Redundant Binary Upload."}]}'
  exit 1
fi
touch first
echo 'Unable to authenticate. (-19209)' >&2
exit 1"#,
        );
        let error = upload_ipa_with(
            &program,
            Path::new("/tmp/Runner.ipa"),
            &credentials(),
            &dir.join("keys"),
            Duration::from_secs(10),
            Duration::ZERO,
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("Redundant Binary Upload"), "{error}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn transient_failures_are_recognised() {
        assert!(is_transient_altool_failure(
            "Unable to authenticate. (-19209)"
        ));
        assert!(is_transient_altool_failure("The request timed out."));
        assert!(!is_transient_altool_failure("Redundant Binary Upload"));
    }
}
