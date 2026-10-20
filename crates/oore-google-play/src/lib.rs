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
    let key: ServiceAccountKey = serde_json::from_str(service_account_json)
        .context("the service account secret is not a Google service account JSON key")?;
    let token_uri = key.token_uri.as_deref().unwrap_or(DEFAULT_TOKEN_URI);
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let claims = JwtClaims {
        iss: &key.client_email,
        scope: PUBLISHER_SCOPE,
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

async fn play_json<T: serde::de::DeserializeOwned>(
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
            "Google Play could not {action} ({status}): {}",
            google_error_message(&body)
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
