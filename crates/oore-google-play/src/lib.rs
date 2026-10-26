//! Google Play Developer API client shared by the runner and the daemon.
//!
//! Covers service account authentication and the Edits flow: insert an edit,
//! upload a bundle and its deobfuscation file, set a track release, commit.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

mod aab;

pub use aab::{AabManifest, read_aab_manifest};

const API_BASE: &str = "https://androidpublisher.googleapis.com/androidpublisher/v3/applications";
const UPLOAD_BASE: &str =
    "https://androidpublisher.googleapis.com/upload/androidpublisher/v3/applications";
const PUBLISHER_SCOPE: &str = "https://www.googleapis.com/auth/androidpublisher";
/// Scope for the Play Developer Reporting API (vitals such as crash rate).
pub const REPORTING_SCOPE: &str = "https://www.googleapis.com/auth/playdeveloperreporting";
const REPORTING_BASE: &str = "https://playdeveloperreporting.googleapis.com/v1beta1/apps";
/// Reporting API daily metrics are bucketed in Pacific time.
const REPORTING_TIME_ZONE: &str = "America/Los_Angeles";
const CRASH_RATE_LOOKBACK_DAYS: i64 = 7;
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";

/// Attempts for a Play call that fails with 429 or a 5xx, which Google
/// documents as retryable.
const PLAY_ATTEMPTS: u32 = 3;
const PLAY_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Play limits release notes to 500 characters per language.
pub const MAX_RELEASE_NOTES_CHARS: usize = 500;

#[derive(Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    #[serde(default)]
    token_uri: Option<String>,
}

#[derive(Serialize)]
struct JwtClaims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

/// The `client_email` of a service account JSON key.
pub fn service_account_email(service_account_json: &str) -> anyhow::Result<String> {
    let key: ServiceAccountKey = serde_json::from_str(service_account_json)
        .context("the service account secret is not a Google service account JSON key")?;
    Ok(key.client_email)
}

/// Exchange a service account JSON key for an androidpublisher access token.
pub async fn access_token(
    http: &reqwest::Client,
    service_account_json: &str,
) -> anyhow::Result<String> {
    access_token_with_scope(http, service_account_json, PUBLISHER_SCOPE).await
}

/// Exchange a service account JSON key for an access token with `scope`.
pub async fn access_token_with_scope(
    http: &reqwest::Client,
    service_account_json: &str,
    scope: &str,
) -> anyhow::Result<String> {
    let key: ServiceAccountKey = serde_json::from_str(service_account_json)
        .context("the service account secret is not a Google service account JSON key")?;
    let token_uri = key.token_uri.as_deref().unwrap_or(DEFAULT_TOKEN_URI);
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let claims = JwtClaims {
        iss: &key.client_email,
        scope,
        aud: token_uri,
        iat: now,
        exp: now + 3600,
    };
    let signing_key = jsonwebtoken::EncodingKey::from_rsa_pem(key.private_key.as_bytes())
        .context("the service account private_key is not a valid RSA PEM key")?;
    let assertion = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
        &claims,
        &signing_key,
    )?;

    // The assertion is base64url, so it needs no form encoding.
    let response = http
        .post(token_uri)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(format!(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={assertion}"
        ))
        .send()
        .await
        .context("failed to reach the Google token endpoint")?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        bail!(
            "Google rejected the service account ({status}): {}",
            google_error_message(&body)
        );
    }
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
    }
    let token: TokenResponse =
        serde_json::from_str(&body).context("unexpected Google token response")?;
    Ok(token.access_token)
}

/// Extract `error.message` (API errors) or `error_description` (OAuth errors).
fn google_error_message(body: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    parsed
        .pointer("/error/message")
        .or_else(|| parsed.get("error_description"))
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| html_title(body))
        .unwrap_or_else(|| body.chars().take(500).collect())
}

fn wants_changes_not_sent_for_review(message: &str) -> bool {
    message.contains("changesNotSentForReview to true")
}

/// Google's front end answers some errors with an HTML page; its title
/// ("Error 411 (Length Required)!!1") says all the page does.
fn html_title(body: &str) -> Option<String> {
    let start = body.find("<title>")? + "<title>".len();
    let end = start + body[start..].find("</title>")?;
    Some(body[start..end].trim().to_string())
}

fn play_access_hint(status: reqwest::StatusCode) -> &'static str {
    match status.as_u16() {
        401 | 403 => {
            ". Check that the service account is invited in Play Console under Users and permissions with release access to this app, and that the Google Play Android Developer API is enabled in the key's Google Cloud project"
        }
        _ => "",
    }
}

/// Map Oore's track names to Play's: `closed` is `alpha`, `open` is `beta`.
/// Other names (internal, production, custom closed tracks) pass through.
pub fn play_track_name(track: &str) -> String {
    match track {
        "closed" => "alpha".to_string(),
        "open" => "beta".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReleaseStatus {
    Completed,
    InProgress,
    Draft,
    Halted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalizedText {
    pub language: String,
    pub text: String,
}

/// One release on a track, in the Play API's shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackRelease {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Play sends version codes as strings.
    #[serde(default, with = "version_codes")]
    pub version_codes: Vec<i64>,
    pub status: ReleaseStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_fraction: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub release_notes: Vec<LocalizedText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_app_update_priority: Option<u8>,
}

mod version_codes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(codes: &[i64], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(codes.iter().map(ToString::to_string))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<i64>, D::Error> {
        Vec::<String>::deserialize(deserializer)?
            .iter()
            .map(|code| code.parse().map_err(serde::de::Error::custom))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackState {
    pub track: String,
    #[serde(default)]
    pub releases: Vec<TrackRelease>,
}

/// An open Play edit. Nothing is visible in Play Console until `commit`.
pub struct PlayEdits<'a> {
    http: &'a reqwest::Client,
    token: String,
    package_name: String,
    edit_id: String,
    api_base: String,
    upload_base: String,
}

/// Google Play answered with an error status. Anything else that fails a
/// call, such as a dropped connection, leaves its outcome unknown.
#[derive(Debug)]
pub struct PlayApiError {
    pub action: String,
    pub status: reqwest::StatusCode,
    pub message: String,
}

impl std::fmt::Display for PlayApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Google Play could not {} ({}): {}{}",
            self.action,
            self.status,
            self.message,
            play_access_hint(self.status)
        )
    }
}

impl std::error::Error for PlayApiError {}

/// True when Google Play answered `error` itself, so the call surely did not
/// take effect, rather than the request or its reply getting lost.
pub fn is_play_rejection(error: &anyhow::Error) -> bool {
    error.downcast_ref::<PlayApiError>().is_some()
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Sends the request `build` makes, up to `attempts` times: again after a
/// 429 or 5xx, and after a lost connection too when `idempotent`. Returns
/// the reply body.
async fn play_send(
    build: impl Fn() -> anyhow::Result<reqwest::RequestBuilder>,
    action: &str,
    idempotent: bool,
    attempts: u32,
) -> anyhow::Result<String> {
    let mut attempt = 1;
    loop {
        let last = attempt >= attempts;
        match build()?.send().await {
            Ok(response) => {
                let status = response.status();
                match response.text().await {
                    Ok(body) if status.is_success() => return Ok(body),
                    Ok(body) if last || !is_retryable_status(status) => {
                        return Err(PlayApiError {
                            action: action.to_string(),
                            status,
                            message: google_error_message(&body),
                        }
                        .into());
                    }
                    Ok(_) => {}
                    Err(error) if last || !idempotent => {
                        return Err(anyhow::Error::new(error)
                            .context(format!("lost Google Play's reply to {action}")));
                    }
                    Err(_) => {}
                }
            }
            Err(error) if last || !idempotent => {
                return Err(anyhow::Error::new(error)
                    .context(format!("failed to reach Google Play to {action}")));
            }
            Err(_) => {}
        }
        tokio::time::sleep(PLAY_RETRY_DELAY * 2u32.pow(attempt - 1)).await;
        attempt += 1;
    }
}

/// Send `request` and parse its JSON reply. A failure names `action`, the
/// HTTP status and Google's own error message, plus a hint on 401 and 403.
/// A 429 or 5xx is retried; so is a lost connection on a GET, PUT or DELETE.
pub async fn play_json<T: serde::de::DeserializeOwned>(
    request: reqwest::RequestBuilder,
    action: &str,
) -> anyhow::Result<T> {
    let body = match request.try_clone().and_then(|clone| clone.build().ok()) {
        Some(built) => {
            let idempotent = matches!(
                *built.method(),
                reqwest::Method::GET | reqwest::Method::PUT | reqwest::Method::DELETE
            );
            play_send(
                || {
                    request
                        .try_clone()
                        .context("Google Play request could not be repeated")
                },
                action,
                idempotent,
                PLAY_ATTEMPTS,
            )
            .await?
        }
        // A streamed body can only be sent once.
        None => {
            let once = std::sync::Mutex::new(Some(request));
            play_send(
                || {
                    once.lock()
                        .ok()
                        .and_then(|mut request| request.take())
                        .context("Google Play request could not be repeated")
                },
                action,
                false,
                1,
            )
            .await?
        }
    };
    parse_play_reply(&body, action)
}

fn parse_play_reply<T: serde::de::DeserializeOwned>(body: &str, action: &str) -> anyhow::Result<T> {
    serde_json::from_str(if body.is_empty() { "null" } else { body })
        .with_context(|| format!("unexpected Google Play response to {action}"))
}

/// Streams the file at `path` as a request body, so a large bundle is never
/// held in memory. Returns the body and its length.
async fn file_body(path: &Path) -> anyhow::Result<(reqwest::Body, u64)> {
    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("failed to read {}", path.display()))?;
    let length = file.metadata().await?.len();
    let chunks = async_stream::stream! {
        let mut buffer = vec![0u8; 256 * 1024];
        loop {
            match file.read(&mut buffer).await {
                Ok(0) => break,
                Ok(read) => yield Ok::<_, std::io::Error>(buffer[..read].to_vec()),
                Err(error) => {
                    yield Err(error);
                    break;
                }
            }
        }
    };
    Ok((reqwest::Body::wrap_stream(chunks), length))
}

impl<'a> PlayEdits<'a> {
    pub async fn insert(
        http: &'a reqwest::Client,
        token: &str,
        package_name: &str,
    ) -> anyhow::Result<Self> {
        Self::insert_at(http, API_BASE, UPLOAD_BASE, token, package_name).await
    }

    /// [`PlayEdits::insert`] against other API hosts, for tests.
    pub async fn insert_at(
        http: &'a reqwest::Client,
        api_base: &str,
        upload_base: &str,
        token: &str,
        package_name: &str,
    ) -> anyhow::Result<Self> {
        #[derive(Deserialize)]
        struct Edit {
            id: String,
        }
        let edit: Edit = play_json(
            http.post(format!("{api_base}/{package_name}/edits"))
                .bearer_auth(token)
                .json(&serde_json::json!({})),
            "open an edit",
        )
        .await?;
        Ok(Self {
            http,
            token: token.to_string(),
            package_name: package_name.to_string(),
            edit_id: edit.id,
            api_base: api_base.to_string(),
            upload_base: upload_base.to_string(),
        })
    }

    pub fn edit_id(&self) -> &str {
        &self.edit_id
    }

    fn api(&self, path: &str) -> String {
        format!(
            "{}/{}/edits/{}{path}",
            self.api_base, self.package_name, self.edit_id
        )
    }

    fn upload(&self, path: &str) -> String {
        format!(
            "{}/{}/edits/{}{path}?uploadType=media",
            self.upload_base, self.package_name, self.edit_id
        )
    }

    /// Streams the file at `path` to `url`, again after a 429 or 5xx.
    async fn upload_file<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        path: &Path,
        action: &str,
    ) -> anyhow::Result<T> {
        let mut attempt = 1;
        loop {
            let (body, length) = file_body(path).await?;
            let request = self
                .http
                .post(url)
                .bearer_auth(&self.token)
                .header("content-type", "application/octet-stream")
                .header("content-length", length)
                .body(body);
            match play_json(request, action).await {
                Err(error)
                    if attempt < PLAY_ATTEMPTS
                        && error
                            .downcast_ref::<PlayApiError>()
                            .is_some_and(|error| is_retryable_status(error.status)) =>
                {
                    tokio::time::sleep(PLAY_RETRY_DELAY * 2u32.pow(attempt - 1)).await;
                    attempt += 1;
                }
                result => return result,
            }
        }
    }

    /// Upload an AAB and return the version code Play assigned to it.
    pub async fn upload_bundle(&self, path: &Path) -> anyhow::Result<i64> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Bundle {
            version_code: i64,
        }
        let bundle: Bundle = self
            .upload_file(&self.upload("/bundles"), path, "upload the bundle")
            .await?;
        Ok(bundle.version_code)
    }

    /// Upload R8/ProGuard `mapping.txt` for a version code.
    pub async fn upload_mapping(&self, version_code: i64, path: &Path) -> anyhow::Result<()> {
        let _: serde_json::Value = self
            .upload_file(
                &self.upload(&format!("/apks/{version_code}/deobfuscationFiles/proguard")),
                path,
                "upload mapping.txt",
            )
            .await?;
        Ok(())
    }

    pub async fn get_track(&self, track: &str) -> anyhow::Result<TrackState> {
        play_json(
            self.http
                .get(self.api(&format!("/tracks/{track}")))
                .bearer_auth(&self.token),
            "read the track",
        )
        .await
    }

    /// Replace the track's releases with `release`.
    pub async fn update_track(&self, track: &str, release: &TrackRelease) -> anyhow::Result<()> {
        let _: serde_json::Value = play_json(
            self.http
                .put(self.api(&format!("/tracks/{track}")))
                .bearer_auth(&self.token)
                .json(&TrackState {
                    track: track.to_string(),
                    releases: vec![release.clone()],
                }),
            "update the track",
        )
        .await?;
        Ok(())
    }

    /// Commit the edit. Apps whose changes Play reviews in batches (managed
    /// publishing, or some transferred apps) refuse a plain commit and ask for
    /// `changesNotSentForReview=true`; that commit is retried once, as
    /// fastlane does.
    ///
    /// On an error the edit stays open for the caller to delete. An error
    /// for which [`is_play_rejection`] is false may hide a commit that went
    /// through.
    pub async fn commit(&self) -> anyhow::Result<()> {
        match play_json::<serde_json::Value>(self.commit_request(false), "commit the edit").await {
            Ok(_) => Ok(()),
            Err(error) if wants_changes_not_sent_for_review(&error.to_string()) => {
                let _: serde_json::Value =
                    play_json(self.commit_request(true), "commit the edit").await?;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    // Google's front end answers a bodyless POST with 411 Length Required,
    // so send an empty JSON object as insert does.
    fn commit_request(&self, changes_not_sent_for_review: bool) -> reqwest::RequestBuilder {
        let mut request = self.http.post(self.api(":commit"));
        if changes_not_sent_for_review {
            request = request.query(&[("changesNotSentForReview", "true")]);
        }
        request
            .bearer_auth(&self.token)
            .json(&serde_json::json!({}))
    }

    /// Discard the edit. Best effort: an uncommitted edit also expires on its own.
    pub async fn delete(&self) -> anyhow::Result<()> {
        let response = self
            .http
            .delete(self.api(""))
            .bearer_auth(&self.token)
            .send()
            .await?;
        if !response.status().is_success() {
            bail!(
                "Google Play could not delete the edit ({})",
                response.status()
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
struct ReportingDate {
    year: i32,
    month: u32,
    day: u32,
}

impl ReportingDate {
    fn to_wire(self) -> serde_json::Value {
        serde_json::json!({
            "year": self.year,
            "month": self.month,
            "day": self.day,
            "timeZone": {"id": REPORTING_TIME_ZONE},
        })
    }

    fn days_before(self, days: i64) -> anyhow::Result<Self> {
        use chrono::Datelike;
        let date = chrono::NaiveDate::from_ymd_opt(self.year, self.month, self.day)
            .context("invalid reporting date")?
            - chrono::Duration::days(days);
        Ok(Self {
            year: date.year(),
            month: date.month(),
            day: date.day(),
        })
    }
}

/// Latest daily user-perceived crash rate (0..1) for one version code, or
/// `None` when Play has no data for it yet.
pub async fn crash_rate(
    http: &reqwest::Client,
    reporting_token: &str,
    package_name: &str,
    version_code: i64,
) -> anyhow::Result<Option<f64>> {
    // Querying past the freshest available day is an error, so read it first.
    let metric_set: serde_json::Value = play_json(
        http.get(format!(
            "{REPORTING_BASE}/{package_name}/crashRateMetricSet"
        ))
        .bearer_auth(reporting_token),
        "read crash rate freshness",
    )
    .await?;
    let Some(end) = metric_set
        .pointer("/freshnessInfo/freshnesses")
        .and_then(|value| value.as_array())
        .and_then(|freshnesses| {
            freshnesses
                .iter()
                .find(|entry| entry["aggregationPeriod"] == "DAILY")
        })
        .and_then(|entry| {
            serde_json::from_value::<ReportingDate>(entry["latestEndTime"].clone()).ok()
        })
    else {
        return Ok(None);
    };
    let start = end.days_before(CRASH_RATE_LOOKBACK_DAYS)?;
    let response: serde_json::Value = play_json(
        http.post(format!(
            "{REPORTING_BASE}/{package_name}/crashRateMetricSet:query"
        ))
        .bearer_auth(reporting_token)
        .json(&serde_json::json!({
            "timelineSpec": {
                "aggregationPeriod": "DAILY",
                "startTime": start.to_wire(),
                "endTime": end.to_wire(),
            },
            "dimensions": ["versionCode"],
            "metrics": ["userPerceivedCrashRate"],
            "pageSize": 1000,
        })),
        "query crash rate",
    )
    .await?;
    Ok(latest_crash_rate(&response, version_code))
}

fn latest_crash_rate(response: &serde_json::Value, version_code: i64) -> Option<f64> {
    let code = version_code.to_string();
    response["rows"]
        .as_array()?
        .iter()
        .filter(|row| {
            row["dimensions"].as_array().is_some_and(|dimensions| {
                dimensions.iter().any(|dimension| {
                    dimension["dimension"] == "versionCode"
                        && dimension["stringValue"].as_str() == Some(&code)
                })
            })
        })
        .filter_map(|row| {
            let date: ReportingDate = serde_json::from_value(row["startTime"].clone()).ok()?;
            let rate = row["metrics"]
                .as_array()?
                .iter()
                .find(|metric| metric["metric"] == "userPerceivedCrashRate")?
                .pointer("/decimalValue/value")?
                .as_str()?
                .parse::<f64>()
                .ok()?;
            Some(((date.year, date.month, date.day), rate))
        })
        .max_by_key(|(date, _)| *date)
        .map(|(_, rate)| rate)
}

/// One day of app-wide vitals, dated in Pacific time like Play Console.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayVitalsDay {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub rate: Option<f64>,
}

/// An app-wide Android vitals rate over the last 28 days.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayVitalsRate {
    /// Latest 28-day user-weighted rate (0..1), the figure Play Console shows.
    pub rate_28d: Option<f64>,
    /// Daily rates, oldest first.
    pub days: Vec<PlayVitalsDay>,
}

/// A grouped crash or ANR from Android vitals.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayErrorIssue {
    /// `CRASH` or `ANR`.
    pub kind: String,
    pub cause: String,
    pub location: String,
    pub reports: i64,
    pub users: i64,
    /// RFC 3339 start of the hour of the latest report.
    pub last_report_time: Option<String>,
    pub url: Option<String>,
}

const VITALS_LOOKBACK_DAYS: i64 = 27;

async fn latest_daily_date(
    http: &reqwest::Client,
    reporting_token: &str,
    package_name: &str,
    metric_set: &str,
) -> anyhow::Result<Option<ReportingDate>> {
    let response: serde_json::Value = play_json(
        http.get(format!("{REPORTING_BASE}/{package_name}/{metric_set}"))
            .bearer_auth(reporting_token),
        "read vitals freshness",
    )
    .await?;
    Ok(response
        .pointer("/freshnessInfo/freshnesses")
        .and_then(|value| value.as_array())
        .and_then(|freshnesses| {
            freshnesses
                .iter()
                .find(|entry| entry["aggregationPeriod"] == "DAILY")
        })
        .and_then(|entry| {
            serde_json::from_value::<ReportingDate>(entry["latestEndTime"].clone()).ok()
        }))
}

/// App-wide user-perceived crash rate (`crashRateMetricSet`) or ANR rate
/// (`anrRateMetricSet`) for the last 28 days with data.
pub async fn app_vitals_rate(
    http: &reqwest::Client,
    reporting_token: &str,
    package_name: &str,
    kind: PlayVitalsKind,
) -> anyhow::Result<PlayVitalsRate> {
    let (metric_set, daily, rolling) = kind.metrics();
    let Some(end) = latest_daily_date(http, reporting_token, package_name, metric_set).await?
    else {
        return Ok(PlayVitalsRate::default());
    };
    let start = end.days_before(VITALS_LOOKBACK_DAYS)?;
    let response: serde_json::Value = play_json(
        http.post(format!(
            "{REPORTING_BASE}/{package_name}/{metric_set}:query"
        ))
        .bearer_auth(reporting_token)
        .json(&serde_json::json!({
            "timelineSpec": {
                "aggregationPeriod": "DAILY",
                "startTime": start.to_wire(),
                "endTime": end.to_wire(),
            },
            "metrics": [daily, rolling],
            "pageSize": 100,
        })),
        "query vitals",
    )
    .await?;
    Ok(parse_vitals_rate(&response, daily, rolling))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayVitalsKind {
    CrashRate,
    AnrRate,
}

impl PlayVitalsKind {
    fn metrics(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::CrashRate => (
                "crashRateMetricSet",
                "userPerceivedCrashRate",
                "userPerceivedCrashRate28dUserWeighted",
            ),
            Self::AnrRate => (
                "anrRateMetricSet",
                "userPerceivedAnrRate",
                "userPerceivedAnrRate28dUserWeighted",
            ),
        }
    }
}

fn metric_value(row: &serde_json::Value, metric: &str) -> Option<f64> {
    row["metrics"]
        .as_array()?
        .iter()
        .find(|entry| entry["metric"] == metric)?
        .pointer("/decimalValue/value")?
        .as_str()?
        .parse::<f64>()
        .ok()
}

fn parse_vitals_rate(response: &serde_json::Value, daily: &str, rolling: &str) -> PlayVitalsRate {
    let mut rows: Vec<(ReportingDate, Option<f64>, Option<f64>)> = response["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let date: ReportingDate =
                        serde_json::from_value(row["startTime"].clone()).ok()?;
                    Some((date, metric_value(row, daily), metric_value(row, rolling)))
                })
                .collect()
        })
        .unwrap_or_default();
    rows.sort_by_key(|(date, _, _)| (date.year, date.month, date.day));
    PlayVitalsRate {
        rate_28d: rows.iter().rev().find_map(|(_, _, rolling)| *rolling),
        days: rows
            .into_iter()
            .map(|(date, rate, _)| PlayVitalsDay {
                date: format!("{:04}-{:02}-{:02}", date.year, date.month, date.day),
                rate,
            })
            .collect(),
    }
}

/// The most-reported user-perceived crashes and ANRs of the last 28 days.
pub async fn top_error_issues(
    http: &reqwest::Client,
    reporting_token: &str,
    package_name: &str,
    limit: u32,
) -> anyhow::Result<Vec<PlayErrorIssue>> {
    use chrono::{Duration, Timelike, Utc};
    let end = Utc::now()
        .with_minute(0)
        .and_then(|time| time.with_second(0))
        .and_then(|time| time.with_nanosecond(0))
        .context("invalid time")?;
    let start = end - Duration::days(28);
    let mut query: Vec<(String, String)> = vec![
        ("pageSize".into(), limit.to_string()),
        ("orderBy".into(), "errorReportCount desc".into()),
        (
            "filter".into(),
            "(errorIssueType = CRASH OR errorIssueType = ANR) AND isUserPerceived".into(),
        ),
    ];
    for (name, time) in [("startTime", start), ("endTime", end)] {
        use chrono::Datelike;
        for (field, value) in [
            ("year", time.year().to_string()),
            ("month", time.month().to_string()),
            ("day", time.day().to_string()),
            ("hours", time.hour().to_string()),
            ("timeZone.id", "UTC".to_string()),
        ] {
            query.push((format!("interval.{name}.{field}"), value));
        }
    }
    let response: serde_json::Value = play_json(
        http.get(format!(
            "{REPORTING_BASE}/{package_name}/errorIssues:search"
        ))
        .query(&query)
        .bearer_auth(reporting_token),
        "search crash and ANR issues",
    )
    .await?;
    Ok(parse_error_issues(&response))
}

fn parse_error_issues(response: &serde_json::Value) -> Vec<PlayErrorIssue> {
    let count = |value: &serde_json::Value| {
        value
            .as_str()
            .and_then(|raw| raw.parse::<i64>().ok())
            .or_else(|| value.as_i64())
            .unwrap_or(0)
    };
    let text = |value: &serde_json::Value| value.as_str().unwrap_or_default().to_string();
    response["errorIssues"]
        .as_array()
        .map(|issues| {
            issues
                .iter()
                .map(|issue| PlayErrorIssue {
                    kind: text(&issue["type"]),
                    cause: text(&issue["cause"]),
                    location: text(&issue["location"]),
                    reports: count(&issue["errorReportCount"]),
                    users: count(&issue["distinctUsers"]),
                    last_report_time: issue["lastErrorReportTime"].as_str().map(str::to_string),
                    url: issue["issueUri"].as_str().map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_sends_a_body_so_google_gets_a_content_length() {
        let http = reqwest::Client::new();
        let edits = PlayEdits {
            http: &http,
            token: "token".to_string(),
            package_name: "com.example".to_string(),
            edit_id: "e1".to_string(),
            api_base: API_BASE.to_string(),
            upload_base: UPLOAD_BASE.to_string(),
        };
        let request = edits.commit_request(false).build().unwrap();
        assert_eq!(
            request.url().as_str(),
            format!("{API_BASE}/com.example/edits/e1:commit")
        );
        let body = request.body().and_then(|body| body.as_bytes()).unwrap();
        assert_eq!(body, b"{}");

        let retry = edits.commit_request(true).build().unwrap();
        assert_eq!(retry.url().query(), Some("changesNotSentForReview=true"));
        assert!(wants_changes_not_sent_for_review(
            "Google Play could not commit the edit (400 Bad Request): Changes cannot be sent for review automatically. Please set the query parameter changesNotSentForReview to true."
        ));
        assert!(!wants_changes_not_sent_for_review(
            "The query parameter changesNotSentForReview must not be set"
        ));
    }

    #[test]
    fn html_error_pages_shrink_to_their_title() {
        let page = "<!DOCTYPE html>\n<html lang=en>\n  <meta charset=utf-8>\n  <title>Error 411 (Length Required)!!1</title>\n  <style>*{margin:0}</style>";
        assert_eq!(google_error_message(page), "Error 411 (Length Required)!!1");
    }

    #[test]
    fn track_release_matches_play_wire_format() {
        let release = TrackRelease {
            name: None,
            version_codes: vec![42],
            status: ReleaseStatus::InProgress,
            user_fraction: Some(0.05),
            release_notes: vec![LocalizedText {
                language: "en-US".to_string(),
                text: "Fixes".to_string(),
            }],
            in_app_update_priority: Some(3),
        };
        let json = serde_json::to_value(&release).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "versionCodes": ["42"],
                "status": "inProgress",
                "userFraction": 0.05,
                "releaseNotes": [{"language": "en-US", "text": "Fixes"}],
                "inAppUpdatePriority": 3,
            })
        );
        let back: TrackRelease = serde_json::from_value(json).unwrap();
        assert_eq!(back, release);
    }

    #[test]
    fn vitals_rate_keeps_days_in_order_and_the_latest_rolling_rate() {
        let day = |d: u32, daily: &str, rolling: Option<&str>| {
            let mut metrics = vec![serde_json::json!({
                "metric": "userPerceivedCrashRate",
                "decimalValue": {"value": daily}
            })];
            if let Some(rolling) = rolling {
                metrics.push(serde_json::json!({
                    "metric": "userPerceivedCrashRate28dUserWeighted",
                    "decimalValue": {"value": rolling}
                }));
            }
            serde_json::json!({
                "startTime": {"year": 2026, "month": 10, "day": d},
                "metrics": metrics
            })
        };
        let response = serde_json::json!({"rows": [
            day(19, "0.02", Some("0.011")),
            day(17, "0.01", Some("0.009")),
            day(20, "0.015", None),
        ]});

        let rate = parse_vitals_rate(
            &response,
            "userPerceivedCrashRate",
            "userPerceivedCrashRate28dUserWeighted",
        );

        assert_eq!(rate.rate_28d, Some(0.011));
        let days: Vec<_> = rate
            .days
            .iter()
            .map(|d| (d.date.as_str(), d.rate))
            .collect();
        assert_eq!(
            days,
            [
                ("2026-10-17", Some(0.01)),
                ("2026-10-19", Some(0.02)),
                ("2026-10-20", Some(0.015))
            ]
        );
    }

    #[test]
    fn error_issues_read_int64_strings() {
        let response = serde_json::json!({"errorIssues": [{
            "type": "CRASH",
            "cause": "java.lang.NullPointerException",
            "location": "com.example.CheckoutActivity",
            "errorReportCount": "412",
            "distinctUsers": "97",
            "lastErrorReportTime": "2026-10-21T08:00:00Z",
            "issueUri": "https://play.google.com/console/issue"
        }]});

        let issues = parse_error_issues(&response);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].reports, 412);
        assert_eq!(issues[0].users, 97);
        assert_eq!(issues[0].kind, "CRASH");
    }

    #[test]
    fn crash_rate_picks_latest_day_for_the_version() {
        let response = serde_json::json!({
            "rows": [
                {
                    "startTime": {"year": 2026, "month": 10, "day": 18},
                    "dimensions": [{"dimension": "versionCode", "stringValue": "120"}],
                    "metrics": [{"metric": "userPerceivedCrashRate", "decimalValue": {"value": "0.004"}}]
                },
                {
                    "startTime": {"year": 2026, "month": 10, "day": 19},
                    "dimensions": [{"dimension": "versionCode", "stringValue": "120"}],
                    "metrics": [{"metric": "userPerceivedCrashRate", "decimalValue": {"value": "0.031"}}]
                },
                {
                    "startTime": {"year": 2026, "month": 10, "day": 20},
                    "dimensions": [{"dimension": "versionCode", "stringValue": "119"}],
                    "metrics": [{"metric": "userPerceivedCrashRate", "decimalValue": {"value": "0.5"}}]
                }
            ]
        });
        assert_eq!(latest_crash_rate(&response, 120), Some(0.031));
        assert_eq!(latest_crash_rate(&response, 121), None);
        assert_eq!(latest_crash_rate(&serde_json::json!({}), 120), None);
        let start = ReportingDate {
            year: 2026,
            month: 3,
            day: 2,
        }
        .days_before(7)
        .unwrap();
        assert_eq!(
            start,
            ReportingDate {
                year: 2026,
                month: 2,
                day: 23
            }
        );
    }

    #[test]
    fn track_names_and_errors() {
        assert_eq!(play_track_name("closed"), "alpha");
        assert_eq!(play_track_name("open"), "beta");
        assert_eq!(play_track_name("internal"), "internal");
        assert_eq!(play_track_name("qa-team"), "qa-team");
        assert_eq!(
            google_error_message(
                r#"{"error":{"code":403,"message":"APK specifies a version code that has already been used."}}"#
            ),
            "APK specifies a version code that has already been used."
        );
        assert_eq!(
            google_error_message(
                r#"{"error":"invalid_grant","error_description":"Invalid JWT Signature."}"#
            ),
            "Invalid JWT Signature."
        );
    }

    /// One HTTP request as the mock server saw it.
    #[derive(Debug)]
    struct SeenRequest {
        line: String,
        head: String,
        body_len: usize,
    }

    /// Serves `replies` (status, body) in turn, one per connection; status 0
    /// closes the connection without replying.
    async fn mock_play(
        replies: Vec<(u16, &'static str)>,
    ) -> (String, tokio::task::JoinHandle<Vec<SeenRequest>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let mut seen = Vec::new();
            for (status, body) in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut data = Vec::new();
                let mut buf = vec![0u8; 64 * 1024];
                let (head, body_len) = loop {
                    let read = socket.read(&mut buf).await.unwrap();
                    data.extend_from_slice(&buf[..read]);
                    let text = String::from_utf8_lossy(&data).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let head = text[..end].to_string();
                        let length: usize = head
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|value| value.trim().parse().unwrap())
                            })
                            .unwrap_or(0);
                        while data.len() < end + 4 + length {
                            let read = socket.read(&mut buf).await.unwrap();
                            data.extend_from_slice(&buf[..read]);
                        }
                        break (head, data.len() - end - 4);
                    }
                };
                seen.push(SeenRequest {
                    line: head.lines().next().unwrap().to_string(),
                    head,
                    body_len,
                });
                if status == 0 {
                    continue;
                }
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            seen
        });
        (base, server)
    }

    #[tokio::test]
    async fn a_brief_play_outage_is_retried() {
        let (base, server) = mock_play(vec![(503, "{}"), (200, r#"{"track":"internal"}"#)]).await;
        let http = reqwest::Client::new();
        let track: TrackState = play_json(http.get(format!("{base}/tracks/internal")), "read")
            .await
            .unwrap();
        assert_eq!(track.track, "internal");
        assert_eq!(server.await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_rejected_play_call_is_not_retried() {
        let (base, server) = mock_play(vec![(400, r#"{"error":{"message":"bad"}}"#)]).await;
        let http = reqwest::Client::new();
        let error = play_json::<serde_json::Value>(http.get(format!("{base}/x")), "read")
            .await
            .unwrap_err();
        assert!(is_play_rejection(&error));
        assert!(
            error.to_string().contains("(400 Bad Request): bad"),
            "{error}"
        );
        assert_eq!(server.await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_lost_commit_reply_is_not_a_rejection_and_is_not_resent() {
        let (base, server) = mock_play(vec![(200, r#"{"id":"e1"}"#), (0, "")]).await;
        let http = reqwest::Client::new();
        let edits = PlayEdits::insert_at(&http, &base, &base, "token", "com.example")
            .await
            .unwrap();
        let error = edits.commit().await.unwrap_err();
        assert!(!is_play_rejection(&error), "{error:#}");
        let seen = server.await.unwrap();
        assert_eq!(seen.len(), 2);
        assert!(
            seen[1]
                .line
                .starts_with("POST /com.example/edits/e1:commit ")
        );
    }

    #[tokio::test]
    async fn bundles_stream_from_disk_and_retry_after_a_503() {
        let path =
            std::env::temp_dir().join(format!("oore-play-upload-{}.aab", std::process::id()));
        let size = 3 * 1024 * 1024 + 17;
        tokio::fs::write(&path, vec![7u8; size]).await.unwrap();
        let (base, server) = mock_play(vec![
            (200, r#"{"id":"e1"}"#),
            (503, "{}"),
            (200, r#"{"versionCode":42}"#),
        ])
        .await;
        let http = reqwest::Client::new();
        let edits = PlayEdits::insert_at(&http, &base, &base, "token", "com.example")
            .await
            .unwrap();
        assert_eq!(edits.upload_bundle(&path).await.unwrap(), 42);
        let seen = server.await.unwrap();
        for upload in &seen[1..] {
            assert!(
                upload
                    .line
                    .starts_with("POST /com.example/edits/e1/bundles?uploadType=media "),
                "{}",
                upload.line
            );
            assert!(
                upload.head.contains(&format!("content-length: {size}")),
                "{}",
                upload.head
            );
            assert_eq!(upload.body_len, size);
        }
        let _ = tokio::fs::remove_file(&path).await;
    }
}
