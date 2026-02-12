use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use axum::Json;
use axum::http::{HeaderMap, StatusCode};
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
/// Resolution order:
/// 1. `OORED_DATA_DIR`
/// 2. `OORE_DATA_DIR`
/// 3. Platform default: `dirs::data_dir()/oore`
pub fn resolve_oored_data_dir() -> anyhow::Result<PathBuf> {
    for key in ["OORED_DATA_DIR", "OORE_DATA_DIR"] {
        if let Ok(raw) = std::env::var(key) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return Ok(PathBuf::from(trimmed));
            }
        }
    }

    let data_dir =
        dirs::data_dir().context("could not determine platform data directory (dirs::data_dir)")?;
    Ok(data_dir.join("oore"))
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
