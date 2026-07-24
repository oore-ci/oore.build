use std::sync::{Arc, OnceLock};

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use oore_contract::ApiError;
use ring::hmac;
use serde::Serialize;
use sha2::{Digest, Sha256};
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
    repository_id: Option<String>,
    repository_external_id: Option<String>,
    repository_full_name: Option<String>,
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
pub(crate) enum WebhookProvider {
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

pub(crate) async fn invalidate_webhook_secret_cache(provider: WebhookProvider) {
    let mut cache = webhook_secret_cache().write().await;
    *cache.provider_mut(provider) = ProviderSecretCache::default();
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

    let query = match provider {
        WebhookProvider::Github => {
            "SELECT i.id AS integration_id, NULL AS repository_id, \
                    NULL AS repository_external_id, NULL AS repository_full_name, \
                    c.encrypted_value AS encrypted_secret \
             FROM integrations i \
             JOIN integration_credentials c ON c.integration_id = i.id \
             WHERE i.provider = 'github' AND i.status = 'active' \
             AND c.credential_type = 'webhook_secret'"
        }
        WebhookProvider::Gitlab => {
            "SELECT i.id AS integration_id, r.id AS repository_id, \
                    r.external_id AS repository_external_id, \
                    r.full_name AS repository_full_name, \
                    s.encrypted_secret \
             FROM integration_repository_webhook_secrets s \
             JOIN integration_repositories r ON r.id = s.repository_id \
             JOIN integration_installations inst ON inst.id = r.installation_id \
             JOIN integrations i ON i.id = inst.integration_id \
             WHERE i.provider = 'gitlab' AND i.status = 'active'"
        }
    };
    let rows = sqlx::query(query).fetch_all(pool).await.map_err(|e| {
        error!(error = %e, provider = provider.as_str(), "failed to fetch webhook integrations");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Internal error",
        )
    })?;

    let mut decrypted = Vec::with_capacity(rows.len());
    for row in rows {
        let integration_id: String = row.get("integration_id");
        let encrypted_secret: String = row.get("encrypted_secret");
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
            repository_id: row.get("repository_id"),
            repository_external_id: row.get("repository_external_id"),
            repository_full_name: row.get("repository_full_name"),
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
    pub repository_external_id: Option<String>,
    pub source_repository_external_id: Option<String>,
    pub target_repository_external_id: Option<String>,
    pub action: Option<String>,
    pub change_number: Option<String>,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub previous_commit_sha: Option<String>,
    pub actor: Option<String>,
    pub payload: serde_json::Value,
}

fn json_id(value: Option<&serde_json::Value>) -> Option<String> {
    let id = match value? {
        serde_json::Value::String(id) => id.clone(),
        serde_json::Value::Number(id) => id.to_string(),
        _ => return None,
    };
    let id = id.trim();
    (!id.is_empty()).then(|| id.to_string())
}

fn json_non_empty_string(value: Option<&serde_json::Value>) -> Option<String> {
    value?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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

    let pool = state.db.clone();
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
    let repository_external_id = json_id(payload.pointer("/repository/id"));

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
            let sha = json_non_empty_string(payload.pointer("/pull_request/head/sha"));
            (branch, sha)
        }
        _ => (None, None),
    };

    let (source_repository_external_id, target_repository_external_id, action, change_number) =
        if event_type == "pull_request" {
            (
                json_id(payload.pointer("/pull_request/head/repo/id")),
                json_id(payload.pointer("/pull_request/base/repo/id")),
                json_non_empty_string(payload.get("action")),
                json_id(payload.get("number")),
            )
        } else {
            (None, None, None, None)
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
        repository_external_id,
        source_repository_external_id,
        target_repository_external_id,
        action,
        change_number,
        branch,
        commit_sha,
        previous_commit_sha: None,
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

    let mut event_uuid = headers
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

    let pool = state.db.clone();
    require_remote_mode(&pool).await?;

    let payload_repository_id = payload
        .pointer("/project/id")
        .and_then(|value| {
            value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_i64().map(|id| id.to_string()))
                .or_else(|| value.as_u64().map(|id| id.to_string()))
        })
        .ok_or_else(|| {
            warn!("GitLab webhook payload is missing project.id");
            api_err(
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                "Webhook token verification failed",
            )
        })?;

    let secrets =
        get_webhook_secrets(&pool, &state.encryption_key, WebhookProvider::Gitlab, false).await?;

    let mut matched = secrets
        .iter()
        .find(|candidate| {
            candidate.repository_external_id.as_deref() == Some(payload_repository_id.as_str())
                && constant_time_eq(gitlab_token.as_bytes(), &candidate.secret)
        })
        .cloned();

    if matched.is_none() {
        let refreshed =
            get_webhook_secrets(&pool, &state.encryption_key, WebhookProvider::Gitlab, true)
                .await?;

        if !Arc::ptr_eq(&secrets, &refreshed) {
            matched = refreshed
                .iter()
                .find(|candidate| {
                    candidate.repository_external_id.as_deref()
                        == Some(payload_repository_id.as_str())
                        && constant_time_eq(gitlab_token.as_bytes(), &candidate.secret)
                })
                .cloned();
        }
    }

    let matched = matched.ok_or_else(|| {
        warn!("GitLab webhook token verification failed for all integrations");
        api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_token",
            "Webhook token verification failed",
        )
    })?;
    let integration_id = matched.integration_id;
    let repository_id = matched.repository_id.ok_or_else(|| {
        api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_token",
            "Webhook token verification failed",
        )
    })?;
    let repository_full_name = matched.repository_full_name.ok_or_else(|| {
        api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_token",
            "Webhook token verification failed",
        )
    })?;

    if event_uuid.is_empty() {
        let mut hasher = Sha256::new();
        hasher.update(integration_id.as_bytes());
        hasher.update([0]);
        hasher.update(event_type.as_bytes());
        hasher.update([0]);
        hasher.update(&body);
        event_uuid = format!("sha256:{}", hex::encode(hasher.finalize()));
    }

    // Idempotency check
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

    let now = now_unix();

    // Replay window check — reject events with timestamps > 5 min old
    if let Some(timestamp) = payload.get("created_at").and_then(|v| v.as_str())
        && let Ok(event_time) = chrono::DateTime::parse_from_rfc3339(timestamp)
    {
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

    // Store webhook record
    let webhook_id = Uuid::new_v4().to_string();
    let delivery_id = event_uuid.clone();

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
    let normalized = normalize_gitlab_event(
        &event_type,
        &delivery_id,
        &integration_id,
        &repository_full_name,
        &payload,
    );

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
        repository_id = %repository_id,
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
    repository_full_name: &str,
    payload: &serde_json::Value,
) -> NormalizedWebhookEvent {
    let repository_external_id = json_id(payload.pointer("/project/id"));
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
            let sha = json_non_empty_string(payload.pointer("/object_attributes/last_commit/id"));
            (branch, sha)
        }
        _ => (None, None),
    };

    let (
        source_repository_external_id,
        target_repository_external_id,
        action,
        change_number,
        previous_commit_sha,
    ) = if event_type == "Merge Request Hook" {
        (
            json_id(payload.pointer("/object_attributes/source_project_id")),
            json_id(payload.pointer("/object_attributes/target_project_id")),
            json_non_empty_string(payload.pointer("/object_attributes/action")),
            json_id(payload.pointer("/object_attributes/iid")),
            json_non_empty_string(payload.pointer("/object_attributes/oldrev")),
        )
    } else {
        (None, None, None, None, None)
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
        repository_full_name: Some(repository_full_name.to_string()),
        repository_external_id,
        source_repository_external_id,
        target_repository_external_id,
        action,
        change_number,
        branch,
        commit_sha,
        previous_commit_sha,
        actor,
        payload: payload.clone(),
    }
}

// ── Webhook event processing ────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebhookTriggerDecision {
    Trigger,
    Ignore(&'static str),
    NotActionable,
}

async fn registered_target_repository_matches(
    pool: &sqlx::SqlitePool,
    integration_id: &str,
    repository_external_id: &str,
    repository_full_name: &str,
) -> anyhow::Result<bool> {
    let matches: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 \
         FROM integration_repositories r \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         WHERE inst.integration_id = ?1 AND r.external_id = ?2 AND r.full_name = ?3",
    )
    .bind(integration_id)
    .bind(repository_external_id)
    .bind(repository_full_name)
    .fetch_one(pool)
    .await?;
    Ok(matches)
}

async fn webhook_trigger_decision(
    pool: &sqlx::SqlitePool,
    event: &NormalizedWebhookEvent,
) -> anyhow::Result<WebhookTriggerDecision> {
    match event.event_type.as_str() {
        "push" | "Push Hook" => {
            // Preserve existing push behavior when an immutable provider ID is absent,
            // while rejecting any provider ID that explicitly mismatches this integration.
            if let Some(repository_external_id) = event.repository_external_id.as_deref()
                && let Some(repository_full_name) = event.repository_full_name.as_deref()
                && !registered_target_repository_matches(
                    pool,
                    &event.integration_id,
                    repository_external_id,
                    repository_full_name,
                )
                .await?
            {
                return Ok(WebhookTriggerDecision::Ignore(
                    "target repository does not match the integration",
                ));
            }
            Ok(WebhookTriggerDecision::Trigger)
        }
        "pull_request" => {
            if !matches!(
                event.action.as_deref(),
                Some("opened" | "reopened" | "synchronize")
            ) {
                return Ok(WebhookTriggerDecision::Ignore(
                    "pull request action does not introduce a revision",
                ));
            }
            if event.change_number.is_none()
                || event.commit_sha.as_deref().is_none_or(str::is_empty)
            {
                return Ok(WebhookTriggerDecision::Ignore(
                    "pull request revision identity is incomplete",
                ));
            }

            let (Some(repository_id), Some(source_id), Some(target_id)) = (
                event.repository_external_id.as_deref(),
                event.source_repository_external_id.as_deref(),
                event.target_repository_external_id.as_deref(),
            ) else {
                return Ok(WebhookTriggerDecision::Ignore(
                    "pull request repository identity is incomplete",
                ));
            };
            if repository_id != target_id {
                return Ok(WebhookTriggerDecision::Ignore(
                    "pull request target repository is ambiguous",
                ));
            }
            if source_id != target_id {
                return Ok(WebhookTriggerDecision::Ignore(
                    "external-fork pull request revisions are blocked by policy",
                ));
            }
            let Some(repository_full_name) = event.repository_full_name.as_deref() else {
                return Ok(WebhookTriggerDecision::Ignore(
                    "pull request target repository name is missing",
                ));
            };
            if !registered_target_repository_matches(
                pool,
                &event.integration_id,
                target_id,
                repository_full_name,
            )
            .await?
            {
                return Ok(WebhookTriggerDecision::Ignore(
                    "target repository does not match the integration",
                ));
            }
            Ok(WebhookTriggerDecision::Trigger)
        }
        "Merge Request Hook" => {
            let revision_action = match event.action.as_deref() {
                Some("open" | "reopen") => true,
                Some("update") => matches!(
                    (
                        event.previous_commit_sha.as_deref(),
                        event.commit_sha.as_deref()
                    ),
                    (Some(previous), Some(current)) if !previous.is_empty()
                        && !current.is_empty()
                        && previous != current
                ),
                _ => false,
            };
            if !revision_action {
                return Ok(WebhookTriggerDecision::Ignore(
                    "merge request action does not prove a new revision",
                ));
            }
            if event.change_number.is_none()
                || event.commit_sha.as_deref().is_none_or(str::is_empty)
            {
                return Ok(WebhookTriggerDecision::Ignore(
                    "merge request revision identity is incomplete",
                ));
            }

            let (Some(repository_id), Some(source_id), Some(target_id)) = (
                event.repository_external_id.as_deref(),
                event.source_repository_external_id.as_deref(),
                event.target_repository_external_id.as_deref(),
            ) else {
                return Ok(WebhookTriggerDecision::Ignore(
                    "merge request repository identity is incomplete",
                ));
            };
            if repository_id != target_id {
                return Ok(WebhookTriggerDecision::Ignore(
                    "merge request target repository is ambiguous",
                ));
            }
            if source_id != target_id {
                return Ok(WebhookTriggerDecision::Ignore(
                    "external-fork merge request revisions are blocked by policy",
                ));
            }
            let Some(repository_full_name) = event.repository_full_name.as_deref() else {
                return Ok(WebhookTriggerDecision::Ignore(
                    "merge request target repository name is missing",
                ));
            };
            if !registered_target_repository_matches(
                pool,
                &event.integration_id,
                target_id,
                repository_full_name,
            )
            .await?
            {
                return Ok(WebhookTriggerDecision::Ignore(
                    "target repository does not match the integration",
                ));
            }
            Ok(WebhookTriggerDecision::Trigger)
        }
        _ => Ok(WebhookTriggerDecision::NotActionable),
    }
}

/// Process a normalized webhook event — trigger builds as appropriate.
///
/// This function is called from a tokio::spawn task.
async fn process_webhook_event(
    pool: &sqlx::SqlitePool,
    webhook_id: &str,
    event: &NormalizedWebhookEvent,
) -> anyhow::Result<()> {
    use crate::builds::trigger_build_from_webhook;

    let decision = webhook_trigger_decision(pool, event).await?;

    if let WebhookTriggerDecision::Ignore(reason) = decision {
        let now = now_unix();
        sqlx::query(
            "UPDATE integration_webhooks \
             SET status = 'ignored', processing_error = ?1, processed_at = ?2 \
             WHERE id = ?3",
        )
        .bind(reason)
        .bind(now)
        .bind(webhook_id)
        .execute(pool)
        .await?;
        info!(
            webhook_id = %webhook_id,
            provider = %event.provider,
            event_type = %event.event_type,
            reason,
            "webhook event ignored by revision trust policy"
        );
        return Ok(());
    }

    if decision == WebhookTriggerDecision::Trigger
        && let Some(repo) = event.repository_full_name.as_deref()
    {
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
