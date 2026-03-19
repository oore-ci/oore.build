use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, CreateNotificationChannelRequest, DeleteNotificationChannelResponse,
    ListNotificationChannelsResponse, ListNotificationDeliveriesResponse, NotificationChannel,
    NotificationChannelResponse, NotificationChannelType, NotificationDelivery,
    NotificationDeliveryStatus, TestNotificationChannelResponse, UpdateNotificationChannelRequest,
};
use sqlx::Row;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

// ── Row helpers ─────────────────────────────────────────────────

fn row_to_channel(row: &sqlx::sqlite::SqliteRow) -> NotificationChannel {
    let events_json: Option<String> = row.get("event_filter_json");
    let events: Vec<String> = events_json
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();

    let channel_type_str: String = row.get("channel_type");
    let channel_type = channel_type_str
        .parse::<NotificationChannelType>()
        .unwrap_or(NotificationChannelType::Webhook);

    let enabled_int: i32 = row.get("enabled");
    let encrypted_url: Option<String> = row.get("encrypted_url");
    let encrypted_secret: Option<String> = row.get("encrypted_secret");

    NotificationChannel {
        id: row.get("id"),
        name: row.get("name"),
        channel_type,
        enabled: enabled_int != 0,
        events,
        has_url: encrypted_url.is_some(),
        has_secret: encrypted_secret.is_some(),
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_delivery(row: &sqlx::sqlite::SqliteRow) -> NotificationDelivery {
    let status_str: String = row.get("status");
    let status = match status_str.as_str() {
        "delivered" => NotificationDeliveryStatus::Delivered,
        "failed" => NotificationDeliveryStatus::Failed,
        _ => NotificationDeliveryStatus::Pending,
    };

    NotificationDelivery {
        id: row.get("id"),
        channel_id: row.get("channel_id"),
        build_id: row.try_get("build_id").ok().flatten(),
        event_type: row.get("event_type"),
        status,
        attempt_count: row.get("attempt_count"),
        last_error: row.get("last_error"),
        created_at: row.get("created_at"),
        delivered_at: row.get("delivered_at"),
    }
}

// ── Validation ──────────────────────────────────────────────────

const VALID_EVENTS: &[&str] = &["succeeded", "failed", "canceled", "timed_out", "expired", "runner_offline"];

fn validate_events(events: &[String]) -> Result<(), (StatusCode, Json<ApiError>)> {
    for event in events {
        if !VALID_EVENTS.contains(&event.as_str()) {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                format!(
                    "invalid event filter '{}'; valid values: {}",
                    event,
                    VALID_EVENTS.join(", ")
                ),
            ));
        }
    }
    Ok(())
}

fn validate_url(url: &str) -> Result<(), (StatusCode, Json<ApiError>)> {
    let parsed = url::Url::parse(url).map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "invalid URL format",
        )
    })?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "URL must use http or https scheme",
        ));
    }
    Ok(())
}

// ── Handlers ────────────────────────────────────────────────────

/// `POST /v1/settings/notification-channels` — create a notification channel.
pub async fn create_notification_channel(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateNotificationChannelRequest>,
) -> ApiResult<NotificationChannelResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;

    let name = req.name.trim();
    if name.is_empty() || name.len() > 200 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "name must be 1-200 characters",
        ));
    }

    validate_url(&req.url)?;
    validate_events(&req.events)?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let id = Uuid::new_v4().to_string();
    let now = now_unix();
    let channel_type_str = req.channel_type.to_string();

    let encrypted_url = crypto::encrypt(&req.url, &state.encryption_key).map_err(|e| {
        error!(error = %e, "failed to encrypt notification channel URL");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "encryption_error",
            "Failed to encrypt channel URL",
        )
    })?;

    let encrypted_secret = req
        .secret
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| crypto::encrypt(s, &state.encryption_key))
        .transpose()
        .map_err(|e| {
            error!(error = %e, "failed to encrypt notification channel secret");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "encryption_error",
                "Failed to encrypt channel secret",
            )
        })?;

    let events_json = if req.events.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&req.events).unwrap_or_else(|_| "[]".to_string()))
    };

    sqlx::query(
        "INSERT INTO notification_channels \
         (id, name, channel_type, enabled, event_filter_json, encrypted_url, encrypted_secret, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
    )
    .bind(&id)
    .bind(name)
    .bind(&channel_type_str)
    .bind(req.enabled as i32)
    .bind(&events_json)
    .bind(&encrypted_url)
    .bind(&encrypted_secret)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to insert notification channel");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to create notification channel",
        )
    })?;

    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "notification_channel_created",
        "notification_channel",
        Some(&id),
        Some(&format!("type={channel_type_str}, name={name}")),
    )
    .await;

    info!(channel_id = %id, channel_type = %channel_type_str, name = %name, "notification channel created");

    let channel = NotificationChannel {
        id,
        name: name.to_string(),
        channel_type: req.channel_type,
        enabled: req.enabled,
        events: req.events,
        has_url: true,
        has_secret: encrypted_secret.is_some(),
        created_by: Some(auth.0.user_id),
        created_at: now,
        updated_at: now,
    };

    Ok(Json(NotificationChannelResponse { channel }))
}

/// `GET /v1/settings/notification-channels` — list all notification channels.
pub async fn list_notification_channels(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<ListNotificationChannelsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let rows = sqlx::query("SELECT * FROM notification_channels ORDER BY created_at DESC")
        .fetch_all(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to list notification channels");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to list notification channels",
            )
        })?;

    let total = rows.len() as i64;
    let channels = rows.iter().map(row_to_channel).collect();

    Ok(Json(ListNotificationChannelsResponse { channels, total }))
}

/// `GET /v1/settings/notification-channels/{id}` — get a single notification channel.
pub async fn get_notification_channel(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<NotificationChannelResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let row = sqlx::query("SELECT * FROM notification_channels WHERE id = ?1")
        .bind(&id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch notification channel");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch notification channel",
            )
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                "not_found",
                "Notification channel not found",
            )
        })?;

    Ok(Json(NotificationChannelResponse {
        channel: row_to_channel(&row),
    }))
}

/// `PUT /v1/settings/notification-channels/{id}` — update a notification channel.
pub async fn update_notification_channel(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateNotificationChannelRequest>,
) -> ApiResult<NotificationChannelResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    // Verify channel exists
    let existing = sqlx::query("SELECT * FROM notification_channels WHERE id = ?1")
        .bind(&id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch notification channel");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch notification channel",
            )
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                "not_found",
                "Notification channel not found",
            )
        })?;

    let now = now_unix();

    // Build update fields
    let name = req
        .name
        .as_deref()
        .map(|n| n.trim())
        .unwrap_or_else(|| existing.get::<&str, _>("name"));

    if name.is_empty() || name.len() > 200 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "name must be 1-200 characters",
        ));
    }

    let enabled = req
        .enabled
        .unwrap_or_else(|| existing.get::<i32, _>("enabled") != 0);

    let events_json = if let Some(ref events) = req.events {
        validate_events(events)?;
        if events.is_empty() {
            None
        } else {
            Some(serde_json::to_string(events).unwrap_or_else(|_| "[]".to_string()))
        }
    } else {
        existing.get::<Option<String>, _>("event_filter_json")
    };

    let encrypted_url = if let Some(ref url) = req.url {
        validate_url(url)?;
        Some(crypto::encrypt(url, &state.encryption_key).map_err(|e| {
            error!(error = %e, "failed to encrypt URL");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "encryption_error",
                "Failed to encrypt URL",
            )
        })?)
    } else {
        existing.get::<Option<String>, _>("encrypted_url")
    };

    let encrypted_secret = if let Some(ref secret) = req.secret {
        if secret.is_empty() {
            None // Clear the secret
        } else {
            Some(crypto::encrypt(secret, &state.encryption_key).map_err(|e| {
                error!(error = %e, "failed to encrypt secret");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "encryption_error",
                    "Failed to encrypt secret",
                )
            })?)
        }
    } else {
        existing.get::<Option<String>, _>("encrypted_secret")
    };

    sqlx::query(
        "UPDATE notification_channels \
         SET name = ?1, enabled = ?2, event_filter_json = ?3, \
             encrypted_url = ?4, encrypted_secret = ?5, updated_at = ?6 \
         WHERE id = ?7",
    )
    .bind(name)
    .bind(enabled as i32)
    .bind(&events_json)
    .bind(&encrypted_url)
    .bind(&encrypted_secret)
    .bind(now)
    .bind(&id)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to update notification channel");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update notification channel",
        )
    })?;

    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "notification_channel_updated",
        "notification_channel",
        Some(&id),
        None,
    )
    .await;

    info!(channel_id = %id, "notification channel updated");

    // Reload from DB
    let row = sqlx::query("SELECT * FROM notification_channels WHERE id = ?1")
        .bind(&id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to reload notification channel");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to reload notification channel",
            )
        })?;

    Ok(Json(NotificationChannelResponse {
        channel: row_to_channel(&row),
    }))
}

/// `DELETE /v1/settings/notification-channels/{id}` — delete a notification channel.
pub async fn delete_notification_channel(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<DeleteNotificationChannelResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let result = sqlx::query("DELETE FROM notification_channels WHERE id = ?1")
        .bind(&id)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to delete notification channel");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to delete notification channel",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Notification channel not found",
        ));
    }

    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "notification_channel_deleted",
        "notification_channel",
        Some(&id),
        None,
    )
    .await;

    info!(channel_id = %id, "notification channel deleted");

    Ok(Json(DeleteNotificationChannelResponse { deleted: true }))
}

/// `POST /v1/settings/notification-channels/{id}/test` — send a test notification.
pub async fn test_notification_channel(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<TestNotificationChannelResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let row = sqlx::query("SELECT * FROM notification_channels WHERE id = ?1")
        .bind(&id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch notification channel");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch notification channel",
            )
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                "not_found",
                "Notification channel not found",
            )
        })?;

    let channel_type_str: String = row.get("channel_type");
    let encrypted_url: Option<String> = row.get("encrypted_url");
    let encrypted_secret: Option<String> = row.get("encrypted_secret");

    let url = encrypted_url
        .as_deref()
        .map(|eu| crypto::decrypt(eu, &state.encryption_key))
        .transpose()
        .map_err(|e| {
            error!(error = %e, "failed to decrypt notification channel URL");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "decryption_error",
                "Failed to decrypt channel URL",
            )
        })?
        .ok_or_else(|| {
            api_err(
                StatusCode::BAD_REQUEST,
                "invalid_state",
                "Channel has no URL configured",
            )
        })?;

    let secret = encrypted_secret
        .as_deref()
        .map(|es| crypto::decrypt(es, &state.encryption_key))
        .transpose()
        .map_err(|e| {
            error!(error = %e, "failed to decrypt notification channel secret");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "decryption_error",
                "Failed to decrypt channel secret",
            )
        })?;

    // Build test payload
    let channel_type = channel_type_str
        .parse::<NotificationChannelType>()
        .unwrap_or(NotificationChannelType::Webhook);

    let result = crate::notification_dispatch::send_notification(
        &url,
        secret.as_deref(),
        channel_type,
        &crate::notification_dispatch::test_payload(),
    )
    .await;

    match result {
        Ok(()) => {
            info!(channel_id = %id, "test notification sent successfully");
            Ok(Json(TestNotificationChannelResponse {
                success: true,
                error: None,
            }))
        }
        Err(e) => {
            warn!(channel_id = %id, error = %e, "test notification failed");
            Ok(Json(TestNotificationChannelResponse {
                success: false,
                error: Some(e.to_string()),
            }))
        }
    }
}

/// `GET /v1/settings/notification-channels/{id}/deliveries` — list delivery history.
pub async fn list_deliveries(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<ListNotificationDeliveriesResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    // Verify channel exists
    let exists =
        sqlx::query_scalar::<_, i32>("SELECT COUNT(*) FROM notification_channels WHERE id = ?1")
            .bind(&id)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to check notification channel existence");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to check channel",
                )
            })?;

    if exists == 0 {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Notification channel not found",
        ));
    }

    let rows = sqlx::query(
        "SELECT * FROM notification_deliveries WHERE channel_id = ?1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(&id)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to list notification deliveries");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to list deliveries",
        )
    })?;

    let total = rows.len() as i64;
    let deliveries = rows.iter().map(row_to_delivery).collect();

    Ok(Json(ListNotificationDeliveriesResponse {
        deliveries,
        total,
    }))
}
