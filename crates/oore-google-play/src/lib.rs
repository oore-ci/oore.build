//! Google Play Developer API client shared by the runner and the daemon.
//!
//! Covers service account authentication and the Edits flow: insert an edit,
//! upload a bundle and its deobfuscation file, set a track release, commit.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

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
        .unwrap_or_else(|| body.chars().take(500).collect())
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
}

/// Send `request` and parse its JSON reply. A failure names `action`, the
/// HTTP status and Google's own error message, plus a hint on 401 and 403.
pub async fn play_json<T: serde::de::DeserializeOwned>(
    request: reqwest::RequestBuilder,
    action: &str,
) -> anyhow::Result<T> {
    let response = request
        .send()
        .await
        .with_context(|| format!("failed to reach Google Play to {action}"))?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        bail!(
            "Google Play could not {action} ({status}): {}{}",
            google_error_message(&body),
            play_access_hint(status)
        );
    }
    serde_json::from_str(if body.is_empty() { "null" } else { &body })
        .with_context(|| format!("unexpected Google Play response to {action}"))
}

impl<'a> PlayEdits<'a> {
    pub async fn insert(
        http: &'a reqwest::Client,
        token: &str,
        package_name: &str,
    ) -> anyhow::Result<Self> {
        #[derive(Deserialize)]
        struct Edit {
            id: String,
        }
        let edit: Edit = play_json(
            http.post(format!("{API_BASE}/{package_name}/edits"))
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
        })
    }

    pub fn edit_id(&self) -> &str {
        &self.edit_id
    }

    fn api(&self, path: &str) -> String {
        format!(
            "{API_BASE}/{}/edits/{}{path}",
            self.package_name, self.edit_id
        )
    }

    fn upload(&self, path: &str) -> String {
        format!(
            "{UPLOAD_BASE}/{}/edits/{}{path}?uploadType=media",
            self.package_name, self.edit_id
        )
    }

    /// Upload an AAB and return the version code Play assigned to it.
    pub async fn upload_bundle(&self, path: &Path) -> anyhow::Result<i64> {
        let bytes = tokio_read(path).await?;
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Bundle {
            version_code: i64,
        }
        let bundle: Bundle = play_json(
            self.http
                .post(self.upload("/bundles"))
                .bearer_auth(&self.token)
                .header("content-type", "application/octet-stream")
                .body(bytes),
            "upload the bundle",
        )
        .await?;
        Ok(bundle.version_code)
    }

    /// Upload R8/ProGuard `mapping.txt` for a version code.
    pub async fn upload_mapping(&self, version_code: i64, path: &Path) -> anyhow::Result<()> {
        let bytes = tokio_read(path).await?;
        let _: serde_json::Value = play_json(
            self.http
                .post(self.upload(&format!("/apks/{version_code}/deobfuscationFiles/proguard")))
                .bearer_auth(&self.token)
                .header("content-type", "application/octet-stream")
                .body(bytes),
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

    pub async fn commit(self) -> anyhow::Result<()> {
        let _: serde_json::Value = play_json(
            self.http.post(self.api(":commit")).bearer_auth(&self.token),
            "commit the edit",
        )
        .await?;
        Ok(())
    }

    /// Discard the edit. Best effort: an uncommitted edit also expires on its own.
    pub async fn delete(self) -> anyhow::Result<()> {
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

async fn tokio_read(path: &Path) -> anyhow::Result<Vec<u8>> {
    tokio::fs::read(path)
        .await
        .with_context(|| format!("failed to read {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
