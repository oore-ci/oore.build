use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiTokenSummary, CreateApiTokenRequest, CreateApiTokenResponse, ListApiTokensResponse,
    RevokeApiTokenResponse,
};
use sqlx::{Row, SqlitePool};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::rbac;
use crate::session::{AuthSource, SessionInfo};
use crate::store::write_audit_log;
use crate::token::{generate_token, hash_token};
use crate::util::{api_err, now_unix};

// ── Role hierarchy ───────────────────────────────────────────────

fn role_level(role: &str) -> u8 {
    match role {
        "owner" => 4,
        "admin" => 3,
        "developer" => 2,
        "qa_viewer" => 1,
        _ => 0,
    }
}

const VALID_ROLES: &[&str] = &["owner", "admin", "developer", "qa_viewer"];

// ── DB helpers ───────────────────────────────────────────────────

pub async fn create_api_token(
    pool: &SqlitePool,
    created_by: &str,
    name: &str,
    role: &str,
    expires_at: Option<i64>,
) -> Result<(String, String, String, i64), sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    let token = generate_token();
    let hashed = hash_token(&token);
    let prefix = token[..8].to_string();
    let now = now_unix();

    sqlx::query(
        "INSERT INTO api_tokens (id, name, token_hash, prefix, created_by, role, expires_at, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&id)
    .bind(name)
    .bind(&hashed)
    .bind(&prefix)
    .bind(created_by)
    .bind(role)
    .bind(expires_at)
    .bind(now)
    .execute(pool)
    .await?;

    Ok((id, token, prefix, now))
}

pub async fn validate_api_token(
    pool: &SqlitePool,
    token: &str,
) -> Result<Option<SessionInfo>, sqlx::Error> {
    let hashed = hash_token(token);
    let now = now_unix();

    let row = sqlx::query(
        "SELECT t.id, t.role, t.expires_at AS token_expires_at, \
                u.id AS user_id, u.email, u.oidc_subject \
         FROM api_tokens t \
         JOIN users u ON u.id = t.created_by \
         WHERE t.token_hash = ?1 \
           AND t.revoked_at IS NULL \
           AND (t.expires_at IS NULL OR t.expires_at > ?2) \
           AND u.status = 'active'",
    )
    .bind(&hashed)
    .bind(now)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        let token_expires_at: Option<i64> = r.get("token_expires_at");
        SessionInfo {
            user_id: r.get("user_id"),
            email: r.get("email"),
            oidc_subject: r.get("oidc_subject"),
            role: r.get("role"),
            expires_at: token_expires_at.unwrap_or(i64::MAX),
            auth_source: AuthSource::ApiToken,
        }
    }))
}

pub async fn update_last_used(pool: &SqlitePool, token_hash: &str) {
    let now = now_unix();
    if let Err(e) = sqlx::query("UPDATE api_tokens SET last_used_at = ?1 WHERE token_hash = ?2")
        .bind(now)
        .bind(token_hash)
        .execute(pool)
        .await
    {
        error!(error = %e, "failed to update api_token last_used_at");
    }
}

// ── Handlers ─────────────────────────────────────────────────────

pub async fn list_api_tokens_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<ListApiTokensResponse>, (StatusCode, Json<oore_contract::ApiError>)> {
    rbac::check_permission(&state.enforcer, &auth.0.role, "api_tokens", "read").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    let now = now_unix();

    let is_admin = auth.0.role == "owner" || auth.0.role == "admin";

    let rows = if is_admin {
        sqlx::query(
            "SELECT t.id, t.name, t.prefix, t.role, t.created_by, t.created_at, \
                    t.expires_at, t.last_used_at, t.revoked_at, u.email AS created_by_email \
             FROM api_tokens t \
             JOIN users u ON u.id = t.created_by \
             ORDER BY t.created_at DESC",
        )
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            "SELECT t.id, t.name, t.prefix, t.role, t.created_by, t.created_at, \
                    t.expires_at, t.last_used_at, t.revoked_at, u.email AS created_by_email \
             FROM api_tokens t \
             JOIN users u ON u.id = t.created_by \
             WHERE t.created_by = ?1 \
             ORDER BY t.created_at DESC",
        )
        .bind(&auth.0.user_id)
        .fetch_all(&pool)
        .await
    };

    let rows = rows.map_err(|e| {
        error!(error = %e, "failed to list api tokens");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "db_error",
            "Failed to list API tokens",
        )
    })?;

    let tokens: Vec<ApiTokenSummary> = rows
        .iter()
        .map(|r| {
            let expires_at: Option<i64> = r.get("expires_at");
            let revoked_at: Option<i64> = r.get("revoked_at");
            ApiTokenSummary {
                id: r.get("id"),
                name: r.get("name"),
                prefix: r.get("prefix"),
                role: r.get("role"),
                created_by: r.get("created_by"),
                created_by_email: r.get("created_by_email"),
                created_at: r.get("created_at"),
                expires_at,
                last_used_at: r.get("last_used_at"),
                is_expired: expires_at.is_some_and(|ea| ea <= now),
                is_revoked: revoked_at.is_some(),
            }
        })
        .collect();

    Ok(Json(ListApiTokensResponse { tokens }))
}

pub async fn create_api_token_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateApiTokenRequest>,
) -> Result<Json<CreateApiTokenResponse>, (StatusCode, Json<oore_contract::ApiError>)> {
    rbac::check_permission(&state.enforcer, &auth.0.role, "api_tokens", "write").await?;

    // Validate name
    let name = req.name.trim();
    if name.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "Token name must not be empty",
        ));
    }

    // Validate role
    if !VALID_ROLES.contains(&req.role.as_str()) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_role",
            format!(
                "Invalid role '{}'. Must be one of: owner, admin, developer, qa_viewer",
                req.role
            ),
        ));
    }

    // Cannot create a token with a higher role than your own
    if role_level(&req.role) > role_level(&auth.0.role) {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "role_escalation",
            "Cannot create a token with a higher role than your own",
        ));
    }

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let (id, token, prefix, created_at) =
        create_api_token(&pool, &auth.0.user_id, name, &req.role, req.expires_at)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to create api token");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "db_error",
                    "Failed to create API token",
                )
            })?;

    // Audit log
    let details = format!("name={}, role={}, prefix={}", name, req.role, prefix);
    if let Err(e) = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "api_token_created",
        "api_token",
        Some(&id),
        Some(&details),
    )
    .await
    {
        error!(error = %e, "failed to write audit log for api token creation");
    }

    info!(
        user_id = %auth.0.user_id,
        token_id = %id,
        role = %req.role,
        "API token created"
    );

    Ok(Json(CreateApiTokenResponse {
        id,
        name: name.to_string(),
        prefix,
        role: req.role,
        created_at,
        expires_at: req.expires_at,
        token,
    }))
}

pub async fn revoke_api_token_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(token_id): Path<String>,
) -> Result<Json<RevokeApiTokenResponse>, (StatusCode, Json<oore_contract::ApiError>)> {
    rbac::check_permission(&state.enforcer, &auth.0.role, "api_tokens", "delete").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    // Fetch the token to check ownership
    let row = sqlx::query("SELECT created_by, revoked_at FROM api_tokens WHERE id = ?1")
        .bind(&token_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to query api token");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_error",
                "Failed to query API token",
            )
        })?;

    let row =
        row.ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "API token not found"))?;

    let created_by: String = row.get("created_by");
    let revoked_at: Option<i64> = row.get("revoked_at");

    if revoked_at.is_some() {
        return Ok(Json(RevokeApiTokenResponse { revoked: true }));
    }

    // Non-admin users can only revoke their own tokens
    let is_admin = auth.0.role == "owner" || auth.0.role == "admin";
    if !is_admin && created_by != auth.0.user_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "permission_denied",
            "You can only revoke your own API tokens",
        ));
    }

    let now = now_unix();
    sqlx::query("UPDATE api_tokens SET revoked_at = ?1 WHERE id = ?2")
        .bind(now)
        .bind(&token_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to revoke api token");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_error",
                "Failed to revoke API token",
            )
        })?;

    // Audit log
    if let Err(e) = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "api_token_revoked",
        "api_token",
        Some(&token_id),
        None,
    )
    .await
    {
        error!(error = %e, "failed to write audit log for api token revocation");
    }

    info!(
        user_id = %auth.0.user_id,
        token_id = %token_id,
        "API token revoked"
    );

    Ok(Json(RevokeApiTokenResponse { revoked: true }))
}
