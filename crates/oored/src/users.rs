use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, InviteUserRequest, InviteUserResponse, ListUsersResponse, ReEnableUserResponse,
    UpdateUserRoleRequest, UpdateUserRoleResponse, User, UserProfileResponse,
};
use sqlx::Row;
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

// ── Helpers ──────────────────────────────────────────────────────

fn row_to_user(row: &sqlx::sqlite::SqliteRow) -> User {
    User {
        id: row.get("id"),
        email: row.get("email"),
        display_name: row.get("display_name"),
        role: row.get("role"),
        status: row.get("status"),
        avatar_url: row.get("avatar_url"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

async fn fetch_user_by_id(
    state: &AppState,
    user_id: &str,
) -> Result<User, (StatusCode, Json<ApiError>)> {
    let store = state.store.lock().await;
    let pool = store.pool();

    let row = sqlx::query(
        "SELECT id, email, display_name, role, status, avatar_url, created_at, updated_at \
         FROM users WHERE id = ?1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to fetch user");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to fetch user",
        )
    })?
    .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "user_not_found", "User not found"))?;

    Ok(row_to_user(&row))
}

// ── Handlers ─────────────────────────────────────────────────────

/// `GET /v1/users/me` — current user profile.
pub async fn get_me(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<UserProfileResponse> {
    let user = fetch_user_by_id(&state, &auth.0.user_id).await?;
    Ok(Json(UserProfileResponse { user }))
}

/// `GET /v1/users` — list all users (owner/admin only).
pub async fn list_users(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<ListUsersResponse> {
    check_permission(&state.enforcer, &auth.0.role, "users", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let rows = sqlx::query(
        "SELECT id, email, display_name, role, status, avatar_url, created_at, updated_at \
         FROM users ORDER BY created_at ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to list users");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to list users",
        )
    })?;

    let users = rows.iter().map(row_to_user).collect();

    Ok(Json(ListUsersResponse { users }))
}

/// `POST /v1/users/invite` — invite a user (owner/admin only).
pub async fn invite_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<InviteUserRequest>,
) -> ApiResult<InviteUserResponse> {
    check_permission(&state.enforcer, &auth.0.role, "users", "invite").await?;

    // Validate email
    if req.email.is_empty() || !req.email.contains('@') || req.email.len() > 256 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_email",
            "Invalid email address",
        ));
    }

    // Validate role — cannot invite as owner
    let role = req.role.as_str();
    if !matches!(role, "admin" | "developer" | "qa_viewer") {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_role",
            "Role must be admin, developer, or qa_viewer",
        ));
    }

    let store = state.store.lock().await;
    let pool = store.pool();

    // Check for duplicate email
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = ?1")
        .bind(&req.email)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to check duplicate email");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to check user",
            )
        })?;

    if count > 0 {
        return Err(api_err(
            StatusCode::CONFLICT,
            "email_exists",
            "A user with this email already exists",
        ));
    }

    let user_id = Uuid::new_v4().to_string();
    let now = now_unix();

    sqlx::query(
        "INSERT INTO users (id, email, oidc_subject, display_name, role, status, invited_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'invited', ?6, ?7, ?7)",
    )
    .bind(&user_id)
    .bind(&req.email)
    .bind(&format!("pending-{user_id}")) // placeholder oidc_subject until first login
    .bind(&req.email)
    .bind(role)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to insert invited user");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to create user")
    })?;

    let details = serde_json::json!({
        "email": req.email,
        "role": role,
        "invited_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "user_invited",
        "user",
        Some(&user_id),
        Some(&details),
    )
    .await;

    info!(email = %req.email, role = %role, invited_by = %auth.0.email, "user invited");

    let user = User {
        id: user_id,
        email: req.email,
        display_name: None,
        role: role.to_string(),
        status: "invited".to_string(),
        avatar_url: None,
        created_at: now,
        updated_at: now,
    };

    Ok(Json(InviteUserResponse { user }))
}

/// `PATCH /v1/users/{user_id}/role` — change a user's role (owner/admin only).
pub async fn update_user_role(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(user_id): Path<String>,
    Json(req): Json<UpdateUserRoleRequest>,
) -> ApiResult<UpdateUserRoleResponse> {
    check_permission(&state.enforcer, &auth.0.role, "users", "write").await?;

    let role = req.role.as_str();
    if !matches!(role, "admin" | "developer" | "qa_viewer") {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_role",
            "Role must be admin, developer, or qa_viewer",
        ));
    }

    // Only the owner can promote users to admin
    if role == "admin" {
        auth.require_owner()?;
    }

    let store = state.store.lock().await;
    let pool = store.pool();

    // Check target user exists and isn't the owner
    let target_role: Option<String> = sqlx::query_scalar("SELECT role FROM users WHERE id = ?1")
        .bind(&user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to look up user");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to look up user",
            )
        })?;

    let current_role = target_role
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "user_not_found", "User not found"))?;

    if current_role == "owner" {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "cannot_change_owner",
            "The owner role cannot be changed",
        ));
    }

    // Only the owner can modify admin users
    if current_role == "admin" {
        auth.require_owner()?;
    }

    let now = now_unix();
    sqlx::query("UPDATE users SET role = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(role)
        .bind(now)
        .bind(&user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to update user role");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to update user role",
            )
        })?;

    let details = serde_json::json!({
        "from_role": current_role,
        "to_role": role,
        "changed_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "role_changed",
        "user",
        Some(&user_id),
        Some(&details),
    )
    .await;

    info!(user_id = %user_id, from = %current_role, to = %role, "user role changed");
    drop(store);

    let user = fetch_user_by_id(&state, &user_id).await?;
    Ok(Json(UpdateUserRoleResponse { user }))
}

/// `DELETE /v1/users/{user_id}` — soft-delete (disable) a user (owner/admin only).
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    check_permission(&state.enforcer, &auth.0.role, "users", "delete").await?;

    // Cannot delete yourself
    if auth.0.user_id == user_id {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "cannot_delete_self",
            "You cannot disable your own account",
        ));
    }

    let store = state.store.lock().await;
    let pool = store.pool();

    // Check target user exists and isn't the owner
    let target_role: Option<String> = sqlx::query_scalar("SELECT role FROM users WHERE id = ?1")
        .bind(&user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to look up user");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to look up user",
            )
        })?;

    let current_role = target_role
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "user_not_found", "User not found"))?;

    if current_role == "owner" {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "cannot_delete_owner",
            "The owner account cannot be disabled",
        ));
    }

    // Only the owner can disable admin users
    if current_role == "admin" {
        auth.require_owner()?;
    }

    let now = now_unix();
    sqlx::query("UPDATE users SET status = 'disabled', updated_at = ?1 WHERE id = ?2")
        .bind(now)
        .bind(&user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to disable user");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to disable user",
            )
        })?;

    // Cascade: revoke all sessions for this user
    state
        .sessions
        .revoke_user_sessions(&user_id)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to revoke user sessions");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_error",
                "Failed to revoke sessions",
            )
        })?;

    let details = serde_json::json!({
        "disabled_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "user_disabled",
        "user",
        Some(&user_id),
        Some(&details),
    )
    .await;

    info!(user_id = %user_id, disabled_by = %auth.0.email, "user disabled");

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `POST /v1/users/{user_id}/enable` — re-enable a disabled user (owner/admin only).
pub async fn re_enable_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(user_id): Path<String>,
) -> ApiResult<ReEnableUserResponse> {
    check_permission(&state.enforcer, &auth.0.role, "users", "enable").await?;

    // Cannot enable yourself
    if auth.0.user_id == user_id {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "cannot_enable_self",
            "You cannot re-enable your own account",
        ));
    }

    let store = state.store.lock().await;
    let pool = store.pool();

    // Check target user exists, get role and status
    let row = sqlx::query("SELECT role, status FROM users WHERE id = ?1")
        .bind(&user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to look up user");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to look up user",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "user_not_found", "User not found"))?;

    let current_role: String = row.get("role");
    let current_status: String = row.get("status");

    // Cannot enable the owner
    if current_role == "owner" {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "cannot_enable_owner",
            "The owner account cannot be re-enabled this way",
        ));
    }

    // Only owner can re-enable admin users
    if current_role == "admin" {
        auth.require_owner()?;
    }

    // Target must be disabled
    if current_status != "disabled" {
        return Err(api_err(
            StatusCode::CONFLICT,
            "not_disabled",
            "User is not currently disabled",
        ));
    }

    let now = now_unix();
    sqlx::query("UPDATE users SET status = 'active', updated_at = ?1 WHERE id = ?2")
        .bind(now)
        .bind(&user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to enable user");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to enable user",
            )
        })?;

    let details = serde_json::json!({
        "enabled_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "user_enabled",
        "user",
        Some(&user_id),
        Some(&details),
    )
    .await;

    info!(user_id = %user_id, enabled_by = %auth.0.email, "user re-enabled");
    drop(store);

    let user = fetch_user_by_id(&state, &user_id).await?;
    Ok(Json(ReEnableUserResponse { user }))
}
