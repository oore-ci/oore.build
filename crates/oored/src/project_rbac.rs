use axum::Json;
use axum::http::StatusCode;
use oore_contract::{ApiError, ProjectRole};
use sqlx::Row;
use tracing::error;

use crate::session::AuthSource;
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
    /// Pause or halt a store release.
    OperateRelease,
    /// Create, raise, resume, complete or approve a store release.
    ManageReleases,
    /// Approve or reject a gated build, and change a pipeline's approval policy.
    ApproveBuild,
}

// ── Resolution ──────────────────────────────────────────────────

/// Apply instance-role limits to an existing project membership.
/// Owner/admin bypass belongs to callers; this also serves project-list responses.
pub(crate) fn effective_member_role(
    stored_role: ProjectRole,
    instance_role: &str,
    auth_source: &AuthSource,
) -> ProjectRole {
    if instance_role == "qa_viewer" {
        return ProjectRole::Viewer;
    }
    if *auth_source == AuthSource::ApiToken {
        return match (instance_role, stored_role) {
            ("developer", ProjectRole::Maintainer | ProjectRole::Developer) => {
                ProjectRole::Developer
            }
            _ => ProjectRole::Viewer,
        };
    }
    stored_role
}

/// Resolve the effective project role for a user on a given project.
///
/// Rules:
/// 1. Instance `owner` / `admin` → `InstanceAdmin` (bypass membership).
/// 2. Explicit `project_members` row → `Member(role)`.
/// 3. Otherwise → `None`.
///
/// QA sessions are always capped at Viewer. API tokens are capped at the
/// maximum implied by the token's instance role. This prevents a stale or
/// accidentally elevated membership from bypassing the instance-level role.
pub async fn resolve_effective_project_role(
    pool: &sqlx::SqlitePool,
    user_id: &str,
    instance_role: &str,
    project_id: &str,
    auth_source: &AuthSource,
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

        let effective = effective_member_role(role, instance_role, auth_source);

        return Ok(EffectiveProjectRole::Member(effective));
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
                | ProjectPermission::OperateRelease
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

/// Resolve project membership for a direct pipeline-id route and require the
/// requested project permission. Returns the pipeline's project id.
pub async fn require_pipeline_project_permission(
    pool: &sqlx::SqlitePool,
    user_id: &str,
    instance_role: &str,
    auth_source: &AuthSource,
    pipeline_id: &str,
    permission: ProjectPermission,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM pipelines WHERE id = ?1")
        .bind(pipeline_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to query pipeline project");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to check pipeline access",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Pipeline not found"))?;

    let effective =
        resolve_effective_project_role(pool, user_id, instance_role, &project_id, auth_source)
            .await?;
    require_project_permission(&effective, permission)?;
    Ok(project_id)
}

/// Return the project role string for inclusion in API responses.
pub fn effective_role_string(effective_role: &EffectiveProjectRole) -> Option<String> {
    match effective_role {
        EffectiveProjectRole::InstanceAdmin => Some("maintainer".to_string()),
        EffectiveProjectRole::Member(role) => Some(role.to_string()),
        EffectiveProjectRole::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn membership_limits_match_project_responses_and_authorization() {
        use AuthSource::{ApiToken, Session};
        use ProjectRole::{Developer, Maintainer, Viewer};

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE project_members (project_id TEXT, user_id TEXT, role TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO project_members VALUES ('project', 'member', 'viewer')")
            .execute(&pool)
            .await
            .unwrap();

        for (instance_role, source, expected) in [
            ("developer", Session, [Maintainer, Developer, Viewer]),
            ("developer", ApiToken, [Developer, Developer, Viewer]),
            ("qa_viewer", Session, [Viewer, Viewer, Viewer]),
            ("qa_viewer", ApiToken, [Viewer, Viewer, Viewer]),
            ("unknown", Session, [Maintainer, Developer, Viewer]),
            ("unknown", ApiToken, [Viewer, Viewer, Viewer]),
        ] {
            for (stored, expected) in [Maintainer, Developer, Viewer].into_iter().zip(expected) {
                sqlx::query("UPDATE project_members SET role = ?1")
                    .bind(stored.to_string())
                    .execute(&pool)
                    .await
                    .unwrap();
                assert_eq!(
                    effective_member_role(stored, instance_role, &source),
                    expected
                );
                assert_eq!(
                    resolve_effective_project_role(
                        &pool,
                        "member",
                        instance_role,
                        "project",
                        &source
                    )
                    .await
                    .unwrap(),
                    EffectiveProjectRole::Member(expected)
                );
            }
            assert_eq!(
                resolve_effective_project_role(
                    &pool,
                    "non-member",
                    instance_role,
                    "project",
                    &source
                )
                .await
                .unwrap(),
                EffectiveProjectRole::None
            );
        }
        for instance_role in ["owner", "admin"] {
            for source in [Session, ApiToken] {
                assert_eq!(
                    resolve_effective_project_role(
                        &pool,
                        "non-member",
                        instance_role,
                        "project",
                        &source
                    )
                    .await
                    .unwrap(),
                    EffectiveProjectRole::InstanceAdmin
                );
            }
        }
    }
}
