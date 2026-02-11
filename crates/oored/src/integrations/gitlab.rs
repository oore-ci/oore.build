use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Json;
use oore_contract::{
    ApiError, GitLabAuthorizeRequest, GitLabAuthorizeResponse, GitLabCompleteResponse,
    GitLabStartRequest, Integration,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::error_page;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};
use crate::AppState;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Maximum age (seconds) for a GitLab OAuth state token.
const STATE_MAX_AGE_SECS: i64 = 600; // 10 minutes

fn build_http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

fn configured_redirect_origins() -> Vec<url::Url> {
    let raw_origins = std::env::var("OORE_AUTH_REDIRECT_ORIGINS")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            std::env::var("OORE_CORS_ORIGINS")
                .ok()
                .filter(|v| !v.trim().is_empty())
        })
        .or_else(|| std::env::var("OORE_CORS_ORIGIN").ok())
        .unwrap_or_else(|| "http://localhost:3000".to_string());

    let mut origins = Vec::new();
    for raw in raw_origins.split(',') {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        match url::Url::parse(trimmed) {
            Ok(url) => origins.push(url),
            Err(err) => warn!(origin = trimmed, error = %err, "ignoring invalid redirect origin"),
        }
    }
    if origins.is_empty() {
        vec![url::Url::parse("http://localhost:3000")
            .expect("default redirect origin must be valid")]
    } else {
        origins
    }
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

    // Validate host URL
    let host_url = req.host_url.trim_end_matches('/').to_string();
    if host_url.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid_input", "host_url is required"));
    }
    if url::Url::parse(&host_url).is_err() {
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid_input", "host_url is not a valid URL"));
    }

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
            if req.client_id.as_ref().map_or(true, |s| s.is_empty()) {
                return Err(api_err(StatusCode::BAD_REQUEST, "invalid_input", "client_id required for OAuth mode"));
            }
            if req.client_secret.as_ref().map_or(true, |s| s.is_empty()) {
                return Err(api_err(StatusCode::BAD_REQUEST, "invalid_input", "client_secret required for OAuth mode"));
            }
        }
        "personal_token" => {
            if req.access_token.as_ref().map_or(true, |s| s.is_empty()) {
                return Err(api_err(StatusCode::BAD_REQUEST, "invalid_input", "access_token required for token mode"));
            }
        }
        _ => unreachable!(),
    }

    // Validate token/credentials by calling GitLab API
    let client = build_http_client().map_err(|e| {
        error!(error = %e, "failed to build HTTP client");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "http_client_error", "Failed to create HTTP client")
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
                    api_err(StatusCode::BAD_GATEWAY, "gitlab_api_error", "Failed to communicate with GitLab")
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
                api_err(StatusCode::BAD_GATEWAY, "gitlab_parse_error", "Failed to parse GitLab response")
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
                    api_err(StatusCode::BAD_GATEWAY, "gitlab_api_error", "Failed to communicate with GitLab")
                })?;

            if !resp.status().is_success() {
                return Err(api_err(
                    StatusCode::BAD_GATEWAY,
                    "gitlab_unreachable",
                    "GitLab instance is unreachable or returned an error",
                ));
            }

            #[derive(serde::Deserialize)]
            struct GitLabVersion {
                version: String,
            }

            let version: GitLabVersion = resp.json().await.map_err(|e| {
                error!(error = %e, "failed to parse GitLab version response");
                api_err(StatusCode::BAD_GATEWAY, "gitlab_parse_error", "Failed to parse GitLab response")
            })?;

            let display = format!("GitLab {}", version.version);
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

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
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
    let encrypted_webhook_secret = crypto::encrypt(req.webhook_secret.trim(), &state.encryption_key).map_err(|e| {
        error!(error = %e, "failed to encrypt webhook secret");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "encryption_error", "Failed to encrypt credentials")
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
                api_err(StatusCode::INTERNAL_SERVER_ERROR, "encryption_error", "Failed to encrypt credentials")
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

            for (cred_type, value) in [("oauth_client_id", client_id), ("oauth_client_secret", client_secret)] {
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
        if let Err(e) = sync_gitlab_projects(&client, &pool, &host_url, token, &inst_id, false, now).await {
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

    let mut req_builder = client
        .get(format!("{api_base}/projects?membership=true&per_page=100&simple=true"))
        .header("User-Agent", "oore-ci");

    if use_bearer_auth {
        req_builder = req_builder.header("Authorization", format!("Bearer {token}"));
    } else {
        req_builder = req_builder.header("PRIVATE-TOKEN", token);
    }

    let resp = req_builder
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "GitLab projects API failed");
            api_err(StatusCode::BAD_GATEWAY, "gitlab_api_error", "Failed to list GitLab projects")
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        return Err(api_err(
            StatusCode::BAD_GATEWAY,
            "gitlab_api_error",
            format!("GitLab returned {status}"),
        ));
    }

    #[derive(serde::Deserialize)]
    struct GitLabProject {
        id: i64,
        path_with_namespace: String,
        default_branch: Option<String>,
        visibility: Option<String>,
        web_url: Option<String>,
    }

    let projects: Vec<GitLabProject> = resp.json().await.map_err(|e| {
        error!(error = %e, "failed to parse GitLab projects");
        api_err(StatusCode::BAD_GATEWAY, "gitlab_parse_error", "Failed to parse GitLab response")
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
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, project = %project.path_with_namespace, "failed to upsert GitLab project");
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to store project")
        })?;
    }

    info!(project_count = projects.len(), "GitLab projects synced");
    Ok(())
}

// ── GitLab OAuth flow ────────────────────────────────────────────

/// Encrypted payload stored in the `state` query parameter for GitLab OAuth.
#[derive(Debug, Serialize, Deserialize)]
struct GitLabOAuthState {
    integration_id: String,
    redirect_url: String,
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
    let state: GitLabOAuthState =
        serde_json::from_str(&json).map_err(|e| format!("parse: {e}"))?;

    let now = now_unix();
    if now - state.created_at > STATE_MAX_AGE_SECS {
        return Err("state token expired".into());
    }

    Ok(state)
}

/// Validate that a redirect URL belongs to configured trusted frontend origins.
fn validate_redirect_origin(url: &str) -> Result<(), (StatusCode, Json<ApiError>)> {
    let parsed = url::Url::parse(url).map_err(|_| {
        api_err(StatusCode::BAD_REQUEST, "invalid_redirect_url", "redirect_url is not a valid URL")
    })?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_url",
            "redirect_url must not contain credentials",
        ));
    }

    let matches_allowed = configured_redirect_origins().iter().any(|candidate| {
        parsed.scheme() == candidate.scheme()
            && parsed.host_str() == candidate.host_str()
            && parsed.port_or_known_default() == candidate.port_or_known_default()
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
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid_input", "integration_id is required"));
    }
    if req.redirect_url.is_empty() {
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid_input", "redirect_url is required"));
    }

    // Validate redirect against configured frontend origin
    validate_redirect_origin(&req.redirect_url)?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    // Load the integration
    let row = sqlx::query("SELECT * FROM integrations WHERE id = ?1")
        .bind(&req.integration_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch integration");
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to fetch integration")
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Integration not found"))?;

    let auth_mode: String = row.get("auth_mode");
    let status: String = row.get("status");
    let host_url: String = row.get("host_url");

    if auth_mode != "oauth_app" {
        return Err(api_err(StatusCode::BAD_REQUEST, "invalid_auth_mode", "Integration is not OAuth mode"));
    }
    if status != "inactive" {
        return Err(api_err(StatusCode::CONFLICT, "already_active", "Integration is already active"));
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
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to fetch credentials")
    })?
    .ok_or_else(|| api_err(StatusCode::INTERNAL_SERVER_ERROR, "missing_credentials", "OAuth client_id not found"))?;

    let client_id = crypto::decrypt(&encrypted_client_id, &state.encryption_key).map_err(|e| {
        error!(error = %e, "failed to decrypt client_id");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "encryption_error", "Failed to decrypt credentials")
    })?;

    // Build callback URL
    let public_url = std::env::var("OORE_PUBLIC_URL")
        .unwrap_or_else(|_| "http://localhost:8787".to_string());
    let callback_url = format!("{}/v1/integrations/gitlab/callback", public_url);

    // Seal the state token
    let oauth_state = GitLabOAuthState {
        integration_id: req.integration_id.clone(),
        redirect_url: req.redirect_url,
        created_at: now_unix(),
    };

    let state_token = seal_gitlab_state(&oauth_state, &state.encryption_key).map_err(|e| {
        error!(error = %e, "failed to seal GitLab OAuth state");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "encryption_error", "Failed to create state token")
    })?;

    // Build the authorize URL
    let authorize_url = format!(
        "{}/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&state={}&scope=api+read_user",
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
    // Handle GitLab error response
    if let Some(ref err) = params.error {
        let desc = params.error_description.as_deref().unwrap_or("Unknown error");
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
    if validate_redirect_origin(&oauth_state.redirect_url).is_err() {
        warn!(redirect_url = %oauth_state.redirect_url, "callback redirect_url does not match configured origin");
        return Html(error_page(
            "Invalid redirect",
            "The redirect URL does not match the configured frontend origin.",
        ))
        .into_response();
    }

    // Exchange code for tokens
    match exchange_gitlab_code(&state, &code, &oauth_state.integration_id).await {
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
    let public_url = std::env::var("OORE_PUBLIC_URL")
        .unwrap_or_else(|_| "http://localhost:8787".to_string());
    let callback_url = format!("{}/v1/integrations/gitlab/callback", public_url);

    let http_client = build_http_client().map_err(|e| format!("failed to build HTTP client: {e}"))?;

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
            ("redirect_uri", &callback_url),
            ("grant_type", "authorization_code"),
        ])
        .header("User-Agent", "oore-ci")
        .send()
        .await
        .map_err(|e| format!("GitLab token request failed: {e}"))?;

    if !token_resp.status().is_success() {
        let status = token_resp.status();
        let body = token_resp.text().await.unwrap_or_default();
        error!(status = %status, body = %body, "GitLab token exchange failed");
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
        &http_client, &pool, &host_url, &tokens.access_token, &inst_id, true, now,
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
    use super::validate_redirect_origin;

    #[test]
    fn redirect_origin_accepts_default_localhost_origin() {
        assert!(validate_redirect_origin("http://localhost:3000/settings/integrations").is_ok());
    }

    #[test]
    fn redirect_origin_rejects_untrusted_origin() {
        let err = validate_redirect_origin("https://evil.example/callback")
            .expect_err("untrusted origin should be rejected");
        assert_eq!(err.0, axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn redirect_origin_rejects_embedded_credentials() {
        let err = validate_redirect_origin("http://user:pass@localhost:3000/settings")
            .expect_err("credential-bearing URL should be rejected");
        assert_eq!(err.0, axum::http::StatusCode::BAD_REQUEST);
    }
}
