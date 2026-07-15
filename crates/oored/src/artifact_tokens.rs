//! Scoped download tokens for artifacts (OOR-140).
//!
//! Each token grants access to download a single artifact, with optional
//! single-use and time-limited expiry. Tokens are DB-backed (survive restarts).

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use oore_contract::{
    ApiError, ArtifactDownloadTokenSummary, CreateScopedDownloadTokenRequest,
    CreateScopedDownloadTokenResponse, ListArtifactDownloadTokensResponse,
    RevokeArtifactDownloadTokenResponse,
};
use sqlx::{Row, SqlitePool};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::store::write_audit_log;
use crate::token::{generate_token, hash_token};
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Default TTL for scoped download tokens: 24 hours.
const DEFAULT_TTL_SECS: i64 = 86_400;

/// Maximum TTL: 7 days.
const MAX_TTL_SECS: i64 = 604_800;

// ── DB helpers ──────────────────────────────────────────────────

async fn require_project_artifact_write(
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
    require_project_permission(&effective, ProjectPermission::WriteArtifacts)
}

async fn load_artifact_access(
    pool: &SqlitePool,
    artifact_id: &str,
) -> Result<(Option<i64>, String), (StatusCode, Json<ApiError>)> {
    let row = sqlx::query(
        "SELECT a.expires_at, b.project_id \
         FROM artifacts a \
         JOIN builds b ON b.id = a.build_id \
         WHERE a.id = ?1 AND a.state = 'available'",
    )
    .bind(artifact_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to fetch artifact project");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to fetch artifact",
        )
    })?
    .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Artifact not found"))?;

    Ok((row.get("expires_at"), row.get("project_id")))
}

pub async fn create_download_token(
    pool: &SqlitePool,
    artifact_id: &str,
    created_by: &str,
    ttl_secs: i64,
    single_use: bool,
) -> Result<(String, String, String, i64), sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    let token = generate_token();
    let hashed = hash_token(&token);
    let prefix = token[..8].to_string();
    let now = now_unix();
    let expires_at = now + ttl_secs;

    sqlx::query(
        "INSERT INTO artifact_download_tokens \
         (id, artifact_id, token_hash, prefix, created_by, expires_at, single_use, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&id)
    .bind(artifact_id)
    .bind(&hashed)
    .bind(&prefix)
    .bind(created_by)
    .bind(expires_at)
    .bind(single_use as i32)
    .bind(now)
    .execute(pool)
    .await?;

    Ok((id, token, prefix, expires_at))
}

/// Validated token data needed to serve the download.
pub struct ValidatedDownloadToken {
    pub token_hash: String,
    pub artifact_id: String,
    pub file_path: String,
    pub artifact_name: String,
    pub single_use: bool,
}

pub async fn validate_download_token(
    pool: &SqlitePool,
    raw_token: &str,
) -> Result<Option<ValidatedDownloadToken>, sqlx::Error> {
    let hashed = hash_token(raw_token);
    let now = now_unix();

    let row = sqlx::query(
        "SELECT t.token_hash, t.artifact_id, t.single_use, t.used_at, \
                a.file_path, a.name AS artifact_name, a.expires_at AS artifact_expires_at \
         FROM artifact_download_tokens t \
         JOIN artifacts a ON a.id = t.artifact_id \
         WHERE t.token_hash = ?1 \
           AND a.state = 'available' \
           AND t.revoked_at IS NULL \
           AND t.expires_at > ?2 \
           AND (t.single_use = 0 OR t.used_at IS NULL)",
    )
    .bind(&hashed)
    .bind(now)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| ValidatedDownloadToken {
        token_hash: r.get("token_hash"),
        artifact_id: r.get("artifact_id"),
        file_path: r.get("file_path"),
        artifact_name: r.get("artifact_name"),
        single_use: r.get::<i32, _>("single_use") != 0,
    }))
}

/// Atomically claim a single-use token.  Returns `true` if this call won the
/// race (i.e. `used_at` was `NULL` and is now set).  A concurrent second call
/// will see `rows_affected == 0` and return `false`.
pub async fn claim_single_use_token(pool: &SqlitePool, token_hash: &str) -> bool {
    let now = now_unix();
    match sqlx::query(
        "UPDATE artifact_download_tokens SET used_at = ?1 \
         WHERE token_hash = ?2 AND used_at IS NULL",
    )
    .bind(now)
    .bind(token_hash)
    .execute(pool)
    .await
    {
        Ok(result) => result.rows_affected() > 0,
        Err(e) => {
            error!(error = %e, "failed to mark download token as used");
            false
        }
    }
}

// ── Handlers ────────────────────────────────────────────────────

/// `POST /v1/artifacts/{artifact_id}/scoped-token` — create a scoped download token.
pub async fn create_scoped_token_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(artifact_id): Path<String>,
    Json(req): Json<CreateScopedDownloadTokenRequest>,
) -> ApiResult<CreateScopedDownloadTokenResponse> {
    let ttl = req.ttl_secs.unwrap_or(DEFAULT_TTL_SECS);
    if ttl < 60 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "TTL must be at least 60 seconds",
        ));
    }
    if ttl > MAX_TTL_SECS {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "TTL must not exceed 604800 seconds (7 days)",
        ));
    }

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let (artifact_expires_at, project_id) = load_artifact_access(&pool, &artifact_id).await?;
    require_project_artifact_write(&pool, &auth, &project_id).await?;
    if let Some(ea) = artifact_expires_at
        && ea <= now_unix()
    {
        return Err(api_err(
            StatusCode::GONE,
            "artifact_expired",
            "This artifact has expired",
        ));
    }

    let (id, token, prefix, expires_at) =
        create_download_token(&pool, &artifact_id, &auth.0.user_id, ttl, req.single_use)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to create scoped download token");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "db_error",
                    "Failed to create download token",
                )
            })?;

    // Build the download URL
    let public_url = state.public_url.read().await.clone();
    let base = public_url.as_deref().unwrap_or("http://127.0.0.1:8787");
    let download_url = format!("{}/install/artifact/{}", base.trim_end_matches('/'), &token);

    // Audit log
    let details = serde_json::json!({
        "artifact_id": artifact_id,
        "token_prefix": prefix,
        "single_use": req.single_use,
        "ttl_secs": ttl,
        "expires_at": expires_at,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "scoped_download_token_created",
        "artifact_download_token",
        Some(&id),
        Some(&details),
    )
    .await;

    info!(
        user_id = %auth.0.user_id,
        artifact_id = %artifact_id,
        token_id = %id,
        single_use = req.single_use,
        "scoped download token created"
    );

    Ok(Json(CreateScopedDownloadTokenResponse {
        id,
        download_url,
        token,
        prefix,
        expires_at,
        single_use: req.single_use,
    }))
}

/// `GET /v1/artifacts/{artifact_id}/scoped-tokens` — list scoped tokens for an artifact.
pub async fn list_scoped_tokens_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(artifact_id): Path<String>,
) -> ApiResult<ListArtifactDownloadTokensResponse> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    let (_, project_id) = load_artifact_access(&pool, &artifact_id).await?;
    require_project_artifact_write(&pool, &auth, &project_id).await?;
    let now = now_unix();

    let rows = sqlx::query(
        "SELECT t.id, t.artifact_id, t.prefix, t.created_by, t.expires_at, \
                t.single_use, t.used_at, t.revoked_at, t.created_at, \
                u.email AS created_by_email \
         FROM artifact_download_tokens t \
         JOIN users u ON u.id = t.created_by \
         WHERE t.artifact_id = ?1 \
         ORDER BY t.created_at DESC",
    )
    .bind(&artifact_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to list scoped download tokens");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "db_error",
            "Failed to list download tokens",
        )
    })?;

    let tokens: Vec<ArtifactDownloadTokenSummary> = rows
        .iter()
        .map(|r| {
            let expires_at: i64 = r.get("expires_at");
            let used_at: Option<i64> = r.get("used_at");
            let revoked_at: Option<i64> = r.get("revoked_at");
            let single_use = r.get::<i32, _>("single_use") != 0;

            ArtifactDownloadTokenSummary {
                id: r.get("id"),
                artifact_id: r.get("artifact_id"),
                prefix: r.get("prefix"),
                created_by: r.get("created_by"),
                created_by_email: r.get("created_by_email"),
                expires_at,
                single_use,
                used_at,
                revoked_at,
                is_expired: expires_at <= now,
                is_used: single_use && used_at.is_some(),
                is_revoked: revoked_at.is_some(),
                created_at: r.get("created_at"),
            }
        })
        .collect();

    Ok(Json(ListArtifactDownloadTokensResponse { tokens }))
}

/// `DELETE /v1/artifact-tokens/{token_id}` — revoke a scoped download token.
pub async fn revoke_scoped_token_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(token_id): Path<String>,
) -> ApiResult<RevokeArtifactDownloadTokenResponse> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    // Fetch the token to check existence
    let row = sqlx::query(
        "SELECT t.created_by, t.revoked_at, b.project_id \
         FROM artifact_download_tokens t \
         JOIN artifacts a ON a.id = t.artifact_id \
         JOIN builds b ON b.id = a.build_id \
         WHERE t.id = ?1",
    )
    .bind(&token_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to query download token");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "db_error",
            "Failed to query download token",
        )
    })?;

    let row = row.ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Download token not found",
        )
    })?;

    let project_id: String = row.get("project_id");
    require_project_artifact_write(&pool, &auth, &project_id).await?;

    let revoked_at: Option<i64> = row.get("revoked_at");
    if revoked_at.is_some() {
        return Ok(Json(RevokeArtifactDownloadTokenResponse { revoked: true }));
    }

    // Non-admin users can only revoke their own tokens
    let created_by: String = row.get("created_by");
    let is_admin = auth.0.role == "owner" || auth.0.role == "admin";
    if !is_admin && created_by != auth.0.user_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "permission_denied",
            "You can only revoke your own download tokens",
        ));
    }

    let now = now_unix();
    sqlx::query("UPDATE artifact_download_tokens SET revoked_at = ?1 WHERE id = ?2")
        .bind(now)
        .bind(&token_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to revoke download token");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_error",
                "Failed to revoke download token",
            )
        })?;

    // Audit log
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "scoped_download_token_revoked",
        "artifact_download_token",
        Some(&token_id),
        None,
    )
    .await;

    info!(
        user_id = %auth.0.user_id,
        token_id = %token_id,
        "scoped download token revoked"
    );

    Ok(Json(RevokeArtifactDownloadTokenResponse { revoked: true }))
}

/// `GET /install/artifact/{token}` — download an artifact via scoped token (no session auth).
pub async fn download_via_scoped_token(
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
            error!(error = %e, "failed to validate scoped download token");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_error",
                "Failed to validate download token",
            )
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                "Download token is invalid, expired, or already used",
            )
        })?;

    // Check if the artifact itself has expired
    let artifact_expires = sqlx::query("SELECT expires_at FROM artifacts WHERE id = ?1")
        .bind(&validated.artifact_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to check artifact expiry");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to check artifact",
            )
        })?;

    if let Some(row) = &artifact_expires {
        let expires_at: Option<i64> = row.get("expires_at");
        if let Some(ea) = expires_at
            && ea <= now_unix()
        {
            return Err(api_err(
                StatusCode::GONE,
                "artifact_expired",
                "This artifact has expired",
            ));
        }
    }

    // Atomically claim single-use tokens — reject if another request already consumed it
    if validated.single_use && !claim_single_use_token(&pool, &validated.token_hash).await {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_token",
            "Download token is invalid, expired, or already used",
        ));
    }

    // Serve the file via storage backend
    let network_settings =
        crate::instance_settings::load_effective_external_access_network_settings(&pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to load artifact delivery settings");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to load artifact delivery settings",
                )
            })?;
    let storage = state.storage.read().await;

    // S3/R2 returns a presigned object URL; local storage returns a second,
    // short-lived token URL on the artifact delivery origin.
    let download_url = storage
        .generate_download_url_with_base(
            &validated.file_path,
            900,
            network_settings
                .artifact_delivery_url
                .as_deref()
                .or(network_settings.public_url.as_deref()),
        )
        .await
        .map_err(|e| {
            error!(error = %e, "failed to generate download URL for scoped token");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "Failed to generate download URL",
            )
        })?;

    if let Some(url) = download_url {
        // For S3/R2 presigned URLs, redirect
        Ok(Redirect::temporary(&url).into_response())
    } else {
        // Storage is disabled
        Err(api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_not_configured",
            "Artifact storage backend is not configured",
        ))
    }
}
