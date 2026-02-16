use std::sync::{Arc, OnceLock};

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use oore_contract::ApiError;
use ring::hmac;
use serde::Serialize;
use sqlx::Row;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::require_remote_mode;
use crate::AppState;
use crate::crypto;
use crate::util::{api_err, now_unix};

/// Maximum webhook body size (1 MB).
const MAX_WEBHOOK_BODY_SIZE: usize = 1_048_576;

/// Maximum age of a webhook event before rejection (5 minutes).
const MAX_WEBHOOK_AGE_SECS: i64 = 300;
/// Webhook secret cache TTL (seconds).
const WEBHOOK_SECRET_CACHE_TTL_SECS: i64 = 60;

#[derive(Debug, Clone)]
struct CachedWebhookSecret {
    integration_id: String,
    secret: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ProviderSecretCache {
    refreshed_at: i64,
    entries: Arc<Vec<CachedWebhookSecret>>,
}

impl Default for ProviderSecretCache {
    fn default() -> Self {
        Self {
            refreshed_at: 0,
            entries: Arc::new(Vec::new()),
        }
    }
}

#[derive(Debug, Default)]
struct WebhookSecretCache {
    github: ProviderSecretCache,
    gitlab: ProviderSecretCache,
}

impl WebhookSecretCache {
    fn provider(&self, provider: WebhookProvider) -> &ProviderSecretCache {
        match provider {
            WebhookProvider::Github => &self.github,
            WebhookProvider::Gitlab => &self.gitlab,
        }
    }

    fn provider_mut(&mut self, provider: WebhookProvider) -> &mut ProviderSecretCache {
        match provider {
            WebhookProvider::Github => &mut self.github,
            WebhookProvider::Gitlab => &mut self.gitlab,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum WebhookProvider {
    Github,
    Gitlab,
}

impl WebhookProvider {
    fn as_str(self) -> &'static str {
        match self {
            WebhookProvider::Github => "github",
            WebhookProvider::Gitlab => "gitlab",
        }
    }
}

static WEBHOOK_SECRET_CACHE: OnceLock<tokio::sync::RwLock<WebhookSecretCache>> = OnceLock::new();

fn webhook_secret_cache() -> &'static tokio::sync::RwLock<WebhookSecretCache> {
    WEBHOOK_SECRET_CACHE.get_or_init(|| tokio::sync::RwLock::new(WebhookSecretCache::default()))
}

async fn get_webhook_secrets(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    provider: WebhookProvider,
    force_refresh: bool,
) -> Result<Arc<Vec<CachedWebhookSecret>>, (StatusCode, Json<ApiError>)> {
    let now = now_unix();

    if !force_refresh {
        let cached = {
            let cache = webhook_secret_cache().read().await;
            let slot = cache.provider(provider);
            if now - slot.refreshed_at <= WEBHOOK_SECRET_CACHE_TTL_SECS {
                Some(slot.entries.clone())
            } else {
                None
            }
        };

        if let Some(entries) = cached {
            return Ok(entries);
        }
    }

    let rows = sqlx::query(
        "SELECT i.id, c.encrypted_value \
         FROM integrations i \
         JOIN integration_credentials c ON c.integration_id = i.id \
         WHERE i.provider = ?1 AND i.status = 'active' \
         AND c.credential_type = 'webhook_secret'",
    )
    .bind(provider.as_str())
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!(error = %e, provider = provider.as_str(), "failed to fetch webhook integrations");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Internal error",
        )
    })?;

    let mut decrypted = Vec::with_capacity(rows.len());
    for row in rows {
        let integration_id: String = row.get("id");
        let encrypted_secret: String = row.get("encrypted_value");
        let secret = match crypto::decrypt(&encrypted_secret, encryption_key) {
            Ok(s) => s.into_bytes(),
            Err(e) => {
                error!(
                    error = %e,
                    integration_id = %integration_id,
                    provider = provider.as_str(),
                    "failed to decrypt webhook secret"
                );
                continue;
            }
        };
        decrypted.push(CachedWebhookSecret {
            integration_id,
            secret,
        });
    }

    let entries = Arc::new(decrypted);
    let mut cache = webhook_secret_cache().write().await;
    let slot = cache.provider_mut(provider);
    slot.refreshed_at = now;
    slot.entries = entries.clone();
    drop(cache);

    Ok(entries)
}

/// Normalized webhook event for downstream processing.
#[derive(Debug, Clone, Serialize)]
pub struct NormalizedWebhookEvent {
    pub provider: String,
    pub event_type: String,
    pub delivery_id: String,
    pub integration_id: String,
    pub repository_full_name: Option<String>,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub actor: Option<String>,
    pub payload: serde_json::Value,
}

// ── GitHub webhook handler ──────────────────────────────────────

/// `POST /v1/webhooks/github` — GitHub webhook receiver.
///
/// No auth middleware, no CORS — called directly by GitHub.
/// Verifies X-Hub-Signature-256 HMAC, checks idempotency, ACKs fast.
pub async fn github_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    // Body size check
    if body.len() > MAX_WEBHOOK_BODY_SIZE {
        return Err(api_err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Webhook payload exceeds size limit",
        ));
    }

    // Extract required headers
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            warn!("GitHub webhook missing X-Hub-Signature-256");
            api_err(
                StatusCode::UNAUTHORIZED,
                "missing_signature",
                "X-Hub-Signature-256 header required",
            )
        })?;

    let delivery_id = headers
        .get("x-github-delivery")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let event_type = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Parse payload to extract repository info for integration resolution
    let payload: serde_json::Value = serde_json::from_slice(&body).map_err(|e| {
        error!(error = %e, "failed to parse GitHub webhook payload");
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_payload",
            "Invalid JSON payload",
        )
    })?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    require_remote_mode(&pool).await?;

    let secrets =
        get_webhook_secrets(&pool, &state.encryption_key, WebhookProvider::Github, false).await?;

    let mut matched_integration_id = secrets
        .iter()
        .find(|candidate| verify_github_signature(signature, &body, &candidate.secret))
        .map(|candidate| candidate.integration_id.clone());

    if matched_integration_id.is_none() {
        let refreshed =
            get_webhook_secrets(&pool, &state.encryption_key, WebhookProvider::Github, true)
                .await?;

        if !Arc::ptr_eq(&secrets, &refreshed) {
            matched_integration_id = refreshed
                .iter()
                .find(|candidate| verify_github_signature(signature, &body, &candidate.secret))
                .map(|candidate| candidate.integration_id.clone());
        }
    }

    let integration_id = matched_integration_id.ok_or_else(|| {
        warn!("GitHub webhook signature verification failed for all integrations");
        api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_signature",
            "Webhook signature verification failed",
        )
    })?;

    // Idempotency check
    if !delivery_id.is_empty() {
        let existing: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM integration_webhooks \
             WHERE integration_id = ?1 AND provider_delivery_id = ?2",
        )
        .bind(&integration_id)
        .bind(&delivery_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(false);

        if existing {
            info!(delivery_id = %delivery_id, "duplicate GitHub webhook delivery, returning OK");
            return Ok(Json(serde_json::json!({ "ok": true, "duplicate": true })));
        }
    }

    let now = now_unix();

    // Store webhook record
    let webhook_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO integration_webhooks (id, integration_id, provider_delivery_id, event_type, payload, status, received_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'received', ?6)",
    )
    .bind(&webhook_id)
    .bind(&integration_id)
    .bind(&delivery_id)
    .bind(&event_type)
    .bind(payload.to_string())
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to store webhook record");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to store webhook")
    })?;

    // Normalize the event for downstream processing
    let normalized = normalize_github_event(&event_type, &delivery_id, &integration_id, &payload);

    // Process asynchronously — clone what we need
    let pool_clone = pool.clone();
    let webhook_id_clone = webhook_id.clone();
    tokio::spawn(async move {
        // Process the webhook event (trigger builds, etc.)
        if let Err(e) = process_webhook_event(&pool_clone, &webhook_id_clone, &normalized).await {
            error!(error = ?e, webhook_id = %webhook_id_clone, "webhook processing failed");
        }
    });

    info!(
        delivery_id = %delivery_id,
        event_type = %event_type,
        integration_id = %integration_id,
        "GitHub webhook received"
    );

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Verify GitHub HMAC-SHA256 signature.
fn verify_github_signature(signature_header: &str, body: &[u8], secret: &[u8]) -> bool {
    let expected = match signature_header.strip_prefix("sha256=") {
        Some(hex) => hex,
        None => return false,
    };

    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let tag = hmac::sign(&key, body);
    let computed = hex::encode(tag.as_ref());

    // Constant-time comparison
    computed.len() == expected.len()
        && computed
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

fn normalize_github_event(
    event_type: &str,
    delivery_id: &str,
    integration_id: &str,
    payload: &serde_json::Value,
) -> NormalizedWebhookEvent {
    let repo_full_name = payload
        .get("repository")
        .and_then(|r| r.get("full_name"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let (branch, commit_sha) = match event_type {
        "push" => {
            let git_ref = payload.get("ref").and_then(|v| v.as_str()).unwrap_or("");
            let branch = git_ref.strip_prefix("refs/heads/").map(String::from);
            let sha = payload
                .get("after")
                .and_then(|v| v.as_str())
                .map(String::from);
            (branch, sha)
        }
        "pull_request" => {
            let branch = payload
                .get("pull_request")
                .and_then(|pr| pr.get("head"))
                .and_then(|h| h.get("ref"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let sha = payload
                .get("pull_request")
                .and_then(|pr| pr.get("head"))
                .and_then(|h| h.get("sha"))
                .and_then(|v| v.as_str())
                .map(String::from);
            (branch, sha)
        }
        _ => (None, None),
    };

    let actor = payload
        .get("sender")
        .and_then(|s| s.get("login"))
        .and_then(|v| v.as_str())
        .map(String::from);

    NormalizedWebhookEvent {
        provider: "github".to_string(),
        event_type: event_type.to_string(),
        delivery_id: delivery_id.to_string(),
        integration_id: integration_id.to_string(),
        repository_full_name: repo_full_name,
        branch,
        commit_sha,
        actor,
        payload: payload.clone(),
    }
}

// ── GitLab webhook handler ──────────────────────────────────────

/// `POST /v1/webhooks/gitlab` — GitLab webhook receiver.
///
/// No auth middleware, no CORS — called directly by GitLab.
/// Verifies X-Gitlab-Token, checks idempotency, ACKs fast.
pub async fn gitlab_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    // Body size check
    if body.len() > MAX_WEBHOOK_BODY_SIZE {
        return Err(api_err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Webhook payload exceeds size limit",
        ));
    }

    // Extract GitLab token
    let gitlab_token = headers
        .get("x-gitlab-token")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            warn!("GitLab webhook missing X-Gitlab-Token");
            api_err(
                StatusCode::UNAUTHORIZED,
                "missing_token",
                "X-Gitlab-Token header required",
            )
        })?;

    let event_uuid = headers
        .get("x-gitlab-event-uuid")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let event_type = headers
        .get("x-gitlab-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Parse payload
    let payload: serde_json::Value = serde_json::from_slice(&body).map_err(|e| {
        error!(error = %e, "failed to parse GitLab webhook payload");
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_payload",
            "Invalid JSON payload",
        )
    })?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    require_remote_mode(&pool).await?;

    let secrets =
        get_webhook_secrets(&pool, &state.encryption_key, WebhookProvider::Gitlab, false).await?;

    let mut matched_integration_id = secrets
        .iter()
        .find(|candidate| constant_time_eq(gitlab_token.as_bytes(), &candidate.secret))
        .map(|candidate| candidate.integration_id.clone());

    if matched_integration_id.is_none() {
        let refreshed =
            get_webhook_secrets(&pool, &state.encryption_key, WebhookProvider::Gitlab, true)
                .await?;

        if !Arc::ptr_eq(&secrets, &refreshed) {
            matched_integration_id = refreshed
                .iter()
                .find(|candidate| constant_time_eq(gitlab_token.as_bytes(), &candidate.secret))
                .map(|candidate| candidate.integration_id.clone());
        }
    }

    let integration_id = matched_integration_id.ok_or_else(|| {
        warn!("GitLab webhook token verification failed for all integrations");
        api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_token",
            "Webhook token verification failed",
        )
    })?;

    // Idempotency check
    if !event_uuid.is_empty() {
        let existing: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM integration_webhooks \
             WHERE integration_id = ?1 AND provider_delivery_id = ?2",
        )
        .bind(&integration_id)
        .bind(&event_uuid)
        .fetch_one(&pool)
        .await
        .unwrap_or(false);

        if existing {
            info!(event_uuid = %event_uuid, "duplicate GitLab webhook delivery, returning OK");
            return Ok(Json(serde_json::json!({ "ok": true, "duplicate": true })));
        }
    }

    let now = now_unix();

    // Replay window check — reject events with timestamps > 5 min old
    if let Some(timestamp) = payload.get("created_at").and_then(|v| v.as_str()) {
        if let Ok(event_time) = chrono::DateTime::parse_from_rfc3339(timestamp) {
            let event_unix = event_time.timestamp();
            if now - event_unix > MAX_WEBHOOK_AGE_SECS {
                warn!(
                    event_age = now - event_unix,
                    "rejecting stale GitLab webhook"
                );
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "stale_event",
                    "Webhook event is too old",
                ));
            }
        }
    }

    // Store webhook record
    let webhook_id = Uuid::new_v4().to_string();
    let delivery_id = if event_uuid.is_empty() {
        webhook_id.clone()
    } else {
        event_uuid.clone()
    };

    sqlx::query(
        "INSERT INTO integration_webhooks (id, integration_id, provider_delivery_id, event_type, payload, status, received_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'received', ?6)",
    )
    .bind(&webhook_id)
    .bind(&integration_id)
    .bind(&delivery_id)
    .bind(&event_type)
    .bind(payload.to_string())
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to store webhook record");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to store webhook")
    })?;

    // Normalize and process async
    let normalized = normalize_gitlab_event(&event_type, &delivery_id, &integration_id, &payload);

    let pool_clone = pool.clone();
    let webhook_id_clone = webhook_id.clone();
    tokio::spawn(async move {
        if let Err(e) = process_webhook_event(&pool_clone, &webhook_id_clone, &normalized).await {
            error!(error = ?e, webhook_id = %webhook_id_clone, "webhook processing failed");
        }
    });

    info!(
        event_uuid = %delivery_id,
        event_type = %event_type,
        integration_id = %integration_id,
        "GitLab webhook received"
    );

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Constant-time byte comparison.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn normalize_gitlab_event(
    event_type: &str,
    delivery_id: &str,
    integration_id: &str,
    payload: &serde_json::Value,
) -> NormalizedWebhookEvent {
    let repo_full_name = payload
        .get("project")
        .and_then(|p| p.get("path_with_namespace"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let (branch, commit_sha) = match event_type {
        "Push Hook" => {
            let git_ref = payload.get("ref").and_then(|v| v.as_str()).unwrap_or("");
            let branch = git_ref.strip_prefix("refs/heads/").map(String::from);
            let sha = payload
                .get("checkout_sha")
                .and_then(|v| v.as_str())
                .map(String::from);
            (branch, sha)
        }
        "Merge Request Hook" => {
            let branch = payload
                .get("object_attributes")
                .and_then(|oa| oa.get("source_branch"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let sha = payload
                .get("object_attributes")
                .and_then(|oa| oa.get("last_commit"))
                .and_then(|lc| lc.get("id"))
                .and_then(|v| v.as_str())
                .map(String::from);
            (branch, sha)
        }
        _ => (None, None),
    };

    let actor = payload
        .get("user")
        .and_then(|u| u.get("username"))
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("user_username").and_then(|v| v.as_str()))
        .map(String::from);

    NormalizedWebhookEvent {
        provider: "gitlab".to_string(),
        event_type: event_type.to_string(),
        delivery_id: delivery_id.to_string(),
        integration_id: integration_id.to_string(),
        repository_full_name: repo_full_name,
        branch,
        commit_sha,
        actor,
        payload: payload.clone(),
    }
}

// ── Webhook event processing ────────────────────────────────────

/// Process a normalized webhook event — trigger builds as appropriate.
///
/// This function is called from a tokio::spawn task.
async fn process_webhook_event(
    pool: &sqlx::SqlitePool,
    webhook_id: &str,
    event: &NormalizedWebhookEvent,
) -> anyhow::Result<()> {
    use crate::builds::trigger_build_from_webhook;

    // Only trigger builds for actionable events
    let should_trigger = matches!(
        event.event_type.as_str(),
        "push" | "pull_request" | "Push Hook" | "Merge Request Hook"
    );

    if should_trigger {
        if let Some(ref repo) = event.repository_full_name {
            match trigger_build_from_webhook(
                pool,
                webhook_id,
                &event.integration_id,
                repo,
                event.branch.as_deref(),
                event.commit_sha.as_deref(),
                &event.event_type,
                event.actor.as_deref(),
            )
            .await
            {
                Ok(builds) => {
                    info!(
                        webhook_id = %webhook_id,
                        builds_created = builds.len(),
                        "webhook triggered builds"
                    );
                }
                Err(e) => {
                    error!(error = ?e, webhook_id = %webhook_id, "failed to trigger builds from webhook");
                    let now = now_unix();
                    let _ = sqlx::query(
                        "UPDATE integration_webhooks SET status = 'failed', processing_error = ?1, processed_at = ?2 WHERE id = ?3",
                    )
                    .bind(format!("{e:?}"))
                    .bind(now)
                    .bind(webhook_id)
                    .execute(pool)
                    .await;
                    return Ok(());
                }
            }
        }
    }

    let now = now_unix();
    sqlx::query(
        "UPDATE integration_webhooks SET status = 'processed', processed_at = ?1 WHERE id = ?2",
    )
    .bind(now)
    .bind(webhook_id)
    .execute(pool)
    .await?;

    info!(
        webhook_id = %webhook_id,
        provider = %event.provider,
        event_type = %event.event_type,
        repo = ?event.repository_full_name,
        branch = ?event.branch,
        "webhook event processed"
    );

    Ok(())
}
