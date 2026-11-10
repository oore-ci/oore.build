//! Uploads a signed App Store IPA to App Store Connect with `xcrun altool`
//! and the pipeline's App Store Connect API key. The key is written to a
//! private directory only for the duration of the upload.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use oore_contract::{
    AppStoreDestination, AppStoreListing, AppStorePublishConfig, AppStoreReleaseNote,
    PublishPreflight, PublishStore, RunnerAppStoreUploadCredentials, RunnerPublishResult,
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
    altool_args("--upload-app", ipa, credentials)
}

/// `--validate-app` runs App Store Connect's checks on the IPA without
/// uploading it, so it uses no build number.
fn altool_validate_args(ipa: &Path, credentials: &RunnerAppStoreUploadCredentials) -> Vec<String> {
    altool_args("--validate-app", ipa, credentials)
}

fn altool_args(
    action: &str,
    ipa: &Path,
    credentials: &RunnerAppStoreUploadCredentials,
) -> Vec<String> {
    vec![
        "altool".to_string(),
        action.to_string(),
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
/// failed" at the top level and put the reason in `detail`, inside
/// `NSUnderlyingError`, or in altool 26's nested `underlying-errors`.
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
            collect_product_error_details(error, &mut details);
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

/// altool 8 calls an error's info `userInfo`; altool 26 calls it `user-info`
/// and lists the errors behind it under `underlying-errors`.
fn user_info(error: &Value) -> Option<&Value> {
    error.get("userInfo").or_else(|| error.get("user-info"))
}

fn collect_product_error_details(error: &Value, details: &mut Vec<String>) {
    if let Some(info) = user_info(error) {
        collect_error_details(info, details);
    }
    for underlying in error
        .get("underlying-errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        collect_product_error_details(underlying, details);
    }
}

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
            collect_error_details(user_info(underlying).unwrap_or(underlying), details);
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

fn check_credentials(credentials: &RunnerAppStoreUploadCredentials) -> anyhow::Result<()> {
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
    Ok(())
}

async fn upload_ipa_with(
    program: &Path,
    ipa: &Path,
    credentials: &RunnerAppStoreUploadCredentials,
    key_dir: &Path,
    limit: Duration,
    retry_delay: Duration,
) -> anyhow::Result<String> {
    check_credentials(credentials)?;
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

/// Has App Store Connect validate the signed IPA without uploading it, for
/// a release rehearsal. `Err` carries Apple's reasons.
pub(crate) async fn validate_ipa(
    ipa: &Path,
    credentials: &RunnerAppStoreUploadCredentials,
    key_dir: &Path,
) -> anyhow::Result<String> {
    validate_ipa_with(
        Path::new("xcrun"),
        ipa,
        credentials,
        key_dir,
        ALTOOL_TIMEOUT,
        ALTOOL_RETRY_DELAY,
    )
    .await
}

async fn validate_ipa_with(
    program: &Path,
    ipa: &Path,
    credentials: &RunnerAppStoreUploadCredentials,
    key_dir: &Path,
    limit: Duration,
    retry_delay: Duration,
) -> anyhow::Result<String> {
    check_credentials(credentials)?;
    let _key = write_private_key(key_dir, credentials)?;
    let mut attempt = 1;
    loop {
        let output = run_altool(
            program,
            altool_validate_args(ipa, credentials),
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
                .unwrap_or("App Store Connect validated the IPA")
                .to_string());
        }
        let combined = format!("{stdout}\n{stderr}");
        // Validation sends nothing that counts, so it is always safe to retry.
        if attempt == 1 && is_transient_altool_failure(&combined) {
            attempt += 1;
            tokio::time::sleep(retry_delay).await;
            continue;
        }
        anyhow::bail!(
            "App Store Connect validation failed: {}",
            summarize_altool_failure(&stdout, &stderr)
        );
    }
}

/// The publish result for an uploaded IPA, from the app identity the runner
/// recorded when it signed the archive.
pub(crate) fn publish_result(
    ios_metadata: &Value,
    config: &AppStorePublishConfig,
    release_notes: Option<Vec<AppStoreReleaseNote>>,
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
        held_for_review: false,
        release_notes,
        listing: None,
    })
}

/// Longest "What's New" App Store Connect takes.
const MAX_WHATS_NEW_CHARS: usize = 4000;

/// Reads "What's New" per locale from the repository: `<dir>/<locale>.txt`,
/// or fastlane's `<dir>/<locale>/release_notes.txt`. `None` when the
/// directory is missing or holds no notes.
pub(crate) fn read_release_notes_dir(
    workspace: &Path,
    dir: &str,
) -> anyhow::Result<Option<Vec<AppStoreReleaseNote>>> {
    let root = workspace.join(dir);
    let canonical_workspace = workspace.canonicalize()?;
    let Ok(canonical) = root.canonicalize() else {
        return Ok(None);
    };
    if !canonical.starts_with(&canonical_workspace) {
        anyhow::bail!("release_notes_dir {dir} is outside the repository");
    }
    let mut notes = Vec::new();
    for entry in fs::read_dir(&canonical)? {
        let entry = entry?;
        let path = entry.path();
        let (locale, file) = if path.is_dir() {
            let file = path.join("release_notes.txt");
            if !file.is_file() {
                continue;
            }
            (entry.file_name().to_string_lossy().to_string(), file)
        } else if path.extension().is_some_and(|extension| extension == "txt") {
            let Some(stem) = path.file_stem() else {
                continue;
            };
            (stem.to_string_lossy().to_string(), path.clone())
        } else {
            continue;
        };
        let text = fs::read_to_string(&file)?;
        let text = text.trim();
        if text.is_empty() || locale.is_empty() || locale == "default" {
            continue;
        }
        notes.push(AppStoreReleaseNote {
            locale,
            whats_new: text.chars().take(MAX_WHATS_NEW_CHARS).collect(),
        });
    }
    notes.sort_by(|a, b| a.locale.cmp(&b.locale));
    Ok((!notes.is_empty()).then_some(notes))
}

/// Longest App Store listing fields, per App Store Connect.
const MAX_DESCRIPTION_CHARS: usize = 4000;
const MAX_KEYWORDS_CHARS: usize = 100;
const MAX_PROMOTIONAL_TEXT_CHARS: usize = 170;

fn listing_file(
    dir: &Path,
    file: &str,
    max_chars: Option<usize>,
) -> anyhow::Result<Option<String>> {
    let path = dir.join(file);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if let Some(max) = max_chars
        && text.chars().count() > max
    {
        anyhow::bail!(
            "{} is {} characters; App Store Connect takes at most {max}",
            path.display(),
            text.chars().count()
        );
    }
    Ok(Some(text.to_string()))
}

/// Reads the App Store listing in fastlane's layout:
/// `<dir>/<locale>/description.txt`, `keywords.txt`,
/// `promotional_text.txt`, `marketing_url.txt` and `support_url.txt`.
/// `None` when the directory is missing or holds no listing.
pub(crate) fn read_listing_dir(
    workspace: &Path,
    dir: &str,
) -> anyhow::Result<Option<Vec<AppStoreListing>>> {
    let canonical_workspace = workspace.canonicalize()?;
    let Ok(canonical) = workspace.join(dir).canonicalize() else {
        return Ok(None);
    };
    if !canonical.starts_with(&canonical_workspace) {
        anyhow::bail!("listing_dir {dir} is outside the repository");
    }
    let mut listings = Vec::new();
    for entry in fs::read_dir(&canonical)? {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }
        let locale = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if locale.is_empty() || locale == "default" || locale == "review_information" {
            continue;
        }
        let listing = AppStoreListing {
            locale,
            description: listing_file(&path, "description.txt", Some(MAX_DESCRIPTION_CHARS))?,
            keywords: listing_file(&path, "keywords.txt", Some(MAX_KEYWORDS_CHARS))?,
            promotional_text: listing_file(
                &path,
                "promotional_text.txt",
                Some(MAX_PROMOTIONAL_TEXT_CHARS),
            )?,
            marketing_url: listing_file(&path, "marketing_url.txt", None)?,
            support_url: listing_file(&path, "support_url.txt", None)?,
        };
        if listing
            != (AppStoreListing {
                locale: listing.locale.clone(),
                ..AppStoreListing::default()
            })
        {
            listings.push(listing);
        }
    }
    listings.sort_by(|a, b| a.locale.cmp(&b.locale));
    Ok((!listings.is_empty()).then_some(listings))
}

/// Whether a PNG has an alpha channel, from its IHDR colour type (4 is grey
/// with alpha, 6 is RGBA). `None` when the file is not a PNG.
fn png_has_alpha(bytes: &[u8]) -> Option<bool> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 26 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    Some(matches!(bytes[25], 4 | 6))
}

fn find_app_icon_sets(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(
            name.as_ref(),
            "Pods" | "build" | "DerivedData" | ".git" | ".dart_tool" | "node_modules" | ".symlinks"
        ) {
            continue;
        }
        if name == "AppIcon.appiconset" {
            found.push(path);
        } else {
            find_app_icon_sets(&path, depth - 1, found);
        }
    }
}

/// Local checks for what App Store Connect refuses after the upload: an app
/// icon with transparency. Returns one line per finding.
pub(crate) fn preflight_findings(workspace: &Path) -> Vec<String> {
    let mut sets = Vec::new();
    find_app_icon_sets(workspace, 8, &mut sets);
    let mut findings = Vec::new();
    for set in sets {
        let Ok(entries) = fs::read_dir(&set) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.extension().is_some_and(|extension| extension == "png") {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            if png_has_alpha(&bytes) == Some(true) {
                let shown = path.strip_prefix(workspace).unwrap_or(&path);
                findings.push(format!(
                    "App icon {} has an alpha channel; App Store Connect refuses app icons with transparency",
                    shown.display()
                ));
            }
        }
    }
    findings
}

/// Applies the pre-flight level to the findings: `Err` when they block the
/// upload, otherwise warning lines to log.
pub(crate) fn apply_preflight(
    level: PublishPreflight,
    findings: Vec<String>,
) -> anyhow::Result<Vec<String>> {
    if findings.is_empty() || level == PublishPreflight::Off {
        return Ok(Vec::new());
    }
    if level == PublishPreflight::Block {
        anyhow::bail!(
            "Pre-flight found what App Store Connect would refuse; nothing was uploaded:\n{}",
            findings.join("\n")
        );
    }
    Ok(findings
        .into_iter()
        .map(|finding| format!("[oore-publish] Pre-flight warning: {finding}"))
        .collect())
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
    fn reads_alpha_from_the_png_header() {
        let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        png.extend_from_slice(&[0, 0, 4, 0, 0, 0, 4, 0, 8, 6, 0, 0, 0]);
        assert_eq!(png_has_alpha(&png), Some(true));
        png[25] = 2;
        assert_eq!(png_has_alpha(&png), Some(false));
        assert_eq!(png_has_alpha(b"not a png at all, really not"), None);
    }

    #[test]
    fn preflight_blocks_or_warns() {
        let findings = vec!["icon".to_string()];
        assert!(apply_preflight(PublishPreflight::Block, findings.clone()).is_err());
        assert_eq!(
            apply_preflight(PublishPreflight::Warn, findings.clone())
                .unwrap()
                .len(),
            1
        );
        assert!(
            apply_preflight(PublishPreflight::Off, findings)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn reads_release_notes_from_the_repository() {
        let dir = test_dir("release-notes");
        fs::create_dir_all(dir.join("notes/de-DE")).unwrap();
        fs::write(dir.join("notes/en-US.txt"), "Faster sync\n").unwrap();
        fs::write(dir.join("notes/de-DE/release_notes.txt"), "Schneller").unwrap();
        let notes = read_release_notes_dir(&dir, "notes").unwrap().unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].locale, "de-DE");
        assert_eq!(notes[1].whats_new, "Faster sync");
        assert!(read_release_notes_dir(&dir, "missing").unwrap().is_none());
        assert!(read_release_notes_dir(&dir, "../").is_err());
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn reads_the_listing_from_the_repository() {
        let dir = std::env::temp_dir().join(format!("oore-listing-{}", std::process::id()));
        fs::create_dir_all(dir.join("meta/en-US")).unwrap();
        fs::create_dir_all(dir.join("meta/review_information")).unwrap();
        fs::create_dir_all(dir.join("meta/de-DE")).unwrap();
        fs::write(dir.join("meta/en-US/description.txt"), "Shop faster\n").unwrap();
        fs::write(dir.join("meta/en-US/keywords.txt"), "shop,cart").unwrap();
        fs::write(dir.join("meta/review_information/notes.txt"), "x").unwrap();
        fs::write(dir.join("meta/de-DE/release_notes.txt"), "only notes").unwrap();
        let listing = read_listing_dir(&dir, "meta").unwrap().unwrap();
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].locale, "en-US");
        assert_eq!(listing[0].description.as_deref(), Some("Shop faster"));
        assert_eq!(listing[0].keywords.as_deref(), Some("shop,cart"));
        fs::write(dir.join("meta/en-US/keywords.txt"), "k".repeat(101)).unwrap();
        assert!(read_listing_dir(&dir, "meta").is_err());
        assert!(read_listing_dir(&dir, "missing").unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
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
        let result = publish_result(&metadata, &config, None).unwrap();
        assert_eq!(result.store, PublishStore::AppStore);
        assert_eq!(result.track, "QA, Beta");
        assert_eq!(
            result.testflight_groups,
            Some(vec!["QA".to_string(), "Beta".to_string()])
        );
        assert_eq!(result.package_name, "com.example.app");
        assert_eq!(result.version_code, 42);
        assert_eq!(result.version_name.as_deref(), Some("1.2.0"));
        assert!(publish_result(&serde_json::json!({}), &config, None).is_err());
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
    fn altool_26_user_info_and_underlying_errors_are_read() {
        // Trimmed from a real altool 26.10.1 rejection of pulse.
        let stdout = r#"{"os-version":"Version 26.0.1 (Build 25A362)","product-errors":[{"code":409,"message":"Validation failed","underlying-errors":[{"code":-19241,"message":"Validation failed","underlying-errors":[],"user-info":{"NSLocalizedDescription":"Validation failed","NSLocalizedFailureReason":"Invalid large app icon. The large app icon in the asset catalog in “Runner.app” can’t be transparent or contain an alpha channel.","code":"STATE_ERROR.VALIDATION_ERROR","detail":"Invalid large app icon. The large app icon in the asset catalog in “Runner.app” can’t be transparent or contain an alpha channel.","status":"409","title":"Validation failed"}}],"user-info":{"NSLocalizedDescription":"Validation failed","NSLocalizedFailureReason":"Invalid large app icon. The large app icon in the asset catalog in “Runner.app” can’t be transparent or contain an alpha channel. (ID: 5d13c193)","iris-code":"STATE_ERROR.VALIDATION_ERROR"}}],"tool-version":"26.10.1 (171001)"}"#;
        assert_eq!(
            summarize_altool_failure(stdout, "UPLOAD FAILED with 1 error"),
            "Validation failed Invalid large app icon. The large app icon in the asset catalog in “Runner.app” can’t be transparent or contain an alpha channel. (ID: 5d13c193)"
        );
        let nested_only = r#"{"product-errors":[{"message":"Validation failed","underlying-errors":[{"message":"Validation failed","user-info":{"detail":"Missing icon."}}]}]}"#;
        assert_eq!(
            summarize_altool_failure(nested_only, ""),
            "Validation failed Missing icon."
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
            // Long enough for a busy host to start the script and record its pid.
            Duration::from_secs(3),
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
        // No pid file means the timeout stopped the script before it ran
        // its first line, so nothing it started can be left behind.
        let Ok(pid) = fs::read_to_string(dir.join("altool.pid")) else {
            let _ = fs::remove_dir_all(dir);
            return;
        };
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

    #[tokio::test]
    async fn validation_validates_and_reports_apples_reasons() {
        let dir = test_dir("validate");
        let program = fake_xcrun(
            &dir,
            r#"echo "$@" > args
case "$*" in
  *--upload-app*) echo 'uploaded'; exit 0 ;;
esac
echo '{"product-errors":[{"message":"Invalid Bundle. The bundle version must be higher than the previously uploaded version: 41."}]}'
exit 1"#,
        );
        let error = validate_ipa_with(
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
        assert!(
            error.starts_with("App Store Connect validation failed"),
            "{error}"
        );
        assert!(error.contains("must be higher"), "{error}");
        let args = fs::read_to_string(dir.join("args")).unwrap();
        assert!(args.contains("--validate-app"), "{args}");
        assert!(!args.contains("--upload-app"), "{args}");
        assert!(!dir.join("keys").join("AuthKey_ABC123DEF4.p8").exists());
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
