use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use oore_contract::ApiError;

/// Current UNIX timestamp in seconds.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Extract a Bearer token from the Authorization header.
pub fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

/// Convenience error constructor for API error responses.
pub fn api_err(status: StatusCode, code: &str, message: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError::new(code, message)))
}
