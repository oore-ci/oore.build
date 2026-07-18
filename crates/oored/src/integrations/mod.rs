pub mod github;
pub mod gitlab;
pub mod local_git;
pub mod webhooks;

use std::sync::Arc;

const FAVICON_DATA_URI: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAzMiAzMiI+CiAgPHN0eWxlPgogICAgLmZpbGwtYnJhbmQgeyBmaWxsOiAjMjQ1N2M1OyB9CiAgPC9zdHlsZT4KICA8ZGVmcz4KICAgIDxjaXJjbGUgaWQ9ImN1dCIgY3g9IjE2IiBjeT0iMTYiIHI9IjciIC8+CiAgICA8bWFzayBpZD0iaG9sZSI+CiAgICAgIDxyZWN0IHg9IjAiIHk9IjAiIHdpZHRoPSIzMiIgaGVpZ2h0PSIzMiIgZmlsbD0id2hpdGUiIC8+CiAgICAgIDx1c2UgaHJlZj0iI2N1dCIgZmlsbD0iYmxhY2siIC8+CiAgICA8L21hc2s+CiAgICA8Y2xpcFBhdGggaWQ9ImxlZnQiPgogICAgICA8cmVjdCB4PSIwIiB5PSIwIiB3aWR0aD0iMTUiIGhlaWdodD0iMzIiIC8+CiAgICA8L2NsaXBQYXRoPgogICAgPGNsaXBQYXRoIGlkPSJyaWdodCI+CiAgICAgIDxyZWN0IHg9IjE3IiB5PSIwIiB3aWR0aD0iMTUiIGhlaWdodD0iMzIiIC8+CiAgICA8L2NsaXBQYXRoPgogIDwvZGVmcz4KICA8cmVjdAogICAgeD0iMiIKICAgIHk9IjIiCiAgICB3aWR0aD0iMjgiCiAgICBoZWlnaHQ9IjI4IgogICAgcng9IjYiCiAgICBjbGFzcz0iZmlsbC1icmFuZCIKICAgIGNsaXAtcGF0aD0idXJsKCNsZWZ0KSIKICAgIG1hc2s9InVybCgjaG9sZSkiCiAgLz4KICA8cmVjdAogICAgeD0iMiIKICAgIHk9IjIiCiAgICB3aWR0aD0iMjgiCiAgICBoZWlnaHQ9IjI4IgogICAgcng9IjYiCiAgICBjbGFzcz0iZmlsbC1icmFuZCIKICAgIGNsaXAtcGF0aD0idXJsKCNyaWdodCkiCiAgICBtYXNrPSJ1cmwoI2hvbGUpIgogIC8+Cjwvc3ZnPgo=";

// ── Shared HTML helpers ──────────────────────────────────────────

/// Escape HTML special characters in a string.
pub(crate) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(crate) fn favicon_data_uri() -> &'static str {
    FAVICON_DATA_URI
}

/// Render a simple error HTML page.
pub(crate) fn error_page(title: &str, message: &str) -> String {
    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <link rel="icon" href="{favicon}">
  <link rel="apple-touch-icon" href="{favicon}">
  <meta name="theme-color" content="#2457c5">
  <title>{title}</title>
  <style>
    body {{
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      display: flex; align-items: center; justify-content: center;
      min-height: 100vh; margin: 0; background: #0a0a0a; color: #fafafa;
    }}
    .container {{ text-align: center; max-width: 400px; padding: 24px; }}
    h1 {{ font-size: 1.25rem; margin-bottom: 8px; }}
    p {{ color: #a1a1a1; font-size: 0.875rem; }}
  </style>
</head>
<body>
  <div class="container">
    <h1>{title}</h1>
    <p>{message}</p>
  </div>
</body>
</html>"##,
        favicon = favicon_data_uri(),
        title = html_escape(title),
        message = html_escape(message),
    )
}

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use oore_contract::{
    ApiError, Integration, IntegrationDetailResponse, IntegrationInstallation,
    IntegrationRepository, ListInstallationsResponse, ListIntegrationsResponse,
    ListRepositoriesResponse, RuntimeMode, SyncInstallationsRequest, SyncInstallationsResponse,
    UpdateRepositoryRunnerPolicyRequest, UpdateRepositoryRunnerPolicyResponse,
};
use serde::Deserialize;
use sqlx::Row;
use tracing::{error, info};

use crate::AppState;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

pub(crate) struct CommitSummary {
    pub title: String,
    pub author: String,
}

pub(crate) async fn require_remote_mode(
    pool: &sqlx::SqlitePool,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let mode = crate::instance_settings::load_runtime_mode(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load runtime mode");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to determine runtime mode",
            )
        })?;

    if mode != RuntimeMode::Remote {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "mode_restricted",
            "Remote mode is required for GitHub/GitLab integrations",
        ));
    }

    Ok(())
}

pub(crate) async fn resolve_branch_commit(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    repository_id: &str,
    branch: &str,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let row = sqlx::query(
        "SELECT i.id AS integration_id, i.provider, i.host_url, i.auth_mode, \
                inst.external_id AS installation_external_id, r.external_id AS repository_external_id, \
                r.full_name, r.html_url \
         FROM integration_repositories r \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         JOIN integrations i ON i.id = inst.integration_id \
         WHERE r.id = ?1 AND i.status = 'active'",
    )
    .bind(repository_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, repository_id = %repository_id, "failed to load source revision target");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load the linked repository",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::CONFLICT,
            "source_unresolvable",
            "The linked source is unavailable; reconnect it before triggering a build",
        )
    })?;

    let provider: String = row.get("provider");
    let integration_id: String = row.get("integration_id");
    match provider.as_str() {
        "local_git" => {
            let path: Option<String> = row.get("html_url");
            local_git::resolve_branch_commit(
                path.as_deref().ok_or_else(|| {
                    api_err(
                        StatusCode::CONFLICT,
                        "source_unresolvable",
                        "The linked local repository path is missing",
                    )
                })?,
                branch,
            )
            .await
        }
        "github" => {
            let installation_id: String = row.get("installation_external_id");
            let full_name: String = row.get("full_name");
            github::resolve_branch_commit(
                pool,
                encryption_key,
                &integration_id,
                &installation_id,
                &full_name,
                branch,
            )
            .await
        }
        "gitlab" => {
            let host_url: String = row.get("host_url");
            let auth_mode: String = row.get("auth_mode");
            let repository_external_id: String = row.get("repository_external_id");
            gitlab::resolve_branch_commit(
                pool,
                encryption_key,
                &integration_id,
                &host_url,
                &auth_mode,
                &repository_external_id,
                branch,
            )
            .await
        }
        _ => Err(api_err(
            StatusCode::CONFLICT,
            "unsupported_provider",
            "The linked source provider cannot resolve branch revisions",
        )),
    }
}

pub(crate) async fn compare_commits(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    repository_id: &str,
    base: &str,
    head: &str,
) -> Result<Vec<CommitSummary>, (StatusCode, Json<ApiError>)> {
    let row = sqlx::query(
        "SELECT i.id AS integration_id, i.provider, i.host_url, i.auth_mode, \
                inst.external_id AS installation_external_id, r.external_id AS repository_external_id, \
                r.full_name, r.html_url \
         FROM integration_repositories r \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         JOIN integrations i ON i.id = inst.integration_id \
         WHERE r.id = ?1 AND i.status = 'active'",
    )
    .bind(repository_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, repository_id = %repository_id, "failed to load source comparison target");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load the linked repository",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::CONFLICT,
            "source_unresolvable",
            "The linked source is unavailable; reconnect it before triggering a build",
        )
    })?;

    let provider: String = row.get("provider");
    let integration_id: String = row.get("integration_id");
    match provider.as_str() {
        "local_git" => {
            let path: Option<String> = row.get("html_url");
            local_git::compare_commits(
                path.as_deref().ok_or_else(|| {
                    api_err(
                        StatusCode::CONFLICT,
                        "source_unresolvable",
                        "The linked local repository path is missing",
                    )
                })?,
                base,
                head,
            )
            .await
        }
        "github" => {
            github::compare_commits(
                pool,
                encryption_key,
                &integration_id,
                &row.get::<String, _>("installation_external_id"),
                &row.get::<String, _>("full_name"),
                base,
                head,
            )
            .await
        }
        "gitlab" => {
            gitlab::compare_commits(
                pool,
                encryption_key,
                &integration_id,
                (
                    &row.get::<String, _>("host_url"),
                    &row.get::<String, _>("auth_mode"),
                    &row.get::<String, _>("repository_external_id"),
                ),
                base,
                head,
            )
            .await
        }
        _ => Err(api_err(
            StatusCode::CONFLICT,
            "unsupported_provider",
            "The linked source provider cannot compare revisions",
        )),
    }
}

// ── Row conversion helpers ──────────────────────────────────────

pub fn row_to_integration(row: &sqlx::sqlite::SqliteRow) -> Integration {
    Integration {
        id: row.get("id"),
        provider: row.get("provider"),
        host_url: row.get("host_url"),
        auth_mode: row.get("auth_mode"),
        status: row.get("status"),
        display_name: row.get("display_name"),
        app_id: row.get("app_id"),
        app_slug: row.get("app_slug"),
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub fn row_to_installation(row: &sqlx::sqlite::SqliteRow) -> IntegrationInstallation {
    IntegrationInstallation {
        id: row.get("id"),
        integration_id: row.get("integration_id"),
        external_id: row.get("external_id"),
        account_name: row.get("account_name"),
        account_type: row.get("account_type"),
        created_at: row.get("created_at"),
    }
}

pub fn row_to_repository(row: &sqlx::sqlite::SqliteRow) -> IntegrationRepository {
    IntegrationRepository {
        id: row.get("id"),
        installation_id: row.get("installation_id"),
        external_id: row.get("external_id"),
        full_name: row.get("full_name"),
        default_branch: row.get("default_branch"),
        is_private: row.get::<i32, _>("is_private") != 0,
        allow_direct_macos_runner: row.get::<i32, _>("allow_direct_macos_runner") != 0,
        avatar_url: row.get("avatar_url"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ── Query params ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListIntegrationsQuery {
    pub provider: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ListReposQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ── Common CRUD handlers ────────────────────────────────────────

/// `GET /v1/integrations` — list all integrations (paginated, filterable).
pub async fn list_integrations(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListIntegrationsQuery>,
) -> ApiResult<ListIntegrationsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let (rows, total) = if let Some(ref provider) = params.provider {
        let rows = sqlx::query(
            "SELECT * FROM integrations WHERE provider = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
        )
        .bind(provider)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to list integrations");
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to list integrations")
        })?;

        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM integrations WHERE provider = ?1")
                .bind(provider)
                .fetch_one(pool)
                .await
                .map_err(|e| {
                    error!(error = %e, "failed to count integrations");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "store_error",
                        "Failed to count integrations",
                    )
                })?;

        (rows, total)
    } else {
        let rows =
            sqlx::query("SELECT * FROM integrations ORDER BY created_at DESC LIMIT ?1 OFFSET ?2")
                .bind(limit)
                .bind(offset)
                .fetch_all(pool)
                .await
                .map_err(|e| {
                    error!(error = %e, "failed to list integrations");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "store_error",
                        "Failed to list integrations",
                    )
                })?;

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM integrations")
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to count integrations");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to count integrations",
                )
            })?;

        (rows, total)
    };

    let integrations = rows.iter().map(row_to_integration).collect();

    Ok(Json(ListIntegrationsResponse {
        integrations,
        total,
    }))
}

/// `GET /v1/integrations/{id}` — detail view with counts.
pub async fn get_integration(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<IntegrationDetailResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let row = sqlx::query("SELECT * FROM integrations WHERE id = ?1")
        .bind(&id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch integration");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch integration",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Integration not found"))?;

    let integration = row_to_integration(&row);

    let installation_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM integration_installations WHERE integration_id = ?1",
    )
    .bind(&id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let repository_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM integration_repositories r \
         JOIN integration_installations i ON i.id = r.installation_id \
         WHERE i.integration_id = ?1",
    )
    .bind(&id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let last_webhook_at: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(received_at) FROM integration_webhooks WHERE integration_id = ?1",
    )
    .bind(&id)
    .fetch_one(pool)
    .await
    .unwrap_or(None);

    Ok(Json(IntegrationDetailResponse {
        integration,
        installation_count,
        repository_count,
        last_webhook_at,
    }))
}

pub(crate) async fn delete_integration_records(
    pool: &sqlx::SqlitePool,
    id: &str,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE projects SET repository_id = NULL \
         WHERE repository_id IN (\
             SELECT r.id FROM integration_repositories r \
             JOIN integration_installations i ON i.id = r.installation_id \
             WHERE i.integration_id = ?1\
         )",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let deleted = sqlx::query("DELETE FROM integrations WHERE id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(deleted.rows_affected() > 0)
}

/// `DELETE /v1/integrations/{id}` — disconnect and cascade delete.
pub async fn delete_integration(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "delete").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    // Verify it exists
    let row = sqlx::query("SELECT provider, display_name FROM integrations WHERE id = ?1")
        .bind(&id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch integration");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch integration",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Integration not found"))?;

    let provider: String = row.get("provider");
    let display_name: Option<String> = row.get("display_name");

    // Credentials, installations, repositories, and webhooks cascade from the integration.
    delete_integration_records(pool, &id).await.map_err(|e| {
        error!(error = %e, "failed to delete integration");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete integration",
        )
    })?;

    let details = serde_json::json!({
        "provider": provider,
        "display_name": display_name,
        "deleted_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "integration_deleted",
        "integration",
        Some(&id),
        Some(&details),
    )
    .await;

    info!(integration_id = %id, provider = %provider, "integration deleted");

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `GET /v1/integrations/{id}/repositories` — list repos for an integration.
pub async fn list_repositories(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Query(params): Query<ListReposQuery>,
) -> ApiResult<ListRepositoriesResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    // Verify integration exists
    let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM integrations WHERE id = ?1")
        .bind(&id)
        .fetch_one(pool)
        .await
        .unwrap_or(false);

    if !exists {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Integration not found",
        ));
    }

    let limit = params.limit.unwrap_or(100).min(500);
    let offset = params.offset.unwrap_or(0);

    let rows = sqlx::query(
        "SELECT r.* FROM integration_repositories r \
         JOIN integration_installations i ON i.id = r.installation_id \
         WHERE i.integration_id = ?1 \
         ORDER BY r.full_name ASC \
         LIMIT ?2 OFFSET ?3",
    )
    .bind(&id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to list repositories");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to list repositories",
        )
    })?;

    let repositories = rows.iter().map(row_to_repository).collect();

    Ok(Json(ListRepositoriesResponse { repositories }))
}

/// `PUT /v1/integration-repositories/{id}/runner-policy` — update whether a
/// repository may execute builds on the direct macOS runner.
pub async fn update_repository_runner_policy(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateRepositoryRunnerPolicyRequest>,
) -> ApiResult<UpdateRepositoryRunnerPolicyResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "write").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    let existing = sqlx::query("SELECT * FROM integration_repositories WHERE id = ?1")
        .bind(&id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, repository_id = %id, "failed to load repository runner policy");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load repository runner policy",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Repository not found"))?;

    let previous = existing.get::<i32, _>("allow_direct_macos_runner") != 0;
    if previous != req.allow_direct_macos_runner {
        let now = now_unix();
        sqlx::query(
            "UPDATE integration_repositories \
             SET allow_direct_macos_runner = ?1, updated_at = ?2 WHERE id = ?3",
        )
        .bind(req.allow_direct_macos_runner)
        .bind(now)
        .bind(&id)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, repository_id = %id, "failed to update repository runner policy");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to update repository runner policy",
            )
        })?;

        let details = serde_json::json!({
            "previous_allow_direct_macos_runner": previous,
            "allow_direct_macos_runner": req.allow_direct_macos_runner,
            "updated_by": auth.0.email,
        })
        .to_string();
        let _ = write_audit_log(
            &pool,
            Some(&auth.0.user_id),
            "repository_runner_policy_updated",
            "integration_repository",
            Some(&id),
            Some(&details),
        )
        .await;

        info!(
            repository_id = %id,
            allow_direct_macos_runner = req.allow_direct_macos_runner,
            updated_by = %auth.0.email,
            "repository runner policy updated"
        );
    }

    let row = sqlx::query("SELECT * FROM integration_repositories WHERE id = ?1")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, repository_id = %id, "failed to reload repository runner policy");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load updated repository runner policy",
            )
        })?;

    Ok(Json(UpdateRepositoryRunnerPolicyResponse {
        repository: row_to_repository(&row),
    }))
}

/// `GET /v1/integration-repositories/{id}/avatar` — proxy a private GitLab avatar.
pub async fn repository_avatar(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "read").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    let row = sqlx::query(
        "SELECT i.id AS integration_id, i.host_url, i.auth_mode, r.external_id, r.avatar_url \
         FROM integration_repositories r \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         JOIN integrations i ON i.id = inst.integration_id \
         WHERE r.id = ?1 AND i.provider = 'gitlab' AND i.status = 'active'",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, repository_id = %id, "failed to load GitLab repository avatar");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load the repository avatar",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "GitLab repository avatar not found",
        )
    })?;

    let avatar_url: Option<String> = row.get("avatar_url");
    let avatar_url = avatar_url
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                "not_found",
                "GitLab repository avatar not found",
            )
        })?;
    let (content_type, body) = gitlab::fetch_repository_avatar(
        &pool,
        &state.encryption_key,
        row.get("integration_id"),
        row.get("host_url"),
        row.get("auth_mode"),
        row.get("external_id"),
        &avatar_url,
    )
    .await?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(Body::from(body))
        .map_err(|e| {
            error!(error = %e, "failed to build repository avatar response");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "response_error",
                "Failed to return the repository avatar",
            )
        })
}

/// `GET /v1/integrations/{id}/installations` — list installations for an integration.
pub async fn list_installations(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<ListInstallationsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    // Verify integration exists
    let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM integrations WHERE id = ?1")
        .bind(&id)
        .fetch_one(pool)
        .await
        .unwrap_or(false);

    if !exists {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Integration not found",
        ));
    }

    let rows = sqlx::query(
        "SELECT * FROM integration_installations WHERE integration_id = ?1 ORDER BY created_at DESC",
    )
    .bind(&id)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to list installations");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to list installations")
    })?;

    let installations = rows.iter().map(row_to_installation).collect();

    Ok(Json(ListInstallationsResponse { installations }))
}

/// `POST /v1/integrations/{id}/installations` — sync installations and repositories.
///
/// - **GitHub**: fetch GitHub App installations + repositories and upsert them.
/// - **GitLab**: refresh accessible projects for linked accounts and upsert them.
pub async fn sync_installations(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(_req): Json<SyncInstallationsRequest>,
) -> ApiResult<SyncInstallationsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "write").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    require_remote_mode(&pool).await?;

    let provider: Option<String> =
        sqlx::query_scalar("SELECT provider FROM integrations WHERE id = ?1")
            .bind(&id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to fetch integration provider");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to fetch integration",
                )
            })?;

    let provider = provider
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Integration not found"))?;

    let installations = match provider.as_str() {
        "github" => github::perform_sync(&pool, &state.encryption_key, &id)
            .await
            .map_err(|msg| {
                error!(error = %msg, "sync failed");
                api_err(StatusCode::BAD_GATEWAY, "sync_failed", msg)
            })?,
        "gitlab" => gitlab::perform_sync_installations(&pool, &state.encryption_key, &id).await?,
        other => {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_provider",
                format!("Unsupported integration provider: {other}"),
            ));
        }
    };

    Ok(Json(SyncInstallationsResponse { installations }))
}
