use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use axum::Json;
use axum::extract::{ConnectInfo, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use oore_contract::{
    ApiError, BrowseLocalGitDirectoriesResponse, CreateLocalGitIntegrationRequest,
    CreateLocalGitIntegrationResponse, Integration, IntegrationRepository,
    ListIntegrationsResponse, LocalGitDirectoryEntry, LocalGitPathSuggestion,
};
use serde::Deserialize;
use sqlx::Row;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::row_to_integration;
use crate::AppState;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;
const MAX_BROWSE_DIRECTORY_ENTRIES: usize = 300;

fn normalize_repo_path(raw: &str) -> Result<PathBuf, (StatusCode, Json<ApiError>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "repository_path is required",
        ));
    }

    let candidate = PathBuf::from(trimmed);
    if !candidate.is_absolute() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "repository_path must be an absolute path",
        ));
    }

    std::fs::canonicalize(&candidate).map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "repository_path does not exist or is not accessible",
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
        "repository_path is not a valid git repository",
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

struct LocalGitRepoInspection {
    canonical_str: String,
    default_branch: Option<String>,
    repo_name: String,
}

async fn inspect_local_git_repo(
    raw_path: &str,
) -> Result<LocalGitRepoInspection, (StatusCode, Json<ApiError>)> {
    let raw_path = raw_path.to_string();
    tokio::task::spawn_blocking(move || {
        let canonical_path = normalize_repo_path(&raw_path)?;
        assert_git_repo(&canonical_path)?;

        let default_branch = resolve_default_branch(&canonical_path);
        let canonical_str = canonical_path.to_string_lossy().into_owned();
        let repo_name = canonical_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "local-repo".to_string());

        Ok(LocalGitRepoInspection {
            canonical_str,
            default_branch,
            repo_name,
        })
    })
    .await
    .map_err(|e| {
        error!(error = %e, "local git repository inspection task panicked or was cancelled");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to inspect repository_path",
        )
    })?
}

#[derive(Debug, Deserialize)]
pub struct BrowseLocalGitDirectoriesQuery {
    pub path: Option<String>,
}

fn canonicalize_directory(raw: &str) -> Result<PathBuf, (StatusCode, Json<ApiError>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "path must not be empty",
        ));
    }

    let candidate = PathBuf::from(trimmed);
    if !candidate.is_absolute() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "path must be an absolute path",
        ));
    }

    let canonical = std::fs::canonicalize(&candidate).map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "path does not exist or is not accessible",
        )
    })?;
    if !canonical.is_dir() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "path must point to a directory",
        ));
    }
    Ok(canonical)
}

fn looks_like_git_repository(path: &std::path::Path) -> bool {
    let dot_git = path.join(".git");
    dot_git.is_dir() || dot_git.is_file()
}

fn build_browse_suggestions(current_path: &std::path::Path) -> Vec<LocalGitPathSuggestion> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    let mut push_if_dir = |label: &str, candidate: PathBuf| {
        let canonical = match std::fs::canonicalize(&candidate) {
            Ok(value) => value,
            Err(_) => return,
        };
        if !canonical.is_dir() {
            return;
        }
        let path = canonical.to_string_lossy().into_owned();
        if seen.insert(path.clone()) {
            out.push(LocalGitPathSuggestion {
                label: label.to_string(),
                path,
            });
        }
    };

    if let Some(home) = dirs::home_dir() {
        push_if_dir("Home", home.clone());
        push_if_dir("Desktop", home.join("Desktop"));
        push_if_dir("Documents", home.join("Documents"));
        push_if_dir("Downloads", home.join("Downloads"));
        push_if_dir("Code", home.join("Code"));
        push_if_dir("Projects", home.join("Projects"));
    }

    push_if_dir("Current", current_path.to_path_buf());
    out
}

/// `GET /v1/integrations/local-git/directories`
///
/// Lists child directories for local filesystem browsing.
pub async fn browse_local_git_directories(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    auth: AuthUser,
    Query(params): Query<BrowseLocalGitDirectoriesQuery>,
) -> ApiResult<BrowseLocalGitDirectoriesResponse> {
    if check_permission(&state.enforcer, &auth.0.role, "integrations", "write")
        .await
        .is_err()
    {
        check_permission(&state.enforcer, &auth.0.role, "projects", "write").await?;
    }

    let effective_ip = crate::effective_client_ip(peer_addr, &headers);
    if !effective_ip.is_loopback() {
        warn!(
            peer_ip = %peer_addr.ip(),
            source_ip = %effective_ip,
            "blocked local git directory browsing from non-loopback client"
        );
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "loopback_required",
            "Local filesystem browsing is only available from loopback clients",
        ));
    }

    let requested_path = params.path.clone();
    let response = tokio::task::spawn_blocking(move || {
        let current_path = if let Some(path) = requested_path.as_deref() {
            canonicalize_directory(path)?
        } else {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
            std::fs::canonicalize(&home).unwrap_or(home)
        };

        let read_dir = std::fs::read_dir(&current_path).map_err(|_| {
            api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "path does not exist or is not accessible",
            )
        })?;

        let mut directories = Vec::new();
        for entry_result in read_dir {
            let entry = match entry_result {
                Ok(value) => value,
                Err(_) => continue,
            };
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with('.') {
                continue;
            }

            let candidate = entry.path();
            let metadata = match std::fs::metadata(&candidate) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !metadata.is_dir() {
                continue;
            }

            let canonical = std::fs::canonicalize(&candidate).unwrap_or(candidate);
            directories.push(LocalGitDirectoryEntry {
                name: file_name,
                path: canonical.to_string_lossy().into_owned(),
                is_git_repository: looks_like_git_repository(&canonical),
            });

            if directories.len() >= MAX_BROWSE_DIRECTORY_ENTRIES {
                break;
            }
        }

        directories.sort_by(|a, b| {
            b.is_git_repository
                .cmp(&a.is_git_repository)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        let current_path_str = current_path.to_string_lossy().into_owned();
        let parent_path = current_path
            .parent()
            .map(|path| path.to_string_lossy().into_owned())
            .filter(|path| path != &current_path_str && !path.is_empty());

        Ok(BrowseLocalGitDirectoriesResponse {
            current_path: current_path_str,
            current_is_git_repository: looks_like_git_repository(&current_path),
            parent_path,
            directories,
            suggestions: build_browse_suggestions(&current_path),
        })
    })
    .await
    .map_err(|e| {
        error!(error = %e, "local git directory browsing task panicked or was cancelled");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to browse local filesystem",
        )
    })??;

    Ok(Json(response))
}

/// `POST /v1/integrations/local-git`
///
/// Registers a local filesystem git repository as an integration source.
pub async fn create_local_git_integration(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateLocalGitIntegrationRequest>,
) -> ApiResult<CreateLocalGitIntegrationResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "write").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let inspection = inspect_local_git_repo(&req.repository_path).await?;
    let canonical_str = inspection.canonical_str.clone();
    let default_branch = inspection.default_branch;
    let repo_name = inspection.repo_name;
    let display_name = req
        .display_name
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| repo_name.clone());

    let existing_id: Option<String> = sqlx::query_scalar(
        "SELECT i.id FROM integrations i \
         JOIN integration_installations inst ON inst.integration_id = i.id \
         JOIN integration_repositories r ON r.installation_id = inst.id \
         WHERE i.provider = 'local_git' AND r.external_id = ?1 \
         LIMIT 1",
    )
    .bind(&canonical_str)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to query existing local git integration");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to check existing integrations",
        )
    })?;
    if let Some(integration_id) = existing_id {
        return Err(api_err(
            StatusCode::CONFLICT,
            "already_exists",
            format!("Repository already connected via integration {integration_id}"),
        ));
    }

    let now = now_unix();
    let integration_id = Uuid::new_v4().to_string();
    let installation_id = Uuid::new_v4().to_string();
    let repository_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO integrations (id, provider, host_url, auth_mode, status, display_name, created_by, created_at, updated_at) \
         VALUES (?1, 'local_git', 'local://filesystem', 'local_path', 'active', ?2, ?3, ?4, ?4)",
    )
    .bind(&integration_id)
    .bind(&display_name)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to create local git integration");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to create integration",
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
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to create local git installation");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to create integration installation",
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
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to create local git repository entry");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to register repository",
        )
    })?;

    let details = serde_json::json!({
        "provider": "local_git",
        "repository_path": canonical_str,
        "created_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "integration_created",
        "integration",
        Some(&integration_id),
        Some(&details),
    )
    .await;

    info!(
        integration_id = %integration_id,
        repo_name = %repo_name,
        "local git integration created"
    );

    let integration = Integration {
        id: integration_id,
        provider: "local_git".to_string(),
        host_url: "local://filesystem".to_string(),
        auth_mode: "local_path".to_string(),
        status: "active".to_string(),
        display_name: Some(display_name),
        app_id: None,
        app_slug: None,
        created_by: auth.0.user_id,
        created_at: now,
        updated_at: now,
    };
    let repository = IntegrationRepository {
        id: repository_id,
        installation_id,
        external_id: canonical_str.clone(),
        full_name: repo_name,
        default_branch,
        is_private: true,
        created_at: now,
        updated_at: now,
    };

    Ok(Json(CreateLocalGitIntegrationResponse {
        integration,
        repository,
    }))
}

/// `GET /v1/integrations/local-git`
///
/// Lists local git integrations.
pub async fn list_local_git_integrations(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<ListIntegrationsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "read").await?;
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let rows = sqlx::query(
        "SELECT * FROM integrations WHERE provider = 'local_git' ORDER BY created_at DESC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to list local git integrations");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to list integrations",
        )
    })?;
    let total = i64::try_from(rows.len()).unwrap_or(0);
    let integrations = rows.iter().map(row_to_integration).collect::<Vec<_>>();

    Ok(Json(ListIntegrationsResponse {
        integrations,
        total,
    }))
}

/// `DELETE /v1/integrations/local-git/{id}`
///
/// Deletes a local git integration.
pub async fn delete_local_git_integration(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<serde_json::Value> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "delete").await?;
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let row = sqlx::query(
        "SELECT display_name FROM integrations WHERE id = ?1 AND provider = 'local_git'",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to query local git integration");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to query integration",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Local git integration not found",
        )
    })?;

    let display_name: Option<String> = row.get("display_name");

    sqlx::query("DELETE FROM integrations WHERE id = ?1")
        .bind(&id)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to delete local git integration");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to delete integration",
            )
        })?;

    let details = serde_json::json!({
        "provider": "local_git",
        "display_name": display_name,
        "deleted_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "integration_deleted",
        "integration",
        Some(&id),
        Some(&details),
    )
    .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}
