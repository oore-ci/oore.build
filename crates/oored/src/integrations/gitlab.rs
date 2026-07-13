use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, RawQuery, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use base64::Engine;
use oore_contract::{
    ApiError, GitLabAuthorizeRequest, GitLabAuthorizeResponse, GitLabCompleteResponse,
    GitLabStartRequest, Integration, IntegrationInstallation,
};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite};
use tracing::{error, info, warn};
use uuid::Uuid;

use super::{error_page, require_remote_mode, row_to_installation};
use crate::AppState;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::runners::RunnerAuth;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Maximum age (seconds) for a GitLab OAuth state token.
const STATE_MAX_AGE_SECS: i64 = 600; // 10 minutes

fn build_http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

async fn access_token(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    integration_id: &str,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let encrypted: Option<String> = sqlx::query_scalar(
        "SELECT encrypted_value FROM integration_credentials \
         WHERE integration_id = ?1 AND credential_type = 'access_token'",
    )
    .bind(integration_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to fetch GitLab access token");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to fetch source credentials",
        )
    })?;
    let encrypted = encrypted.ok_or_else(|| {
        api_err(
            StatusCode::CONFLICT,
            "missing_credentials",
            "GitLab access token is missing; reconnect or re-authorize the source",
        )
    })?;
    crypto::decrypt(&encrypted, encryption_key).map_err(|e| {
        error!(error = %e, "failed to decrypt GitLab access token");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "encryption_error",
            "Failed to decrypt source credentials",
        )
    })
}

pub(crate) async fn resolve_branch_commit(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    integration_id: &str,
    host_url: &str,
    auth_mode: &str,
    repository_external_id: &str,
    branch: &str,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let token = access_token(pool, encryption_key, integration_id).await?;
    let client = build_http_client().map_err(|e| {
        error!(error = %e, "failed to build GitLab client");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "http_client_error",
            "Failed to create GitLab client",
        )
    })?;
    let mut request = client
        .get(format!(
            "{}/api/v4/projects/{}/repository/commits/{}",
            host_url.trim_end_matches('/'),
            urlencoding::encode(repository_external_id),
            urlencoding::encode(branch)
        ))
        .header("User-Agent", "oore-ci");
    request = if auth_mode == "oauth_app" {
        request.header("Authorization", format!("Bearer {token}"))
    } else {
        request.header("PRIVATE-TOKEN", token)
    };
    let response = request.send().await.map_err(|e| {
        error!(error = %e, "failed to resolve GitLab branch");
        api_err(
            StatusCode::BAD_GATEWAY,
            "gitlab_api_error",
            "Failed to resolve the GitLab branch",
        )
    })?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_ref",
            format!("Branch '{branch}' was not found in the linked repository"),
        ));
    }
    if !response.status().is_success() {
        return Err(api_err(
            StatusCode::BAD_GATEWAY,
            "gitlab_api_error",
            format!(
                "GitLab returned {} while resolving the branch",
                response.status()
            ),
        ));
    }

    #[derive(Deserialize)]
    struct CommitResponse {
        id: String,
    }
    response
        .json::<CommitResponse>()
        .await
        .map(|response| response.id)
        .map_err(|e| {
            error!(error = %e, "failed to parse GitLab commit response");
            api_err(
                StatusCode::BAD_GATEWAY,
                "gitlab_parse_error",
                "Failed to parse GitLab commit response",
            )
        })
}

fn normalize_gitlab_host_url(raw: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let parsed = url::Url::parse(raw.trim()).map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "host_url must be an http or https origin",
        )
    })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !matches!(parsed.path(), "" | "/")
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "host_url must be an http or https origin without credentials, a path, query, or fragment",
        ));
    }
    Ok(parsed.origin().ascii_serialization())
}

fn oauth_callback_url(redirect_url: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let parsed = url::Url::parse(redirect_url).map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_url",
            "redirect_url is not a valid URL",
        )
    })?;
    Ok(format!(
        "{}/v1/integrations/gitlab/callback",
        parsed.origin().ascii_serialization()
    ))
}

fn git_checkout_request_allowed(
    method: &Method,
    git_path: &str,
    query: Option<&str>,
    content_type: Option<&str>,
    full_name: &str,
) -> bool {
    match *method {
        Method::GET => {
            git_path == format!("{full_name}.git/info/refs")
                && query == Some("service=git-upload-pack")
        }
        Method::POST => {
            git_path == format!("{full_name}.git/git-upload-pack")
                && query.is_none()
                && content_type == Some("application/x-git-upload-pack-request")
        }
        _ => false,
    }
}

/// Sync a GitLab integration by refreshing repositories for all installations.
///
/// For GitLab, "installations" correspond to authorized users/accounts. Sync
/// re-fetches accessible projects and upserts them into `integration_repositories`.
pub(crate) async fn perform_sync_installations(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    integration_id: &str,
) -> Result<Vec<IntegrationInstallation>, (StatusCode, Json<ApiError>)> {
    let row =
        sqlx::query("SELECT provider, host_url, auth_mode, status FROM integrations WHERE id = ?1")
            .bind(integration_id)
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
    if provider != "gitlab" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_provider",
            "Integration is not a GitLab integration",
        ));
    }

    let status: String = row.get("status");
    if status != "active" {
        return Err(api_err(
            StatusCode::CONFLICT,
            "integration_inactive",
            "Integration is not active",
        ));
    }

    let host_url: String = row.get("host_url");
    let auth_mode: String = row.get("auth_mode");
    let use_bearer_auth = auth_mode == "oauth_app";

    let token = access_token(pool, encryption_key, integration_id).await?;

    let installation_rows = sqlx::query(
        "SELECT * FROM integration_installations WHERE integration_id = ?1 ORDER BY created_at DESC",
    )
    .bind(integration_id)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to list installations");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to list installations",
        )
    })?;

    let installations: Vec<IntegrationInstallation> =
        installation_rows.iter().map(row_to_installation).collect();

    // Nothing to do if we don't have any linked accounts yet.
    if installations.is_empty() {
        return Ok(installations);
    }

    let http_client = build_http_client().map_err(|e| {
        error!(error = %e, "failed to build HTTP client");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "http_client_error",
            "Failed to create HTTP client",
        )
    })?;

    let now = now_unix();
    for inst in &installations {
        // Refresh project list for each installation/user.
        sync_gitlab_projects(
            &http_client,
            pool,
            &host_url,
            &token,
            &inst.id,
            use_bearer_auth,
            now,
        )
        .await?;
    }

    Ok(installations)
}

/// `POST /v1/integrations/gitlab/start` — create GitLab integration (OAuth or token mode).
///
/// - **OAuth mode**: user provides client_id + client_secret from their GitLab application.
/// - **Token mode**: user provides a personal/group access token.
///
/// Both modes support gitlab.com and self-managed instances (host_url).
pub async fn gitlab_start(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<GitLabStartRequest>,
) -> ApiResult<GitLabCompleteResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "write").await?;
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    require_remote_mode(&pool).await?;

    let host_url = normalize_gitlab_host_url(&req.host_url)?;

    if req.webhook_secret.trim().is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "webhook_secret is required",
        ));
    }

    let auth_mode = req.auth_mode.as_str();
    if !matches!(auth_mode, "oauth_app" | "personal_token") {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "auth_mode must be 'oauth_app' or 'personal_token'",
        ));
    }

    // Validate mode-specific fields
    match auth_mode {
        "oauth_app" => {
            if req.client_id.as_ref().is_none_or(|s| s.is_empty()) {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_input",
                    "client_id required for OAuth mode",
                ));
            }
            if req.client_secret.as_ref().is_none_or(|s| s.is_empty()) {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_input",
                    "client_secret required for OAuth mode",
                ));
            }
        }
        "personal_token" => {
            if req.access_token.as_ref().is_none_or(|s| s.is_empty()) {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_input",
                    "access_token required for token mode",
                ));
            }
        }
        _ => unreachable!(),
    }

    // Validate token/credentials by calling GitLab API
    let client = build_http_client().map_err(|e| {
        error!(error = %e, "failed to build HTTP client");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "http_client_error",
            "Failed to create HTTP client",
        )
    })?;
    let api_base = format!("{}/api/v4", host_url);

    let (display_name, username) = match auth_mode {
        "personal_token" => {
            let token = req.access_token.as_ref().unwrap();
            let resp = client
                .get(format!("{api_base}/user"))
                .header("PRIVATE-TOKEN", token.as_str())
                .header("User-Agent", "oore-ci")
                .send()
                .await
                .map_err(|e| {
                    error!(error = %e, "GitLab API request failed");
                    api_err(
                        StatusCode::BAD_GATEWAY,
                        "gitlab_api_error",
                        "Failed to communicate with GitLab",
                    )
                })?;

            if !resp.status().is_success() {
                let status = resp.status();
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "gitlab_auth_failed",
                    format!("GitLab authentication failed ({status}). Check your access token."),
                ));
            }

            #[derive(serde::Deserialize)]
            struct GitLabUser {
                username: String,
                name: Option<String>,
            }

            let user: GitLabUser = resp.json().await.map_err(|e| {
                error!(error = %e, "failed to parse GitLab user response");
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "gitlab_parse_error",
                    "Failed to parse GitLab response",
                )
            })?;

            let display = user.name.unwrap_or_else(|| user.username.clone());
            (display, user.username)
        }
        "oauth_app" => {
            // For OAuth mode, we don't have a token yet — we store the credentials
            // and the actual OAuth flow happens when the user authorizes.
            // For now, validate the host is reachable.
            let resp = client
                .get(format!("{api_base}/version"))
                .header("User-Agent", "oore-ci")
                .send()
                .await
                .map_err(|e| {
                    error!(error = %e, "GitLab API request failed");
                    api_err(
                        StatusCode::BAD_GATEWAY,
                        "gitlab_api_error",
                        "Failed to communicate with GitLab",
                    )
                })?;

            // Some self-managed GitLab instances restrict /version to authenticated requests.
            // Treat 401/403 as "reachable" for the purpose of initial OAuth app setup.
            let status = resp.status();
            if !(status.is_success()
                || status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN)
            {
                return Err(api_err(
                    StatusCode::BAD_GATEWAY,
                    "gitlab_unreachable",
                    "GitLab instance is unreachable or returned an error",
                ));
            }

            let display = if status.is_success() {
                #[derive(serde::Deserialize)]
                struct GitLabVersion {
                    version: String,
                }

                match resp.json::<GitLabVersion>().await {
                    Ok(version) => format!("GitLab {}", version.version),
                    Err(e) => {
                        warn!(error = %e, "failed to parse GitLab version response (non-fatal)");
                        "GitLab".to_string()
                    }
                }
            } else {
                "GitLab".to_string()
            };
            (display, "oauth".to_string())
        }
        _ => unreachable!(),
    };

    let now = now_unix();
    let integration_id = Uuid::new_v4().to_string();
    let full_display_name = format!("{display_name} ({host_url})");
    let integration_status = if auth_mode == "oauth_app" {
        "inactive"
    } else {
        "active"
    };

    // Insert integration
    sqlx::query(
        "INSERT INTO integrations (id, provider, host_url, auth_mode, status, display_name, created_by, created_at, updated_at) \
         VALUES (?1, 'gitlab', ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
    )
    .bind(&integration_id)
    .bind(&host_url)
    .bind(auth_mode)
    .bind(integration_status)
    .bind(&full_display_name)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to insert GitLab integration");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to create integration")
    })?;

    // Store credentials
    let encrypted_webhook_secret =
        crypto::encrypt(req.webhook_secret.trim(), &state.encryption_key).map_err(|e| {
            error!(error = %e, "failed to encrypt webhook secret");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "encryption_error",
                "Failed to encrypt credentials",
            )
        })?;

    sqlx::query(
        "INSERT INTO integration_credentials (id, integration_id, credential_type, encrypted_value, created_at, updated_at) \
         VALUES (?1, ?2, 'webhook_secret', ?3, ?4, ?4)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&integration_id)
    .bind(&encrypted_webhook_secret)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to store webhook secret");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to store credentials")
    })?;

    match auth_mode {
        "personal_token" => {
            let token = req.access_token.as_ref().unwrap();
            let encrypted = crypto::encrypt(token, &state.encryption_key).map_err(|e| {
                error!(error = %e, "failed to encrypt access token");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "encryption_error",
                    "Failed to encrypt credentials",
                )
            })?;

            sqlx::query(
                "INSERT INTO integration_credentials (id, integration_id, credential_type, encrypted_value, created_at, updated_at) \
                 VALUES (?1, ?2, 'access_token', ?3, ?4, ?4)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&integration_id)
            .bind(&encrypted)
            .bind(now)
            .execute(&pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to store access token");
                api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to store credentials")
            })?;
        }
        "oauth_app" => {
            let client_id = req.client_id.as_ref().unwrap();
            let client_secret = req.client_secret.as_ref().unwrap();

            for (cred_type, value) in [
                ("oauth_client_id", client_id),
                ("oauth_client_secret", client_secret),
            ] {
                let encrypted = crypto::encrypt(value, &state.encryption_key).map_err(|e| {
                    error!(error = %e, credential_type = %cred_type, "failed to encrypt credential");
                    api_err(StatusCode::INTERNAL_SERVER_ERROR, "encryption_error", "Failed to encrypt credentials")
                })?;

                sqlx::query(
                    "INSERT INTO integration_credentials (id, integration_id, credential_type, encrypted_value, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                )
                .bind(Uuid::new_v4().to_string())
                .bind(&integration_id)
                .bind(cred_type)
                .bind(&encrypted)
                .bind(now)
                .execute(&pool)
                .await
                .map_err(|e| {
                    error!(error = %e, "failed to store credential");
                    api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to store credentials")
                })?;
            }
        }
        _ => unreachable!(),
    }

    // For personal token mode, create a default installation entry
    if auth_mode == "personal_token" {
        let inst_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO integration_installations (id, integration_id, external_id, account_name, account_type, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'user', ?5, ?5)",
        )
        .bind(&inst_id)
        .bind(&integration_id)
        .bind(&username)
        .bind(&username)
        .bind(now)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to create default installation");
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to create installation")
        })?;

        // Fetch accessible projects via GitLab API
        let token = req.access_token.as_ref().unwrap();
        if let Err(e) =
            sync_gitlab_projects(&client, &pool, &host_url, token, &inst_id, false, now).await
        {
            error!(error = ?e, "failed to sync GitLab projects (non-fatal)");
        }
    }

    let details = serde_json::json!({
        "provider": "gitlab",
        "host_url": host_url,
        "auth_mode": auth_mode,
        "display_name": full_display_name,
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
        host = %host_url,
        mode = %auth_mode,
        status = %integration_status,
        "GitLab integration created"
    );

    let integration = Integration {
        id: integration_id,
        provider: "gitlab".to_string(),
        host_url,
        auth_mode: auth_mode.to_string(),
        status: integration_status.to_string(),
        display_name: Some(full_display_name),
        app_id: None,
        app_slug: None,
        created_by: auth.0.user_id,
        created_at: now,
        updated_at: now,
    };

    Ok(Json(GitLabCompleteResponse { integration }))
}

/// Sync accessible GitLab projects into integration_repositories.
///
/// When `use_bearer_auth` is true, uses `Authorization: Bearer` (OAuth).
/// Otherwise, uses `PRIVATE-TOKEN` header (personal access token).
async fn sync_gitlab_projects(
    client: &reqwest::Client,
    pool: &sqlx::SqlitePool,
    host_url: &str,
    token: &str,
    installation_id: &str,
    use_bearer_auth: bool,
    now: i64,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let api_base = format!("{}/api/v4", host_url);

    #[derive(serde::Deserialize)]
    struct GitLabProject {
        id: i64,
        path_with_namespace: String,
        default_branch: Option<String>,
        visibility: Option<String>,
        web_url: Option<String>,
    }

    let mut projects = Vec::new();
    let mut page = 1usize;
    loop {
        let mut req_builder = client
            .get(format!(
                "{api_base}/projects?membership=true&per_page=100&simple=true&page={page}"
            ))
            .header("User-Agent", "oore-ci");

        if use_bearer_auth {
            req_builder = req_builder.header("Authorization", format!("Bearer {token}"));
        } else {
            req_builder = req_builder.header("PRIVATE-TOKEN", token);
        }

        let resp = req_builder.send().await.map_err(|e| {
            error!(error = %e, page, "GitLab projects API failed");
            api_err(
                StatusCode::BAD_GATEWAY,
                "gitlab_api_error",
                "Failed to list GitLab projects",
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(api_err(
                StatusCode::BAD_GATEWAY,
                "gitlab_api_error",
                format!("GitLab returned {status}"),
            ));
        }

        let next_page = resp
            .headers()
            .get("x-next-page")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|next| *next > page);
        let mut response_projects: Vec<GitLabProject> = resp.json().await.map_err(|e| {
            error!(error = %e, page, "failed to parse GitLab projects");
            api_err(
                StatusCode::BAD_GATEWAY,
                "gitlab_parse_error",
                "Failed to parse GitLab response",
            )
        })?;
        projects.append(&mut response_projects);

        let Some(next_page) = next_page else { break };
        page = next_page;
    }

    let mut tx = pool.begin().await.map_err(|e| {
        error!(error = %e, "failed to start GitLab repository sync transaction");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to sync projects",
        )
    })?;

    for project in &projects {
        let is_private = project.visibility.as_deref() != Some("public");

        sqlx::query(
            "INSERT INTO integration_repositories (id, installation_id, external_id, full_name, default_branch, is_private, html_url, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8) \
             ON CONFLICT(installation_id, external_id) DO UPDATE SET \
             full_name = excluded.full_name, default_branch = excluded.default_branch, \
             is_private = excluded.is_private, html_url = excluded.html_url, updated_at = excluded.updated_at",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(installation_id)
        .bind(project.id.to_string())
        .bind(&project.path_with_namespace)
        .bind(&project.default_branch)
        .bind(is_private as i32)
        .bind(&project.web_url)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            error!(error = %e, project = %project.path_with_namespace, "failed to upsert GitLab project");
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to store project")
        })?;
    }

    let external_ids: Vec<String> = projects.iter().map(|p| p.id.to_string()).collect();
    for prefix in [
        "UPDATE projects SET repository_id = NULL WHERE repository_id IN (SELECT id FROM integration_repositories WHERE installation_id = ",
        "DELETE FROM integration_repositories WHERE installation_id = ",
    ] {
        let mut query = QueryBuilder::<Sqlite>::new(prefix);
        query.push_bind(installation_id);
        if !external_ids.is_empty() {
            query.push(" AND external_id NOT IN (");
            let mut separated = query.separated(", ");
            for external_id in &external_ids {
                separated.push_bind(external_id);
            }
            separated.push_unseparated(")");
        }
        if prefix.starts_with("UPDATE") {
            query.push(")");
        }
        query.build().execute(&mut *tx).await.map_err(|e| {
            error!(error = %e, "failed to remove stale GitLab projects");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to sync projects",
            )
        })?;
    }

    tx.commit().await.map_err(|e| {
        error!(error = %e, "failed to commit GitLab repository sync");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to sync projects",
        )
    })?;

    info!(project_count = projects.len(), "GitLab projects synced");
    Ok(())
}

/// Stream Git smart-HTTP through the daemon so runner jobs can clone private
/// repositories without SCM credentials entering build snapshots or logs.
pub async fn proxy_git_checkout(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id, git_path)): Path<(String, String, String)>,
    runner_auth: RunnerAuth,
    method: Method,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }
    if !matches!(method, Method::GET | Method::POST)
        || git_path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
        || git_path.contains('\\')
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_git_request",
            "Invalid Git checkout request",
        ));
    }

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    let row = sqlx::query(
        "SELECT i.host_url, r.full_name, c.encrypted_value \
         FROM builds b \
         JOIN projects p ON p.id = b.project_id \
         JOIN integration_repositories r ON r.id = p.repository_id \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         JOIN integrations i ON i.id = inst.integration_id \
         JOIN integration_credentials c ON c.integration_id = i.id \
         WHERE b.id = ?1 AND b.runner_id = ?2 AND i.provider = 'gitlab' \
         AND i.status = 'active' AND c.credential_type = 'access_token'",
    )
    .bind(&job_id)
    .bind(&runner_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, job_id = %job_id, "failed to resolve GitLab checkout");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to resolve checkout",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "checkout_not_found",
            "GitLab checkout not found",
        )
    })?;

    let host_url: String = row.get("host_url");
    let full_name: String = row.get("full_name");
    let encrypted_token: String = row.get("encrypted_value");

    if !git_checkout_request_allowed(
        &method,
        &git_path,
        query.as_deref(),
        headers
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        &full_name,
    ) {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "checkout_path_forbidden",
            "Git checkout request does not match the assigned repository",
        ));
    }

    let token = crypto::decrypt(&encrypted_token, &state.encryption_key).map_err(|e| {
        error!(error = %e, job_id = %job_id, "failed to decrypt GitLab checkout token");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "encryption_error",
            "Failed to authorize checkout",
        )
    })?;

    let mut upstream = url::Url::parse(&host_url).map_err(|_| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid_host",
            "GitLab host is invalid",
        )
    })?;
    upstream
        .path_segments_mut()
        .map_err(|_| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "invalid_host",
                "GitLab host is invalid",
            )
        })?
        .extend(git_path.split('/'));
    upstream.set_query(query.as_deref());

    let basic = base64::engine::general_purpose::STANDARD.encode(format!("oauth2:{token}"));
    let client = build_http_client().map_err(|e| {
        error!(error = %e, "failed to build GitLab checkout client");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "http_client_error",
            "Failed to create checkout client",
        )
    })?;
    let mut request = client
        .request(method, upstream)
        .header("Authorization", format!("Basic {basic}"))
        .header("User-Agent", "oore-ci");
    if headers.get("content-type").is_some() {
        request = request.body(reqwest::Body::wrap_stream(body.into_data_stream()));
    }
    for name in ["accept", "content-type", "git-protocol"] {
        if let Some(value) = headers.get(name) {
            request = request.header(name, value);
        }
    }

    let response = request.send().await.map_err(|e| {
        error!(error = %e, job_id = %job_id, "GitLab checkout proxy failed");
        api_err(
            StatusCode::BAD_GATEWAY,
            "gitlab_checkout_failed",
            "GitLab checkout failed",
        )
    })?;
    if response.status().is_redirection() {
        return Err(api_err(
            StatusCode::BAD_GATEWAY,
            "gitlab_redirect_rejected",
            "GitLab checkout host redirected unexpectedly",
        ));
    }

    let status = response.status();
    let response_headers = response.headers().clone();
    let mut downstream = Response::builder().status(status);
    for name in ["content-type", "cache-control", "etag", "expires", "pragma"] {
        if let Some(value) = response_headers.get(name) {
            downstream = downstream.header(name, value);
        }
    }
    downstream
        .body(Body::from_stream(response.bytes_stream()))
        .map_err(|e| {
            error!(error = %e, "failed to build GitLab checkout response");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "response_error",
                "Failed to stream checkout",
            )
        })
}

// ── GitLab OAuth flow ────────────────────────────────────────────

/// Encrypted payload stored in the `state` query parameter for GitLab OAuth.
#[derive(Debug, Serialize, Deserialize)]
struct GitLabOAuthState {
    integration_id: String,
    redirect_url: String,
    #[serde(default)]
    callback_url: String,
    created_at: i64,
}

/// Encrypt a GitLab OAuth state payload into a URL-safe token.
fn seal_gitlab_state(state: &GitLabOAuthState, key: &[u8]) -> Result<String, anyhow::Error> {
    let json = serde_json::to_string(state)?;
    let encrypted = crypto::encrypt(&json, key)?;
    Ok(urlencoding::encode(&encrypted).into_owned())
}

/// Decrypt and validate a GitLab OAuth state token.
fn open_gitlab_state(token: &str, key: &[u8]) -> Result<GitLabOAuthState, String> {
    let decoded = urlencoding::decode(token).map_err(|e| format!("url decode: {e}"))?;
    let json = crypto::decrypt(&decoded, key).map_err(|e| format!("decrypt: {e}"))?;
    let mut state: GitLabOAuthState =
        serde_json::from_str(&json).map_err(|e| format!("parse: {e}"))?;

    if state.callback_url.is_empty() {
        let parsed = url::Url::parse(&state.redirect_url).map_err(|e| format!("redirect: {e}"))?;
        state.callback_url = format!(
            "{}/v1/integrations/gitlab/callback",
            parsed.origin().ascii_serialization()
        );
    }

    let now = now_unix();
    if now - state.created_at > STATE_MAX_AGE_SECS {
        return Err("state token expired".into());
    }

    Ok(state)
}

/// Validate that a redirect URL belongs to configured trusted frontend origins.
fn validate_redirect_origin(
    url: &str,
    allowed_origins: &[String],
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let parsed = url::Url::parse(url).map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_url",
            "redirect_url is not a valid URL",
        )
    })?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_url",
            "redirect_url must not contain credentials",
        ));
    }

    let matches_allowed = allowed_origins.iter().any(|candidate| {
        let Ok(candidate_url) = url::Url::parse(candidate) else {
            return false;
        };
        parsed.scheme() == candidate_url.scheme()
            && parsed.host_str() == candidate_url.host_str()
            && parsed.port_or_known_default() == candidate_url.port_or_known_default()
    });
    if !matches_allowed {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_url",
            "redirect_url must match a configured redirect origin",
        ));
    }

    Ok(())
}

/// `POST /v1/integrations/gitlab/authorize` — generate GitLab OAuth authorize URL.
///
/// The user calls this after `gitlab_start` with `oauth_app` mode. Builds the
/// authorization URL with the stored client_id and returns it.
pub async fn gitlab_authorize(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<GitLabAuthorizeRequest>,
) -> ApiResult<GitLabAuthorizeResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "write").await?;

    if req.integration_id.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "integration_id is required",
        ));
    }
    if req.redirect_url.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "redirect_url is required",
        ));
    }

    // Validate redirect against configured frontend origin.
    let allowed_origins = state.allowed_origins.read().await.clone();
    validate_redirect_origin(&req.redirect_url, &allowed_origins)?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    require_remote_mode(&pool).await?;

    // Load the integration
    let row = sqlx::query("SELECT * FROM integrations WHERE id = ?1")
        .bind(&req.integration_id)
        .fetch_optional(&pool)
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

    let auth_mode: String = row.get("auth_mode");
    let status: String = row.get("status");
    let host_url: String = row.get("host_url");

    if auth_mode != "oauth_app" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_auth_mode",
            "Integration is not OAuth mode",
        ));
    }
    if status != "inactive" {
        return Err(api_err(
            StatusCode::CONFLICT,
            "already_active",
            "Integration is already active",
        ));
    }

    // Decrypt client_id
    let encrypted_client_id: String = sqlx::query_scalar(
        "SELECT encrypted_value FROM integration_credentials \
         WHERE integration_id = ?1 AND credential_type = 'oauth_client_id'",
    )
    .bind(&req.integration_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to fetch client_id");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to fetch credentials",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "missing_credentials",
            "OAuth client_id not found",
        )
    })?;

    let client_id = crypto::decrypt(&encrypted_client_id, &state.encryption_key).map_err(|e| {
        error!(error = %e, "failed to decrypt client_id");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "encryption_error",
            "Failed to decrypt credentials",
        )
    })?;

    // The browser reaches the callback through the validated frontend proxy,
    // not necessarily through the daemon's own public URL.
    let callback_url = oauth_callback_url(&req.redirect_url)?;

    // Seal the state token
    let oauth_state = GitLabOAuthState {
        integration_id: req.integration_id.clone(),
        redirect_url: req.redirect_url,
        callback_url: callback_url.clone(),
        created_at: now_unix(),
    };

    let state_token = seal_gitlab_state(&oauth_state, &state.encryption_key).map_err(|e| {
        error!(error = %e, "failed to seal GitLab OAuth state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "encryption_error",
            "Failed to create state token",
        )
    })?;

    // Build the authorize URL
    let authorize_url = format!(
        "{}/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&state={}&scope=read_api+read_repository",
        host_url,
        urlencoding::encode(&client_id),
        urlencoding::encode(&callback_url),
        state_token,
    );

    info!(
        integration_id = %req.integration_id,
        "GitLab OAuth authorize URL generated"
    );

    Ok(Json(GitLabAuthorizeResponse { authorize_url }))
}

// ── GitLab OAuth callback ────────────────────────────────────────

#[derive(Deserialize)]
pub struct GitLabCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// `GET /v1/integrations/gitlab/callback?code=...&state=...`
///
/// GitLab redirects here after the user authorizes. Exchanges the code for
/// tokens, stores them, and redirects back to the frontend.
pub async fn gitlab_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<GitLabCallbackQuery>,
) -> Response {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    if require_remote_mode(&pool).await.is_err() {
        return Html(error_page(
            "Remote mode required",
            "GitLab integration setup is available only when remote mode is enabled.",
        ))
        .into_response();
    }

    // Handle GitLab error response
    if let Some(ref err) = params.error {
        let desc = params
            .error_description
            .as_deref()
            .unwrap_or("Unknown error");
        warn!(error = %err, description = %desc, "GitLab OAuth error");
        return Html(error_page(
            "GitLab Authorization Failed",
            &format!("GitLab returned an error: {desc}"),
        ))
        .into_response();
    }

    let code = match params.code {
        Some(c) if !c.is_empty() => c,
        _ => {
            return Html(error_page(
                "Missing code",
                "GitLab did not provide an authorization code. Please try again.",
            ))
            .into_response();
        }
    };

    let state_token = match params.state {
        Some(s) if !s.is_empty() => s,
        _ => {
            return Html(error_page(
                "Missing state",
                "State parameter is missing. Please try again.",
            ))
            .into_response();
        }
    };

    let oauth_state = match open_gitlab_state(&state_token, &state.encryption_key) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "invalid GitLab callback state token");
            return Html(error_page(
                "Invalid or expired link",
                "The authorization link has expired. Please go back and start again.",
            ))
            .into_response();
        }
    };

    // Validate the redirect_url from the sealed state against the configured frontend origin
    let allowed_origins = state.allowed_origins.read().await.clone();
    if validate_redirect_origin(&oauth_state.redirect_url, &allowed_origins).is_err() {
        warn!(redirect_url = %oauth_state.redirect_url, "callback redirect_url does not match configured origin");
        return Html(error_page(
            "Invalid redirect",
            "The redirect URL does not match the configured frontend origin.",
        ))
        .into_response();
    }

    // Exchange code for tokens
    match exchange_gitlab_code(
        &state,
        &code,
        &oauth_state.integration_id,
        &oauth_state.callback_url,
    )
    .await
    {
        Ok(()) => {
            info!(
                integration_id = %oauth_state.integration_id,
                "GitLab OAuth flow completed successfully"
            );

            let sep = if oauth_state.redirect_url.contains('?') {
                "&"
            } else {
                "?"
            };
            let redirect_url = format!(
                "{}{}gitlab=success&integration_id={}",
                oauth_state.redirect_url, sep, oauth_state.integration_id
            );
            Redirect::to(&redirect_url).into_response()
        }
        Err(msg) => {
            error!(error = %msg, "GitLab OAuth token exchange failed");
            Html(error_page(
                "Authorization failed",
                &format!("Failed to complete GitLab authorization: {msg}. Please try again."),
            ))
            .into_response()
        }
    }
}

/// Exchange a GitLab authorization code for access + refresh tokens.
///
/// Stores both tokens encrypted, activates the integration, creates an
/// installation entry, and syncs projects.
///
/// The store mutex is held only for short DB-read/write windows; all outbound
/// HTTP calls to GitLab happen with the mutex released.
async fn exchange_gitlab_code(
    app_state: &Arc<AppState>,
    code: &str,
    integration_id: &str,
    callback_url: &str,
) -> Result<(), String> {
    // ── Phase 1: Read credentials from DB (short lock) ──────────
    let (host_url, client_id, client_secret, pool) = {
        let store = app_state.store.lock().await;
        let pool = store.pool().clone();

        let row = sqlx::query("SELECT host_url FROM integrations WHERE id = ?1")
            .bind(integration_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| format!("Failed to fetch integration: {e}"))?
            .ok_or_else(|| "Integration not found".to_string())?;

        let host_url: String = row.get("host_url");

        let encrypted_client_id: String = sqlx::query_scalar(
            "SELECT encrypted_value FROM integration_credentials \
             WHERE integration_id = ?1 AND credential_type = 'oauth_client_id'",
        )
        .bind(integration_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| format!("Failed to fetch client_id: {e}"))?
        .ok_or_else(|| "OAuth client_id not found".to_string())?;

        let client_id = crypto::decrypt(&encrypted_client_id, &app_state.encryption_key)
            .map_err(|e| format!("Failed to decrypt client_id: {e}"))?;

        let encrypted_client_secret: String = sqlx::query_scalar(
            "SELECT encrypted_value FROM integration_credentials \
             WHERE integration_id = ?1 AND credential_type = 'oauth_client_secret'",
        )
        .bind(integration_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| format!("Failed to fetch client_secret: {e}"))?
        .ok_or_else(|| "OAuth client_secret not found".to_string())?;

        let client_secret = crypto::decrypt(&encrypted_client_secret, &app_state.encryption_key)
            .map_err(|e| format!("Failed to decrypt client_secret: {e}"))?;

        // Mutex guard is dropped here
        (host_url, client_id, client_secret, pool)
    };

    // ── Phase 2: Outbound HTTP — token exchange (no lock held) ──
    let http_client =
        build_http_client().map_err(|e| format!("failed to build HTTP client: {e}"))?;

    #[derive(Deserialize)]
    struct GitLabTokenResponse {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<i64>,
        #[allow(dead_code)]
        token_type: Option<String>,
    }

    let token_resp = http_client
        .post(format!("{}/oauth/token", host_url))
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", code),
            ("redirect_uri", callback_url),
            ("grant_type", "authorization_code"),
        ])
        .header("User-Agent", "oore-ci")
        .send()
        .await
        .map_err(|e| format!("GitLab token request failed: {e}"))?;

    if !token_resp.status().is_success() {
        let status = token_resp.status();
        error!(status = %status, "GitLab token exchange failed");
        return Err(format!("GitLab returned {status}"));
    }

    let tokens: GitLabTokenResponse = token_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GitLab token response: {e}"))?;

    // ── Phase 3: Outbound HTTP — fetch user info (no lock held) ──
    let api_base = format!("{}/api/v4", host_url);
    let user_resp = http_client
        .get(format!("{api_base}/user"))
        .header("Authorization", format!("Bearer {}", tokens.access_token))
        .header("User-Agent", "oore-ci")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch GitLab user: {e}"))?;

    let (username, display_name) = if user_resp.status().is_success() {
        #[derive(Deserialize)]
        struct GitLabUser {
            username: String,
            name: Option<String>,
        }

        match user_resp.json::<GitLabUser>().await {
            Ok(user) => {
                let display = user.name.unwrap_or_else(|| user.username.clone());
                (user.username, display)
            }
            Err(_) => ("oauth-user".to_string(), "OAuth User".to_string()),
        }
    } else {
        ("oauth-user".to_string(), "OAuth User".to_string())
    };

    // ── Phase 4: Persist tokens + activate (short lock) ─────────
    // Integration stays `inactive` until ALL writes succeed.
    let now = now_unix();

    // Store access_token (upsert)
    let encrypted_access_token = crypto::encrypt(&tokens.access_token, &app_state.encryption_key)
        .map_err(|e| format!("Failed to encrypt access_token: {e}"))?;

    sqlx::query(
        "INSERT INTO integration_credentials (id, integration_id, credential_type, encrypted_value, created_at, updated_at) \
         VALUES (?1, ?2, 'access_token', ?3, ?4, ?4) \
         ON CONFLICT(integration_id, credential_type) DO UPDATE SET \
         encrypted_value = excluded.encrypted_value, updated_at = excluded.updated_at",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(integration_id)
    .bind(&encrypted_access_token)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| format!("Failed to store access_token: {e}"))?;

    // Store refresh_token if present (upsert)
    if let Some(ref refresh_token) = tokens.refresh_token {
        let encrypted_refresh = crypto::encrypt(refresh_token, &app_state.encryption_key)
            .map_err(|e| format!("Failed to encrypt refresh_token: {e}"))?;

        sqlx::query(
            "INSERT INTO integration_credentials (id, integration_id, credential_type, encrypted_value, created_at, updated_at) \
             VALUES (?1, ?2, 'refresh_token', ?3, ?4, ?4) \
             ON CONFLICT(integration_id, credential_type) DO UPDATE SET \
             encrypted_value = excluded.encrypted_value, updated_at = excluded.updated_at",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(integration_id)
        .bind(&encrypted_refresh)
        .bind(now)
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to store refresh_token: {e}"))?;
    }

    // Update access_token expires_at if present
    if let Some(expires_in) = tokens.expires_in {
        let expires_at = now + expires_in;
        sqlx::query(
            "UPDATE integration_credentials SET expires_at = ?1, updated_at = ?2 \
             WHERE integration_id = ?3 AND credential_type = 'access_token'",
        )
        .bind(expires_at)
        .bind(now)
        .bind(integration_id)
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to update token expires_at: {e}"))?;
    }

    // Create installation entry
    let inst_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO integration_installations (id, integration_id, external_id, account_name, account_type, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, 'user', ?5, ?5) \
         ON CONFLICT(integration_id, external_id) DO UPDATE SET \
         account_name = excluded.account_name, updated_at = excluded.updated_at",
    )
    .bind(&inst_id)
    .bind(integration_id)
    .bind(&username)
    .bind(&username)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| format!("Failed to create installation: {e}"))?;

    // All critical writes succeeded — now activate the integration.
    sqlx::query("UPDATE integrations SET status = 'active', updated_at = ?1 WHERE id = ?2")
        .bind(now)
        .bind(integration_id)
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to activate integration: {e}"))?;

    // ── Phase 5: Non-critical finalization (best-effort) ─────────

    // Sync projects with bearer auth (non-fatal)
    if let Err(e) = sync_gitlab_projects(
        &http_client,
        &pool,
        &host_url,
        &tokens.access_token,
        &inst_id,
        true,
        now,
    )
    .await
    {
        error!(error = ?e, "failed to sync GitLab projects after OAuth (non-fatal)");
    }

    // Update display name (non-fatal)
    let full_display_name = format!("{display_name} ({host_url})");
    let _ = sqlx::query("UPDATE integrations SET display_name = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(&full_display_name)
        .bind(now)
        .bind(integration_id)
        .execute(&pool)
        .await;

    info!(
        integration_id = %integration_id,
        username = %username,
        "GitLab OAuth token exchange completed"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_http_client, git_checkout_request_allowed, normalize_gitlab_host_url,
        oauth_callback_url, sync_gitlab_projects, validate_redirect_origin,
    };
    use axum::Json;
    use axum::extract::Query;
    use axum::http::Method;
    use axum::routing::get;
    use std::collections::HashMap;

    #[test]
    fn gitlab_host_accepts_http_and_https_origins() {
        assert_eq!(
            normalize_gitlab_host_url("https://gitlab.example.com/").unwrap(),
            "https://gitlab.example.com"
        );
        assert_eq!(
            normalize_gitlab_host_url("http://gitlab.internal:8080").unwrap(),
            "http://gitlab.internal:8080"
        );
    }

    #[test]
    fn gitlab_host_rejects_non_origins() {
        for host in [
            "ftp://gitlab.example.com",
            "https://user:pass@gitlab.example.com",
            "https://gitlab.example.com/gitlab",
            "https://gitlab.example.com?x=1",
            "https://gitlab.example.com/#x",
        ] {
            assert!(normalize_gitlab_host_url(host).is_err(), "accepted {host}");
        }
    }

    #[test]
    fn oauth_callback_uses_frontend_origin() {
        assert_eq!(
            oauth_callback_url("https://oore.example.com/settings/integrations?tab=gitlab")
                .unwrap(),
            "https://oore.example.com/v1/integrations/gitlab/callback"
        );
    }

    #[test]
    fn checkout_proxy_only_allows_assigned_repository_upload_pack() {
        assert!(git_checkout_request_allowed(
            &Method::GET,
            "group/app.git/info/refs",
            Some("service=git-upload-pack"),
            None,
            "group/app",
        ));
        assert!(git_checkout_request_allowed(
            &Method::POST,
            "group/app.git/git-upload-pack",
            None,
            Some("application/x-git-upload-pack-request"),
            "group/app",
        ));
        assert!(!git_checkout_request_allowed(
            &Method::GET,
            "group/other.git/info/refs",
            Some("service=git-upload-pack"),
            None,
            "group/app",
        ));
        assert!(!git_checkout_request_allowed(
            &Method::GET,
            "group/app.git/info/refs",
            Some("service=git-receive-pack"),
            None,
            "group/app",
        ));
    }

    #[tokio::test]
    async fn project_sync_paginates_and_removes_stale_repositories() {
        let app = axum::Router::new().route(
            "/api/v4/projects",
            get(|Query(params): Query<HashMap<String, String>>| async move {
                let page = params.get("page").map(String::as_str).unwrap_or("1");
                let projects: Vec<_> = if page == "1" {
                    (1..=100)
                        .map(|id| {
                            serde_json::json!({
                                "id": id,
                                "path_with_namespace": format!("group/repo-{id}"),
                                "default_branch": "main",
                                "visibility": "private",
                                "web_url": format!("https://gitlab.example/group/repo-{id}"),
                            })
                        })
                        .collect()
                } else {
                    vec![serde_json::json!({
                        "id": 101,
                        "path_with_namespace": "group/repo-101",
                        "default_branch": "main",
                        "visibility": "private",
                        "web_url": "https://gitlab.example/group/repo-101",
                    })]
                };
                let mut headers = axum::http::HeaderMap::new();
                if page == "1" {
                    headers.insert("x-next-page", "2".parse().unwrap());
                }
                (headers, Json(projects))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE integration_repositories (
                id TEXT PRIMARY KEY, installation_id TEXT NOT NULL, external_id TEXT NOT NULL,
                full_name TEXT NOT NULL, default_branch TEXT, is_private INTEGER NOT NULL,
                html_url TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
                UNIQUE(installation_id, external_id)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("CREATE TABLE projects (id TEXT PRIMARY KEY, repository_id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO integration_repositories VALUES ('stale', 'install', 'stale', 'group/stale', 'main', 1, NULL, 1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO projects VALUES ('project', 'stale')")
            .execute(&pool)
            .await
            .unwrap();

        sync_gitlab_projects(
            &build_http_client().unwrap(),
            &pool,
            &host,
            "token",
            "install",
            false,
            2,
        )
        .await
        .unwrap();

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM integration_repositories WHERE installation_id = 'install'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let linked: Option<String> =
            sqlx::query_scalar("SELECT repository_id FROM projects WHERE id = 'project'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 101);
        assert!(linked.is_none());
        server.abort();
    }

    #[test]
    fn redirect_origin_accepts_default_localhost_origin() {
        let allowed = vec!["http://localhost:3000".to_string()];
        assert!(
            validate_redirect_origin("http://localhost:3000/settings/integrations", &allowed)
                .is_ok()
        );
    }

    #[test]
    fn redirect_origin_rejects_untrusted_origin() {
        let allowed = vec!["http://localhost:3000".to_string()];
        let err = validate_redirect_origin("https://evil.example/callback", &allowed)
            .expect_err("untrusted origin should be rejected");
        assert_eq!(err.0, axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn redirect_origin_rejects_embedded_credentials() {
        let allowed = vec!["http://localhost:3000".to_string()];
        let err = validate_redirect_origin("http://user:pass@localhost:3000/settings", &allowed)
            .expect_err("credential-bearing URL should be rejected");
        assert_eq!(err.0, axum::http::StatusCode::BAD_REQUEST);
    }
}
