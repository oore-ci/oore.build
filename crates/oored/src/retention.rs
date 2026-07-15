//! Build retention policy settings and cleanup endpoints.

use std::str::FromStr;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, EffectiveProjectRetentionResponse, ProjectRetentionOverride, RetentionCleanupSummary,
    RetentionCleanupSummaryResponse, RetentionCleanupTarget, RetentionPolicy,
    RetentionPolicyResponse, UpdateProjectRetentionOverrideRequest, UpdateRetentionPolicyRequest,
};
use sqlx::Row;
use tracing::error;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;
const RETENTION_PERMISSION_RESOURCE: &str = "instance_settings";

// ── Helpers ──────────────────────────────────────────────────────

fn default_policy() -> RetentionPolicy {
    RetentionPolicy {
        enabled: false,
        max_age_days: None,
        max_builds_per_project: None,
        max_artifact_size_bytes: None,
        cleanup_target: RetentionCleanupTarget::ArtifactsOnly,
        keep_statuses: Vec::new(),
        dry_run: false,
        cleanup_interval_secs: 3600,
        artifact_ttl_days: None,
        updated_at: None,
    }
}

pub async fn load_global_policy(pool: &sqlx::SqlitePool) -> Result<RetentionPolicy, sqlx::Error> {
    let row = sqlx::query("SELECT * FROM retention_policy WHERE id = 1")
        .fetch_optional(pool)
        .await?;

    let Some(row) = row else {
        return Ok(default_policy());
    };

    Ok(RetentionPolicy {
        enabled: row.get::<i32, _>("enabled") != 0,
        max_age_days: row.get("max_age_days"),
        max_builds_per_project: row.get("max_builds_per_project"),
        max_artifact_size_bytes: row.get("max_artifact_size_bytes"),
        cleanup_target: RetentionCleanupTarget::from_str(row.get::<&str, _>("cleanup_target"))
            .unwrap_or(RetentionCleanupTarget::ArtifactsOnly),
        keep_statuses: serde_json::from_str(row.get::<&str, _>("keep_statuses"))
            .unwrap_or_default(),
        dry_run: row.get::<i32, _>("dry_run") != 0,
        cleanup_interval_secs: row.get("cleanup_interval_secs"),
        artifact_ttl_days: row.get("artifact_ttl_days"),
        updated_at: Some(row.get::<i64, _>("updated_at")),
    })
}

/// Load the effective retention policy for a project (global merged with project override).
///
/// Used by artifact creation to compute per-artifact expiry and by the background
/// cleanup task.  Falls back to the global policy when no project override exists.
pub async fn load_effective_policy(
    pool: &sqlx::SqlitePool,
    project_id: &str,
) -> Result<RetentionPolicy, sqlx::Error> {
    let global = load_global_policy(pool).await?;

    let row = sqlx::query("SELECT * FROM project_retention_overrides WHERE project_id = ?1")
        .bind(project_id)
        .fetch_optional(pool)
        .await?;

    let Some(row) = row else {
        return Ok(global);
    };

    let ovr = ProjectRetentionOverride {
        project_id: project_id.to_string(),
        enabled: row.get::<Option<i32>, _>("enabled").map(|v| v != 0),
        max_age_days: row.get("max_age_days"),
        max_builds_per_project: row.get("max_builds_per_project"),
        max_artifact_size_bytes: row.get("max_artifact_size_bytes"),
        cleanup_target: row
            .get::<Option<String>, _>("cleanup_target")
            .and_then(|s| RetentionCleanupTarget::from_str(&s).ok()),
        keep_statuses: row
            .get::<Option<String>, _>("keep_statuses")
            .and_then(|s| serde_json::from_str(&s).ok()),
        artifact_ttl_days: row.get("artifact_ttl_days"),
        updated_at: Some(row.get::<i64, _>("updated_at")),
    };

    Ok(merge_override(&global, &ovr))
}

fn merge_override(global: &RetentionPolicy, ovr: &ProjectRetentionOverride) -> RetentionPolicy {
    RetentionPolicy {
        enabled: ovr.enabled.unwrap_or(global.enabled),
        max_age_days: ovr.max_age_days.or(global.max_age_days),
        max_builds_per_project: ovr.max_builds_per_project.or(global.max_builds_per_project),
        max_artifact_size_bytes: ovr
            .max_artifact_size_bytes
            .or(global.max_artifact_size_bytes),
        cleanup_target: ovr.cleanup_target.unwrap_or(global.cleanup_target),
        keep_statuses: ovr
            .keep_statuses
            .clone()
            .unwrap_or_else(|| global.keep_statuses.clone()),
        dry_run: global.dry_run,
        cleanup_interval_secs: global.cleanup_interval_secs,
        artifact_ttl_days: ovr.artifact_ttl_days.or(global.artifact_ttl_days),
        updated_at: ovr.updated_at.or(global.updated_at),
    }
}

// ── Global retention policy ──────────────────────────────────────

pub async fn get_retention_policy(
    State(state): State<std::sync::Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<RetentionPolicyResponse> {
    check_permission(
        &state.enforcer,
        &auth.0.role,
        RETENTION_PERMISSION_RESOURCE,
        "read",
    )
    .await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let policy = load_global_policy(pool).await.map_err(|e| {
        error!(error = %e, "failed to load retention policy");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Failed to load retention policy",
        )
    })?;

    Ok(Json(RetentionPolicyResponse { policy }))
}

pub async fn update_retention_policy(
    State(state): State<std::sync::Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<UpdateRetentionPolicyRequest>,
) -> ApiResult<RetentionPolicyResponse> {
    check_permission(
        &state.enforcer,
        &auth.0.role,
        RETENTION_PERMISSION_RESOURCE,
        "write",
    )
    .await?;

    // Validate numeric fields to prevent self-DoS (tight loop) or mass deletion (negative values)
    if req.cleanup_interval_secs < 60 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "cleanup_interval_secs must be at least 60",
        ));
    }
    if let Some(v) = req.max_age_days
        && v < 1
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "max_age_days must be at least 1",
        ));
    }
    if let Some(v) = req.max_builds_per_project
        && v < 1
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "max_builds_per_project must be at least 1",
        ));
    }
    if let Some(v) = req.max_artifact_size_bytes
        && v < 1
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "max_artifact_size_bytes must be at least 1",
        ));
    }
    if let Some(v) = req.artifact_ttl_days
        && v < 1
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "validation_error",
            "artifact_ttl_days must be at least 1",
        ));
    }

    let now = now_unix();
    let keep_statuses_json =
        serde_json::to_string(&req.keep_statuses).unwrap_or_else(|_| "[]".to_string());

    let store = state.store.lock().await;
    let pool = store.pool();

    sqlx::query(
        "INSERT INTO retention_policy (id, enabled, max_age_days, max_builds_per_project, \
         max_artifact_size_bytes, cleanup_target, keep_statuses, dry_run, cleanup_interval_secs, \
         artifact_ttl_days, updated_by, created_at, updated_at) \
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11) \
         ON CONFLICT(id) DO UPDATE SET \
         enabled = ?1, max_age_days = ?2, max_builds_per_project = ?3, \
         max_artifact_size_bytes = ?4, cleanup_target = ?5, keep_statuses = ?6, \
         dry_run = ?7, cleanup_interval_secs = ?8, artifact_ttl_days = ?9, \
         updated_by = ?10, updated_at = ?11",
    )
    .bind(req.enabled as i32)
    .bind(req.max_age_days)
    .bind(req.max_builds_per_project)
    .bind(req.max_artifact_size_bytes)
    .bind(req.cleanup_target.to_string())
    .bind(&keep_statuses_json)
    .bind(req.dry_run as i32)
    .bind(req.cleanup_interval_secs)
    .bind(req.artifact_ttl_days)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to update retention policy");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Failed to update retention policy",
        )
    })?;

    let details = serde_json::to_string(&serde_json::json!({
        "enabled": req.enabled,
        "max_age_days": req.max_age_days,
        "max_builds_per_project": req.max_builds_per_project,
        "max_artifact_size_bytes": req.max_artifact_size_bytes,
        "cleanup_target": req.cleanup_target.to_string(),
        "dry_run": req.dry_run,
    }))
    .ok();

    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "update_retention_policy",
        "retention_policy",
        Some("global"),
        details.as_deref(),
    )
    .await;

    let policy = load_global_policy(pool).await.map_err(|e| {
        error!(error = %e, "failed to reload retention policy");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Failed to reload retention policy",
        )
    })?;

    Ok(Json(RetentionPolicyResponse { policy }))
}

// ── Per-project retention overrides ──────────────────────────────

pub async fn get_project_retention(
    State(state): State<std::sync::Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> ApiResult<EffectiveProjectRetentionResponse> {
    check_permission(&state.enforcer, &auth.0.role, "projects", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let global = load_global_policy(pool).await.map_err(|e| {
        error!(error = %e, "failed to load global retention policy");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Failed to load retention policy",
        )
    })?;

    let row = sqlx::query("SELECT * FROM project_retention_overrides WHERE project_id = ?1")
        .bind(&project_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load project retention override");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Failed to load project retention override",
            )
        })?;

    if let Some(row) = row {
        let ovr = ProjectRetentionOverride {
            project_id: project_id.clone(),
            enabled: row.get::<Option<i32>, _>("enabled").map(|v| v != 0),
            max_age_days: row.get("max_age_days"),
            max_builds_per_project: row.get("max_builds_per_project"),
            max_artifact_size_bytes: row.get("max_artifact_size_bytes"),
            cleanup_target: row
                .get::<Option<String>, _>("cleanup_target")
                .and_then(|s| RetentionCleanupTarget::from_str(&s).ok()),
            keep_statuses: row
                .get::<Option<String>, _>("keep_statuses")
                .and_then(|s| serde_json::from_str(&s).ok()),
            artifact_ttl_days: row.get("artifact_ttl_days"),
            updated_at: Some(row.get::<i64, _>("updated_at")),
        };

        let effective = merge_override(&global, &ovr);
        Ok(Json(EffectiveProjectRetentionResponse {
            effective,
            has_override: true,
            override_fields: Some(ovr),
        }))
    } else {
        Ok(Json(EffectiveProjectRetentionResponse {
            effective: global,
            has_override: false,
            override_fields: None,
        }))
    }
}

pub async fn update_project_retention(
    State(state): State<std::sync::Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(req): Json<UpdateProjectRetentionOverrideRequest>,
) -> ApiResult<EffectiveProjectRetentionResponse> {
    check_permission(&state.enforcer, &auth.0.role, "projects", "write").await?;

    let now = now_unix();
    let keep_statuses_json = req
        .keep_statuses
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok());
    let cleanup_target_str = req.cleanup_target.map(|t| t.to_string());

    let store = state.store.lock().await;
    let pool = store.pool();

    sqlx::query(
        "INSERT INTO project_retention_overrides \
         (project_id, enabled, max_age_days, max_builds_per_project, max_artifact_size_bytes, \
          cleanup_target, keep_statuses, artifact_ttl_days, updated_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10) \
         ON CONFLICT(project_id) DO UPDATE SET \
         enabled = ?2, max_age_days = ?3, max_builds_per_project = ?4, \
         max_artifact_size_bytes = ?5, cleanup_target = ?6, keep_statuses = ?7, \
         artifact_ttl_days = ?8, updated_by = ?9, updated_at = ?10",
    )
    .bind(&project_id)
    .bind(req.enabled.map(|v| v as i32))
    .bind(req.max_age_days)
    .bind(req.max_builds_per_project)
    .bind(req.max_artifact_size_bytes)
    .bind(&cleanup_target_str)
    .bind(&keep_statuses_json)
    .bind(req.artifact_ttl_days)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to update project retention override");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Failed to update project retention override",
        )
    })?;

    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "update_project_retention",
        "project_retention",
        Some(&project_id),
        None,
    )
    .await;

    drop(store);

    // Re-fetch to return the effective policy
    get_project_retention(State(state), auth, Path(project_id)).await
}

pub async fn delete_project_retention(
    State(state): State<std::sync::Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> ApiResult<EffectiveProjectRetentionResponse> {
    check_permission(&state.enforcer, &auth.0.role, "projects", "write").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    sqlx::query("DELETE FROM project_retention_overrides WHERE project_id = ?1")
        .bind(&project_id)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to delete project retention override");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Failed to delete project retention override",
            )
        })?;

    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "delete_project_retention",
        "project_retention",
        Some(&project_id),
        None,
    )
    .await;

    drop(store);

    get_project_retention(State(state), auth, Path(project_id)).await
}

// ── Last cleanup summary ─────────────────────────────────────────

pub async fn get_last_cleanup(
    State(state): State<std::sync::Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<RetentionCleanupSummaryResponse> {
    check_permission(
        &state.enforcer,
        &auth.0.role,
        RETENTION_PERMISSION_RESOURCE,
        "read",
    )
    .await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let row = sqlx::query(
        "SELECT details FROM audit_logs \
         WHERE action = 'retention_cleanup_completed' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to query last cleanup");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Failed to load last cleanup summary",
        )
    })?;

    let last_cleanup = row.and_then(|r| {
        let details: Option<String> = r.get("details");
        details.and_then(|d| serde_json::from_str::<RetentionCleanupSummary>(&d).ok())
    });

    Ok(Json(RetentionCleanupSummaryResponse { last_cleanup }))
}

#[cfg(test)]
mod tests {
    use super::RETENTION_PERMISSION_RESOURCE;
    use crate::rbac::init_enforcer;

    #[tokio::test]
    async fn retention_uses_registered_instance_settings_permission() {
        let enforcer = init_enforcer().await.expect("RBAC policy should load");

        assert!(
            enforcer
                .check("owner", RETENTION_PERMISSION_RESOURCE, "read")
                .await
        );
        assert!(
            enforcer
                .check("owner", RETENTION_PERMISSION_RESOURCE, "write")
                .await
        );
        assert!(
            enforcer
                .check("admin", RETENTION_PERMISSION_RESOURCE, "read")
                .await
        );
    }
}
