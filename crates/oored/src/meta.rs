//! What a tool needs to know about the instance it talks to: the API
//! contract this daemon was built from, and which optional features it has.

use std::sync::Arc;

use axum::Json;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use oore_contract::{ApiError, InstanceAuthMode, InstanceMeta, RemoteAuthMode, RuntimeMode};
use tracing::error;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::util::api_err;

/// The OpenAPI file generated from this source tree. `make check-openapi`
/// keeps it equal to what `openapi-export` prints.
const OPENAPI_JSON: &str = include_str!("../../../apps/docs/public/openapi.json");

/// Optional API features every daemon of this version has. Add one string
/// per feature a client may want to detect; clients ignore unknown ones.
const FEATURES: &[&str] = &[
    "diagnosis",
    "log_filters",
    "build_wait",
    "pipeline_search",
    "error_hints",
    "idempotency",
    "event_replay",
];

/// `GET /v1/openapi.json` — the API contract this daemon serves.
pub async fn get_openapi() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/json")], OPENAPI_JSON)
}

/// `GET /v1/meta` — setup state, sign-in mode and features. The version and
/// channel are only for a signed-in caller.
pub async fn get_meta(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Result<Json<InstanceMeta>, (StatusCode, Json<ApiError>)> {
    let store_error = |e: anyhow::Error| {
        error!(error = %e, "failed to load instance meta");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load instance details",
        )
    };
    let setup_state = {
        let store = state.store.lock().await;
        store.load().await.map_err(store_error)?.setup_state
    };
    let runtime_mode = crate::instance_settings::load_runtime_mode(&state.db)
        .await
        .map_err(store_error)?;
    let auth_mode = match runtime_mode {
        RuntimeMode::Local => InstanceAuthMode::Local,
        RuntimeMode::Remote => {
            match crate::instance_settings::load_remote_auth_mode(&state.db)
                .await
                .map_err(store_error)?
            {
                RemoteAuthMode::Oidc => InstanceAuthMode::Oidc,
                RemoteAuthMode::TrustedProxy => InstanceAuthMode::TrustedProxy,
            }
        }
    };
    let mut features: Vec<String> = FEATURES.iter().map(|f| (*f).to_string()).collect();
    if crate::instance_settings::load_mcp_enabled(&state.db)
        .await
        .map_err(store_error)?
    {
        features.push("mcp".to_string());
    }

    let (mut parts, _) = request.into_parts();
    let signed_in = parts.headers.contains_key(header::AUTHORIZATION)
        && AuthUser::from_request_parts(&mut parts, &state)
            .await
            .is_ok();
    let (version, channel) = if signed_in {
        let runtime = crate::installed_runtime_metadata();
        (Some(runtime.version), runtime.channel)
    } else {
        (None, None)
    };

    Ok(Json(InstanceMeta {
        setup_state,
        auth_mode,
        features,
        version,
        channel,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_spec_is_json_with_paths() {
        let spec: serde_json::Value = serde_json::from_str(OPENAPI_JSON).unwrap();
        assert!(spec["paths"]["/v1/meta"].is_object());
        assert!(spec["paths"]["/v1/openapi.json"].is_object());
    }
}
