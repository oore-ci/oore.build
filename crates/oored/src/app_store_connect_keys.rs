//! App Store Connect API keys shared across the instance.
//!
//! A company with several apps under one Apple team uploads its key once here,
//! and each pipeline's iOS signing points at it instead of storing a copy. A
//! pipeline can still keep its own key, which wins when no shared key is set.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use base64::Engine;
use jsonwebtoken::EncodingKey;
use oore_contract::{
    ApiError, AppStoreConnectKeyResponse, AppStoreConnectKeySummary,
    CreateAppStoreConnectKeyRequest, ListAppStoreConnectKeysResponse, OkResponse,
    UpdateAppStoreConnectKeyRequest,
};
use sqlx::{FromRow, SqlitePool};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::apple_api::AppleApiCredentials;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiFailure = (StatusCode, Json<ApiError>);
type ApiResult<T> = Result<Json<T>, ApiFailure>;

const MAX_NAME_LEN: usize = 64;
const MAX_ID_LEN: usize = 128;
const MAX_P8_BYTES: usize = 16 * 1024;

/// A summary query over `app_store_connect_keys k` followed by `$tail`.
macro_rules! summary_query {
    ($tail:literal) => {
        concat!(
            "SELECT k.id, k.name, k.key_id, k.issuer_id, k.created_at, k.updated_at, ",
            "(SELECT COUNT(*) FROM pipeline_ios_signing_settings s ",
            "WHERE s.shared_api_key_id = k.id) AS pipeline_count ",
            "FROM app_store_connect_keys k ",
            $tail
        )
    };
}

#[derive(Debug, FromRow)]
struct KeyRow {
    id: String,
    name: String,
    key_id: String,
    issuer_id: String,
    created_at: i64,
    updated_at: i64,
    pipeline_count: i64,
}

impl From<KeyRow> for AppStoreConnectKeySummary {
    fn from(row: KeyRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            key_id: row.key_id,
            issuer_id: row.issuer_id,
            pipeline_count: row.pipeline_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

fn store_error(context: &'static str, e: impl std::fmt::Display) -> ApiFailure {
    error!(error = %e, "{context}");
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", context)
}

fn invalid(message: impl Into<String>) -> ApiFailure {
    api_err(StatusCode::BAD_REQUEST, "invalid_input", message)
}

pub(crate) async fn load_summary(
    pool: &SqlitePool,
    id: &str,
) -> sqlx::Result<Option<AppStoreConnectKeySummary>> {
    let row = sqlx::query_as::<_, KeyRow>(summary_query!("WHERE k.id = ?1"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Into::into))
}

/// Decrypted credentials of a shared key, or `None` when it no longer exists.
pub(crate) async fn load_credentials(
    pool: &SqlitePool,
    encryption_key: &[u8],
    id: &str,
) -> anyhow::Result<Option<AppleApiCredentials>> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT key_id, issuer_id, private_key_encrypted FROM app_store_connect_keys WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some((key_id, issuer_id, encrypted)) = row else {
        return Ok(None);
    };
    Ok(Some(AppleApiCredentials {
        key_id,
        issuer_id,
        private_key_pem: crypto::decrypt(&encrypted, encryption_key)?,
    }))
}

fn normalize_name(raw: &str) -> Result<String, ApiFailure> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_LEN {
        return Err(invalid(format!(
            "Name must be between 1 and {MAX_NAME_LEN} characters"
        )));
    }
    Ok(name.to_string())
}

fn normalize_identifier(raw: &str, label: &str) -> Result<String, ApiFailure> {
    let value = raw.trim();
    if value.is_empty()
        || value.len() > MAX_ID_LEN
        || !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(invalid(format!(
            "{label} must be the value shown in App Store Connect → Users and Access → Integrations"
        )));
    }
    Ok(value.to_string())
}

/// Decode a `.p8` upload and check it is an EC private key Apple would accept.
fn parse_private_key(base64_payload: &str) -> Result<String, ApiFailure> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_payload.trim())
        .map_err(|_| invalid("Invalid base64 for the App Store Connect private key"))?;
    if bytes.is_empty() || bytes.len() > MAX_P8_BYTES {
        return Err(invalid(format!(
            "The private key must be between 1 and {MAX_P8_BYTES} bytes"
        )));
    }
    let pem = String::from_utf8(bytes)
        .map_err(|_| invalid("The private key must be the .p8 file from App Store Connect"))?;
    EncodingKey::from_ec_pem(pem.as_bytes())
        .map_err(|_| invalid("The private key must be the .p8 file from App Store Connect"))?;
    Ok(pem)
}

async fn ensure_name_free(
    pool: &SqlitePool,
    name: &str,
    except_id: Option<&str>,
) -> Result<(), ApiFailure> {
    let taken: Option<String> = sqlx::query_scalar(
        "SELECT id FROM app_store_connect_keys WHERE name = ?1 AND id IS NOT ?2",
    )
    .bind(name)
    .bind(except_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| store_error("Failed to check the key name", e))?;
    if taken.is_some() {
        return Err(api_err(
            StatusCode::CONFLICT,
            "key_exists",
            format!("A shared key named '{name}' already exists"),
        ));
    }
    Ok(())
}

async fn audit(pool: &SqlitePool, auth: &AuthUser, action: &str, id: &str, name: &str) {
    let details = serde_json::json!({ "name": name, "actor": &auth.0.email }).to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        action,
        "app_store_connect_key",
        Some(id),
        Some(&details),
    )
    .await;
}

/// `GET /v1/app-store-connect-keys` — list shared keys. Anyone who can read
/// pipelines can see them, so they can pick one in iOS signing.
pub async fn list_app_store_connect_keys(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<ListAppStoreConnectKeysResponse> {
    check_permission(&state.enforcer, &auth.0.role, "pipelines", "read").await?;
    let rows = sqlx::query_as::<_, KeyRow>(summary_query!("ORDER BY k.name COLLATE NOCASE"))
        .fetch_all(&state.db)
        .await
        .map_err(|e| store_error("Failed to list App Store Connect keys", e))?;
    Ok(Json(ListAppStoreConnectKeysResponse {
        keys: rows.into_iter().map(Into::into).collect(),
    }))
}

/// `POST /v1/app-store-connect-keys` — store a shared key.
pub async fn create_app_store_connect_key(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateAppStoreConnectKeyRequest>,
) -> Result<(StatusCode, Json<AppStoreConnectKeyResponse>), ApiFailure> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;
    let name = normalize_name(&req.name)?;
    let key_id = normalize_identifier(&req.key_id, "Key ID")?;
    let issuer_id = normalize_identifier(&req.issuer_id, "Issuer ID")?;
    let pem = parse_private_key(&req.private_key_base64)?;
    ensure_name_free(&state.db, &name, None).await?;

    let encrypted = crypto::encrypt(&pem, &state.encryption_key).map_err(|e| {
        error!(error = %e, "failed to encrypt App Store Connect key");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "encryption_error",
            "Failed to encrypt the private key",
        )
    })?;
    let id = Uuid::new_v4().to_string();
    let now = now_unix();
    sqlx::query(
        "INSERT INTO app_store_connect_keys (
            id, name, key_id, issuer_id, private_key_encrypted,
            created_by, updated_by, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?7)",
    )
    .bind(&id)
    .bind(&name)
    .bind(&key_id)
    .bind(&issuer_id)
    .bind(encrypted)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| store_error("Failed to save the App Store Connect key", e))?;

    audit(
        &state.db,
        &auth,
        "app_store_connect_key_created",
        &id,
        &name,
    )
    .await;
    info!(name = %name, key_id = %key_id, actor = %auth.0.email, "created shared App Store Connect key");

    let key = load_summary(&state.db, &id)
        .await
        .map_err(|e| store_error("Failed to load the App Store Connect key", e))?
        .ok_or_else(|| {
            store_error(
                "Failed to load the App Store Connect key",
                "key vanished after insert",
            )
        })?;
    Ok((
        StatusCode::CREATED,
        Json(AppStoreConnectKeyResponse { key }),
    ))
}

/// `PUT /v1/app-store-connect-keys/{id}` — rename or replace a shared key.
/// Every pipeline using it picks up the change on its next build.
pub async fn update_app_store_connect_key(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateAppStoreConnectKeyRequest>,
) -> ApiResult<AppStoreConnectKeyResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;
    let existing = load_summary(&state.db, &id)
        .await
        .map_err(|e| store_error("Failed to load the App Store Connect key", e))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Shared key not found"))?;

    let name = match req.name.as_deref() {
        Some(raw) => normalize_name(raw)?,
        None => existing.name.clone(),
    };
    let key_id = match req.key_id.as_deref() {
        Some(raw) => normalize_identifier(raw, "Key ID")?,
        None => existing.key_id.clone(),
    };
    let issuer_id = match req.issuer_id.as_deref() {
        Some(raw) => normalize_identifier(raw, "Issuer ID")?,
        None => existing.issuer_id.clone(),
    };
    let pem = req
        .private_key_base64
        .as_deref()
        .filter(|payload| !payload.trim().is_empty())
        .map(parse_private_key)
        .transpose()?;
    if key_id != existing.key_id && pem.is_none() {
        return Err(invalid("Upload the .p8 file for the new Key ID"));
    }
    if name != existing.name {
        ensure_name_free(&state.db, &name, Some(&id)).await?;
    }
    let encrypted = pem
        .map(|pem| crypto::encrypt(&pem, &state.encryption_key))
        .transpose()
        .map_err(|e| {
            error!(error = %e, "failed to encrypt App Store Connect key");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "encryption_error",
                "Failed to encrypt the private key",
            )
        })?;

    sqlx::query(
        "UPDATE app_store_connect_keys
         SET name = ?2, key_id = ?3, issuer_id = ?4,
             private_key_encrypted = COALESCE(?5, private_key_encrypted),
             updated_by = ?6, updated_at = ?7
         WHERE id = ?1",
    )
    .bind(&id)
    .bind(&name)
    .bind(&key_id)
    .bind(&issuer_id)
    .bind(encrypted)
    .bind(&auth.0.user_id)
    .bind(now_unix())
    .execute(&state.db)
    .await
    .map_err(|e| store_error("Failed to save the App Store Connect key", e))?;

    audit(
        &state.db,
        &auth,
        "app_store_connect_key_updated",
        &id,
        &name,
    )
    .await;
    info!(name = %name, key_id = %key_id, actor = %auth.0.email, "updated shared App Store Connect key");

    let key = load_summary(&state.db, &id)
        .await
        .map_err(|e| store_error("Failed to load the App Store Connect key", e))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Shared key not found"))?;
    Ok(Json(AppStoreConnectKeyResponse { key }))
}

/// `DELETE /v1/app-store-connect-keys/{id}` — remove a shared key that no
/// pipeline uses.
pub async fn delete_app_store_connect_key(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<OkResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;
    let existing = load_summary(&state.db, &id)
        .await
        .map_err(|e| store_error("Failed to load the App Store Connect key", e))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Shared key not found"))?;
    if existing.pipeline_count > 0 {
        let pipelines = if existing.pipeline_count == 1 {
            "1 pipeline uses".to_string()
        } else {
            format!("{} pipelines use", existing.pipeline_count)
        };
        return Err(api_err(
            StatusCode::CONFLICT,
            "key_in_use",
            format!(
                "{pipelines} this key. Switch their iOS signing to another key before deleting it."
            ),
        ));
    }
    sqlx::query("DELETE FROM app_store_connect_keys WHERE id = ?1")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(|e| store_error("Failed to delete the App Store Connect key", e))?;

    audit(
        &state.db,
        &auth,
        "app_store_connect_key_deleted",
        &id,
        &existing.name,
    )
    .await;
    info!(name = %existing.name, actor = %auth.0.email, "deleted shared App Store Connect key");
    Ok(Json(OkResponse { ok: true }))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const TEST_P8: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgeCUiGBVW/3dZCg5h
umW5ytSQL8G9CiXKj+OhPm7AF2ihRANCAARFoY+uIOtnMXDumzAlThCkSZBQSheA
u7FtNIKhMcPaYJ6YVeHr1zsO6PJMj5yCh1gv5ufc+cP0Yj4BubQIax4E
-----END PRIVATE KEY-----
";

    /// iOS signing settings and shared keys, without the rest of the schema.
    pub(crate) async fn signing_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE users (id TEXT PRIMARY KEY)",
            "CREATE TABLE pipelines (id TEXT PRIMARY KEY)",
            include_str!("../migrations/014_pipeline_ios_signing.sql"),
            include_str!("../migrations/055_app_store_connect_keys.sql"),
            "INSERT INTO pipelines VALUES ('pl1'), ('pl2')",
        ] {
            sqlx::raw_sql(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    pub(crate) async fn insert_shared_key(pool: &SqlitePool, encryption_key: &[u8], id: &str) {
        sqlx::query(
            "INSERT INTO app_store_connect_keys
                (id, name, key_id, issuer_id, private_key_encrypted, created_at, updated_at)
             VALUES (?1, ?1, 'SHARED1234', 'shared-issuer', ?2, 0, 0)",
        )
        .bind(id)
        .bind(crypto::encrypt("shared-pem", encryption_key).unwrap())
        .execute(pool)
        .await
        .unwrap();
    }

    pub(crate) async fn insert_signing(
        pool: &SqlitePool,
        encryption_key: &[u8],
        pipeline_id: &str,
        shared_key_id: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO pipeline_ios_signing_settings
                (id, pipeline_id, enabled, mode, export_method, api_key_id, api_issuer_id,
                 api_private_key_encrypted, shared_api_key_id, created_at, updated_at)
             VALUES (?1, ?1, 1, 'api', 'app_store', 'OWNKEY1234', 'own-issuer', ?2, ?3, 0, 0)",
        )
        .bind(pipeline_id)
        .bind(crypto::encrypt("own-pem", encryption_key).unwrap())
        .bind(shared_key_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[test]
    fn only_an_app_store_connect_p8_is_accepted() {
        let encode = |value: &str| base64::engine::general_purpose::STANDARD.encode(value);
        assert_eq!(parse_private_key(&encode(TEST_P8)).unwrap(), TEST_P8);
        assert!(parse_private_key("not base64!").is_err());
        assert!(parse_private_key(&encode("{\"type\": \"service_account\"}")).is_err());
        assert!(parse_private_key("").is_err());
    }

    #[test]
    fn identifiers_are_trimmed_and_checked() {
        assert_eq!(
            normalize_identifier(" ABC123DEF4 ", "Key ID").unwrap(),
            "ABC123DEF4"
        );
        assert!(normalize_identifier("abc def", "Key ID").is_err());
        assert!(normalize_name("   ").is_err());
    }

    #[tokio::test]
    async fn shared_key_counts_its_pipelines_and_resolves_its_credentials() {
        let pool = signing_pool().await;
        let encryption_key = vec![7u8; 32];
        insert_shared_key(&pool, &encryption_key, "team").await;
        insert_signing(&pool, &encryption_key, "pl1", Some("team")).await;
        insert_signing(&pool, &encryption_key, "pl2", None).await;

        let summary = load_summary(&pool, "team").await.unwrap().unwrap();
        assert_eq!(summary.pipeline_count, 1);
        assert_eq!(summary.key_id, "SHARED1234");

        let creds = load_credentials(&pool, &encryption_key, "team")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(creds.private_key_pem, "shared-pem");
        assert!(
            load_credentials(&pool, &encryption_key, "missing")
                .await
                .unwrap()
                .is_none()
        );

        // A key in use cannot be removed out from under its pipelines.
        assert!(
            sqlx::query("DELETE FROM app_store_connect_keys WHERE id = 'team'")
                .execute(&pool)
                .await
                .is_err()
        );
    }
}
