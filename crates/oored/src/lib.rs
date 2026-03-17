pub mod apple_api;
pub mod artifacts;
pub mod auth;
pub mod background;
pub mod builds;
pub mod crypto;
pub mod embedded_runner;
pub mod extractors;
pub mod instance_settings;
pub mod integrations;
pub mod logs;
pub mod notification_channels;
pub mod notification_dispatch;
pub mod observability;
pub mod oidc;
pub mod pipeline_ios_signing;
pub mod pipeline_signing;
pub mod pipelines;
pub mod projects;
pub mod rbac;
pub mod runners;
pub mod scheduler;
pub mod session;
pub mod storage;
pub mod store;
pub mod token;
pub mod users;
pub mod util;

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, DefaultBodyLimit, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware as axum_mw;
use axum::routing::{get, post};
use axum::{Json, Router};
use metrics_exporter_prometheus::PrometheusHandle;
use oore_contract::{
    ApiError, BootstrapTokenVerifyRequest, BootstrapTokenVerifyResponse, OidcConfigRecord,
    OidcConfigureRequest, OidcConfigureResponse, OidcSecretRecord, OwnerRecord, RemoteAuthMode,
    RuntimeMode, SetupCompleteResponse, SetupLocalOwnerCreateRequest,
    SetupLocalOwnerCreateResponse, SetupOidcStartRequest, SetupOidcStartResponse,
    SetupOidcVerifyRequest, SetupOidcVerifyResponse, SetupPreferencesRequest,
    SetupPreferencesResponse, SetupSessionRecord, SetupState, SetupStateFile, SetupStatus,
    SetupSummaryResponse, SetupTrustedProxyClaimOwnerResponse, SetupTrustedProxyConfigureRequest,
    SetupTrustedProxyConfigureResponse,
};
use serde_json::json;
use sqlx::Row;
use tokio::sync::{Mutex, RwLock};
use tower_http::cors::{AllowOrigin, CorsLayer};
use zeroize::Zeroizing;

use openidconnect::core::CoreProviderMetadata;
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndUserEmail,
    IssuerUrl, Nonce, PkceCodeChallenge, RedirectUrl, Scope, TokenResponse,
};
use tracing::{error, info, warn};

use crate::auth::{PendingAuth, build_http_client, load_oidc_config_for_setup};
use crate::session::SessionStore;
use crate::store::{SetupStore, write_audit_log};
use crate::token::{generate_session_token, hash_token};
use crate::util::{api_err, extract_bearer, now_unix};

// ── Shared application state ─────────────────────────────────────

pub struct AppState {
    pub store: Mutex<SetupStore>,
    pub sessions: SessionStore,
    pub pending_auth: Mutex<HashMap<String, PendingAuth>>,
    /// AES-256 encryption key used to encrypt secrets at rest.
    /// Wrapped in Zeroizing so the key is zeroed on drop.
    pub encryption_key: Zeroizing<Vec<u8>>,
    /// Casbin RBAC enforcer for permission checks.
    pub enforcer: rbac::CasbinEnforcer,
    /// When true, `configure_oidc` skips the real OIDC discovery HTTP call
    /// and populates the config from the raw request values with placeholder
    /// endpoint URLs. Only available with test-support feature or in tests.
    #[cfg(any(test, feature = "test-support"))]
    pub skip_oidc_discovery: bool,
    /// Failed bootstrap token verification attempts (keyed by token hash).
    pub bootstrap_failures: Mutex<HashMap<String, u32>>,
    /// In-process job scheduler for runner dispatch.
    pub scheduler: Arc<scheduler::Scheduler>,
    /// Runtime-configurable artifact storage backend.
    pub storage: Arc<RwLock<storage::StorageBackend>>,
    /// In-memory store for short-lived SSE streaming tokens.
    pub stream_tokens: logs::StreamTokenStore,
    /// Origins allowed for redirect URIs and CORS.
    pub allowed_origins: Arc<RwLock<Vec<String>>>,
    /// Effective public base URL for externally reachable callbacks/download links.
    pub public_url: Arc<RwLock<Option<String>>>,
}

// ── Constants ────────────────────────────────────────────────────

/// Convenience type alias
type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

const SETUP_SESSION_TTL_SECS: i64 = 30 * 60;

fn local_subject_for_email(email: &str) -> String {
    format!("local::{}", email.trim().to_lowercase())
}

fn trusted_proxy_subject_for_email(email: &str) -> String {
    format!("warpgate::{}", email.trim().to_lowercase())
}

/// Maximum number of concurrent pending OIDC auth requests.
const MAX_PENDING_AUTH: usize = 1000;

/// Maximum allowed failed bootstrap token attempts before lockout.
const MAX_BOOTSTRAP_FAILURES: u32 = 5;

// ── Helpers ──────────────────────────────────────────────────────

/// Check if skip_oidc_discovery is enabled (always false in production builds).
fn should_skip_oidc_discovery(state: &AppState) -> bool {
    #[cfg(any(test, feature = "test-support"))]
    {
        state.skip_oidc_discovery
    }
    #[cfg(not(any(test, feature = "test-support")))]
    {
        let _ = state;
        false
    }
}

/// Validate a redirect_uri against the allowed origins list.
///
/// Rules:
/// 1. Must be a valid URL with no embedded credentials.
/// 2. Local-network callback hosts (localhost, .local, private/link-local IPs)
///    may use `http` or `https`.
/// 3. Public callback hosts must use `https` scheme.
/// 4. Path must be exactly `/auth/callback` (the single unified callback route).
/// 5. Public callback origins must appear in `allowed_origins`.
pub fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    match host.parse::<IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

pub fn is_loopback_client(peer_addr: SocketAddr) -> bool {
    peer_addr.ip().is_loopback()
}

fn parse_ip_from_header(headers: &HeaderMap, name: &str) -> Option<IpAddr> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<IpAddr>().ok())
}

fn parse_ip_token(raw: &str) -> Option<IpAddr> {
    let mut value = raw.trim();
    if value.is_empty() {
        return None;
    }

    // Header values may be quoted.
    value = value.trim_matches('"').trim();
    if value.is_empty() || value.eq_ignore_ascii_case("unknown") {
        return None;
    }

    // IPv6 with port (Forwarded spec, best-effort): for="[2001:db8::1]:1234"
    // or XFF variants like: [2001:db8::1]:1234
    if let Some(rest) = value.strip_prefix('[')
        && let Some(end) = rest.find(']')
    {
        return rest[..end].parse::<IpAddr>().ok();
    }

    // Raw IP.
    if let Ok(ip) = value.parse::<IpAddr>() {
        return Some(ip);
    }

    // IPv4 with port (best-effort)
    if value.contains('.')
        && let Some((ip_part, _port)) = value.rsplit_once(':')
        && let Ok(ip) = ip_part.parse::<IpAddr>()
    {
        return Some(ip);
    }

    None
}

fn parse_x_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    let raw = headers.get("x-forwarded-for")?.to_str().ok()?;
    // X-Forwarded-For: client, proxy1, proxy2. When a same-host proxy appends
    // to a client-provided header, the first element can be attacker-controlled.
    // For loopback-only enforcement, return a non-loopback IP if any is present.
    let mut first: Option<IpAddr> = None;
    for token in raw.split(',') {
        let Some(ip) = parse_ip_token(token) else {
            continue;
        };
        if first.is_none() {
            first = Some(ip);
        }
        if !ip.is_loopback() {
            return Some(ip);
        }
    }
    first
}

fn parse_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    let raw = headers.get("forwarded")?.to_str().ok()?;
    // RFC 7239: Forwarded: for=<client-ip>;proto=https;by=<proxy-ip>
    // Multiple values are comma-separated. The left-most entry can be
    // attacker-controlled if an upstream proxy appends to an existing header.
    let mut first: Option<IpAddr> = None;
    for entry in raw.split(',') {
        for part in entry.split(';') {
            let part = part.trim();
            let Some(rest) = part.strip_prefix("for=") else {
                continue;
            };
            let Some(ip) = parse_ip_token(rest) else {
                continue;
            };
            if first.is_none() {
                first = Some(ip);
            }
            if !ip.is_loopback() {
                return Some(ip);
            }
        }
    }
    first
}

/// Determine the effective client IP address for a request.
///
/// For loopback peers, this optionally consults common forwarded headers that
/// same-host proxies (ex: cloudflared) set. This prevents loopback-only
/// endpoints from being reachable over a public reverse proxy that connects to
/// the daemon via 127.0.0.1.
///
/// Security: we only trust forwarded headers when the immediate peer is
/// loopback, because forwarded headers are otherwise trivially spoofable by
/// remote clients.
pub fn effective_client_ip(peer_addr: SocketAddr, headers: &HeaderMap) -> IpAddr {
    let peer_ip = peer_addr.ip();
    if !peer_ip.is_loopback() {
        return peer_ip;
    }

    // Collect candidates from trusted-by-loopback forwarded headers.
    let mut candidates = Vec::new();

    // Cloudflare: the true client IP (when set by a same-host tunnel/proxy).
    if let Some(ip) = parse_ip_from_header(headers, "cf-connecting-ip") {
        candidates.push(ip);
    }

    // RFC 7239
    if let Some(ip) = parse_forwarded_for(headers) {
        candidates.push(ip);
    }

    // De-facto standard
    if let Some(ip) = parse_x_forwarded_for(headers) {
        candidates.push(ip);
    }

    // Common nginx header
    if let Some(ip) = parse_ip_from_header(headers, "x-real-ip") {
        candidates.push(ip);
    }

    // Prefer any non-loopback claim to prevent spoofed loopback prefixes from
    // bypassing loopback-only endpoints when a proxy appends the real client IP.
    if let Some(ip) = candidates.iter().copied().find(|ip| !ip.is_loopback()) {
        return ip;
    }

    candidates.first().copied().unwrap_or(peer_ip)
}

fn is_local_network_host(host: &str) -> bool {
    if is_loopback_host(host) {
        return true;
    }

    let host_lower = host.to_ascii_lowercase();
    if host_lower.ends_with(".local") {
        return true;
    }

    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_private() || ip.is_link_local(),
        Ok(IpAddr::V6(ip)) => ip.is_unique_local() || ip.is_unicast_link_local(),
        Err(_) => false,
    }
}

pub fn validate_redirect_uri(
    uri: &str,
    allowed_origins: &[String],
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let parsed = url::Url::parse(uri).map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            "redirect_uri is not a valid URL",
        )
    })?;

    // Reject URLs with embedded credentials
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            "redirect_uri must not contain credentials",
        ));
    }

    // Validate path
    if parsed.path() != "/auth/callback" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            "redirect_uri path must be /auth/callback",
        ));
    }

    let host = parsed.host_str().unwrap_or("");
    if is_local_network_host(host) {
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_redirect_uri",
                "local-network redirect_uri must use http or https scheme",
            ));
        }
        return Ok(());
    }

    // Public host: require https
    if parsed.scheme() != "https" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            "public redirect_uri must use https scheme",
        ));
    }

    // Public callback origins must be explicitly allowlisted.
    let origin = parsed.origin().ascii_serialization();
    if !allowed_origins.iter().any(|o| o == &origin) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            "redirect_uri origin is not in the allowed origins list",
        ));
    }

    Ok(())
}

/// Validate the setup session token against the current state file.
///
/// On success, bumps the session expiry (sliding window) so that each
/// successful request resets the 30-minute TTL.
///
/// Returns `Ok(())` if the session is valid, or an appropriate HTTP error.
fn validate_session(
    state_file: &mut SetupStateFile,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let token = extract_bearer(headers).ok_or_else(|| {
        api_err(
            StatusCode::UNAUTHORIZED,
            "missing_auth",
            "Authorization header required",
        )
    })?;

    let session = state_file.setup_session.as_ref().ok_or_else(|| {
        api_err(
            StatusCode::UNAUTHORIZED,
            "no_session",
            "No active setup session",
        )
    })?;

    let hashed = hash_token(token);
    if hashed != session.hash {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_session",
            "Invalid session token",
        ));
    }

    if now_unix() > session.expires_at {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "session_expired",
            "Setup session has expired",
        ));
    }

    // Sliding window: bump session expiry on each successful validation
    if let Some(ref mut session) = state_file.setup_session {
        session.expires_at = now_unix() + SETUP_SESSION_TTL_SECS;
    }

    Ok(())
}

// ── Handlers ─────────────────────────────────────────────────────

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({"ok": true}))
}

async fn setup_status(State(state): State<Arc<AppState>>) -> ApiResult<SetupStatus> {
    let store = state.store.lock().await;
    let sf = store.load().await.map_err(|e| {
        error!(error = %e, "failed to load setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load setup state",
        )
    })?;

    let runtime_mode = crate::instance_settings::load_runtime_mode(store.pool())
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load runtime mode");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load runtime mode",
            )
        })?;
    let remote_auth_mode = crate::instance_settings::load_remote_auth_mode(store.pool())
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load remote auth mode");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load remote auth mode",
            )
        })?;

    Ok(Json(SetupStatus::from_state(
        sf.instance_id,
        sf.setup_state,
        runtime_mode,
        remote_auth_mode,
    )))
}

async fn verify_bootstrap_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BootstrapTokenVerifyRequest>,
) -> ApiResult<BootstrapTokenVerifyResponse> {
    // H6: Check failed attempt counter
    let token_hash_for_tracking = hash_token(&req.token);
    {
        let failures = state.bootstrap_failures.lock().await;
        let count = failures.get(&token_hash_for_tracking).copied().unwrap_or(0);
        if count >= MAX_BOOTSTRAP_FAILURES {
            return Err(api_err(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_attempts",
                "Too many failed verification attempts. Generate a new bootstrap token.",
            ));
        }
    }

    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        error!(error = %e, "failed to load setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load setup state",
        )
    })?;

    // Setup must not be complete — all setup endpoints are disabled after ready
    if sf.setup_state == SetupState::Ready {
        return Err(api_err(
            StatusCode::CONFLICT,
            "already_configured",
            "Setup is already complete",
        ));
    }

    // Bootstrap token record must exist
    let bt = sf.bootstrap_token.as_ref().ok_or_else(|| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "no_bootstrap_token",
            "No bootstrap token has been generated",
        )
    })?;

    // Must not already be consumed
    if bt.consumed_at.is_some() {
        return Err(api_err(
            StatusCode::GONE,
            "token_consumed",
            "Bootstrap token has already been consumed",
        ));
    }

    // Must not be expired
    if now_unix() > bt.expires_at {
        return Err(api_err(
            StatusCode::GONE,
            "token_expired",
            "Bootstrap token has expired",
        ));
    }

    // Hash must match
    let request_hash = hash_token(&req.token);
    if request_hash != bt.hash {
        // H6: Increment failed attempt counter
        let mut failures = state.bootstrap_failures.lock().await;
        let count = failures.entry(token_hash_for_tracking).or_insert(0);
        *count += 1;
        warn!(attempts = *count, "invalid bootstrap token attempt");
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_token",
            "Bootstrap token is invalid",
        ));
    }

    // Mark token as consumed
    let now = now_unix();
    if let Some(ref mut bt_mut) = sf.bootstrap_token {
        bt_mut.consumed_at = Some(now);
    }

    // Generate session token and store its hash with sliding TTL
    let session_token = generate_session_token();
    let session_expires_at = now + SETUP_SESSION_TTL_SECS;
    sf.setup_session = Some(SetupSessionRecord {
        hash: hash_token(&session_token),
        expires_at: session_expires_at,
    });

    sf.updated_at = now;

    store.save(&sf).await.map_err(|e| {
        error!(error = %e, "failed to save setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save setup state",
        )
    })?;

    Ok(Json(BootstrapTokenVerifyResponse {
        session_token,
        expires_at: session_expires_at,
    }))
}

async fn configure_oidc(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<OidcConfigureRequest>,
) -> ApiResult<OidcConfigureResponse> {
    // M7: Validate input
    if req.issuer_url.is_empty() || req.issuer_url.len() > 2048 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "issuer_url must be between 1 and 2048 characters",
        ));
    }

    if url::Url::parse(&req.issuer_url).is_err() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "issuer_url is not a valid URL",
        ));
    }

    if req.client_id.is_empty() || req.client_id.len() > 256 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "client_id must be between 1 and 256 characters",
        ));
    }

    if let Some(ref secret) = req.client_secret
        && (secret.is_empty() || secret.len() > 1024)
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "client_secret must be between 1 and 1024 characters",
        ));
    }

    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        error!(error = %e, "failed to load setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load setup state",
        )
    })?;

    // State gates first — deterministic 409 regardless of session state
    if sf.setup_state == SetupState::Ready {
        return Err(api_err(
            StatusCode::CONFLICT,
            "already_configured",
            "Setup is already complete",
        ));
    }

    let is_reconfigure = sf.setup_state == SetupState::IdpConfigured;
    if sf.setup_state != SetupState::BootstrapPending && !is_reconfigure {
        return Err(api_err(
            StatusCode::CONFLICT,
            "invalid_state",
            format!(
                "OIDC can only be configured in bootstrap_pending or idp_configured state, current: {}",
                sf.setup_state
            ),
        ));
    }

    validate_session(&mut sf, &headers)?;

    let now = now_unix();
    let has_client_secret = req.client_secret.is_some();

    // When skip_oidc_discovery is set (test mode), populate the config from
    // the raw request values with placeholder endpoint URLs instead of
    // performing a real HTTP discovery call.
    let (issuer, authorization_endpoint, token_endpoint, userinfo_endpoint, jwks_uri) =
        if should_skip_oidc_discovery(&state) {
            (
                req.issuer_url.clone(),
                format!("{}/o/oauth2/v2/auth", req.issuer_url),
                format!("{}/token", req.issuer_url),
                Some(format!("{}/userinfo", req.issuer_url)),
                format!("{}/jwks", req.issuer_url),
            )
        } else {
            let discovered = oidc::discover_provider(&req.issuer_url)
                .await
                .map_err(|e| {
                    error!(error = %e, "OIDC discovery failed");
                    api_err(
                        StatusCode::BAD_REQUEST,
                        "oidc_discovery_failed",
                        "Failed to discover OIDC provider",
                    )
                })?;
            (
                discovered.issuer,
                discovered.authorization_endpoint,
                discovered.token_endpoint,
                discovered.userinfo_endpoint,
                discovered.jwks_uri,
            )
        };

    sf.oidc_config = Some(OidcConfigRecord {
        issuer_url: issuer.clone(),
        client_id: req.client_id,
        has_client_secret,
        authorization_endpoint,
        token_endpoint,
        userinfo_endpoint,
        jwks_uri,
        configured_at: now,
    });

    // Encrypt and store the client secret separately if provided
    if let Some(secret) = req.client_secret {
        // C2: secret will be dropped (and zeroized if using Zeroizing wrapper)
        // at end of scope naturally. The encryption key is already Zeroizing.
        let encrypted = crypto::encrypt(&secret, &state.encryption_key).map_err(|e| {
            error!(error = %e, "failed to encrypt client secret");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "encryption_error",
                "Failed to encrypt client secret",
            )
        })?;
        sf.oidc_secret = Some(OidcSecretRecord {
            encrypted_client_secret: encrypted,
            stored_at: now,
        });
    }

    sf.setup_state = SetupState::IdpConfigured;
    sf.updated_at = now;

    store.save(&sf).await.map_err(|e| {
        error!(error = %e, "failed to save setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save setup state",
        )
    })?;
    drop(store);

    if is_reconfigure {
        let mut pending = state.pending_auth.lock().await;
        pending.clear();
        info!("cleared pending OIDC auth requests after OIDC reconfiguration");
    }

    Ok(Json(OidcConfigureResponse {
        state: SetupState::IdpConfigured,
        discovered_issuer: issuer,
        session_expires_at: sf.setup_session.as_ref().map(|s| s.expires_at),
    }))
}

/// `POST /v1/setup/preferences`
///
/// Persists setup-time runtime/auth mode preferences while setup is still in
/// progress. This endpoint is bootstrap-session gated.
async fn setup_preferences(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SetupPreferencesRequest>,
) -> ApiResult<SetupPreferencesResponse> {
    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        error!(error = %e, "failed to load setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load setup state",
        )
    })?;

    if sf.setup_state == SetupState::Ready {
        return Err(api_err(
            StatusCode::CONFLICT,
            "already_configured",
            "Setup is already complete",
        ));
    }

    if sf.setup_state == SetupState::OwnerCreated {
        return Err(api_err(
            StatusCode::CONFLICT,
            "invalid_state",
            "Setup preferences cannot be changed after owner creation",
        ));
    }

    validate_session(&mut sf, &headers)?;

    let runtime_mode = req.runtime_mode;
    let remote_auth_mode = if runtime_mode == RuntimeMode::Local {
        RemoteAuthMode::Oidc
    } else {
        req.remote_auth_mode.unwrap_or(RemoteAuthMode::Oidc)
    };

    let now = now_unix();
    sqlx::query(
        "INSERT INTO instance_preferences (id, key_storage_mode, runtime_mode, remote_auth_mode, updated_by, created_at, updated_at)
         VALUES (1, 'file', ?1, ?2, NULL, ?3, ?3)
         ON CONFLICT(id) DO UPDATE SET
            key_storage_mode = excluded.key_storage_mode,
            runtime_mode = excluded.runtime_mode,
            remote_auth_mode = excluded.remote_auth_mode,
            updated_at = excluded.updated_at",
    )
    .bind(runtime_mode.to_string())
    .bind(remote_auth_mode.to_string())
    .bind(now)
    .execute(store.pool())
    .await
    .map_err(|e| {
        error!(error = %e, "failed to persist setup preferences");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save setup preferences",
        )
    })?;

    // Persist bumped setup session expiry from validate_session.
    sf.updated_at = now;
    store.save(&sf).await.map_err(|e| {
        error!(error = %e, "failed to save setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save setup state",
        )
    })?;

    Ok(Json(SetupPreferencesResponse {
        runtime_mode,
        remote_auth_mode,
        session_expires_at: sf.setup_session.as_ref().map(|s| s.expires_at),
    }))
}

/// `POST /v1/setup/trusted-proxy/configure`
///
/// Configures trusted proxy runtime auth during setup and marks auth as
/// configured (`setup_state = IdpConfigured`).
async fn setup_trusted_proxy_configure(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SetupTrustedProxyConfigureRequest>,
) -> ApiResult<SetupTrustedProxyConfigureResponse> {
    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        error!(error = %e, "failed to load setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load setup state",
        )
    })?;

    if sf.setup_state == SetupState::Ready {
        return Err(api_err(
            StatusCode::CONFLICT,
            "already_configured",
            "Setup is already complete",
        ));
    }

    if sf.setup_state == SetupState::OwnerCreated {
        return Err(api_err(
            StatusCode::CONFLICT,
            "invalid_state",
            "Trusted proxy settings cannot be changed after owner creation",
        ));
    }

    validate_session(&mut sf, &headers)?;

    let runtime_mode = crate::instance_settings::load_runtime_mode(store.pool())
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load runtime mode");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to determine runtime mode",
            )
        })?;
    let remote_auth_mode = crate::instance_settings::load_remote_auth_mode(store.pool())
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load remote auth mode");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to determine remote auth mode",
            )
        })?;
    if runtime_mode != RuntimeMode::Remote || remote_auth_mode != RemoteAuthMode::TrustedProxy {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "mode_restricted",
            "Trusted proxy setup requires remote runtime with trusted_proxy auth mode",
        ));
    }

    let user_email_header = req
        .user_email_header
        .as_deref()
        .and_then(crate::instance_settings::normalize_header_name)
        .unwrap_or_else(|| {
            crate::instance_settings::DEFAULT_TRUSTED_PROXY_EMAIL_HEADER.to_string()
        });
    let trusted_proxy_cidrs =
        crate::instance_settings::normalize_requested_trusted_proxy_cidrs(req.trusted_proxy_cidrs)?;
    let trusted_proxy_cidrs_json = serde_json::to_string(&trusted_proxy_cidrs).map_err(|e| {
        error!(error = %e, "failed to serialize trusted proxy cidrs");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save trusted proxy settings",
        )
    })?;

    let encrypted_shared_secret = if let Some(secret) = req.shared_secret {
        let trimmed = secret.trim();
        if trimmed.is_empty() {
            None
        } else {
            if trimmed.len() > 1024 {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_input",
                    "shared_secret must be 1024 characters or fewer",
                ));
            }
            Some(
                crypto::encrypt(trimmed, &state.encryption_key).map_err(|e| {
                    error!(error = %e, "failed to encrypt trusted proxy shared secret");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "encryption_error",
                        "Failed to save trusted proxy shared secret",
                    )
                })?,
            )
        }
    } else {
        None
    };

    let now = now_unix();
    sqlx::query(
        "INSERT INTO trusted_proxy_settings (id, user_email_header, trusted_proxy_cidrs_json, encrypted_shared_secret, updated_by, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, NULL, ?4, ?4)
         ON CONFLICT(id) DO UPDATE SET
            user_email_header = excluded.user_email_header,
            trusted_proxy_cidrs_json = excluded.trusted_proxy_cidrs_json,
            encrypted_shared_secret = excluded.encrypted_shared_secret,
            updated_at = excluded.updated_at",
    )
    .bind(&user_email_header)
    .bind(&trusted_proxy_cidrs_json)
    .bind(encrypted_shared_secret.clone())
    .bind(now)
    .execute(store.pool())
    .await
    .map_err(|e| {
        error!(error = %e, "failed to persist trusted proxy setup settings");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save trusted proxy settings",
        )
    })?;

    sf.setup_state = SetupState::IdpConfigured;
    sf.updated_at = now;
    store.save(&sf).await.map_err(|e| {
        error!(error = %e, "failed to save setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save setup state",
        )
    })?;

    Ok(Json(SetupTrustedProxyConfigureResponse {
        state: sf.setup_state,
        has_shared_secret: encrypted_shared_secret.is_some(),
        configured_at: now,
        session_expires_at: sf.setup_session.as_ref().map(|s| s.expires_at),
    }))
}

/// `POST /v1/setup/owner/claim-trusted-proxy`
///
/// Creates the owner from trusted proxy identity headers.
async fn setup_owner_claim_trusted_proxy(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> ApiResult<SetupTrustedProxyClaimOwnerResponse> {
    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        error!(error = %e, "failed to load setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load setup state",
        )
    })?;

    if sf.setup_state == SetupState::Ready {
        return Err(api_err(
            StatusCode::CONFLICT,
            "already_configured",
            "Setup is already complete",
        ));
    }

    if sf.setup_state != SetupState::IdpConfigured {
        return Err(api_err(
            StatusCode::CONFLICT,
            "invalid_state",
            format!(
                "Trusted proxy owner claim requires idp_configured state, current: {}",
                sf.setup_state
            ),
        ));
    }

    validate_session(&mut sf, &headers)?;

    let runtime_mode = crate::instance_settings::load_runtime_mode(store.pool())
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load runtime mode");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to determine runtime mode",
            )
        })?;
    let remote_auth_mode = crate::instance_settings::load_remote_auth_mode(store.pool())
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load remote auth mode");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to determine remote auth mode",
            )
        })?;
    if runtime_mode != RuntimeMode::Remote || remote_auth_mode != RemoteAuthMode::TrustedProxy {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "mode_restricted",
            "Trusted proxy owner claim is only available in remote trusted proxy mode",
        ));
    }

    let trusted_proxy_settings =
        crate::instance_settings::load_effective_trusted_proxy_settings(store.pool())
            .await
            .map_err(|e| {
                error!(error = %e, "failed to load trusted proxy settings");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to load trusted proxy settings",
                )
            })?;
    if !trusted_proxy_settings.configured {
        return Err(api_err(
            StatusCode::CONFLICT,
            "trusted_proxy_not_configured",
            "Configure trusted proxy settings before claiming owner identity",
        ));
    }
    if !crate::instance_settings::is_trusted_proxy_peer(peer_addr.ip(), &trusted_proxy_settings) {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "trusted_proxy_peer_not_allowed",
            "Trusted proxy owner claim must come from an allowlisted proxy peer",
        ));
    }
    let email =
        crate::instance_settings::extract_trusted_proxy_email(&headers, &trusted_proxy_settings)?;

    let now = now_unix();
    sf.owner = Some(OwnerRecord {
        email: email.clone(),
        oidc_subject: Some(trusted_proxy_subject_for_email(&email)),
        created_at: now,
    });
    sf.setup_state = SetupState::OwnerCreated;
    sf.updated_at = now;

    store.save(&sf).await.map_err(|e| {
        error!(error = %e, "failed to save setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save setup state",
        )
    })?;

    Ok(Json(SetupTrustedProxyClaimOwnerResponse {
        state: sf.setup_state,
        owner_email: email,
        session_expires_at: sf.setup_session.as_ref().map(|s| s.expires_at),
    }))
}

/// `POST /v1/setup/owner/start-oidc`
///
/// Initiates an OIDC authorization code flow during setup. Requires a valid
/// setup session and `setup_state == IdpConfigured`.
///
/// The frontend provides a `redirect_uri` (its own callback URL) and gets
/// back an `authorization_url` + CSRF `state` to redirect the user to.
async fn setup_oidc_start(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SetupOidcStartRequest>,
) -> ApiResult<SetupOidcStartResponse> {
    // H5: Validate redirect_uri
    let allowed_origins = state.allowed_origins.read().await.clone();
    validate_redirect_uri(&req.redirect_uri, &allowed_origins)?;

    // Validate setup session and state
    {
        let store = state.store.lock().await;
        let mut sf = store.load().await.map_err(|e| {
            error!(error = %e, "failed to load setup state");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load setup state",
            )
        })?;

        if sf.setup_state == SetupState::Ready {
            return Err(api_err(
                StatusCode::CONFLICT,
                "already_configured",
                "Setup is already complete",
            ));
        }

        if sf.setup_state != SetupState::IdpConfigured {
            return Err(api_err(
                StatusCode::CONFLICT,
                "invalid_state",
                format!(
                    "Owner OIDC can only be started in idp_configured state, current: {}",
                    sf.setup_state
                ),
            ));
        }

        validate_session(&mut sf, &headers)?;

        // Persist the bumped session expiry
        store.save(&sf).await.map_err(|e| {
            error!(error = %e, "failed to save setup state");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to save setup state",
            )
        })?;
    }

    // Load the OIDC config (allows IdpConfigured state)
    let oidc_config = load_oidc_config_for_setup(&state).await?;

    if should_skip_oidc_discovery(&state) {
        // Test mode: return a placeholder authorization URL without real discovery
        let csrf_state = CsrfToken::new_random();
        let state_value = csrf_state.secret().clone();
        let nonce = Nonce::new_random();
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let _ = pkce_challenge; // consumed by the URL builder in real flow

        let auth_url = format!(
            "{}/o/oauth2/v2/auth?client_id={}&redirect_uri={}&state={}&nonce={}",
            oidc_config.issuer_url,
            oidc_config.client_id,
            req.redirect_uri,
            state_value,
            nonce.secret()
        );

        {
            let mut pending = state.pending_auth.lock().await;
            let now = now_unix();
            pending.retain(|_, pa| now - pa.created_at < 600);

            // H3: Reject if too many pending auth requests
            if pending.len() >= MAX_PENDING_AUTH {
                return Err(api_err(
                    StatusCode::TOO_MANY_REQUESTS,
                    "too_many_pending",
                    "Too many pending authentication requests",
                ));
            }

            pending.insert(
                state_value.clone(),
                PendingAuth {
                    pkce_verifier,
                    nonce,
                    redirect_uri: req.redirect_uri,
                    created_at: now,
                },
            );
        }

        return Ok(Json(SetupOidcStartResponse {
            authorization_url: auth_url,
            state: state_value,
        }));
    }

    // Real discovery flow
    let issuer = IssuerUrl::new(oidc_config.issuer_url).map_err(|e| {
        error!(error = %e, "invalid issuer URL");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "oidc_config_error",
            "Invalid OIDC issuer URL",
        )
    })?;

    let http_client = build_http_client()?;
    let provider_metadata = CoreProviderMetadata::discover_async(issuer, &http_client)
        .await
        .map_err(|e| {
            error!(error = %e, "OIDC discovery failed");
            api_err(
                StatusCode::BAD_GATEWAY,
                "oidc_discovery_error",
                "Failed to discover OIDC provider",
            )
        })?;

    let oidc_client_id = ClientId::new(oidc_config.client_id);
    let oidc_client_secret = oidc_config.client_secret.map(ClientSecret::new);

    let client = openidconnect::core::CoreClient::from_provider_metadata(
        provider_metadata,
        oidc_client_id,
        oidc_client_secret,
    )
    .set_redirect_uri(RedirectUrl::new(req.redirect_uri.clone()).map_err(|e| {
        error!(error = %e, "invalid redirect URI");
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            "Invalid redirect URI",
        )
    })?);

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_state, nonce) = client
        .authorize_url(
            AuthenticationFlow::<openidconnect::core::CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    let state_value = csrf_state.secret().clone();

    {
        let mut pending = state.pending_auth.lock().await;
        let now = now_unix();
        pending.retain(|_, pa| now - pa.created_at < 600);

        // H3: Reject if too many pending auth requests
        if pending.len() >= MAX_PENDING_AUTH {
            return Err(api_err(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_pending",
                "Too many pending authentication requests",
            ));
        }

        pending.insert(
            state_value.clone(),
            PendingAuth {
                pkce_verifier,
                nonce,
                redirect_uri: req.redirect_uri,
                created_at: now,
            },
        );
    }

    Ok(Json(SetupOidcStartResponse {
        authorization_url: auth_url.to_string(),
        state: state_value,
    }))
}

/// `POST /v1/setup/owner/verify-oidc`
///
/// Completes the setup OIDC flow. Exchanges the authorization code for tokens,
/// extracts email + oidc_subject from the ID token, and creates the owner.
///
/// Requires a valid setup session and `setup_state == IdpConfigured`.
async fn setup_oidc_verify(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SetupOidcVerifyRequest>,
) -> ApiResult<SetupOidcVerifyResponse> {
    info!("verify-oidc: request received");

    // Validate setup session and state
    {
        let store = state.store.lock().await;
        let mut sf = store.load().await.map_err(|e| {
            error!(error = %e, "failed to load setup state");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load setup state",
            )
        })?;

        if sf.setup_state == SetupState::Ready {
            return Err(api_err(
                StatusCode::CONFLICT,
                "already_configured",
                "Setup is already complete",
            ));
        }

        if sf.setup_state != SetupState::IdpConfigured {
            return Err(api_err(
                StatusCode::CONFLICT,
                "invalid_state",
                format!(
                    "Owner OIDC verify requires idp_configured state, current: {}",
                    sf.setup_state
                ),
            ));
        }

        validate_session(&mut sf, &headers)?;

        store.save(&sf).await.map_err(|e| {
            error!(error = %e, "failed to save setup state");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to save setup state",
            )
        })?;
    }

    info!("verify-oidc: session validated");

    // Retrieve pending auth entry (validates CSRF state)
    let pending = {
        let mut pending_map = state.pending_auth.lock().await;
        pending_map.remove(&req.state).ok_or_else(|| {
            api_err(
                StatusCode::BAD_REQUEST,
                "invalid_state",
                "Unknown or expired OIDC state parameter",
            )
        })?
    };

    if now_unix() - pending.created_at >= 600 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "auth_expired",
            "OIDC authorization request has expired",
        ));
    }

    info!("verify-oidc: pending auth found, starting token exchange");

    // In test mode, simulate the token exchange with mock claims
    let (email, subject) = if should_skip_oidc_discovery(&state) {
        // In test mode, derive owner email/subject from the code
        // Convention: code format is "test-code" and we use known test values
        (
            "admin@example.com".to_string(),
            format!("test-subject-{}", &req.code),
        )
    } else {
        // Real OIDC token exchange
        let oidc_config = load_oidc_config_for_setup(&state).await?;

        let issuer = IssuerUrl::new(oidc_config.issuer_url.clone()).map_err(|e| {
            error!(error = %e, "invalid issuer URL");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "oidc_config_error",
                "Invalid OIDC issuer URL",
            )
        })?;

        info!(issuer = %oidc_config.issuer_url, "verify-oidc: starting OIDC discovery");

        let http_client = build_http_client()?;
        let provider_metadata = CoreProviderMetadata::discover_async(issuer, &http_client)
            .await
            .map_err(|e| {
                error!(error = %e, "OIDC discovery failed");
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "oidc_discovery_error",
                    "Failed to discover OIDC provider",
                )
            })?;

        info!("verify-oidc: discovery complete, exchanging code for tokens");

        let oidc_client_id = ClientId::new(oidc_config.client_id);
        let oidc_client_secret = oidc_config.client_secret.map(ClientSecret::new);

        let client = openidconnect::core::CoreClient::from_provider_metadata(
            provider_metadata,
            oidc_client_id,
            oidc_client_secret,
        )
        .set_redirect_uri(RedirectUrl::new(pending.redirect_uri).map_err(|e| {
            error!(error = %e, "invalid redirect URI");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "oidc_config_error",
                "Invalid redirect URI",
            )
        })?);

        let code_request = client
            .exchange_code(AuthorizationCode::new(req.code))
            .map_err(|e| {
                error!(error = %e, "OIDC token endpoint not configured");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "oidc_config_error",
                    "Token endpoint not available",
                )
            })?;

        let token_response = code_request
            .set_pkce_verifier(pending.pkce_verifier)
            .request_async(&http_client)
            .await
            .map_err(|e| {
                error!(error = %e, "OIDC token exchange failed");
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "token_exchange_error",
                    "Failed to exchange authorization code",
                )
            })?;

        info!("verify-oidc: token exchange complete, verifying ID token");

        let id_token = token_response.id_token().ok_or_else(|| {
            error!("no ID token in OIDC token response");
            api_err(
                StatusCode::BAD_GATEWAY,
                "missing_id_token",
                "Identity provider did not return an ID token",
            )
        })?;

        let id_token_verifier = client.id_token_verifier();
        let claims = id_token
            .claims(&id_token_verifier, &pending.nonce)
            .map_err(|e| {
                error!(error = %e, "ID token verification failed");
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "id_token_verification_error",
                    "Failed to verify ID token",
                )
            })?;

        let subject = claims.subject().to_string();
        let email = claims
            .email()
            .map(|addr: &EndUserEmail| addr.to_string())
            .ok_or_else(|| {
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "missing_email",
                    "ID token missing email claim",
                )
            })?;

        info!(email = %email, subject = %subject, "verify-oidc: ID token verified");

        (email, subject)
    };

    // Create owner and transition state
    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        error!(error = %e, "failed to load setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load setup state",
        )
    })?;

    let now = now_unix();
    sf.owner = Some(OwnerRecord {
        email: email.clone(),
        oidc_subject: Some(subject.clone()),
        created_at: now,
    });

    sf.setup_state = SetupState::OwnerCreated;
    sf.updated_at = now;

    store.save(&sf).await.map_err(|e| {
        error!(error = %e, "failed to save setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save setup state",
        )
    })?;

    info!(email = %email, "verify-oidc: owner created, state -> OwnerCreated");

    Ok(Json(SetupOidcVerifyResponse {
        state: SetupState::OwnerCreated,
        owner_email: email,
        oidc_subject: subject,
        session_expires_at: sf.setup_session.as_ref().map(|s| s.expires_at),
    }))
}

/// `POST /v1/setup/local-owner/create`
///
/// Creates the owner record without OIDC in local mode.
/// Requires a valid setup session and local runtime mode.
async fn setup_local_owner_create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SetupLocalOwnerCreateRequest>,
) -> ApiResult<SetupLocalOwnerCreateResponse> {
    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        error!(error = %e, "failed to load setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load setup state",
        )
    })?;

    if sf.setup_state == SetupState::Ready {
        return Err(api_err(
            StatusCode::CONFLICT,
            "already_configured",
            "Setup is already complete",
        ));
    }

    let runtime_mode = crate::instance_settings::load_runtime_mode(store.pool())
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load runtime mode");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to determine runtime mode",
            )
        })?;
    if runtime_mode != RuntimeMode::Local {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "mode_restricted",
            "Local owner setup is only available in local mode",
        ));
    }

    if !matches!(
        sf.setup_state,
        SetupState::BootstrapPending | SetupState::IdpConfigured
    ) {
        return Err(api_err(
            StatusCode::CONFLICT,
            "invalid_state",
            format!(
                "Local owner creation requires bootstrap_pending or idp_configured state, current: {}",
                sf.setup_state
            ),
        ));
    }

    validate_session(&mut sf, &headers)?;

    let email = req.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "A valid owner email is required",
        ));
    }

    let now = now_unix();
    sf.owner = Some(OwnerRecord {
        email: email.clone(),
        oidc_subject: Some(local_subject_for_email(&email)),
        created_at: now,
    });
    sf.setup_state = SetupState::OwnerCreated;
    sf.updated_at = now;

    store.save(&sf).await.map_err(|e| {
        error!(error = %e, "failed to save setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save setup state",
        )
    })?;

    Ok(Json(SetupLocalOwnerCreateResponse {
        state: SetupState::OwnerCreated,
        owner_email: email,
        session_expires_at: sf.setup_session.as_ref().map(|s| s.expires_at),
    }))
}

async fn complete_setup(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<SetupCompleteResponse> {
    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        error!(error = %e, "failed to load setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load setup state",
        )
    })?;

    // State gates first — deterministic 409 regardless of session state
    if sf.setup_state == SetupState::Ready {
        return Err(api_err(
            StatusCode::CONFLICT,
            "already_configured",
            "Setup is already complete",
        ));
    }

    if sf.setup_state != SetupState::OwnerCreated {
        return Err(api_err(
            StatusCode::CONFLICT,
            "invalid_state",
            format!(
                "Setup can only be completed in owner_created state, current: {}",
                sf.setup_state
            ),
        ));
    }

    validate_session(&mut sf, &headers)?;

    let runtime_mode = crate::instance_settings::load_runtime_mode(store.pool())
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load runtime mode for setup completion");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to determine runtime mode",
            )
        })?;
    let remote_auth_mode = crate::instance_settings::load_remote_auth_mode(store.pool())
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load remote auth mode for setup completion");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to determine remote auth mode",
            )
        })?;

    if runtime_mode == RuntimeMode::Remote {
        match remote_auth_mode {
            RemoteAuthMode::Oidc => {
                if sf.oidc_config.is_none() {
                    return Err(api_err(
                        StatusCode::CONFLICT,
                        "remote_auth_not_configured",
                        "Remote OIDC authentication is not configured",
                    ));
                }
            }
            RemoteAuthMode::TrustedProxy => {
                let row = sqlx::query(
                    "SELECT user_email_header FROM trusted_proxy_settings WHERE id = 1",
                )
                .fetch_optional(store.pool())
                .await
                .map_err(|e| {
                    error!(error = %e, "failed to load trusted proxy settings for setup completion");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "store_error",
                        "Failed to load trusted proxy settings",
                    )
                })?;
                let Some(row) = row else {
                    return Err(api_err(
                        StatusCode::CONFLICT,
                        "remote_auth_not_configured",
                        "Trusted proxy authentication is not configured",
                    ));
                };
                let header_value: String = row.try_get("user_email_header").unwrap_or_else(|_| {
                    crate::instance_settings::DEFAULT_TRUSTED_PROXY_EMAIL_HEADER.to_string()
                });
                if crate::instance_settings::normalize_header_name(&header_value).is_none() {
                    return Err(api_err(
                        StatusCode::CONFLICT,
                        "trusted_proxy_config_invalid",
                        "Trusted proxy email header configuration is invalid",
                    ));
                }
            }
        }
    }

    let now = now_unix();
    if let Some(owner) = sf.owner.as_mut()
        && owner.oidc_subject.is_none()
    {
        owner.oidc_subject = Some(local_subject_for_email(&owner.email));
    }
    sf.setup_state = SetupState::Ready;
    sf.setup_session = None; // Clear session on completion
    sf.updated_at = now;

    let instance_id = sf.instance_id.clone();

    store.save(&sf).await.map_err(|e| {
        error!(error = %e, "failed to save setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save setup state",
        )
    })?;

    // Insert owner into users table
    if let Some(ref owner) = sf.owner {
        let oidc_subject = owner
            .oidc_subject
            .clone()
            .unwrap_or_else(|| local_subject_for_email(&owner.email));
        let user_id = uuid::Uuid::new_v4().to_string();
        let pool = store.pool();

        sqlx::query(
            "INSERT OR IGNORE INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'owner', 'active', ?5, ?5)",
        )
        .bind(&user_id)
        .bind(&owner.email)
        .bind(&oidc_subject)
        .bind(&owner.email)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to insert owner user");
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to create owner user")
        })?;

        let _ = write_audit_log(
            pool,
            Some(&user_id),
            "owner_created",
            "user",
            Some(&user_id),
            None,
        )
        .await;

        info!(email = %owner.email, "owner user created in users table");
    }

    Ok(Json(SetupCompleteResponse {
        state: SetupState::Ready,
        instance_id,
    }))
}

/// `GET /v1/setup/summary`
///
/// Returns a summary of the current setup configuration including issuer URL
/// and owner email. Session-gated — requires a valid setup session token.
async fn setup_summary(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<SetupSummaryResponse> {
    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        error!(error = %e, "failed to load setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load setup state",
        )
    })?;

    validate_session(&mut sf, &headers)?;

    // Persist the bumped session expiry
    store.save(&sf).await.map_err(|e| {
        error!(error = %e, "failed to save setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save setup state",
        )
    })?;

    Ok(Json(SetupSummaryResponse {
        instance_id: sf.instance_id,
        state: sf.setup_state,
        issuer_url: sf.oidc_config.as_ref().map(|c| c.issuer_url.clone()),
        owner_email: sf.owner.as_ref().map(|o| o.email.clone()),
    }))
}

// ── Router builder ──────────────────────────────────────────────

/// Build the Axum router with all setup and auth endpoints, given a `SetupStore`,
/// the AES-256 encryption key for secrets at rest, and a Prometheus metrics handle.
pub async fn build_router(
    store: SetupStore,
    encryption_key: Vec<u8>,
    metrics_handle: PrometheusHandle,
) -> Router {
    build_router_inner(store, encryption_key, false, metrics_handle).await
}

/// Build a test router that skips real OIDC discovery in `configure_oidc`.
///
/// This allows integration tests to exercise the full setup flow without
/// making any network calls.
#[cfg(any(test, feature = "test-support"))]
pub async fn build_test_router(store: SetupStore, encryption_key: Vec<u8>) -> Router {
    use std::sync::OnceLock;
    static TEST_METRICS: OnceLock<PrometheusHandle> = OnceLock::new();
    let metrics_handle = TEST_METRICS
        .get_or_init(|| {
            metrics_exporter_prometheus::PrometheusBuilder::new()
                .install_recorder()
                .expect("failed to install test metrics recorder")
        })
        .clone();
    build_router_inner(store, encryption_key, true, metrics_handle).await
}

async fn build_router_inner(
    store: SetupStore,
    encryption_key: Vec<u8>,
    _skip_oidc_discovery: bool,
    metrics_handle: PrometheusHandle,
) -> Router {
    let session_store = SessionStore::new(store.pool().clone());
    let enforcer = rbac::init_enforcer()
        .await
        .expect("failed to initialise RBAC enforcer");
    let sched = scheduler::Scheduler::new(1000);

    // Reload pending builds from DB into scheduler queue
    {
        let pool = store.pool();
        if let Err(e) = sched.reload_pending(pool).await {
            tracing::error!(error = %e, "failed to reload pending builds into scheduler");
        }
    }

    let external_access_network =
        match instance_settings::load_effective_external_access_network_settings(store.pool()).await
        {
            Ok(settings) => settings,
            Err(error) => {
                warn!(error = %error, "failed to load external access network settings; falling back to defaults");
                instance_settings::EffectiveExternalAccessNetworkSettings {
                    public_url: None,
                    allowed_origins: instance_settings::default_allowed_origins(),
                    source: oore_contract::ExternalAccessNetworkSource::Default,
                    updated_at: None,
                }
            }
        };

    let allowed_origins_state =
        Arc::new(RwLock::new(external_access_network.allowed_origins.clone()));
    let public_url_state = Arc::new(RwLock::new(external_access_network.public_url.clone()));

    // Initialize artifact storage backend from DB settings.
    let storage_backend = storage::load_backend(
        store.pool(),
        &encryption_key,
        external_access_network.public_url.clone(),
    )
    .await;

    info!(
        source = ?external_access_network.source,
        public_url = ?external_access_network.public_url,
        origins = ?external_access_network.allowed_origins,
        "configured External Access network settings"
    );
    info!(
        max_bytes = artifacts::MAX_LOCAL_UPLOAD_BYTES,
        "configured local artifact upload size limit"
    );

    let shared_state = Arc::new(AppState {
        store: Mutex::new(store),
        sessions: session_store,
        pending_auth: Mutex::new(HashMap::new()),
        encryption_key: Zeroizing::new(encryption_key),
        enforcer,
        #[cfg(any(test, feature = "test-support"))]
        skip_oidc_discovery: _skip_oidc_discovery,
        bootstrap_failures: Mutex::new(HashMap::new()),
        scheduler: sched.clone(),
        storage: Arc::new(RwLock::new(storage_backend)),
        stream_tokens: logs::StreamTokenStore::new(),
        allowed_origins: allowed_origins_state.clone(),
        public_url: public_url_state,
    });

    // Start background tasks (lease timeout, build timeout, heartbeat monitor)
    {
        let store_guard = shared_state.store.lock().await;
        let pool = store_guard.pool().clone();
        background::start_background_tasks(pool.clone(), sched.clone());
        notification_dispatch::start_notification_dispatcher(
            pool,
            sched,
            shared_state.encryption_key.to_vec(),
        );
    }

    let allowed_origins_for_cors = allowed_origins_state.clone();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _request| {
            let Ok(value) = origin.to_str() else {
                return false;
            };
            match allowed_origins_for_cors.try_read() {
                Ok(guard) => guard.iter().any(|allowed| allowed == value),
                Err(_) => false,
            }
        }))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    // Webhook routes are mounted OUTSIDE the CORS layer since they're called by providers
    let webhook_routes = Router::new()
        .route(
            "/v1/webhooks/github",
            post(integrations::webhooks::github_webhook),
        )
        .route(
            "/v1/webhooks/gitlab",
            post(integrations::webhooks::gitlab_webhook),
        )
        .with_state(shared_state.clone());

    // GitHub App manifest flow routes — browser-navigated, return HTML, no auth middleware
    // (authentication is via encrypted state token in query params)
    let github_flow_routes = Router::new()
        .route(
            "/v1/integrations/github/create",
            get(integrations::github::github_create_page),
        )
        .route(
            "/v1/integrations/github/callback",
            get(integrations::github::github_callback),
        )
        .route(
            "/v1/integrations/github/installed",
            get(integrations::github::github_installed),
        )
        .with_state(shared_state.clone());

    // GitLab OAuth callback route — browser-navigated, unauthenticated
    let gitlab_flow_routes = Router::new()
        .route(
            "/v1/integrations/gitlab/callback",
            get(integrations::gitlab::gitlab_callback),
        )
        .with_state(shared_state.clone());

    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/public/setup-status", get(setup_status))
        .route(
            "/v1/setup/bootstrap-token/verify",
            post(verify_bootstrap_token),
        )
        .route("/v1/setup/preferences", post(setup_preferences))
        .route("/v1/setup/oidc/configure", post(configure_oidc))
        .route(
            "/v1/setup/trusted-proxy/configure",
            post(setup_trusted_proxy_configure),
        )
        .route("/v1/setup/owner/start-oidc", post(setup_oidc_start))
        .route("/v1/setup/owner/verify-oidc", post(setup_oidc_verify))
        .route(
            "/v1/setup/owner/claim-trusted-proxy",
            post(setup_owner_claim_trusted_proxy),
        )
        .route(
            "/v1/setup/local-owner/create",
            post(setup_local_owner_create),
        )
        .route("/v1/setup/complete", post(complete_setup))
        .route("/v1/setup/summary", get(setup_summary))
        // Auth endpoints (only functional when setup_state == Ready)
        .route("/v1/auth/oidc/start", get(auth::oidc_start))
        .route("/v1/auth/oidc/callback", post(auth::oidc_callback))
        .route("/v1/auth/local/login", post(auth::local_login))
        .route(
            "/v1/auth/trusted-proxy/login",
            post(auth::trusted_proxy_login),
        )
        .route("/v1/auth/logout", post(auth::logout))
        // User management endpoints
        .route("/v1/users/me", get(users::get_me))
        .route("/v1/users", get(users::list_users))
        .route("/v1/users/invite", post(users::invite_user))
        .route(
            "/v1/users/{user_id}/role",
            axum::routing::patch(users::update_user_role),
        )
        .route(
            "/v1/users/{user_id}",
            axum::routing::delete(users::delete_user),
        )
        .route("/v1/users/{user_id}/enable", post(users::re_enable_user))
        // Instance settings endpoints
        .route(
            "/v1/settings/artifact-storage",
            get(instance_settings::get_artifact_storage_settings)
                .put(instance_settings::update_artifact_storage_settings),
        )
        .route(
            "/v1/settings/preferences",
            get(instance_settings::get_instance_preferences)
                .put(instance_settings::update_instance_preferences),
        )
        .route(
            "/v1/settings/external-access/network",
            get(instance_settings::get_external_access_network_settings)
                .put(instance_settings::update_external_access_network_settings),
        )
        .route(
            "/v1/settings/external-access/trusted-proxy",
            get(instance_settings::get_external_access_trusted_proxy_settings)
                .put(instance_settings::update_external_access_trusted_proxy_settings),
        )
        .route(
            "/v1/settings/external-access/preflight",
            get(instance_settings::get_external_access_preflight),
        )
        .route(
            "/v1/settings/external-access/oidc",
            axum::routing::put(instance_settings::configure_external_access_oidc),
        )
        // Notification channel settings
        .route(
            "/v1/settings/notification-channels",
            get(notification_channels::list_notification_channels)
                .post(notification_channels::create_notification_channel),
        )
        .route(
            "/v1/settings/notification-channels/{id}",
            get(notification_channels::get_notification_channel)
                .put(notification_channels::update_notification_channel)
                .delete(notification_channels::delete_notification_channel),
        )
        .route(
            "/v1/settings/notification-channels/{id}/test",
            post(notification_channels::test_notification_channel),
        )
        .route(
            "/v1/settings/notification-channels/{id}/deliveries",
            get(notification_channels::list_deliveries),
        )
        // Integration management endpoints
        .route("/v1/integrations", get(integrations::list_integrations))
        .route("/v1/integrations/{id}", get(integrations::get_integration))
        .route(
            "/v1/integrations/{id}",
            axum::routing::delete(integrations::delete_integration),
        )
        .route(
            "/v1/integrations/{id}/repositories",
            get(integrations::list_repositories),
        )
        .route(
            "/v1/integrations/github/start",
            post(integrations::github::github_start),
        )
        .route(
            "/v1/integrations/github/complete",
            post(integrations::github::github_complete),
        )
        .route(
            "/v1/integrations/{id}/installations",
            get(integrations::list_installations).post(integrations::sync_installations),
        )
        .route(
            "/v1/integrations/gitlab/start",
            post(integrations::gitlab::gitlab_start),
        )
        .route(
            "/v1/integrations/gitlab/authorize",
            post(integrations::gitlab::gitlab_authorize),
        )
        .route(
            "/v1/integrations/local-git",
            get(integrations::local_git::list_local_git_integrations)
                .post(integrations::local_git::create_local_git_integration),
        )
        .route(
            "/v1/integrations/local-git/directories",
            get(integrations::local_git::browse_local_git_directories),
        )
        .route(
            "/v1/integrations/local-git/{id}",
            axum::routing::delete(integrations::local_git::delete_local_git_integration),
        )
        // Project endpoints
        .route(
            "/v1/projects",
            get(projects::list_projects).post(projects::create_project),
        )
        .route(
            "/v1/projects/{project_id}",
            get(projects::get_project)
                .patch(projects::update_project)
                .delete(projects::delete_project),
        )
        // Pipeline endpoints
        .route(
            "/v1/projects/{project_id}/pipelines",
            get(pipelines::list_pipelines).post(pipelines::create_pipeline),
        )
        .route(
            "/v1/pipelines/{pipeline_id}",
            get(pipelines::get_pipeline)
                .patch(pipelines::update_pipeline)
                .delete(pipelines::delete_pipeline),
        )
        .route("/v1/pipelines/validate", post(pipelines::validate_pipeline))
        .route(
            "/v1/pipelines/{pipeline_id}/android-signing",
            get(pipeline_signing::get_pipeline_android_signing)
                .put(pipeline_signing::update_pipeline_android_signing),
        )
        .route(
            "/v1/pipelines/{pipeline_id}/ios-signing",
            get(pipeline_ios_signing::get_pipeline_ios_signing)
                .put(pipeline_ios_signing::update_pipeline_ios_signing)
                .layer(DefaultBodyLimit::max(
                    pipeline_ios_signing::MAX_IOS_SIGNING_REQUEST_BYTES,
                )),
        )
        .route(
            "/v1/pipelines/{pipeline_id}/ios-signing/sync",
            post(pipeline_ios_signing::sync_pipeline_ios_signing).layer(DefaultBodyLimit::max(
                pipeline_ios_signing::MAX_IOS_SIGNING_REQUEST_BYTES,
            )),
        )
        .route(
            "/v1/pipelines/{pipeline_id}/ios-signing/devices",
            get(pipeline_ios_signing::list_pipeline_ios_devices),
        )
        .route(
            "/v1/pipelines/{pipeline_id}/ios-signing/devices/register",
            post(pipeline_ios_signing::register_pipeline_ios_device),
        )
        // Build endpoints
        .route(
            "/v1/projects/{project_id}/builds",
            post(builds::create_build),
        )
        .route("/v1/builds", get(builds::list_builds))
        .route("/v1/builds/{build_id}", get(builds::get_build))
        .route("/v1/builds/{build_id}/cancel", post(builds::cancel_build))
        // Runner endpoints
        .route("/v1/runners/register", post(runners::register_runner))
        .route(
            "/v1/runners/{runner_id}",
            axum::routing::patch(runners::update_runner),
        )
        .route(
            "/v1/runners/{runner_id}/heartbeat",
            post(runners::runner_heartbeat),
        )
        .route("/v1/runners/{runner_id}/claim", post(runners::claim_job))
        .route(
            "/v1/runners/{runner_id}/jobs/{job_id}/status",
            post(runners::update_job_status),
        )
        .route(
            "/v1/runners/{runner_id}/jobs/{job_id}",
            get(runners::get_job_status),
        )
        .route(
            "/v1/runners/{runner_id}/jobs/{job_id}/android-signing",
            get(pipeline_signing::get_job_android_signing),
        )
        .route(
            "/v1/runners/{runner_id}/jobs/{job_id}/ios-signing",
            get(pipeline_ios_signing::get_job_ios_signing),
        )
        .route("/v1/runners", get(runners::list_runners))
        // Build log endpoints
        .route(
            "/v1/runners/{runner_id}/jobs/{job_id}/logs",
            post(logs::append_build_logs),
        )
        .route("/v1/builds/{build_id}/logs", get(logs::get_build_logs))
        .route(
            "/v1/builds/{build_id}/logs/stream",
            get(logs::stream_build_logs),
        )
        .route(
            "/v1/builds/{build_id}/stream-token",
            post(logs::create_stream_token),
        )
        // Artifact endpoints
        .route(
            "/v1/runners/{runner_id}/jobs/{job_id}/artifacts",
            post(artifacts::create_artifact),
        )
        .route(
            "/v1/builds/{build_id}/artifacts",
            get(artifacts::list_artifacts),
        )
        .route(
            "/v1/artifacts/{artifact_id}/download-link",
            post(artifacts::generate_download_link),
        )
        .route(
            "/v1/artifacts/local-upload/{token}",
            axum::routing::put(artifacts::upload_local_artifact)
                // Local artifact uploads can be large (APK/IPA), but must remain bounded.
                .layer(DefaultBodyLimit::max(artifacts::MAX_LOCAL_UPLOAD_BYTES)),
        )
        // Backend-agnostic local signed download URL (preferred)
        .route(
            "/v1/artifacts/download/{token}",
            get(artifacts::download_local_artifact),
        )
        // Backward-compatible legacy local download path
        .route(
            "/v1/artifacts/local-download/{token}",
            get(artifacts::download_local_artifact),
        )
        .layer(cors)
        .with_state(shared_state)
        // Merge webhook routes (outside CORS)
        .merge(webhook_routes)
        // Merge GitHub App manifest flow routes (outside CORS — browser-navigated HTML pages)
        .merge(github_flow_routes)
        // Merge GitLab OAuth flow routes (outside CORS — browser-navigated)
        .merge(gitlab_flow_routes)
        // Merge the Prometheus /metrics endpoint (uses its own state)
        .merge(observability::metrics_router(metrics_handle))
        // Request metrics middleware wraps all routes (including /metrics)
        .layer(axum_mw::from_fn(observability::track_http_metrics))
}
