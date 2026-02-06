pub mod auth;
pub mod crypto;
pub mod oidc;
pub mod session;
pub mod store;
pub mod token;
pub mod util;

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use oore_contract::{
    ApiError, BootstrapTokenVerifyRequest, BootstrapTokenVerifyResponse, OidcConfigRecord,
    OidcConfigureRequest, OidcConfigureResponse, OidcSecretRecord, OwnerFinalizeRequest,
    OwnerFinalizeResponse, OwnerRecord, SetupCompleteResponse, SetupOidcStartRequest,
    SetupOidcStartResponse, SetupOidcVerifyRequest, SetupOidcVerifyResponse,
    SetupSessionRecord, SetupState, SetupStateFile, SetupStatus,
};
use serde_json::json;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

use openidconnect::core::CoreProviderMetadata;
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndUserEmail,
    IssuerUrl, Nonce, PkceCodeChallenge, RedirectUrl, Scope, TokenResponse,
};
use tracing::{error, info};

use crate::auth::{PendingAuth, build_http_client, load_oidc_config_for_setup};
use crate::session::SessionStore;
use crate::store::SetupStore;
use crate::token::{generate_session_token, hash_token};
use crate::util::{api_err, extract_bearer, now_unix};

// ── Shared application state ─────────────────────────────────────

pub struct AppState {
    pub store: Mutex<SetupStore>,
    pub sessions: Mutex<SessionStore>,
    pub pending_auth: Mutex<HashMap<String, PendingAuth>>,
    /// AES-256 encryption key used to encrypt secrets at rest.
    pub encryption_key: Vec<u8>,
    /// When true, `configure_oidc` skips the real OIDC discovery HTTP call
    /// and populates the config from the raw request values with placeholder
    /// endpoint URLs. Used only in tests.
    pub skip_oidc_discovery: bool,
}

// ── Convenience type alias ───────────────────────────────────────

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

// ── Constants ────────────────────────────────────────────────────

const SETUP_SESSION_TTL_SECS: i64 = 30 * 60;

// ── Helpers ──────────────────────────────────────────────────────

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
        api_err(StatusCode::UNAUTHORIZED, "missing_auth", "Authorization header required")
    })?;

    let session = state_file.setup_session.as_ref().ok_or_else(|| {
        api_err(StatusCode::UNAUTHORIZED, "no_session", "No active setup session")
    })?;

    let hashed = hash_token(token);
    if hashed != session.hash {
        return Err(api_err(StatusCode::UNAUTHORIZED, "invalid_session", "Invalid session token"));
    }

    if now_unix() > session.expires_at {
        return Err(api_err(StatusCode::UNAUTHORIZED, "session_expired", "Setup session has expired"));
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
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
    })?;

    Ok(Json(SetupStatus::from_state(sf.instance_id, sf.setup_state)))
}

async fn verify_bootstrap_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BootstrapTokenVerifyRequest>,
) -> ApiResult<BootstrapTokenVerifyResponse> {
    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
    })?;

    // Setup must not be complete — all setup endpoints are disabled after ready
    if sf.setup_state == SetupState::Ready {
        return Err(api_err(StatusCode::CONFLICT, "already_configured", "Setup is already complete"));
    }

    // Bootstrap token record must exist
    let bt = sf.bootstrap_token.as_ref().ok_or_else(|| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "no_bootstrap_token", "No bootstrap token has been generated")
    })?;

    // Must not already be consumed
    if bt.consumed_at.is_some() {
        return Err(api_err(StatusCode::GONE, "token_consumed", "Bootstrap token has already been consumed"));
    }

    // Must not be expired
    if now_unix() > bt.expires_at {
        return Err(api_err(StatusCode::GONE, "token_expired", "Bootstrap token has expired"));
    }

    // Hash must match
    let request_hash = hash_token(&req.token);
    if request_hash != bt.hash {
        return Err(api_err(StatusCode::UNAUTHORIZED, "invalid_token", "Bootstrap token is invalid"));
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
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
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
    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
    })?;

    // State gates first — deterministic 409 regardless of session state
    if sf.setup_state == SetupState::Ready {
        return Err(api_err(StatusCode::CONFLICT, "already_configured", "Setup is already complete"));
    }

    if sf.setup_state != SetupState::BootstrapPending {
        return Err(api_err(
            StatusCode::CONFLICT,
            "invalid_state",
            format!("OIDC can only be configured in bootstrap_pending state, current: {:?}", sf.setup_state),
        ));
    }

    validate_session(&mut sf, &headers)?;

    let now = now_unix();
    let has_client_secret = req.client_secret.is_some();

    // When skip_oidc_discovery is set (test mode), populate the config from
    // the raw request values with placeholder endpoint URLs instead of
    // performing a real HTTP discovery call.
    let (issuer, authorization_endpoint, token_endpoint, userinfo_endpoint, jwks_uri) =
        if state.skip_oidc_discovery {
            (
                req.issuer_url.clone(),
                format!("{}/o/oauth2/v2/auth", req.issuer_url),
                format!("{}/token", req.issuer_url),
                Some(format!("{}/userinfo", req.issuer_url)),
                format!("{}/jwks", req.issuer_url),
            )
        } else {
            let discovered = oidc::discover_provider(&req.issuer_url).await.map_err(|e| {
                api_err(StatusCode::BAD_REQUEST, "oidc_discovery_failed", e.to_string())
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
        let encrypted = crypto::encrypt(&secret, &state.encryption_key).map_err(|e| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "encryption_error", e.to_string())
        })?;
        sf.oidc_secret = Some(OidcSecretRecord {
            encrypted_client_secret: encrypted,
            stored_at: now,
        });
    }

    sf.setup_state = SetupState::IdpConfigured;
    sf.updated_at = now;

    store.save(&sf).await.map_err(|e| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
    })?;

    Ok(Json(OidcConfigureResponse {
        state: SetupState::IdpConfigured,
        discovered_issuer: issuer,
        session_expires_at: sf.setup_session.as_ref().map(|s| s.expires_at),
    }))
}

async fn finalize_owner(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<OwnerFinalizeRequest>,
) -> ApiResult<OwnerFinalizeResponse> {
    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
    })?;

    // State gates first — deterministic 409 regardless of session state
    if sf.setup_state == SetupState::Ready {
        return Err(api_err(StatusCode::CONFLICT, "already_configured", "Setup is already complete"));
    }

    if sf.setup_state != SetupState::IdpConfigured {
        return Err(api_err(
            StatusCode::CONFLICT,
            "invalid_state",
            format!("Owner can only be finalized in idp_configured state, current: {:?}", sf.setup_state),
        ));
    }

    validate_session(&mut sf, &headers)?;

    let now = now_unix();
    sf.owner = Some(OwnerRecord {
        email: req.owner_email,
        oidc_subject: None,
        created_at: now,
    });

    sf.setup_state = SetupState::OwnerCreated;
    sf.updated_at = now;

    store.save(&sf).await.map_err(|e| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
    })?;

    Ok(Json(OwnerFinalizeResponse {
        state: SetupState::OwnerCreated,
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
    // Validate setup session and state
    {
        let store = state.store.lock().await;
        let mut sf = store.load().await.map_err(|e| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
        })?;

        if sf.setup_state == SetupState::Ready {
            return Err(api_err(StatusCode::CONFLICT, "already_configured", "Setup is already complete"));
        }

        if sf.setup_state != SetupState::IdpConfigured {
            return Err(api_err(
                StatusCode::CONFLICT,
                "invalid_state",
                format!("Owner OIDC can only be started in idp_configured state, current: {:?}", sf.setup_state),
            ));
        }

        validate_session(&mut sf, &headers)?;

        // Persist the bumped session expiry
        store.save(&sf).await.map_err(|e| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
        })?;
    }

    // Load the OIDC config (allows IdpConfigured state)
    let oidc_config = load_oidc_config_for_setup(&state).await?;

    if state.skip_oidc_discovery {
        // Test mode: return a placeholder authorization URL without real discovery
        let csrf_state = CsrfToken::new_random();
        let state_value = csrf_state.secret().clone();
        let nonce = Nonce::new_random();
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let _ = pkce_challenge; // consumed by the URL builder in real flow

        let auth_url = format!(
            "{}/o/oauth2/v2/auth?client_id={}&redirect_uri={}&state={}&nonce={}",
            oidc_config.issuer_url, oidc_config.client_id, req.redirect_uri, state_value, nonce.secret()
        );

        {
            let mut pending = state.pending_auth.lock().await;
            let now = now_unix();
            pending.retain(|_, pa| now - pa.created_at < 600);
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
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "oidc_config_error", "Invalid OIDC issuer URL")
    })?;

    let http_client = build_http_client()?;
    let provider_metadata = CoreProviderMetadata::discover_async(issuer, &http_client)
        .await
        .map_err(|e| {
            error!(error = %e, "OIDC discovery failed");
            api_err(StatusCode::BAD_GATEWAY, "oidc_discovery_error", "Failed to discover OIDC provider")
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
        api_err(StatusCode::BAD_REQUEST, "invalid_redirect_uri", "Invalid redirect URI")
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
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
        })?;

        if sf.setup_state == SetupState::Ready {
            return Err(api_err(StatusCode::CONFLICT, "already_configured", "Setup is already complete"));
        }

        if sf.setup_state != SetupState::IdpConfigured {
            return Err(api_err(
                StatusCode::CONFLICT,
                "invalid_state",
                format!("Owner OIDC verify requires idp_configured state, current: {:?}", sf.setup_state),
            ));
        }

        validate_session(&mut sf, &headers)?;

        store.save(&sf).await.map_err(|e| {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
        })?;
    }

    info!("verify-oidc: session validated");

    // Retrieve pending auth entry (validates CSRF state)
    let pending = {
        let mut pending_map = state.pending_auth.lock().await;
        pending_map.remove(&req.state).ok_or_else(|| {
            api_err(StatusCode::BAD_REQUEST, "invalid_state", "Unknown or expired OIDC state parameter")
        })?
    };

    if now_unix() - pending.created_at >= 600 {
        return Err(api_err(StatusCode::BAD_REQUEST, "auth_expired", "OIDC authorization request has expired"));
    }

    info!("verify-oidc: pending auth found, starting token exchange");

    // In test mode, simulate the token exchange with mock claims
    let (email, subject) = if state.skip_oidc_discovery {
        // In test mode, derive owner email/subject from the code
        // Convention: code format is "test-code" and we use known test values
        ("admin@example.com".to_string(), format!("test-subject-{}", &req.code))
    } else {
        // Real OIDC token exchange
        let oidc_config = load_oidc_config_for_setup(&state).await?;

        let issuer = IssuerUrl::new(oidc_config.issuer_url.clone()).map_err(|e| {
            error!(error = %e, "invalid issuer URL");
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "oidc_config_error", "Invalid OIDC issuer URL")
        })?;

        info!(issuer = %oidc_config.issuer_url, "verify-oidc: starting OIDC discovery");

        let http_client = build_http_client()?;
        let provider_metadata = CoreProviderMetadata::discover_async(issuer, &http_client)
            .await
            .map_err(|e| {
                error!(error = %e, "OIDC discovery failed");
                api_err(StatusCode::BAD_GATEWAY, "oidc_discovery_error", "Failed to discover OIDC provider")
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
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "oidc_config_error", "Invalid redirect URI")
        })?);

        let code_request = client
            .exchange_code(AuthorizationCode::new(req.code))
            .map_err(|e| {
                error!(error = %e, "OIDC token endpoint not configured");
                api_err(StatusCode::INTERNAL_SERVER_ERROR, "oidc_config_error", "Token endpoint not available")
            })?;

        let token_response = code_request
            .set_pkce_verifier(pending.pkce_verifier)
            .request_async(&http_client)
            .await
            .map_err(|e| {
                error!(error = %e, "OIDC token exchange failed");
                api_err(StatusCode::BAD_GATEWAY, "token_exchange_error", "Failed to exchange authorization code")
            })?;

        info!("verify-oidc: token exchange complete, verifying ID token");

        let id_token = token_response.id_token().ok_or_else(|| {
            error!("no ID token in OIDC token response");
            api_err(StatusCode::BAD_GATEWAY, "missing_id_token", "Identity provider did not return an ID token")
        })?;

        let id_token_verifier = client.id_token_verifier();
        let claims = id_token.claims(&id_token_verifier, &pending.nonce).map_err(|e| {
            error!(error = %e, "ID token verification failed");
            api_err(StatusCode::BAD_GATEWAY, "id_token_verification_error", "Failed to verify ID token")
        })?;

        let subject = claims.subject().to_string();
        let email = claims
            .email()
            .map(|addr: &EndUserEmail| addr.to_string())
            .ok_or_else(|| {
                api_err(StatusCode::BAD_GATEWAY, "missing_email", "ID token missing email claim")
            })?;

        info!(email = %email, subject = %subject, "verify-oidc: ID token verified");

        (email, subject)
    };

    // Create owner and transition state
    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
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
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
    })?;

    info!(email = %email, "verify-oidc: owner created, state → OwnerCreated");

    Ok(Json(SetupOidcVerifyResponse {
        state: SetupState::OwnerCreated,
        owner_email: email,
        oidc_subject: subject,
        session_expires_at: sf.setup_session.as_ref().map(|s| s.expires_at),
    }))
}

async fn complete_setup(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<SetupCompleteResponse> {
    let store = state.store.lock().await;
    let mut sf = store.load().await.map_err(|e| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
    })?;

    // State gates first — deterministic 409 regardless of session state
    if sf.setup_state == SetupState::Ready {
        return Err(api_err(StatusCode::CONFLICT, "already_configured", "Setup is already complete"));
    }

    if sf.setup_state != SetupState::OwnerCreated {
        return Err(api_err(
            StatusCode::CONFLICT,
            "invalid_state",
            format!("Setup can only be completed in owner_created state, current: {:?}", sf.setup_state),
        ));
    }

    validate_session(&mut sf, &headers)?;

    let now = now_unix();
    sf.setup_state = SetupState::Ready;
    sf.setup_session = None; // Clear session on completion
    sf.updated_at = now;

    let instance_id = sf.instance_id.clone();

    store.save(&sf).await.map_err(|e| {
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", e.to_string())
    })?;

    Ok(Json(SetupCompleteResponse {
        state: SetupState::Ready,
        instance_id,
    }))
}

// ── Router builder ──────────────────────────────────────────────

/// Build the Axum router with all setup and auth endpoints, given a `SetupStore`
/// and the AES-256 encryption key for secrets at rest.
pub fn build_router(store: SetupStore, encryption_key: Vec<u8>) -> Router {
    build_router_inner(store, encryption_key, false)
}

/// Build a test router that skips real OIDC discovery in `configure_oidc`.
///
/// This allows integration tests to exercise the full setup flow without
/// making any network calls.
pub fn build_test_router(store: SetupStore, encryption_key: Vec<u8>) -> Router {
    build_router_inner(store, encryption_key, true)
}

fn build_router_inner(store: SetupStore, encryption_key: Vec<u8>, skip_oidc_discovery: bool) -> Router {
    let shared_state = Arc::new(AppState {
        store: Mutex::new(store),
        sessions: Mutex::new(SessionStore::new()),
        pending_auth: Mutex::new(HashMap::new()),
        encryption_key,
        skip_oidc_discovery,
    });

    let cors = CorsLayer::new()
        .allow_origin(["http://localhost:3000".parse().unwrap()])
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/public/setup-status", get(setup_status))
        .route(
            "/v1/setup/bootstrap-token/verify",
            post(verify_bootstrap_token),
        )
        .route("/v1/setup/oidc/configure", post(configure_oidc))
        .route("/v1/setup/owner/finalize", post(finalize_owner))
        .route("/v1/setup/owner/start-oidc", post(setup_oidc_start))
        .route("/v1/setup/owner/verify-oidc", post(setup_oidc_verify))
        .route("/v1/setup/complete", post(complete_setup))
        // Auth endpoints (only functional when setup_state == Ready)
        .route("/v1/auth/oidc/start", get(auth::oidc_start))
        .route("/v1/auth/oidc/callback", get(auth::oidc_callback))
        .route("/v1/auth/logout", post(auth::logout))
        .layer(cors)
        .with_state(shared_state)
}
