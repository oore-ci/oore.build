use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, CreateProjectRequest, CreateProjectResponse, ListProjectsResponse, Project,
    ProjectDetailResponse, UpdateProjectRequest,
};
use serde::Deserialize;
use sqlx::Row;
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

// ── Row conversion ──────────────────────────────────────────────

fn row_to_project(row: &sqlx::sqlite::SqliteRow) -> Project {
    let settings_str: String = row.get("settings");
    let settings: serde_json::Value =
        serde_json::from_str(&settings_str).unwrap_or(serde_json::json!({}));

    Project {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        repository_id: row.get("repository_id"),
        settings,
        default_branch: row.get("default_branch"),
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ── Query parameters ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListProjectsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub search: Option<String>,
}

// ── Handlers ────────────────────────────────────────────────────

/// `POST /v1/projects` — create a new project.
pub async fn create_project(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateProjectRequest>,
) -> ApiResult<CreateProjectResponse> {
    check_permission(&state.enforcer, &auth.0.role, "projects", "write").await?;

    let name = req.name.trim();
    if name.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Project name must not be empty",
        ));
    }

    let store = state.store.lock().await;
    let pool = store.pool();

    // Validate repository_id if provided
    if let Some(ref repo_id) = req.repository_id {
        let repo_exists: bool =
            sqlx::query_scalar("SELECT COUNT(*) > 0 FROM integration_repositories WHERE id = ?1")
                .bind(repo_id)
                .fetch_one(pool)
                .await
                .unwrap_or(false);

        if !repo_exists {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_repository",
                "Repository not found",
            ));
        }
    }

    let now = now_unix();
    let project_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO projects (id, name, description, repository_id, settings, default_branch, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, '{}', ?5, ?6, ?7, ?7)",
    )
    .bind(&project_id)
    .bind(name)
    .bind(&req.description)
    .bind(&req.repository_id)
    .bind(&req.default_branch)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to create project");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to create project")
    })?;

    let details = serde_json::json!({
        "project_name": name,
        "created_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "project_created",
        "project",
        Some(&project_id),
        Some(&details),
    )
    .await;

    info!(project_id = %project_id, name = %name, "project created");

    let project = Project {
        id: project_id,
        name: name.to_string(),
        description: req.description,
        repository_id: req.repository_id,
        settings: serde_json::json!({}),
        default_branch: req.default_branch,
        created_by: auth.0.user_id,
        created_at: now,
        updated_at: now,
    };

    Ok(Json(CreateProjectResponse { project }))
}

/// `GET /v1/projects` — list projects with optional search.
pub async fn list_projects(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListProjectsQuery>,
) -> ApiResult<ListProjectsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "projects", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let (total, rows) = if let Some(ref search) = params.search {
        let pattern = format!("%{search}%");
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM projects WHERE name LIKE ?1 OR description LIKE ?1",
        )
        .bind(&pattern)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        let rows = sqlx::query(
            "SELECT * FROM projects WHERE name LIKE ?1 OR description LIKE ?1 \
             ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
        )
        .bind(&pattern)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to list projects");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to list projects",
            )
        })?;

        (total, rows)
    } else {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects")
            .fetch_one(pool)
            .await
            .unwrap_or(0);

        let rows =
            sqlx::query("SELECT * FROM projects ORDER BY created_at DESC LIMIT ?1 OFFSET ?2")
                .bind(limit)
                .bind(offset)
                .fetch_all(pool)
                .await
                .map_err(|e| {
                    error!(error = %e, "failed to list projects");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "store_error",
                        "Failed to list projects",
                    )
                })?;

        (total, rows)
    };

    let projects = rows.iter().map(row_to_project).collect();

    Ok(Json(ListProjectsResponse { projects, total }))
}

/// `GET /v1/projects/{project_id}` — project detail with counts.
pub async fn get_project(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> ApiResult<ProjectDetailResponse> {
    check_permission(&state.enforcer, &auth.0.role, "projects", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let project_row = sqlx::query("SELECT * FROM projects WHERE id = ?1")
        .bind(&project_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch project");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch project",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Project not found"))?;

    let project = row_to_project(&project_row);

    let pipeline_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pipelines WHERE project_id = ?1")
            .bind(&project_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    let build_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM builds WHERE project_id = ?1")
        .bind(&project_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    Ok(Json(ProjectDetailResponse {
        project,
        pipeline_count,
        build_count,
    }))
}

/// `PATCH /v1/projects/{project_id}` — partial update.
pub async fn update_project(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> ApiResult<CreateProjectResponse> {
    check_permission(&state.enforcer, &auth.0.role, "projects", "write").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    // Verify project exists
    let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM projects WHERE id = ?1")
        .bind(&project_id)
        .fetch_one(pool)
        .await
        .unwrap_or(false);

    if !exists {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Project not found",
        ));
    }

    // Validate name if provided
    if let Some(ref name) = req.name {
        if name.trim().is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "Project name must not be empty",
            ));
        }
    }

    // Validate repository_id if provided
    if let Some(ref repo_id) = req.repository_id {
        let repo_exists: bool =
            sqlx::query_scalar("SELECT COUNT(*) > 0 FROM integration_repositories WHERE id = ?1")
                .bind(repo_id)
                .fetch_one(pool)
                .await
                .unwrap_or(false);

        if !repo_exists {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_repository",
                "Repository not found",
            ));
        }
    }

    let now = now_unix();

    // Build dynamic SET clause for partial update
    let mut set_parts = Vec::new();
    let mut bind_values: Vec<String> = Vec::new();

    if let Some(ref name) = req.name {
        bind_values.push(name.trim().to_string());
        set_parts.push(format!("name = ?{}", bind_values.len()));
    }
    if let Some(ref description) = req.description {
        bind_values.push(description.clone());
        set_parts.push(format!("description = ?{}", bind_values.len()));
    }
    if let Some(ref repository_id) = req.repository_id {
        bind_values.push(repository_id.clone());
        set_parts.push(format!("repository_id = ?{}", bind_values.len()));
    }
    if let Some(ref default_branch) = req.default_branch {
        bind_values.push(default_branch.clone());
        set_parts.push(format!("default_branch = ?{}", bind_values.len()));
    }
    if let Some(ref settings) = req.settings {
        bind_values.push(settings.to_string());
        set_parts.push(format!("settings = ?{}", bind_values.len()));
    }

    if set_parts.is_empty() {
        // Nothing to update — just return the current project
        let row = sqlx::query("SELECT * FROM projects WHERE id = ?1")
            .bind(&project_id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to fetch project");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to fetch project",
                )
            })?;
        return Ok(Json(CreateProjectResponse {
            project: row_to_project(&row),
        }));
    }

    // Always update updated_at
    bind_values.push(now.to_string());
    set_parts.push(format!("updated_at = ?{}", bind_values.len()));

    let query = format!(
        "UPDATE projects SET {} WHERE id = ?{}",
        set_parts.join(", "),
        bind_values.len() + 1
    );

    let mut q = sqlx::query(&query);
    for val in &bind_values {
        q = q.bind(val);
    }
    q = q.bind(&project_id);

    q.execute(pool).await.map_err(|e| {
        error!(error = %e, "failed to update project");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update project",
        )
    })?;

    let details = serde_json::json!({
        "updated_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "project_updated",
        "project",
        Some(&project_id),
        Some(&details),
    )
    .await;

    info!(project_id = %project_id, "project updated");

    let row = sqlx::query("SELECT * FROM projects WHERE id = ?1")
        .bind(&project_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to reload project");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to reload project",
            )
        })?;

    Ok(Json(CreateProjectResponse {
        project: row_to_project(&row),
    }))
}

/// `DELETE /v1/projects/{project_id}` — delete a project.
pub async fn delete_project(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    check_permission(&state.enforcer, &auth.0.role, "projects", "delete").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    // Use a transaction so the active-build check, terminal-build cleanup,
    // and project delete are atomic (prevents race with concurrent build creation).
    let mut tx = pool.begin().await.map_err(|e| {
        error!(error = %e, "failed to begin transaction");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete project",
        )
    })?;

    // Verify project exists
    let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM projects WHERE id = ?1")
        .bind(&project_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap_or(false);

    if !exists {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Project not found",
        ));
    }

    // Check for non-terminal builds
    let active_builds: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM builds WHERE project_id = ?1 \
         AND status NOT IN ('succeeded', 'failed', 'canceled', 'timed_out', 'expired')",
    )
    .bind(&project_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(0);

    if active_builds > 0 {
        return Err(api_err(
            StatusCode::CONFLICT,
            "active_builds",
            "Cannot delete project with active builds",
        ));
    }

    // Delete terminal builds first (non-cascading FK on builds.project_id)
    sqlx::query(
        "DELETE FROM builds WHERE project_id = ?1 \
         AND status IN ('succeeded', 'failed', 'canceled', 'timed_out', 'expired')",
    )
    .bind(&project_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to clean up builds for project");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete project",
        )
    })?;

    sqlx::query("DELETE FROM projects WHERE id = ?1")
        .bind(&project_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to delete project");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to delete project",
            )
        })?;

    tx.commit().await.map_err(|e| {
        error!(error = %e, "failed to commit delete transaction");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete project",
        )
    })?;

    let details = serde_json::json!({
        "deleted_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "project_deleted",
        "project",
        Some(&project_id),
        Some(&details),
    )
    .await;

    info!(project_id = %project_id, "project deleted");

    Ok(Json(serde_json::json!({"ok": true})))
}
