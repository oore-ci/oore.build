use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{HeaderMap, StatusCode};
use oore_contract::{
    ApiError, AuthenticatedUser, LocalLoginRequest, LocalLoginResponse, LogoutResponse,
    OidcCallbackResponse, OidcStartResponse, OwnerRecord, RemoteAuthMode, RuntimeMode, SetupState,
};
use openidconnect::core::CoreProviderMetadata;
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndUserEmail,
    IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use serde::Deserialize;
use sqlx::Row;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::session::DEFAULT_SESSION_TTL;
use crate::store::write_audit_log;
use crate::util::{api_err, extract_bearer, now_unix};
use crate::{AppState, MAX_PENDING_AUTH};

/// Maximum lifetime of a pending OIDC auth request before it expires (10 minutes).
const PENDING_AUTH_TTL_SECS: i64 = 600;
const AUTO_LOCAL_OWNER_EMAIL: &str = "owner@local";

fn local_subject_for_email(email: &str) -> String {
    format!("local::{}", email.trim().to_lowercase())
}

fn trusted_proxy_subject_for_email(email: &str) -> String {
    format!("trusted-proxy::{}", email.trim().to_lowercase())
}

/// Pending OIDC authorization request stored in memory while the user is
/// redirected to the identity provider.
pub struct PendingAuth {
    pub pkce_verifier: PkceCodeVerifier,
    pub nonce: Nonce,
    pub redirect_uri: String,
    pub created_at: i64,
}

/// Query parameters returned by the IdP on the callback redirect.
#[derive(Debug, Deserialize)]
pub struct OidcCallbackParams {
    pub code: String,
    pub state: String,
}

/// Query parameters for the OIDC start endpoint.
#[derive(Debug, Deserialize)]
pub struct OidcStartParams {
    /// Optional override for the redirect URI. If not provided, the daemon
    /// constructs a default based on its own listen address.
    pub redirect_uri: Option<String>,
}

/// OIDC configuration extracted from the state file. Used to pass config
/// out of the Mutex lock scope.
pub struct OidcConfig {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
}

async fn auto_complete_local_setup_if_needed(
    store: &crate::store::SetupStore,
    state_file: &mut oore_contract::SetupStateFile,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    if state_file.setup_state == SetupState::Ready {
        return Ok(());
    }

    let owner_email = state_file
        .owner
        .as_ref()
        .map(|owner| owner.email.trim().to_lowercase())
        .filter(|email| !email.is_empty())
        .unwrap_or_else(|| AUTO_LOCAL_OWNER_EMAIL.to_string());
    let owner_subject = state_file
        .owner
        .as_ref()
        .and_then(|owner| owner.oidc_subject.clone())
        .filter(|subject| !subject.trim().is_empty())
        .unwrap_or_else(|| local_subject_for_email(&owner_email));

    let now = now_unix();
    let owner_created_at = state_file
        .owner
        .as_ref()
        .map(|owner| owner.created_at)
        .unwrap_or(now);

    state_file.owner = Some(OwnerRecord {
        email: owner_email.clone(),
        oidc_subject: Some(owner_subject.clone()),
        created_at: owner_created_at,
    });
    state_file.setup_state = SetupState::Ready;
    state_file.setup_session = None;
    state_file.updated_at = now;

    store.save(state_file).await.map_err(|e| {
        error!(error = %e, "failed to save setup state during local auto-bootstrap");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to finalize local setup",
        )
    })?;

    let user_id_seed = Uuid::new_v4().to_string();
    let pool = store.pool();
    sqlx::query(
        "INSERT INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, 'owner', 'active', ?5, ?5) \
         ON CONFLICT(email) DO UPDATE SET \
            oidc_subject = excluded.oidc_subject, \
            display_name = excluded.display_name, \
            role = 'owner', \
            status = 'active', \
            updated_at = excluded.updated_at",
    )
    .bind(&user_id_seed)
    .bind(&owner_email)
    .bind(&owner_subject)
    .bind(&owner_email)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to upsert local owner user");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to create local owner",
        )
    })?;

    let owner_user_id: String =
        sqlx::query_scalar("SELECT id FROM users WHERE lower(email) = lower(?1) LIMIT 1")
            .bind(&owner_email)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to resolve local owner user id");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to load local owner",
                )
            })?;

    let _ = write_audit_log(
        pool,
        Some(&owner_user_id),
        "owner_created_auto_local",
        "user",
        Some(&owner_user_id),
        Some("auto-bootstrap on first local login"),
    )
    .await;

    info!(
        email = %owner_email,
        "local setup auto-completed on first local login"
    );
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────

/// Build the HTTP client used for OIDC discovery and token exchange.
pub fn build_http_client() -> Result<reqwest::Client, (StatusCode, Json<ApiError>)> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| {
            error!(error = %e, "failed to build HTTP client");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "http_client_error",
                "Failed to create HTTP client",
            )
        })
}

/// Load the OIDC configuration from the state file.
///
/// When `allow_setup` is false (default for regular auth endpoints), returns
/// an error unless `setup_state == Ready`. When `allow_setup` is true (used
/// by setup OIDC verification), also allows `IdpConfigured` state.
async fn load_oidc_config(state: &AppState) -> Result<OidcConfig, (StatusCode, Json<ApiError>)> {
    load_oidc_config_inner(state, false).await
}

/// Load OIDC config, optionally allowing setup states (`IdpConfigured`).
pub async fn load_oidc_config_for_setup(
    state: &AppState,
) -> Result<OidcConfig, (StatusCode, Json<ApiError>)> {
    load_oidc_config_inner(state, true).await
}

async fn load_oidc_config_inner(
    state: &AppState,
    allow_setup: bool,
) -> Result<OidcConfig, (StatusCode, Json<ApiError>)> {
    let store = state.store.lock().await;
    let sf = store.load().await.map_err(|e| {
        // M4: Log the full error server-side, return generic message to client
        error!(error = %e, "failed to load setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load setup state",
        )
    })?;

    let state_ok = if allow_setup {
        sf.setup_state == SetupState::Ready || sf.setup_state == SetupState::IdpConfigured
    } else {
        sf.setup_state == SetupState::Ready
    };

    if !state_ok {
        return Err(api_err(
            StatusCode::CONFLICT,
            "setup_incomplete",
            "Auth endpoints are only available after setup is complete",
        ));
    }

    let oidc = sf.oidc_config.as_ref().ok_or_else(|| {
        api_err(
            // Missing config is a user/actionable setup issue, not a server fault.
            StatusCode::CONFLICT,
            "oidc_not_configured",
            "OIDC configuration is missing",
        )
    })?;

    // C2: Decrypt client secret; the decrypted value is used briefly and dropped
    let secret = if let Some(s) = sf.oidc_secret.as_ref() {
        let decrypted = crate::crypto::decrypt(&s.encrypted_client_secret, &state.encryption_key)
            .map_err(|e| {
            error!(error = %e, "failed to decrypt OIDC client secret");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "decryption_error",
                "Failed to decrypt OIDC client secret",
            )
        })?;
        Some(decrypted)
    } else {
        None
    };

    Ok(OidcConfig {
        issuer_url: oidc.issuer_url.clone(),
        client_id: oidc.client_id.clone(),
        client_secret: secret,
    })
}

// ── Handlers ─────────────────────────────────────────────────────

/// `GET /v1/auth/oidc/start`
///
/// Initiates the OIDC authorization code flow with PKCE. Only works when
/// setup is complete (`setup_state == Ready`).
///
/// Returns the authorization URL for the frontend to redirect the user to.
pub async fn oidc_start(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OidcStartParams>,
) -> Result<Json<OidcStartResponse>, (StatusCode, Json<ApiError>)> {
    let oidc_config = load_oidc_config(&state).await?;

    // Determine redirect URI
    let redirect_uri = params
        .redirect_uri
        .unwrap_or_else(|| "http://127.0.0.1:8787/v1/auth/oidc/callback".to_string());

    // Validate redirect_uri to prevent open redirects.
    let allowed_origins = state.allowed_origins.read().await.clone();
    crate::validate_redirect_uri(&redirect_uri, &allowed_origins)?;

    // Parse issuer URL
    let issuer = IssuerUrl::new(oidc_config.issuer_url).map_err(|e| {
        error!(error = %e, "invalid issuer URL");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "oidc_config_error",
            "Invalid OIDC issuer URL in configuration",
        )
    })?;

    // Perform OIDC discovery
    let http_client = build_http_client()?;
    let provider_metadata = CoreProviderMetadata::discover_async(issuer, &http_client)
        .await
        .map_err(|e| {
            error!(error = %e, "OIDC discovery failed");
            api_err(
                StatusCode::BAD_GATEWAY,
                "oidc_discovery_error",
                "Failed to discover OIDC provider metadata",
            )
        })?;

    // Build OIDC client from discovered metadata
    let oidc_client_id = ClientId::new(oidc_config.client_id);
    let oidc_client_secret = oidc_config.client_secret.map(ClientSecret::new);

    let client = openidconnect::core::CoreClient::from_provider_metadata(
        provider_metadata,
        oidc_client_id,
        oidc_client_secret,
    )
    .set_redirect_uri(RedirectUrl::new(redirect_uri.clone()).map_err(|e| {
        error!(error = %e, "invalid redirect URI");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "oidc_config_error",
            "Invalid redirect URI",
        )
    })?);

    // Generate PKCE challenge
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    // Build authorization URL
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

    // Store pending auth keyed by CSRF state value
    {
        let mut pending = state.pending_auth.lock().await;

        // Cleanup expired entries while we have the lock
        let now = now_unix();
        pending.retain(|_, pa| now - pa.created_at < PENDING_AUTH_TTL_SECS);

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
                redirect_uri,
                created_at: now,
            },
        );
    }

    Ok(Json(OidcStartResponse {
        authorization_url: auth_url.to_string(),
        state: state_value,
    }))
}

/// `POST /v1/auth/oidc/callback`
///
/// Handles the OIDC callback from the identity provider. Exchanges the
/// authorization code for tokens, validates the ID token, and creates a
/// session. Uses POST to keep the authorization code out of URL query
/// params, server logs, and browser history.
pub async fn oidc_callback(
    State(state): State<Arc<AppState>>,
    Json(params): Json<OidcCallbackParams>,
) -> Result<Json<OidcCallbackResponse>, (StatusCode, Json<ApiError>)> {
    // Retrieve and remove the pending auth entry (validates CSRF state)
    let pending = {
        let mut pending_map = state.pending_auth.lock().await;
        pending_map.remove(&params.state).ok_or_else(|| {
            warn!(
                state_len = params.state.len(),
                "unknown or expired OIDC state parameter"
            );
            api_err(
                StatusCode::BAD_REQUEST,
                "invalid_state",
                "Unknown or expired OIDC state parameter (possible CSRF attempt)",
            )
        })?
    };

    // Check if the pending auth has expired
    if now_unix() - pending.created_at >= PENDING_AUTH_TTL_SECS {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "auth_expired",
            "OIDC authorization request has expired",
        ));
    }

    // Load OIDC config from state file
    let oidc_config = load_oidc_config(&state).await?;

    // Parse issuer URL
    let issuer = IssuerUrl::new(oidc_config.issuer_url).map_err(|e| {
        error!(error = %e, "invalid issuer URL");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "oidc_config_error",
            "Invalid OIDC issuer URL in configuration",
        )
    })?;

    // Perform OIDC discovery and build client
    let http_client = build_http_client()?;
    let provider_metadata = CoreProviderMetadata::discover_async(issuer, &http_client)
        .await
        .map_err(|e| {
            error!(error = %e, "OIDC discovery failed");
            api_err(
                StatusCode::BAD_GATEWAY,
                "oidc_discovery_error",
                "Failed to discover OIDC provider metadata",
            )
        })?;

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

    // Exchange the authorization code for tokens
    let code_request = client
        .exchange_code(AuthorizationCode::new(params.code))
        .map_err(|e| {
            error!(error = %e, "OIDC token endpoint not configured");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "oidc_config_error",
                "Token endpoint not available in OIDC configuration",
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
                "Failed to exchange authorization code for tokens",
            )
        })?;

    // Extract and validate the ID token
    let id_token = token_response.id_token().ok_or_else(|| {
        error!("no ID token in OIDC token response");
        api_err(
            StatusCode::BAD_GATEWAY,
            "missing_id_token",
            "Identity provider did not return an ID token",
        )
    })?;

    // Verify the ID token signature and claims
    let id_token_verifier = client.id_token_verifier();
    let nonce = pending.nonce;
    let claims = id_token.claims(&id_token_verifier, &nonce).map_err(|e| {
        error!(error = %e, "ID token verification failed");
        api_err(
            StatusCode::BAD_GATEWAY,
            "id_token_verification_error",
            "Failed to verify identity provider's ID token",
        )
    })?;

    // Extract user info from claims
    let subject = claims.subject().to_string();
    let email = claims
        .email()
        .map(|addr: &EndUserEmail| addr.to_string())
        .ok_or_else(|| {
            warn!(subject = %subject, "ID token missing email claim");
            api_err(
                StatusCode::BAD_GATEWAY,
                "missing_email",
                "Identity provider did not include an email claim in the ID token",
            )
        })?;

    // Extract picture URL from ID token (profile scope is already requested).
    // CoreIdTokenClaims uses EmptyAdditionalClaims, so we extract the picture
    // from the raw JWT payload instead.
    let picture_url = {
        let token_str = id_token.to_string();
        let parts: Vec<&str> = token_str.split('.').collect();
        if parts.len() >= 2 {
            use base64::Engine;
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(parts[1])
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|val| {
                    val.get("picture")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
        } else {
            None
        }
    };

    // Look up user by oidc_subject
    let store = state.store.lock().await;
    let pool = store.pool();

    let user_row = sqlx::query(
        "SELECT id, email, role, avatar_url FROM users WHERE oidc_subject = ?1 AND status = 'active'",
    )
    .bind(&subject)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to look up user by oidc_subject");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to look up user")
    })?;

    // If not found by subject, check for invited user by email and activate
    let (user_id, user_email, user_role, user_avatar_url) = if let Some(row) = user_row {
        let uid: String = row.get("id");
        let uemail: String = row.get("email");
        let urole: String = row.get("role");
        let uavatar: Option<String> = row.get("avatar_url");

        // Update avatar_url if the OIDC provider gave a new one
        if picture_url.is_some() && picture_url != uavatar {
            let now = now_unix();
            let _ = sqlx::query("UPDATE users SET avatar_url = ?1, updated_at = ?2 WHERE id = ?3")
                .bind(&picture_url)
                .bind(now)
                .bind(&uid)
                .execute(pool)
                .await;
        }

        let final_avatar = picture_url.or(uavatar);
        (uid, uemail, urole, final_avatar)
    } else {
        // Check for invited user by email
        let invited = sqlx::query(
            "SELECT id, email, role FROM users WHERE email = ?1 AND status = 'invited'",
        )
        .bind(&email)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to look up invited user");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to look up user",
            )
        })?;

        if let Some(inv_row) = invited {
            let uid: String = inv_row.get("id");
            let uemail: String = inv_row.get("email");
            let urole: String = inv_row.get("role");
            // Activate the invited user and set their oidc_subject + avatar_url
            let now = now_unix();
            sqlx::query(
                "UPDATE users SET oidc_subject = ?1, status = 'active', avatar_url = ?2, updated_at = ?3 WHERE id = ?4",
            )
            .bind(&subject)
            .bind(&picture_url)
            .bind(now)
            .bind(&uid)
            .execute(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to activate invited user");
                api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to activate user")
            })?;

            let _ =
                write_audit_log(pool, Some(&uid), "user_activated", "user", Some(&uid), None).await;

            (uid, uemail, urole, picture_url)
        } else {
            // No matching user — reject login
            warn!(email = %email, subject = %subject, "OIDC login rejected: no matching user");
            return Err(api_err(
                StatusCode::FORBIDDEN,
                "user_not_found",
                "No user account exists for this identity. Contact an administrator.",
            ));
        }
    };
    drop(store);

    // Create session linked to user_id
    let session_token = state
        .sessions
        .create_session(&user_id, DEFAULT_SESSION_TTL)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to create session");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_error",
                "Failed to create session",
            )
        })?;

    let session_info = state
        .sessions
        .validate_session(&session_token)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to validate just-created session");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_error",
                "Failed to validate session",
            )
        })?
        .ok_or_else(|| {
            error!("just-created session could not be validated — possible race condition");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_error",
                "Session created but could not be validated",
            )
        })?;

    Ok(Json(OidcCallbackResponse {
        session_token,
        expires_at: session_info.expires_at,
        user: AuthenticatedUser {
            email: user_email,
            oidc_subject: subject,
            user_id: Some(user_id),
            role: Some(user_role),
            avatar_url: user_avatar_url,
        },
    }))
}

/// `POST /v1/auth/logout`
///
/// Revokes the caller's session. Requires a valid session token in the
/// Authorization header.
pub async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<LogoutResponse>, (StatusCode, Json<ApiError>)> {
    let token = extract_bearer(&headers).ok_or_else(|| {
        api_err(
            StatusCode::UNAUTHORIZED,
            "missing_auth",
            "Authorization header required",
        )
    })?;

    // Validate the session exists before revoking
    let session = state.sessions.validate_session(token).await.map_err(|e| {
        error!(error = %e, "session validation failed");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "session_error",
            "Session validation failed",
        )
    })?;

    if session.is_none() {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_session",
            "Invalid or expired session token",
        ));
    }

    state.sessions.revoke_session(token).await.map_err(|e| {
        error!(error = %e, "session revocation failed");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "session_error",
            "Session revocation failed",
        )
    })?;

    Ok(Json(LogoutResponse { ok: true }))
}

/// `POST /v1/auth/local/login`
///
/// Creates a loopback-only local session without OIDC.
/// - If setup is not complete, local setup is auto-finalized on first login.
/// - If `email` is omitted, auto-selects the single active user.
pub async fn local_login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LocalLoginRequest>,
) -> Result<Json<LocalLoginResponse>, (StatusCode, Json<ApiError>)> {
    let effective_ip = crate::effective_client_ip(peer_addr, &headers);
    if !effective_ip.is_loopback() {
        let peer_ip = peer_addr.ip().to_string();
        let source_ip = effective_ip.to_string();
        let details = serde_json::json!({
            "peer_ip": peer_ip,
            "source_ip": source_ip,
            "cf_connecting_ip": headers.get("cf-connecting-ip").and_then(|v| v.to_str().ok()),
            "x_forwarded_for": headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()),
            "forwarded": headers.get("forwarded").and_then(|v| v.to_str().ok()),
            "x_real_ip": headers.get("x-real-ip").and_then(|v| v.to_str().ok()),
        })
        .to_string();

        let pool = {
            let store = state.store.lock().await;
            store.pool().clone()
        };
        let _ = write_audit_log(
            &pool,
            None,
            "local_login_blocked_non_loopback",
            "auth",
            None,
            Some(&details),
        )
        .await;

        warn!(
            peer_ip = %peer_addr.ip(),
            source_ip = %source_ip,
            "blocked local login attempt from non-loopback client"
        );
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "local_login_loopback_required",
            "Local login is only available from loopback clients",
        ));
    }

    let requested_email = req
        .email
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());

    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        error!(error = %e, "failed to load setup state");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load setup state",
        )
    })?;
    let mode = crate::instance_settings::load_runtime_mode(store.pool())
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load runtime mode");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to determine runtime mode",
            )
        })?;
    if sf.setup_state != SetupState::Ready {
        if mode != RuntimeMode::Local {
            return Err(api_err(
                StatusCode::FORBIDDEN,
                "mode_restricted",
                "Local login during setup is only available in Local Only mode",
            ));
        }
        auto_complete_local_setup_if_needed(&store, &mut sf).await?;
    }

    let pool = store.pool().clone();
    drop(store);

    let row = if let Some(email) = requested_email {
        sqlx::query(
            "SELECT id, email, role, oidc_subject, avatar_url \
             FROM users WHERE lower(email) = ?1 AND status = 'active' LIMIT 1",
        )
        .bind(&email)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to look up local user");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to look up user",
            )
        })?
    } else {
        let active_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE status = 'active'")
                .fetch_one(&pool)
                .await
                .map_err(|e| {
                    error!(error = %e, "failed to count active users");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "store_error",
                        "Failed to look up users",
                    )
                })?;

        if active_count != 1 {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "email_required",
                "Specify email when multiple active users exist",
            ));
        }

        sqlx::query(
            "SELECT id, email, role, oidc_subject, avatar_url \
             FROM users WHERE status = 'active' LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to select local user");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to look up user",
            )
        })?
    };
    let row = row.ok_or_else(|| {
        api_err(
            StatusCode::FORBIDDEN,
            "user_not_found",
            "No active local user found for login",
        )
    })?;

    let user_id: String = row.get("id");
    let user_email: String = row.get("email");
    let user_role: String = row.get("role");
    let oidc_subject: String = row.get("oidc_subject");
    let avatar_url: Option<String> = row.get("avatar_url");

    let session_token = state
        .sessions
        .create_session(&user_id, DEFAULT_SESSION_TTL)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to create session");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_error",
                "Failed to create session",
            )
        })?;

    let session_info = state
        .sessions
        .validate_session(&session_token)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to validate session");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_error",
                "Failed to validate session",
            )
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_error",
                "Session created but could not be validated",
            )
        })?;

    Ok(Json(LocalLoginResponse {
        session_token,
        expires_at: session_info.expires_at,
        user: AuthenticatedUser {
            email: user_email,
            oidc_subject,
            user_id: Some(user_id),
            role: Some(user_role),
            avatar_url,
        },
    }))
}

/// `POST /v1/auth/trusted-proxy/login`
///
/// Creates a session for the identity asserted by a trusted upstream proxy
/// This endpoint is only available in Remote mode when remote auth mode is
/// configured to trusted_proxy.
pub async fn trusted_proxy_login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<LocalLoginResponse>, (StatusCode, Json<ApiError>)> {
    let pool = {
        let store = state.store.lock().await;
        let sf = store.load().await.map_err(|e| {
            error!(error = %e, "failed to load setup state for trusted proxy login");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load setup state",
            )
        })?;
        if sf.setup_state != SetupState::Ready {
            return Err(api_err(
                StatusCode::CONFLICT,
                "setup_incomplete",
                "Auth endpoints are only available after setup is complete",
            ));
        }

        let runtime_mode = crate::instance_settings::load_runtime_mode(store.pool())
            .await
            .map_err(|e| {
                error!(error = %e, "failed to load runtime mode for trusted proxy login");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to determine runtime mode",
                )
            })?;
        if runtime_mode != RuntimeMode::Remote {
            return Err(api_err(
                StatusCode::FORBIDDEN,
                "mode_restricted",
                "Trusted proxy login is only available in External Access mode",
            ));
        }

        let remote_auth_mode = crate::instance_settings::load_remote_auth_mode(store.pool())
            .await
            .map_err(|e| {
                error!(error = %e, "failed to load remote auth mode for trusted proxy login");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to determine remote auth mode",
                )
            })?;
        if remote_auth_mode != RemoteAuthMode::TrustedProxy {
            return Err(api_err(
                StatusCode::FORBIDDEN,
                "mode_restricted",
                "Trusted proxy login is not enabled for this instance",
            ));
        }

        store.pool().clone()
    };

    let proxy_settings = crate::instance_settings::load_effective_trusted_proxy_settings(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load trusted proxy settings");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load trusted proxy settings",
            )
        })?;

    if !crate::instance_settings::is_trusted_proxy_peer(peer_addr.ip(), &proxy_settings) {
        let details = serde_json::json!({
            "peer_ip": peer_addr.ip().to_string(),
        })
        .to_string();
        let _ = write_audit_log(
            &pool,
            None,
            "trusted_proxy_login_blocked_untrusted_peer",
            "auth",
            None,
            Some(&details),
        )
        .await;

        return Err(api_err(
            StatusCode::FORBIDDEN,
            "trusted_proxy_peer_not_allowed",
            "Trusted proxy login requests must come from an allowlisted proxy peer",
        ));
    }

    crate::instance_settings::verify_trusted_proxy_shared_secret(
        &headers,
        &proxy_settings,
        &state.encryption_key,
    )?;

    let email = crate::instance_settings::extract_trusted_proxy_email(&headers, &proxy_settings)?;
    let subject = trusted_proxy_subject_for_email(&email);

    let row = sqlx::query(
        "SELECT id, email, role, status, oidc_subject, avatar_url \
         FROM users WHERE lower(email) = lower(?1) LIMIT 1",
    )
    .bind(&email)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to look up user for trusted proxy login");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to look up user",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::FORBIDDEN,
            "user_not_found",
            "No user account exists for this identity. Contact an administrator.",
        )
    })?;

    let user_id: String = row.get("id");
    let user_email: String = row.get("email");
    let user_role: String = row.get("role");
    let user_status: String = row.get("status");
    let current_subject: String = row.get("oidc_subject");
    let avatar_url: Option<String> = row.get("avatar_url");

    if user_status == "disabled" {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "user_disabled",
            "This user account is disabled",
        ));
    }

    let oidc_subject = if user_status == "invited" {
        let now = now_unix();
        sqlx::query(
            "UPDATE users SET oidc_subject = ?1, status = 'active', updated_at = ?2 WHERE id = ?3",
        )
        .bind(&subject)
        .bind(now)
        .bind(&user_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to activate invited trusted proxy user");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to activate user",
            )
        })?;

        let _ = write_audit_log(
            &pool,
            Some(&user_id),
            "user_activated",
            "user",
            Some(&user_id),
            None,
        )
        .await;
        subject
    } else {
        current_subject
    };

    let session_token = state
        .sessions
        .create_session(&user_id, DEFAULT_SESSION_TTL)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to create trusted proxy session");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_error",
                "Failed to create session",
            )
        })?;

    let session_info = state
        .sessions
        .validate_session(&session_token)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to validate trusted proxy session");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_error",
                "Failed to validate session",
            )
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_error",
                "Session created but could not be validated",
            )
        })?;

    let details = serde_json::json!({
        "email": user_email,
        "peer_ip": peer_addr.ip().to_string(),
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&user_id),
        "trusted_proxy_login_succeeded",
        "auth",
        Some(&user_id),
        Some(&details),
    )
    .await;

    Ok(Json(LocalLoginResponse {
        session_token,
        expires_at: session_info.expires_at,
        user: AuthenticatedUser {
            email: user_email,
            oidc_subject,
            user_id: Some(user_id),
            role: Some(user_role),
            avatar_url,
        },
    }))
}
