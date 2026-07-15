use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use oore_contract::{
    AddProjectMemberRequest, AddProjectMemberResponse, ApiError, ListProjectMembersResponse,
    ProjectMember, ProjectRole, UpdateProjectMemberRequest, UpdateProjectMemberResponse,
};
use sqlx::Row;
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

// ── Row conversion ──────────────────────────────────────────────

fn row_to_project_member(row: &sqlx::sqlite::SqliteRow) -> Result<ProjectMember, String> {
    let role_str: String = row.get("role");
    let role: ProjectRole = role_str.parse()?;

    Ok(ProjectMember {
        id: row.get("id"),
        project_id: row.get("project_id"),
        user_id: row.get("user_id"),
        role,
        user_email: row.get("user_email"),
        user_display_name: row.get("user_display_name"),
        user_avatar_url: row.get("user_avatar_url"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

// ── Handlers ────────────────────────────────────────────────────

/// `GET /v1/projects/{project_id}/members` — list project members.
pub async fn list_project_members(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath(project_id): AxumPath<String>,
) -> ApiResult<ListProjectMembersResponse> {
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

    let rows = sqlx::query(
        "SELECT pm.id, pm.project_id, pm.user_id, pm.role, pm.created_at, pm.updated_at, \
                u.email AS user_email, u.display_name AS user_display_name, u.avatar_url AS user_avatar_url \
         FROM project_members pm \
         JOIN users u ON u.id = pm.user_id \
         WHERE pm.project_id = ?1 \
         ORDER BY pm.created_at ASC",
    )
    .bind(&project_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to list project members");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to list project members")
    })?;

    let members: Vec<ProjectMember> = rows
        .iter()
        .filter_map(|row| row_to_project_member(row).ok())
        .collect();

    Ok(Json(ListProjectMembersResponse { members }))
}

/// `POST /v1/projects/{project_id}/members` — add a member to a project.
pub async fn add_project_member(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath(project_id): AxumPath<String>,
    Json(req): Json<AddProjectMemberRequest>,
) -> ApiResult<AddProjectMemberResponse> {
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
    require_project_permission(&effective, ProjectPermission::ManageMembers)?;

    // Verify project exists
    let project_exists: bool =
        sqlx::query_scalar("SELECT COUNT(*) > 0 FROM projects WHERE id = ?1")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap_or(false);
    if !project_exists {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Project not found",
        ));
    }

    // Invited users can be assigned before their first sign-in; disabled users
    // remain ineligible until re-enabled.
    let target_user = sqlx::query("SELECT id, email, display_name, avatar_url, role FROM users WHERE id = ?1 AND status IN ('active', 'invited')")
        .bind(&req.user_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to query target user");
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to verify user")
        })?
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "invalid_user", "User not found or not eligible for project access"))?;

    let target_instance_role: String = target_user.get("role");
    // owner/admin don't need project membership (they have implicit full access)
    if target_instance_role == "owner" || target_instance_role == "admin" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_user",
            "Owner and admin users already have full access to all projects",
        ));
    }
    if target_instance_role == "qa_viewer" && req.role != ProjectRole::Viewer {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_project_role",
            "QA Viewer users can only be assigned the Viewer project role",
        ));
    }

    // Check for existing membership
    let already_member: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM project_members WHERE project_id = ?1 AND user_id = ?2",
    )
    .bind(&project_id)
    .bind(&req.user_id)
    .fetch_one(&pool)
    .await
    .unwrap_or(false);

    if already_member {
        return Err(api_err(
            StatusCode::CONFLICT,
            "already_member",
            "User is already a member of this project",
        ));
    }

    let now = now_unix();
    let member_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO project_members (id, project_id, user_id, role, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
    )
    .bind(&member_id)
    .bind(&project_id)
    .bind(&req.user_id)
    .bind(req.role.to_string())
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to add project member");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to add project member")
    })?;

    let details = serde_json::json!({
        "added_user_id": req.user_id,
        "role": req.role.to_string(),
        "added_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "project_member_added",
        "project",
        Some(&project_id),
        Some(&details),
    )
    .await;

    info!(project_id = %project_id, user_id = %req.user_id, role = %req.role, "project member added");

    let member = ProjectMember {
        id: member_id,
        project_id,
        user_id: req.user_id,
        role: req.role,
        user_email: target_user.get("email"),
        user_display_name: target_user.get("display_name"),
        user_avatar_url: target_user.get("avatar_url"),
        created_at: now,
        updated_at: now,
    };

    Ok(Json(AddProjectMemberResponse { member }))
}

/// `PATCH /v1/projects/{project_id}/members/{user_id}` — update a member's role.
pub async fn update_project_member(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath((project_id, user_id)): AxumPath<(String, String)>,
    Json(req): Json<UpdateProjectMemberRequest>,
) -> ApiResult<UpdateProjectMemberResponse> {
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
    require_project_permission(&effective, ProjectPermission::ManageMembers)?;

    let target_instance_role: Option<String> =
        sqlx::query_scalar("SELECT role FROM users WHERE id = ?1")
            .bind(&user_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to check target user role");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to check target user role",
                )
            })?;
    if target_instance_role.as_deref() == Some("qa_viewer") && req.role != ProjectRole::Viewer {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_project_role",
            "QA Viewer users can only be assigned the Viewer project role",
        ));
    }

    let now = now_unix();

    // Prevent demoting the last maintainer — would leave project unmanageable
    if req.role != ProjectRole::Maintainer {
        let current_role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM project_members WHERE project_id = ?1 AND user_id = ?2",
        )
        .bind(&project_id)
        .bind(&user_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to check current member role");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to check member role",
            )
        })?;

        if current_role.as_deref() == Some("maintainer") {
            let maintainer_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM project_members WHERE project_id = ?1 AND role = 'maintainer'",
            )
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap_or(0);

            if maintainer_count <= 1 {
                return Err(api_err(
                    StatusCode::CONFLICT,
                    "last_maintainer",
                    "Cannot demote the last maintainer — project would become unmanageable",
                ));
            }
        }
    }

    let result = sqlx::query(
        "UPDATE project_members SET role = ?1, updated_at = ?2 WHERE project_id = ?3 AND user_id = ?4",
    )
    .bind(req.role.to_string())
    .bind(now)
    .bind(&project_id)
    .bind(&user_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to update project member");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to update project member")
    })?;

    if result.rows_affected() == 0 {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Project member not found",
        ));
    }

    let details = serde_json::json!({
        "target_user_id": user_id,
        "new_role": req.role.to_string(),
        "updated_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "project_member_role_updated",
        "project",
        Some(&project_id),
        Some(&details),
    )
    .await;

    info!(project_id = %project_id, user_id = %user_id, role = %req.role, "project member role updated");

    // Fetch the updated member with user info
    let row = sqlx::query(
        "SELECT pm.id, pm.project_id, pm.user_id, pm.role, pm.created_at, pm.updated_at, \
                u.email AS user_email, u.display_name AS user_display_name, u.avatar_url AS user_avatar_url \
         FROM project_members pm \
         JOIN users u ON u.id = pm.user_id \
         WHERE pm.project_id = ?1 AND pm.user_id = ?2",
    )
    .bind(&project_id)
    .bind(&user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to fetch updated project member");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to fetch updated member")
    })?;

    let member = row_to_project_member(&row).map_err(|_| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "data_error",
            "Invalid project role in database",
        )
    })?;

    Ok(Json(UpdateProjectMemberResponse { member }))
}

/// `DELETE /v1/projects/{project_id}/members/{user_id}` — remove a member from a project.
pub async fn remove_project_member(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath((project_id, user_id)): AxumPath<(String, String)>,
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
    require_project_permission(&effective, ProjectPermission::ManageMembers)?;

    // Prevent removing the last maintainer — would leave project unmanageable
    let target_role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM project_members WHERE project_id = ?1 AND user_id = ?2",
    )
    .bind(&project_id)
    .bind(&user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to check target member role");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to check member role",
        )
    })?;

    if target_role.as_deref() == Some("maintainer") {
        let maintainer_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM project_members WHERE project_id = ?1 AND role = 'maintainer'",
        )
        .bind(&project_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

        if maintainer_count <= 1 {
            return Err(api_err(
                StatusCode::CONFLICT,
                "last_maintainer",
                "Cannot remove the last maintainer — project would become unmanageable",
            ));
        }
    }

    let result = sqlx::query("DELETE FROM project_members WHERE project_id = ?1 AND user_id = ?2")
        .bind(&project_id)
        .bind(&user_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to remove project member");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to remove project member",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Project member not found",
        ));
    }

    let details = serde_json::json!({
        "removed_user_id": user_id,
        "removed_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "project_member_removed",
        "project",
        Some(&project_id),
        Some(&details),
    )
    .await;

    info!(project_id = %project_id, user_id = %user_id, "project member removed");

    Ok(Json(serde_json::json!({"ok": true})))
}
