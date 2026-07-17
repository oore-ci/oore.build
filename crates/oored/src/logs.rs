use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use oore_contract::{
    ApiError, AppendBuildLogsRequest, AppendBuildLogsResponse, BuildLogChunk, BuildLogsResponse,
};
use serde::Deserialize;
use sqlx::{QueryBuilder, Row, Sqlite};
use tokio_stream::Stream;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::runners::RunnerAuth;
use crate::session::{AuthSource, SessionInfo};
use crate::token::{generate_token, hash_token};
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Maximum number of log lines stored per build.
const MAX_LOG_LINES_PER_BUILD: i64 = 10_000;

/// Maximum content length per log line in bytes.
const MAX_LINE_BYTES: usize = 4096;

fn truncate_utf8_bytes(input: &str, max_bytes: usize) -> &str {
    if input.len() <= max_bytes {
        return input;
    }

    let mut boundary = max_bytes.min(input.len());
    while boundary > 0 && !input.is_char_boundary(boundary) {
        boundary -= 1;
    }

    &input[..boundary]
}

/// Polling interval for SSE log streaming.
const SSE_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// How often to query build status while streaming logs.
const SSE_STATUS_CHECK_INTERVAL: Duration = Duration::from_secs(2);

/// Streaming token TTL: 5 minutes.
const STREAM_TOKEN_TTL_SECS: i64 = 300;

const MAX_OUTSTANDING_STREAM_TOKENS: usize = 256;
const MAX_OUTSTANDING_STREAM_TOKENS_PER_USER: usize = 16;
const MAX_OUTSTANDING_STREAM_TOKENS_PER_BUILD: usize = 32;
const MAX_ACTIVE_STREAMS_PER_USER: usize = 4;
const MAX_ACTIVE_STREAMS_PER_BUILD: usize = 16;
const MAX_ACTIVE_STREAMS_GLOBAL: usize = 128;

// ── Stream token store ─────────────────────────────────────────

/// Entry for a short-lived streaming token.
#[derive(Clone)]
struct StreamAuthorization {
    user_id: String,
    credential_hash: String,
    auth_source: AuthSource,
    expires_at: i64,
}

#[derive(Clone)]
struct StreamTokenEntry {
    authorization: StreamAuthorization,
    build_id: String,
}

#[derive(Default)]
struct StreamState {
    tokens: HashMap<String, StreamTokenEntry>,
    active_by_user: HashMap<String, usize>,
    active_by_build: HashMap<String, usize>,
    active_total: usize,
}

/// In-memory store for short-lived SSE streaming tokens.
///
/// These tokens are exchanged from full session tokens before opening an
/// EventSource connection, so the long-lived session token never appears
/// in URL query strings.
pub struct StreamTokenStore {
    state: Arc<Mutex<StreamState>>,
}

impl StreamTokenStore {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(StreamState::default())),
        }
    }

    /// Create a short-lived streaming token derived from a validated session.
    /// Returns the plaintext token and deadline (the store keeps the hash).
    pub fn create(
        &self,
        session: &SessionInfo,
        credential_hash: String,
        build_id: &str,
    ) -> Result<(String, i64), ()> {
        let now = now_unix();
        let expires_at = (now + STREAM_TOKEN_TTL_SECS).min(session.expires_at);
        let mut state = self.lock();
        state
            .tokens
            .retain(|_, entry| entry.authorization.expires_at > now);

        let replace = state.tokens.iter().find_map(|(hash, entry)| {
            (entry.authorization.user_id == session.user_id && entry.build_id == build_id)
                .then(|| hash.clone())
        });
        if replace.is_none() {
            let user_tokens = state
                .tokens
                .values()
                .filter(|entry| entry.authorization.user_id == session.user_id)
                .count();
            let build_tokens = state
                .tokens
                .values()
                .filter(|entry| entry.build_id == build_id)
                .count();
            if state.tokens.len() >= MAX_OUTSTANDING_STREAM_TOKENS
                || user_tokens >= MAX_OUTSTANDING_STREAM_TOKENS_PER_USER
                || build_tokens >= MAX_OUTSTANDING_STREAM_TOKENS_PER_BUILD
            {
                return Err(());
            }
        }
        if let Some(hash) = replace {
            state.tokens.remove(&hash);
        }

        let token = generate_token();
        state.tokens.insert(
            hash_token(&token),
            StreamTokenEntry {
                authorization: StreamAuthorization {
                    user_id: session.user_id.clone(),
                    credential_hash,
                    auth_source: session.auth_source.clone(),
                    expires_at,
                },
                build_id: build_id.to_string(),
            },
        );
        Ok((token, expires_at))
    }

    /// Consume a streaming token. Each capability admits at most one stream.
    fn consume(&self, token: &str) -> Option<StreamTokenEntry> {
        let hashed = hash_token(token);
        let now = now_unix();
        let mut state = self.lock();
        state
            .tokens
            .remove(&hashed)
            .filter(|entry| entry.authorization.expires_at > now)
    }

    fn acquire(&self, user_id: &str, build_id: &str) -> Result<StreamAdmission, ()> {
        let mut state = self.lock();
        if state.active_total >= MAX_ACTIVE_STREAMS_GLOBAL
            || state.active_by_user.get(user_id).copied().unwrap_or(0)
                >= MAX_ACTIVE_STREAMS_PER_USER
            || state.active_by_build.get(build_id).copied().unwrap_or(0)
                >= MAX_ACTIVE_STREAMS_PER_BUILD
        {
            return Err(());
        }

        state.active_total += 1;
        *state.active_by_user.entry(user_id.to_string()).or_default() += 1;
        *state
            .active_by_build
            .entry(build_id.to_string())
            .or_default() += 1;
        Ok(StreamAdmission {
            state: self.state.clone(),
            user_id: user_id.to_string(),
            build_id: build_id.to_string(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, StreamState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

struct StreamAdmission {
    state: Arc<Mutex<StreamState>>,
    user_id: String,
    build_id: String,
}

impl Drop for StreamAdmission {
    fn drop(&mut self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active_total -= 1;
        decrement_count(&mut state.active_by_user, &self.user_id);
        decrement_count(&mut state.active_by_build, &self.build_id);
    }
}

fn decrement_count(counts: &mut HashMap<String, usize>, key: &str) {
    let remove = counts.get(key).is_some_and(|count| *count == 1);
    if remove {
        counts.remove(key);
    } else if let Some(count) = counts.get_mut(key) {
        *count -= 1;
    }
}

impl Default for StreamTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

// ── Query parameters ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GetBuildLogsQuery {
    pub after_sequence: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SseTokenQuery {
    pub token: Option<String>,
}

async fn load_build_project_id(
    pool: &sqlx::SqlitePool,
    build_id: &str,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(build_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load build project");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch build",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))
}

async fn require_build_log_read(
    pool: &sqlx::SqlitePool,
    session: &SessionInfo,
    build_id: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let project_id = load_build_project_id(pool, build_id).await?;
    let effective = resolve_effective_project_role(
        pool,
        &session.user_id,
        &session.role,
        &project_id,
        &session.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::Read)
}

async fn load_current_stream_session(
    pool: &sqlx::SqlitePool,
    authorization: &StreamAuthorization,
) -> Result<Option<SessionInfo>, sqlx::Error> {
    let now = now_unix();
    let row = match authorization.auth_source {
        AuthSource::Session => {
            sqlx::query(
                "SELECT u.id, u.email, u.oidc_subject, u.role, s.expires_at \
                 FROM sessions s JOIN users u ON u.id = s.user_id \
                 WHERE s.token_hash = ?1 AND s.expires_at > ?2 AND u.status = 'active'",
            )
            .bind(&authorization.credential_hash)
            .bind(now)
            .fetch_optional(pool)
            .await?
        }
        AuthSource::ApiToken => {
            sqlx::query(
                "SELECT u.id, u.email, u.oidc_subject, t.role, \
                        COALESCE(t.expires_at, 9223372036854775807) AS expires_at \
                 FROM api_tokens t JOIN users u ON u.id = t.created_by \
                 WHERE t.token_hash = ?1 AND t.revoked_at IS NULL \
                   AND (t.expires_at IS NULL OR t.expires_at > ?2) \
                   AND u.status = 'active'",
            )
            .bind(&authorization.credential_hash)
            .bind(now)
            .fetch_optional(pool)
            .await?
        }
    };

    Ok(row.map(|row| SessionInfo {
        user_id: row.get("id"),
        email: row.get("email"),
        oidc_subject: row.get("oidc_subject"),
        role: row.get("role"),
        expires_at: row.get("expires_at"),
        auth_source: authorization.auth_source.clone(),
    }))
}

async fn require_current_stream_authorization(
    pool: &sqlx::SqlitePool,
    authorization: &StreamAuthorization,
    build_id: &str,
) -> Result<SessionInfo, (StatusCode, Json<ApiError>)> {
    if authorization.expires_at <= now_unix() {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "stream_authorization_ended",
            "Stream authorization expired",
        ));
    }

    let session = load_current_stream_session(pool, authorization)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to revalidate stream authorization");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to validate stream authorization",
            )
        })?
        .filter(|session| session.user_id == authorization.user_id)
        .ok_or_else(|| {
            api_err(
                StatusCode::UNAUTHORIZED,
                "stream_authorization_ended",
                "Stream authorization is no longer active",
            )
        })?;

    require_build_log_read(pool, &session, build_id).await?;
    Ok(session)
}

// ── Handlers ────────────────────────────────────────────────────

/// `POST /v1/runners/{runner_id}/jobs/{job_id}/logs` — runner appends log chunks.
pub async fn append_build_logs(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
    Json(req): Json<AppendBuildLogsRequest>,
) -> ApiResult<AppendBuildLogsResponse> {
    // Verify runner identity matches the path
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    // Verify build exists and belongs to this runner
    let build_row = sqlx::query("SELECT runner_id FROM builds WHERE id = ?1")
        .bind(&job_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch build");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch build",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;

    let build_runner_id: Option<String> = build_row.get("runner_id");
    if build_runner_id.as_deref() != Some(&runner_id) {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "This build is not assigned to your runner",
        ));
    }

    if req.chunks.is_empty() {
        return Ok(Json(AppendBuildLogsResponse { appended: 0 }));
    }

    // Check current log count to enforce cap
    let current_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM build_logs WHERE build_id = ?1")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to count build logs");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to count build logs",
                )
            })?;

    let remaining_capacity = (MAX_LOG_LINES_PER_BUILD - current_count).max(0) as usize;
    if remaining_capacity == 0 {
        warn!(
            build_id = %job_id,
            "build log capacity reached, dropping new chunks"
        );
        return Ok(Json(AppendBuildLogsResponse { appended: 0 }));
    }

    // Limit chunks to remaining capacity
    let chunks_to_insert = if req.chunks.len() > remaining_capacity {
        &req.chunks[..remaining_capacity]
    } else {
        &req.chunks
    };

    let now = now_unix();
    let mut appended: i64 = 0;

    // Batch insert to reduce per-row SQL overhead. 100 keeps us safely under
    // SQLite's bind parameter limit (6 params/row => 600 params/query).
    for batch in chunks_to_insert.chunks(100) {
        let mut qb = QueryBuilder::<Sqlite>::new(
            "INSERT OR IGNORE INTO build_logs (id, build_id, sequence, content, stream, created_at) ",
        );
        qb.push_values(batch, |mut b, chunk| {
            let content = truncate_utf8_bytes(&chunk.content, MAX_LINE_BYTES);
            b.push_bind(Uuid::new_v4().to_string())
                .push_bind(&job_id)
                .push_bind(chunk.sequence)
                .push_bind(content)
                .push_bind(&chunk.stream)
                .push_bind(now);
        });
        let result = qb.build().execute(&pool).await.map_err(|e| {
            error!(error = %e, build_id = %job_id, "failed to insert build log chunk batch");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to insert log chunk",
            )
        })?;
        appended += result.rows_affected() as i64;
    }

    info!(
        build_id = %job_id,
        runner_id = %runner_id,
        appended = appended,
        "log chunks appended"
    );

    Ok(Json(AppendBuildLogsResponse { appended }))
}

/// `GET /v1/builds/{build_id}/logs` — fetch build logs with pagination.
pub async fn get_build_logs(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
    Query(params): Query<GetBuildLogsQuery>,
) -> ApiResult<BuildLogsResponse> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    require_build_log_read(&pool, &auth.0, &build_id).await?;

    let after_seq = params.after_sequence.unwrap_or(-1);
    let limit = params.limit.unwrap_or(1000).min(5000);

    let rows = sqlx::query(
        "SELECT sequence, content, stream FROM build_logs \
         WHERE build_id = ?1 AND sequence > ?2 \
         ORDER BY sequence ASC LIMIT ?3",
    )
    .bind(&build_id)
    .bind(after_seq)
    .bind(limit)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to fetch build logs");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to fetch build logs",
        )
    })?;

    let logs: Vec<BuildLogChunk> = rows
        .iter()
        .map(|r| BuildLogChunk {
            sequence: r.get("sequence"),
            content: r.get("content"),
            stream: r.get("stream"),
        })
        .collect();

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM build_logs WHERE build_id = ?1")
        .bind(&build_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    Ok(Json(BuildLogsResponse { logs, total }))
}

/// `POST /v1/builds/{build_id}/stream-token` — exchange session for a short-lived SSE streaming token.
pub async fn create_stream_token(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<serde_json::Value> {
    // Verify build exists
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    require_build_log_read(&pool, &auth.0, &build_id).await?;

    let credential_hash = hash_token(crate::util::extract_bearer(&headers).ok_or_else(|| {
        api_err(
            StatusCode::UNAUTHORIZED,
            "missing_auth",
            "Authentication required",
        )
    })?);
    let (token, expires_at) = state
        .stream_tokens
        .create(&auth.0, credential_hash, &build_id)
        .map_err(|_| {
            api_err(
                StatusCode::TOO_MANY_REQUESTS,
                "stream_token_limit",
                "Too many outstanding stream tokens",
            )
        })?;

    info!(
        build_id = %build_id,
        user = %auth.0.email,
        "stream token issued"
    );

    Ok(Json(serde_json::json!({
        "token": token,
        "expires_at": expires_at,
    })))
}

/// `GET /v1/builds/{build_id}/logs/stream` — SSE stream for live build logs.
pub async fn stream_build_logs(
    State(state): State<Arc<AppState>>,
    Path(build_id): Path<String>,
    Query(sse_query): Query<SseTokenQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ApiError>)> {
    // Authenticate: ?token= accepts ONLY short-lived stream tokens (never session tokens).
    // Authorization header accepts full session tokens for non-browser clients (curl, etc.).
    // This prevents long-lived session tokens from appearing in URL query strings.
    let query_token = sse_query.token.as_deref();
    let header_token = crate::util::extract_bearer(&headers);

    let authorization = if let Some(qt) = query_token {
        // Query param: must be a valid one-use stream token — reject if not
        let entry = state.stream_tokens.consume(qt).ok_or_else(|| {
            api_err(
                StatusCode::UNAUTHORIZED,
                "invalid_stream_token",
                "Invalid or expired stream token (obtain one via POST /v1/builds/{build_id}/stream-token)",
            )
        })?;
        if entry.build_id != build_id {
            return Err(api_err(
                StatusCode::FORBIDDEN,
                "stream_token_build_mismatch",
                "Stream token is not valid for this build",
            ));
        }
        entry.authorization
    } else if let Some(ht) = header_token {
        // Authorization header: accept full session token (non-browser clients)
        let session = state
            .sessions
            .validate_session(ht)
            .await
            .map_err(|e| {
                error!(error = %e, "session validation failed");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "session_error",
                    "Session validation failed",
                )
            })?
            .ok_or_else(|| {
                api_err(
                    StatusCode::UNAUTHORIZED,
                    "invalid_session",
                    "Invalid or expired session token",
                )
            })?;
        StreamAuthorization {
            user_id: session.user_id,
            credential_hash: hash_token(ht),
            auth_source: AuthSource::Session,
            expires_at: (now_unix() + STREAM_TOKEN_TTL_SECS).min(session.expires_at),
        }
    } else {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "missing_auth",
            "Authentication required (pass token query param or Authorization header)",
        ));
    };

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    let session = require_current_stream_authorization(&pool, &authorization, &build_id).await?;
    let admission = state
        .stream_tokens
        .acquire(&session.user_id, &build_id)
        .map_err(|_| {
            api_err(
                StatusCode::TOO_MANY_REQUESTS,
                "stream_limit",
                "Too many active log streams",
            )
        })?;

    // Check for Last-Event-ID header for reconnection support
    let last_event_id: i64 = headers
        .get("Last-Event-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(-1);

    let stream = build_log_sse_stream(pool, build_id, last_event_id, authorization, admission);

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

/// Produce an SSE stream that polls the DB for new log entries.
fn build_log_sse_stream(
    pool: sqlx::SqlitePool,
    build_id: String,
    initial_last_seq: i64,
    authorization: StreamAuthorization,
    admission: StreamAdmission,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        let _admission = admission;
        let mut last_seq = initial_last_seq;
        let mut interval = tokio::time::interval(SSE_POLL_INTERVAL);
        let mut last_status_check_at = Instant::now();
        let mut check_status_now = true;

        loop {
            interval.tick().await;

            if require_current_stream_authorization(&pool, &authorization, &build_id)
                .await
                .is_err()
            {
                yield Ok(Event::default().event("done").data("authorization_ended"));
                break;
            }

            // Fetch new log entries
            let rows = sqlx::query(
                "SELECT sequence, content, stream FROM build_logs \
                 WHERE build_id = ?1 AND sequence > ?2 \
                 ORDER BY sequence ASC LIMIT 500",
            )
            .bind(&build_id)
            .bind(last_seq)
            .fetch_all(&pool)
            .await;

            match rows {
                Ok(rows) => {
                    for row in &rows {
                        let seq: i64 = row.get("sequence");
                        let chunk = BuildLogChunk {
                            sequence: seq,
                            content: row.get("content"),
                            stream: row.get("stream"),
                        };

                        let data = serde_json::to_string(&chunk).unwrap_or_default();
                        let event = Event::default()
                            .event("log")
                            .id(seq.to_string())
                            .data(data);

                        last_seq = seq;
                        yield Ok(event);
                    }
                }
                Err(e) => {
                    warn!(error = %e, build_id = %build_id, "failed to poll build logs for SSE");
                }
            }

            if check_status_now || last_status_check_at.elapsed() >= SSE_STATUS_CHECK_INTERVAL {
                check_status_now = false;
                last_status_check_at = Instant::now();

                // Check if build is in terminal state
                let status_result: Result<Option<String>, _> =
                    sqlx::query_scalar("SELECT status FROM builds WHERE id = ?1")
                        .bind(&build_id)
                        .fetch_optional(&pool)
                        .await;

                match status_result {
                    Ok(Some(status)) => {
                        let is_terminal = matches!(
                            status.as_str(),
                            "succeeded" | "failed" | "canceled" | "timed_out" | "expired"
                        );
                        if is_terminal {
                            let done_event = Event::default().event("done").data("build_finished");
                            yield Ok(done_event);
                            break;
                        }
                    }
                    Ok(None) => {
                        // Build was deleted
                        let done_event = Event::default().event("done").data("build_not_found");
                        yield Ok(done_event);
                        break;
                    }
                    Err(e) => {
                        warn!(error = %e, build_id = %build_id, "failed to check build status for SSE");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(user_id: impl Into<String>) -> SessionInfo {
        SessionInfo {
            user_id: user_id.into(),
            email: "stream@example.com".to_string(),
            oidc_subject: "stream-subject".to_string(),
            role: "developer".to_string(),
            expires_at: i64::MAX,
            auth_source: AuthSource::Session,
        }
    }

    #[test]
    fn stream_token_store_replaces_same_user_build_capability() {
        let store = StreamTokenStore::new();
        let session = session("user-1");
        let (first, _) = store
            .create(&session, "credential-1".to_string(), "build-1")
            .unwrap();
        let (second, _) = store
            .create(&session, "credential-1".to_string(), "build-1")
            .unwrap();

        assert!(store.consume(&first).is_none());
        assert!(store.consume(&second).is_some());
    }

    #[test]
    fn stream_token_store_caps_outstanding_capabilities() {
        let store = StreamTokenStore::new();
        for index in 0..MAX_OUTSTANDING_STREAM_TOKENS {
            store
                .create(
                    &session(format!("user-{index}")),
                    format!("credential-{index}"),
                    &format!("build-{index}"),
                )
                .unwrap();
        }

        assert!(
            store
                .create(
                    &session("excess-user"),
                    "excess-credential".to_string(),
                    "excess-build",
                )
                .is_err()
        );
    }

    #[test]
    fn stream_token_store_caps_user_and_build_capabilities() {
        let user_store = StreamTokenStore::new();
        let user = session("bounded-user");
        for index in 0..MAX_OUTSTANDING_STREAM_TOKENS_PER_USER {
            user_store
                .create(
                    &user,
                    "bounded-credential".to_string(),
                    &format!("build-{index}"),
                )
                .unwrap();
        }
        assert!(
            user_store
                .create(&user, "bounded-credential".to_string(), "excess-build")
                .is_err()
        );

        let build_store = StreamTokenStore::new();
        for index in 0..MAX_OUTSTANDING_STREAM_TOKENS_PER_BUILD {
            build_store
                .create(
                    &session(format!("user-{index}")),
                    format!("credential-{index}"),
                    "bounded-build",
                )
                .unwrap();
        }
        assert!(
            build_store
                .create(
                    &session("excess-user"),
                    "excess-credential".to_string(),
                    "bounded-build",
                )
                .is_err()
        );
    }

    #[test]
    fn stream_admission_enforces_all_budgets() {
        let user_store = StreamTokenStore::new();
        let mut user_admissions = (0..MAX_ACTIVE_STREAMS_PER_USER)
            .map(|index| user_store.acquire("user", &format!("build-{index}")))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(user_store.acquire("user", "excess-build").is_err());
        user_admissions.pop();
        assert!(user_store.acquire("user", "replacement-build").is_ok());

        let build_store = StreamTokenStore::new();
        let _build_admissions = (0..MAX_ACTIVE_STREAMS_PER_BUILD)
            .map(|index| build_store.acquire(&format!("user-{index}"), "build"))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(build_store.acquire("excess-user", "build").is_err());

        let global_store = StreamTokenStore::new();
        let _global_admissions = (0..MAX_ACTIVE_STREAMS_GLOBAL)
            .map(|index| global_store.acquire(&format!("user-{index}"), &format!("build-{index}")))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(global_store.acquire("excess-user", "excess-build").is_err());
    }
}
