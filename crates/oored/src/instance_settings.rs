use std::path::Path;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use oore_contract::{
    ApiError, ArtifactStorageProvider, ArtifactStorageSettingsResponse,
    ExternalAccessPreflightCheck, ExternalAccessPreflightResponse, InstancePreferences,
    InstancePreferencesResponse, KeyStorageMode, RuntimeMode, SetupState,
    UpdateArtifactStorageSettingsRequest, UpdateInstancePreferencesRequest,
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
    let _ = pool;
    Ok(KeyStorageMode::File)
}

pub async fn load_runtime_mode(pool: &sqlx::SqlitePool) -> anyhow::Result<RuntimeMode> {
    let row = sqlx::query("SELECT runtime_mode FROM instance_preferences WHERE id = 1")
        .fetch_optional(pool)
        .await?;

    let mode = match row {
        Some(row) => {
            let raw: Option<String> = row.try_get("runtime_mode").ok();
            raw.and_then(|value| value.parse::<RuntimeMode>().ok())
                .unwrap_or(RuntimeMode::Local)
        }
        None => RuntimeMode::Local,
    };

    Ok(mode)
}

fn preferences_response(
    mode: KeyStorageMode,
    runtime_mode: RuntimeMode,
    updated_at: Option<i64>,
) -> InstancePreferencesResponse {
    InstancePreferencesResponse {
        preferences: InstancePreferences {
            key_storage_mode: mode,
            runtime_mode,
            restart_required: true,
            updated_at,
        },
    }
}

fn build_external_access_check(
    id: &str,
    label: &str,
    ok: bool,
    message: impl Into<String>,
    failure_code: Option<&str>,
) -> ExternalAccessPreflightCheck {
    ExternalAccessPreflightCheck {
        id: id.to_string(),
        label: label.to_string(),
        ok,
        message: message.into(),
        failure_code: failure_code.map(str::to_string),
    }
}

fn first_preflight_failure_code(result: &ExternalAccessPreflightResponse) -> &str {
    result
        .checks
        .iter()
        .find(|check| !check.ok)
        .and_then(|check| check.failure_code.as_deref())
        .unwrap_or("external_access_preflight_failed")
}

fn preflight_failure_summary(result: &ExternalAccessPreflightResponse) -> Vec<String> {
    result
        .checks
        .iter()
        .filter(|check| !check.ok)
        .map(|check| check.id.clone())
        .collect()
}

async fn evaluate_external_access_preflight(
    state: &Arc<AppState>,
) -> Result<ExternalAccessPreflightResponse, (StatusCode, Json<ApiError>)> {
    let setup_state = {
        let store = state.store.lock().await;
        let sf = store.load().await.map_err(|e| {
            error!(error = %e, "failed to load setup state for external access preflight");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load setup state",
            )
        })?;
        sf.setup_state
    };

    let mut checks = Vec::new();

    checks.push(build_external_access_check(
        "setup_ready",
        "Setup state is ready",
        setup_state == SetupState::Ready,
        if setup_state == SetupState::Ready {
            "Setup is complete."
        } else {
            "Complete setup before enabling External Access."
        },
        None,
    ));

    let oidc_check = match crate::auth::load_oidc_config_for_setup(state).await {
        Ok(_) => build_external_access_check(
            "oidc_configured",
            "OIDC configuration is valid",
            true,
            "OIDC configuration is present and valid for runtime auth.",
            None,
        ),
        Err((_, Json(err))) => build_external_access_check(
            "oidc_configured",
            "OIDC configuration is valid",
            false,
            format!("OIDC is not ready for External Access: {}", err.error),
            None,
        ),
    };
    checks.push(oidc_check);

    let public_url_raw = std::env::var("OORE_PUBLIC_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let mut parsed_public_url: Option<url::Url> = None;
    checks.push(match public_url_raw {
        None => build_external_access_check(
            "public_url_https",
            "Public URL is configured with HTTPS",
            false,
            "Set OORE_PUBLIC_URL to a non-loopback HTTPS URL before enabling External Access.",
            Some("external_access_public_url_missing"),
        ),
        Some(raw) => match url::Url::parse(&raw) {
            Ok(parsed) => {
                let host = parsed.host_str().unwrap_or_default();
                if host.is_empty() || crate::is_loopback_host(host) {
                    build_external_access_check(
                        "public_url_https",
                        "Public URL is configured with HTTPS",
                        false,
                        "OORE_PUBLIC_URL must resolve to a non-loopback host for External Access.",
                        Some("external_access_public_url_missing"),
                    )
                } else if parsed.scheme() != "https" {
                    build_external_access_check(
                        "public_url_https",
                        "Public URL is configured with HTTPS",
                        false,
                        "OORE_PUBLIC_URL must use https for External Access.",
                        Some("external_access_https_required"),
                    )
                } else {
                    parsed_public_url = Some(parsed);
                    build_external_access_check(
                        "public_url_https",
                        "Public URL is configured with HTTPS",
                        true,
                        "Public URL is HTTPS and non-loopback.",
                        None,
                    )
                }
            }
            Err(_) => build_external_access_check(
                "public_url_https",
                "Public URL is configured with HTTPS",
                false,
                "OORE_PUBLIC_URL must be a valid URL.",
                Some("external_access_public_url_missing"),
            ),
        },
    });

    checks.push(if let Some(public_url) = parsed_public_url.as_ref() {
        let origin = public_url.origin().ascii_serialization();
        if state
            .allowed_origins
            .iter()
            .any(|allowed| allowed == &origin)
        {
            build_external_access_check(
                "public_origin_allowed",
                "Public URL origin is allowlisted in CORS",
                true,
                "Public origin is present in allowed CORS origins.",
                None,
            )
        } else {
            build_external_access_check(
                "public_origin_allowed",
                "Public URL origin is allowlisted in CORS",
                false,
                format!(
                    "Add {} to OORE_CORS_ORIGINS before enabling External Access.",
                    origin
                ),
                Some("external_access_origin_not_allowed"),
            )
        }
    } else {
        build_external_access_check(
            "public_origin_allowed",
            "Public URL origin is allowlisted in CORS",
            false,
            "Public URL check must pass before origin allowlist validation can run.",
            None,
        )
    });

    checks.push(if let Some(public_url) = parsed_public_url {
        let redirect_uri = format!(
            "{}/auth/callback",
            public_url.origin().ascii_serialization()
        );
        match crate::validate_redirect_uri(&redirect_uri, &state.allowed_origins) {
            Ok(()) => build_external_access_check(
                "redirect_policy_consistent",
                "Redirect policy is consistent with allowed origins",
                true,
                "Redirect URI policy is consistent with current origin rules.",
                None,
            ),
            Err((_, Json(err))) => build_external_access_check(
                "redirect_policy_consistent",
                "Redirect policy is consistent with allowed origins",
                false,
                format!("Redirect/origin policy validation failed: {}", err.error),
                None,
            ),
        }
    } else {
        build_external_access_check(
            "redirect_policy_consistent",
            "Redirect policy is consistent with allowed origins",
            false,
            "Public URL check must pass before redirect/origin consistency can be validated.",
            None,
        )
    });

    let ready = checks.iter().all(|check| check.ok);
    Ok(ExternalAccessPreflightResponse { ready, checks })
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

    let row = sqlx::query("SELECT runtime_mode, updated_at FROM instance_preferences WHERE id = 1")
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
        let runtime_mode = row
            .try_get::<Option<String>, _>("runtime_mode")
            .ok()
            .flatten()
            .and_then(|raw| raw.parse::<RuntimeMode>().ok())
            .unwrap_or(RuntimeMode::Local);
        let updated_at: Option<i64> = row.get("updated_at");
        return Ok(Json(preferences_response(
            KeyStorageMode::File,
            runtime_mode,
            updated_at,
        )));
    }

    Ok(Json(preferences_response(
        KeyStorageMode::File,
        RuntimeMode::Local,
        None,
    )))
}

pub async fn get_external_access_preflight(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<ExternalAccessPreflightResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;

    let result = evaluate_external_access_preflight(&state).await?;

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    let details = serde_json::json!({
        "ready": result.ready,
        "failed_checks": preflight_failure_summary(&result),
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "external_access_preflight_checked",
        "instance_settings",
        Some("external_access"),
        Some(&details),
    )
    .await;

    Ok(Json(result))
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

    if req.key_storage_mode != KeyStorageMode::File {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "unsupported_key_storage_mode",
            "Keychain mode is disabled in this release. Use file mode.",
        ));
    }

    let existing_mode = load_runtime_mode(&pool).await.map_err(|e| {
        error!(error = %e, "failed to load existing runtime mode");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update instance preferences",
        )
    })?;
    let runtime_mode = req.runtime_mode.unwrap_or(existing_mode);
    let runtime_mode_changed = runtime_mode != existing_mode;

    if runtime_mode_changed && auth.0.role != "owner" {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "external_access_owner_required",
            "Only the owner can change External Access mode",
        ));
    }

    let mut preflight_result: Option<ExternalAccessPreflightResponse> = None;
    if runtime_mode_changed && runtime_mode == RuntimeMode::Remote {
        let result = evaluate_external_access_preflight(&state).await?;
        let preflight_details = serde_json::json!({
            "ready": result.ready,
            "failed_checks": preflight_failure_summary(&result),
        })
        .to_string();
        let _ = write_audit_log(
            &pool,
            Some(&auth.0.user_id),
            "external_access_preflight_checked",
            "instance_settings",
            Some("external_access"),
            Some(&preflight_details),
        )
        .await;

        if !result.ready {
            let blocked_details = serde_json::json!({
                "failed_checks": preflight_failure_summary(&result),
            })
            .to_string();
            let _ = write_audit_log(
                &pool,
                Some(&auth.0.user_id),
                "external_access_enable_blocked",
                "instance_settings",
                Some("external_access"),
                Some(&blocked_details),
            )
            .await;

            let first_failed = result.checks.iter().find(|check| !check.ok);
            let message = first_failed
                .map(|check| check.message.clone())
                .unwrap_or_else(|| {
                    "External Access cannot be enabled until all required checks pass.".to_string()
                });
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                first_preflight_failure_code(&result),
                message,
            ));
        }

        preflight_result = Some(result);
    }

    let active_source =
        crypto::persist_current_key_for_mode(state.encryption_key.as_ref(), KeyStorageMode::File)
            .map_err(|e| {
            error!(error = %e, mode = %KeyStorageMode::File, "failed to persist key storage mode");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "key_storage_error",
                "Failed to persist key storage mode",
            )
        })?;

    sqlx::query(
        "INSERT INTO instance_preferences (id, key_storage_mode, runtime_mode, updated_by, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?4)
         ON CONFLICT(id) DO UPDATE SET
            key_storage_mode = excluded.key_storage_mode,
            runtime_mode = excluded.runtime_mode,
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at",
    )
    .bind(KeyStorageMode::File.to_string())
    .bind(runtime_mode.to_string())
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

    let mut details_value = serde_json::json!({
        "key_storage_mode": KeyStorageMode::File.to_string(),
        "runtime_mode": runtime_mode.to_string(),
        "active_key_source": active_source.as_str(),
    });
    if let Some(result) = preflight_result.as_ref() {
        details_value["external_access_preflight_ready"] = serde_json::json!(result.ready);
    }
    let details = details_value.to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "instance_preferences_updated",
        "instance_settings",
        Some("preferences"),
        Some(&details),
    )
    .await;

    if runtime_mode_changed {
        let revoked_sessions = state.sessions.revoke_all_sessions().await.map_err(|e| {
            error!(error = %e, "failed to revoke sessions after runtime mode change");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session_error",
                "Failed to revoke sessions after runtime mode change",
            )
        })?;

        let runtime_details = serde_json::json!({
            "from_mode": existing_mode.to_string(),
            "to_mode": runtime_mode.to_string(),
            "revoked_sessions": revoked_sessions,
        })
        .to_string();
        let _ = write_audit_log(
            &pool,
            Some(&auth.0.user_id),
            "runtime_mode_changed",
            "instance_settings",
            Some("preferences"),
            Some(&runtime_details),
        )
        .await;

        if runtime_mode == RuntimeMode::Remote {
            let _ = write_audit_log(
                &pool,
                Some(&auth.0.user_id),
                "external_access_enabled",
                "instance_settings",
                Some("external_access"),
                Some(&runtime_details),
            )
            .await;
        }

        info!(
            mode = %KeyStorageMode::File,
            from_mode = %existing_mode,
            to_mode = %runtime_mode,
            source = %active_source.as_str(),
            revoked_sessions,
            user_id = %auth.0.user_id,
            "instance preferences updated and sessions revoked after runtime mode change"
        );
    } else {
        info!(
            mode = %KeyStorageMode::File,
            runtime_mode = %runtime_mode,
            source = %active_source.as_str(),
            user_id = %auth.0.user_id,
            "instance preferences updated"
        );
    }

    Ok(Json(preferences_response(
        KeyStorageMode::File,
        runtime_mode,
        Some(now),
    )))
}
