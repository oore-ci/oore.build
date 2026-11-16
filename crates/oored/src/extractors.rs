use std::sync::Arc;

use axum::Json;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
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
    ///
    /// These checks guard instance administration, so an API token also
    /// needs the `manage` ability.
    pub fn require_role(&self, allowed: &[&str]) -> Result<(), (StatusCode, Json<ApiError>)> {
        let admin_only = !allowed.contains(&"developer") && !allowed.contains(&"qa_viewer");
        if allowed.contains(&self.0.role.as_str())
            && (!admin_only || self.0.has_ability(crate::session::Ability::Manage))
        {
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

/// The signed-in caller, resolved once per request by `scope_request_auth`.
#[derive(Clone)]
struct ResolvedAuth(SessionInfo);

/// Resolve a bearer credential to a session: a sign-in session first, then
/// an API token. `Ok(None)` means the credential is not valid.
pub async fn authenticate_bearer(
    state: &AppState,
    token: &str,
) -> Result<Option<SessionInfo>, (StatusCode, Json<ApiError>)> {
    let session_result = state.sessions.validate_session(token).await.map_err(|e| {
        tracing::error!(error = %e, "session validation failed");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "session_error",
            "Session validation failed",
        )
    })?;
    if let Some(session) = session_result {
        return Ok(Some(session));
    }

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
    if api_token_result.is_some() {
        // Fire-and-forget last_used update
        let hash = hash_token(token);
        tokio::spawn(async move {
            crate::api_tokens::update_last_used(&pool, &hash).await;
        });
    }
    Ok(api_token_result)
}

/// Router layer: sign the caller in once, and run the rest of the request
/// with their API token's grant in scope, so the permission helpers and the
/// audit log see which token acted and what it may do.
pub async fn scope_request_auth(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let mut grant = None;
    if let Some(token) = extract_bearer(request.headers()).map(str::to_owned)
        && let Ok(Some(session)) = authenticate_bearer(&state, &token).await
    {
        grant = session.token.clone();
        request.extensions_mut().insert(ResolvedAuth(session));
    }
    crate::session::scope_request_grant(grant, next.run(request)).await
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = (StatusCode, Json<ApiError>);

    fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let state = state.clone();
        let headers = parts.headers.clone();
        let resolved = parts.extensions.get::<ResolvedAuth>().cloned();

        async move {
            if let Some(ResolvedAuth(session)) = resolved {
                return Ok(AuthUser(session));
            }

            let token = extract_bearer(&headers).ok_or_else(|| {
                api_err(
                    StatusCode::UNAUTHORIZED,
                    "missing_auth",
                    "Authorization header required",
                )
            })?;

            match authenticate_bearer(&state, token).await? {
                // A token's limits are enforced through the request scope
                // that `scope_request_auth` sets. Refuse a token on a route
                // outside that layer rather than let it act unlimited.
                Some(info) if info.token.is_some() && !crate::session::in_request_scope() => {
                    Err(api_err(
                        StatusCode::FORBIDDEN,
                        "api_token_not_supported",
                        "API tokens cannot be used on this route",
                    ))
                }
                Some(info) => Ok(AuthUser(info)),
                None => Err(api_err(
                    StatusCode::UNAUTHORIZED,
                    "invalid_session",
                    "Invalid or expired token",
                )),
            }
        }
    }
}
