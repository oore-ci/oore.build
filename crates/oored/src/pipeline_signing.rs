use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use base64::Engine as _;
use oore_contract::{
    AndroidSigningBuildType, AndroidSigningProfile, AndroidSigningProfileInput, ApiError,
    PipelineAndroidSigningResponse, RunnerAndroidSigningProfile, RunnerAndroidSigningResponse,
    UpdatePipelineAndroidSigningRequest,
};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::project_rbac::{ProjectPermission, require_pipeline_project_permission};
use crate::runners::{RunnerAuth, require_active_job_signing_grant};
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
        identity_id: None,
        identity_name: None,
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
        identity_id: row.get("identity_id"),
        identity_name: None,
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
    if row.get::<Option<String>, _>("identity_id").is_some() {
        // Served from the signing library by the caller.
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
    connection: &mut SqliteConnection,
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
    .fetch_optional(&mut *connection)
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
    // An upload replaces the picked identity; the caller then adds the
    // upload to the signing library and links it.
    let identity_id = if keystore_payload_input.is_some() {
        None
    } else {
        match input.identity_id.as_deref().map(str::trim) {
            Some("") => None,
            Some(id) => Some(id.to_string()),
            None => existing
                .as_ref()
                .and_then(|row| row.get::<Option<String>, _>("identity_id")),
        }
    };

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
        && identity_id.is_none()
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
                "Enabled {build_type_label} profile requires a signing identity, or a keystore, filename, store password, key alias, and key password"
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
                 updated_at = ?9,
                 identity_id = ?11
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
        .bind(&identity_id)
        .execute(&mut *connection)
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
                created_by, updated_by, created_at, updated_at, identity_id
             ) VALUES (
                ?1, ?2, ?3, ?4,
                ?5, ?6, ?7,
                ?8, ?9, ?10,
                ?11, ?11, ?12, ?12, ?13
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
        .bind(&identity_id)
        .execute(&mut *connection)
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

/// Saves the debug and release profiles together, so a rejected release
/// profile does not leave a changed debug one behind.
async fn save_profiles(
    pool: &SqlitePool,
    encryption_key: &[u8],
    pipeline_id: &str,
    actor_id: &str,
    debug: Option<AndroidSigningProfileInput>,
    release: Option<AndroidSigningProfileInput>,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let store_error = |e: sqlx::Error| {
        error!(error = %e, "failed to save Android signing profiles");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save Android signing settings",
        )
    };
    let mut transaction = pool.begin().await.map_err(store_error)?;
    for (build_type, input) in [
        (AndroidSigningBuildType::Debug, debug),
        (AndroidSigningBuildType::Release, release),
    ] {
        if let Some(input) = input {
            upsert_profile(
                &mut transaction,
                encryption_key,
                pipeline_id,
                actor_id,
                build_type,
                input,
            )
            .await?;
        }
    }
    transaction.commit().await.map_err(store_error)
}

pub async fn get_pipeline_android_signing(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<PipelineAndroidSigningResponse> {
    let pool = state.db.clone();
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

    let mut debug = debug;
    let mut release = release;
    for profile in [&mut debug, &mut release] {
        profile.identity_name =
            crate::signing_identities::identity_name(&pool, profile.identity_id.as_deref())
                .await
                .map_err(|e| {
                    error!(error = %e, "failed to load Android signing identity name");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "store_error",
                        "Failed to load Android signing settings",
                    )
                })?;
    }

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

    let pool = state.db.clone();
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
    for input in [req.debug.as_ref(), req.release.as_ref()]
        .into_iter()
        .flatten()
    {
        if let Some(id) = input
            .identity_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
        {
            crate::signing_identities::pick_for_pipeline(
                &pool,
                &pipeline_id,
                id,
                oore_contract::SigningIdentityKind::AndroidKeystore,
            )
            .await?;
        }
    }

    save_profiles(
        &pool,
        &state.encryption_key,
        &pipeline_id,
        &auth.0.user_id,
        req.debug,
        req.release,
    )
    .await?;
    // A keystore uploaded here goes into the app's signing library.
    if let Err(e) = crate::signing_adoption::adopt_pipeline_signing(
        &pool,
        &state.encryption_key,
        Some(&pipeline_id),
    )
    .await
    {
        error!(error = %e, "failed to add the uploaded keystore to the signing library");
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
    headers: HeaderMap,
) -> ApiResult<RunnerAndroidSigningResponse> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }

    let pool = state.db.clone();

    let pipeline_id =
        require_active_job_signing_grant(&pool, &job_id, &runner_id, &headers).await?;
    job_android_signing(&pool, &state.encryption_key, &pipeline_id, &job_id)
        .await
        .map(Json)
}

/// The Android signing a build of `pipeline_id` gets. Per build type: the
/// identity the pipeline picks, else its own files; the release profile of
/// a workflow that names an identity replaces both.
pub(crate) async fn job_android_signing(
    pool: &SqlitePool,
    encryption_key: &[u8],
    pipeline_id: &str,
    job_id: &str,
) -> Result<RunnerAndroidSigningResponse, (StatusCode, Json<ApiError>)> {
    let (debug_row, release_row) = load_profile_rows(pool, pipeline_id).await.map_err(|e| {
        error!(error = %e, "failed to query Android signing profiles for job");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load build signing settings",
        )
    })?;

    let debug = row_to_runner_profile(
        debug_row.as_ref(),
        encryption_key,
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
        encryption_key,
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

    // A build type that picks a library identity signs with it.
    let mut profiles = [debug, release];
    for (slot, row, build_type) in [
        (0, debug_row.as_ref(), AndroidSigningBuildType::Debug),
        (1, release_row.as_ref(), AndroidSigningBuildType::Release),
    ] {
        let Some(row) = row.filter(|row| row.get::<i32, _>("enabled") != 0) else {
            continue;
        };
        if let Some(identity_id) = row.get::<Option<String>, _>("identity_id") {
            profiles[slot] = Some(
                crate::signing_identities::pipeline_android_profile(
                    pool,
                    encryption_key,
                    &identity_id,
                    build_type,
                )
                .await?,
            );
        }
    }
    let [debug, release] = profiles;

    // A workflow that names an identity signs release builds with it.
    let release =
        match crate::signing_identities::job_android_release_profile(pool, encryption_key, job_id)
            .await?
        {
            Some(profile) => Some(profile),
            None => release,
        };

    Ok(RunnerAndroidSigningResponse { debug, release })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE users (id TEXT PRIMARY KEY)",
            "CREATE TABLE pipelines (id TEXT PRIMARY KEY)",
            include_str!("../migrations/013_pipeline_android_signing.sql"),
            // From 101, without the signing library it points at.
            "ALTER TABLE pipeline_android_signing_profiles ADD COLUMN identity_id TEXT",
            "INSERT INTO users VALUES ('u1')",
            "INSERT INTO pipelines VALUES ('pl1')",
        ] {
            sqlx::raw_sql(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    fn profile(keystore_base64: &str) -> AndroidSigningProfileInput {
        AndroidSigningProfileInput {
            enabled: true,
            keystore_filename: Some("app.jks".to_string()),
            keystore_base64: Some(keystore_base64.to_string()),
            store_password: Some("store".to_string()),
            key_alias: Some("alias".to_string()),
            key_password: Some("key".to_string()),
            identity_id: None,
        }
    }

    async fn saved_build_types(pool: &SqlitePool) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT build_type FROM pipeline_android_signing_profiles ORDER BY build_type",
        )
        .fetch_all(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn an_invalid_release_profile_does_not_save_the_debug_one() {
        let pool = pool().await;
        let key = [7u8; 32];
        let keystore = base64::engine::general_purpose::STANDARD.encode(b"keystore");

        let (status, _) = save_profiles(
            &pool,
            &key,
            "pl1",
            "u1",
            Some(profile(&keystore)),
            Some(profile("not base64!")),
        )
        .await
        .unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(saved_build_types(&pool).await.is_empty());

        save_profiles(
            &pool,
            &key,
            "pl1",
            "u1",
            Some(profile(&keystore)),
            Some(profile(&keystore)),
        )
        .await
        .unwrap();
        assert_eq!(saved_build_types(&pool).await, ["debug", "release"]);
    }

    async fn release_identity(pool: &SqlitePool) -> Option<String> {
        sqlx::query_scalar(
            "SELECT identity_id FROM pipeline_android_signing_profiles WHERE build_type = 'release'",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a_picked_identity_signs_without_files_and_an_upload_replaces_it() {
        let pool = pool().await;
        let key = [7u8; 32];
        let pick = |id: &str| AndroidSigningProfileInput {
            enabled: true,
            identity_id: Some(id.to_string()),
            ..AndroidSigningProfileInput::default()
        };
        save_profiles(&pool, &key, "pl1", "u1", None, Some(pick("lib-1")))
            .await
            .unwrap();
        assert_eq!(release_identity(&pool).await.as_deref(), Some("lib-1"));

        // Omitted keeps the pick, so turning signing off and on keeps it.
        let keep = AndroidSigningProfileInput {
            enabled: true,
            ..AndroidSigningProfileInput::default()
        };
        save_profiles(&pool, &key, "pl1", "u1", None, Some(keep))
            .await
            .unwrap();
        assert_eq!(release_identity(&pool).await.as_deref(), Some("lib-1"));

        let keystore = base64::engine::general_purpose::STANDARD.encode(b"keystore");
        save_profiles(&pool, &key, "pl1", "u1", None, Some(profile(&keystore)))
            .await
            .unwrap();
        assert_eq!(release_identity(&pool).await, None);

        // Clearing the pick with no files of its own is refused when enabled.
        sqlx::query("UPDATE pipeline_android_signing_profiles SET keystore_encrypted = NULL")
            .execute(&pool)
            .await
            .unwrap();
        let (status, _) = save_profiles(&pool, &key, "pl1", "u1", None, Some(pick("")))
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
