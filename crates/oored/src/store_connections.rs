//! Store connections per app.
//!
//! An app connects Google Play and App Store Connect once: the service
//! account secret or stored App Store Connect key, and the package or bundle
//! ID. Its pipelines then only pick a track. A pipeline can still keep its own
//! credentials (a named Play secret, or `upload_with_signing_key` for App
//! Store Connect); those are listed as overrides. Without a connection,
//! pipelines keep working with their own settings as before.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, AppStoreConnectionTestResponse, GooglePlayCredentialsTestResponse,
    ProjectAppStoreConnection, ProjectGooglePlayConnection, ProjectStoreOverride,
    ProjectStoresResponse, PublishStore, UpdateProjectAppStoreConnectionRequest,
    UpdateProjectGooglePlayConnectionRequest,
};
use sqlx::{FromRow, SqlitePool};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::app_store_connect::AppStoreConnect;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::secrets::{resolve_secret_value, secret_name_visible, visible_secret_version};
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiFailure = (StatusCode, Json<ApiError>);
type ApiResult<T> = Result<Json<T>, ApiFailure>;

const GOOGLE_PLAY: &str = "google_play";
const APP_STORE: &str = "app_store";

/// An app's Google Play connection, as uploads use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GooglePlayConnection {
    pub service_account_secret: String,
    pub package_name: Option<String>,
}

#[derive(Debug, FromRow)]
struct ConnectionRow {
    service_account_secret: Option<String>,
    app_store_connect_key_id: Option<String>,
    app_identifier: Option<String>,
    last_check: Option<String>,
    last_check_fingerprint: Option<String>,
    updated_at: i64,
}

async fn load_row(
    pool: &SqlitePool,
    project_id: &str,
    store: &str,
) -> sqlx::Result<Option<ConnectionRow>> {
    sqlx::query_as::<_, ConnectionRow>(
        "SELECT service_account_secret, app_store_connect_key_id, app_identifier, \
                last_check, last_check_fingerprint, updated_at \
         FROM project_store_connections WHERE project_id = ?1 AND store = ?2",
    )
    .bind(project_id)
    .bind(store)
    .fetch_optional(pool)
    .await
}

/// The app's Google Play connection.
pub(crate) async fn google_play_connection(
    pool: &SqlitePool,
    project_id: &str,
) -> sqlx::Result<Option<GooglePlayConnection>> {
    Ok(load_row(pool, project_id, GOOGLE_PLAY)
        .await?
        .and_then(|row| {
            Some(GooglePlayConnection {
                service_account_secret: row.service_account_secret?,
                package_name: row.app_identifier,
            })
        }))
}

/// The Google Play connection of the pipeline's app.
pub(crate) async fn pipeline_google_play_connection(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> sqlx::Result<Option<GooglePlayConnection>> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT c.service_account_secret, c.app_identifier \
         FROM project_store_connections c JOIN pipelines p ON p.project_id = c.project_id \
         WHERE p.id = ?1 AND c.store = 'google_play' AND c.service_account_secret IS NOT NULL",
    )
    .bind(pipeline_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(service_account_secret, package_name)| GooglePlayConnection {
            service_account_secret,
            package_name,
        },
    ))
}

fn store_error(context: &'static str, e: impl std::fmt::Display) -> ApiFailure {
    error!(error = %e, "{context}");
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", context)
}

fn invalid(code: &'static str, message: impl Into<String>) -> ApiFailure {
    api_err(StatusCode::BAD_REQUEST, code, message)
}

async fn require_project(
    state: &AppState,
    auth: &AuthUser,
    project_id: &str,
    permission: ProjectPermission,
) -> Result<(), ApiFailure> {
    let role = resolve_effective_project_role(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&role, ProjectPermission::Read)?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM projects WHERE id = ?1")
        .bind(project_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| store_error("Failed to load the app", e))?;
    if exists.is_none() {
        return Err(api_err(StatusCode::NOT_FOUND, "not_found", "App not found"));
    }
    require_project_permission(&role, permission)
}

/// An Android application ID such as `com.example.app`.
fn valid_package(package: &str) -> bool {
    package.len() <= 255
        && package.split('.').count() >= 2
        && package.split('.').all(|part| {
            part.chars()
                .next()
                .is_some_and(|first| first.is_ascii_alphabetic())
                && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

/// An Apple bundle ID such as `com.example.app`.
fn valid_bundle_id(bundle_id: &str) -> bool {
    !bundle_id.is_empty()
        && bundle_id.len() <= 155
        && bundle_id.contains('.')
        && bundle_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
}

fn optional_trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The fingerprint a Google Play check is pinned to: the version of the
/// secret the name resolves to in the app's scope.
async fn play_fingerprint(
    pool: &SqlitePool,
    project_id: &str,
    secret: &str,
) -> anyhow::Result<Option<String>> {
    visible_secret_version(pool, project_id, None, secret).await
}

/// The fingerprint an App Store Connect check is pinned to: the key and
/// when it last changed.
async fn key_fingerprint(pool: &SqlitePool, key_id: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT id || ':' || updated_at FROM app_store_connect_keys WHERE id = ?1")
        .bind(key_id)
        .fetch_optional(pool)
        .await
}

async fn overrides(
    pool: &SqlitePool,
    project_id: &str,
    play: Option<&str>,
    app_store: bool,
) -> sqlx::Result<Vec<ProjectStoreOverride>> {
    let mut found = Vec::new();
    if let Some(app_secret) = play {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT p.id, p.name, trim(json_extract(t.config, '$.service_account_secret')) \
             FROM pipeline_publish_targets t JOIN pipelines p ON p.id = t.pipeline_id \
             WHERE p.project_id = ?1 AND t.store = 'google_play' AND json_valid(t.config) \
               AND trim(COALESCE(json_extract(t.config, '$.service_account_secret'), '')) \
                   NOT IN ('', ?2) \
             ORDER BY p.name, p.id",
        )
        .bind(project_id)
        .bind(app_secret)
        .fetch_all(pool)
        .await?;
        found.extend(
            rows.into_iter().map(
                |(pipeline_id, pipeline_name, secret)| ProjectStoreOverride {
                    pipeline_id,
                    pipeline_name,
                    store: PublishStore::GooglePlay,
                    detail: format!("Uses the secret {secret}"),
                },
            ),
        );
    }
    if app_store {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT p.id, p.name \
             FROM pipeline_publish_targets t JOIN pipelines p ON p.id = t.pipeline_id \
             WHERE p.project_id = ?1 AND t.store = 'app_store' AND json_valid(t.config) \
               AND COALESCE(json_extract(t.config, '$.upload_with_signing_key'), 0) = 1 \
             ORDER BY p.name, p.id",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;
        found.extend(
            rows.into_iter()
                .map(|(pipeline_id, pipeline_name)| ProjectStoreOverride {
                    pipeline_id,
                    pipeline_name,
                    store: PublishStore::AppStore,
                    detail: "Uses the key in its iOS signing".to_string(),
                }),
        );
    }
    Ok(found)
}

async fn stores_response(
    pool: &SqlitePool,
    project_id: &str,
) -> anyhow::Result<ProjectStoresResponse> {
    let google_play = match load_row(pool, project_id, GOOGLE_PLAY).await? {
        Some(row) => {
            let secret = row.service_account_secret.clone().unwrap_or_default();
            let present = secret_name_visible(pool, project_id, None, &secret).await?;
            let fingerprint = play_fingerprint(pool, project_id, &secret).await?;
            let last_check = row
                .last_check
                .as_deref()
                .filter(|_| fingerprint.is_some() && row.last_check_fingerprint == fingerprint)
                .and_then(|raw| {
                    serde_json::from_str::<GooglePlayCredentialsTestResponse>(raw).ok()
                });
            Some(ProjectGooglePlayConnection {
                service_account_secret: secret,
                service_account_secret_present: present,
                package_name: row.app_identifier,
                last_check,
                updated_at: row.updated_at,
            })
        }
        None => None,
    };
    let app_store = match load_row(pool, project_id, APP_STORE).await? {
        Some(row) => {
            let key_id = row.app_store_connect_key_id.clone().unwrap_or_default();
            match crate::app_store_connect_keys::load_summary(pool, &key_id).await? {
                Some(key) => {
                    let fingerprint = key_fingerprint(pool, &key_id).await?;
                    let last_check = row
                        .last_check
                        .as_deref()
                        .filter(|_| row.last_check_fingerprint == fingerprint)
                        .and_then(|raw| {
                            serde_json::from_str::<AppStoreConnectionTestResponse>(raw).ok()
                        });
                    Some(ProjectAppStoreConnection {
                        key,
                        bundle_id: row.app_identifier,
                        last_check,
                        updated_at: row.updated_at,
                    })
                }
                None => None,
            }
        }
        None => None,
    };
    let overrides = overrides(
        pool,
        project_id,
        google_play
            .as_ref()
            .map(|connection| connection.service_account_secret.as_str()),
        app_store.is_some(),
    )
    .await?;
    Ok(ProjectStoresResponse {
        project_id: project_id.to_string(),
        google_play,
        app_store,
        overrides,
    })
}

async fn respond(state: &AppState, project_id: &str) -> ApiResult<ProjectStoresResponse> {
    stores_response(&state.db, project_id)
        .await
        .map(Json)
        .map_err(|e| store_error("Failed to load the app's stores", e))
}

async fn audit(
    state: &AppState,
    auth: &AuthUser,
    action: &str,
    project_id: &str,
    details: serde_json::Value,
) {
    let details = details.to_string();
    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        action,
        "project",
        Some(project_id),
        Some(&details),
    )
    .await;
}

/// Save one connection. A changed credential or app forgets the last test.
#[allow(clippy::too_many_arguments)]
async fn save_connection(
    pool: &SqlitePool,
    project_id: &str,
    store: &str,
    service_account_secret: Option<&str>,
    app_store_connect_key_id: Option<&str>,
    app_identifier: Option<&str>,
    user_id: &str,
) -> sqlx::Result<()> {
    let now = now_unix();
    sqlx::query(
        "INSERT INTO project_store_connections ( \
            id, project_id, store, service_account_secret, app_store_connect_key_id, \
            app_identifier, created_by, updated_by, created_at, updated_at \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, ?8) \
         ON CONFLICT (project_id, store) DO UPDATE SET \
            last_check = CASE WHEN service_account_secret IS excluded.service_account_secret \
                               AND app_store_connect_key_id IS excluded.app_store_connect_key_id \
                               AND app_identifier IS excluded.app_identifier \
                              THEN last_check END, \
            last_check_fingerprint = CASE WHEN service_account_secret IS excluded.service_account_secret \
                               AND app_store_connect_key_id IS excluded.app_store_connect_key_id \
                               AND app_identifier IS excluded.app_identifier \
                              THEN last_check_fingerprint END, \
            service_account_secret = excluded.service_account_secret, \
            app_store_connect_key_id = excluded.app_store_connect_key_id, \
            app_identifier = excluded.app_identifier, \
            updated_by = excluded.updated_by, \
            updated_at = excluded.updated_at",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(project_id)
    .bind(store)
    .bind(service_account_secret)
    .bind(app_store_connect_key_id)
    .bind(app_identifier)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Keep a test result on the saved connection when it tested what is saved.
async fn remember_check(
    pool: &SqlitePool,
    project_id: &str,
    store: &str,
    credential: &str,
    app_identifier: Option<&str>,
    fingerprint: Option<&str>,
    check: &str,
) {
    let Some(fingerprint) = fingerprint else {
        return;
    };
    let result = sqlx::query(
        "UPDATE project_store_connections SET last_check = ?1, last_check_fingerprint = ?2 \
         WHERE project_id = ?3 AND store = ?4 \
           AND COALESCE(service_account_secret, app_store_connect_key_id) = ?5 \
           AND app_identifier IS ?6",
    )
    .bind(check)
    .bind(fingerprint)
    .bind(project_id)
    .bind(store)
    .bind(credential)
    .bind(app_identifier)
    .execute(pool)
    .await;
    if let Err(error) = result {
        warn!(%error, project_id, store, "failed to save the store connection test");
    }
}

/// `GET /v1/projects/{project_id}/stores`
pub async fn get_project_stores(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> ApiResult<ProjectStoresResponse> {
    require_project(&state, &auth, &project_id, ProjectPermission::Read).await?;
    respond(&state, &project_id).await
}

/// `PUT /v1/projects/{project_id}/stores/google-play`
pub async fn update_project_google_play_connection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(request): Json<UpdateProjectGooglePlayConnectionRequest>,
) -> ApiResult<ProjectStoresResponse> {
    require_project(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    let secret = request.service_account_secret.trim();
    if let Err(error) = oore_contract::validate_secret_name(secret) {
        return Err(invalid(
            "invalid_secret_name",
            format!("service_account_secret {error}"),
        ));
    }
    let package = optional_trimmed(request.package_name.as_deref());
    if package
        .as_deref()
        .is_some_and(|package| !valid_package(package))
    {
        return Err(invalid(
            "invalid_package_name",
            "package_name must be an Android application id such as com.example.app",
        ));
    }
    save_connection(
        &state.db,
        &project_id,
        GOOGLE_PLAY,
        Some(secret),
        None,
        package.as_deref(),
        &auth.0.user_id,
    )
    .await
    .map_err(|e| store_error("Failed to save the Google Play connection", e))?;
    audit(
        &state,
        &auth,
        "project_store_connected",
        &project_id,
        serde_json::json!({
            "store": GOOGLE_PLAY,
            "service_account_secret": secret,
            "package_name": package,
            "updated_by": &auth.0.email,
        }),
    )
    .await;
    info!(project_id = %project_id, actor = %auth.0.email, "connected Google Play");
    respond(&state, &project_id).await
}

/// `DELETE /v1/projects/{project_id}/stores/google-play`
pub async fn delete_project_google_play_connection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> ApiResult<ProjectStoresResponse> {
    require_project(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    let dependents: Vec<String> = sqlx::query_scalar(
        "SELECT p.name FROM pipeline_publish_targets t JOIN pipelines p ON p.id = t.pipeline_id \
         WHERE p.project_id = ?1 AND t.store = 'google_play' AND t.enabled = 1 \
           AND json_valid(t.config) \
           AND trim(COALESCE(json_extract(t.config, '$.service_account_secret'), '')) = '' \
         ORDER BY p.name",
    )
    .bind(&project_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| store_error("Failed to disconnect Google Play", e))?;
    if !dependents.is_empty() {
        return Err(api_err(
            StatusCode::CONFLICT,
            "store_connection_in_use",
            format!(
                "{} upload to Google Play with this connection. Turn their uploads off first.",
                dependents.join(", ")
            ),
        ));
    }
    sqlx::query("DELETE FROM project_store_connections WHERE project_id = ?1 AND store = ?2")
        .bind(&project_id)
        .bind(GOOGLE_PLAY)
        .execute(&state.db)
        .await
        .map_err(|e| store_error("Failed to disconnect Google Play", e))?;
    audit(
        &state,
        &auth,
        "project_store_disconnected",
        &project_id,
        serde_json::json!({ "store": GOOGLE_PLAY, "updated_by": &auth.0.email }),
    )
    .await;
    respond(&state, &project_id).await
}

/// `POST /v1/projects/{project_id}/stores/google-play/test`
pub async fn test_project_google_play_connection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(request): Json<UpdateProjectGooglePlayConnectionRequest>,
) -> ApiResult<GooglePlayCredentialsTestResponse> {
    require_project(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    let secret = request.service_account_secret.trim();
    if let Err(error) = oore_contract::validate_secret_name(secret) {
        return Err(invalid(
            "invalid_secret_name",
            format!("service_account_secret {error}"),
        ));
    }
    let requested_package = optional_trimmed(request.package_name.as_deref());
    let package_name = match requested_package.clone() {
        Some(package) => Some(package),
        None => sqlx::query_scalar(
            "SELECT package_name FROM releases \
             WHERE project_id = ?1 AND store = 'google_play' AND package_name IS NOT NULL \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(&project_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| store_error("Failed to load the app's package", e))?,
    };
    let fingerprint = play_fingerprint(&state.db, &project_id, secret)
        .await
        .map_err(|e| store_error("Failed to read the service account secret", e))?;
    let service_account =
        resolve_secret_value(&state.db, &state.encryption_key, &project_id, None, secret)
            .await
            .map_err(|e| store_error("Failed to read the service account secret", e))?
            .map(zeroize::Zeroizing::new);
    let http = reqwest::Client::builder()
        .timeout(crate::pipeline_publish::PLAY_CHECK_TIMEOUT)
        .build()
        .map_err(|e| store_error("Failed to start the Google Play check", e))?;
    let outcome = crate::pipeline_publish::check_play_credentials(
        &http,
        crate::pipeline_publish::PLAY_API_BASE,
        secret,
        service_account.as_ref().map(|value| value.as_str()),
        package_name.as_deref(),
    )
    .await;
    let response = GooglePlayCredentialsTestResponse {
        ok: outcome.ok,
        service_account_secret: secret.to_string(),
        message: outcome.message,
        package_name,
        service_account_email: outcome.service_account_email,
        checked_at: now_unix(),
    };
    if let Ok(check) = serde_json::to_string(&response) {
        remember_check(
            &state.db,
            &project_id,
            GOOGLE_PLAY,
            secret,
            requested_package.as_deref(),
            fingerprint.as_deref(),
            &check,
        )
        .await;
    }
    audit(
        &state,
        &auth,
        "project_store_tested",
        &project_id,
        serde_json::json!({
            "store": GOOGLE_PLAY,
            "service_account_secret": secret,
            "package_name": &response.package_name,
            "ok": response.ok,
            "checked_by": &auth.0.email,
        }),
    )
    .await;
    Ok(Json(response))
}

async fn require_key(state: &AppState, key_id: &str) -> Result<(), ApiFailure> {
    let exists = crate::app_store_connect_keys::load_summary(&state.db, key_id)
        .await
        .map_err(|e| store_error("Failed to load the App Store Connect key", e))?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(invalid(
            "invalid_api_key",
            "The selected App Store Connect key no longer exists",
        ))
    }
}

fn normalize_app_store_request(
    request: &UpdateProjectAppStoreConnectionRequest,
) -> Result<(String, Option<String>), ApiFailure> {
    let key_id = request.app_store_connect_key_id.trim().to_string();
    if key_id.is_empty() {
        return Err(invalid(
            "invalid_api_key",
            "app_store_connect_key_id is required",
        ));
    }
    let bundle_id = optional_trimmed(request.bundle_id.as_deref());
    if bundle_id
        .as_deref()
        .is_some_and(|bundle| !valid_bundle_id(bundle))
    {
        return Err(invalid(
            "invalid_bundle_id",
            "bundle_id must be a bundle ID such as com.example.app",
        ));
    }
    Ok((key_id, bundle_id))
}

/// `PUT /v1/projects/{project_id}/stores/app-store`
pub async fn update_project_app_store_connection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(request): Json<UpdateProjectAppStoreConnectionRequest>,
) -> ApiResult<ProjectStoresResponse> {
    require_project(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    let (key_id, bundle_id) = normalize_app_store_request(&request)?;
    require_key(&state, &key_id).await?;
    save_connection(
        &state.db,
        &project_id,
        APP_STORE,
        None,
        Some(&key_id),
        bundle_id.as_deref(),
        &auth.0.user_id,
    )
    .await
    .map_err(|e| store_error("Failed to save the App Store Connect connection", e))?;
    audit(
        &state,
        &auth,
        "project_store_connected",
        &project_id,
        serde_json::json!({
            "store": APP_STORE,
            "app_store_connect_key_id": key_id,
            "bundle_id": bundle_id,
            "updated_by": &auth.0.email,
        }),
    )
    .await;
    info!(project_id = %project_id, actor = %auth.0.email, "connected App Store Connect");
    respond(&state, &project_id).await
}

/// `DELETE /v1/projects/{project_id}/stores/app-store`
pub async fn delete_project_app_store_connection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> ApiResult<ProjectStoresResponse> {
    require_project(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    sqlx::query("DELETE FROM project_store_connections WHERE project_id = ?1 AND store = ?2")
        .bind(&project_id)
        .bind(APP_STORE)
        .execute(&state.db)
        .await
        .map_err(|e| store_error("Failed to disconnect App Store Connect", e))?;
    audit(
        &state,
        &auth,
        "project_store_disconnected",
        &project_id,
        serde_json::json!({ "store": APP_STORE, "updated_by": &auth.0.email }),
    )
    .await;
    respond(&state, &project_id).await
}

/// `POST /v1/projects/{project_id}/stores/app-store/test`
pub async fn test_project_app_store_connection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(request): Json<UpdateProjectAppStoreConnectionRequest>,
) -> ApiResult<AppStoreConnectionTestResponse> {
    require_project(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    let (key_id, bundle_id) = normalize_app_store_request(&request)?;
    let credentials =
        crate::app_store_connect_keys::load_credentials(&state.db, &state.encryption_key, &key_id)
            .await
            .map_err(|e| store_error("Failed to load the App Store Connect key", e))?
            .ok_or_else(|| {
                invalid(
                    "invalid_api_key",
                    "The selected App Store Connect key no longer exists",
                )
            })?;
    let message_and_ok = match bundle_id.as_deref() {
        None => (
            false,
            "Oore does not know which app to check yet. Set the bundle ID, then test again."
                .to_string(),
        ),
        Some(bundle) => {
            let http = crate::app_store_publish::app_store_http_client();
            match AppStoreConnect::new(&http, &credentials)
                .find_app_id(bundle)
                .await
            {
                Ok(Some(_)) => (
                    true,
                    format!(
                        "Key {} can reach {bundle} in App Store Connect.",
                        credentials.key_id
                    ),
                ),
                Ok(None) => (
                    false,
                    format!(
                        "App Store Connect has no app with the bundle ID {bundle} for key {}. Create the app record or check the bundle ID.",
                        credentials.key_id
                    ),
                ),
                Err(error) => (false, error.to_string()),
            }
        }
    };
    let response = AppStoreConnectionTestResponse {
        ok: message_and_ok.0,
        message: message_and_ok.1,
        bundle_id: bundle_id.clone(),
        checked_at: now_unix(),
    };
    let fingerprint = key_fingerprint(&state.db, &key_id)
        .await
        .map_err(|e| store_error("Failed to load the App Store Connect key", e))?;
    if let Ok(check) = serde_json::to_string(&response) {
        remember_check(
            &state.db,
            &project_id,
            APP_STORE,
            &key_id,
            bundle_id.as_deref(),
            fingerprint.as_deref(),
            &check,
        )
        .await;
    }
    audit(
        &state,
        &auth,
        "project_store_tested",
        &project_id,
        serde_json::json!({
            "store": APP_STORE,
            "bundle_id": bundle_id,
            "ok": response.ok,
            "checked_by": &auth.0.email,
        }),
    )
    .await;
    Ok(Json(response))
}

#[cfg(test)]
mod tests;
