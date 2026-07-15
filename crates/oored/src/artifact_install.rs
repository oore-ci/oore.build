//! Device installation links for Android APK and signed iOS ad-hoc artifacts.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use oore_contract::{ApiError, ArtifactInstallLinkResponse, ArtifactInstallPlatform};
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use tracing::{error, info};
use url::Url;

use crate::AppState;
use crate::artifact_tokens::{create_download_token, validate_download_token};
use crate::extractors::AuthUser;
use crate::instance_settings::{
    load_effective_external_access_network_settings, load_warpgate_install_ticket,
};
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

const INSTALL_TOKEN_TTL_SECS: i64 = 60 * 60;

struct InstallArtifact {
    id: String,
    artifact_type: String,
    metadata: Value,
    expires_at: Option<i64>,
    project_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IosManifestMetadata {
    bundle_identifier: String,
    display_name: String,
    version: String,
    build_number: String,
}

async fn load_install_artifact(
    pool: &SqlitePool,
    artifact_id: &str,
) -> Result<InstallArtifact, (StatusCode, Json<ApiError>)> {
    let row = sqlx::query(
        "SELECT a.id, a.artifact_type, a.metadata, a.expires_at, b.project_id \
         FROM artifacts a \
         JOIN builds b ON b.id = a.build_id \
         WHERE a.id = ?1 AND a.state = 'available'",
    )
    .bind(artifact_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to load artifact for installation");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load artifact",
        )
    })?
    .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Artifact not found"))?;

    let metadata_raw: String = row.get("metadata");
    Ok(InstallArtifact {
        id: row.get("id"),
        artifact_type: row.get("artifact_type"),
        metadata: serde_json::from_str(&metadata_raw).unwrap_or_default(),
        expires_at: row.get("expires_at"),
        project_id: row.get("project_id"),
    })
}

async fn require_artifact_read(
    pool: &SqlitePool,
    auth: &AuthUser,
    project_id: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let effective = resolve_effective_project_role(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::ReadArtifacts)
}

fn required_metadata_string(object: &Value, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_ios_manifest_metadata(metadata: &Value) -> Result<IosManifestMetadata, &'static str> {
    let signing = metadata
        .get("ios_signing")
        .ok_or("Signed iOS metadata is missing")?;
    let export_method = required_metadata_string(signing, "effective_export_method")
        .ok_or("iOS export method is missing")?;
    if !matches!(export_method.as_str(), "ad-hoc" | "release-testing") {
        return Err("The IPA was not exported for ad-hoc device installation");
    }

    let app = metadata
        .get("ios_app")
        .ok_or("App identity metadata is missing; rebuild this IPA with the current runner")?;
    let bundle_identifier = required_metadata_string(app, "bundle_identifier")
        .ok_or("iOS bundle identifier is missing")?;
    let display_name =
        required_metadata_string(app, "display_name").ok_or("iOS app name is missing")?;
    let version = required_metadata_string(app, "version").ok_or("iOS app version is missing")?;
    let build_number =
        required_metadata_string(app, "build_number").ok_or("iOS build number is missing")?;

    let profile_matches = signing
        .get("bundle_ids")
        .and_then(Value::as_array)
        .is_some_and(|bundle_ids| {
            bundle_ids
                .iter()
                .filter_map(Value::as_str)
                .any(|value| value == bundle_identifier)
        });
    if !profile_matches {
        return Err("The app bundle identifier is not covered by the signing profiles");
    }

    Ok(IosManifestMetadata {
        bundle_identifier,
        display_name,
        version,
        build_number,
    })
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn ios_manifest(metadata: &IosManifestMetadata, download_url: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>items</key>
  <array>
    <dict>
      <key>assets</key>
      <array>
        <dict>
          <key>kind</key>
          <string>software-package</string>
          <key>url</key>
          <string>{}</string>
        </dict>
      </array>
      <key>metadata</key>
      <dict>
        <key>bundle-identifier</key>
        <string>{}</string>
        <key>bundle-version</key>
        <string>{}</string>
        <key>kind</key>
        <string>software</string>
        <key>subtitle</key>
        <string>Version {} ({})</string>
        <key>title</key>
        <string>{}</string>
      </dict>
    </dict>
  </array>
</dict>
</plist>
"#,
        escape_xml(download_url),
        escape_xml(&metadata.bundle_identifier),
        escape_xml(&metadata.build_number),
        escape_xml(&metadata.version),
        escape_xml(&metadata.build_number),
        escape_xml(&metadata.display_name),
    )
}

fn public_base_url(
    public_url: Option<&str>,
    require_https: bool,
) -> Result<Url, (StatusCode, Json<ApiError>)> {
    let raw = public_url.ok_or_else(|| {
        api_err(
            StatusCode::PRECONDITION_FAILED,
            "external_access_required",
            "Configure an Artifact delivery URL or External Access public URL before installing artifacts on a device",
        )
    })?;
    let parsed = Url::parse(raw).map_err(|_| {
        api_err(
            StatusCode::PRECONDITION_FAILED,
            "external_access_invalid",
            "The configured External Access public URL is invalid",
        )
    })?;
    if require_https && parsed.scheme() != "https" {
        return Err(api_err(
            StatusCode::PRECONDITION_FAILED,
            "ios_install_requires_https",
            "iOS over-the-air installation requires an HTTPS public URL",
        ));
    }
    Ok(parsed)
}

async fn artifact_delivery_base_url(
    pool: &SqlitePool,
    require_https: bool,
) -> Result<Url, (StatusCode, Json<ApiError>)> {
    let settings = load_effective_external_access_network_settings(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load artifact delivery URL");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load artifact delivery settings",
            )
        })?;
    public_base_url(
        settings
            .artifact_delivery_url
            .as_deref()
            .or(settings.public_url.as_deref()),
        require_https,
    )
}

fn public_endpoint(base: &Url, path: &str, warpgate_ticket: Option<&str>) -> String {
    let mut endpoint = base.clone();
    endpoint.set_path(&format!("/{}", path.trim_start_matches('/')));
    endpoint.set_query(None);
    endpoint.set_fragment(None);
    if let Some(ticket) = warpgate_ticket {
        endpoint
            .query_pairs_mut()
            .append_pair("warpgate-ticket", ticket);
    }
    endpoint.into()
}

/// `POST /v1/artifacts/{artifact_id}/install-link` — create a device install session.
pub async fn create_install_link(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(artifact_id): Path<String>,
) -> ApiResult<ArtifactInstallLinkResponse> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    let artifact = load_install_artifact(&pool, &artifact_id).await?;
    require_artifact_read(&pool, &auth, &artifact.project_id).await?;

    let now = now_unix();
    if artifact
        .expires_at
        .is_some_and(|expires_at| expires_at <= now)
    {
        return Err(api_err(
            StatusCode::GONE,
            "artifact_expired",
            "This artifact has expired",
        ));
    }

    let platform = match artifact.artifact_type.as_str() {
        "apk" => ArtifactInstallPlatform::Android,
        "ipa" => {
            parse_ios_manifest_metadata(&artifact.metadata).map_err(|message| {
                api_err(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "ios_artifact_not_installable",
                    message,
                )
            })?;
            ArtifactInstallPlatform::Ios
        }
        _ => {
            return Err(api_err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "artifact_not_installable",
                "Only APK and signed ad-hoc IPA artifacts can be installed on a device",
            ));
        }
    };

    let base =
        artifact_delivery_base_url(&pool, matches!(platform, ArtifactInstallPlatform::Ios)).await?;
    let warpgate_ticket = if matches!(platform, ArtifactInstallPlatform::Ios) {
        load_warpgate_install_ticket(&pool, &state.encryption_key)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to load Warpgate install ticket");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "warpgate_ticket_error",
                    "Failed to load Warpgate install configuration",
                )
            })?
    } else {
        None
    };
    let ttl_secs = artifact
        .expires_at
        .map(|expires_at| (expires_at - now).min(INSTALL_TOKEN_TTL_SECS))
        .unwrap_or(INSTALL_TOKEN_TTL_SECS);
    let (token_id, token, prefix, expires_at) =
        create_download_token(&pool, &artifact.id, &auth.0.user_id, ttl_secs, false)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to create artifact install token");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "db_error",
                    "Failed to create install link",
                )
            })?;

    let download_url = public_endpoint(
        &base,
        &format!("install/artifact/{token}"),
        warpgate_ticket.as_deref(),
    );
    let (install_url, manifest_url) = match platform {
        ArtifactInstallPlatform::Android => (download_url.clone(), None),
        ArtifactInstallPlatform::Ios => {
            let manifest_url = public_endpoint(
                &base,
                &format!("install/ios/{token}/manifest.plist"),
                warpgate_ticket.as_deref(),
            );
            (
                format!(
                    "itms-services://?action=download-manifest&url={}",
                    urlencoding::encode(&manifest_url)
                ),
                Some(manifest_url),
            )
        }
    };

    let details = serde_json::json!({
        "artifact_id": artifact.id,
        "platform": match platform {
            ArtifactInstallPlatform::Android => "android",
            ArtifactInstallPlatform::Ios => "ios",
        },
        "token_prefix": prefix,
        "expires_at": expires_at,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "artifact_install_link_generated",
        "artifact_download_token",
        Some(&token_id),
        Some(&details),
    )
    .await;

    info!(
        artifact_id = %artifact_id,
        user_id = %auth.0.user_id,
        "artifact install link generated"
    );

    Ok(Json(ArtifactInstallLinkResponse {
        platform,
        install_url,
        download_url,
        manifest_url,
        expires_at,
    }))
}

/// `GET /install/ios/{token}/manifest.plist` — serve an Apple OTA manifest.
pub async fn ios_install_manifest(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    let validated = validate_download_token(&pool, &token)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to validate iOS install token");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_error",
                "Failed to validate install token",
            )
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                "Install token is invalid, expired, or already used",
            )
        })?;

    let artifact = load_install_artifact(&pool, &validated.artifact_id).await?;
    if artifact
        .expires_at
        .is_some_and(|expires_at| expires_at <= now_unix())
    {
        return Err(api_err(
            StatusCode::GONE,
            "artifact_expired",
            "This artifact has expired",
        ));
    }
    if artifact.artifact_type != "ipa" {
        return Err(api_err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "artifact_not_installable",
            "This install token does not reference an IPA artifact",
        ));
    }
    let metadata = parse_ios_manifest_metadata(&artifact.metadata).map_err(|message| {
        api_err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "ios_artifact_not_installable",
            message,
        )
    })?;
    let base = artifact_delivery_base_url(&pool, true).await?;
    let warpgate_ticket = load_warpgate_install_ticket(&pool, &state.encryption_key)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load Warpgate install ticket");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "warpgate_ticket_error",
                "Failed to load Warpgate install configuration",
            )
        })?;
    let download_url = public_endpoint(
        &base,
        &format!("install/artifact/{token}"),
        warpgate_ticket.as_deref(),
    );
    let manifest = ios_manifest(&metadata, &download_url);

    let mut response = (StatusCode::OK, manifest).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install_metadata() -> Value {
        serde_json::json!({
            "ios_app": {
                "bundle_identifier": "com.example.kite",
                "display_name": "Kite & QA <adhoc>",
                "version": "3.2.1",
                "build_number": "42"
            },
            "ios_signing": {
                "bundle_ids": ["com.example.kite"],
                "effective_export_method": "release-testing"
            }
        })
    }

    #[test]
    fn accepts_complete_ad_hoc_metadata() {
        let parsed = parse_ios_manifest_metadata(&install_metadata()).expect("metadata");
        assert_eq!(parsed.bundle_identifier, "com.example.kite");
        assert_eq!(parsed.version, "3.2.1");
        assert_eq!(parsed.build_number, "42");
    }

    #[test]
    fn rejects_ipa_without_current_app_metadata() {
        let metadata = serde_json::json!({
            "ios_signing": {
                "bundle_ids": ["com.example.kite"],
                "effective_export_method": "ad-hoc"
            }
        });
        assert_eq!(
            parse_ios_manifest_metadata(&metadata).unwrap_err(),
            "App identity metadata is missing; rebuild this IPA with the current runner"
        );
    }

    #[test]
    fn manifest_escapes_values_and_keeps_version_metadata() {
        let metadata = parse_ios_manifest_metadata(&install_metadata()).expect("metadata");
        let manifest = ios_manifest(
            &metadata,
            "https://ci.example.com/install/artifact/token?a=1&b=2",
        );
        assert!(manifest.contains("Kite &amp; QA &lt;adhoc&gt;"));
        assert!(manifest.contains("Version 3.2.1 (42)"));
        assert!(manifest.contains("token?a=1&amp;b=2"));
        assert!(manifest.contains("com.example.kite"));
    }

    #[test]
    fn ios_public_url_must_use_https() {
        let error = public_base_url(Some("http://ci.example.com"), true).unwrap_err();
        assert_eq!(error.0, StatusCode::PRECONDITION_FAILED);
        assert!(public_base_url(Some("https://ci.example.com"), true).is_ok());
    }
}
