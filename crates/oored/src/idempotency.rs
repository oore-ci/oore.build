//! `Idempotency-Key` for requests that start something: a retried request
//! with the same key and body replays the first answer instead of starting
//! a second build or rollout step.

use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use tracing::{error, warn};

use crate::AppState;
use crate::util::{api_err, extract_bearer, now_unix};

pub const IDEMPOTENCY_KEY: HeaderName = HeaderName::from_static("idempotency-key");
pub const IDEMPOTENT_REPLAYED: HeaderName = HeaderName::from_static("idempotent-replayed");

/// How long a key and its response are kept.
pub const KEY_TTL_SECS: i64 = 24 * 60 * 60;
const MAX_KEY_LEN: usize = 255;
/// Requests and responses past this size are not kept.
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

fn sha256_hex(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hex::encode(hasher.finalize())
}

fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= MAX_KEY_LEN && key.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

/// Middleware for the routes that take `Idempotency-Key`.
pub async fn idempotent(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let Some(key) = request.headers().get(&IDEMPOTENCY_KEY) else {
        return next.run(request).await;
    };
    let Some(key) = key
        .to_str()
        .ok()
        .filter(|key| valid_key(key))
        .map(str::to_string)
    else {
        return api_err(
            StatusCode::BAD_REQUEST,
            "invalid_idempotency_key",
            "Idempotency-Key must be 1 to 255 visible ASCII characters",
        )
        .into_response();
    };
    // Without a credential the request fails sign-in; nothing to keep.
    let Some(credential) = extract_bearer(request.headers()).map(crate::token::hash_token) else {
        return next.run(request).await;
    };

    let (parts, body) = request.into_parts();
    let Ok(body) = to_bytes(body, MAX_BODY_BYTES).await else {
        return api_err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Request body is too large",
        )
        .into_response();
    };
    let scope = sha256_hex(&[
        credential.as_bytes(),
        parts.method.as_str().as_bytes(),
        parts.uri.path().as_bytes(),
        key.as_bytes(),
    ]);
    let body_hash = sha256_hex(&[&body]);
    let pool = &state.db;
    let now = now_unix();

    match claim(pool, &scope, &body_hash, now).await {
        Ok(Claim::New) => {}
        Ok(Claim::Existing(stored)) => return stored.answer(&body_hash),
        Err(e) => {
            error!(error = %e, "failed to check the idempotency key");
            return api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to check the Idempotency-Key",
            )
            .into_response();
        }
    }

    let response = next.run(Request::from_parts(parts, Body::from(body))).await;
    let status = response.status();
    // A failure that may pass on a later try is not kept, so the retry runs.
    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        release(pool, &scope).await;
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = to_bytes(body, MAX_BODY_BYTES).await else {
        release(pool, &scope).await;
        return api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "The response was too large to keep for Idempotency-Key",
        )
        .into_response();
    };
    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    if let Err(e) = sqlx::query(
        "UPDATE idempotency_keys SET status = ?1, content_type = ?2, response_body = ?3 \
         WHERE scope_hash = ?4",
    )
    .bind(i64::from(status.as_u16()))
    .bind(&content_type)
    .bind(bytes.as_ref())
    .bind(&scope)
    .execute(pool)
    .await
    {
        warn!(error = %e, "failed to keep the response for its idempotency key");
    }
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(bytes))
}

enum Claim {
    New,
    Existing(Stored),
}

struct Stored {
    body_hash: String,
    status: Option<i64>,
    content_type: Option<String>,
    body: Option<Vec<u8>>,
}

impl Stored {
    fn answer(self, body_hash: &str) -> Response {
        if self.body_hash != body_hash {
            return api_err(
                StatusCode::CONFLICT,
                "idempotency_key_reused",
                "This Idempotency-Key was already used with a different request body",
            )
            .into_response();
        }
        let Some(status) = self
            .status
            .and_then(|status| u16::try_from(status).ok())
            .and_then(|status| StatusCode::from_u16(status).ok())
        else {
            return api_err(
                StatusCode::CONFLICT,
                "idempotency_in_progress",
                "A request with this Idempotency-Key is still running",
            )
            .into_response();
        };
        let mut response = Response::new(Body::from(self.body.unwrap_or_default()));
        *response.status_mut() = status;
        if let Some(content_type) = self
            .content_type
            .and_then(|value| HeaderValue::from_str(&value).ok())
        {
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, content_type);
        }
        response
            .headers_mut()
            .insert(IDEMPOTENT_REPLAYED, HeaderValue::from_static("true"));
        response
    }
}

async fn claim(pool: &SqlitePool, scope: &str, body_hash: &str, now: i64) -> sqlx::Result<Claim> {
    sqlx::query("DELETE FROM idempotency_keys WHERE scope_hash = ?1 AND created_at < ?2")
        .bind(scope)
        .bind(now - KEY_TTL_SECS)
        .execute(pool)
        .await?;
    let inserted = sqlx::query(
        "INSERT INTO idempotency_keys (scope_hash, body_hash, created_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(scope_hash) DO NOTHING",
    )
    .bind(scope)
    .bind(body_hash)
    .bind(now)
    .execute(pool)
    .await?;
    if inserted.rows_affected() == 1 {
        return Ok(Claim::New);
    }
    let row = sqlx::query(
        "SELECT body_hash, status, content_type, response_body FROM idempotency_keys \
         WHERE scope_hash = ?1",
    )
    .bind(scope)
    .fetch_one(pool)
    .await?;
    Ok(Claim::Existing(Stored {
        body_hash: row.get("body_hash"),
        status: row.get("status"),
        content_type: row.get("content_type"),
        body: row.get("response_body"),
    }))
}

async fn release(pool: &SqlitePool, scope: &str) {
    if let Err(e) = sqlx::query("DELETE FROM idempotency_keys WHERE scope_hash = ?1")
        .bind(scope)
        .execute(pool)
        .await
    {
        warn!(error = %e, "failed to release an idempotency key");
    }
}

/// Removes keys past their 24 hours. Run from the background tasks.
pub async fn delete_expired_keys(pool: &SqlitePool) -> sqlx::Result<u64> {
    Ok(
        sqlx::query("DELETE FROM idempotency_keys WHERE created_at < ?1")
            .bind(now_unix() - KEY_TTL_SECS)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}
