use std::sync::Arc;

use axum::Json;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use oore_contract::ApiError;

use crate::AppState;
use crate::session::SessionInfo;
use crate::token::hash_token;
use crate::util::{api_err, extract_bearer};

/// Axum extractor that validates the bearer token and returns session info.
///
/// Usage in handlers:
/// ```ignore
/// async fn my_handler(auth: AuthUser) -> impl IntoResponse {
///     let user_id = &auth.0.user_id;
///     // ...
/// }
/// ```
pub struct AuthUser(pub SessionInfo);

impl AuthUser {
    /// Require the user to have one of the specified roles.
    pub fn require_role(&self, allowed: &[&str]) -> Result<(), (StatusCode, Json<ApiError>)> {
        if allowed.contains(&self.0.role.as_str()) {
            Ok(())
        } else {
            Err(api_err(
                StatusCode::FORBIDDEN,
                "insufficient_role",
                "You do not have permission to perform this action",
            ))
        }
    }

    /// Require the user to be the owner.
    pub fn require_owner(&self) -> Result<(), (StatusCode, Json<ApiError>)> {
        self.require_role(&["owner"])
    }

    /// Require the user to be owner or admin.
    pub fn require_admin_or_above(&self) -> Result<(), (StatusCode, Json<ApiError>)> {
        self.require_role(&["owner", "admin"])
    }
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = (StatusCode, Json<ApiError>);

    fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let state = state.clone();
        let headers = parts.headers.clone();

        async move {
            let token = extract_bearer(&headers).ok_or_else(|| {
                api_err(
                    StatusCode::UNAUTHORIZED,
                    "missing_auth",
                    "Authorization header required",
                )
            })?;

            // Try session token first
            let session_result = state.sessions.validate_session(token).await.map_err(|e| {
                tracing::error!(error = %e, "session validation failed");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "session_error",
                    "Session validation failed",
                )
            })?;

            if let Some(session) = session_result {
                return Ok(AuthUser(session));
            }

            // Fallback: try API token
            let pool = state.db.clone();

            let api_token_result = crate::api_tokens::validate_api_token(&pool, token)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "API token validation failed");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "token_error",
                        "Token validation failed",
                    )
                })?;

            match api_token_result {
                Some(info) => {
                    // Fire-and-forget last_used update
                    let pool_clone = pool.clone();
                    let hash = hash_token(token);
                    tokio::spawn(async move {
                        crate::api_tokens::update_last_used(&pool_clone, &hash).await;
                    });
                    Ok(AuthUser(info))
                }
                None => Err(api_err(
                    StatusCode::UNAUTHORIZED,
                    "invalid_session",
                    "Invalid or expired token",
                )),
            }
        }
    }
}
