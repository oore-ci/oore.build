mod store;
mod token;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::{Parser, Subcommand};
use oore_contract::{
    ApiError, BootstrapTokenVerifyRequest, BootstrapTokenVerifyResponse, OidcConfigRecord,
    OidcConfigureRequest, OidcConfigureResponse, OwnerFinalizeRequest, OwnerFinalizeResponse,
    OwnerRecord, SetupCompleteResponse, SetupSessionRecord, SetupState, SetupStateFile,
    SetupStatus,
};
use serde_json::json;
use tokio::sync::Mutex;
use tracing::info;

use crate::store::SetupStore;
use crate::token::{generate_session_token, hash_token};

// ── CLI ──────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(name = "oored")]
#[command(about = "oore daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Run(RunArgs),
    InstallService,
    UninstallService,
    Version,
}

#[derive(Debug, clap::Args)]
struct RunArgs {
    #[arg(long, env = "OORED_LISTEN_ADDR", default_value = "127.0.0.1:8787")]
    listen: String,

    /// Path to the setup state file (overrides OORE_SETUP_STATE_FILE and default).
    #[arg(long, env = "OORE_SETUP_STATE_FILE")]
    state_file: Option<String>,
}

// ── Shared application state ─────────────────────────────────────

struct AppState {
    store: Mutex<SetupStore>,
}

// ── Convenience type alias ───────────────────────────────────────

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

// ── Helpers ──────────────────────────────────────────────────────

/// Current UNIX timestamp in seconds.
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Extract a Bearer token from the Authorization header.
fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

/// Validate the setup session token against the current state file.
///
/// Returns `Ok(())` if the session is valid, or an appropriate HTTP error.
fn validate_session(
    state_file: &SetupStateFile,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let token = extract_bearer(headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new("missing_auth", "Authorization header required")),
        )
    })?;

    let session = state_file.setup_session.as_ref().ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new("no_session", "No active setup session")),
        )
    })?;

    let hashed = hash_token(token);
    if hashed != session.hash {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new("invalid_session", "Invalid session token")),
        ));
    }

    if now_unix() > session.expires_at {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new("session_expired", "Setup session has expired")),
        ));
    }

    Ok(())
}

// ── Handlers ─────────────────────────────────────────────────────

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({"ok": true}))
}

async fn setup_status(State(state): State<Arc<AppState>>) -> ApiResult<SetupStatus> {
    let store = state.store.lock().await;
    let sf = store.load().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("store_error", e.to_string())),
        )
    })?;

    Ok(Json(SetupStatus::from_state(sf.instance_id, sf.setup_state)))
}

async fn verify_bootstrap_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BootstrapTokenVerifyRequest>,
) -> ApiResult<BootstrapTokenVerifyResponse> {
    let store = state.store.lock().await;
    let mut sf = store.load().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("store_error", e.to_string())),
        )
    })?;

    // Setup must not be complete — all setup endpoints are disabled after ready
    if sf.setup_state == SetupState::Ready {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new(
                "already_configured",
                "Setup is already complete",
            )),
        ));
    }

    // Bootstrap token record must exist
    let bt = sf.bootstrap_token.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(
                "no_bootstrap_token",
                "No bootstrap token has been generated",
            )),
        )
    })?;

    // Must not already be consumed
    if bt.consumed_at.is_some() {
        return Err((
            StatusCode::GONE,
            Json(ApiError::new(
                "token_consumed",
                "Bootstrap token has already been consumed",
            )),
        ));
    }

    // Must not be expired
    if now_unix() > bt.expires_at {
        return Err((
            StatusCode::GONE,
            Json(ApiError::new(
                "token_expired",
                "Bootstrap token has expired",
            )),
        ));
    }

    // Hash must match
    let request_hash = hash_token(&req.token);
    if request_hash != bt.hash {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new(
                "invalid_token",
                "Bootstrap token is invalid",
            )),
        ));
    }

    // Mark token as consumed
    let now = now_unix();
    if let Some(ref mut bt_mut) = sf.bootstrap_token {
        bt_mut.consumed_at = Some(now);
    }

    // Generate session token and store its hash with 30-min expiry
    let session_token = generate_session_token();
    let session_expires_at = now + 30 * 60; // 30 minutes
    sf.setup_session = Some(SetupSessionRecord {
        hash: hash_token(&session_token),
        expires_at: session_expires_at,
    });

    sf.updated_at = now;

    store.save(&sf).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("store_error", e.to_string())),
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
    let store = state.store.lock().await;
    let mut sf = store.load().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("store_error", e.to_string())),
        )
    })?;

    // State gates first — deterministic 409 regardless of session state
    if sf.setup_state == SetupState::Ready {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new(
                "already_configured",
                "Setup is already complete",
            )),
        ));
    }

    if sf.setup_state != SetupState::BootstrapPending {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new(
                "invalid_state",
                format!(
                    "OIDC can only be configured in bootstrap_pending state, current: {:?}",
                    sf.setup_state
                ),
            )),
        ));
    }

    validate_session(&sf, &headers)?;

    let now = now_unix();
    sf.oidc_config = Some(OidcConfigRecord {
        issuer_url: req.issuer_url,
        client_id: req.client_id,
        has_client_secret: req.client_secret.is_some(),
        configured_at: now,
    });

    sf.setup_state = SetupState::IdpConfigured;
    sf.updated_at = now;

    store.save(&sf).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("store_error", e.to_string())),
        )
    })?;

    Ok(Json(OidcConfigureResponse {
        state: SetupState::IdpConfigured,
    }))
}

async fn finalize_owner(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<OwnerFinalizeRequest>,
) -> ApiResult<OwnerFinalizeResponse> {
    let store = state.store.lock().await;
    let mut sf = store.load().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("store_error", e.to_string())),
        )
    })?;

    // State gates first — deterministic 409 regardless of session state
    if sf.setup_state == SetupState::Ready {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new(
                "already_configured",
                "Setup is already complete",
            )),
        ));
    }

    if sf.setup_state != SetupState::IdpConfigured {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new(
                "invalid_state",
                format!(
                    "Owner can only be finalized in idp_configured state, current: {:?}",
                    sf.setup_state
                ),
            )),
        ));
    }

    validate_session(&sf, &headers)?;

    let now = now_unix();
    sf.owner = Some(OwnerRecord {
        email: req.owner_email,
        created_at: now,
    });

    sf.setup_state = SetupState::OwnerCreated;
    sf.updated_at = now;

    store.save(&sf).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("store_error", e.to_string())),
        )
    })?;

    Ok(Json(OwnerFinalizeResponse {
        state: SetupState::OwnerCreated,
    }))
}

async fn complete_setup(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<SetupCompleteResponse> {
    let store = state.store.lock().await;
    let mut sf = store.load().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("store_error", e.to_string())),
        )
    })?;

    // State gates first — deterministic 409 regardless of session state
    if sf.setup_state == SetupState::Ready {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new(
                "already_configured",
                "Setup is already complete",
            )),
        ));
    }

    if sf.setup_state != SetupState::OwnerCreated {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new(
                "invalid_state",
                format!(
                    "Setup can only be completed in owner_created state, current: {:?}",
                    sf.setup_state
                ),
            )),
        ));
    }

    validate_session(&sf, &headers)?;

    let now = now_unix();
    sf.setup_state = SetupState::Ready;
    sf.setup_session = None; // Clear session on completion
    sf.updated_at = now;

    let instance_id = sf.instance_id.clone();

    store.save(&sf).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("store_error", e.to_string())),
        )
    })?;

    Ok(Json(SetupCompleteResponse {
        state: SetupState::Ready,
        instance_id,
    }))
}

// ── Server bootstrap ─────────────────────────────────────────────

async fn run_server(args: RunArgs) -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let addr: SocketAddr = args
        .listen
        .parse()
        .with_context(|| format!("invalid listen address: {}", args.listen))?;

    // Resolve state file path and initialise store
    let state_path = SetupStore::resolve_path(args.state_file.as_deref())
        .context("failed to resolve state file path")?;
    info!(path = %state_path.display(), "using state file");

    let store = SetupStore::new(state_path);
    let initial = store
        .init_if_missing()
        .context("failed to initialise state file")?;

    info!(
        instance_id = %initial.instance_id,
        state = ?initial.setup_state,
        "state file ready"
    );

    let shared_state = Arc::new(AppState {
        store: Mutex::new(store),
    });

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/public/setup-status", get(setup_status))
        .route(
            "/v1/setup/bootstrap-token/verify",
            post(verify_bootstrap_token),
        )
        .route("/v1/setup/oidc/configure", post(configure_oidc))
        .route("/v1/setup/owner/finalize", post(finalize_owner))
        .route("/v1/setup/complete", post(complete_setup))
        .with_state(shared_state);

    info!(listen = %addr, "starting oored daemon");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .await
        .context("oored server failed")?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(run_server(args))?;
        }
        Commands::InstallService => {
            println!("install-service placeholder (launchd integration pending)");
        }
        Commands::UninstallService => {
            println!("uninstall-service placeholder (launchd integration pending)");
        }
        Commands::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
        }
    }

    Ok(())
}
