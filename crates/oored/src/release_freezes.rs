//! Release freezes: hold store uploads, rollout changes that reach more
//! users, and promotions for one app or the whole instance, with a reason
//! everyone sees.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{ApiError, ListReleaseFreezesResponse, ReleaseFreeze, SetReleaseFreezeRequest};
use sqlx::{Row, SqlitePool};
use tracing::error;
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{ProjectPermission, require_project_permission};
use crate::releases::project_role;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiFailure = (StatusCode, Json<ApiError>);
type ApiResult<T> = Result<Json<T>, ApiFailure>;

const MAX_REASON_CHARS: usize = 500;

fn store_err(message: &'static str) -> impl FnOnce(sqlx::Error) -> ApiFailure {
    move |e| {
        error!(error = %e, "{message}");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", message)
    }
}

const SELECT: &str = "SELECT f.id, f.project_id, p.name AS project_name, f.reason, \
     u.email AS created_by_email, f.created_at \
     FROM release_freezes f \
     LEFT JOIN projects p ON p.id = f.project_id \
     LEFT JOIN users u ON u.id = f.created_by";

fn freeze_from_row(row: &sqlx::sqlite::SqliteRow) -> ReleaseFreeze {
    ReleaseFreeze {
        id: row.get("id"),
        project_id: row.get("project_id"),
        project_name: row.get("project_name"),
        reason: row.get("reason"),
        created_by_email: row.get("created_by_email"),
        created_at: row.get("created_at"),
    }
}

/// The freeze that holds a project's releases: its own, else the
/// instance's.
pub async fn active_freeze(
    pool: &SqlitePool,
    project_id: &str,
) -> sqlx::Result<Option<ReleaseFreeze>> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{SELECT} WHERE f.project_id = ?1 OR f.project_id IS NULL \
         ORDER BY f.project_id IS NULL LIMIT 1"
    )))
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(freeze_from_row))
}

/// The 409 an action a freeze holds answers with.
pub fn frozen_error(freeze: &ReleaseFreeze) -> ApiFailure {
    let scope = match &freeze.project_name {
        Some(name) => format!("{name} is frozen"),
        None => "Releases are frozen".to_string(),
    };
    api_err(
        StatusCode::CONFLICT,
        "release_frozen",
        format!("{scope}: {}. Lift the freeze to continue", freeze.reason),
    )
}

/// Fails with [`frozen_error`] while a freeze holds the project.
pub async fn ensure_not_frozen(pool: &SqlitePool, project_id: &str) -> Result<(), ApiFailure> {
    match active_freeze(pool, project_id)
        .await
        .map_err(store_err("Failed to load release freezes"))?
    {
        Some(freeze) => Err(frozen_error(&freeze)),
        None => Ok(()),
    }
}

fn is_instance_admin(auth: &AuthUser) -> bool {
    auth.0.role == "owner" || auth.0.role == "admin"
}

/// `GET /v1/release-freezes` — the instance freeze and the freezes of apps
/// the caller can see.
pub async fn list_release_freezes(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<ListReleaseFreezesResponse> {
    let rows = if is_instance_admin(&auth) {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "{SELECT} ORDER BY f.created_at DESC"
        )))
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "{SELECT} WHERE f.project_id IS NULL OR EXISTS (SELECT 1 FROM project_members pm \
             WHERE pm.project_id = f.project_id AND pm.user_id = ?1) \
             ORDER BY f.created_at DESC"
        )))
        .bind(&auth.0.user_id)
        .fetch_all(&state.db)
        .await
    }
    .map_err(store_err("Failed to load release freezes"))?;
    Ok(Json(ListReleaseFreezesResponse {
        freezes: rows.iter().map(freeze_from_row).collect(),
    }))
}

/// `PUT /v1/release-freezes` — freeze one app, or every app with no
/// `project_id`. Freezing again replaces the reason.
pub async fn set_release_freeze(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(request): Json<SetReleaseFreezeRequest>,
) -> ApiResult<ReleaseFreeze> {
    let reason = request.reason.trim();
    if reason.is_empty() || reason.chars().count() > MAX_REASON_CHARS {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "reason must be 1 to 500 characters",
        ));
    }
    match &request.project_id {
        Some(project_id) => {
            let role = project_role(&state, &auth, project_id).await?;
            require_project_permission(&role, ProjectPermission::ManageReleases)?;
        }
        None if !is_instance_admin(&auth) => {
            return Err(api_err(
                StatusCode::FORBIDDEN,
                "forbidden",
                "Only an instance owner or admin can freeze every app",
            ));
        }
        None => {}
    }
    let scope = request
        .project_id
        .clone()
        .unwrap_or_else(|| "*".to_string());
    let id = format!("freeze_{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO release_freezes (id, project_id, scope, reason, created_by, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT (scope) DO UPDATE SET reason = excluded.reason, \
         created_by = excluded.created_by, created_at = excluded.created_at",
    )
    .bind(&id)
    .bind(&request.project_id)
    .bind(&scope)
    .bind(reason)
    .bind(&auth.0.user_id)
    .bind(now_unix())
    .execute(&state.db)
    .await
    .map_err(store_err("Failed to save the release freeze"))?;
    let row = sqlx::query(sqlx::AssertSqlSafe(format!("{SELECT} WHERE f.scope = ?1")))
        .bind(&scope)
        .fetch_one(&state.db)
        .await
        .map_err(store_err("Failed to load the release freeze"))?;
    let freeze = freeze_from_row(&row);
    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        "release_freeze_set",
        if request.project_id.is_some() {
            "project"
        } else {
            "instance"
        },
        request.project_id.as_deref(),
        Some(&serde_json::json!({ "reason": reason }).to_string()),
    )
    .await;
    Ok(Json(freeze))
}

/// `DELETE /v1/release-freezes/{freeze_id}` — lift a freeze.
pub async fn delete_release_freeze(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(freeze_id): Path<String>,
) -> Result<StatusCode, ApiFailure> {
    let project_id: Option<Option<String>> =
        sqlx::query_scalar("SELECT project_id FROM release_freezes WHERE id = ?1")
            .bind(&freeze_id)
            .fetch_optional(&state.db)
            .await
            .map_err(store_err("Failed to load the release freeze"))?;
    let Some(project_id) = project_id else {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Freeze not found",
        ));
    };
    match &project_id {
        Some(project_id) => {
            let role = project_role(&state, &auth, project_id).await?;
            require_project_permission(&role, ProjectPermission::Read)
                .map_err(|_| api_err(StatusCode::NOT_FOUND, "not_found", "Freeze not found"))?;
            require_project_permission(&role, ProjectPermission::ManageReleases)?;
        }
        None if !is_instance_admin(&auth) => {
            return Err(api_err(
                StatusCode::FORBIDDEN,
                "forbidden",
                "Only an instance owner or admin can lift a freeze of every app",
            ));
        }
        None => {}
    }
    sqlx::query("DELETE FROM release_freezes WHERE id = ?1")
        .bind(&freeze_id)
        .execute(&state.db)
        .await
        .map_err(store_err("Failed to lift the release freeze"))?;
    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        "release_freeze_lifted",
        if project_id.is_some() {
            "project"
        } else {
            "instance"
        },
        project_id.as_deref(),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'a@example.com', 's1', 'owner', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p1', 'Shop', 'u1', 0, 0), ('p2', 'Pay', 'u1', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    async fn freeze(pool: &SqlitePool, project_id: Option<&str>, reason: &str) {
        sqlx::query(
            "INSERT INTO release_freezes (id, project_id, scope, reason, created_at) \
             VALUES (?1, ?2, ?3, ?4, 1)",
        )
        .bind(format!("f-{}", project_id.unwrap_or("all")))
        .bind(project_id)
        .bind(project_id.unwrap_or("*"))
        .bind(reason)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn an_app_freeze_holds_only_that_app() {
        let pool = pool().await;
        assert!(active_freeze(&pool, "p1").await.unwrap().is_none());
        freeze(&pool, Some("p1"), "Sale weekend").await;
        let held = active_freeze(&pool, "p1").await.unwrap().unwrap();
        assert_eq!(held.reason, "Sale weekend");
        assert_eq!(held.project_name.as_deref(), Some("Shop"));
        assert!(active_freeze(&pool, "p2").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn the_instance_freeze_holds_every_app_and_the_app_one_wins() {
        let pool = pool().await;
        freeze(&pool, None, "Holiday").await;
        assert_eq!(
            active_freeze(&pool, "p2").await.unwrap().unwrap().reason,
            "Holiday"
        );
        freeze(&pool, Some("p1"), "Audit").await;
        assert_eq!(
            active_freeze(&pool, "p1").await.unwrap().unwrap().reason,
            "Audit"
        );
        let error = frozen_error(&active_freeze(&pool, "p2").await.unwrap().unwrap());
        assert_eq!(error.0, StatusCode::CONFLICT);
        assert_eq!(error.1.code, "release_frozen");
    }
}
