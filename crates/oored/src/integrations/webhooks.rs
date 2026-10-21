use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use oore_contract::{ApiError, WebhookResponse};
use ring::hmac;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio::sync::mpsc;
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
const WEBHOOK_WAKE_CAPACITY: usize = 1;
const WEBHOOK_RECOVERY_INTERVAL_SECS: u64 = 30;
const WEBHOOK_PROCESSING_LEASE_SECS: i64 = 5 * 60;
const MAX_WEBHOOK_PROCESSING_ATTEMPTS: i64 = 5;

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
    /// Tag name for tag pushes.
    pub tag: Option<String>,
    pub commit_sha: Option<String>,
    pub previous_commit_sha: Option<String>,
    /// Files changed by a push, when the payload lists all of its commits.
    pub changed_files: Option<Vec<String>>,
    /// Revision a pull request's changes are compared against.
    pub compare_base: Option<String>,
    pub actor: Option<String>,
    pub payload: serde_json::Value,
}

pub fn start_webhook_processor(
    pool: sqlx::SqlitePool,
    encryption_key: Vec<u8>,
) -> mpsc::Sender<()> {
    let (sender, mut receiver) = mpsc::channel::<()>(WEBHOOK_WAKE_CAPACITY);

    let worker_pool = pool.clone();
    tokio::spawn(async move {
        while receiver.recv().await.is_some() {
            loop {
                let webhook_id = match claim_next_webhook(&worker_pool).await {
                    Ok(Some(webhook_id)) => webhook_id,
                    Ok(None) => break,
                    Err(error) => {
                        error!(%error, "failed to claim webhook work");
                        break;
                    }
                };

                if let Err(error) =
                    process_claimed_webhook(&worker_pool, &encryption_key, &webhook_id).await
                {
                    error!(%error, %webhook_id, "webhook processing failed");
                    if let Err(store_error) =
                        record_webhook_processing_error(&worker_pool, &webhook_id, &error).await
                    {
                        error!(%store_error, %webhook_id, "failed to record webhook processing error");
                    }
                }
                tokio::task::yield_now().await;
            }
        }
    });

    let recovery_sender = sender.clone();
    tokio::spawn(async move {
        loop {
            match fail_exhausted_webhook_leases(&pool).await {
                Ok(0) => {}
                Ok(failed) => warn!(failed, "failed webhooks with expired processing leases"),
                Err(error) => error!(%error, "failed to expire webhook processing leases"),
            }
            if signal_webhook_processor(&recovery_sender).is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(WEBHOOK_RECOVERY_INTERVAL_SECS)).await;
        }
    });

    sender
}

fn signal_webhook_processor(sender: &mpsc::Sender<()>) -> Result<(), ()> {
    match sender.try_send(()) {
        Ok(()) | Err(mpsc::error::TrySendError::Full(())) => Ok(()),
        Err(mpsc::error::TrySendError::Closed(())) => Err(()),
    }
}

fn wake_webhook_processor(state: &AppState) -> Result<(), (StatusCode, Json<ApiError>)> {
    signal_webhook_processor(&state.webhook_wake).map_err(|()| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "webhook_queue_unavailable",
            "Webhook processing is unavailable",
        )
    })
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
) -> Result<Json<WebhookResponse>, (StatusCode, Json<ApiError>)> {
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
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM integration_webhooks \
             WHERE integration_id = ?1 AND provider_delivery_id = ?2",
        )
        .bind(&integration_id)
        .bind(&delivery_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| {
            error!(%error, "failed to check GitHub webhook idempotency");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to check webhook delivery",
            )
        })?;

        if existing.is_some() {
            wake_webhook_processor(&state)?;
            info!(delivery_id = %delivery_id, "duplicate GitHub webhook delivery, returning OK");
            return Ok(Json(WebhookResponse {
                ok: true,
                duplicate: Some(true),
            }));
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

    wake_webhook_processor(&state)?;

    info!(
        delivery_id = %delivery_id,
        event_type = %event_type,
        integration_id = %integration_id,
        "GitHub webhook received"
    );

    Ok(Json(WebhookResponse {
        ok: true,
        duplicate: None,
    }))
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

/// Files added, modified or removed across a push's commits. `None` when the
/// payload does not list every commit (`total_commits` above the list) or
/// lists none, as for a new branch pushed without new commits.
fn push_changed_files(
    payload: &serde_json::Value,
    total_commits: Option<u64>,
) -> Option<Vec<String>> {
    let commits = payload.get("commits")?.as_array()?;
    // GitHub lists at most 2048 commits and gives no total.
    if commits.is_empty()
        || commits.len() >= 2048
        || total_commits.is_some_and(|total| total > commits.len() as u64)
    {
        return None;
    }
    let mut files = std::collections::BTreeSet::new();
    for commit in commits {
        for key in ["added", "modified", "removed"] {
            for file in commit.get(key)?.as_array()? {
                files.insert(file.as_str()?.to_string());
            }
        }
    }
    Some(files.into_iter().collect())
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

    let git_ref = payload.get("ref").and_then(|v| v.as_str()).unwrap_or("");
    let tag = (event_type == "push")
        .then(|| git_ref.strip_prefix("refs/tags/").map(String::from))
        .flatten();
    let (branch, commit_sha) = match event_type {
        "push" => {
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
        tag,
        commit_sha,
        previous_commit_sha: None,
        changed_files: (event_type == "push")
            .then(|| push_changed_files(payload, None))
            .flatten(),
        compare_base: (event_type == "pull_request")
            .then(|| json_non_empty_string(payload.pointer("/pull_request/base/sha")))
            .flatten(),
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
) -> Result<Json<WebhookResponse>, (StatusCode, Json<ApiError>)> {
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
    matched.repository_full_name.ok_or_else(|| {
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
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM integration_webhooks \
         WHERE integration_id = ?1 AND provider_delivery_id = ?2",
    )
    .bind(&integration_id)
    .bind(&event_uuid)
    .fetch_optional(&pool)
    .await
    .map_err(|error| {
        error!(%error, "failed to check GitLab webhook idempotency");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to check webhook delivery",
        )
    })?;

    if existing.is_some() {
        wake_webhook_processor(&state)?;
        info!(event_uuid = %event_uuid, "duplicate GitLab webhook delivery, returning OK");
        return Ok(Json(WebhookResponse {
            ok: true,
            duplicate: Some(true),
        }));
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

    wake_webhook_processor(&state)?;

    info!(
        event_uuid = %delivery_id,
        event_type = %event_type,
        integration_id = %integration_id,
        repository_id = %repository_id,
        "GitLab webhook received"
    );

    Ok(Json(WebhookResponse {
        ok: true,
        duplicate: None,
    }))
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
    let git_ref = payload.get("ref").and_then(|v| v.as_str()).unwrap_or("");
    let tag = (event_type == "Tag Push Hook")
        .then(|| git_ref.strip_prefix("refs/tags/").map(String::from))
        .flatten();
    let (branch, commit_sha) = match event_type {
        "Push Hook" | "Tag Push Hook" => {
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
        tag,
        commit_sha,
        previous_commit_sha,
        changed_files: (event_type == "Push Hook")
            .then(|| {
                push_changed_files(
                    payload,
                    payload
                        .get("total_commits_count")
                        .and_then(serde_json::Value::as_u64),
                )
            })
            .flatten(),
        // Comparing from the target branch name diffs against the merge base.
        compare_base: (event_type == "Merge Request Hook")
            .then(|| json_non_empty_string(payload.pointer("/object_attributes/target_branch")))
            .flatten(),
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
        "push" | "Push Hook" | "Tag Push Hook" => {
            if event
                .payload
                .get("deleted")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                return Ok(WebhookTriggerDecision::Ignore(
                    "ref deletion does not introduce a revision",
                ));
            }
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

async fn claim_next_webhook(pool: &sqlx::SqlitePool) -> Result<Option<String>, sqlx::Error> {
    let now = now_unix();
    sqlx::query_scalar(
        "UPDATE integration_webhooks \
         SET processing_started_at = ?1, processing_attempts = processing_attempts + 1 \
         WHERE id = ( \
             SELECT id FROM integration_webhooks \
             WHERE status = 'received' \
               AND processing_attempts < ?2 \
               AND (next_attempt_at IS NULL OR next_attempt_at <= ?1) \
               AND (processing_started_at IS NULL OR processing_started_at < ?3) \
             ORDER BY received_at LIMIT 1 \
         ) \
         RETURNING id",
    )
    .bind(now)
    .bind(MAX_WEBHOOK_PROCESSING_ATTEMPTS)
    .bind(now - WEBHOOK_PROCESSING_LEASE_SECS)
    .fetch_optional(pool)
    .await
}

async fn fail_exhausted_webhook_leases(pool: &sqlx::SqlitePool) -> Result<u64, sqlx::Error> {
    let now = now_unix();
    let result = sqlx::query(
        "UPDATE integration_webhooks \
         SET status = 'failed', \
             processing_error = COALESCE(processing_error, 'processing lease expired'), \
             processing_started_at = NULL, next_attempt_at = NULL, processed_at = ?1 \
         WHERE status = 'received' \
           AND processing_attempts >= ?2 \
           AND processing_started_at < ?3",
    )
    .bind(now)
    .bind(MAX_WEBHOOK_PROCESSING_ATTEMPTS)
    .bind(now - WEBHOOK_PROCESSING_LEASE_SECS)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Load and process one claimed webhook event.
async fn process_claimed_webhook(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    webhook_id: &str,
) -> anyhow::Result<()> {
    let Some(row) = sqlx::query(
        "SELECT w.integration_id, w.provider_delivery_id, w.event_type, w.payload, i.provider \
         FROM integration_webhooks w \
         JOIN integrations i ON i.id = w.integration_id \
         WHERE w.id = ?1 AND w.status = 'received' AND w.processing_started_at IS NOT NULL",
    )
    .bind(webhook_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };

    let integration_id: String = row.get("integration_id");
    let delivery_id: String = row.get("provider_delivery_id");
    let event_type: String = row.get("event_type");
    let provider: String = row.get("provider");
    let payload_raw: String = row.get("payload");
    let payload: serde_json::Value = serde_json::from_str(&payload_raw)?;

    let event = match provider.as_str() {
        "github" => normalize_github_event(&event_type, &delivery_id, &integration_id, &payload),
        "gitlab" => {
            let repository_external_id = json_id(payload.pointer("/project/id"))
                .ok_or_else(|| anyhow::anyhow!("GitLab webhook has no project ID"))?;
            let repository_full_name: String = sqlx::query_scalar(
                "SELECT r.full_name FROM integration_repositories r \
                 JOIN integration_installations i ON i.id = r.installation_id \
                 WHERE i.integration_id = ?1 AND r.external_id = ?2",
            )
            .bind(&integration_id)
            .bind(&repository_external_id)
            .fetch_one(pool)
            .await?;
            normalize_gitlab_event(
                &event_type,
                &delivery_id,
                &integration_id,
                &repository_full_name,
                &payload,
            )
        }
        _ => return Err(anyhow::anyhow!("unsupported webhook provider: {provider}")),
    };

    process_webhook_event(pool, encryption_key, webhook_id, &event).await
}

async fn record_webhook_processing_error(
    pool: &sqlx::SqlitePool,
    webhook_id: &str,
    error: &anyhow::Error,
) -> Result<(), sqlx::Error> {
    let now = now_unix();
    sqlx::query(
        "UPDATE integration_webhooks \
         SET status = CASE \
                 WHEN processing_attempts >= ?1 THEN 'failed' \
                 ELSE 'received' \
             END, \
             processing_error = ?2, \
             processing_started_at = NULL, \
             next_attempt_at = CASE \
                 WHEN processing_attempts >= ?1 THEN NULL \
                 ELSE ?3 \
             END, \
             processed_at = CASE \
                 WHEN processing_attempts >= ?1 THEN ?4 \
                 ELSE NULL \
             END \
         WHERE id = ?5 AND status = 'received'",
    )
    .bind(MAX_WEBHOOK_PROCESSING_ATTEMPTS)
    .bind(format!("{error:#}"))
    .bind(now + WEBHOOK_RECOVERY_INTERVAL_SECS as i64)
    .bind(now)
    .bind(webhook_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn process_webhook_event(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    webhook_id: &str,
    event: &NormalizedWebhookEvent,
) -> anyhow::Result<()> {
    use crate::builds::{ChangeSet, WebhookBuildTrigger, trigger_build_from_webhook};

    let decision = webhook_trigger_decision(pool, event).await?;

    if let WebhookTriggerDecision::Ignore(reason) = decision {
        let now = now_unix();
        sqlx::query(
            "UPDATE integration_webhooks \
             SET status = 'ignored', processing_error = ?1, processing_started_at = NULL, \
                 next_attempt_at = NULL, processed_at = ?2 \
             WHERE id = ?3 AND status = 'received'",
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
        let changes = match (
            &event.changed_files,
            event.compare_base.as_deref(),
            event.commit_sha.as_deref(),
        ) {
            (Some(files), _, _) => ChangeSet::Files(files),
            (None, Some(base), Some(head)) => ChangeSet::Compare { base, head },
            _ => ChangeSet::Unknown,
        };
        match trigger_build_from_webhook(
            pool,
            encryption_key,
            &WebhookBuildTrigger {
                webhook_id,
                integration_id: &event.integration_id,
                repo_full_name: repo,
                event_type: &event.event_type,
                branch: event.branch.as_deref(),
                tag: event.tag.as_deref(),
                commit_sha: event.commit_sha.as_deref(),
                actor: event.actor.as_deref(),
                changes,
            },
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
                    "UPDATE integration_webhooks \
                     SET status = 'failed', processing_error = ?1, processing_started_at = NULL, \
                         next_attempt_at = NULL, processed_at = ?2 \
                     WHERE id = ?3 AND status = 'received'",
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
        "UPDATE integration_webhooks \
         SET status = 'processed', processing_started_at = NULL, next_attempt_at = NULL, processed_at = ?1 \
         WHERE id = ?2 AND status = 'received'",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_tag_push_carries_the_tag_and_no_branch() {
        let payload = serde_json::json!({
            "ref": "refs/tags/v1.4.0",
            "after": "abc123",
            "repository": { "full_name": "acme/app", "id": 7 },
            "commits": [],
        });
        let event = normalize_github_event("push", "d1", "i1", &payload);
        assert_eq!(event.tag.as_deref(), Some("v1.4.0"));
        assert_eq!(event.branch, None);
        assert_eq!(event.commit_sha.as_deref(), Some("abc123"));
        assert_eq!(event.changed_files, None);
    }

    #[test]
    fn push_changed_files_needs_the_full_commit_list() {
        let payload = serde_json::json!({
            "commits": [
                { "added": ["lib/a.dart"], "modified": ["pubspec.yaml"], "removed": [] },
                { "added": [], "modified": ["lib/a.dart"], "removed": ["old.txt"] },
            ],
        });
        assert_eq!(
            push_changed_files(&payload, None),
            Some(vec![
                "lib/a.dart".to_string(),
                "old.txt".to_string(),
                "pubspec.yaml".to_string(),
            ])
        );
        // GitLab lists 20 commits and reports the total.
        assert_eq!(push_changed_files(&payload, Some(21)), None);
        assert_eq!(
            push_changed_files(&serde_json::json!({ "commits": [] }), None),
            None
        );
    }

    #[test]
    fn gitlab_tag_push_and_merge_request_base() {
        let tag = normalize_gitlab_event(
            "Tag Push Hook",
            "d1",
            "i1",
            "acme/app",
            &serde_json::json!({ "ref": "refs/tags/v2", "checkout_sha": "def456", "project": { "id": 7 } }),
        );
        assert_eq!(tag.tag.as_deref(), Some("v2"));
        assert_eq!(tag.commit_sha.as_deref(), Some("def456"));

        let merge_request = normalize_gitlab_event(
            "Merge Request Hook",
            "d2",
            "i1",
            "acme/app",
            &serde_json::json!({
                "project": { "id": 7 },
                "object_attributes": {
                    "source_branch": "feature",
                    "target_branch": "main",
                    "last_commit": { "id": "fff" },
                },
            }),
        );
        assert_eq!(merge_request.compare_base.as_deref(), Some("main"));
        assert_eq!(merge_request.tag, None);
    }
}
