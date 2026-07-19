use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header::SET_COOKIE};
use axum::response::{Html, IntoResponse, Redirect, Response};
use oore_contract::{
    ApiError, GitHubAppCompleteRequest, GitHubAppCompleteResponse, GitHubAppStartRequest,
    GitHubAppStartResponse, Integration, IntegrationInstallation, SyncInstallationsRequest,
    SyncInstallationsResponse,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::token::hash_token;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Maximum age (seconds) for a GitHub OAuth state token.
const STATE_MAX_AGE_SECS: i64 = 600; // 10 minutes
/// Maximum age (seconds) for GitHub install-callback browser cookie.
const INSTALL_STATE_MAX_AGE_SECS: i64 = 1800; // 30 minutes
const GITHUB_API_BASE_URL: &str = "https://api.github.com";
const GITHUB_PAGE_SIZE: usize = 100;

type PendingStates = tokio::sync::Mutex<HashMap<String, i64>>;

// ponytail: process-local state fails closed across daemon restarts; persist it if
// callback continuity across restarts becomes a product requirement.
static PENDING_OAUTH_STATES: OnceLock<PendingStates> = OnceLock::new();
static PENDING_INSTALL_STATES: OnceLock<PendingStates> = OnceLock::new();

fn pending_oauth_states() -> &'static PendingStates {
    PENDING_OAUTH_STATES.get_or_init(Default::default)
}

fn pending_install_states() -> &'static PendingStates {
    PENDING_INSTALL_STATES.get_or_init(Default::default)
}

async fn register_pending_state(store: &PendingStates, token: &str, max_age_secs: i64) {
    let now = now_unix();
    let mut pending = store.lock().await;
    pending.retain(|_, expires_at| *expires_at >= now);
    pending.insert(callback_state_hash(token), now + max_age_secs);
}

async fn consume_pending_state(store: &PendingStates, token: &str) -> bool {
    let now = now_unix();
    let mut pending = store.lock().await;
    pending.retain(|_, expires_at| *expires_at >= now);
    pending.remove(&callback_state_hash(token)).is_some()
}

fn callback_state_hash(token: &str) -> String {
    hash_token(urlencoding::decode(token).as_deref().unwrap_or(token))
}

async fn current_integration_writer(
    state: &AppState,
    pool: &sqlx::SqlitePool,
    user_id: &str,
) -> Option<String> {
    let row = sqlx::query("SELECT email, role FROM users WHERE id = ?1 AND status = 'active'")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()??;
    let role: String = row.get("role");
    check_permission(&state.enforcer, &role, "integrations", "write")
        .await
        .ok()?;
    Some(row.get("email"))
}

fn build_http_client() -> Result<&'static reqwest::Client, &'static str> {
    super::scm_http_client()
}

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

fn preferred_frontend_origin(allowed_origins: &[String], public_url: Option<&str>) -> String {
    let public_origin = public_url
        .and_then(|raw| url::Url::parse(raw).ok())
        .map(|parsed| parsed.origin().ascii_serialization());

    // Prefer an origin that isn't one of the local defaults and (when present)
    // isn't the daemon's own public_url origin. This avoids redirecting users to
    // the daemon origin when multiple External Access origins are configured.
    allowed_origins
        .iter()
        .find(|origin| {
            if crate::instance_settings::DEFAULT_ALLOWED_ORIGINS.contains(&origin.as_str()) {
                return false;
            }
            match public_origin.as_ref() {
                Some(public_origin) => public_origin != origin.as_str(),
                None => true,
            }
        })
        .cloned()
        .or_else(|| {
            allowed_origins
                .iter()
                .find(|origin| {
                    !crate::instance_settings::DEFAULT_ALLOWED_ORIGINS.contains(&origin.as_str())
                })
                .cloned()
        })
        .or_else(|| allowed_origins.first().cloned())
        .unwrap_or_else(|| "http://localhost:3000".to_string())
}

fn cookie_secure_enabled(default_secure: bool) -> bool {
    match std::env::var("OORE_COOKIE_SECURE") {
        Ok(raw) => match parse_cookie_secure_override(&raw) {
            Some(value) => value,
            None => {
                warn!(
                    value = raw.trim(),
                    "invalid OORE_COOKIE_SECURE value; using default behavior"
                );
                default_secure
            }
        },
        Err(_) => default_secure,
    }
}

fn parse_cookie_secure_override(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn secure_cookie_attr(secure: bool) -> &'static str {
    if secure { "; Secure" } else { "" }
}

// ── State token ──────────────────────────────────────────────────

/// Encrypted payload stored in the `state` query parameter.
#[derive(Debug, Serialize, Deserialize)]
struct GitHubOAuthState {
    user_id: String,
    user_email: String,
    webhook_url: String,
    redirect_url: String,
    created_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct GitHubInstallState {
    integration_id: String,
    user_id: String,
    created_at: i64,
}

/// Encrypt a state payload into a URL-safe token.
fn seal_state(state: &GitHubOAuthState, key: &[u8]) -> Result<String, anyhow::Error> {
    let json = serde_json::to_string(state)?;
    let encrypted = crypto::encrypt(&json, key)?;
    // base64 is not URL-safe — use percent-encoding
    Ok(urlencoding::encode(&encrypted).into_owned())
}

/// Decrypt and validate a state token.
fn open_state(token: &str, key: &[u8]) -> Result<GitHubOAuthState, String> {
    let decoded = urlencoding::decode(token).map_err(|e| format!("url decode: {e}"))?;
    let json = crypto::decrypt(&decoded, key).map_err(|e| format!("decrypt: {e}"))?;
    let state: GitHubOAuthState = serde_json::from_str(&json).map_err(|e| format!("parse: {e}"))?;

    let now = now_unix();
    if now - state.created_at > STATE_MAX_AGE_SECS {
        return Err("state token expired".into());
    }

    Ok(state)
}

fn seal_install_state(state: &GitHubInstallState, key: &[u8]) -> Result<String, anyhow::Error> {
    let json = serde_json::to_string(state)?;
    let encrypted = crypto::encrypt(&json, key)?;
    Ok(urlencoding::encode(&encrypted).into_owned())
}

fn open_install_state(token: &str, key: &[u8]) -> Result<GitHubInstallState, String> {
    let decoded = urlencoding::decode(token).map_err(|e| format!("url decode: {e}"))?;
    let json = crypto::decrypt(&decoded, key).map_err(|e| format!("decrypt: {e}"))?;
    let state: GitHubInstallState =
        serde_json::from_str(&json).map_err(|e| format!("parse: {e}"))?;

    let now = now_unix();
    if now - state.created_at > INSTALL_STATE_MAX_AGE_SECS {
        return Err("install state token expired".into());
    }

    Ok(state)
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get("cookie")?.to_str().ok()?;
    for pair in raw.split(';') {
        let mut parts = pair.trim().splitn(2, '=');
        let key = parts.next()?.trim();
        if key != name {
            continue;
        }
        let value = parts.next().unwrap_or("").trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn set_cookie_headers(resp: &mut Response, cookie: &str) {
    if let Ok(value) = HeaderValue::from_str(cookie) {
        resp.headers_mut().append(SET_COOKIE, value);
    }
}

// ── Manifest ─────────────────────────────────────────────────────

/// Build the GitHub App manifest JSON.
fn build_manifest(webhook_url: &str, callback_url: &str, setup_url: &str) -> serde_json::Value {
    serde_json::json!({
        "name": "oore-ci",
        "url": "https://oore.build",
        "hook_attributes": {
            "url": webhook_url,
            "active": true
        },
        "redirect_url": callback_url,
        "setup_url": setup_url,
        "setup_on_update": true,
        "public": false,
        "default_permissions": {
            "contents": "read",
            "metadata": "read",
            "pull_requests": "read",
            "statuses": "write",
            "checks": "write"
        },
        "default_events": [
            "push",
            "pull_request",
            "check_run",
            "check_suite"
        ]
    })
}

/// Derive the backend base URL from the webhook URL.
/// e.g. `https://example.com/v1/webhooks/github` → `https://example.com`
fn base_url_from_webhook(webhook_url: &str) -> String {
    webhook_url
        .trim_end_matches("/v1/webhooks/github")
        .trim_end_matches('/')
        .to_string()
}

// ── Step 1: POST /v1/integrations/github/start ───────────────────

/// `POST /v1/integrations/github/start`
///
/// Creates an encrypted state token and returns a `create_url` that the
/// frontend should navigate the browser to.
pub async fn github_start(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<GitHubAppStartRequest>,
) -> ApiResult<GitHubAppStartResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "write").await?;
    let pool = state.db.clone();
    require_remote_mode(&pool).await?;

    if req.webhook_url.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "webhook_url is required",
        ));
    }

    if url::Url::parse(&req.webhook_url).is_err() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "webhook_url is not a valid URL",
        ));
    }

    if req.redirect_url.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "redirect_url is required",
        ));
    }
    let allowed_origins = state.allowed_origins.read().await.clone();
    validate_redirect_origin(&req.redirect_url, &allowed_origins)?;

    let oauth_state = GitHubOAuthState {
        user_id: auth.0.user_id.clone(),
        user_email: auth.0.email.clone(),
        webhook_url: req.webhook_url.clone(),
        redirect_url: req.redirect_url,
        created_at: now_unix(),
    };

    let token = seal_state(&oauth_state, &state.encryption_key).map_err(|e| {
        error!(error = %e, "failed to seal state token");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "encryption_error",
            "Failed to create state token",
        )
    })?;
    register_pending_state(pending_oauth_states(), &token, STATE_MAX_AGE_SECS).await;

    let base_url = base_url_from_webhook(&req.webhook_url);
    let create_url = format!("{}/v1/integrations/github/create?state={}", base_url, token);

    Ok(Json(GitHubAppStartResponse { create_url }))
}

// ── Step 2: GET /v1/integrations/github/create ───────────────────

#[derive(Deserialize)]
pub struct CreatePageQuery {
    state: String,
}

/// `GET /v1/integrations/github/create?state=...`
///
/// Serves an HTML page with a hidden form that auto-POSTs the manifest
/// to GitHub's "new app from manifest" page. No auth middleware — the
/// encrypted state token provides authentication.
pub async fn github_create_page(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CreatePageQuery>,
) -> Response {
    let pool = state.db.clone();
    if require_remote_mode(&pool).await.is_err() {
        return Html(error_page(
            "Remote mode required",
            "GitHub integration setup is available only when remote mode is enabled.",
        ))
        .into_response();
    }

    let oauth_state = match open_state(&params.state, &state.encryption_key) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "invalid github create state token");
            return Html(error_page(
                "Invalid or expired link",
                "Please go back and try again.",
            ))
            .into_response();
        }
    };

    let base_url = base_url_from_webhook(&oauth_state.webhook_url);
    let callback_url = format!("{}/v1/integrations/github/callback", base_url);
    let setup_url = format!("{}/v1/integrations/github/installed", base_url);
    let manifest = build_manifest(&oauth_state.webhook_url, &callback_url, &setup_url);
    let manifest_json = manifest.to_string();

    // Re-seal the state for the callback (fresh timestamp not needed — same token)
    let html = format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <link rel="icon" href="{favicon}">
  <link rel="apple-touch-icon" href="{favicon}">
  <meta name="theme-color" content="#2457c5">
  <title>Creating GitHub App...</title>
  <style>
    body {{
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      display: flex; align-items: center; justify-content: center;
      min-height: 100vh; margin: 0; background: #0a0a0a; color: #fafafa;
    }}
    .container {{ text-align: center; }}
    .spinner {{
      width: 32px; height: 32px; border: 3px solid #333;
      border-top-color: #fafafa; border-radius: 50%;
      animation: spin 0.8s linear infinite; margin: 0 auto 16px;
    }}
    @keyframes spin {{ to {{ transform: rotate(360deg); }} }}
  </style>
</head>
<body>
  <div class="container">
    <div class="spinner"></div>
    <p>Redirecting to GitHub...</p>
  </div>
  <form id="manifest-form" action="https://github.com/settings/apps/new?state={state}" method="post" style="display:none">
    <input type="hidden" name="manifest" value="{manifest_escaped}">
  </form>
  <script>document.getElementById('manifest-form').submit();</script>
</body>
</html>"##,
        favicon = favicon_data_uri(),
        state = urlencoding::encode(&params.state),
        manifest_escaped = html_escape(&manifest_json),
    );

    Html(html).into_response()
}

// ── Step 3: GET /v1/integrations/github/callback ─────────────────

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

/// `GET /v1/integrations/github/callback?code=...&state=...`
///
/// GitHub redirects here after the user creates the app. Exchanges the
/// code for credentials, stores them, and redirects back to the frontend.
pub async fn github_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CallbackQuery>,
) -> Response {
    let pool = state.db.clone();
    if require_remote_mode(&pool).await.is_err() {
        return Html(error_page(
            "Remote mode required",
            "GitHub integration setup is available only when remote mode is enabled.",
        ))
        .into_response();
    }

    let code = match params.code {
        Some(c) if !c.is_empty() => c,
        _ => {
            return Html(error_page(
                "Missing code",
                "GitHub did not provide a code. Please try again.",
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

    let oauth_state = match open_state(&state_token, &state.encryption_key) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "invalid github callback state token");
            return Html(error_page(
                "Invalid or expired link",
                "The setup link has expired. Please go back and start again.",
            ))
            .into_response();
        }
    };
    if !consume_pending_state(pending_oauth_states(), &state_token).await {
        warn!("replayed or unissued github callback state token");
        return Html(error_page(
            "Invalid or expired link",
            "The setup link has expired. Please go back and start again.",
        ))
        .into_response();
    }
    let Some(current_user_email) =
        current_integration_writer(&state, &pool, &oauth_state.user_id).await
    else {
        warn!(user_id = %oauth_state.user_id, "github callback initiator is no longer authorized");
        return Html(error_page(
            "Authorization expired",
            "The user who started this setup is no longer authorized. Please start again.",
        ))
        .into_response();
    };
    let allowed_origins = state.allowed_origins.read().await.clone();
    if validate_redirect_origin(&oauth_state.redirect_url, &allowed_origins).is_err() {
        warn!(
            redirect_url = %oauth_state.redirect_url,
            "github callback redirect_url does not match configured origins"
        );
        return Html(error_page(
            "Invalid redirect",
            "The redirect URL does not match configured frontend origins.",
        ))
        .into_response();
    }
    let cookie_secure = cookie_secure_enabled(
        base_url_from_webhook(&oauth_state.webhook_url).starts_with("https://"),
    );

    // Exchange code for credentials
    match exchange_and_store(&state, &code, &oauth_state.user_id, &current_user_email).await {
        Ok(integration) => {
            info!(
                integration_id = %integration.id,
                display_name = ?integration.display_name,
                "GitHub App created via callback"
            );

            let install_state = GitHubInstallState {
                integration_id: integration.id.clone(),
                user_id: oauth_state.user_id.clone(),
                created_at: now_unix(),
            };
            let sealed_install_state =
                match seal_install_state(&install_state, &state.encryption_key) {
                    Ok(token) => {
                        register_pending_state(
                            pending_install_states(),
                            &token,
                            INSTALL_STATE_MAX_AGE_SECS,
                        )
                        .await;
                        Some(token)
                    }
                    Err(error) => {
                        warn!(%error, "failed to seal github install state");
                        None
                    }
                };

            // Redirect to GitHub install page so user can install the app on their org/account
            if let Some(ref slug) = integration.app_slug {
                let install_url = format!("https://github.com/apps/{}/installations/new", slug);
                let mut resp = Redirect::to(&install_url).into_response();
                if let Some(token) = sealed_install_state {
                    let cookie = format!(
                        "oore_gh_install_state={token}; Max-Age={INSTALL_STATE_MAX_AGE_SECS}; Path=/v1/integrations/github/installed; HttpOnly; SameSite=Lax{}",
                        secure_cookie_attr(cookie_secure),
                    );
                    set_cookie_headers(&mut resp, &cookie);
                }
                resp
            } else {
                // Fallback: redirect to frontend if slug is not available
                let sep = if oauth_state.redirect_url.contains('?') {
                    "&"
                } else {
                    "?"
                };
                let redirect_url = format!(
                    "{}{}github=success&integration_id={}",
                    oauth_state.redirect_url, sep, integration.id
                );
                let mut resp = Redirect::to(&redirect_url).into_response();
                if let Some(token) = sealed_install_state {
                    let cookie = format!(
                        "oore_gh_install_state={token}; Max-Age={INSTALL_STATE_MAX_AGE_SECS}; Path=/v1/integrations/github/installed; HttpOnly; SameSite=Lax{}",
                        secure_cookie_attr(cookie_secure),
                    );
                    set_cookie_headers(&mut resp, &cookie);
                }
                resp
            }
        }
        Err(msg) => {
            error!(error = %msg, "GitHub callback exchange failed");
            Html(error_page(
                "Setup failed",
                &format!(
                    "Failed to complete GitHub App setup: {}. Please try again.",
                    msg
                ),
            ))
            .into_response()
        }
    }
}

// ── Step 4: GET /v1/integrations/github/installed ─────────────────

#[derive(Deserialize)]
pub struct InstalledQuery {
    installation_id: Option<i64>,
    setup_action: Option<String>,
}

/// `GET /v1/integrations/github/installed?installation_id=...&setup_action=...`
///
/// GitHub redirects here after a user installs the GitHub App on their org/account.
/// This endpoint is unauthenticated (browser redirect from GitHub). It serves a
/// simple HTML page that auto-redirects to the frontend integration detail page.
pub async fn github_installed(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<InstalledQuery>,
) -> Response {
    info!(
        installation_id = ?params.installation_id,
        setup_action = ?params.setup_action,
        "GitHub App installation callback"
    );

    let install_state = if let Some(token) = cookie_value(&headers, "oore_gh_install_state") {
        match open_install_state(&token, &state.encryption_key) {
            Ok(install_state) if consume_pending_state(pending_install_states(), &token).await => {
                Some(install_state)
            }
            _ => {
                warn!("invalid, expired, or replayed github install callback state");
                None
            }
        }
    } else {
        None
    };
    // Clone the pool, then release the store lock so perform_sync (which makes
    // HTTP calls) doesn't hold it.
    let pool = state.db.clone();
    let integration_id = match install_state {
        Some(install_state)
            if current_integration_writer(&state, &pool, &install_state.user_id)
                .await
                .is_some() =>
        {
            Some(install_state.integration_id)
        }
        Some(install_state) => {
            warn!(user_id = %install_state.user_id, "github install callback initiator is no longer authorized");
            None
        }
        None => None,
    };

    if require_remote_mode(&pool).await.is_err() {
        return Html(error_page(
            "Remote mode required",
            "GitHub integration setup is available only when remote mode is enabled.",
        ))
        .into_response();
    }

    // Auto-sync installations and repos so the detail page shows fresh data
    if let Some(ref id) = integration_id {
        match perform_sync(&pool, &state.encryption_key, id).await {
            Ok(installations) => info!(
                integration_id = %id,
                count = installations.len(),
                "auto-synced on install callback"
            ),
            Err(e) => warn!(error = %e, "auto-sync failed on install callback"),
        }
    }

    let allowed_origins = state.allowed_origins.read().await.clone();
    let public_url = state.public_url.read().await.clone();
    let frontend_base = preferred_frontend_origin(&allowed_origins, public_url.as_deref());

    let redirect_target = if let Some(ref id) = integration_id {
        format!(
            "{}/settings/integrations/{}?installed=true",
            frontend_base, id
        )
    } else {
        format!("{}/settings/integrations?github=success", frontend_base)
    };

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="refresh" content="2;url={redirect_url}">
  <link rel="icon" href="{favicon}">
  <link rel="apple-touch-icon" href="{favicon}">
  <meta name="theme-color" content="#2457c5">
  <title>GitHub App Installed</title>
  <style>
    body {{
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      display: flex; align-items: center; justify-content: center;
      min-height: 100vh; margin: 0; background: #0a0a0a; color: #fafafa;
    }}
    .container {{ text-align: center; }}
    .spinner {{
      width: 32px; height: 32px; border: 3px solid #333;
      border-top-color: #fafafa; border-radius: 50%;
      animation: spin 0.8s linear infinite; margin: 0 auto 16px;
    }}
    @keyframes spin {{ to {{ transform: rotate(360deg); }} }}
    a {{ color: #fbbf24; }}
  </style>
</head>
<body>
  <div class="container">
    <div class="spinner"></div>
    <p>GitHub App installed successfully!</p>
    <p style="color: #a1a1a1; font-size: 0.875rem;">Syncing and redirecting to Oore CI...</p>
    <p style="font-size: 0.75rem; margin-top: 16px;">
      <a href="{redirect_url}">Click here if not redirected</a>
    </p>
  </div>
</body>
    </html>"##,
        favicon = favicon_data_uri(),
        redirect_url = html_escape(&redirect_target),
    );

    let mut resp = Html(html).into_response();
    let cookie_secure = cookie_secure_enabled(
        public_url
            .as_deref()
            .is_some_and(|value| value.starts_with("https://")),
    );
    // Clear install-state cookie after callback handling.
    set_cookie_headers(
        &mut resp,
        &format!(
            "oore_gh_install_state=; Max-Age=0; Path=/v1/integrations/github/installed; HttpOnly; SameSite=Lax{}",
            secure_cookie_attr(cookie_secure)
        ),
    );
    resp
}

/// Exchange the manifest code with GitHub and store credentials.
/// Extracted so it can be called from both the callback and the JSON API.
async fn exchange_and_store(
    state: &Arc<AppState>,
    code: &str,
    user_id: &str,
    user_email: &str,
) -> Result<Integration, String> {
    let client = build_http_client().map_err(|e| format!("failed to build HTTP client: {e}"))?;
    let conversion_url = format!("https://api.github.com/app-manifests/{}/conversions", code);

    let resp = client
        .post(&conversion_url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "oore-ci")
        .send()
        .await
        .map_err(|e| format!("GitHub API request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        error!(status = %status, body = %body, "GitHub manifest conversion failed");
        return Err(format!("GitHub returned {status}"));
    }

    let conversion: GitHubManifestConversionResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GitHub response: {e}"))?;

    let now = now_unix();
    let integration_id = Uuid::new_v4().to_string();
    let display_name = conversion
        .owner
        .as_ref()
        .map(|o| format!("{} ({})", conversion.name, o.login))
        .unwrap_or_else(|| conversion.name.clone());

    let pool = state.db.clone();

    // Insert integration record
    sqlx::query(
        "INSERT INTO integrations (id, provider, host_url, auth_mode, status, display_name, app_id, app_slug, created_by, created_at, updated_at) \
         VALUES (?1, 'github', 'https://github.com', 'github_app', 'active', ?2, ?3, ?4, ?5, ?6, ?6)",
    )
    .bind(&integration_id)
    .bind(&display_name)
    .bind(conversion.id.to_string())
    .bind(&conversion.slug)
    .bind(user_id)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| format!("Failed to create integration: {e}"))?;

    // Encrypt and store credentials
    let credentials = [
        ("app_private_key", &conversion.pem),
        ("client_secret", &conversion.client_secret),
        ("oauth_client_id", &conversion.client_id),
    ];

    for (cred_type, value) in &credentials {
        let encrypted = crypto::encrypt(value, &state.encryption_key)
            .map_err(|e| format!("Failed to encrypt {cred_type}: {e}"))?;

        sqlx::query(
            "INSERT INTO integration_credentials (id, integration_id, credential_type, encrypted_value, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&integration_id)
        .bind(*cred_type)
        .bind(&encrypted)
        .bind(now)
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to store {cred_type}: {e}"))?;
    }

    // Store webhook secret if present
    if let Some(ref webhook_secret) = conversion.webhook_secret {
        let encrypted = crypto::encrypt(webhook_secret, &state.encryption_key)
            .map_err(|e| format!("Failed to encrypt webhook_secret: {e}"))?;

        sqlx::query(
            "INSERT INTO integration_credentials (id, integration_id, credential_type, encrypted_value, created_at, updated_at) \
             VALUES (?1, ?2, 'webhook_secret', ?3, ?4, ?4)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&integration_id)
        .bind(&encrypted)
        .bind(now)
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to store webhook_secret: {e}"))?;
    }

    let details = serde_json::json!({
        "provider": "github",
        "app_id": conversion.id,
        "display_name": display_name,
        "created_by": user_email,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(user_id),
        "integration_created",
        "integration",
        Some(&integration_id),
        Some(&details),
    )
    .await;

    Ok(Integration {
        id: integration_id,
        provider: "github".to_string(),
        host_url: "https://github.com".to_string(),
        auth_mode: "github_app".to_string(),
        status: "active".to_string(),
        display_name: Some(display_name),
        app_id: Some(conversion.id.to_string()),
        app_slug: conversion.slug.clone(),
        created_by: user_id.to_string(),
        created_at: now,
        updated_at: now,
    })
}

// ── JSON API fallback: POST /v1/integrations/github/complete ─────

/// Response from GitHub's POST /app-manifests/{code}/conversions endpoint.
#[derive(Debug, Deserialize)]
struct GitHubManifestConversionResponse {
    id: i64,
    slug: Option<String>,
    name: String,
    pem: String,
    webhook_secret: Option<String>,
    client_id: String,
    client_secret: String,
    owner: Option<GitHubOwner>,
}

#[derive(Debug, Deserialize)]
struct GitHubOwner {
    login: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    owner_type: Option<String>,
}

/// `POST /v1/integrations/github/complete` — exchange manifest code for app credentials.
/// This is the JSON API fallback (e.g. for CLI use). The primary flow uses the callback.
pub async fn github_complete(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<GitHubAppCompleteRequest>,
) -> ApiResult<GitHubAppCompleteResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "write").await?;
    let pool = state.db.clone();
    require_remote_mode(&pool).await?;

    if req.code.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "code is required",
        ));
    }

    let integration = exchange_and_store(&state, &req.code, &auth.0.user_id, &auth.0.email)
        .await
        .map_err(|msg| api_err(StatusCode::BAD_GATEWAY, "github_conversion_failed", msg))?;

    Ok(Json(GitHubAppCompleteResponse { integration }))
}

// ── Installations sync ───────────────────────────────────────────

/// GitHub API response for listing installations.
#[derive(Debug, Deserialize)]
struct GitHubInstallation {
    id: i64,
    account: GitHubOwner,
    target_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubRepositoriesResponse {
    repositories: Vec<GitHubRepository>,
}

#[derive(Debug, Deserialize)]
struct GitHubRepository {
    id: i64,
    full_name: String,
    default_branch: Option<String>,
    private: bool,
    html_url: Option<String>,
    owner: Option<GitHubRepositoryOwner>,
}

#[derive(Debug, Deserialize)]
struct GitHubRepositoryOwner {
    avatar_url: Option<String>,
}

/// Generate a GitHub App JWT for authenticating as the app.
fn generate_github_jwt(
    app_id: &str,
    private_key_pem: &str,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    use jsonwebtoken::{Algorithm, EncodingKey, Header};

    #[derive(Serialize)]
    struct Claims {
        iat: i64,
        exp: i64,
        iss: String,
    }

    let now = now_unix();
    let claims = Claims {
        iat: now - 60,
        exp: now + (10 * 60),
        iss: app_id.to_string(),
    };

    let key = EncodingKey::from_rsa_pem(private_key_pem.as_bytes()).map_err(|e| {
        error!(error = %e, "failed to parse GitHub App private key");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "key_error",
            "Invalid GitHub App private key",
        )
    })?;

    let header = Header::new(Algorithm::RS256);
    jsonwebtoken::encode(&header, &claims, &key).map_err(|e| {
        error!(error = %e, "failed to generate GitHub JWT");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "jwt_error",
            "Failed to generate authentication token",
        )
    })
}

async fn installation_access_token(
    client: &reqwest::Client,
    app_id: &str,
    private_key: &str,
    installation_id: &str,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let jwt = generate_github_jwt(app_id, private_key)?;
    let response = client
        .post(format!(
            "https://api.github.com/app/installations/{installation_id}/access_tokens"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "oore-ci")
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "failed to get GitHub installation access token");
            api_err(
                StatusCode::BAD_GATEWAY,
                "github_api_error",
                "Failed to authenticate with GitHub",
            )
        })?;
    if !response.status().is_success() {
        return Err(api_err(
            StatusCode::BAD_GATEWAY,
            "github_api_error",
            format!("GitHub returned {} for access token", response.status()),
        ));
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        token: String,
    }

    response
        .json::<TokenResponse>()
        .await
        .map(|response| response.token)
        .map_err(|e| {
            error!(error = %e, "failed to parse GitHub access token response");
            api_err(
                StatusCode::BAD_GATEWAY,
                "github_parse_error",
                "Failed to parse GitHub response",
            )
        })
}

pub(crate) async fn load_installation_access_token(
    client: &reqwest::Client,
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    integration_id: &str,
    installation_id: &str,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let row = sqlx::query(
        "SELECT i.app_id, c.encrypted_value FROM integrations i \
         JOIN integration_credentials c ON c.integration_id = i.id \
         WHERE i.id = ?1 AND c.credential_type = 'app_private_key'",
    )
    .bind(integration_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to load GitHub credentials");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load source credentials",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::CONFLICT,
            "missing_credentials",
            "GitHub credentials are missing; reconnect the source",
        )
    })?;
    let app_id: Option<String> = row.get("app_id");
    let encrypted_key: String = row.get("encrypted_value");
    let private_key = crypto::decrypt(&encrypted_key, encryption_key).map_err(|e| {
        error!(error = %e, "failed to decrypt GitHub credentials");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "encryption_error",
            "Failed to decrypt source credentials",
        )
    })?;
    installation_access_token(
        client,
        app_id.as_deref().ok_or_else(|| {
            api_err(
                StatusCode::CONFLICT,
                "missing_credentials",
                "GitHub App ID is missing; reconnect the source",
            )
        })?,
        &private_key,
        installation_id,
    )
    .await
}

pub(crate) async fn resolve_branch_commit(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    integration_id: &str,
    installation_id: &str,
    full_name: &str,
    branch: &str,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let client = build_http_client().map_err(|e| {
        error!(error = %e, "failed to build GitHub client");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "http_client_error",
            "Failed to create GitHub client",
        )
    })?;
    let token = load_installation_access_token(
        client,
        pool,
        encryption_key,
        integration_id,
        installation_id,
    )
    .await?;
    let response = client
        .get(format!(
            "https://api.github.com/repos/{full_name}/commits/{}",
            urlencoding::encode(branch)
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "oore-ci")
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "failed to resolve GitHub branch");
            api_err(
                StatusCode::BAD_GATEWAY,
                "github_api_error",
                "Failed to resolve the GitHub branch",
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
            "github_api_error",
            format!(
                "GitHub returned {} while resolving the branch",
                response.status()
            ),
        ));
    }

    #[derive(Deserialize)]
    struct CommitResponse {
        sha: String,
    }
    response
        .json::<CommitResponse>()
        .await
        .map(|response| response.sha)
        .map_err(|e| {
            error!(error = %e, "failed to parse GitHub commit response");
            api_err(
                StatusCode::BAD_GATEWAY,
                "github_parse_error",
                "Failed to parse GitHub commit response",
            )
        })
}

pub(crate) async fn compare_commits(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    integration_id: &str,
    installation_id: &str,
    full_name: &str,
    base: &str,
    head: &str,
) -> Result<Vec<super::CommitSummary>, (StatusCode, Json<ApiError>)> {
    #[derive(Deserialize)]
    struct CompareResponse {
        commits: Vec<CompareCommit>,
    }
    #[derive(Deserialize)]
    struct CompareCommit {
        author: Option<GitHubUser>,
        commit: GitCommit,
    }
    #[derive(Deserialize)]
    struct GitHubUser {
        login: String,
    }
    #[derive(Deserialize)]
    struct GitCommit {
        author: GitAuthor,
        message: String,
    }
    #[derive(Deserialize)]
    struct GitAuthor {
        name: String,
    }

    let client = build_http_client().map_err(|e| {
        error!(error = %e, "failed to build GitHub client");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "http_client_error",
            "Failed to create GitHub client",
        )
    })?;
    let token = load_installation_access_token(
        client,
        pool,
        encryption_key,
        integration_id,
        installation_id,
    )
    .await?;
    let response = client
        .get(format!(
            "https://api.github.com/repos/{full_name}/compare/{}...{}",
            urlencoding::encode(base),
            urlencoding::encode(head)
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "oore-ci")
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "failed to compare GitHub commits");
            api_err(
                StatusCode::BAD_GATEWAY,
                "github_api_error",
                "Failed to compare repository revisions",
            )
        })?;
    if !response.status().is_success() {
        return Err(api_err(
            StatusCode::BAD_GATEWAY,
            "github_api_error",
            format!(
                "GitHub returned {} while comparing revisions",
                response.status()
            ),
        ));
    }
    response
        .json::<CompareResponse>()
        .await
        .map(|response| {
            response
                .commits
                .into_iter()
                .map(|commit| super::CommitSummary {
                    title: commit
                        .commit
                        .message
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                    author: commit
                        .author
                        .map(|author| author.login)
                        .unwrap_or(commit.commit.author.name),
                })
                .collect()
        })
        .map_err(|e| {
            error!(error = %e, "failed to parse GitHub comparison");
            api_err(
                StatusCode::BAD_GATEWAY,
                "github_parse_error",
                "Failed to read repository comparison",
            )
        })
}

/// Fetch every GitHub App installation page before using the result for cleanup.
async fn list_github_installations(
    client: &reqwest::Client,
    jwt: &str,
    api_base_url: &str,
) -> Result<Vec<GitHubInstallation>, String> {
    let mut installations = Vec::new();
    let mut page = 1;

    loop {
        let resp = client
            .get(format!(
                "{api_base_url}/app/installations?per_page={GITHUB_PAGE_SIZE}&page={page}"
            ))
            .header("Authorization", format!("Bearer {jwt}"))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "oore-ci")
            .send()
            .await
            .map_err(|e| format!("GitHub API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            error!(status = %status, body = %body, page, "GitHub installations list failed");
            return Err(format!("GitHub returned {status}"));
        }

        let mut response_installations: Vec<GitHubInstallation> = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse GitHub response: {e}"))?;
        let page_len = response_installations.len();
        installations.append(&mut response_installations);
        if page_len < GITHUB_PAGE_SIZE {
            break;
        }
        page += 1;
    }

    Ok(installations)
}

async fn cleanup_stale_installations(
    pool: &sqlx::SqlitePool,
    integration_id: &str,
    synced_external_ids: &[String],
) -> Result<u64, String> {
    let synced_json = serde_json::json!(synced_external_ids).to_string();
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| format!("Failed to begin stale installation cleanup: {e}"))?;

    sqlx::query(
        "UPDATE projects SET repository_id = NULL \
         WHERE repository_id IN (\
             SELECT r.id FROM integration_repositories r \
             JOIN integration_installations i ON i.id = r.installation_id \
             WHERE i.integration_id = ?1 \
               AND i.external_id NOT IN (SELECT value FROM json_each(?2))\
         )",
    )
    .bind(integration_id)
    .bind(&synced_json)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("Failed to unlink projects from stale installations: {e}"))?;

    sqlx::query(
        "DELETE FROM integration_repositories WHERE installation_id IN (\
             SELECT id FROM integration_installations \
             WHERE integration_id = ?1 \
               AND external_id NOT IN (SELECT value FROM json_each(?2))\
         )",
    )
    .bind(integration_id)
    .bind(&synced_json)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("Failed to remove repositories from stale installations: {e}"))?;

    let deleted = sqlx::query(
        "DELETE FROM integration_installations \
         WHERE integration_id = ?1 \
           AND external_id NOT IN (SELECT value FROM json_each(?2))",
    )
    .bind(integration_id)
    .bind(&synced_json)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("Failed to clean up stale installations: {e}"))?;

    tx.commit()
        .await
        .map_err(|e| format!("Failed to commit stale installation cleanup: {e}"))?;
    Ok(deleted.rows_affected())
}

/// Core sync logic — fetches installations and repos from GitHub, upserts them,
/// and removes stale records that are no longer present on GitHub.
///
/// Callable from both the authenticated handler and the unauthenticated install callback.
pub(crate) async fn perform_sync(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    integration_id: &str,
) -> Result<Vec<IntegrationInstallation>, String> {
    let row = sqlx::query("SELECT * FROM integrations WHERE id = ?1")
        .bind(integration_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to fetch integration: {e}"))?
        .ok_or_else(|| "Integration not found".to_string())?;

    let provider: String = row.get("provider");
    let auth_mode: String = row.get("auth_mode");
    let app_id: Option<String> = row.get("app_id");

    if provider != "github" || auth_mode != "github_app" {
        return Err("Not a GitHub App integration".to_string());
    }

    let app_id = app_id.ok_or("Integration missing app_id")?;

    let encrypted_key: Option<String> = sqlx::query_scalar(
        "SELECT encrypted_value FROM integration_credentials \
         WHERE integration_id = ?1 AND credential_type = 'app_private_key'",
    )
    .bind(integration_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch credentials: {e}"))?;

    let encrypted_key = encrypted_key.ok_or("Integration missing private key")?;

    let private_key = crypto::decrypt(&encrypted_key, encryption_key)
        .map_err(|e| format!("Failed to decrypt credentials: {e}"))?;

    let jwt = generate_github_jwt(&app_id, &private_key)
        .map_err(|(_, e)| format!("JWT generation failed: {}", e.0.error))?;

    let client = build_http_client().map_err(|e| format!("failed to build HTTP client: {e}"))?;
    let gh_installations = list_github_installations(client, &jwt, GITHUB_API_BASE_URL).await?;

    let now = now_unix();
    let mut installations = Vec::new();
    let mut synced_external_ids: Vec<String> = Vec::new();

    for gh_inst in &gh_installations {
        let inst_id = Uuid::new_v4().to_string();
        let external_id = gh_inst.id.to_string();
        synced_external_ids.push(external_id.clone());

        sqlx::query(
            "INSERT INTO integration_installations (id, integration_id, external_id, account_name, account_type, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6) \
             ON CONFLICT(integration_id, external_id) DO UPDATE SET \
             account_name = excluded.account_name, account_type = excluded.account_type, updated_at = excluded.updated_at \
             RETURNING id",
        )
        .bind(&inst_id)
        .bind(integration_id)
        .bind(&external_id)
        .bind(&gh_inst.account.login)
        .bind(&gh_inst.target_type)
        .bind(now)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("Failed to store installation: {e}"))?;

        let actual_id: String = sqlx::query_scalar(
            "SELECT id FROM integration_installations WHERE integration_id = ?1 AND external_id = ?2",
        )
        .bind(integration_id)
        .bind(&external_id)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("Failed to fetch installation: {e}"))?;

        installations.push(IntegrationInstallation {
            id: actual_id.clone(),
            integration_id: integration_id.to_string(),
            external_id,
            account_name: gh_inst.account.login.clone(),
            account_type: gh_inst.target_type.clone(),
            created_at: now,
        });

        sync_installation_repos(
            client,
            pool,
            encryption_key,
            &app_id,
            &private_key,
            gh_inst.id,
            &actual_id,
            now,
        )
        .await
        .map_err(|(_, error)| {
            format!(
                "Failed to sync repositories for installation {}: {}",
                gh_inst.id, error.error
            )
        })?;
    }

    // Remove installations (and their repos) that no longer exist on GitHub
    let removed = cleanup_stale_installations(pool, integration_id, &synced_external_ids).await?;

    if removed > 0 {
        info!(
            integration_id = %integration_id,
            removed,
            "removed stale installations"
        );
    }

    info!(
        integration_id = %integration_id,
        installation_count = installations.len(),
        "GitHub installations synced"
    );

    Ok(installations)
}

/// `POST /v1/integrations/{id}/installations` — sync GitHub App installations and repos.
pub async fn sync_installations(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    axum::extract::Path(integration_id): axum::extract::Path<String>,
    Json(_req): Json<SyncInstallationsRequest>,
) -> ApiResult<SyncInstallationsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "integrations", "write").await?;

    let pool = state.db.clone();
    require_remote_mode(&pool).await?;

    let installations = perform_sync(&pool, &state.encryption_key, &integration_id)
        .await
        .map_err(|msg| {
            error!(error = %msg, "sync failed");
            api_err(StatusCode::BAD_GATEWAY, "sync_failed", msg)
        })?;

    Ok(Json(SyncInstallationsResponse { installations }))
}

/// Fetch repos for a GitHub App installation and upsert them.
#[allow(clippy::too_many_arguments)]
async fn sync_installation_repos(
    client: &reqwest::Client,
    pool: &sqlx::SqlitePool,
    _encryption_key: &[u8],
    app_id: &str,
    private_key: &str,
    installation_id_external: i64,
    installation_id_internal: &str,
    now: i64,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let token = installation_access_token(
        client,
        app_id,
        private_key,
        &installation_id_external.to_string(),
    )
    .await?;

    sync_installation_repos_with_token(
        client,
        pool,
        GITHUB_API_BASE_URL,
        &token,
        installation_id_external,
        installation_id_internal,
        now,
    )
    .await
}

async fn sync_installation_repos_with_token(
    client: &reqwest::Client,
    pool: &sqlx::SqlitePool,
    api_base_url: &str,
    token: &str,
    installation_id_external: i64,
    installation_id_internal: &str,
    now: i64,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let mut repositories = Vec::new();
    let mut page = 1;

    loop {
        let repos_resp = client
            .get(format!(
                "{api_base_url}/installation/repositories?per_page={GITHUB_PAGE_SIZE}&page={page}"
            ))
            .header("Authorization", format!("token {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "oore-ci")
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, page, "failed to list installation repos");
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "github_api_error",
                    "Failed to list repositories",
                )
            })?;

        if !repos_resp.status().is_success() {
            let status = repos_resp.status();
            return Err(api_err(
                StatusCode::BAD_GATEWAY,
                "github_api_error",
                format!("GitHub returned {status} for repos"),
            ));
        }

        let mut response: GitHubRepositoriesResponse = repos_resp.json().await.map_err(|e| {
            error!(error = %e, page, "failed to parse repos response");
            api_err(
                StatusCode::BAD_GATEWAY,
                "github_parse_error",
                "Failed to parse repos response",
            )
        })?;
        let page_len = response.repositories.len();
        repositories.append(&mut response.repositories);
        if page_len < GITHUB_PAGE_SIZE {
            break;
        }
        page += 1;
    }

    let mut tx = pool.begin().await.map_err(|e| {
        error!(error = %e, "failed to begin GitHub repository sync transaction");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to sync repositories",
        )
    })?;

    let mut synced_repo_ids: Vec<String> = Vec::with_capacity(repositories.len());

    for repo in &repositories {
        let repo_id = Uuid::new_v4().to_string();
        let external_id = repo.id.to_string();
        synced_repo_ids.push(external_id.clone());

        sqlx::query(
            "INSERT INTO integration_repositories (id, installation_id, external_id, full_name, default_branch, is_private, html_url, avatar_url, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9) \
             ON CONFLICT(installation_id, external_id) DO UPDATE SET \
             full_name = excluded.full_name, default_branch = excluded.default_branch, \
             is_private = excluded.is_private, html_url = excluded.html_url, avatar_url = excluded.avatar_url, updated_at = excluded.updated_at",
        )
        .bind(&repo_id)
        .bind(installation_id_internal)
        .bind(&external_id)
        .bind(&repo.full_name)
        .bind(&repo.default_branch)
        .bind(repo.private as i32)
        .bind(&repo.html_url)
        .bind(repo.owner.as_ref().and_then(|owner| owner.avatar_url.as_ref()))
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            error!(error = %e, repo = %repo.full_name, "failed to upsert repo");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to store repository",
            )
        })?;
    }

    let synced_json = serde_json::json!(synced_repo_ids).to_string();
    sqlx::query(
        "UPDATE projects SET repository_id = NULL \
         WHERE repository_id IN (\
             SELECT id FROM integration_repositories \
             WHERE installation_id = ?1 \
               AND external_id NOT IN (SELECT value FROM json_each(?2))\
         )",
    )
    .bind(installation_id_internal)
    .bind(&synced_json)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to unlink projects from stale GitHub repositories");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to clean up stale repositories",
        )
    })?;

    let deleted = sqlx::query(
        "DELETE FROM integration_repositories \
         WHERE installation_id = ?1 \
           AND external_id NOT IN (SELECT value FROM json_each(?2))",
    )
    .bind(installation_id_internal)
    .bind(&synced_json)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to clean up stale repos");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to clean up stale repositories",
        )
    })?;

    tx.commit().await.map_err(|e| {
        error!(error = %e, "failed to commit GitHub repository sync");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to sync repositories",
        )
    })?;

    if deleted.rows_affected() > 0 {
        info!(
            installation_id = %installation_id_external,
            removed = deleted.rows_affected(),
            "removed stale repos"
        );
    }

    info!(
        installation_id = %installation_id_external,
        repo_count = repositories.len(),
        "repos synced"
    );

    Ok(())
}

// ── HTML helpers (re-exported from parent module) ─────────────────
use super::{error_page, favicon_data_uri, html_escape, require_remote_mode};

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use axum::Json;
    use axum::extract::Query;
    use axum::routing::get;

    use super::{
        INSTALL_STATE_MAX_AGE_SECS, build_http_client, cleanup_stale_installations,
        consume_pending_state, list_github_installations, parse_cookie_secure_override,
        pending_install_states, preferred_frontend_origin, register_pending_state,
        sync_installation_repos_with_token, validate_redirect_origin,
    };

    #[tokio::test]
    async fn github_sync_paginates_and_preserves_or_resets_repository_approval() {
        let app = axum::Router::new()
            .route(
                "/app/installations",
                get(|Query(params): Query<HashMap<String, String>>| async move {
                    let page = params.get("page").map(String::as_str).unwrap_or("1");
                    let installations: Vec<_> = if page == "1" {
                        (1..=100)
                            .map(|id| {
                                serde_json::json!({
                                    "id": id,
                                    "account": { "login": format!("org-{id}") },
                                    "target_type": "Organization",
                                })
                            })
                            .collect()
                    } else {
                        vec![serde_json::json!({
                            "id": 101,
                            "account": { "login": "org-101" },
                            "target_type": "Organization",
                        })]
                    };
                    Json(installations)
                }),
            )
            .route(
                "/installation/repositories",
                get(|Query(params): Query<HashMap<String, String>>| async move {
                    let page = params.get("page").map(String::as_str).unwrap_or("1");
                    let repositories: Vec<_> = if page == "1" {
                        (1..=100)
                            .map(|id| {
                                serde_json::json!({
                                    "id": id,
                                    "full_name": format!("org/repo-{id}"),
                                    "default_branch": "main",
                                    "private": true,
                                    "html_url": format!("https://github.example/org/repo-{id}"),
                                    "owner": { "avatar_url": "https://github.example/avatar.png" },
                                })
                            })
                            .collect()
                    } else {
                        vec![serde_json::json!({
                            "id": 101,
                            "full_name": "org/repo-101",
                            "default_branch": "main",
                            "private": true,
                            "html_url": "https://github.example/org/repo-101",
                        })]
                    };
                    Json(serde_json::json!({
                        "total_count": 101,
                        "repositories": repositories,
                    }))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = build_http_client().unwrap();

        let installations = list_github_installations(client, "jwt", &host)
            .await
            .unwrap();
        assert_eq!(installations.len(), 101);

        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE integration_repositories (
                id TEXT PRIMARY KEY, installation_id TEXT NOT NULL, external_id TEXT NOT NULL,
                full_name TEXT NOT NULL, default_branch TEXT, is_private INTEGER NOT NULL,
                html_url TEXT, avatar_url TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
                allow_direct_macos_runner INTEGER NOT NULL DEFAULT 0,
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
            "INSERT INTO integration_repositories
             (id, installation_id, external_id, full_name, default_branch, is_private,
              created_at, updated_at, allow_direct_macos_runner)
             VALUES ('approved', 'install', '1', 'org/old-name', 'main', 1, 1, 1, 1),
                    ('stale', 'install', 'stale', 'org/stale', 'main', 1, 1, 1, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO projects VALUES ('project', 'stale')")
            .execute(&pool)
            .await
            .unwrap();

        sync_installation_repos_with_token(client, &pool, &host, "token", 1, "install", 2)
            .await
            .unwrap();

        let repository_count: i64 = sqlx::query_scalar(
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
        let approval: i64 = sqlx::query_scalar(
            "SELECT allow_direct_macos_runner FROM integration_repositories WHERE external_id = '1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(repository_count, 101);
        assert!(linked.is_none());
        assert_eq!(approval, 1, "repository sync must preserve runner approval");

        sqlx::query("DELETE FROM integration_repositories WHERE external_id = '1'")
            .execute(&pool)
            .await
            .unwrap();
        sync_installation_repos_with_token(client, &pool, &host, "token", 1, "install", 3)
            .await
            .unwrap();
        let readded_approval: i64 = sqlx::query_scalar(
            "SELECT allow_direct_macos_runner FROM integration_repositories WHERE external_id = '1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            readded_approval, 0,
            "deleting and rediscovering a repository must reset runner approval"
        );
        server.abort();
    }

    #[tokio::test]
    async fn stale_installation_cleanup_unlinks_projects_before_deletion() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE integration_installations (
                id TEXT PRIMARY KEY, integration_id TEXT NOT NULL, external_id TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE integration_repositories (
                id TEXT PRIMARY KEY, installation_id TEXT NOT NULL
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
            "INSERT INTO integration_installations VALUES
             ('current', 'integration', '1'), ('stale', 'integration', '2')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO integration_repositories VALUES
             ('current-repo', 'current'), ('stale-repo', 'stale')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO projects VALUES ('project', 'stale-repo')")
            .execute(&pool)
            .await
            .unwrap();

        let removed = cleanup_stale_installations(&pool, "integration", &["1".to_string()])
            .await
            .unwrap();

        let installation_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM integration_installations")
                .fetch_one(&pool)
                .await
                .unwrap();
        let repository_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM integration_repositories")
                .fetch_one(&pool)
                .await
                .unwrap();
        let linked: Option<String> =
            sqlx::query_scalar("SELECT repository_id FROM projects WHERE id = 'project'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(removed, 1);
        assert_eq!(installation_count, 1);
        assert_eq!(repository_count, 1);
        assert!(linked.is_none());
    }

    #[test]
    fn redirect_origin_accepts_default_localhost_origin() {
        let allowed = vec!["http://localhost:3000".to_string()];
        assert!(validate_redirect_origin("http://localhost:3000/callback?ok=1", &allowed).is_ok());
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
        let err = validate_redirect_origin("http://user:pass@localhost:3000/callback", &allowed)
            .expect_err("credential-bearing URL should be rejected");
        assert_eq!(err.0, axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn cookie_secure_override_true_values() {
        for value in ["1", "true", "TRUE", " yes ", "on"] {
            assert_eq!(parse_cookie_secure_override(value), Some(true));
        }
    }

    #[test]
    fn cookie_secure_override_false_values() {
        for value in ["0", "false", "FALSE", " no ", "off"] {
            assert_eq!(parse_cookie_secure_override(value), Some(false));
        }
    }

    #[test]
    fn cookie_secure_override_invalid_value() {
        assert_eq!(parse_cookie_secure_override("maybe"), None);
        assert_eq!(parse_cookie_secure_override(""), None);
    }

    #[test]
    fn preferred_frontend_origin_skips_public_url_origin_when_possible() {
        let allowed = vec![
            "https://daemon.example.com".to_string(),
            "https://ci.oore.build".to_string(),
        ];
        let resolved = preferred_frontend_origin(&allowed, Some("https://daemon.example.com"));
        assert_eq!(resolved, "https://ci.oore.build");
    }

    #[test]
    fn preferred_frontend_origin_falls_back_to_public_url_origin_when_only_choice() {
        let allowed = vec!["https://daemon.example.com".to_string()];
        let resolved = preferred_frontend_origin(&allowed, Some("https://daemon.example.com"));
        assert_eq!(resolved, "https://daemon.example.com");
    }

    #[tokio::test]
    async fn install_callback_state_is_single_use() {
        let token = format!("install-state-{}", uuid::Uuid::new_v4());
        register_pending_state(pending_install_states(), &token, INSTALL_STATE_MAX_AGE_SECS).await;
        assert!(consume_pending_state(pending_install_states(), &token).await);
        assert!(!consume_pending_state(pending_install_states(), &token).await);
    }
}
