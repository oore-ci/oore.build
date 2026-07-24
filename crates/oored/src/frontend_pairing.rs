use std::net::SocketAddr;
use std::sync::Arc;

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use oore_contract::{ApiError, FrontendPairRequest, FrontendPairResponse, SetupState};
use tracing::{error, warn};

use crate::AppState;
use crate::instance_settings;
use crate::token::hash_token;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

pub async fn pair(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    Json(request): Json<FrontendPairRequest>,
) -> ApiResult<FrontendPairResponse> {
    let code = request.code.trim();
    if !code.starts_with("fp_") || code.len() > 128 {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_pairing_code",
            "Pairing code is invalid, expired, or already used",
        ));
    }

    let setup = {
        let store = state.store.lock().await;
        store.load().await.map_err(|error| {
            error!(%error, "failed to load setup state for frontend pairing");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load setup state",
            )
        })?
    };
    if setup.setup_state != SetupState::Ready {
        return Err(api_err(
            StatusCode::CONFLICT,
            "setup_incomplete",
            "Backend setup must be complete before frontend pairing",
        ));
    }

    let settings = instance_settings::load_effective_trusted_proxy_settings(&state.db)
        .await
        .map_err(|error| {
            error!(%error, "failed to load trusted proxy settings for frontend pairing");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load trusted proxy settings",
            )
        })?;
    if !instance_settings::is_trusted_proxy_peer(peer_addr.ip(), &settings) {
        warn!(peer_ip = %peer_addr.ip(), "rejected frontend pairing from non-allowlisted peer");
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "pairing_peer_not_allowed",
            "Frontend pairing is not allowed from this network peer",
        ));
    }

    let encrypted_proof = settings.encrypted_shared_secret.as_deref().ok_or_else(|| {
        api_err(
            StatusCode::CONFLICT,
            "trusted_proxy_not_configured",
            "Trusted Proxy backend proof is not configured",
        )
    })?;
    let backend_proof = crate::crypto::decrypt(encrypted_proof, state.encryption_key.as_slice())
        .map_err(|error| {
            error!(%error, "failed to decrypt backend proof for frontend pairing");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "decryption_failed",
                "Failed to prepare frontend pairing proof",
            )
        })?;

    let token_hash = hash_token(code);
    let now = now_unix();
    let mut transaction = state.db.begin().await.map_err(|error| {
        error!(%error, "failed to begin frontend pairing transaction");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to consume frontend pairing invite",
        )
    })?;
    let invite_id: Option<String> = sqlx::query_scalar(
        "UPDATE frontend_pairing_invites SET consumed_at = ?1 \
         WHERE token_hash = ?2 AND consumed_at IS NULL AND expires_at >= ?1 \
         RETURNING id",
    )
    .bind(now)
    .bind(token_hash)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| {
        error!(%error, "failed to consume frontend pairing invite");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to consume frontend pairing invite",
        )
    })?;
    let Some(invite_id) = invite_id else {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_pairing_code",
            "Pairing code is invalid, expired, or already used",
        ));
    };
    let audit_details = serde_json::json!({ "peer_ip": peer_addr.ip().to_string() }).to_string();
    sqlx::query(
        "INSERT INTO audit_logs (actor_id, action, resource_type, resource_id, details, created_at) \
         VALUES (NULL, 'frontend_pairing_completed', 'frontend_pairing_invite', ?1, ?2, ?3)",
    )
    .bind(&invite_id)
    .bind(audit_details)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        error!(%error, "failed to audit frontend pairing completion");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to complete frontend pairing",
        )
    })?;
    transaction.commit().await.map_err(|error| {
        error!(%error, "failed to commit frontend pairing transaction");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to complete frontend pairing",
        )
    })?;

    Ok(Json(FrontendPairResponse {
        backend_proof,
        user_email_header: settings.user_email_header,
    }))
}
