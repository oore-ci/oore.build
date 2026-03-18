use axum::Json;
use axum::http::StatusCode;
use oore_contract::{ApiError, ProjectRole};
use sqlx::Row;
use tracing::error;

use crate::util::api_err;

// ── Effective project role ──────────────────────────────────────

/// The resolved permission level a user has on a specific project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectiveProjectRole {
    /// Instance owner or admin — implicit full access to every project.
    InstanceAdmin,
    /// Explicit membership in the project.
    Member(ProjectRole),
    /// No access.
    None,
}

// ── Project permissions ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectPermission {
    Read,
    Write,
    Delete,
    ManageMembers,
    ManagePipelines,
    TriggerBuild,
    CancelBuild,
    ReadArtifacts,
    WriteArtifacts,
    DeleteArtifacts,
}

// ── Resolution ──────────────────────────────────────────────────

/// Resolve the effective project role for a user on a given project.
///
/// Rules:
/// 1. Instance `owner` / `admin` → `InstanceAdmin` (bypass membership).
/// 2. Explicit `project_members` row → `Member(role)`.
/// 3. Otherwise → `None`.
pub async fn resolve_effective_project_role(
    pool: &sqlx::SqlitePool,
    user_id: &str,
    instance_role: &str,
    project_id: &str,
) -> Result<EffectiveProjectRole, (StatusCode, Json<ApiError>)> {
    if instance_role == "owner" || instance_role == "admin" {
        return Ok(EffectiveProjectRole::InstanceAdmin);
    }

    let row =
        sqlx::query("SELECT role FROM project_members WHERE project_id = ?1 AND user_id = ?2")
            .bind(project_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to query project membership");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to check project membership",
                )
            })?;

    if let Some(row) = row {
        let role_str: String = row.get("role");
        let role: ProjectRole = role_str.parse().map_err(|_| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "data_error",
                "Invalid project role in database",
            )
        })?;
        return Ok(EffectiveProjectRole::Member(role));
    }

    Ok(EffectiveProjectRole::None)
}

// ── Permission check ────────────────────────────────────────────

/// Check whether the effective project role grants the requested permission.
pub fn check_project_permission(
    effective_role: &EffectiveProjectRole,
    permission: ProjectPermission,
) -> bool {
    match effective_role {
        EffectiveProjectRole::InstanceAdmin => true,
        EffectiveProjectRole::Member(role) => role_has_permission(role, permission),
        EffectiveProjectRole::None => false,
    }
}

fn role_has_permission(role: &ProjectRole, perm: ProjectPermission) -> bool {
    match role {
        ProjectRole::Maintainer => true,
        ProjectRole::Developer => matches!(
            perm,
            ProjectPermission::Read
                | ProjectPermission::ManagePipelines
                | ProjectPermission::TriggerBuild
                | ProjectPermission::CancelBuild
                | ProjectPermission::ReadArtifacts
                | ProjectPermission::WriteArtifacts
        ),
        ProjectRole::Viewer => matches!(
            perm,
            ProjectPermission::Read | ProjectPermission::ReadArtifacts
        ),
    }
}

// ── Convenience: require permission or 404 ──────────────────────

/// Require a specific project permission, returning 404 if the user has no
/// access at all (to avoid leaking project existence) or 403 if they have
/// access but lack the specific permission.
pub fn require_project_permission(
    effective_role: &EffectiveProjectRole,
    permission: ProjectPermission,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    if *effective_role == EffectiveProjectRole::None {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Project not found",
        ));
    }
    if check_project_permission(effective_role, permission) {
        Ok(())
    } else {
        Err(api_err(
            StatusCode::FORBIDDEN,
            "permission_denied",
            "You do not have permission to perform this action on this project",
        ))
    }
}

/// Return the project role string for inclusion in API responses.
pub fn effective_role_string(effective_role: &EffectiveProjectRole) -> Option<String> {
    match effective_role {
        EffectiveProjectRole::InstanceAdmin => Some("maintainer".to_string()),
        EffectiveProjectRole::Member(role) => Some(role.to_string()),
        EffectiveProjectRole::None => None,
    }
}
