use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Context;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use oore_contract::{
    ApiError, ArtifactStorageProvider, ArtifactStorageSettingsResponse, InstancePreferences,
    InstancePreferencesResponse, KeyStorageMode, UpdateArtifactStorageSettingsRequest,
    UpdateInstancePreferencesRequest,
};
use sqlx::Row;
use tracing::{error, info};

use crate::AppState;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::storage;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

fn trim_opt(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub async fn load_key_storage_mode(pool: &sqlx::SqlitePool) -> anyhow::Result<KeyStorageMode> {
    let row = sqlx::query("SELECT key_storage_mode FROM instance_preferences WHERE id = 1")
        .fetch_optional(pool)
        .await
        .context("failed to load instance preferences")?;

    if let Some(row) = row {
        let mode_str: String = row.get("key_storage_mode");
        return KeyStorageMode::from_str(&mode_str).map_err(anyhow::Error::msg);
    }

    Ok(crypto::default_key_storage_mode())
}

fn preferences_response(
    mode: KeyStorageMode,
    updated_at: Option<i64>,
) -> InstancePreferencesResponse {
    InstancePreferencesResponse {
        preferences: InstancePreferences {
            key_storage_mode: mode,
            restart_required: true,
            updated_at,
        },
    }
}

pub async fn get_artifact_storage_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<ArtifactStorageSettingsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let cfg = storage::load_effective_config(&pool, &state.encryption_key)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load artifact storage settings");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load artifact storage settings",
            )
        })?;

    Ok(Json(ArtifactStorageSettingsResponse {
        settings: cfg.to_public_settings(),
    }))
}

pub async fn update_artifact_storage_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<UpdateArtifactStorageSettingsRequest>,
) -> ApiResult<ArtifactStorageSettingsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;

    let now = now_unix();
    let provider = req.provider;

    let local_base_dir = trim_opt(req.local_base_dir);
    let s3_bucket = trim_opt(req.s3_bucket);
    let s3_region = trim_opt(req.s3_region).or(Some("us-east-1".to_string()));
    let s3_endpoint = trim_opt(req.s3_endpoint);
    let access_key_id = trim_opt(req.access_key_id);
    let secret_access_key = trim_opt(req.secret_access_key);

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let existing = sqlx::query(
        "SELECT s3_access_key_encrypted, s3_secret_key_encrypted FROM artifact_storage_settings WHERE id = 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to load existing artifact storage row");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update artifact storage settings",
        )
    })?;

    let existing_access_encrypted = existing
        .as_ref()
        .and_then(|row| row.get::<Option<String>, _>("s3_access_key_encrypted"));
    let existing_secret_encrypted = existing
        .as_ref()
        .and_then(|row| row.get::<Option<String>, _>("s3_secret_key_encrypted"));

    let (
        persist_local_base_dir,
        persist_s3_bucket,
        persist_s3_region,
        persist_s3_endpoint,
        persist_access_encrypted,
        persist_secret_encrypted,
    ) = match provider {
        ArtifactStorageProvider::Disabled => (None, None, None, None, None, None),
        ArtifactStorageProvider::Local => {
            let Some(dir) = local_base_dir else {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_local_base_dir",
                    "local_base_dir is required when provider is local",
                ));
            };

            if !Path::new(&dir).is_absolute() {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_local_base_dir",
                    "local_base_dir must be an absolute path",
                ));
            }

            (Some(dir), None, None, None, None, None)
        }
        ArtifactStorageProvider::S3 | ArtifactStorageProvider::R2 => {
            let Some(bucket) = s3_bucket else {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_s3_bucket",
                    "s3_bucket is required for s3/r2 providers",
                ));
            };

            if provider == ArtifactStorageProvider::R2 && s3_endpoint.is_none() {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_s3_endpoint",
                    "s3_endpoint is required for r2 provider",
                ));
            }

            let access_encrypted = if let Some(value) = access_key_id {
                Some(crypto::encrypt(&value, &state.encryption_key).map_err(|e| {
                    error!(error = %e, "failed to encrypt access key");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "encryption_error",
                        "Failed to encrypt access key",
                    )
                })?)
            } else {
                existing_access_encrypted
            };

            let secret_encrypted = if let Some(value) = secret_access_key {
                Some(crypto::encrypt(&value, &state.encryption_key).map_err(|e| {
                    error!(error = %e, "failed to encrypt secret key");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "encryption_error",
                        "Failed to encrypt secret key",
                    )
                })?)
            } else {
                existing_secret_encrypted
            };

            if access_encrypted.is_none() || secret_encrypted.is_none() {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "missing_s3_credentials",
                    "access_key_id and secret_access_key are required for s3/r2 providers",
                ));
            }

            (
                None,
                Some(bucket),
                s3_region,
                s3_endpoint,
                access_encrypted,
                secret_encrypted,
            )
        }
    };

    sqlx::query(
        "INSERT INTO artifact_storage_settings (
            id, provider, local_base_dir, s3_bucket, s3_region, s3_endpoint,
            s3_access_key_encrypted, s3_secret_key_encrypted,
            updated_by, created_at, updated_at
         ) VALUES (
            1, ?1, ?2, ?3, ?4, ?5,
            ?6, ?7,
            ?8, ?9, ?9
         )
         ON CONFLICT(id) DO UPDATE SET
            provider = excluded.provider,
            local_base_dir = excluded.local_base_dir,
            s3_bucket = excluded.s3_bucket,
            s3_region = excluded.s3_region,
            s3_endpoint = excluded.s3_endpoint,
            s3_access_key_encrypted = excluded.s3_access_key_encrypted,
            s3_secret_key_encrypted = excluded.s3_secret_key_encrypted,
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at",
    )
    .bind(provider.to_string())
    .bind(persist_local_base_dir)
    .bind(persist_s3_bucket)
    .bind(persist_s3_region)
    .bind(persist_s3_endpoint)
    .bind(persist_access_encrypted)
    .bind(persist_secret_encrypted)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to persist artifact storage settings");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update artifact storage settings",
        )
    })?;

    let details = serde_json::json!({
        "provider": provider.to_string(),
    })
    .to_string();

    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "artifact_storage_updated",
        "instance_settings",
        Some("artifact_storage"),
        Some(&details),
    )
    .await;

    // Hot-reload backend so changes apply without daemon restart.
    let backend = storage::load_backend(&pool, &state.encryption_key).await;
    {
        let mut guard = state.storage.write().await;
        *guard = backend;
    }

    let cfg = storage::load_effective_config(&pool, &state.encryption_key)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to reload artifact storage settings after update");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to update artifact storage settings",
            )
        })?;

    info!(provider = %provider, user_id = %auth.0.user_id, "artifact storage settings updated");

    Ok(Json(ArtifactStorageSettingsResponse {
        settings: cfg.to_public_settings(),
    }))
}

pub async fn get_instance_preferences(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<InstancePreferencesResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let row =
        sqlx::query("SELECT key_storage_mode, updated_at FROM instance_preferences WHERE id = 1")
            .fetch_optional(&pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to load instance preferences");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to load instance preferences",
                )
            })?;

    if let Some(row) = row {
        let mode_str: String = row.get("key_storage_mode");
        let mode = KeyStorageMode::from_str(&mode_str).map_err(|e| {
            error!(error = %e, "invalid key storage mode in database");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Invalid instance preferences",
            )
        })?;
        let updated_at: Option<i64> = row.get("updated_at");
        return Ok(Json(preferences_response(mode, updated_at)));
    }

    Ok(Json(preferences_response(
        crypto::default_key_storage_mode(),
        None,
    )))
}

pub async fn update_instance_preferences(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<UpdateInstancePreferencesRequest>,
) -> ApiResult<InstancePreferencesResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    let now = now_unix();

    let active_source =
        crypto::persist_current_key_for_mode(state.encryption_key.as_ref(), req.key_storage_mode)
            .map_err(|e| {
            error!(error = %e, mode = %req.key_storage_mode, "failed to persist key storage mode");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "key_storage_error",
                "Failed to persist key storage mode",
            )
        })?;

    sqlx::query(
        "INSERT INTO instance_preferences (id, key_storage_mode, updated_by, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, ?3)
         ON CONFLICT(id) DO UPDATE SET
            key_storage_mode = excluded.key_storage_mode,
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at",
    )
    .bind(req.key_storage_mode.to_string())
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to persist instance preferences");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update instance preferences",
        )
    })?;

    let details = serde_json::json!({
        "key_storage_mode": req.key_storage_mode.to_string(),
        "active_key_source": active_source.as_str(),
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "instance_preferences_updated",
        "instance_settings",
        Some("preferences"),
        Some(&details),
    )
    .await;

    info!(
        mode = %req.key_storage_mode,
        source = %active_source.as_str(),
        user_id = %auth.0.user_id,
        "instance preferences updated"
    );

    Ok(Json(preferences_response(req.key_storage_mode, Some(now))))
}
