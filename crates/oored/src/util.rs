use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use axum::Json;
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use oore_contract::ApiError;

/// Current UNIX timestamp in seconds.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Resolve the daemon data root directory.
///
/// Resolution order, shared with the `oore` CLI:
/// 1. `OORED_DATA_DIR`
/// 2. `OORE_DATA_DIR`
/// 3. Platform default: `dirs::data_dir()/oore`
pub fn resolve_oored_data_dir() -> anyhow::Result<PathBuf> {
    oore_contract::data_paths::resolve_data_dir(|key| std::env::var(key).ok(), dirs::data_dir())
        .context("could not determine platform data directory (dirs::data_dir)")
}

/// Extract a Bearer token from the Authorization header.
pub fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

/// Convenience error constructor for API error responses.
pub fn api_err(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError::new(code, message)))
}

/// Convert Axum's plain-text extractor rejections into the API error envelope.
pub async fn normalize_extractor_rejection(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    let is_plain_text = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/plain"));

    if !is_plain_text
        || !matches!(
            status,
            StatusCode::BAD_REQUEST
                | StatusCode::UNSUPPORTED_MEDIA_TYPE
                | StatusCode::UNPROCESSABLE_ENTITY
                | StatusCode::PAYLOAD_TOO_LARGE
        )
    {
        return response;
    }

    if status == StatusCode::PAYLOAD_TOO_LARGE {
        return api_err(status, "payload_too_large", "Request body is too large").into_response();
    }

    api_err(
        StatusCode::BAD_REQUEST,
        "invalid_request",
        "Request parameters or body are invalid",
    )
    .into_response()
}
