use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
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
use crate::project_rbac::{
    ProjectPermission, effective_role_string, require_project_permission,
    resolve_effective_project_role,
};
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

fn normalize_local_repo_path(raw: &str) -> Result<PathBuf, (StatusCode, Json<ApiError>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "local_repository_path is required",
        ));
    }

    let candidate = PathBuf::from(trimmed);
    if !candidate.is_absolute() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "local_repository_path must be an absolute path",
        ));
    }

    std::fs::canonicalize(&candidate).map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "local_repository_path does not exist or is not accessible",
        )
    })
}

fn assert_git_repo(path: &std::path::Path) -> Result<(), (StatusCode, Json<ApiError>)> {
    let path_str = path.to_string_lossy().into_owned();

    let inside = Command::new("git")
        .args([
            "-C",
            path_str.as_str(),
            "rev-parse",
            "--is-inside-work-tree",
        ])
        .output();
    if let Ok(output) = inside
        && output.status.success()
        && String::from_utf8_lossy(&output.stdout).trim() == "true"
    {
        return Ok(());
    }

    let bare = Command::new("git")
        .args(["-C", path_str.as_str(), "rev-parse", "--is-bare-repository"])
        .output();
    if let Ok(output) = bare
        && output.status.success()
        && String::from_utf8_lossy(&output.stdout).trim() == "true"
    {
        return Ok(());
    }

    Err(api_err(
        StatusCode::BAD_REQUEST,
        "invalid_repository",
        "local_repository_path is not a valid git repository",
    ))
}

fn resolve_default_branch(path: &std::path::Path) -> Option<String> {
    let path_str = path.to_string_lossy().into_owned();
    Command::new("git")
        .args(["-C", path_str.as_str(), "symbolic-ref", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

struct LocalRepoInspection {
    canonical_str: String,
    default_branch: Option<String>,
    repo_name: String,
}

async fn inspect_local_repo_for_project(
    raw_path: &str,
) -> Result<LocalRepoInspection, (StatusCode, Json<ApiError>)> {
    let raw_path = raw_path.to_string();
    tokio::task::spawn_blocking(move || {
        let canonical_path = normalize_local_repo_path(&raw_path)?;
        assert_git_repo(&canonical_path)?;

        let default_branch = resolve_default_branch(&canonical_path);
        let canonical_str = canonical_path.to_string_lossy().into_owned();
        let repo_name = canonical_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "local-repo".to_string());

        Ok(LocalRepoInspection {
            canonical_str,
            default_branch,
            repo_name,
        })
    })
    .await
    .map_err(|e| {
        error!(error = %e, "local repository inspection task panicked or was cancelled");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to inspect local repository",
        )
    })?
}

async fn ensure_local_repository_for_project(
    pool: &sqlx::SqlitePool,
    actor_user_id: &str,
    raw_path: &str,
) -> Result<(String, Option<String>), (StatusCode, Json<ApiError>)> {
    let inspection = inspect_local_repo_for_project(raw_path).await?;
    let canonical_str = inspection.canonical_str.clone();

    let existing = sqlx::query(
        "SELECT r.id as repository_id, r.default_branch as default_branch \
         FROM integration_repositories r \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         JOIN integrations i ON i.id = inst.integration_id \
         WHERE i.provider = 'local_git' AND r.external_id = ?1 \
         LIMIT 1",
    )
    .bind(&canonical_str)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to query existing local repository mapping");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to check existing repository mappings",
        )
    })?;

    if let Some(row) = existing {
        let repository_id: String = row.get("repository_id");
        let default_branch: Option<String> = row.get("default_branch");
        return Ok((repository_id, default_branch));
    }

    let default_branch = inspection.default_branch;
    let repo_name = inspection.repo_name;
    let now = now_unix();
    let integration_id = Uuid::new_v4().to_string();
    let installation_id = Uuid::new_v4().to_string();
    let repository_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO integrations (id, provider, host_url, auth_mode, status, display_name, created_by, created_at, updated_at) \
         VALUES (?1, 'local_git', 'local://filesystem', 'local_path', 'active', ?2, ?3, ?4, ?4)",
    )
    .bind(&integration_id)
    .bind(&repo_name)
    .bind(actor_user_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to create local integration during project create");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to register local repository",
        )
    })?;

    sqlx::query(
        "INSERT INTO integration_installations (id, integration_id, external_id, account_name, account_type, permissions, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'local', 'filesystem', '{}', ?4, ?4)",
    )
    .bind(&installation_id)
    .bind(&integration_id)
    .bind(&canonical_str)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to create local installation during project create");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to register local repository",
        )
    })?;

    sqlx::query(
        "INSERT INTO integration_repositories (id, installation_id, external_id, full_name, default_branch, is_private, html_url, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?7)",
    )
    .bind(&repository_id)
    .bind(&installation_id)
    .bind(&canonical_str)
    .bind(&repo_name)
    .bind(&default_branch)
    .bind(&canonical_str)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to create local repository during project create");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to register local repository",
        )
    })?;

    Ok((repository_id, default_branch))
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

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    if req.repository_id.is_some() && req.local_repository_path.is_some() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Provide either repository_id or local_repository_path, not both",
        ));
    }

    let (repository_id, inferred_default_branch) = match (
        req.repository_id.clone(),
        req.local_repository_path.as_deref(),
    ) {
        (Some(repo_id), None) => {
            // Validate repository_id if provided
            let repo_exists: bool = sqlx::query_scalar(
                "SELECT COUNT(*) > 0 FROM integration_repositories WHERE id = ?1",
            )
            .bind(&repo_id)
            .fetch_one(&pool)
            .await
            .unwrap_or(false);

            if !repo_exists {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_repository",
                    "Repository not found",
                ));
            }
            (Some(repo_id), None)
        }
        (None, Some(local_repo_path)) => {
            let (repo_id, branch) =
                ensure_local_repository_for_project(&pool, &auth.0.user_id, local_repo_path)
                    .await?;
            (Some(repo_id), branch)
        }
        (None, None) => {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "repository_id or local_repository_path is required",
            ));
        }
        (Some(_), Some(_)) => {
            // Already validated above.
            unreachable!("validated mutually exclusive repository inputs")
        }
    };

    let default_branch = req.default_branch.clone().or(inferred_default_branch);

    let now = now_unix();
    let project_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO projects (id, name, description, repository_id, settings, default_branch, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, '{}', ?5, ?6, ?7, ?7)",
    )
    .bind(&project_id)
    .bind(name)
    .bind(&req.description)
    .bind(&repository_id)
    .bind(&default_branch)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to create project");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to create project")
    })?;

    let details = serde_json::json!({
        "project_name": name,
        "created_by": auth.0.email,
        "repository_id": repository_id,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "project_created",
        "project",
        Some(&project_id),
        Some(&details),
    )
    .await;

    // Auto-add creator as project maintainer (only for non-admin/non-owner users
    // who will need explicit membership; admins/owners bypass membership checks).
    if auth.0.role != "owner" && auth.0.role != "admin" {
        let member_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO project_members (id, project_id, user_id, role, created_by, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'maintainer', ?3, ?4, ?4)",
        )
        .bind(&member_id)
        .bind(&project_id)
        .bind(&auth.0.user_id)
        .bind(now)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to auto-add creator as project maintainer");
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to add creator as project member")
        })?;
    }

    info!(project_id = %project_id, name = %name, "project created");

    let project = Project {
        id: project_id,
        name: name.to_string(),
        description: req.description,
        repository_id,
        settings: serde_json::json!({}),
        default_branch,
        created_by: auth.0.user_id,
        created_at: now,
        updated_at: now,
    };

    Ok(Json(CreateProjectResponse { project }))
}

/// `GET /v1/projects` — list projects with optional search.
///
/// For owner/admin: returns all projects.
/// For developer/qa_viewer: returns only projects where the user has explicit membership.
pub async fn list_projects(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListProjectsQuery>,
) -> ApiResult<ListProjectsResponse> {
    // All authenticated users can call this endpoint; filtering is role-based.
    let store = state.store.lock().await;
    let pool = store.pool();

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let is_admin = auth.0.role == "owner" || auth.0.role == "admin";

    let (total, rows) = if is_admin {
        // Admin/owner: see all projects (unchanged behaviour).
        if let Some(ref search) = params.search {
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
        }
    } else {
        // Non-admin: only see projects where user has explicit membership.
        if let Some(ref search) = params.search {
            let pattern = format!("%{search}%");
            let total: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM projects p \
                 INNER JOIN project_members pm ON pm.project_id = p.id \
                 WHERE pm.user_id = ?1 AND (p.name LIKE ?2 OR p.description LIKE ?2)",
            )
            .bind(&auth.0.user_id)
            .bind(&pattern)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

            let rows = sqlx::query(
                "SELECT p.* FROM projects p \
                 INNER JOIN project_members pm ON pm.project_id = p.id \
                 WHERE pm.user_id = ?1 AND (p.name LIKE ?2 OR p.description LIKE ?2) \
                 ORDER BY p.created_at DESC LIMIT ?3 OFFSET ?4",
            )
            .bind(&auth.0.user_id)
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
            let total: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM projects p \
                 INNER JOIN project_members pm ON pm.project_id = p.id \
                 WHERE pm.user_id = ?1",
            )
            .bind(&auth.0.user_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

            let rows = sqlx::query(
                "SELECT p.* FROM projects p \
                 INNER JOIN project_members pm ON pm.project_id = p.id \
                 WHERE pm.user_id = ?1 \
                 ORDER BY p.created_at DESC LIMIT ?2 OFFSET ?3",
            )
            .bind(&auth.0.user_id)
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
        }
    };

    let projects = rows.iter().map(row_to_project).collect();

    Ok(Json(ListProjectsResponse { projects, total }))
}

/// `GET /v1/projects/{project_id}` — project detail with counts.
pub async fn get_project(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath(project_id): AxumPath<String>,
) -> ApiResult<ProjectDetailResponse> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let effective = resolve_effective_project_role(
        &pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::Read)?;

    let project_row = sqlx::query("SELECT * FROM projects WHERE id = ?1")
        .bind(&project_id)
        .fetch_optional(&pool)
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
            .fetch_one(&pool)
            .await
            .unwrap_or(0);

    let build_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM builds WHERE project_id = ?1")
        .bind(&project_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    Ok(Json(ProjectDetailResponse {
        project,
        pipeline_count,
        build_count,
        current_user_role: effective_role_string(&effective),
    }))
}

/// `PATCH /v1/projects/{project_id}` — partial update.
pub async fn update_project(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath(project_id): AxumPath<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> ApiResult<CreateProjectResponse> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let effective = resolve_effective_project_role(
        &pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::Write)?;

    // Verify project exists
    let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM projects WHERE id = ?1")
        .bind(&project_id)
        .fetch_one(&pool)
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
    if let Some(ref name) = req.name
        && name.trim().is_empty()
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Project name must not be empty",
        ));
    }

    // Validate repository_id if provided
    if let Some(ref repo_id) = req.repository_id {
        let repo_exists: bool =
            sqlx::query_scalar("SELECT COUNT(*) > 0 FROM integration_repositories WHERE id = ?1")
                .bind(repo_id)
                .fetch_one(&pool)
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
            .fetch_one(&pool)
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

    q.execute(&pool).await.map_err(|e| {
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
        &pool,
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
        .fetch_one(&pool)
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
    AxumPath(project_id): AxumPath<String>,
) -> ApiResult<serde_json::Value> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let effective = resolve_effective_project_role(
        &pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::Delete)?;

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
        &pool,
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
