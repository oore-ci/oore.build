//! Background notification dispatch worker.
//!
//! Subscribes to `BuildStateEvent` and `RunnerStateEvent` broadcast channels and
//! dispatches notifications to configured channels when builds reach terminal
//! states or runners go offline.

use std::sync::Arc;
use std::time::Duration;

use lettre::message::{Mailbox, header::ContentType};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use oore_contract::{NotificationChannelType, SmtpConfig, SmtpTlsMode};
use ring::hmac;
use sqlx::{Row, SqlitePool};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::crypto;
use crate::scheduler::{BuildStateEvent, RunnerStateEvent, Scheduler};
use crate::util::now_unix;

/// Terminal build statuses that trigger notifications.
const TERMINAL_STATUSES: &[&str] = &["succeeded", "failed", "canceled", "timed_out", "expired"];

/// Start the notification dispatch background task.
pub fn start_notification_dispatcher(
    pool: SqlitePool,
    scheduler: Arc<Scheduler>,
    encryption_key: Vec<u8>,
) {
    // Build event dispatcher
    let build_pool = pool.clone();
    let build_key = encryption_key.clone();
    let mut build_rx = scheduler.subscribe_events();
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        loop {
            match build_rx.recv().await {
                Ok(event) => {
                    if !TERMINAL_STATUSES.contains(&event.to_status.as_str()) {
                        continue;
                    }
                    if let Err(e) = dispatch_event(&build_pool, &client, &build_key, &event).await {
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

    // Runner event dispatcher
    let mut runner_rx = scheduler.subscribe_runner_events();
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        loop {
            match runner_rx.recv().await {
                Ok(event) => {
                    if event.to_status != "offline" {
                        continue;
                    }
                    if let Err(e) =
                        dispatch_runner_event(&pool, &client, &encryption_key, &event).await
                    {
                        error!(
                            error = %e,
                            runner_id = %event.runner_id,
                            "runner notification dispatch error"
                        );
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(
                        missed = n,
                        "runner notification dispatcher lagged behind event bus"
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    info!("runner event bus closed, stopping dispatcher");
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

        // Attempt delivery — branch by channel type
        let result = match channel_type {
            NotificationChannelType::Email => {
                dispatch_email(channel_row, encryption_key, &channel_id, || {
                    let p = &payload;
                    build_email_html(p)
                })
                .await
            }
            _ => {
                let effective_payload = match channel_type {
                    NotificationChannelType::Mattermost => build_mattermost_payload(&payload),
                    _ => payload.clone(),
                };
                dispatch_http(
                    client,
                    channel_row,
                    encryption_key,
                    &channel_id,
                    channel_type,
                    &effective_payload,
                )
                .await
            }
        };

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

/// Dispatch notifications for a runner state event (e.g. runner going offline).
async fn dispatch_runner_event(
    pool: &SqlitePool,
    client: &reqwest::Client,
    encryption_key: &[u8],
    event: &RunnerStateEvent,
) -> anyhow::Result<()> {
    let channels = sqlx::query("SELECT * FROM notification_channels WHERE enabled = 1")
        .fetch_all(pool)
        .await?;

    if channels.is_empty() {
        return Ok(());
    }

    let payload = runner_notification_payload(event);
    let event_type = "runner_offline";

    for channel_row in &channels {
        let channel_id: String = channel_row.get("id");
        let channel_type_str: String = channel_row.get("channel_type");
        let channel_name: String = channel_row.get("name");

        // Check event filter — runner_offline must be in the filter list (or filter must be empty)
        let event_filter_json: Option<String> = channel_row.get("event_filter_json");
        if let Some(ref filter_json) = event_filter_json {
            let filters: Vec<String> = serde_json::from_str(filter_json).unwrap_or_default();
            if !filters.is_empty() && !filters.contains(&event_type.to_string()) {
                continue;
            }
        }

        let channel_type = channel_type_str
            .parse::<NotificationChannelType>()
            .unwrap_or(NotificationChannelType::Webhook);

        // Insert delivery record (build_id is NULL for runner events)
        let delivery_id = Uuid::new_v4().to_string();
        let now = now_unix();
        let _ = sqlx::query(
            "INSERT INTO notification_deliveries \
             (id, channel_id, event_type, status, attempt_count, created_at, runner_id, event_category) \
             VALUES (?1, ?2, ?3, 'pending', 0, ?4, ?5, 'runner')",
        )
        .bind(&delivery_id)
        .bind(&channel_id)
        .bind(event_type)
        .bind(now)
        .bind(&event.runner_id)
        .execute(pool)
        .await;

        // Attempt delivery — branch by channel type
        let result = match channel_type {
            NotificationChannelType::Email => {
                dispatch_email(channel_row, encryption_key, &channel_id, || {
                    let p = &payload;
                    runner_email_html(p)
                })
                .await
            }
            _ => {
                let effective_payload = match channel_type {
                    NotificationChannelType::Mattermost => runner_mattermost_payload(&payload),
                    _ => payload.clone(),
                };
                dispatch_http(
                    client,
                    channel_row,
                    encryption_key,
                    &channel_id,
                    channel_type,
                    &effective_payload,
                )
                .await
            }
        };

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
                    runner_id = %event.runner_id,
                    "runner offline notification delivered"
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
                    runner_id = %event.runner_id,
                    error = %error_msg,
                    "runner offline notification delivery failed"
                );
            }
        }
    }

    Ok(())
}

/// Build the JSON notification payload for a runner state event.
fn runner_notification_payload(event: &RunnerStateEvent) -> serde_json::Value {
    serde_json::json!({
        "event": format!("runner.{}", event.to_status),
        "runner": {
            "id": event.runner_id,
            "name": event.runner_name,
            "from_status": event.from_status,
            "to_status": event.to_status,
        },
        "timestamp": event.timestamp,
    })
}

/// Build a Mattermost/Slack-compatible message for a runner event.
fn runner_mattermost_payload(payload: &serde_json::Value) -> serde_json::Value {
    let runner_name = payload["runner"]["name"].as_str().unwrap_or("Unknown");
    let from_status = payload["runner"]["from_status"]
        .as_str()
        .unwrap_or("unknown");

    let text = format!(":warning: Runner **{runner_name}** went offline (was {from_status})");

    serde_json::json!({
        "text": text,
        "username": "Oore CI",
        "icon_url": "https://static.oore.build/logo-avatar-192.png",
    })
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

/// Send a build notification using a provided reqwest client.
///
/// Transforms the payload for Mattermost channels automatically.
async fn send_notification_with_client(
    client: &reqwest::Client,
    url: &str,
    secret: Option<&str>,
    channel_type: NotificationChannelType,
    payload: &serde_json::Value,
) -> anyhow::Result<()> {
    let body = match channel_type {
        NotificationChannelType::Webhook | NotificationChannelType::Email => payload.clone(),
        NotificationChannelType::Mattermost => build_mattermost_payload(payload),
    };
    send_raw_http_payload(client, url, secret, channel_type, &body).await
}

/// Dispatch an HTTP-based notification (webhook/mattermost) from within a dispatch loop.
async fn dispatch_http(
    client: &reqwest::Client,
    channel_row: &sqlx::sqlite::SqliteRow,
    encryption_key: &[u8],
    channel_id: &str,
    channel_type: NotificationChannelType,
    payload: &serde_json::Value,
) -> anyhow::Result<()> {
    let encrypted_url: Option<String> = channel_row.get("encrypted_url");
    let encrypted_secret: Option<String> = channel_row.get("encrypted_secret");

    let url = match encrypted_url
        .as_deref()
        .map(|eu| crypto::decrypt(eu, encryption_key))
        .transpose()
    {
        Ok(Some(u)) => u,
        Ok(None) => {
            anyhow::bail!("channel {channel_id} has no URL configured");
        }
        Err(e) => {
            anyhow::bail!("failed to decrypt channel {channel_id} URL: {e}");
        }
    };

    let secret = match encrypted_secret
        .as_deref()
        .map(|es| crypto::decrypt(es, encryption_key))
        .transpose()
    {
        Ok(s) => s,
        Err(e) => {
            anyhow::bail!("failed to decrypt channel {channel_id} HMAC secret: {e}");
        }
    };

    send_raw_http_payload(client, &url, secret.as_deref(), channel_type, payload).await
}

/// Dispatch an email notification from within a dispatch loop.
async fn dispatch_email<F>(
    channel_row: &sqlx::sqlite::SqliteRow,
    encryption_key: &[u8],
    channel_id: &str,
    build_html: F,
) -> anyhow::Result<()>
where
    F: FnOnce() -> (String, String),
{
    let encrypted_config: Option<String> = channel_row.get("encrypted_config");
    let config_json = match encrypted_config
        .as_deref()
        .map(|ec| crypto::decrypt(ec, encryption_key))
        .transpose()
    {
        Ok(Some(j)) => j,
        Ok(None) => {
            anyhow::bail!("email channel {channel_id} has no SMTP config");
        }
        Err(e) => {
            anyhow::bail!("failed to decrypt email channel {channel_id} config: {e}");
        }
    };

    let smtp_config: SmtpConfig = serde_json::from_str(&config_json)?;
    let (subject, html_body) = build_html();
    send_email_notification(&smtp_config, &subject, &html_body).await
}

/// Send a pre-formatted payload to an HTTP notification channel.
async fn send_raw_http_payload(
    client: &reqwest::Client,
    url: &str,
    secret: Option<&str>,
    channel_type: NotificationChannelType,
    body: &serde_json::Value,
) -> anyhow::Result<()> {
    let body_bytes = serde_json::to_vec(body)?;

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

// ── Email notification functions ─────────────────────────────────

/// Send an email notification via SMTP.
async fn send_email_notification(
    config: &SmtpConfig,
    subject: &str,
    html_body: &str,
) -> anyhow::Result<()> {
    let from: Mailbox = config
        .from_address
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid from address '{}': {e}", config.from_address))?;

    let creds = Credentials::new(config.username.clone(), config.password.clone());

    let transport = match config.tls_mode {
        SmtpTlsMode::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)?
            .port(config.port)
            .credentials(creds)
            .build(),
        SmtpTlsMode::StartTls => {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)?
                .port(config.port)
                .credentials(creds)
                .build()
        }
        SmtpTlsMode::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
            .port(config.port)
            .credentials(creds)
            .build(),
    };

    for recipient in &config.recipients {
        let to: Mailbox = recipient
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid recipient '{recipient}': {e}"))?;

        let email = Message::builder()
            .from(from.clone())
            .to(to)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(html_body.to_string())?;

        tokio::time::timeout(Duration::from_secs(30), transport.send(email))
            .await
            .map_err(|_| anyhow::anyhow!("SMTP send timed out after 30s"))??;
    }

    Ok(())
}

/// Send a test email to verify SMTP configuration.
pub async fn send_test_email(config: &SmtpConfig) -> anyhow::Result<()> {
    send_email_notification(config, "Oore CI — Test Notification", &test_email_html()).await
}

fn test_email_html() -> String {
    format!(
        r#"<!DOCTYPE html>
<html><body style="font-family: -apple-system, BlinkMacSystemFont, sans-serif; max-width: 600px; margin: 0 auto;">
<div style="background: #16a34a; color: white; padding: 16px 24px; border-radius: 8px 8px 0 0;">
  <h2 style="margin: 0;">Oore CI — Test Notification</h2>
</div>
<div style="padding: 24px; border: 1px solid #e5e7eb; border-top: none; border-radius: 0 0 8px 8px;">
  <p>This is a test notification from Oore CI. If you received this email, your SMTP configuration is working correctly.</p>
  <p style="color: #6b7280; font-size: 14px;">Sent at {}</p>
</div>
</body></html>"#,
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    )
}

/// Escape HTML special characters to prevent injection in email templates.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn build_email_html(payload: &serde_json::Value) -> (String, String) {
    let project_name = html_escape(
        payload["project_name"]
            .as_str()
            .unwrap_or("Unknown Project"),
    );
    let pipeline_name = html_escape(
        payload["pipeline_name"]
            .as_str()
            .unwrap_or("Unknown Pipeline"),
    );
    let build_number = payload["build"]["build_number"].as_i64().unwrap_or(0);
    let status_raw = payload["build"]["status"].as_str().unwrap_or("unknown");
    let branch = html_escape(payload["build"]["branch"].as_str().unwrap_or("—"));

    let (color, emoji) = match status_raw {
        "succeeded" => ("#16a34a", "\u{2705}"),
        "failed" => ("#dc2626", "\u{274C}"),
        "canceled" => ("#6b7280", "\u{1F6AB}"),
        "timed_out" => ("#d97706", "\u{23F3}"),
        "expired" => ("#9333ea", "\u{1F552}"),
        _ => ("#6b7280", "\u{1F514}"),
    };

    let status = html_escape(status_raw);
    let project_name_raw = payload["project_name"]
        .as_str()
        .unwrap_or("Unknown Project");
    let pipeline_name_raw = payload["pipeline_name"]
        .as_str()
        .unwrap_or("Unknown Pipeline");

    let subject = format!(
        "{} {} / {} — Build #{} {}",
        emoji, project_name_raw, pipeline_name_raw, build_number, status_raw
    );

    let html = format!(
        r#"<!DOCTYPE html>
<html><body style="font-family: -apple-system, BlinkMacSystemFont, sans-serif; max-width: 600px; margin: 0 auto;">
<div style="background: {color}; color: white; padding: 16px 24px; border-radius: 8px 8px 0 0;">
  <h2 style="margin: 0;">Build #{build_number} — {status}</h2>
</div>
<div style="padding: 24px; border: 1px solid #e5e7eb; border-top: none; border-radius: 0 0 8px 8px;">
  <table style="width: 100%; border-collapse: collapse;">
    <tr><td style="padding: 8px 0; color: #6b7280;">Project</td><td style="padding: 8px 0; font-weight: 600;">{project_name}</td></tr>
    <tr><td style="padding: 8px 0; color: #6b7280;">Pipeline</td><td style="padding: 8px 0; font-weight: 600;">{pipeline_name}</td></tr>
    <tr><td style="padding: 8px 0; color: #6b7280;">Branch</td><td style="padding: 8px 0;"><code>{branch}</code></td></tr>
    <tr><td style="padding: 8px 0; color: #6b7280;">Status</td><td style="padding: 8px 0; font-weight: 600; color: {color};">{status}</td></tr>
  </table>
</div>
<p style="color: #9ca3af; font-size: 12px; text-align: center; margin-top: 16px;">Sent by Oore CI</p>
</body></html>"#
    );

    (subject, html)
}

fn runner_email_html(payload: &serde_json::Value) -> (String, String) {
    let runner_name_raw = payload["runner"]["name"].as_str().unwrap_or("Unknown");
    let from_status_raw = payload["runner"]["from_status"]
        .as_str()
        .unwrap_or("unknown");
    let runner_name = html_escape(runner_name_raw);
    let from_status = html_escape(from_status_raw);

    let subject = format!("\u{26A0}\u{FE0F} Runner {} went offline", runner_name_raw);

    let html = format!(
        r#"<!DOCTYPE html>
<html><body style="font-family: -apple-system, BlinkMacSystemFont, sans-serif; max-width: 600px; margin: 0 auto;">
<div style="background: #d97706; color: white; padding: 16px 24px; border-radius: 8px 8px 0 0;">
  <h2 style="margin: 0;">⚠️ Runner Offline</h2>
</div>
<div style="padding: 24px; border: 1px solid #e5e7eb; border-top: none; border-radius: 0 0 8px 8px;">
  <p>Runner <strong>{runner_name}</strong> went offline (was <em>{from_status}</em>).</p>
  <p>Please check the runner status in the Oore CI dashboard.</p>
</div>
<p style="color: #9ca3af; font-size: 12px; text-align: center; margin-top: 16px;">Sent by Oore CI</p>
</body></html>"#
    );

    (subject, html)
}
