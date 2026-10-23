//! Background notification dispatch worker.
//!
//! Subscribes to the build, runner and release broadcast channels and
//! dispatches notifications to configured channels when builds reach terminal
//! states, runners go offline, or releases and rollouts change state.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use ipnet::IpNet;
use lettre::message::{Mailbox, header::ContentType};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use oore_contract::{NotificationChannelType, SlackMention, SmtpConfig, SmtpTlsMode};
use ring::hmac;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::crypto;
use crate::instance_settings::load_effective_external_access_network_settings;
use crate::scheduler::{
    BuildStateEvent, ReleaseEvent, ReleaseEventKind, RunnerStateEvent, Scheduler,
};
use crate::util::now_unix;

/// Terminal build statuses that trigger notifications.
const TERMINAL_STATUSES: &[&str] = &["succeeded", "failed", "canceled", "timed_out", "expired"];
const MAX_NOTIFICATION_ERROR_BODY_BYTES: usize = 1024;
/// How often finished builds are checked for notifications. A published
/// build event checks at once.
const BUILD_SCAN_INTERVAL: Duration = Duration::from_secs(2);
const BUILD_SCAN_BATCH_SIZE: i64 = 50;
/// How often pending deliveries are checked for a due retry.
const RETRY_SCAN_INTERVAL: Duration = Duration::from_secs(10);
const RETRY_BATCH_SIZE: i64 = 50;
/// Attempts per delivery before it is marked failed. Retries wait 30s, 1m,
/// 2m and 4m.
const MAX_DELIVERY_ATTEMPTS: i64 = 5;
const RETRY_BASE_SECS: i64 = 30;
/// Longer than the 30s send timeout plus the Slack mention lookup.
const DELIVERY_LEASE_SECS: i64 = 120;
/// Sends in flight across every channel.
const MAX_CONCURRENT_SENDS: usize = 8;
/// Keeps a slow Slack lookup from delaying the message itself.
const SLACK_LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

fn notification_addresses_allowed(host: &str, addresses: &[std::net::SocketAddr]) -> bool {
    !addresses.is_empty()
        && if host.eq_ignore_ascii_case("localhost") {
            addresses.iter().all(|address| address.ip().is_loopback())
        } else {
            addresses
                .iter()
                .all(|address| is_public_notification_ip(address.ip()))
        }
}

fn is_public_notification_ip(ip: std::net::IpAddr) -> bool {
    static NON_PUBLIC_NETWORKS: OnceLock<Vec<IpNet>> = OnceLock::new();
    let networks = NON_PUBLIC_NETWORKS.get_or_init(|| {
        [
            "0.0.0.0/8",
            "10.0.0.0/8",
            "100.64.0.0/10",
            "127.0.0.0/8",
            "169.254.0.0/16",
            "172.16.0.0/12",
            "192.0.0.0/24",
            "192.0.2.0/24",
            "192.88.99.0/24",
            "192.168.0.0/16",
            "198.18.0.0/15",
            "198.51.100.0/24",
            "203.0.113.0/24",
            "224.0.0.0/4",
            "240.0.0.0/4",
            "::/96",
            "::ffff:0:0/96",
            "64:ff9b:1::/48",
            "100::/64",
            "2001::/23",
            "2002::/16",
            "3fff::/20",
            "fc00::/7",
            "fe80::/10",
            "fec0::/10",
            "ff00::/8",
        ]
        .into_iter()
        .map(|network| network.parse().expect("valid notification CIDR"))
        .collect()
    });
    !networks.iter().any(|network| network.contains(&ip))
}

async fn notification_http_client(url: &url::Url) -> anyhow::Result<reqwest::Client> {
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("notification URL must include a host"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow::anyhow!("notification URL must include a port"))?;
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none());

    if matches!(url.host(), Some(url::Host::Domain(_))) {
        let addresses: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| anyhow::anyhow!("notification destination resolution failed"))?
            .collect();
        if !notification_addresses_allowed(host, &addresses) {
            anyhow::bail!("notification destination is outside the allowed network policy");
        }

        builder = builder.resolve_to_addrs(host, &addresses);
    }

    builder
        .build()
        .map_err(|_| anyhow::anyhow!("failed to initialize notification HTTP client"))
}

/// Release kinds that notify, mapped to their event filter names.
fn release_event_type(kind: ReleaseEventKind) -> Option<&'static str> {
    match kind {
        ReleaseEventKind::Uploaded => Some("release_uploaded"),
        ReleaseEventKind::RolloutStepRaised => Some("rollout_step_raised"),
        ReleaseEventKind::RolloutHalted => Some("rollout_halted"),
        ReleaseEventKind::RolloutCompleted => Some("rollout_completed"),
        ReleaseEventKind::ApprovalNeeded => Some("release_approval_needed"),
        ReleaseEventKind::Failed => Some("release_failed"),
        ReleaseEventKind::Created
        | ReleaseEventKind::RolloutPaused
        | ReleaseEventKind::RolloutResumed
        | ReleaseEventKind::Approved => None,
    }
}

/// One event ready to fan out to every matching channel. Each delivery
/// stores it, so a failed send can be retried after a restart.
#[derive(Debug, Serialize, Deserialize)]
struct Notification {
    /// Name matched against a channel's event filter, e.g. `failed`.
    event_type: String,
    /// `build`, `runner` or `release`; selects the message renderer.
    category: String,
    /// `None` for instance-wide events, which only reach unscoped channels.
    project_id: Option<String>,
    build_id: Option<String>,
    runner_id: Option<String>,
    release_id: Option<String>,
    /// Email of the user to mention on Slack.
    actor_email: Option<String>,
    /// Git username behind a push or merge request build, matched against a
    /// Slack channel's member mappings.
    #[serde(default)]
    actor_login: Option<String>,
    /// Generic JSON payload sent to webhook channels.
    payload: serde_json::Value,
}

/// Start the notification dispatch background tasks.
pub fn start_notification_dispatcher(
    pool: SqlitePool,
    scheduler: Arc<Scheduler>,
    encryption_key: Vec<u8>,
) {
    let encryption_key: Arc<[u8]> = encryption_key.into();

    // Build notifications come from a scan of the builds table, so terminal
    // writes that publish no event still notify. Published events only make
    // the scan run sooner.
    let build_pool = pool.clone();
    let build_key = encryption_key.clone();
    let mut build_rx = scheduler.subscribe_events();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                received = build_rx.recv() => {
                    if let Err(tokio::sync::broadcast::error::RecvError::Closed) = received {
                        info!("build event bus closed, stopping build notifications");
                        break;
                    }
                }
                () = tokio::time::sleep(BUILD_SCAN_INTERVAL) => {}
            }
            match notify_due_builds(&build_pool).await {
                Ok(deliveries) if !deliveries.is_empty() => {
                    tokio::spawn(deliver(build_pool.clone(), build_key.clone(), deliveries));
                }
                Ok(_) => {}
                Err(e) => error!(error = %e, "build notification scan failed"),
            }
        }
    });

    // Retries failed sends, and sends left pending by a restart.
    let retry_pool = pool.clone();
    let retry_key = encryption_key.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(RETRY_SCAN_INTERVAL).await;
            match due_deliveries(&retry_pool).await {
                Ok(deliveries) => deliver(retry_pool.clone(), retry_key.clone(), deliveries).await,
                Err(e) => error!(error = %e, "notification retry scan failed"),
            }
        }
    });

    // Runner event dispatcher
    let runner_pool = pool.clone();
    let runner_key = encryption_key.clone();
    let mut runner_rx = scheduler.subscribe_runner_events();
    tokio::spawn(async move {
        loop {
            match runner_rx.recv().await {
                Ok(event) => {
                    if event.to_status != "offline" {
                        continue;
                    }
                    let notification = Notification {
                        event_type: "runner_offline".to_string(),
                        category: "runner".to_string(),
                        project_id: None,
                        build_id: None,
                        runner_id: Some(event.runner_id.clone()),
                        release_id: None,
                        actor_email: None,
                        actor_login: None,
                        payload: runner_notification_payload(&event),
                    };
                    match enqueue(&runner_pool, &notification).await {
                        Ok(deliveries) => {
                            tokio::spawn(deliver(
                                runner_pool.clone(),
                                runner_key.clone(),
                                deliveries,
                            ));
                        }
                        Err(e) => error!(
                            error = %e,
                            runner_id = %event.runner_id,
                            "runner notification dispatch error"
                        ),
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

    // Release event dispatcher
    let mut release_rx = scheduler.subscribe_release_events();
    tokio::spawn(async move {
        loop {
            match release_rx.recv().await {
                Ok(event) => match dispatch_release_event(&pool, &event).await {
                    Ok(deliveries) => {
                        tokio::spawn(deliver(pool.clone(), encryption_key.clone(), deliveries));
                    }
                    Err(e) => error!(
                        error = %e,
                        release_id = %event.release_id,
                        "release notification dispatch error"
                    ),
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(
                        missed = n,
                        "release notification dispatcher lagged behind event bus"
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    info!("release event bus closed, stopping dispatcher");
                    break;
                }
            }
        }
    });
}

/// Status used in build payloads for a build waiting for approval.
const APPROVAL_NEEDED_STATUS: &str = "approval_needed";

/// Notify channels that a build is waiting for a maintainer's approval.
pub async fn dispatch_build_approval_needed(
    pool: &SqlitePool,
    encryption_key: &[u8],
    build_id: &str,
) -> anyhow::Result<()> {
    let event = BuildStateEvent {
        build_id: build_id.to_string(),
        from_status: None,
        to_status: APPROVAL_NEEDED_STATUS.to_string(),
        actor: None,
        reason: None,
        timestamp: now_unix(),
    };
    let Some(notification) = build_notification(pool, &event, "build_approval_needed").await?
    else {
        return Ok(());
    };
    let deliveries = enqueue(pool, &notification).await?;
    deliver(pool.clone(), encryption_key.into(), deliveries).await;
    Ok(())
}

/// Event filter name and category for tester feedback from the QA portal.
const QA_FEEDBACK_EVENT: &str = "qa_feedback";
const FEEDBACK_CATEGORY: &str = "feedback";
/// Slack section text is capped at 3000 characters.
const SLACK_FEEDBACK_MESSAGE_CHARS: usize = 2800;

/// Notify the build's project channels about a tester's feedback. Records
/// the deliveries before returning and sends them in the background.
pub async fn dispatch_build_feedback(
    pool: &SqlitePool,
    encryption_key: &[u8],
    feedback: &oore_contract::BuildFeedback,
) -> anyhow::Result<()> {
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM builds WHERE id = ?1")
        .bind(&feedback.build_id)
        .fetch_optional(pool)
        .await?;
    let Some(status) = status else {
        return Ok(());
    };
    let event = BuildStateEvent {
        build_id: feedback.build_id.clone(),
        from_status: None,
        to_status: status,
        actor: None,
        reason: None,
        timestamp: feedback.created_at,
    };
    let Some(mut notification) = build_notification(pool, &event, QA_FEEDBACK_EVENT).await? else {
        return Ok(());
    };
    notification.category = FEEDBACK_CATEGORY.to_string();
    // The tester may not be in the team's chat, and the person who triggered
    // the build did not write the note; name the tester in the text instead
    // of mentioning anyone.
    notification.actor_email = None;
    notification.actor_login = None;
    notification.payload["event"] = serde_json::json!("build.qa_feedback");
    notification.payload["feedback"] = serde_json::json!({
        "id": feedback.id,
        "author_email": feedback.author_email,
        "author_name": feedback.author_name,
        "message": feedback.message,
        "device": feedback.device,
        "os_version": feedback.os_version,
        "app_version": feedback.app_version,
        "artifact_id": feedback.artifact_id,
        "has_screenshot": feedback.has_screenshot,
        "created_at": feedback.created_at,
    });
    let deliveries = enqueue(pool, &notification).await?;
    if !deliveries.is_empty() {
        tokio::spawn(deliver(pool.clone(), encryption_key.into(), deliveries));
    }
    Ok(())
}

/// Record notifications for builds that finished since the last scan.
/// Returns the deliveries to send.
pub(crate) async fn notify_due_builds(pool: &SqlitePool) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT id, status, notified_status, COALESCE(finished_at, updated_at) AS finished_at \
         FROM builds WHERE notified_status IS NOT status \
           AND status IN ('succeeded', 'failed', 'canceled', 'timed_out', 'expired') \
         ORDER BY updated_at LIMIT ?1",
    )
    .bind(BUILD_SCAN_BATCH_SIZE)
    .fetch_all(pool)
    .await?;

    let mut deliveries = Vec::new();
    for row in rows {
        let build_id: String = row.get("id");
        let status: String = row.get("status");
        let notified: Option<String> = row.get("notified_status");
        // Retention expires finished builds; their result was already sent.
        let already_finished = notified
            .as_deref()
            .is_some_and(|notified| TERMINAL_STATUSES.contains(&notified));
        let event = BuildStateEvent {
            build_id: build_id.clone(),
            from_status: None,
            to_status: status.clone(),
            actor: None,
            reason: None,
            timestamp: row.get("finished_at"),
        };
        let notification = if already_finished {
            None
        } else {
            build_notification(pool, &event, &status).await?
        };

        let mut tx = pool.begin().await?;
        let marked =
            sqlx::query("UPDATE builds SET notified_status = status WHERE id = ?1 AND status = ?2")
                .bind(&build_id)
                .bind(&status)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        if marked == 1
            && let Some(notification) = &notification
        {
            deliveries.extend(enqueue_on(&mut tx, notification).await?);
        }
        tx.commit().await?;
    }
    Ok(deliveries)
}

/// A build notification under the channel filter name `event_type`, or
/// `None` when the build is gone.
async fn build_notification(
    pool: &SqlitePool,
    event: &BuildStateEvent,
    event_type: &str,
) -> anyhow::Result<Option<Notification>> {
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

    let Some(build_row) = build_row else {
        warn!(build_id = %event.build_id, "build not found for notification dispatch");
        return Ok(None);
    };

    let trigger_actor: Option<String> = build_row.try_get("trigger_actor").ok().flatten();
    let trigger_type: Option<String> = build_row.try_get("trigger_type").ok();
    let (actor_email, actor_login) = build_actor(trigger_type.as_deref(), trigger_actor);
    Ok(Some(Notification {
        event_type: event_type.to_string(),
        category: "build".to_string(),
        project_id: build_row.try_get("project_id").ok(),
        build_id: Some(event.build_id.clone()),
        runner_id: None,
        release_id: None,
        actor_email,
        actor_login,
        payload: build_notification_payload(&build_row, event),
    }))
}

/// Who a build mentions: the signed-in email for manual and API builds, or
/// the Git username for webhook builds. Scheduled builds have no person.
fn build_actor(
    trigger_type: Option<&str>,
    trigger_actor: Option<String>,
) -> (Option<String>, Option<String>) {
    match trigger_actor {
        Some(actor) if actor.contains('@') => (Some(actor), None),
        Some(actor) if trigger_type == Some("webhook") && !actor.trim().is_empty() => {
            (None, Some(actor))
        }
        _ => (None, None),
    }
}

/// Record deliveries for a release or rollout event. Returns the deliveries
/// to send.
async fn dispatch_release_event(
    pool: &SqlitePool,
    event: &ReleaseEvent,
) -> anyhow::Result<Vec<String>> {
    let Some(event_type) = release_event_type(event.kind) else {
        return Ok(Vec::new());
    };
    let project_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM projects WHERE id = ?1")
            .bind(&event.project_id)
            .fetch_optional(pool)
            .await?;

    let notification = Notification {
        event_type: event_type.to_string(),
        category: "release".to_string(),
        project_id: Some(event.project_id.clone()),
        build_id: event.build_id.clone(),
        runner_id: None,
        release_id: Some(event.release_id.clone()),
        actor_email: event.actor.clone().filter(|actor| actor.contains('@')),
        actor_login: None,
        payload: release_notification_payload(event, event_type, project_name.as_deref()),
    };
    enqueue(pool, &notification).await
}

async fn enqueue(pool: &SqlitePool, notification: &Notification) -> anyhow::Result<Vec<String>> {
    let mut conn = pool.acquire().await?;
    enqueue_on(&mut conn, notification).await
}

/// Record a pending delivery for every enabled channel whose project scope
/// and event filter match. Returns the delivery ids.
async fn enqueue_on(
    conn: &mut sqlx::SqliteConnection,
    notification: &Notification,
) -> anyhow::Result<Vec<String>> {
    // `project_id = NULL` never matches, so instance-wide events only reach
    // unscoped channels.
    let channels = sqlx::query(
        "SELECT id, event_filter_json FROM notification_channels \
         WHERE enabled = 1 AND (project_id IS NULL OR project_id = ?1)",
    )
    .bind(&notification.project_id)
    .fetch_all(&mut *conn)
    .await?;

    let notification_json = serde_json::to_string(notification)?;
    let now = now_unix();
    let mut deliveries = Vec::new();
    for channel in channels {
        let event_filter_json: Option<String> = channel.get("event_filter_json");
        if !event_filter_matches(event_filter_json.as_deref(), &notification.event_type) {
            continue;
        }
        let delivery_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO notification_deliveries \
             (id, channel_id, build_id, runner_id, release_id, event_type, event_category, status, \
              attempt_count, notification_json, next_attempt_at, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, ?8, ?9, ?9)",
        )
        .bind(&delivery_id)
        .bind(channel.get::<String, _>("id"))
        .bind(&notification.build_id)
        .bind(&notification.runner_id)
        .bind(&notification.release_id)
        .bind(&notification.event_type)
        .bind(&notification.category)
        .bind(&notification_json)
        .bind(now)
        .execute(&mut *conn)
        .await?;
        deliveries.push(delivery_id);
    }
    Ok(deliveries)
}

/// Pending deliveries whose next attempt is due.
async fn due_deliveries(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM notification_deliveries \
         WHERE status = 'pending' AND next_attempt_at <= ?1 \
         ORDER BY next_attempt_at LIMIT ?2",
    )
    .bind(now_unix())
    .bind(RETRY_BATCH_SIZE)
    .fetch_all(pool)
    .await
}

/// Send deliveries concurrently, so a slow channel does not hold up the rest.
async fn deliver(pool: SqlitePool, encryption_key: Arc<[u8]>, deliveries: Vec<String>) {
    let mut sends = tokio::task::JoinSet::new();
    for delivery_id in deliveries {
        let pool = pool.clone();
        let encryption_key = encryption_key.clone();
        sends.spawn(async move {
            if let Err(e) = attempt_delivery(&pool, &encryption_key, &delivery_id).await {
                error!(error = %e, %delivery_id, "notification delivery could not be recorded");
            }
        });
    }
    while sends.join_next().await.is_some() {}
}

/// Limits sends in flight across every channel.
fn send_permits() -> &'static tokio::sync::Semaphore {
    static PERMITS: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    PERMITS.get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_SENDS))
}

/// Make one attempt at a due delivery and record the result. A failed send
/// is retried with backoff until it runs out of attempts.
async fn attempt_delivery(
    pool: &SqlitePool,
    encryption_key: &[u8],
    delivery_id: &str,
) -> anyhow::Result<()> {
    let _permit = send_permits().acquire().await?;
    // Claiming pushes the next attempt past the send timeout, so a crash
    // mid-send is retried and a concurrent retry scan skips this delivery.
    let now = now_unix();
    let Some(claimed) = sqlx::query(
        "UPDATE notification_deliveries \
         SET attempt_count = attempt_count + 1, next_attempt_at = ?1 \
         WHERE id = ?2 AND status = 'pending' AND next_attempt_at <= ?3 \
         RETURNING channel_id, attempt_count, notification_json",
    )
    .bind(now + DELIVERY_LEASE_SECS)
    .bind(delivery_id)
    .bind(now)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };
    let channel_id: String = claimed.get("channel_id");
    let attempt: i64 = claimed.get("attempt_count");
    let notification_json: Option<String> = claimed.get("notification_json");

    let channel = sqlx::query("SELECT * FROM notification_channels WHERE id = ?1 AND enabled = 1")
        .bind(&channel_id)
        .fetch_optional(pool)
        .await?;
    let notification = notification_json
        .as_deref()
        .map(serde_json::from_str::<Notification>)
        .transpose();
    let (result, retry) = match (channel, notification) {
        (None, _) => (Err(anyhow::anyhow!("channel was disabled")), false),
        (_, Ok(None)) => (Err(anyhow::anyhow!("notification was not recorded")), false),
        (_, Err(e)) => (
            Err(anyhow::anyhow!("notification is unreadable: {e}")),
            false,
        ),
        (Some(channel), Ok(Some(notification))) => {
            let public_url = load_effective_external_access_network_settings(pool)
                .await
                .ok()
                .and_then(|settings| settings.public_url);
            let link = notification_link(public_url.as_deref(), &notification);
            let result = send(&channel, encryption_key, &notification, link.as_deref()).await;
            (result, attempt < MAX_DELIVERY_ATTEMPTS)
        }
    };

    match result {
        Ok(()) => {
            sqlx::query(
                "UPDATE notification_deliveries \
                 SET status = 'delivered', delivered_at = ?1, next_attempt_at = NULL, last_error = NULL \
                 WHERE id = ?2",
            )
            .bind(now_unix())
            .bind(delivery_id)
            .execute(pool)
            .await?;
            info!(%channel_id, attempt, "notification delivered");
        }
        Err(e) => {
            let error_msg = e.to_string();
            let next_attempt_at =
                retry.then(|| now_unix() + RETRY_BASE_SECS * (1 << (attempt - 1).min(10)));
            sqlx::query(
                "UPDATE notification_deliveries \
                 SET status = CASE WHEN ?1 IS NULL THEN 'failed' ELSE 'pending' END, \
                     next_attempt_at = ?1, last_error = ?2 \
                 WHERE id = ?3",
            )
            .bind(next_attempt_at)
            .bind(&error_msg)
            .bind(delivery_id)
            .execute(pool)
            .await?;
            warn!(
                %channel_id,
                attempt,
                retrying = next_attempt_at.is_some(),
                error = %error_msg,
                "notification delivery failed"
            );
        }
    }
    Ok(())
}

/// Render and send a notification to one channel.
async fn send(
    channel_row: &sqlx::sqlite::SqliteRow,
    encryption_key: &[u8],
    notification: &Notification,
    link: Option<&str>,
) -> anyhow::Result<()> {
    let channel_id: String = channel_row.get("id");
    let channel_type = channel_row
        .get::<String, _>("channel_type")
        .parse::<NotificationChannelType>()
        .unwrap_or(NotificationChannelType::Webhook);
    let payload = &notification.payload;
    match channel_type {
        NotificationChannelType::Email => {
            dispatch_email(
                channel_row,
                encryption_key,
                &channel_id,
                || match notification.category.as_str() {
                    "runner" => runner_email_html(payload),
                    "release" => release_email_html(payload),
                    FEEDBACK_CATEGORY => feedback_email_html(payload, link),
                    _ => build_email_html(payload),
                },
            )
            .await
        }
        NotificationChannelType::Slack => {
            dispatch_slack(channel_row, encryption_key, &channel_id, notification, link).await
        }
        NotificationChannelType::Mattermost => {
            let body = match notification.category.as_str() {
                "runner" => runner_mattermost_payload(payload),
                "release" => release_mattermost_payload(payload),
                FEEDBACK_CATEGORY => feedback_mattermost_payload(payload),
                _ => build_mattermost_payload(payload),
            };
            dispatch_http(
                channel_row,
                encryption_key,
                &channel_id,
                channel_type,
                &body,
            )
            .await
        }
        NotificationChannelType::Webhook => {
            dispatch_http(
                channel_row,
                encryption_key,
                &channel_id,
                channel_type,
                payload,
            )
            .await
        }
    }
}

/// An empty or missing filter matches every event.
fn event_filter_matches(filter_json: Option<&str>, event_type: &str) -> bool {
    let filters: Vec<String> = filter_json
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    filters.is_empty() || filters.iter().any(|filter| filter == event_type)
}

/// Link to the release or build in the browser-visible Oore UI. `None` when
/// no public URL is configured.
fn notification_link(public_url: Option<&str>, notification: &Notification) -> Option<String> {
    let base = public_url?.trim_end_matches('/');
    if let Some(release_id) = &notification.release_id {
        return Some(format!("{base}/releases/{release_id}"));
    }
    let tab = if notification.category == FEEDBACK_CATEGORY {
        "?tab=timeline"
    } else {
        ""
    };
    notification
        .build_id
        .as_ref()
        .map(|build_id| format!("{base}/builds/{build_id}{tab}"))
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

/// Build the JSON notification payload for a release event.
fn release_notification_payload(
    event: &ReleaseEvent,
    event_type: &str,
    project_name: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "event": format!("release.{event_type}"),
        "release": {
            "id": event.release_id,
            "project_id": event.project_id,
            "build_id": event.build_id,
            "version_name": event.version_name,
            "version_code": event.version_code,
            "track": event.track,
            "rollout_percent": event.rollout_percent,
            "actor": event.actor,
            "reason": event.reason,
        },
        "project_name": project_name,
        "timestamp": event.timestamp,
    })
}

/// Build a Mattermost/Slack-compatible message for a runner event.
fn runner_mattermost_payload(payload: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "text": runner_summary(payload, "**"),
        "username": "Oore CI",
        "icon_url": "https://static.oore.build/logo-avatar-192.png",
    })
}

fn runner_summary(payload: &serde_json::Value, bold: &str) -> String {
    let runner_name = payload["runner"]["name"].as_str().unwrap_or("Unknown");
    let from_status = payload["runner"]["from_status"]
        .as_str()
        .unwrap_or("unknown");
    format!(":warning: Runner {bold}{runner_name}{bold} went offline (was {from_status})")
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
            "trigger_actor": row.try_get::<Option<String>, _>("trigger_actor").ok().flatten(),
            "started_at": row.try_get::<Option<i64>, _>("started_at").ok().flatten(),
            "finished_at": row.try_get::<Option<i64>, _>("finished_at").ok().flatten(),
        },
        "project_name": project_name,
        "pipeline_name": pipeline_name,
        "timestamp": event.timestamp,
    })
}

/// Human wording for a build status in messages.
fn build_status_label(status: &str) -> &str {
    match status {
        APPROVAL_NEEDED_STATUS => "needs approval",
        "timed_out" => "timed out",
        other => other,
    }
}

fn build_status_emoji(status: &str) -> &'static str {
    match status {
        APPROVAL_NEEDED_STATUS => ":raising_hand:",
        "succeeded" => ":white_check_mark:",
        "failed" => ":x:",
        "canceled" => ":no_entry_sign:",
        "timed_out" => ":hourglass:",
        "expired" => ":clock1:",
        _ => ":bell:",
    }
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
    let emoji = build_status_emoji(status);
    let status = build_status_label(status);

    let text = format!(
        "{emoji} **{project_name}** / {pipeline_name} — Build #{build_number} **{status}**\nBranch: `{branch}`"
    );

    serde_json::json!({
        "text": text,
        "username": "Oore CI",
        "icon_url": "https://static.oore.build/logo-avatar-192.png",
    })
}

/// One-line description of a release event, using `bold` as the bold marker
/// (`*` for Slack mrkdwn, `**` for Markdown). Values are escaped with `escape`.
fn release_summary(payload: &serde_json::Value, bold: &str, escape: fn(&str) -> String) -> String {
    let release = &payload["release"];
    let project = escape(
        payload["project_name"]
            .as_str()
            .unwrap_or("Unknown Project"),
    );
    let version = match (
        release["version_name"].as_str(),
        release["version_code"].as_i64(),
    ) {
        (Some(name), Some(code)) => format!(" {} ({code})", escape(name)),
        (Some(name), None) => format!(" {}", escape(name)),
        (None, Some(code)) => format!(" ({code})"),
        (None, None) => String::new(),
    };
    let track = release["track"]
        .as_str()
        .map(|track| format!(" on `{}`", escape(track)))
        .unwrap_or_default();
    let percent = release["rollout_percent"]
        .as_f64()
        .map(format_percent)
        .unwrap_or_default();
    let subject = format!("{bold}{project}{version}{bold}");

    match payload["event"].as_str().unwrap_or_default() {
        "release.release_uploaded" => format!(":package: {subject} uploaded{track}"),
        "release.rollout_step_raised" => {
            format!(":arrow_up: {subject} rollout raised to {percent}{track}")
        }
        "release.rollout_halted" => {
            let at = if percent.is_empty() {
                String::new()
            } else {
                format!(" at {percent}")
            };
            format!(":octagonal_sign: {subject} rollout {bold}halted{bold}{at}{track}")
        }
        "release.rollout_completed" => {
            format!(":white_check_mark: {subject} rollout complete{track}")
        }
        "release.release_approval_needed" => {
            format!(":raising_hand: {subject} needs approval{track}")
        }
        "release.release_failed" => format!(":x: {subject} release {bold}failed{bold}{track}"),
        _ => format!(":bell: {subject} release updated{track}"),
    }
}

fn format_percent(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}%")
    } else {
        format!("{value}%")
    }
}

fn release_mattermost_payload(payload: &serde_json::Value) -> serde_json::Value {
    let mut text = release_summary(payload, "**", |value| value.to_string());
    if let Some(reason) = payload["release"]["reason"].as_str() {
        text.push_str(&format!("\nReason: {reason}"));
    }
    serde_json::json!({
        "text": text,
        "username": "Oore CI",
        "icon_url": "https://static.oore.build/logo-avatar-192.png",
    })
}

// ── Tester feedback ──────────────────────────────────────────────

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated: String = value.chars().take(max_chars).collect();
    truncated.push('…');
    truncated
}

/// "Project build #12: feedback from Name", with `bold` marking the project
/// and every value passed through `escape`.
fn feedback_summary(payload: &serde_json::Value, bold: &str, escape: fn(&str) -> String) -> String {
    let project = escape(
        payload["project_name"]
            .as_str()
            .unwrap_or("Unknown Project"),
    );
    let build_number = payload["build"]["build_number"].as_i64().unwrap_or(0);
    let feedback = &payload["feedback"];
    let author = feedback["author_name"]
        .as_str()
        .or_else(|| feedback["author_email"].as_str())
        .unwrap_or("a tester");
    format!(
        "{bold}{project}{bold} build #{build_number}: feedback from {}",
        escape(author)
    )
}

/// Unescaped device, OS, app version and screenshot facts, in that order.
fn feedback_context(payload: &serde_json::Value) -> Vec<String> {
    let feedback = &payload["feedback"];
    let mut context = Vec::new();
    for (key, label) in [("device", ""), ("os_version", ""), ("app_version", "App ")] {
        if let Some(value) = feedback[key].as_str().filter(|value| !value.is_empty()) {
            context.push(format!("{label}{value}"));
        }
    }
    if feedback["has_screenshot"].as_bool() == Some(true) {
        context.push("Screenshot attached".to_string());
    }
    // The tester is named in the headline. Their email stays out of channel
    // text when they have a display name; webhooks still get it as data.
    context
}

/// Escape Markdown so tester text renders literally in Mattermost, including
/// `@channel`-style mentions.
fn markdown_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' | '`' | '*' | '_' | '~' | '[' | ']' | '(' | ')' | '#' | '>' | '<' | '|' | '!' => {
                escaped.push('\\');
                escaped.push(c);
            }
            '@' => escaped.push_str("@\u{200B}"),
            '&' => escaped.push_str("&amp;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

fn feedback_mattermost_payload(payload: &serde_json::Value) -> serde_json::Value {
    let mut text = format!(
        ":speech_balloon: {}",
        feedback_summary(payload, "**", markdown_escape)
    );
    if let Some(message) = payload["feedback"]["message"].as_str() {
        for line in message.lines() {
            text.push_str("\n> ");
            text.push_str(&markdown_escape(line));
        }
    }
    let context = feedback_context(payload);
    if !context.is_empty() {
        text.push('\n');
        text.push_str(
            &context
                .iter()
                .map(|item| markdown_escape(item))
                .collect::<Vec<_>>()
                .join(" · "),
        );
    }
    serde_json::json!({
        "text": text,
        "username": "Oore CI",
        "icon_url": "https://static.oore.build/logo-avatar-192.png",
    })
}

fn feedback_email_html(payload: &serde_json::Value, link: Option<&str>) -> (String, String) {
    let subject = feedback_summary(payload, "", |value| value.to_string());
    let heading = html_escape(&subject);
    let message = html_escape(payload["feedback"]["message"].as_str().unwrap_or_default());
    let context = feedback_context(payload)
        .iter()
        .map(|item| html_escape(item))
        .collect::<Vec<_>>()
        .join(" · ");
    let button = link
        .map(|link| {
            format!(
                r#"
  <p style="margin-bottom: 0;"><a href="{}" style="display: inline-block; background: #2563eb; color: white; padding: 8px 16px; border-radius: 6px; text-decoration: none;">View feedback</a></p>"#,
                html_escape(link)
            )
        })
        .unwrap_or_default();
    let html = format!(
        r#"<!DOCTYPE html>
<html><body style="font-family: -apple-system, BlinkMacSystemFont, sans-serif; max-width: 600px; margin: 0 auto;">
<div style="background: #2563eb; color: white; padding: 16px 24px; border-radius: 8px 8px 0 0;">
  <h2 style="margin: 0;">{heading}</h2>
</div>
<div style="padding: 24px; border: 1px solid #e5e7eb; border-top: none; border-radius: 0 0 8px 8px;">
  <p style="white-space: pre-wrap; margin-top: 0;">{message}</p>
  <p style="color: #6b7280; font-size: 14px;">{context}</p>{button}
</div>
<p style="color: #9ca3af; font-size: 12px; text-align: center; margin-top: 16px;">Sent by Oore CI</p>
</body></html>"#
    );
    (subject, html)
}

// ── Slack ────────────────────────────────────────────────────────

/// Escape text for Slack mrkdwn so names cannot inject links or mentions.
fn slack_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Build a Slack Block Kit message for a notification.
///
/// `mention` is a resolved `<@U…>` mention, or the escaped email when it
/// could not be resolved.
fn slack_message(
    category: &str,
    payload: &serde_json::Value,
    link: Option<&str>,
    mention: Option<&str>,
) -> serde_json::Value {
    // Tester-written text goes in a plain_text block, which Slack never
    // parses for links, mentions or formatting.
    let mut body = None;
    let (mut headline, context, button) = match category {
        "runner" => (runner_summary(payload, "*"), Vec::new(), None),
        FEEDBACK_CATEGORY => {
            let feedback = &payload["feedback"];
            body = feedback["message"]
                .as_str()
                .map(|message| truncate_chars(message, SLACK_FEEDBACK_MESSAGE_CHARS));
            (
                format!(
                    ":speech_balloon: {}",
                    feedback_summary(payload, "*", slack_escape)
                ),
                feedback_context(payload)
                    .into_iter()
                    .map(|item| slack_escape(&item))
                    .collect(),
                Some("View feedback"),
            )
        }
        "release" => {
            let mut context = Vec::new();
            if let Some(reason) = payload["release"]["reason"].as_str() {
                context.push(format!("Reason: {}", slack_escape(reason)));
            }
            (
                release_summary(payload, "*", slack_escape),
                context,
                Some("View release"),
            )
        }
        _ => {
            let build = &payload["build"];
            let status = build["status"].as_str().unwrap_or("unknown");
            let project = slack_escape(
                payload["project_name"]
                    .as_str()
                    .unwrap_or("Unknown Project"),
            );
            let pipeline = slack_escape(
                payload["pipeline_name"]
                    .as_str()
                    .unwrap_or("Unknown Pipeline"),
            );
            let build_number = build["build_number"].as_i64().unwrap_or(0);
            let mut context = Vec::new();
            if let Some(branch) = build["branch"].as_str() {
                context.push(format!("Branch `{}`", slack_escape(branch)));
            }
            if let Some(sha) = build["commit_sha"].as_str() {
                let short: String = sha.chars().take(7).collect();
                context.push(format!("Commit `{}`", slack_escape(&short)));
            }
            let button = match status {
                "succeeded" => "View build",
                APPROVAL_NEEDED_STATUS => "Review build",
                _ => "View logs",
            };
            (
                format!(
                    "{} *{project}* / {pipeline}: build #{build_number} *{}*",
                    build_status_emoji(status),
                    slack_escape(build_status_label(status))
                ),
                context,
                Some(button),
            )
        }
    };

    let fallback = headline.clone();
    if let Some(mention) = mention.filter(|_| category != FEEDBACK_CATEGORY) {
        headline.push_str(&format!("\nTriggered by {mention}"));
    }

    let mut blocks = vec![serde_json::json!({
        "type": "section",
        "text": { "type": "mrkdwn", "text": headline },
    })];
    if let Some(body) = body.filter(|body| !body.is_empty()) {
        blocks.push(serde_json::json!({
            "type": "section",
            "text": { "type": "plain_text", "text": body, "emoji": false },
        }));
    }
    if !context.is_empty() {
        blocks.push(serde_json::json!({
            "type": "context",
            "elements": [{ "type": "mrkdwn", "text": context.join("  ·  ") }],
        }));
    }
    if let (Some(link), Some(label)) = (link, button) {
        blocks.push(serde_json::json!({
            "type": "actions",
            "elements": [{
                "type": "button",
                "text": { "type": "plain_text", "text": label },
                "url": link,
            }],
        }));
    }

    let text = match mention {
        Some(mention) => format!("{fallback} {mention}"),
        None => fallback,
    };
    serde_json::json!({ "text": text, "blocks": blocks })
}

/// Resolve an email to a Slack `<@U…>` mention with `users.lookupByEmail`.
async fn slack_lookup_mention(bot_token: &str, email: &str) -> anyhow::Result<String> {
    let url = url::Url::parse_with_params(
        "https://slack.com/api/users.lookupByEmail",
        &[("email", email)],
    )?;
    let client = notification_http_client(&url).await?;
    let response = client
        .get(url)
        .bearer_auth(bot_token)
        .timeout(SLACK_LOOKUP_TIMEOUT)
        .send()
        .await
        .map_err(safe_notification_transport_error)?;
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|_| anyhow::anyhow!("Slack user lookup returned an invalid response"))?;
    if body["ok"].as_bool() != Some(true) {
        let error = body["error"].as_str().unwrap_or("unknown_error");
        anyhow::bail!("Slack user lookup failed: {}", slack_escape(error));
    }
    match body["user"]["id"].as_str() {
        Some(id) if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()) => {
            Ok(format!("<@{id}>"))
        }
        _ => anyhow::bail!("Slack user lookup returned no user id"),
    }
}

/// A channel's stored Slack member mappings. Unreadable JSON reads as none.
pub(crate) fn parse_slack_mentions(json: Option<&str>) -> Vec<SlackMention> {
    json.and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}

/// The form identities are stored and matched in: trimmed and lowercased.
pub(crate) fn normalize_slack_identity(identity: &str) -> String {
    identity.trim().to_lowercase()
}

/// The mapped `<@U…>` mention for the first identity that has one.
fn mapped_mention(mentions: &[SlackMention], identities: &[&str]) -> Option<String> {
    identities.iter().find_map(|identity| {
        let identity = normalize_slack_identity(identity);
        mentions
            .iter()
            .find(|mention| normalize_slack_identity(&mention.identity) == identity)
            .map(|mention| &mention.member_id)
            .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()))
            .map(|id| format!("<@{id}>"))
    })
}

/// Mention for the person behind an event. A channel mapping wins, then a
/// bot token lookup by email; otherwise the plain email or Git username, so
/// the message still names the person.
async fn slack_mention(
    mentions: &[SlackMention],
    bot_token: Option<&str>,
    email: Option<&str>,
    login: Option<&str>,
) -> Option<String> {
    let identities: Vec<&str> = email.into_iter().chain(login).collect();
    if let Some(mention) = mapped_mention(mentions, &identities) {
        return Some(mention);
    }
    if let (Some(token), Some(email)) = (bot_token, email) {
        match slack_lookup_mention(token, email).await {
            Ok(mention) => return Some(mention),
            Err(e) => warn!(error = %e, "Slack mention lookup failed; using email"),
        }
    }
    email.or(login).map(slack_escape)
}

fn decrypt_channel_field(
    channel_row: &sqlx::sqlite::SqliteRow,
    column: &str,
    encryption_key: &[u8],
) -> anyhow::Result<Option<String>> {
    channel_row
        .get::<Option<String>, _>(column)
        .as_deref()
        .map(|value| crypto::decrypt(value, encryption_key))
        .transpose()
}

async fn dispatch_slack(
    channel_row: &sqlx::sqlite::SqliteRow,
    encryption_key: &[u8],
    channel_id: &str,
    notification: &Notification,
    link: Option<&str>,
) -> anyhow::Result<()> {
    let url = decrypt_channel_field(channel_row, "encrypted_url", encryption_key)
        .map_err(|e| anyhow::anyhow!("failed to decrypt channel {channel_id} URL: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("channel {channel_id} has no URL configured"))?;
    let bot_token = decrypt_channel_field(channel_row, "encrypted_secret", encryption_key)
        .map_err(|e| anyhow::anyhow!("failed to decrypt channel {channel_id} bot token: {e}"))?;

    let mentions = parse_slack_mentions(
        channel_row
            .try_get::<Option<String>, _>("slack_mentions_json")
            .ok()
            .flatten()
            .as_deref(),
    );
    let mention = slack_mention(
        &mentions,
        bot_token.as_deref(),
        notification.actor_email.as_deref(),
        notification.actor_login.as_deref(),
    )
    .await;
    let body = slack_message(
        &notification.category,
        &notification.payload,
        link,
        mention.as_deref(),
    );
    send_raw_http_payload(&url, None, NotificationChannelType::Slack, &body).await
}

/// Send a Slack test message that mentions the person who pressed test, so
/// the email-to-Slack mapping is checked along with the webhook.
pub async fn send_slack_test(
    url: &str,
    bot_token: Option<&str>,
    mentions: &[SlackMention],
    requester_email: &str,
) -> anyhow::Result<()> {
    let mention_line = match (mapped_mention(mentions, &[requester_email]), bot_token) {
        (Some(mention), _) => format!("Mentions work: hello {mention}."),
        (None, Some(token)) => match slack_lookup_mention(token, requester_email).await {
            Ok(mention) => format!("Mentions work: hello {mention}."),
            Err(e) => format!(
                "Mentions are not working for {}: {e}. Check that the bot token has `users:read.email` and that this email matches a Slack account.",
                slack_escape(requester_email)
            ),
        },
        (None, None) => format!(
            "To mention people, map {} to a Slack member ID, or add a bot token with `users:read.email`.",
            slack_escape(requester_email)
        ),
    };
    let body = serde_json::json!({
        "text": "Oore can post to this channel.",
        "blocks": [
            {
                "type": "section",
                "text": { "type": "mrkdwn", "text": ":white_check_mark: *Oore can post to this channel.*" },
            },
            {
                "type": "context",
                "elements": [{ "type": "mrkdwn", "text": mention_line }],
            },
        ],
    });
    send_raw_http_payload(url, None, NotificationChannelType::Slack, &body).await
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
    let body = match channel_type {
        NotificationChannelType::Webhook | NotificationChannelType::Email => payload.clone(),
        NotificationChannelType::Mattermost => build_mattermost_payload(payload),
        NotificationChannelType::Slack => slack_message("build", payload, None, None),
    };
    send_raw_http_payload(url, secret, channel_type, &body).await
}

/// Dispatch an HTTP-based notification (webhook/mattermost) from within a dispatch loop.
async fn dispatch_http(
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

    send_raw_http_payload(&url, secret.as_deref(), channel_type, payload).await
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
    url: &str,
    secret: Option<&str>,
    channel_type: NotificationChannelType,
    body: &serde_json::Value,
) -> anyhow::Result<()> {
    let parsed = url::Url::parse(url).map_err(|_| anyhow::anyhow!("invalid notification URL"))?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        anyhow::bail!("notification URL must not include credentials");
    }
    if !matches!(parsed.scheme(), "http" | "https") {
        anyhow::bail!("notification URL must use HTTP or HTTPS");
    }
    let client = notification_http_client(&parsed).await?;
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

    let response = request
        .body(body_bytes)
        .send()
        .await
        .map_err(safe_notification_transport_error)?;

    let status = response.status();
    if !status.is_success() {
        let body_text = bounded_notification_error_body(response).await;
        anyhow::bail!(
            "notification delivery failed: HTTP {} — {}",
            status,
            body_text.chars().take(200).collect::<String>()
        );
    }

    Ok(())
}

fn safe_notification_transport_error(error: reqwest::Error) -> anyhow::Error {
    let category = if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connection"
    } else if error.is_redirect() {
        "redirect"
    } else if error.is_request() {
        "request"
    } else if error.is_body() {
        "body"
    } else if error.is_decode() {
        "decode"
    } else {
        "transport"
    };
    anyhow::anyhow!("notification transport failed ({category})")
}

async fn bounded_notification_error_body(mut response: reqwest::Response) -> String {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_NOTIFICATION_ERROR_BODY_BYTES as u64)
    {
        return "response body exceeded diagnostic limit".to_string();
    }

    let mut body = Vec::with_capacity(MAX_NOTIFICATION_ERROR_BODY_BYTES);
    while body.len() < MAX_NOTIFICATION_ERROR_BODY_BYTES {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(_) => return "response body unavailable".to_string(),
        };
        let remaining = MAX_NOTIFICATION_ERROR_BODY_BYTES - body.len();
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if chunk.len() >= remaining {
            break;
        }
    }

    String::from_utf8_lossy(&body).into_owned()
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
        APPROVAL_NEEDED_STATUS => ("#d97706", "\u{1F64B}"),
        "succeeded" => ("#16a34a", "\u{2705}"),
        "failed" => ("#dc2626", "\u{274C}"),
        "canceled" => ("#6b7280", "\u{1F6AB}"),
        "timed_out" => ("#d97706", "\u{23F3}"),
        "expired" => ("#9333ea", "\u{1F552}"),
        _ => ("#6b7280", "\u{1F514}"),
    };

    let status_raw = build_status_label(status_raw);
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

fn release_email_html(payload: &serde_json::Value) -> (String, String) {
    let summary = release_summary(payload, "", |value| value.to_string());
    // Drop the leading Slack emoji shortcode for email.
    let summary = summary
        .split_once(' ')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or(summary);
    let reason = payload["release"]["reason"]
        .as_str()
        .map(|reason| format!("<p>Reason: {}</p>", html_escape(reason)))
        .unwrap_or_default();
    let color = match payload["event"].as_str().unwrap_or_default() {
        "release.rollout_halted" | "release.release_failed" => "#dc2626",
        "release.release_approval_needed" => "#d97706",
        _ => "#16a34a",
    };
    let heading = html_escape(&summary.replace('`', ""));

    let html = format!(
        r#"<!DOCTYPE html>
<html><body style="font-family: -apple-system, BlinkMacSystemFont, sans-serif; max-width: 600px; margin: 0 auto;">
<div style="background: {color}; color: white; padding: 16px 24px; border-radius: 8px 8px 0 0;">
  <h2 style="margin: 0;">{heading}</h2>
</div>
<div style="padding: 24px; border: 1px solid #e5e7eb; border-top: none; border-radius: 0 0 8px 8px;">
  {reason}
  <p>Open the release in the Oore CI dashboard for details.</p>
</div>
<p style="color: #9ca3af; font-size: 12px; text-align: center; margin-top: 16px;">Sent by Oore CI</p>
</body></html>"#
    );

    (summary.replace('`', ""), html)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed_build_payload() -> serde_json::Value {
        serde_json::json!({
            "event": "build.failed",
            "build": {
                "id": "b1",
                "build_number": 42,
                "status": "failed",
                "branch": "main",
                "commit_sha": "0123456789abcdef",
            },
            "project_name": "Kite <app>",
            "pipeline_name": "Android",
        })
    }

    fn release_payload(event_type: &str, reason: Option<&str>) -> serde_json::Value {
        let event = ReleaseEvent {
            kind: ReleaseEventKind::RolloutHalted,
            project_id: "p1".to_string(),
            release_id: "r1".to_string(),
            build_id: Some("b1".to_string()),
            version_name: Some("4.2.0".to_string()),
            version_code: Some(420),
            track: Some("production".to_string()),
            rollout_percent: Some(20.0),
            actor: Some("dev@example.com".to_string()),
            reason: reason.map(String::from),
            timestamp: 0,
        };
        release_notification_payload(&event, event_type, Some("Kite"))
    }

    fn notification(build_id: Option<&str>, release_id: Option<&str>) -> Notification {
        Notification {
            event_type: "failed".to_string(),
            category: "build".to_string(),
            project_id: Some("p1".to_string()),
            build_id: build_id.map(String::from),
            runner_id: None,
            release_id: release_id.map(String::from),
            actor_email: None,
            actor_login: None,
            payload: serde_json::Value::Null,
        }
    }

    fn mention(identity: &str, member_id: &str) -> SlackMention {
        SlackMention {
            identity: identity.to_string(),
            member_id: member_id.to_string(),
        }
    }

    #[test]
    fn builds_mention_the_signed_in_email_or_the_git_username() {
        let actor = |actor: &str| Some(actor.to_string());
        assert_eq!(
            build_actor(Some("manual"), actor("dev@example.com")),
            (actor("dev@example.com"), None)
        );
        assert_eq!(
            build_actor(Some("webhook"), actor("octocat")),
            (None, actor("octocat"))
        );
        assert_eq!(
            build_actor(Some("schedule"), actor("schedule: 0 * * * *")),
            (None, None)
        );
        assert_eq!(build_actor(Some("webhook"), None), (None, None));
    }

    #[tokio::test]
    async fn channel_mappings_mention_without_a_bot_token() {
        let mentions = [
            mention("dev@example.com", "U0DEV"),
            mention("octocat", "W0OCTO"),
        ];
        assert_eq!(
            slack_mention(&mentions, None, Some("Dev@Example.com"), None).await,
            Some("<@U0DEV>".to_string())
        );
        assert_eq!(
            slack_mention(&mentions, None, None, Some("OctoCat")).await,
            Some("<@W0OCTO>".to_string())
        );
        // Unmapped people are still named, escaped for mrkdwn.
        assert_eq!(
            slack_mention(&mentions, None, None, Some("<!here>")).await,
            Some("&lt;!here&gt;".to_string())
        );
        assert_eq!(slack_mention(&mentions, None, None, None).await, None);
        // Stored identities are lowercased with Unicode rules, so matching is too.
        assert_eq!(
            mapped_mention(
                &[mention("élodie@example.com", "U0ELO")],
                &["ÉLODIE@Example.com"]
            ),
            Some("<@U0ELO>".to_string())
        );
        // A stored ID that is not plain alphanumerics is never rendered.
        assert_eq!(
            mapped_mention(&[mention("x", "U1> <!channel")], &["x"]),
            None
        );
    }

    #[test]
    fn failed_build_message_links_logs_and_mentions_trigger() {
        let message = slack_message(
            "build",
            &failed_build_payload(),
            Some("https://oore.example.com/builds/b1"),
            Some("<@U123>"),
        );
        let blocks = message["blocks"].as_array().unwrap();
        let headline = blocks[0]["text"]["text"].as_str().unwrap();
        assert!(headline.contains("*Kite &lt;app&gt;*"));
        assert!(headline.contains("build #42 *failed*"));
        assert!(headline.ends_with("Triggered by <@U123>"));
        assert!(
            blocks[1]["elements"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Commit `0123456`")
        );
        assert_eq!(blocks[2]["elements"][0]["text"]["text"], "View logs");
        assert_eq!(
            blocks[2]["elements"][0]["url"],
            "https://oore.example.com/builds/b1"
        );
        assert!(message["text"].as_str().unwrap().contains("<@U123>"));
    }

    #[test]
    fn message_without_public_url_has_no_button() {
        let message = slack_message("build", &failed_build_payload(), None, None);
        let blocks = message["blocks"].as_array().unwrap();
        assert!(blocks.iter().all(|block| block["type"] != "actions"));
    }

    #[test]
    fn rollout_halt_message_names_release_percent_and_reason() {
        let payload = release_payload("rollout_halted", Some("crash rate <2%"));
        let message = slack_message(
            "release",
            &payload,
            Some("https://oore.example.com/releases/r1"),
            None,
        );
        let blocks = message["blocks"].as_array().unwrap();
        assert_eq!(
            blocks[0]["text"]["text"],
            ":octagonal_sign: *Kite 4.2.0 (420)* rollout *halted* at 20% on `production`"
        );
        assert_eq!(
            blocks[1]["elements"][0]["text"],
            "Reason: crash rate &lt;2%"
        );
        assert_eq!(blocks[2]["elements"][0]["text"]["text"], "View release");
    }

    #[test]
    fn release_kinds_map_to_filter_names() {
        assert_eq!(
            release_event_type(ReleaseEventKind::RolloutStepRaised),
            Some("rollout_step_raised")
        );
        assert_eq!(
            release_event_type(ReleaseEventKind::ApprovalNeeded),
            Some("release_approval_needed")
        );
        assert_eq!(release_event_type(ReleaseEventKind::Created), None);
    }

    #[test]
    fn event_filter_matches_empty_or_listed_events() {
        assert!(event_filter_matches(None, "rollout_halted"));
        assert!(event_filter_matches(Some("[]"), "rollout_halted"));
        assert!(event_filter_matches(
            Some(r#"["failed","rollout_halted"]"#),
            "rollout_halted"
        ));
        assert!(!event_filter_matches(
            Some(r#"["failed"]"#),
            "rollout_halted"
        ));
    }

    #[test]
    fn links_prefer_release_then_build() {
        let base = Some("https://oore.example.com/");
        assert_eq!(
            notification_link(base, &notification(Some("b1"), Some("r1"))).as_deref(),
            Some("https://oore.example.com/releases/r1")
        );
        assert_eq!(
            notification_link(base, &notification(Some("b1"), None)).as_deref(),
            Some("https://oore.example.com/builds/b1")
        );
        assert_eq!(
            notification_link(None, &notification(Some("b1"), None)),
            None
        );
    }

    fn feedback_payload() -> serde_json::Value {
        let mut payload = failed_build_payload();
        payload["feedback"] = serde_json::json!({
            "author_email": "qa@example.com",
            "author_name": null,
            "message": "@channel see [this](https://evil.example) <b>now</b>",
            "device": "Pixel 8",
            "os_version": "Android 15",
            "app_version": "2.4.0+118",
            "has_screenshot": false,
        });
        payload
    }

    #[test]
    fn feedback_renders_tester_text_literally_in_every_channel() {
        let payload = feedback_payload();
        let mattermost = feedback_mattermost_payload(&payload);
        let text = mattermost["text"].as_str().unwrap();
        assert!(text.starts_with(
            ":speech_balloon: **Kite \\<app\\>** build #42: feedback from qa@\u{200B}example.com"
        ));
        assert!(text.contains(
            "\n> @\u{200B}channel see \\[this\\]\\(https://evil.example\\) \\<b\\>now\\</b\\>"
        ));
        assert!(text.ends_with("Pixel 8 · Android 15 · App 2.4.0+118"));

        let (subject, html) = feedback_email_html(&payload, None);
        assert_eq!(
            subject,
            "Kite <app> build #42: feedback from qa@example.com"
        );
        assert!(html.contains("&lt;b&gt;now&lt;/b&gt;"));
        assert!(!html.contains("<b>now"));
        assert!(!html.contains("View feedback"));
        let (_, html) = feedback_email_html(
            &payload,
            Some("https://oore.example.com/builds/b1?tab=timeline"),
        );
        assert!(html.contains(r#"<a href="https://oore.example.com/builds/b1?tab=timeline" "#));
        assert!(html.contains(">View feedback</a>"));

        let slack = slack_message(FEEDBACK_CATEGORY, &payload, None, None);
        assert_eq!(slack["blocks"][1]["text"]["type"], "plain_text");
        assert_eq!(
            slack["text"],
            ":speech_balloon: *Kite &lt;app&gt;* build #42: feedback from qa@example.com"
        );
    }

    #[test]
    fn feedback_names_the_tester_without_their_email_when_a_name_exists() {
        let mut payload = feedback_payload();
        payload["feedback"]["author_name"] = "Quinn".into();
        payload["feedback"]["has_screenshot"] = true.into();
        let mattermost = feedback_mattermost_payload(&payload);
        let text = mattermost["text"].as_str().unwrap();
        assert!(text.contains("feedback from Quinn"));
        assert!(!text.contains("qa@"));
        let (subject, html) = feedback_email_html(&payload, None);
        assert!(subject.ends_with("feedback from Quinn"));
        assert!(!html.contains("qa@example.com"));
        let slack = slack_message(FEEDBACK_CATEGORY, &payload, None, None);
        assert!(!slack.to_string().contains("qa@example.com"));
        assert_eq!(
            slack["blocks"][2]["elements"][0]["text"],
            "Pixel 8  ·  Android 15  ·  App 2.4.0+118  ·  Screenshot attached"
        );
    }

    #[test]
    fn feedback_links_open_the_build_timeline() {
        let mut notification = notification(Some("b1"), None);
        notification.category = FEEDBACK_CATEGORY.to_string();
        assert_eq!(
            notification_link(Some("https://oore.example.com"), &notification).as_deref(),
            Some("https://oore.example.com/builds/b1?tab=timeline")
        );
    }

    type Received = Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>;

    /// Receives notification POSTs on localhost, which the network policy
    /// allows for `localhost` URLs.
    async fn capture_server() -> (String, Received) {
        use axum::extract::{Path as AxumPath, State as AxumState};
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let app = axum::Router::new()
            .route(
                "/{hook}",
                axum::routing::post(
                    |AxumState(received): AxumState<Received>,
                     AxumPath(hook): AxumPath<String>,
                     axum::Json(body): axum::Json<serde_json::Value>| async move {
                        received.lock().unwrap().push((hook, body));
                        "ok"
                    },
                ),
            )
            .with_state(received.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://localhost:{port}"), received)
    }

    #[tokio::test]
    async fn release_event_reaches_global_and_matching_project_channels_only() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let key = vec![7u8; 32];
        let (base, received) = capture_server().await;

        sqlx::query(
            "INSERT INTO users (id, email, oidc_subject, created_at, updated_at) \
             VALUES ('u1', 'dev@example.com', 'sub', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        for (id, name) in [("p1", "Kite"), ("p2", "Coin")] {
            sqlx::query(
                "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
                 VALUES (?1, ?2, 'u1', 0, 0)",
            )
            .bind(id)
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
        }
        for (id, channel_type, project_id, filter) in [
            ("global", "webhook", None, None),
            ("kite", "slack", Some("p1"), Some(r#"["rollout_halted"]"#)),
            ("coin", "slack", Some("p2"), None),
            ("kite-failures", "slack", Some("p1"), Some(r#"["failed"]"#)),
        ] {
            let url = crypto::encrypt(&format!("{base}/{id}"), &key).unwrap();
            sqlx::query(
                "INSERT INTO notification_channels \
                 (id, name, channel_type, enabled, event_filter_json, project_id, encrypted_url, created_at, updated_at) \
                 VALUES (?1, ?1, ?2, 1, ?3, ?4, ?5, 0, 0)",
            )
            .bind(id)
            .bind(channel_type)
            .bind(filter)
            .bind(project_id)
            .bind(url)
            .execute(&pool)
            .await
            .unwrap();
        }

        let event = ReleaseEvent {
            kind: ReleaseEventKind::RolloutHalted,
            project_id: "p1".to_string(),
            release_id: "r1".to_string(),
            build_id: None,
            version_name: Some("4.2.0".to_string()),
            version_code: Some(420),
            track: Some("production".to_string()),
            rollout_percent: Some(20.0),
            actor: None,
            reason: None,
            timestamp: 0,
        };
        let deliveries = dispatch_release_event(&pool, &event).await.unwrap();
        deliver(pool.clone(), key.clone().into(), deliveries).await;

        let mut received = received.lock().unwrap().clone();
        received.sort_by(|a, b| a.0.cmp(&b.0));
        let hooks: Vec<&str> = received.iter().map(|(hook, _)| hook.as_str()).collect();
        assert_eq!(hooks, ["global", "kite"]);
        assert_eq!(received[0].1["event"], "release.rollout_halted");
        assert_eq!(
            received[1].1["blocks"][0]["text"]["text"],
            ":octagonal_sign: *Kite 4.2.0 (420)* rollout *halted* at 20% on `production`"
        );

        let delivered: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT channel_id, release_id, status FROM notification_deliveries ORDER BY channel_id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            delivered,
            [
                (
                    "global".to_string(),
                    "r1".to_string(),
                    "delivered".to_string()
                ),
                (
                    "kite".to_string(),
                    "r1".to_string(),
                    "delivered".to_string()
                ),
            ]
        );
    }

    #[tokio::test]
    async fn failed_push_build_posts_to_its_project_channel_with_logs_and_mention() {
        let (base, received) = capture_server().await;
        let key = vec![7u8; 32];
        let pool = channel_pool(&[], &key).await;
        sqlx::query(
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p2', 'Coin', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO external_access_network_settings (id, public_url, created_at, updated_at) \
             VALUES (1, 'https://ci.example.com/', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mentions = r#"[{"identity":"octocat","member_id":"U0OCTO"}]"#;
        for (id, project_id) in [("kite", "p1"), ("coin", "p2")] {
            sqlx::query(
                "INSERT INTO notification_channels \
                 (id, name, channel_type, enabled, project_id, encrypted_url, slack_mentions_json, \
                  created_at, updated_at) \
                 VALUES (?1, ?1, 'slack', 1, ?2, ?3, ?4, 0, 0)",
            )
            .bind(id)
            .bind(project_id)
            .bind(crypto::encrypt(&format!("{base}/{id}"), &key).unwrap())
            .bind(mentions)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
                 trigger_actor, branch, config_snapshot, queued_at, created_at, updated_at) \
             VALUES ('b1', 'p1', 'pl1', 7, 'failed', 'webhook', 'octocat', 'main', '{}', 0, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let ids = notify_due_builds(&pool).await.unwrap();
        deliver(pool.clone(), key.clone().into(), ids).await;

        let received = received.lock().unwrap().clone();
        assert_eq!(received.len(), 1, "only the project's channel posts");
        let (hook, body) = &received[0];
        assert_eq!(hook, "kite");
        let blocks = body["blocks"].as_array().unwrap();
        let headline = blocks[0]["text"]["text"].as_str().unwrap();
        assert!(headline.contains("*Kite* / Android: build #7 *failed*"));
        assert!(headline.ends_with("Triggered by <@U0OCTO>"));
        let button = &blocks.last().unwrap()["elements"][0];
        assert_eq!(button["text"]["text"], "View logs");
        assert_eq!(button["url"], "https://ci.example.com/builds/b1");
    }

    async fn channel_pool(channels: &[(&str, String)], key: &[u8]) -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, created_at, updated_at) \
             VALUES ('u1', 'dev@example.com', 'sub', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p1', 'Kite', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'Android', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        for (id, url) in channels {
            sqlx::query(
                "INSERT INTO notification_channels \
                 (id, name, channel_type, enabled, encrypted_url, created_at, updated_at) \
                 VALUES (?1, ?1, 'webhook', 1, ?2, 0, 0)",
            )
            .bind(id)
            .bind(crypto::encrypt(url, key).unwrap())
            .execute(&pool)
            .await
            .unwrap();
        }
        pool
    }

    async fn deliveries(pool: &SqlitePool) -> Vec<(String, String, String, i64)> {
        sqlx::query_as(
            "SELECT channel_id, event_type, status, attempt_count FROM notification_deliveries \
             ORDER BY channel_id, created_at",
        )
        .fetch_all(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a_failed_delivery_is_retried_until_it_succeeds() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let app = axum::Router::new()
            .route(
                "/hook",
                axum::routing::post(
                    |axum::extract::State(calls): axum::extract::State<Arc<AtomicUsize>>| async move {
                        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                            axum::http::StatusCode::BAD_GATEWAY
                        } else {
                            axum::http::StatusCode::OK
                        }
                    },
                ),
            )
            .with_state(calls.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let key = vec![7u8; 32];
        let pool = channel_pool(&[("hook", format!("http://localhost:{port}/hook"))], &key).await;

        let notification = notification(None, None);
        let ids = enqueue(&pool, &notification).await.unwrap();
        deliver(pool.clone(), key.clone().into(), ids).await;
        let pending: (String, i64, Option<i64>, Option<String>) = sqlx::query_as(
            "SELECT status, attempt_count, next_attempt_at, last_error FROM notification_deliveries",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((pending.0.as_str(), pending.1), ("pending", 1));
        assert!(pending.2.unwrap() > now_unix());
        assert!(pending.3.unwrap().contains("502"));

        // Not due yet, so a retry scan leaves it alone.
        assert!(due_deliveries(&pool).await.unwrap().is_empty());
        sqlx::query("UPDATE notification_deliveries SET next_attempt_at = 0")
            .execute(&pool)
            .await
            .unwrap();
        let due = due_deliveries(&pool).await.unwrap();
        deliver(pool.clone(), key.clone().into(), due).await;

        assert_eq!(
            deliveries(&pool).await,
            [(
                "hook".to_string(),
                "failed".to_string(),
                "delivered".to_string(),
                2
            )]
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_slow_channel_does_not_hold_up_another() {
        let (release, released) = tokio::sync::watch::channel(false);
        let app = axum::Router::new()
            .route(
                "/slow",
                axum::routing::post(
                    |axum::extract::State(mut released): axum::extract::State<
                        tokio::sync::watch::Receiver<bool>,
                    >| async move {
                        let _ = released.wait_for(|released| *released).await;
                        "ok"
                    },
                ),
            )
            .route("/fast", axum::routing::post(|| async { "ok" }))
            .with_state(released);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let key = vec![7u8; 32];
        let base = format!("http://localhost:{port}");
        let pool = channel_pool(
            &[
                ("fast", format!("{base}/fast")),
                ("slow", format!("{base}/slow")),
            ],
            &key,
        )
        .await;

        let ids = enqueue(&pool, &notification(None, None)).await.unwrap();
        let sending = tokio::spawn(deliver(pool.clone(), key.clone().into(), ids));
        let fast_delivered = async {
            loop {
                let status: String = sqlx::query_scalar(
                    "SELECT status FROM notification_deliveries WHERE channel_id = 'fast'",
                )
                .fetch_one(&pool)
                .await
                .unwrap();
                if status == "delivered" {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), fast_delivered)
            .await
            .expect("the fast channel waited for the slow one");
        release.send(true).unwrap();
        sending.await.unwrap();
        let statuses: Vec<String> = deliveries(&pool)
            .await
            .into_iter()
            .map(|(_, _, status, _)| status)
            .collect();
        assert_eq!(statuses, ["delivered", "delivered"]);
    }

    #[tokio::test]
    async fn every_terminal_write_notifies_once_and_retention_stays_quiet() {
        let (base, received) = capture_server().await;
        let key = vec![7u8; 32];
        let pool = channel_pool(&[("hook", format!("{base}/hook"))], &key).await;
        for (id, number) in [("b1", 1), ("b2", 2), ("b3", 3)] {
            sqlx::query(
                "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
                     config_snapshot, queued_at, created_at, updated_at) \
                 VALUES (?1, 'p1', 'pl1', ?2, 'queued', 'manual', '{}', 0, 0, 0)",
            )
            .bind(id)
            .bind(number)
            .execute(&pool)
            .await
            .unwrap();
        }
        assert!(notify_due_builds(&pool).await.unwrap().is_empty());

        // Writes that publish no event, like preflight failures and
        // project-wide cancels.
        sqlx::query("UPDATE builds SET status = 'failed', finished_at = 5 WHERE id = 'b1'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE builds SET status = 'canceled', finished_at = 5 WHERE id = 'b2'")
            .execute(&pool)
            .await
            .unwrap();
        let ids = notify_due_builds(&pool).await.unwrap();
        assert_eq!(ids.len(), 2);
        deliver(pool.clone(), key.clone().into(), ids).await;
        assert!(notify_due_builds(&pool).await.unwrap().is_empty());

        sqlx::query("UPDATE builds SET status = 'expired' WHERE id IN ('b1', 'b2')")
            .execute(&pool)
            .await
            .unwrap();
        assert!(notify_due_builds(&pool).await.unwrap().is_empty());

        let mut events: Vec<String> = received
            .lock()
            .unwrap()
            .iter()
            .map(|(_, body)| body["event"].as_str().unwrap().to_string())
            .collect();
        events.sort();
        assert_eq!(events, ["build.canceled", "build.failed"]);
        let notified: Vec<Option<String>> =
            sqlx::query_scalar("SELECT notified_status FROM builds ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            notified,
            [
                Some("expired".to_string()),
                Some("expired".to_string()),
                None
            ]
        );
    }
}
