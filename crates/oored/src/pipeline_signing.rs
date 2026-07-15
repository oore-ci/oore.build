use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use base64::Engine as _;
use oore_contract::{
    AndroidSigningBuildType, AndroidSigningProfile, AndroidSigningProfileInput, ApiError,
    PipelineAndroidSigningResponse, RunnerAndroidSigningProfile, RunnerAndroidSigningResponse,
    UpdatePipelineAndroidSigningRequest,
};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::project_rbac::{ProjectPermission, require_pipeline_project_permission};
use crate::runners::RunnerAuth;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

const MAX_KEYSTORE_BYTES: usize = 5 * 1024 * 1024;

fn build_type_str(build_type: AndroidSigningBuildType) -> &'static str {
    match build_type {
        AndroidSigningBuildType::Debug => "debug",
        AndroidSigningBuildType::Release => "release",
    }
}

fn trim_opt(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn empty_profile(build_type: AndroidSigningBuildType) -> AndroidSigningProfile {
    AndroidSigningProfile {
        build_type,
        enabled: false,
        has_keystore: false,
        keystore_filename: None,
        keystore_checksum: None,
        key_alias: None,
        has_store_password: false,
        has_key_password: false,
        updated_at: None,
    }
}

async fn ensure_pipeline_exists(pool: &SqlitePool, pipeline_id: &str) -> anyhow::Result<bool> {
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM pipelines WHERE id = ?1")
        .bind(pipeline_id)
        .fetch_optional(pool)
        .await?;
    Ok(exists.is_some())
}

fn decode_b64(value: &str) -> anyhow::Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(value))
        .map_err(|e| anyhow::anyhow!("invalid base64 payload: {e}"))
}

fn encrypt_opt(value: Option<String>, key: &[u8]) -> anyhow::Result<Option<String>> {
    match value {
        Some(v) => Ok(Some(crypto::encrypt(&v, key)?)),
        None => Ok(None),
    }
}

fn decrypt_opt(value: Option<String>, key: &[u8]) -> anyhow::Result<Option<String>> {
    match value {
        Some(v) => Ok(Some(crypto::decrypt(&v, key)?)),
        None => Ok(None),
    }
}

fn row_to_public_profile(
    row: Option<&sqlx::sqlite::SqliteRow>,
    key: &[u8],
    build_type: AndroidSigningBuildType,
) -> anyhow::Result<AndroidSigningProfile> {
    let Some(row) = row else {
        return Ok(empty_profile(build_type));
    };

    let key_alias_encrypted: Option<String> = row.get("key_alias_encrypted");
    let key_alias = decrypt_opt(key_alias_encrypted, key)?;

    Ok(AndroidSigningProfile {
        build_type,
        enabled: row.get::<i32, _>("enabled") != 0,
        has_keystore: row.get::<Option<String>, _>("keystore_encrypted").is_some(),
        keystore_filename: row.get("keystore_filename"),
        keystore_checksum: row.get("keystore_checksum"),
        key_alias,
        has_store_password: row
            .get::<Option<String>, _>("store_password_encrypted")
            .is_some(),
        has_key_password: row
            .get::<Option<String>, _>("key_password_encrypted")
            .is_some(),
        updated_at: Some(row.get("updated_at")),
    })
}

fn row_to_runner_profile(
    row: Option<&sqlx::sqlite::SqliteRow>,
    key: &[u8],
    build_type: AndroidSigningBuildType,
) -> anyhow::Result<Option<RunnerAndroidSigningProfile>> {
    let Some(row) = row else {
        return Ok(None);
    };
    if row.get::<i32, _>("enabled") == 0 {
        return Ok(None);
    }

    let keystore_filename: Option<String> = row.get("keystore_filename");
    let keystore_encrypted: Option<String> = row.get("keystore_encrypted");
    let store_password_encrypted: Option<String> = row.get("store_password_encrypted");
    let key_alias_encrypted: Option<String> = row.get("key_alias_encrypted");
    let key_password_encrypted: Option<String> = row.get("key_password_encrypted");

    let keystore_filename = keystore_filename.ok_or_else(|| {
        anyhow::anyhow!(
            "android signing profile '{}' is enabled but keystore filename is missing",
            build_type_str(build_type)
        )
    })?;
    let keystore_base64 =
        decrypt_opt(keystore_encrypted, key)?.ok_or_else(|| anyhow::anyhow!("missing keystore"))?;
    let store_password = decrypt_opt(store_password_encrypted, key)?
        .ok_or_else(|| anyhow::anyhow!("missing store password"))?;
    let key_alias = decrypt_opt(key_alias_encrypted, key)?
        .ok_or_else(|| anyhow::anyhow!("missing key alias"))?;
    let key_password = decrypt_opt(key_password_encrypted, key)?
        .ok_or_else(|| anyhow::anyhow!("missing key password"))?;

    Ok(Some(RunnerAndroidSigningProfile {
        build_type,
        enabled: true,
        keystore_filename,
        keystore_base64,
        store_password,
        key_alias,
        key_password,
    }))
}

async fn load_profile_rows(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> anyhow::Result<(
    Option<sqlx::sqlite::SqliteRow>,
    Option<sqlx::sqlite::SqliteRow>,
)> {
    let rows = sqlx::query(
        "SELECT * FROM pipeline_android_signing_profiles WHERE pipeline_id = ?1 ORDER BY build_type ASC",
    )
    .bind(pipeline_id)
    .fetch_all(pool)
    .await?;

    let mut debug = None;
    let mut release = None;
    for row in rows {
        let build_type: String = row.get("build_type");
        match build_type.as_str() {
            "debug" => debug = Some(row),
            "release" => release = Some(row),
            _ => {}
        }
    }
    Ok((debug, release))
}

async fn upsert_profile(
    pool: &SqlitePool,
    encryption_key: &[u8],
    pipeline_id: &str,
    actor_id: &str,
    build_type: AndroidSigningBuildType,
    input: AndroidSigningProfileInput,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let build_type_label = build_type_str(build_type);
    let existing = sqlx::query(
        "SELECT * FROM pipeline_android_signing_profiles WHERE pipeline_id = ?1 AND build_type = ?2",
    )
    .bind(pipeline_id)
    .bind(build_type_label)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to load android signing profile");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to update Android signing profile")
    })?;

    let keystore_filename_input = trim_opt(input.keystore_filename);
    let keystore_payload_input = trim_opt(input.keystore_base64);
    let store_password_input = trim_opt(input.store_password);
    let key_alias_input = trim_opt(input.key_alias);
    let key_password_input = trim_opt(input.key_password);

    let now = now_unix();

    let (mut keystore_filename, mut keystore_encrypted, mut keystore_checksum) =
        if let Some(ref row) = existing {
            (
                row.get::<Option<String>, _>("keystore_filename"),
                row.get::<Option<String>, _>("keystore_encrypted"),
                row.get::<Option<String>, _>("keystore_checksum"),
            )
        } else {
            (None, None, None)
        };

    let mut store_password_encrypted = existing
        .as_ref()
        .and_then(|row| row.get::<Option<String>, _>("store_password_encrypted"));
    let mut key_alias_encrypted = existing
        .as_ref()
        .and_then(|row| row.get::<Option<String>, _>("key_alias_encrypted"));
    let mut key_password_encrypted = existing
        .as_ref()
        .and_then(|row| row.get::<Option<String>, _>("key_password_encrypted"));

    if let Some(payload) = keystore_payload_input {
        let keystore_bytes = decode_b64(&payload).map_err(|_| {
            api_err(
                StatusCode::BAD_REQUEST,
                "invalid_keystore",
                format!("Invalid base64 keystore for {build_type_label} profile"),
            )
        })?;
        if keystore_bytes.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_keystore",
                format!("Keystore for {build_type_label} profile must not be empty"),
            ));
        }
        if keystore_bytes.len() > MAX_KEYSTORE_BYTES {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_keystore",
                format!(
                    "Keystore for {build_type_label} profile exceeds {} bytes",
                    MAX_KEYSTORE_BYTES
                ),
            ));
        }

        let canonical_b64 = base64::engine::general_purpose::STANDARD.encode(&keystore_bytes);
        let checksum = hex::encode(Sha256::digest(&keystore_bytes));
        keystore_encrypted = encrypt_opt(Some(canonical_b64), encryption_key).map_err(|e| {
            error!(error = %e, "failed to encrypt keystore payload");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "encryption_error",
                "Failed to encrypt Android keystore",
            )
        })?;
        keystore_checksum = Some(checksum);
        keystore_filename = keystore_filename_input
            .or_else(|| keystore_filename.clone())
            .or(Some(format!("{build_type_label}.jks")));
    } else if keystore_filename_input.is_some() {
        keystore_filename = keystore_filename_input;
    }

    if let Some(value) = store_password_input {
        store_password_encrypted = encrypt_opt(Some(value), encryption_key).map_err(|e| {
            error!(error = %e, "failed to encrypt Android store password");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "encryption_error",
                "Failed to encrypt Android signing password",
            )
        })?;
    }
    if let Some(value) = key_alias_input {
        key_alias_encrypted = encrypt_opt(Some(value), encryption_key).map_err(|e| {
            error!(error = %e, "failed to encrypt Android key alias");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "encryption_error",
                "Failed to encrypt Android key alias",
            )
        })?;
    }
    if let Some(value) = key_password_input {
        key_password_encrypted = encrypt_opt(Some(value), encryption_key).map_err(|e| {
            error!(error = %e, "failed to encrypt Android key password");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "encryption_error",
                "Failed to encrypt Android key password",
            )
        })?;
    }

    if input.enabled
        && (keystore_encrypted.is_none()
            || keystore_filename.is_none()
            || store_password_encrypted.is_none()
            || key_alias_encrypted.is_none()
            || key_password_encrypted.is_none())
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "incomplete_signing_profile",
            format!(
                "Enabled {build_type_label} profile requires keystore, filename, store password, key alias, and key password"
            ),
        ));
    }

    if let Some(existing) = existing {
        let id: String = existing.get("id");
        sqlx::query(
            "UPDATE pipeline_android_signing_profiles
             SET enabled = ?1,
                 keystore_filename = ?2,
                 keystore_encrypted = ?3,
                 keystore_checksum = ?4,
                 store_password_encrypted = ?5,
                 key_alias_encrypted = ?6,
                 key_password_encrypted = ?7,
                 updated_by = ?8,
                 updated_at = ?9
             WHERE id = ?10",
        )
        .bind(if input.enabled { 1 } else { 0 })
        .bind(keystore_filename)
        .bind(keystore_encrypted)
        .bind(keystore_checksum)
        .bind(store_password_encrypted)
        .bind(key_alias_encrypted)
        .bind(key_password_encrypted)
        .bind(actor_id)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to update Android signing profile");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to update Android signing profile",
            )
        })?;
    } else {
        sqlx::query(
            "INSERT INTO pipeline_android_signing_profiles (
                id, pipeline_id, build_type, enabled,
                keystore_filename, keystore_encrypted, keystore_checksum,
                store_password_encrypted, key_alias_encrypted, key_password_encrypted,
                created_by, updated_by, created_at, updated_at
             ) VALUES (
                ?1, ?2, ?3, ?4,
                ?5, ?6, ?7,
                ?8, ?9, ?10,
                ?11, ?11, ?12, ?12
             )",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(pipeline_id)
        .bind(build_type_label)
        .bind(if input.enabled { 1 } else { 0 })
        .bind(keystore_filename)
        .bind(keystore_encrypted)
        .bind(keystore_checksum)
        .bind(store_password_encrypted)
        .bind(key_alias_encrypted)
        .bind(key_password_encrypted)
        .bind(actor_id)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to insert Android signing profile");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to save Android signing profile",
            )
        })?;
    }

    Ok(())
}

pub async fn get_pipeline_android_signing(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<PipelineAndroidSigningResponse> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    require_pipeline_project_permission(
        &pool,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;

    if !ensure_pipeline_exists(&pool, &pipeline_id)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to verify pipeline");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load Android signing settings",
            )
        })?
    {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Pipeline not found",
        ));
    }

    let (debug_row, release_row) = load_profile_rows(&pool, &pipeline_id).await.map_err(|e| {
        error!(error = %e, "failed to query Android signing profiles");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load Android signing settings",
        )
    })?;

    let debug = row_to_public_profile(
        debug_row.as_ref(),
        &state.encryption_key,
        AndroidSigningBuildType::Debug,
    )
    .map_err(|e| {
        error!(error = %e, "failed to decode debug Android signing profile");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "decryption_error",
            "Failed to load Android signing settings",
        )
    })?;
    let release = row_to_public_profile(
        release_row.as_ref(),
        &state.encryption_key,
        AndroidSigningBuildType::Release,
    )
    .map_err(|e| {
        error!(error = %e, "failed to decode release Android signing profile");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "decryption_error",
            "Failed to load Android signing settings",
        )
    })?;

    Ok(Json(PipelineAndroidSigningResponse {
        pipeline_id,
        debug,
        release,
    }))
}

pub async fn update_pipeline_android_signing(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
    Json(req): Json<UpdatePipelineAndroidSigningRequest>,
) -> ApiResult<PipelineAndroidSigningResponse> {
    if req.debug.is_none() && req.release.is_none() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "At least one profile (debug or release) must be provided",
        ));
    }

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    require_pipeline_project_permission(
        &pool,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;

    if !ensure_pipeline_exists(&pool, &pipeline_id)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to verify pipeline");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to save Android signing settings",
            )
        })?
    {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Pipeline not found",
        ));
    }

    let debug_updated = req.debug.is_some();
    let release_updated = req.release.is_some();

    if let Some(debug) = req.debug {
        upsert_profile(
            &pool,
            &state.encryption_key,
            &pipeline_id,
            &auth.0.user_id,
            AndroidSigningBuildType::Debug,
            debug,
        )
        .await?;
    }
    if let Some(release) = req.release {
        upsert_profile(
            &pool,
            &state.encryption_key,
            &pipeline_id,
            &auth.0.user_id,
            AndroidSigningBuildType::Release,
            release,
        )
        .await?;
    }

    let details = serde_json::json!({
        "pipeline_id": &pipeline_id,
        "updated_by": &auth.0.email,
        "debug_updated": debug_updated,
        "release_updated": release_updated,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "pipeline_android_signing_updated",
        "pipeline",
        Some(&pipeline_id),
        Some(&details),
    )
    .await;

    info!(
        pipeline_id = %pipeline_id,
        actor = %auth.0.email,
        "updated pipeline Android signing settings"
    );

    get_pipeline_android_signing(State(state), auth, Path(pipeline_id)).await
}

pub async fn get_job_android_signing(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
) -> ApiResult<RunnerAndroidSigningResponse> {
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

    let build_row = sqlx::query("SELECT pipeline_id, runner_id FROM builds WHERE id = ?1")
        .bind(&job_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load build for signing lookup");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load build signing settings",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;

    let assigned_runner: Option<String> = build_row.get("runner_id");
    if assigned_runner.as_deref() != Some(&runner_id) {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "This build is not assigned to your runner",
        ));
    }

    let pipeline_id: String = build_row.get("pipeline_id");
    let (debug_row, release_row) = load_profile_rows(&pool, &pipeline_id).await.map_err(|e| {
        error!(error = %e, "failed to query Android signing profiles for job");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load build signing settings",
        )
    })?;

    let debug = row_to_runner_profile(
        debug_row.as_ref(),
        &state.encryption_key,
        AndroidSigningBuildType::Debug,
    )
    .map_err(|e| {
        error!(error = %e, "failed to decode debug signing profile for runner");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "decryption_error",
            "Failed to load debug signing profile",
        )
    })?;
    let release = row_to_runner_profile(
        release_row.as_ref(),
        &state.encryption_key,
        AndroidSigningBuildType::Release,
    )
    .map_err(|e| {
        error!(error = %e, "failed to decode release signing profile for runner");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "decryption_error",
            "Failed to load release signing profile",
        )
    })?;

    Ok(Json(RunnerAndroidSigningResponse { debug, release }))
}
