//! Background notification dispatch worker.
//!
//! Subscribes to `BuildStateEvent` broadcast channel and dispatches notifications
//! to configured channels when builds reach terminal states.

use std::sync::Arc;
use std::time::Duration;

use oore_contract::NotificationChannelType;
use ring::hmac;
use sqlx::{Row, SqlitePool};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::crypto;
use crate::scheduler::{BuildStateEvent, Scheduler};
use crate::util::now_unix;

/// Terminal build statuses that trigger notifications.
const TERMINAL_STATUSES: &[&str] = &["succeeded", "failed", "canceled", "timed_out", "expired"];

/// Start the notification dispatch background task.
pub fn start_notification_dispatcher(
    pool: SqlitePool,
    scheduler: Arc<Scheduler>,
    encryption_key: Vec<u8>,
) {
    tokio::spawn(async move {
        let mut rx = scheduler.subscribe_events();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        loop {
            match rx.recv().await {
                Ok(event) => {
                    if !TERMINAL_STATUSES.contains(&event.to_status.as_str()) {
                        continue;
                    }
                    if let Err(e) = dispatch_event(&pool, &client, &encryption_key, &event).await {
                        error!(error = %e, build_id = %event.build_id, "notification dispatch error");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(
                        missed = n,
                        "notification dispatcher lagged behind event bus"
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    info!("notification event bus closed, stopping dispatcher");
                    break;
                }
            }
        }
    });
}

/// Dispatch notifications for a single build state event.
async fn dispatch_event(
    pool: &SqlitePool,
    client: &reqwest::Client,
    encryption_key: &[u8],
    event: &BuildStateEvent,
) -> anyhow::Result<()> {
    // Fetch all enabled notification channels
    let channels = sqlx::query("SELECT * FROM notification_channels WHERE enabled = 1")
        .fetch_all(pool)
        .await?;

    if channels.is_empty() {
        return Ok(());
    }

    // Fetch build details for the notification payload
    let build_row = sqlx::query(
        "SELECT b.*, p.name as project_name, pl.name as pipeline_name \
         FROM builds b \
         LEFT JOIN projects p ON b.project_id = p.id \
         LEFT JOIN pipelines pl ON b.pipeline_id = pl.id \
         WHERE b.id = ?1",
    )
    .bind(&event.build_id)
    .fetch_optional(pool)
    .await?;

    let build_row = match build_row {
        Some(row) => row,
        None => {
            warn!(build_id = %event.build_id, "build not found for notification dispatch");
            return Ok(());
        }
    };

    let payload = build_notification_payload(&build_row, event);

    for channel_row in &channels {
        let channel_id: String = channel_row.get("id");
        let channel_type_str: String = channel_row.get("channel_type");
        let channel_name: String = channel_row.get("name");

        // Check event filter
        let event_filter_json: Option<String> = channel_row.get("event_filter_json");
        if let Some(ref filter_json) = event_filter_json {
            let filters: Vec<String> = serde_json::from_str(filter_json).unwrap_or_default();
            if !filters.is_empty() && !filters.contains(&event.to_status) {
                continue;
            }
        }

        let encrypted_url: Option<String> = channel_row.get("encrypted_url");
        let encrypted_secret: Option<String> = channel_row.get("encrypted_secret");

        let url = match encrypted_url
            .as_deref()
            .map(|eu| crypto::decrypt(eu, encryption_key))
            .transpose()
        {
            Ok(Some(u)) => u,
            Ok(None) => {
                warn!(channel_id = %channel_id, "channel has no URL, skipping");
                continue;
            }
            Err(e) => {
                error!(channel_id = %channel_id, error = %e, "failed to decrypt channel URL");
                continue;
            }
        };

        let secret = match encrypted_secret
            .as_deref()
            .map(|es| crypto::decrypt(es, encryption_key))
            .transpose()
        {
            Ok(s) => s,
            Err(e) => {
                error!(channel_id = %channel_id, error = %e, "failed to decrypt HMAC secret, skipping channel");
                continue;
            }
        };

        let channel_type = channel_type_str
            .parse::<NotificationChannelType>()
            .unwrap_or(NotificationChannelType::Webhook);

        // Insert pending delivery record
        let delivery_id = Uuid::new_v4().to_string();
        let now = now_unix();
        let _ = sqlx::query(
            "INSERT INTO notification_deliveries \
             (id, channel_id, build_id, event_type, status, attempt_count, created_at) \
             VALUES (?1, ?2, ?3, ?4, 'pending', 0, ?5)",
        )
        .bind(&delivery_id)
        .bind(&channel_id)
        .bind(&event.build_id)
        .bind(&event.to_status)
        .bind(now)
        .execute(pool)
        .await;

        // Attempt delivery
        let result =
            send_notification_with_client(client, &url, secret.as_deref(), channel_type, &payload)
                .await;

        match result {
            Ok(()) => {
                let delivered_at = now_unix();
                let _ = sqlx::query(
                    "UPDATE notification_deliveries \
                     SET status = 'delivered', attempt_count = 1, delivered_at = ?1 \
                     WHERE id = ?2",
                )
                .bind(delivered_at)
                .bind(&delivery_id)
                .execute(pool)
                .await;
                info!(
                    channel_id = %channel_id,
                    channel_name = %channel_name,
                    build_id = %event.build_id,
                    status = %event.to_status,
                    "notification delivered"
                );
            }
            Err(e) => {
                let error_msg = e.to_string();
                let _ = sqlx::query(
                    "UPDATE notification_deliveries \
                     SET status = 'failed', attempt_count = 1, last_error = ?1 \
                     WHERE id = ?2",
                )
                .bind(&error_msg)
                .bind(&delivery_id)
                .execute(pool)
                .await;
                warn!(
                    channel_id = %channel_id,
                    channel_name = %channel_name,
                    build_id = %event.build_id,
                    error = %error_msg,
                    "notification delivery failed"
                );
            }
        }
    }

    Ok(())
}

/// Build the JSON notification payload from a build row.
fn build_notification_payload(
    row: &sqlx::sqlite::SqliteRow,
    event: &BuildStateEvent,
) -> serde_json::Value {
    let project_name: Option<String> = row.try_get("project_name").ok().flatten();
    let pipeline_name: Option<String> = row.try_get("pipeline_name").ok().flatten();

    serde_json::json!({
        "event": format!("build.{}", event.to_status),
        "build": {
            "id": event.build_id,
            "project_id": row.try_get::<String, _>("project_id").unwrap_or_default(),
            "pipeline_id": row.try_get::<String, _>("pipeline_id").unwrap_or_default(),
            "build_number": row.try_get::<i64, _>("build_number").unwrap_or(0),
            "status": event.to_status,
            "branch": row.try_get::<Option<String>, _>("branch").ok().flatten(),
            "commit_sha": row.try_get::<Option<String>, _>("commit_sha").ok().flatten(),
            "trigger_type": row.try_get::<String, _>("trigger_type").unwrap_or_default(),
            "started_at": row.try_get::<Option<i64>, _>("started_at").ok().flatten(),
            "finished_at": row.try_get::<Option<i64>, _>("finished_at").ok().flatten(),
        },
        "project_name": project_name,
        "pipeline_name": pipeline_name,
        "timestamp": event.timestamp,
    })
}

/// Build a Mattermost/Slack-compatible message from the payload.
fn build_mattermost_payload(payload: &serde_json::Value) -> serde_json::Value {
    let build_number = payload["build"]["build_number"].as_i64().unwrap_or(0);
    let project_name = payload["project_name"]
        .as_str()
        .unwrap_or("Unknown Project");
    let pipeline_name = payload["pipeline_name"]
        .as_str()
        .unwrap_or("Unknown Pipeline");
    let status = payload["build"]["status"].as_str().unwrap_or("unknown");
    let branch = payload["build"]["branch"].as_str().unwrap_or("—");

    let emoji = match status {
        "succeeded" => ":white_check_mark:",
        "failed" => ":x:",
        "canceled" => ":no_entry_sign:",
        "timed_out" => ":hourglass:",
        "expired" => ":clock1:",
        _ => ":bell:",
    };

    let text = format!(
        "{emoji} **{project_name}** / {pipeline_name} — Build #{build_number} **{status}**\nBranch: `{branch}`"
    );

    serde_json::json!({
        "text": text,
        "username": "Oore CI",
        "icon_url": "https://static.oore.build/logo-avatar-192.png",
    })
}

/// Generate a test notification payload.
pub fn test_payload() -> serde_json::Value {
    serde_json::json!({
        "event": "build.test",
        "build": {
            "id": "test-notification",
            "project_id": "test-project",
            "pipeline_id": "test-pipeline",
            "build_number": 0,
            "status": "succeeded",
            "branch": "main",
            "commit_sha": null,
            "trigger_type": "manual",
            "started_at": null,
            "finished_at": null,
        },
        "project_name": "Test Project",
        "pipeline_name": "Test Pipeline",
        "timestamp": now_unix(),
    })
}

/// Send a notification to a single channel (used by both dispatch and test endpoint).
pub async fn send_notification(
    url: &str,
    secret: Option<&str>,
    channel_type: NotificationChannelType,
    payload: &serde_json::Value,
) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    send_notification_with_client(&client, url, secret, channel_type, payload).await
}

/// Send a notification using a provided reqwest client.
async fn send_notification_with_client(
    client: &reqwest::Client,
    url: &str,
    secret: Option<&str>,
    channel_type: NotificationChannelType,
    payload: &serde_json::Value,
) -> anyhow::Result<()> {
    let body = match channel_type {
        NotificationChannelType::Webhook => payload.clone(),
        NotificationChannelType::Mattermost => build_mattermost_payload(payload),
    };

    let body_bytes = serde_json::to_vec(&body)?;

    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "oore-ci/1.0");

    // Add HMAC signature for webhook channels
    if channel_type == NotificationChannelType::Webhook
        && let Some(secret) = secret
    {
        let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
        let signature = hmac::sign(&key, &body_bytes);
        let sig_hex = hex::encode(signature.as_ref());
        request = request.header("X-Oore-Signature", format!("sha256={sig_hex}"));
    }

    let response = request.body(body_bytes).send().await?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "notification delivery failed: HTTP {} — {}",
            status,
            body_text.chars().take(200).collect::<String>()
        );
    }

    Ok(())
}
