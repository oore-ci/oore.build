use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use oore_contract::{
    ApiError, AuthenticatedUser, LogoutResponse, OidcCallbackResponse, OidcStartResponse,
    SetupState,
};
use openidconnect::core::CoreProviderMetadata;
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndUserEmail,
    IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use serde::Deserialize;
use tracing::{error, warn};

use crate::session::DEFAULT_SESSION_TTL;
use crate::util::{api_err, extract_bearer, now_unix};
use crate::{AppState, MAX_PENDING_AUTH};

/// Maximum lifetime of a pending OIDC auth request before it expires (10 minutes).
const PENDING_AUTH_TTL_SECS: i64 = 600;

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

// ── Helpers ──────────────────────────────────────────────────────

/// Build the HTTP client used for OIDC discovery and token exchange.
pub fn build_http_client() -> Result<reqwest::Client, (StatusCode, Json<ApiError>)> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
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
async fn load_oidc_config(
    state: &AppState,
) -> Result<OidcConfig, (StatusCode, Json<ApiError>)> {
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
            StatusCode::INTERNAL_SERVER_ERROR,
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
    let oidc_client_secret = oidc_config
        .client_secret
        .map(ClientSecret::new);

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

/// `GET /v1/auth/oidc/callback`
///
/// Handles the OIDC callback from the identity provider. Exchanges the
/// authorization code for tokens, validates the ID token, and creates a
/// session.
pub async fn oidc_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OidcCallbackParams>,
) -> Result<Json<OidcCallbackResponse>, (StatusCode, Json<ApiError>)> {
    // Retrieve and remove the pending auth entry (validates CSRF state)
    let pending = {
        let mut pending_map = state.pending_auth.lock().await;
        pending_map.remove(&params.state).ok_or_else(|| {
            warn!(state = %params.state, "unknown or expired OIDC state parameter");
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
    let oidc_client_secret = oidc_config
        .client_secret
        .map(ClientSecret::new);

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

    // Create a session
    let (session_token, expires_at) = {
        let mut sessions = state.sessions.lock().await;
        let token = sessions.create_session(&email, &subject, DEFAULT_SESSION_TTL);
        let session = sessions
            .validate_session(&token)
            .expect("just-created session must be valid");
        let expires = session.expires_at;
        (token, expires)
    };

    Ok(Json(OidcCallbackResponse {
        session_token,
        expires_at,
        user: AuthenticatedUser {
            email,
            oidc_subject: subject,
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

    let mut sessions = state.sessions.lock().await;

    // Validate the session exists before revoking
    if sessions.validate_session(token).is_none() {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_session",
            "Invalid or expired session token",
        ));
    }

    sessions.revoke_session(token);

    Ok(Json(LogoutResponse { ok: true }))
}
