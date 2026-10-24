//! App Store and TestFlight publishing.
//!
//! The runner uploads the signed App Store IPA with `altool` and the
//! pipeline's App Store Connect API key (its own or a shared one), then
//! reports the outcome. From there this module owns the `app_store` release:
//! a daemon task polls App Store Connect until the build has processed, hands
//! it to TestFlight groups or an App Store version, follows review, and
//! mirrors Apple's phased release.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, AppStoreDestination, AppStorePublishConfig, AppStoreReleaseNote, IosExportMethod,
    PipelineAppStorePublishResponse, Release, ReleasePolicy, ReleaseState, ReleaseStore,
    RunnerAppStorePublishPlan, RunnerAppStoreUploadCredentials, RunnerPublishResult,
};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};
use tracing::{error, info, warn};

use crate::AppState;
use crate::app_store_connect::{
    AppStoreConnect, AscApi, AscBetaDetail, AscError, BuildProcessing, phased_release_fraction,
    version_is_editable,
};
use crate::apple_api::AppleApiCredentials;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::project_rbac::{ProjectPermission, require_pipeline_project_permission};
use crate::releases::{
    self, BoxFuture, NewRelease, Promoted, PromotionRefused, PromotionTarget, ReleaseActor,
    ReleaseChange, ReleaseDeps, ReleaseError, ReleaseStoreClient, StoreChange,
};
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

const STORE: &str = "app_store";
const MAX_GROUPS: usize = 20;
const MAX_GROUP_NAME_LEN: usize = 128;
const MAX_NOTES: usize = 40;
const MAX_LOCALE_LEN: usize = 16;
/// App Store Connect limits both "What to Test" and "What's New" to 4000 characters.
const MAX_WHATS_NEW_CHARS: usize = 4000;
const MONITOR_INTERVAL: Duration = Duration::from_secs(60);
/// Apple usually lists an upload within minutes; give up after three hours.
const BUILD_DISCOVERY_TIMEOUT_SECS: i64 = 3 * 60 * 60;
const POLL_PROCESSING_SECS: i64 = 60;
const POLL_REVIEW_SECS: i64 = 5 * 60;
const POLL_LIVE_SECS: i64 = 15 * 60;
const POLL_ERROR_SECS: i64 = 5 * 60;
/// How long App Store Connect may keep failing for a release before Oore
/// gives up on it and marks it failed.
const GIVE_UP_AFTER_SECS: i64 = 24 * 60 * 60;

// ── Settings ────────────────────────────────────────────────────

fn is_valid_locale(locale: &str) -> bool {
    !locale.is_empty()
        && locale.len() <= MAX_LOCALE_LEN
        && locale
            .split('-')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Trims and checks a config before it is stored. Returns every problem so
/// the form can show them together.
pub(crate) fn normalize_config(
    mut config: AppStorePublishConfig,
) -> Result<AppStorePublishConfig, Vec<String>> {
    let mut errors = Vec::new();

    let mut groups: Vec<String> = Vec::new();
    for group in &config.testflight_groups {
        let group = group.trim();
        if group.is_empty() {
            continue;
        }
        if group.chars().count() > MAX_GROUP_NAME_LEN {
            errors.push(format!("TestFlight group '{group}' is too long"));
        } else if !groups.iter().any(|existing| existing == group) {
            groups.push(group.to_string());
        }
    }
    if groups.len() > MAX_GROUPS {
        errors.push(format!(
            "At most {MAX_GROUPS} TestFlight groups are allowed"
        ));
    }
    if config.enabled && config.destination == AppStoreDestination::Testflight && groups.is_empty()
    {
        errors.push("TestFlight publishing needs at least one group".to_string());
    }
    config.testflight_groups = groups;

    let mut notes: Vec<AppStoreReleaseNote> = Vec::new();
    for note in &config.release_notes {
        let locale = note.locale.trim();
        let whats_new = note.whats_new.trim();
        if locale.is_empty() && whats_new.is_empty() {
            continue;
        }
        if !is_valid_locale(locale) {
            errors.push(format!(
                "'{locale}' is not an App Store Connect locale such as en-US"
            ));
            continue;
        }
        if whats_new.chars().count() > MAX_WHATS_NEW_CHARS {
            errors.push(format!(
                "Release notes for {locale} are longer than {MAX_WHATS_NEW_CHARS} characters"
            ));
        }
        if notes.iter().any(|existing| existing.locale == locale) {
            errors.push(format!("Release notes for {locale} are listed twice"));
            continue;
        }
        notes.push(AppStoreReleaseNote {
            locale: locale.to_string(),
            whats_new: whats_new.to_string(),
        });
    }
    if notes.len() > MAX_NOTES {
        errors.push(format!(
            "At most {MAX_NOTES} release note locales are allowed"
        ));
    }
    config.release_notes = notes;

    if config.destination == AppStoreDestination::Testflight {
        config.submit_for_review = false;
        config.phased_release = false;
    }

    if errors.is_empty() {
        Ok(config)
    } else {
        Err(errors)
    }
}

#[derive(Debug, FromRow)]
struct PublishTargetRow {
    enabled: i64,
    config: String,
    updated_at: i64,
}

async fn load_config(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> sqlx::Result<Option<(AppStorePublishConfig, i64)>> {
    let row = sqlx::query_as::<_, PublishTargetRow>(
        "SELECT enabled, config, updated_at FROM pipeline_publish_targets
         WHERE pipeline_id = ?1 AND store = ?2",
    )
    .bind(pipeline_id)
    .bind(STORE)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| {
        let mut config: AppStorePublishConfig =
            serde_json::from_str(&row.config).unwrap_or_default();
        config.enabled = row.enabled != 0;
        (config, row.updated_at)
    }))
}

#[derive(Debug, FromRow)]
struct SigningKeyRow {
    enabled: i64,
    export_method: String,
    api_key_id: Option<String>,
    api_issuer_id: Option<String>,
    api_private_key_encrypted: Option<String>,
}

/// The pipeline's signing settings with the API key it uses: the shared key
/// when one is selected, otherwise its own.
async fn load_signing_key_row(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> sqlx::Result<Option<SigningKeyRow>> {
    sqlx::query_as::<_, SigningKeyRow>(
        "SELECT s.enabled, s.export_method,
                CASE WHEN k.id IS NULL THEN s.api_key_id ELSE k.key_id END AS api_key_id,
                CASE WHEN k.id IS NULL THEN s.api_issuer_id ELSE k.issuer_id END AS api_issuer_id,
                CASE WHEN k.id IS NULL THEN s.api_private_key_encrypted
                     ELSE k.private_key_encrypted END AS api_private_key_encrypted
         FROM pipeline_ios_signing_settings s
         LEFT JOIN app_store_connect_keys k ON k.id = s.shared_api_key_id
         WHERE s.pipeline_id = ?1",
    )
    .bind(pipeline_id)
    .fetch_optional(pool)
    .await
}

fn decrypt_credentials(
    row: &SigningKeyRow,
    encryption_key: &[u8],
) -> anyhow::Result<Option<AppleApiCredentials>> {
    let (Some(key_id), Some(issuer_id), Some(encrypted)) = (
        row.api_key_id.clone(),
        row.api_issuer_id.clone(),
        row.api_private_key_encrypted.as_deref(),
    ) else {
        return Ok(None);
    };
    Ok(Some(AppleApiCredentials {
        key_id,
        issuer_id,
        private_key_pem: crypto::decrypt(encrypted, encryption_key)?,
    }))
}

async fn build_response(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> sqlx::Result<PipelineAppStorePublishResponse> {
    let stored = load_config(pool, pipeline_id).await?;
    let signing = load_signing_key_row(pool, pipeline_id).await?;
    let (config, updated_at) = match stored {
        Some((config, updated_at)) => (config, Some(updated_at)),
        None => (AppStorePublishConfig::default(), None),
    };
    Ok(PipelineAppStorePublishResponse {
        pipeline_id: pipeline_id.to_string(),
        config,
        has_api_key: signing
            .as_ref()
            .is_some_and(|row| row.api_private_key_encrypted.is_some()),
        app_store_export: signing.as_ref().is_some_and(|row| {
            IosExportMethod::from_stored(&row.export_method) == IosExportMethod::AppStore
        }),
        updated_at,
    })
}

fn store_error(error: sqlx::Error) -> (StatusCode, Json<ApiError>) {
    error!(%error, "App Store publish settings store error");
    api_err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "store_error",
        "Failed to load App Store publish settings",
    )
}

async fn ensure_pipeline_exists(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM pipelines WHERE id = ?1")
        .bind(pipeline_id)
        .fetch_optional(pool)
        .await
        .map_err(store_error)?;
    exists
        .map(|_| ())
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Pipeline not found"))
}

pub async fn get_pipeline_app_store_publish(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<PipelineAppStorePublishResponse> {
    require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::Read,
    )
    .await?;
    ensure_pipeline_exists(&state.db, &pipeline_id).await?;
    Ok(Json(
        build_response(&state.db, &pipeline_id)
            .await
            .map_err(store_error)?,
    ))
}

pub async fn update_pipeline_app_store_publish(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
    Json(request): Json<AppStorePublishConfig>,
) -> ApiResult<PipelineAppStorePublishResponse> {
    require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    ensure_pipeline_exists(&state.db, &pipeline_id).await?;

    let config = normalize_config(request).map_err(|errors| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_publish_config",
            errors.join("; "),
        )
    })?;
    if config.enabled {
        let signing = load_signing_key_row(&state.db, &pipeline_id)
            .await
            .map_err(store_error)?;
        let Some(signing) = signing.filter(|row| row.enabled != 0) else {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_publish_config",
                "Turn on iOS signing for this pipeline before publishing to App Store Connect",
            ));
        };
        if IosExportMethod::from_stored(&signing.export_method) != IosExportMethod::AppStore {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_publish_config",
                "Set the iOS export method to App Store before publishing to App Store Connect",
            ));
        }
        if signing.api_private_key_encrypted.is_none() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_publish_config",
                "Add an App Store Connect API key in iOS signing before publishing",
            ));
        }
    }

    let now = now_unix();
    let config_json = serde_json::to_string(&config).map_err(|error| {
        error!(%error, "failed to serialize App Store publish config");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to save App Store publish settings",
        )
    })?;
    sqlx::query(
        "INSERT INTO pipeline_publish_targets (
            id, pipeline_id, store, enabled, config, created_by, updated_by, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?7)
         ON CONFLICT(pipeline_id, store) DO UPDATE SET
            enabled = excluded.enabled,
            config = excluded.config,
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&pipeline_id)
    .bind(STORE)
    .bind(i64::from(config.enabled))
    .bind(&config_json)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(store_error)?;

    let details = serde_json::json!({
        "pipeline_id": &pipeline_id,
        "enabled": config.enabled,
        "destination": config.destination.as_str(),
        "testflight_groups": &config.testflight_groups,
        "submit_for_review": config.submit_for_review,
        "phased_release": config.phased_release,
    })
    .to_string();
    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        "pipeline_app_store_publish_updated",
        "pipeline",
        Some(&pipeline_id),
        Some(&details),
    )
    .await;

    Ok(Json(
        build_response(&state.db, &pipeline_id)
            .await
            .map_err(store_error)?,
    ))
}

/// The App Store part of the runner's publish plan, or `None` when the job
/// should not upload. Credentials are only released for App Store exports.
pub(crate) async fn runner_plan(
    pool: &SqlitePool,
    encryption_key: &[u8],
    pipeline_id: &str,
) -> anyhow::Result<Option<RunnerAppStorePublishPlan>> {
    let Some((config, _)) = load_config(pool, pipeline_id).await? else {
        return Ok(None);
    };
    if !config.enabled {
        return Ok(None);
    }
    let Some(signing) = load_signing_key_row(pool, pipeline_id).await? else {
        return Ok(None);
    };
    if signing.enabled == 0
        || IosExportMethod::from_stored(&signing.export_method) != IosExportMethod::AppStore
    {
        return Ok(None);
    }
    let Some(credentials) = decrypt_credentials(&signing, encryption_key)? else {
        anyhow::bail!(
            "App Store publishing is on but the pipeline has no App Store Connect API key"
        );
    };
    Ok(Some(RunnerAppStorePublishPlan {
        config,
        credentials: RunnerAppStoreUploadCredentials {
            key_id: credentials.key_id,
            issuer_id: credentials.issuer_id,
            private_key_pem: credentials.private_key_pem,
        },
    }))
}

/// Whether builds of this pipeline upload to App Store Connect: publishing is
/// on and the pipeline signs App Store exports.
pub(crate) async fn publishing_enabled(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> anyhow::Result<bool> {
    let enabled = load_config(pool, pipeline_id)
        .await?
        .is_some_and(|(config, _)| config.enabled);
    if !enabled {
        return Ok(false);
    }
    Ok(load_signing_key_row(pool, pipeline_id)
        .await?
        .is_some_and(|signing| {
            signing.enabled != 0
                && IosExportMethod::from_stored(&signing.export_method) == IosExportMethod::AppStore
        }))
}

// ── Upload outcome ──────────────────────────────────────────────

/// App identity of the build's IPA, read from the metadata the runner
/// attached when it signed the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
struct UploadedIpa {
    artifact_id: String,
    bundle_id: String,
    version: String,
    build_number: String,
}

fn uploaded_ipa_from_metadata(artifact_id: String, metadata: &Value) -> Option<UploadedIpa> {
    let export_method = metadata
        .pointer("/ios_signing/effective_export_method")
        .and_then(Value::as_str)?;
    if !matches!(export_method, "app-store-connect" | "app-store") {
        return None;
    }
    let field = |key: &str| {
        metadata
            .pointer(&format!("/ios_app/{key}"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some(UploadedIpa {
        artifact_id,
        bundle_id: field("bundle_identifier")?,
        version: field("version")?,
        build_number: field("build_number")?,
    })
}

/// The build's signed App Store IPA artifact, if it has one.
pub(crate) async fn uploaded_ipa_artifact_id(
    pool: &SqlitePool,
    build_id: &str,
) -> anyhow::Result<Option<String>> {
    Ok(find_uploaded_ipa(pool, build_id)
        .await?
        .map(|ipa| ipa.artifact_id))
}

async fn find_uploaded_ipa(
    pool: &SqlitePool,
    build_id: &str,
) -> anyhow::Result<Option<UploadedIpa>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, metadata FROM artifacts
         WHERE build_id = ?1 AND artifact_type = 'ipa' AND state = 'available'
         ORDER BY created_at DESC",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().find_map(|(id, metadata)| {
        let metadata: Value = serde_json::from_str(&metadata).ok()?;
        uploaded_ipa_from_metadata(id, &metadata)
    }))
}

async fn load_uploaded_ipa(pool: &SqlitePool, build_id: &str) -> anyhow::Result<UploadedIpa> {
    find_uploaded_ipa(pool, build_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("build has no App Store IPA artifact"))
}

/// Records a successful upload as an `app_store` release in `processing`,
/// unless this build already has one. The release and the monitor link commit
/// together, so the monitor always picks it up. Returns the new release's id.
pub(crate) async fn record_upload(
    deps: ReleaseDeps<'_>,
    build_id: &str,
    reported: &RunnerPublishResult,
) -> anyhow::Result<Option<String>> {
    let (project_id, pipeline_id): (String, String) =
        sqlx::query_as("SELECT project_id, pipeline_id FROM builds WHERE id = ?1")
            .bind(build_id)
            .fetch_one(deps.pool)
            .await?;
    let (config, _) = load_config(deps.pool, &pipeline_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("pipeline has no App Store publish settings"))?;
    let config = uploaded_config(config, reported);
    let ipa = load_uploaded_ipa(deps.pool, build_id).await?;
    let track = settings_track(&config);
    let config_json = serde_json::to_string(&config)?;

    let mut tx = deps.pool.begin_with("BEGIN IMMEDIATE").await?;
    if releases::find_build_release(&mut tx, build_id, ReleaseStore::AppStore)
        .await?
        .is_some()
    {
        return Ok(None);
    }
    let created = releases::insert_release(
        &mut tx,
        NewRelease {
            project_id,
            build_id: build_id.to_string(),
            artifact_id: Some(ipa.artifact_id.clone()),
            store: ReleaseStore::AppStore,
            track,
            package_name: Some(ipa.bundle_id.clone()),
            version_name: Some(ipa.version.clone()),
            version_code: ipa.build_number.parse().ok(),
            state: ReleaseState::Processing,
            rollout_fraction: 0.0,
            policy: ReleasePolicy::default(),
            environment_id: None,
            promoted_from_release_id: None,
        },
        &ReleaseActor::System,
        Some("Uploaded to App Store Connect; waiting for Apple to process the build"),
    )
    .await
    .map_err(|error| anyhow::anyhow!("failed to create App Store release: {error:?}"))?;
    insert_release_link(
        &mut tx,
        &created.release.id,
        &pipeline_id,
        &ipa,
        &config_json,
    )
    .await?;
    tx.commit().await?;

    let release = releases::finish_create(deps, created, &ReleaseActor::System).await;
    info!(
        build_id,
        release_id = %release.id,
        bundle_id = %ipa.bundle_id,
        version = %ipa.version,
        build_number = %ipa.build_number,
        "recorded App Store Connect upload"
    );
    Ok(Some(release.id))
}

/// The release track the settings produce: the TestFlight groups, or
/// `app_store`.
fn settings_track(config: &AppStorePublishConfig) -> String {
    match config.destination {
        AppStoreDestination::Testflight => config.testflight_groups.join(", "),
        AppStoreDestination::AppStore => "app_store".to_string(),
    }
}

/// The settings an upload went out with. The runner reports the TestFlight
/// groups it uploaded for, which a `.oore.yaml` `publish.testflight` may
/// have chosen and the saved settings may no longer list, so the release and
/// its monitor use them. A runner that predates `testflight_groups` reports
/// only the track; a track the saved settings don't produce is read as its
/// groups. Credentials and release notes stay the saved ones.
fn uploaded_config(
    mut config: AppStorePublishConfig,
    reported: &RunnerPublishResult,
) -> AppStorePublishConfig {
    let groups: Vec<String> = match &reported.testflight_groups {
        Some(groups) => groups
            .iter()
            .map(|group| group.trim().to_string())
            .filter(|group| !group.is_empty())
            .collect(),
        None => {
            let track = reported.track.trim();
            if track.is_empty() || track == "app_store" || track == settings_track(&config) {
                return config;
            }
            track
                .split(", ")
                .map(str::trim)
                .filter(|group| !group.is_empty())
                .map(str::to_string)
                .collect()
        }
    };
    if groups.is_empty() {
        return config;
    }
    config.destination = AppStoreDestination::Testflight;
    config.testflight_groups = groups;
    config.submit_for_review = false;
    config.phased_release = false;
    config
}

async fn insert_release_link(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    release_id: &str,
    pipeline_id: &str,
    ipa: &UploadedIpa,
    config_json: &str,
) -> Result<(), sqlx::Error> {
    let now = now_unix();
    sqlx::query(
        "INSERT INTO app_store_release_links (
            release_id, pipeline_id, bundle_id, version, build_number, config,
            stage, next_poll_at, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'awaiting_build', ?7, ?7, ?7)",
    )
    .bind(release_id)
    .bind(pipeline_id)
    .bind(&ipa.bundle_id)
    .bind(&ipa.version)
    .bind(&ipa.build_number)
    .bind(config_json)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ── Release monitor ─────────────────────────────────────────────

/// Where an `app_store` release is in Oore's flow. Stored in
/// `app_store_release_links.stage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    AwaitingBuild,
    Distribute,
    BetaReview,
    Review,
    Live,
    Done,
}

impl Stage {
    fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingBuild => "awaiting_build",
            Self::Distribute => "distribute",
            Self::BetaReview => "beta_review",
            Self::Review => "review",
            Self::Live => "live",
            Self::Done => "done",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "distribute" => Self::Distribute,
            "beta_review" => Self::BetaReview,
            "review" => Self::Review,
            "live" => Self::Live,
            "done" => Self::Done,
            _ => Self::AwaitingBuild,
        }
    }
}

/// Release state implied by an App Store version state while it is in
/// review. `None` means the version went live and phased release decides.
fn review_outcome(version_state: &str) -> Option<(ReleaseState, &'static str)> {
    match version_state {
        "READY_FOR_SALE" | "READY_FOR_DISTRIBUTION" => None,
        "PREPARE_FOR_SUBMISSION" | "READY_FOR_REVIEW" => Some((
            ReleaseState::InReview,
            "Build attached; submit the version for review in App Store Connect",
        )),
        "PENDING_DEVELOPER_RELEASE" => Some((
            ReleaseState::InReview,
            "Approved by Apple; release the version in App Store Connect",
        )),
        "REJECTED" | "METADATA_REJECTED" | "INVALID_BINARY" => {
            Some((ReleaseState::Failed, "Rejected in App Store review"))
        }
        "DEVELOPER_REJECTED" => Some((
            ReleaseState::Failed,
            "Withdrawn from review in App Store Connect",
        )),
        "REMOVED_FROM_SALE" | "DEVELOPER_REMOVED_FROM_SALE" | "REPLACED_WITH_NEW_VERSION" => {
            Some((
                ReleaseState::Halted,
                "Version is no longer on the App Store",
            ))
        }
        _ => Some((ReleaseState::InReview, "In App Store review")),
    }
}

/// Release state implied by a live version's phased release.
fn live_outcome(phased_state: Option<&str>, day: Option<i64>) -> (ReleaseState, f64, &'static str) {
    match phased_state {
        Some("ACTIVE") => (
            ReleaseState::Rolling,
            phased_release_fraction(day),
            "Phased release in progress",
        ),
        Some("PAUSED") => (
            ReleaseState::Paused,
            phased_release_fraction(day),
            "Phased release paused",
        ),
        Some("INACTIVE") => (
            ReleaseState::Rolling,
            0.0,
            "Phased release waiting to start",
        ),
        _ => (
            ReleaseState::Complete,
            1.0,
            "Live for all users on the App Store",
        ),
    }
}

/// Release state implied by a build's external TestFlight state.
fn beta_review_outcome(external_state: Option<&str>) -> Option<(ReleaseState, &'static str)> {
    match external_state {
        Some("BETA_APPROVED" | "IN_BETA_TESTING") => Some((
            ReleaseState::Complete,
            "Available to external TestFlight testers",
        )),
        Some("BETA_REJECTED") => Some((ReleaseState::Failed, "Rejected in TestFlight beta review")),
        Some("EXPIRED") => Some((ReleaseState::Halted, "TestFlight build expired")),
        _ => None,
    }
}

/// Whether a processed build is ready to hand out, from its TestFlight state.
#[derive(Debug, PartialEq, Eq)]
enum BetaReadiness {
    Ready,
    Wait,
    Failed(&'static str),
}

/// Apple marks a build `VALID` before its TestFlight copy has finished
/// processing and before export compliance is settled. Like fastlane's pilot
/// and Codemagic's `wait_until_build_is_processed`, distribute only after the
/// internal TestFlight state has moved past both.
fn beta_readiness(detail: &AscBetaDetail, waited_secs: i64) -> BetaReadiness {
    match detail.internal.as_deref() {
        Some("PROCESSING_EXCEPTION") => BetaReadiness::Failed(
            "Apple could not prepare the build for TestFlight; check App Store Connect",
        ),
        Some("MISSING_EXPORT_COMPLIANCE") if waited_secs > BUILD_DISCOVERY_TIMEOUT_SECS => {
            BetaReadiness::Failed(
                "Apple still needs export compliance for this build; answer it in App Store Connect or set ITSAppUsesNonExemptEncryption in Info.plist",
            )
        }
        Some("PROCESSING") if waited_secs > BUILD_DISCOVERY_TIMEOUT_SECS => BetaReadiness::Failed(
            "Apple did not finish processing the build for TestFlight within three hours",
        ),
        Some("PROCESSING" | "MISSING_EXPORT_COMPLIANCE") => BetaReadiness::Wait,
        _ => BetaReadiness::Ready,
    }
}

/// External states that mean the build was already submitted for (or has
/// passed) TestFlight beta review, so it must not be submitted again.
fn beta_review_already_submitted(external_state: Option<&str>) -> bool {
    matches!(
        external_state,
        Some(
            "WAITING_FOR_BETA_REVIEW"
                | "IN_BETA_REVIEW"
                | "BETA_APPROVED"
                | "IN_BETA_TESTING"
                | "BETA_REJECTED"
        )
    )
}

#[derive(Debug, Clone, FromRow)]
struct LinkRow {
    release_id: String,
    pipeline_id: String,
    bundle_id: String,
    version: String,
    build_number: String,
    config: String,
    stage: String,
    asc_app_id: Option<String>,
    asc_build_id: Option<String>,
    asc_version_id: Option<String>,
    created_at: i64,
    release_state: String,
    rollout_fraction: f64,
}

/// Why a distribution stopped before it changed anything in App Store
/// Connect, so promotion can tell the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepRefusal {
    TestflightGroupNotFound,
}

/// What one monitor pass decided for a release.
#[derive(Debug, Default)]
struct Step {
    stage: Option<Stage>,
    refusal: Option<StepRefusal>,
    transition: Option<(ReleaseState, Option<f64>, String)>,
    next_poll_in: i64,
    asc_app_id: Option<String>,
    asc_build_id: Option<String>,
    asc_version_id: Option<String>,
}

impl Step {
    fn wait(seconds: i64) -> Self {
        Self {
            next_poll_in: seconds,
            ..Self::default()
        }
    }
}

async fn advance(asc: &AppStoreConnect<'_>, link: &LinkRow) -> Result<Step, AscError> {
    let config: AppStorePublishConfig = serde_json::from_str(&link.config).unwrap_or_default();
    let mut step = Step::wait(POLL_PROCESSING_SECS);

    let app_id = match link.asc_app_id.clone() {
        Some(id) => id,
        None => match asc.find_app_id(&link.bundle_id).await? {
            Some(id) => {
                step.asc_app_id = Some(id.clone());
                id
            }
            None => {
                step.stage = Some(Stage::Done);
                step.transition = Some((
                    ReleaseState::Failed,
                    None,
                    format!(
                        "App Store Connect has no app with bundle ID {}",
                        link.bundle_id
                    ),
                ));
                return Ok(step);
            }
        },
    };

    match Stage::parse(&link.stage) {
        Stage::AwaitingBuild => {
            let Some(build) = asc
                .find_build(&app_id, &link.version, &link.build_number)
                .await?
            else {
                if now_unix() - link.created_at > BUILD_DISCOVERY_TIMEOUT_SECS {
                    step.stage = Some(Stage::Done);
                    step.transition = Some((
                        ReleaseState::Failed,
                        None,
                        format!(
                            "App Store Connect did not list build {} ({}) within three hours",
                            link.version, link.build_number
                        ),
                    ));
                }
                return Ok(step);
            };
            step.asc_build_id = Some(build.id.clone());
            match build.processing {
                BuildProcessing::Processing => Ok(step),
                BuildProcessing::Failed => {
                    step.stage = Some(Stage::Done);
                    step.transition = Some((
                        ReleaseState::Failed,
                        None,
                        "Apple could not process the build; check the email App Store Connect sent"
                            .to_string(),
                    ));
                    Ok(step)
                }
                BuildProcessing::Valid => {
                    if build.uses_non_exempt_encryption.is_none() {
                        asc.set_export_compliance(&build.id, config.uses_non_exempt_encryption)
                            .await?;
                    }
                    step.stage = Some(Stage::Distribute);
                    step.next_poll_in = 0;
                    Ok(step)
                }
            }
        }
        Stage::Distribute => {
            let build_id = link
                .asc_build_id
                .clone()
                .ok_or_else(|| AscError("App Store build ID was not recorded".to_string()))?;
            let beta = asc.beta_detail(&build_id).await?;
            match beta_readiness(&beta, now_unix() - link.created_at) {
                BetaReadiness::Ready => {}
                BetaReadiness::Wait => return Ok(step),
                BetaReadiness::Failed(reason) => {
                    step.stage = Some(Stage::Done);
                    step.transition = Some((ReleaseState::Failed, None, reason.to_string()));
                    return Ok(step);
                }
            }
            match config.destination {
                AppStoreDestination::Testflight => {
                    distribute_testflight(asc, &app_id, &build_id, &beta, &config, step).await
                }
                AppStoreDestination::AppStore => {
                    distribute_app_store(asc, &app_id, &build_id, link, &config, step).await
                }
            }
        }
        Stage::BetaReview => {
            let build_id = link
                .asc_build_id
                .clone()
                .ok_or_else(|| AscError("App Store build ID was not recorded".to_string()))?;
            step.next_poll_in = POLL_REVIEW_SECS;
            if let Some((state, reason)) =
                beta_review_outcome(asc.beta_detail(&build_id).await?.external.as_deref())
            {
                let fraction = (state == ReleaseState::Complete).then_some(1.0);
                step.stage = Some(Stage::Done);
                step.transition = Some((state, fraction, reason.to_string()));
            }
            Ok(step)
        }
        Stage::Review => {
            let version_id = link
                .asc_version_id
                .clone()
                .ok_or_else(|| AscError("App Store version ID was not recorded".to_string()))?;
            step.next_poll_in = POLL_REVIEW_SECS;
            let version_state = asc.version_state(&version_id).await?;
            match review_outcome(&version_state) {
                Some((state, reason)) => {
                    if matches!(state, ReleaseState::Failed | ReleaseState::Halted) {
                        step.stage = Some(Stage::Done);
                    }
                    step.transition = Some((state, None, reason.to_string()));
                }
                None => {
                    step.stage = Some(Stage::Live);
                    step.next_poll_in = 0;
                }
            }
            Ok(step)
        }
        Stage::Live => {
            let version_id = link
                .asc_version_id
                .clone()
                .ok_or_else(|| AscError("App Store version ID was not recorded".to_string()))?;
            step.next_poll_in = POLL_LIVE_SECS;
            let phased = asc.phased_release(&version_id).await?;
            let (state, fraction, reason) = live_outcome(
                phased.as_ref().map(|phased| phased.state.as_str()),
                phased.as_ref().and_then(|phased| phased.current_day_number),
            );
            if state == ReleaseState::Complete {
                step.stage = Some(Stage::Done);
            }
            step.transition = Some((state, Some(fraction), reason.to_string()));
            Ok(step)
        }
        Stage::Done => Ok(Step::wait(POLL_LIVE_SECS)),
    }
}

async fn distribute_testflight(
    asc: &impl AscApi,
    app_id: &str,
    build_id: &str,
    beta: &AscBetaDetail,
    config: &AppStorePublishConfig,
    mut step: Step,
) -> Result<Step, AscError> {
    let mut notes_warning = None;
    for note in &config.release_notes {
        if let Err(error) = asc
            .set_beta_whats_new(build_id, &note.locale, &note.whats_new)
            .await
        {
            warn!(%error, locale = %note.locale, "failed to set TestFlight notes");
            notes_warning = Some(error.0);
        }
    }

    let groups = asc.list_beta_groups(app_id).await?;
    let mut selected = Vec::new();
    let mut missing = Vec::new();
    for name in &config.testflight_groups {
        match groups.iter().find(|group| group.name == *name).or_else(|| {
            groups
                .iter()
                .find(|group| group.name.eq_ignore_ascii_case(name))
        }) {
            Some(group) => selected.push(group),
            None => missing.push(name.as_str()),
        }
    }
    if !missing.is_empty() {
        step.stage = Some(Stage::Done);
        step.refusal = Some(StepRefusal::TestflightGroupNotFound);
        step.transition = Some((
            ReleaseState::Failed,
            None,
            format!("TestFlight group not found: {}", missing.join(", ")),
        ));
        return Ok(step);
    }
    // Internal groups with access to all builds already see this build, and
    // Apple refuses to add a build to them explicitly.
    let group_ids: Vec<String> = selected
        .iter()
        .filter(|group| !(group.is_internal && group.has_access_to_all_builds))
        .map(|group| group.id.clone())
        .collect();
    asc.add_build_to_groups(build_id, &group_ids).await?;

    let names = config.testflight_groups.join(", ");
    let suffix = notes_warning
        .map(|warning| format!(" (notes not set: {warning})"))
        .unwrap_or_default();
    if selected.iter().any(|group| !group.is_internal) {
        // A retry after a partial failure must not submit a second time.
        if !beta_review_already_submitted(beta.external.as_deref()) {
            asc.submit_for_beta_review(build_id).await?;
        }
        step.stage = Some(Stage::BetaReview);
        step.next_poll_in = POLL_REVIEW_SECS;
        step.transition = Some((
            ReleaseState::InReview,
            None,
            format!("Added to {names}; waiting for TestFlight beta review{suffix}"),
        ));
    } else {
        step.stage = Some(Stage::Done);
        step.transition = Some((
            ReleaseState::Complete,
            Some(1.0),
            format!("Available in TestFlight to {names}{suffix}"),
        ));
    }
    Ok(step)
}

async fn distribute_app_store(
    asc: &impl AscApi,
    app_id: &str,
    build_id: &str,
    link: &LinkRow,
    config: &AppStorePublishConfig,
    mut step: Step,
) -> Result<Step, AscError> {
    let version_id = match link.asc_version_id.clone() {
        Some(id) => id,
        None => {
            let id = asc.find_or_create_version(app_id, &link.version).await?;
            step.asc_version_id = Some(id.clone());
            id
        }
    };
    asc.attach_build_to_version(&version_id, build_id).await?;

    let mut notes_warning = None;
    for note in &config.release_notes {
        // Apple rejects "What's New" on an app's first version; keep going.
        if let Err(error) = asc
            .set_version_whats_new(&version_id, &note.locale, &note.whats_new)
            .await
        {
            warn!(%error, locale = %note.locale, "failed to set App Store notes");
            notes_warning = Some(error.0);
        }
    }
    if config.phased_release {
        asc.enable_phased_release(&version_id).await?;
    } else {
        // A reused editable version may still carry an earlier phased release.
        asc.disable_phased_release(&version_id).await?;
    }
    let suffix = notes_warning
        .map(|warning| format!(" (notes not set: {warning})"))
        .unwrap_or_default();

    step.stage = Some(Stage::Review);
    step.next_poll_in = POLL_REVIEW_SECS;
    if config.submit_for_review {
        asc.submit_for_review(app_id, &version_id).await?;
        step.transition = Some((
            ReleaseState::InReview,
            None,
            format!(
                "Submitted version {} for App Store review{suffix}",
                link.version
            ),
        ));
    } else {
        step.transition = Some((
            ReleaseState::InReview,
            None,
            format!(
                "Build attached to version {}; submit it for review in App Store Connect{suffix}",
                link.version
            ),
        ));
    }
    Ok(step)
}

/// The states to pass through to reach `target`. Release states only move
/// forward one hop at a time, so a build that goes straight from processing
/// to fully available passes through `rolling` at 100% on the way.
fn transition_path(from: ReleaseState, target: ReleaseState) -> Vec<ReleaseState> {
    if from.can_transition_to(target) {
        return vec![target];
    }
    if matches!(target, ReleaseState::Complete | ReleaseState::Paused)
        && from.can_transition_to(ReleaseState::Rolling)
    {
        return vec![ReleaseState::Rolling, target];
    }
    if target == ReleaseState::Halted && from.can_transition_to(ReleaseState::Failed) {
        return vec![ReleaseState::Failed];
    }
    Vec::new()
}

fn state_matches(
    current: &str,
    fraction: f64,
    next: ReleaseState,
    next_fraction: Option<f64>,
) -> bool {
    current == next.as_str()
        && next_fraction.is_none_or(|next_fraction| (next_fraction - fraction).abs() < f64::EPSILON)
}

/// Records what one poll read from Apple. Apple already holds that state, so
/// nothing is sent back to App Store Connect.
async fn apply_step(deps: ReleaseDeps<'_>, link: &LinkRow, step: Step) -> anyhow::Result<()> {
    let mut next_poll_in = step.next_poll_in;
    let mut stage = step.stage;
    if let Some((target, target_fraction, reason)) = step.transition
        && !state_matches(
            &link.release_state,
            link.rollout_fraction,
            target,
            target_fraction,
        )
    {
        let from: ReleaseState = link
            .release_state
            .parse()
            .map_err(|error: String| anyhow::anyhow!(error))?;
        let path = transition_path(from, target);
        if path.is_empty() {
            warn!(
                release_id = %link.release_id,
                from = %from,
                to = %target,
                "App Store state has no release transition"
            );
        }
        // The first hop must start from the state this poll compared against.
        let mut expect = Some((from, link.rollout_fraction));
        for to_state in path {
            let to_fraction = match to_state {
                ReleaseState::Rolling if target != ReleaseState::Rolling => {
                    target_fraction.or(Some(1.0))
                }
                _ => target_fraction,
            };
            // Apple only raises phased releases; never record a step down.
            let to_fraction = match (from, to_state, to_fraction) {
                (ReleaseState::Rolling, ReleaseState::Rolling, Some(fraction))
                    if fraction <= link.rollout_fraction =>
                {
                    continue;
                }
                _ => to_fraction,
            };
            match releases::transition_release(
                deps,
                &link.release_id,
                ReleaseChange {
                    to_state,
                    to_fraction,
                    audit_action: "app_store_release_updated",
                    expect: expect.take(),
                    from_store: true,
                },
                &ReleaseActor::System,
                Some(&reason),
            )
            .await
            {
                Ok(_) => {}
                // Someone changed the release while Apple was read; read again.
                Err(ReleaseError::Conflict(message)) => {
                    info!(release_id = %link.release_id, %message, "App Store poll is stale; polling again");
                    stage = None;
                    next_poll_in = 0;
                    break;
                }
                Err(error) => {
                    anyhow::bail!("failed to update App Store release: {error:?}");
                }
            }
        }
    }
    let now = now_unix();
    sqlx::query(
        "UPDATE app_store_release_links
         SET stage = COALESCE(?1, stage),
             asc_app_id = COALESCE(?2, asc_app_id),
             asc_build_id = COALESCE(?3, asc_build_id),
             asc_version_id = COALESCE(?4, asc_version_id),
             last_error = NULL,
             failing_since = NULL,
             next_poll_at = ?5,
             updated_at = ?6
         WHERE release_id = ?7",
    )
    .bind(stage.map(Stage::as_str))
    .bind(step.asc_app_id)
    .bind(step.asc_build_id)
    .bind(step.asc_version_id)
    .bind(now + next_poll_in)
    .bind(now)
    .bind(&link.release_id)
    .execute(deps.pool)
    .await?;
    Ok(())
}

/// Records a failed poll and backs off. A release that has kept failing for
/// [`GIVE_UP_AFTER_SECS`] is marked failed with the last error as its reason.
async fn record_poll_error(deps: ReleaseDeps<'_>, release_id: &str, message: &str) {
    if let Err(error) = try_record_poll_error(deps, release_id, message, now_unix()).await {
        error!(error = %error, release_id, "failed to record App Store poll error");
    }
}

async fn try_record_poll_error(
    deps: ReleaseDeps<'_>,
    release_id: &str,
    message: &str,
    now: i64,
) -> anyhow::Result<()> {
    let failing_since: Option<i64> = sqlx::query_scalar(
        "UPDATE app_store_release_links
         SET last_error = ?1, next_poll_at = ?2, updated_at = ?3,
             failing_since = COALESCE(failing_since, ?3)
         WHERE release_id = ?4
         RETURNING failing_since",
    )
    .bind(message)
    .bind(now + POLL_ERROR_SECS)
    .bind(now)
    .bind(release_id)
    .fetch_optional(deps.pool)
    .await?
    .flatten();
    if failing_since.is_none_or(|since| now - since < GIVE_UP_AFTER_SECS) {
        return Ok(());
    }
    let release = releases::load_release(deps.pool, release_id)
        .await
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let reason: String =
        format!("App Store Connect kept failing for 24 hours; last error: {message}")
            .chars()
            .take(500)
            .collect();
    for to_state in transition_path(release.state, ReleaseState::Failed) {
        releases::transition_release(
            deps,
            release_id,
            ReleaseChange {
                to_state,
                to_fraction: None,
                audit_action: "app_store_release_updated",
                expect: None,
                from_store: true,
            },
            &ReleaseActor::System,
            Some(&reason),
        )
        .await
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    }
    sqlx::query(
        "UPDATE app_store_release_links SET stage = 'done', updated_at = ?1 WHERE release_id = ?2",
    )
    .bind(now)
    .bind(release_id)
    .execute(deps.pool)
    .await?;
    warn!(
        release_id,
        "gave up on an App Store release that kept failing"
    );
    Ok(())
}

pub(crate) async fn load_credentials(
    state: &AppState,
    pipeline_id: &str,
) -> anyhow::Result<AppleApiCredentials> {
    let row = load_signing_key_row(&state.db, pipeline_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("pipeline has no iOS signing settings"))?;
    decrypt_credentials(&row, &state.encryption_key)?
        .ok_or_else(|| anyhow::anyhow!("pipeline has no App Store Connect API key"))
}

async fn due_links(pool: &SqlitePool) -> sqlx::Result<Vec<LinkRow>> {
    sqlx::query_as::<_, LinkRow>(
        "SELECT l.release_id, l.pipeline_id, l.bundle_id, l.version, l.build_number, l.config,
                l.stage, l.asc_app_id, l.asc_build_id, l.asc_version_id, l.created_at,
                r.state AS release_state, r.rollout_fraction
         FROM app_store_release_links l
         JOIN releases r ON r.id = l.release_id
         WHERE l.stage <> 'done'
           AND r.state NOT IN ('complete', 'failed', 'halted')
           AND l.next_poll_at <= ?1
         ORDER BY l.next_poll_at ASC
         LIMIT 50",
    )
    .bind(now_unix())
    .fetch_all(pool)
    .await
}

/// One monitor pass over every due `app_store` release.
pub(crate) async fn poll_releases(state: &AppState, client: &reqwest::Client) {
    let links = match due_links(&state.db).await {
        Ok(links) => links,
        Err(error) => {
            error!(%error, "failed to load App Store releases to poll");
            return;
        }
    };
    let deps = ReleaseDeps::from(state);
    for link in links {
        let creds = match load_credentials(state, &link.pipeline_id).await {
            Ok(creds) => creds,
            Err(error) => {
                record_poll_error(deps, &link.release_id, &error.to_string()).await;
                continue;
            }
        };
        let asc = AppStoreConnect::new(client, &creds);
        // Distribute right after processing finishes rather than a minute later.
        let mut current = link;
        for _ in 0..2 {
            match advance(&asc, &current).await {
                Ok(step) => {
                    let immediate = step.next_poll_in == 0;
                    let next_stage = step.stage;
                    let asc_ids = (
                        step.asc_app_id.clone(),
                        step.asc_build_id.clone(),
                        step.asc_version_id.clone(),
                    );
                    if let Err(error) = apply_step(deps, &current, step).await {
                        error!(%error, release_id = %current.release_id, "failed to apply App Store release step");
                        record_poll_error(deps, &current.release_id, &format!("{error:#}")).await;
                        break;
                    }
                    if !immediate {
                        break;
                    }
                    if let Some(stage) = next_stage {
                        current.stage = stage.as_str().to_string();
                    }
                    current.asc_app_id = asc_ids.0.or(current.asc_app_id);
                    current.asc_build_id = asc_ids.1.or(current.asc_build_id);
                    current.asc_version_id = asc_ids.2.or(current.asc_version_id);
                }
                Err(error) => {
                    warn!(%error, release_id = %current.release_id, "App Store Connect poll failed");
                    record_poll_error(deps, &current.release_id, &error.0).await;
                    break;
                }
            }
        }
    }
}

/// The client for App Store Connect calls. Bounded, so one hung request
/// cannot stall the monitor for every release or a user's release control.
fn app_store_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .expect("failed to build the App Store Connect HTTP client")
}

pub fn start_release_monitor(state: Arc<AppState>) {
    tokio::spawn(async move {
        let client = app_store_http_client();
        loop {
            poll_releases(&state, &client).await;
            tokio::time::sleep(MONITOR_INTERVAL).await;
        }
    });
}

// ── Release controls ────────────────────────────────────────────

/// Applies release controls to App Store Connect.
///
/// The monitor also records changes it read from Apple, so every call is
/// idempotent: a change that already matches Apple's state is a no-op. Apple
/// owns the phased release percentages, so a fraction change is only recorded.
pub struct AppStoreReleaseClient {
    pool: SqlitePool,
    encryption_key: zeroize::Zeroizing<Vec<u8>>,
    client: reqwest::Client,
}

impl AppStoreReleaseClient {
    pub fn new(pool: SqlitePool, encryption_key: zeroize::Zeroizing<Vec<u8>>) -> Self {
        Self {
            pool,
            encryption_key,
            client: app_store_http_client(),
        }
    }

    async fn apply_change(&self, release: &Release, change: StoreChange) -> anyhow::Result<()> {
        let target = match change {
            // Apple's schedule decides the percentage.
            StoreChange::Rollout { .. } => return Ok(()),
            StoreChange::Pause => "PAUSED",
            StoreChange::Resume { .. } => "ACTIVE",
            StoreChange::Complete => "COMPLETE",
            StoreChange::Halt => anyhow::bail!(
                "App Store releases cannot be halted from Oore; pause the phased release or remove the version from sale in App Store Connect"
            ),
        };
        let link: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT pipeline_id, stage, asc_version_id FROM app_store_release_links
             WHERE release_id = ?1",
        )
        .bind(&release.id)
        .fetch_optional(&self.pool)
        .await?;
        // TestFlight releases and versions that are not live have no phased
        // release; the change mirrors what Apple already reported.
        let Some((pipeline_id, stage, Some(version_id))) = link else {
            return Ok(());
        };
        if !matches!(Stage::parse(&stage), Stage::Live | Stage::Done) {
            return Ok(());
        }
        let row = load_signing_key_row(&self.pool, &pipeline_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("pipeline has no iOS signing settings"))?;
        let creds = decrypt_credentials(&row, &self.encryption_key)?
            .ok_or_else(|| anyhow::anyhow!("pipeline has no App Store Connect API key"))?;
        let asc = AppStoreConnect::new(&self.client, &creds);
        let Some(phased) = asc
            .phased_release(&version_id)
            .await
            .map_err(|e| anyhow::anyhow!(e.0))?
        else {
            if target == "COMPLETE" {
                return Ok(());
            }
            anyhow::bail!("This App Store version does not use phased release");
        };
        if phased.state == target || phased.state == "COMPLETE" {
            return Ok(());
        }
        asc.set_phased_release_state(&phased.id, target)
            .await
            .map_err(|error| anyhow::anyhow!(error.0))?;
        sqlx::query("UPDATE app_store_release_links SET next_poll_at = ?1 WHERE release_id = ?2")
            .bind(now_unix())
            .bind(&release.id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ── Promotion ───────────────────────────────────────────────────

/// The release track of App Store versions; any other App Store track names
/// TestFlight groups.
const APP_STORE_TRACK: &str = "app_store";

/// The monitor link a promoted App Store release starts with. It points at
/// the build the source release already handed out, so the monitor follows
/// beta review, App Review and phased release for the new release.
#[derive(Debug, Clone, PartialEq)]
pub struct PromotedLink {
    pipeline_id: String,
    bundle_id: String,
    version: String,
    build_number: String,
    config: String,
    stage: Stage,
    asc_app_id: String,
    asc_build_id: String,
    asc_version_id: Option<String>,
    next_poll_in: i64,
}

/// Record a promoted release's monitor link in the transaction that inserts
/// the release.
pub(crate) async fn record_promoted_link(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    release_id: &str,
    link: &PromotedLink,
) -> Result<(), sqlx::Error> {
    let now = now_unix();
    sqlx::query(
        "INSERT INTO app_store_release_links (
            release_id, pipeline_id, bundle_id, version, build_number, config, stage,
            asc_app_id, asc_build_id, asc_version_id, next_poll_at, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
    )
    .bind(release_id)
    .bind(&link.pipeline_id)
    .bind(&link.bundle_id)
    .bind(&link.version)
    .bind(&link.build_number)
    .bind(&link.config)
    .bind(link.stage.as_str())
    .bind(&link.asc_app_id)
    .bind(&link.asc_build_id)
    .bind(&link.asc_version_id)
    .bind(now + link.next_poll_in)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(Debug, FromRow)]
struct SourceLink {
    pipeline_id: String,
    bundle_id: String,
    version: String,
    build_number: String,
    config: String,
    asc_app_id: Option<String>,
    asc_build_id: Option<String>,
}

fn refused(code: &'static str, message: impl Into<String>) -> anyhow::Error {
    PromotionRefused {
        code,
        message: message.into(),
    }
    .into()
}

fn asc_failed(error: AscError) -> anyhow::Error {
    anyhow::anyhow!(error.0)
}

/// TestFlight groups an environment's track names. Like the track an upload
/// records, several groups are separated by ", ".
fn track_groups(track: &str) -> Vec<String> {
    track
        .split(", ")
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .map(str::to_string)
        .collect()
}

/// A TestFlight build Apple no longer offers to testers.
fn testflight_expired(beta: &AscBetaDetail) -> bool {
    beta.internal.as_deref() == Some("EXPIRED") || beta.external.as_deref() == Some("EXPIRED")
}

/// The App Store version to attach a build of `version` to: the version with
/// that string, while Apple still lets it change. Promotion never renames or
/// creates a version; upload distribution keeps doing that.
async fn promotion_version(
    asc: &impl AscApi,
    app_id: &str,
    version: &str,
) -> anyhow::Result<String> {
    if let Some(existing) = asc
        .find_version(app_id, version)
        .await
        .map_err(asc_failed)?
    {
        let state = existing.state.as_deref().unwrap_or("UNKNOWN");
        if version_is_editable(state) {
            return Ok(existing.id);
        }
        return Err(refused(
            "version_not_editable",
            format!(
                "App Store version {version} is {state} in App Store Connect, so no other build can be attached to it"
            ),
        ));
    }
    match asc
        .find_editable_version(app_id)
        .await
        .map_err(asc_failed)?
    {
        Some(editable) => Err(refused(
            "version_mismatch",
            format!(
                "The editable App Store version is {}, but the build is {version}. Change the version in App Store Connect to {version}, then promote again.",
                editable.version_string.as_deref().unwrap_or("unnamed")
            ),
        )),
        None => Err(refused(
            "no_editable_version",
            format!(
                "App Store Connect has no editable App Store version for this app. Create version {version} in App Store Connect, then promote again."
            ),
        )),
    }
}

/// Refuse to attach a build to a version another build is already attached
/// to while Oore follows its review or phased release, so one promotion never
/// replaces another build or submits the wrong one.
async fn ensure_version_free(
    pool: &SqlitePool,
    version_id: &str,
    build_id: &str,
) -> anyhow::Result<()> {
    let held: Option<(String, String)> = sqlx::query_as(
        "SELECT l.version, l.build_number FROM app_store_release_links l
         JOIN releases r ON r.id = l.release_id
         WHERE l.asc_version_id = ?1 AND l.stage IN ('review', 'live')
           AND r.state <> 'failed' AND r.build_id <> ?2
         LIMIT 1",
    )
    .bind(version_id)
    .bind(build_id)
    .fetch_optional(pool)
    .await?;
    match held {
        Some((version, build_number)) => Err(refused(
            "version_held_by_other_build",
            format!(
                "App Store version {version} already has build {version} ({build_number}) attached, in review or releasing. Wait until that release finishes or fails, then promote again."
            ),
        )),
        None => Ok(()),
    }
}

/// Hand the build an App Store release already put in TestFlight to more
/// TestFlight groups or to the app's App Store version. Nothing is uploaded:
/// the build Apple already processed is reused, through the same calls the
/// upload's distribution makes.
pub(crate) async fn promote_with(
    pool: &SqlitePool,
    asc: &impl AscApi,
    source: &Release,
    target: PromotionTarget<'_>,
) -> anyhow::Result<Promoted> {
    if source.track == APP_STORE_TRACK {
        return Err(refused(
            "promotion_unsupported",
            "A release on the App Store cannot be promoted further; promote from a TestFlight release",
        ));
    }
    let link = sqlx::query_as::<_, SourceLink>(
        "SELECT pipeline_id, bundle_id, version, build_number, config, asc_app_id, asc_build_id
         FROM app_store_release_links WHERE release_id = ?1",
    )
    .bind(&source.id)
    .fetch_optional(pool)
    .await?;
    let Some(link) = link else {
        return Err(refused(
            "no_source_release",
            "Oore did not upload this release to App Store Connect, so it cannot find its build there",
        ));
    };
    let Some(build_id) = link.asc_build_id.clone() else {
        return Err(refused(
            "no_source_release",
            "App Store Connect has not listed this build yet; promote it once it is in TestFlight",
        ));
    };
    let label = format!("{} ({})", link.version, link.build_number);
    let app_id = match link.asc_app_id.clone() {
        Some(id) => id,
        None => asc
            .find_app_id(&link.bundle_id)
            .await
            .map_err(asc_failed)?
            .ok_or_else(|| {
                refused(
                    "no_source_release",
                    format!(
                        "App Store Connect has no app with bundle ID {}",
                        link.bundle_id
                    ),
                )
            })?,
    };
    let beta = asc.beta_detail(&build_id).await.map_err(asc_failed)?;
    if testflight_expired(&beta) {
        return Err(refused(
            "build_expired",
            format!(
                "Build {label} has expired in TestFlight, so it can no longer be promoted. Upload a new build."
            ),
        ));
    }

    let uploaded: AppStorePublishConfig = serde_json::from_str(&link.config).unwrap_or_default();
    let mut config = AppStorePublishConfig {
        enabled: true,
        uses_non_exempt_encryption: uploaded.uses_non_exempt_encryption,
        ..AppStorePublishConfig::default()
    };
    let mut new_link = LinkRow {
        release_id: String::new(),
        pipeline_id: link.pipeline_id.clone(),
        bundle_id: link.bundle_id.clone(),
        version: link.version.clone(),
        build_number: link.build_number.clone(),
        config: String::new(),
        stage: Stage::Distribute.as_str().to_string(),
        asc_app_id: Some(app_id.clone()),
        asc_build_id: Some(build_id.clone()),
        asc_version_id: None,
        created_at: now_unix(),
        release_state: ReleaseState::Processing.as_str().to_string(),
        rollout_fraction: 0.0,
    };
    let step = if target.track == APP_STORE_TRACK {
        let version_id = promotion_version(asc, &app_id, &link.version).await?;
        ensure_version_free(pool, &version_id, &source.build_id).await?;
        new_link.asc_version_id = Some(version_id);
        config.destination = AppStoreDestination::AppStore;
        // No release notes: "What's New" is prepared in App Store Connect,
        // and the upload's notes were written for TestFlight testers.
        config.submit_for_review = target.submit_for_review;
        config.phased_release = target.phased_release;
        distribute_app_store(asc, &app_id, &build_id, &new_link, &config, Step::wait(0)).await
    } else {
        config.destination = AppStoreDestination::Testflight;
        config.testflight_groups = track_groups(target.track);
        // The build keeps the "What to Test" notes its upload set.
        distribute_testflight(asc, &app_id, &build_id, &beta, &config, Step::wait(0)).await
    }
    .map_err(asc_failed)?;

    let Some((state, fraction, detail)) = step.transition else {
        anyhow::bail!("App Store Connect distribution returned no outcome");
    };
    match step.refusal {
        Some(StepRefusal::TestflightGroupNotFound) => {
            return Err(refused("testflight_group_not_found", detail));
        }
        None if state == ReleaseState::Failed => anyhow::bail!(detail),
        None => {}
    }
    Ok(Promoted {
        start: Some((state, fraction.unwrap_or(0.0))),
        detail: Some(detail),
        app_store_link: Some(PromotedLink {
            pipeline_id: link.pipeline_id,
            bundle_id: link.bundle_id,
            version: link.version,
            build_number: link.build_number,
            config: serde_json::to_string(&config)?,
            stage: step.stage.unwrap_or(Stage::Done),
            asc_app_id: app_id,
            asc_build_id: build_id,
            asc_version_id: step.asc_version_id.or(new_link.asc_version_id),
            next_poll_in: step.next_poll_in,
        }),
    })
}

impl AppStoreReleaseClient {
    /// Promote with the App Store Connect key of the build's pipeline.
    async fn promote(
        &self,
        source: &Release,
        target: PromotionTarget<'_>,
    ) -> anyhow::Result<Promoted> {
        let pipeline_id: String =
            sqlx::query_scalar("SELECT pipeline_id FROM builds WHERE id = ?1")
                .bind(&source.build_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("the release's build no longer exists"))?;
        let row = load_signing_key_row(&self.pool, &pipeline_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("the build's pipeline has no iOS signing settings"))?;
        let creds = decrypt_credentials(&row, &self.encryption_key)?.ok_or_else(|| {
            anyhow::anyhow!("the build's pipeline has no App Store Connect API key")
        })?;
        let asc = AppStoreConnect::new(&self.client, &creds);
        promote_with(&self.pool, &asc, source, target).await
    }
}

/// Sends `app_store` releases to App Store Connect and every other store to
/// `other`.
pub struct StoreRouter {
    app_store: AppStoreReleaseClient,
    other: Arc<dyn ReleaseStoreClient>,
}

impl StoreRouter {
    pub fn new(app_store: AppStoreReleaseClient, other: Arc<dyn ReleaseStoreClient>) -> Self {
        Self { app_store, other }
    }
}

impl ReleaseStoreClient for StoreRouter {
    fn observe<'a>(
        &'a self,
        release: &'a Release,
    ) -> BoxFuture<'a, anyhow::Result<Option<releases::StoreObservation>>> {
        if release.store == ReleaseStore::AppStore {
            Box::pin(async { Ok(None) })
        } else {
            self.other.observe(release)
        }
    }

    fn apply<'a>(
        &'a self,
        release: &'a Release,
        change: StoreChange,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        if release.store == ReleaseStore::AppStore {
            Box::pin(self.app_store.apply_change(release, change))
        } else {
            self.other.apply(release, change)
        }
    }

    fn crash_rate<'a>(
        &'a self,
        release: &'a Release,
    ) -> BoxFuture<'a, anyhow::Result<Option<f64>>> {
        if release.store == ReleaseStore::AppStore {
            Box::pin(async { Ok(None) })
        } else {
            self.other.crash_rate(release)
        }
    }

    fn promote<'a>(
        &'a self,
        source: &'a Release,
        target: PromotionTarget<'a>,
    ) -> BoxFuture<'a, anyhow::Result<Promoted>> {
        if source.store == ReleaseStore::AppStore {
            Box::pin(self.app_store.promote(source, target))
        } else {
            self.other.promote(source, target)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_store_connect_keys::tests::{insert_shared_key, insert_signing, signing_pool};

    #[tokio::test]
    async fn a_shared_key_replaces_the_pipeline_key() {
        let pool = signing_pool().await;
        let encryption_key = vec![7u8; 32];
        insert_shared_key(&pool, &encryption_key, "team").await;
        insert_signing(&pool, &encryption_key, "pl1", Some("team")).await;
        insert_signing(&pool, &encryption_key, "pl2", None).await;

        let shared = load_signing_key_row(&pool, "pl1").await.unwrap().unwrap();
        let shared = decrypt_credentials(&shared, &encryption_key)
            .unwrap()
            .unwrap();
        assert_eq!(shared.key_id, "SHARED1234");
        assert_eq!(shared.issuer_id, "shared-issuer");
        assert_eq!(shared.private_key_pem, "shared-pem");

        let own = load_signing_key_row(&pool, "pl2").await.unwrap().unwrap();
        let own = decrypt_credentials(&own, &encryption_key).unwrap().unwrap();
        assert_eq!(own.key_id, "OWNKEY1234");
        assert_eq!(own.private_key_pem, "own-pem");
    }

    async fn app_store_release(
        deps: ReleaseDeps<'_>,
        state: ReleaseState,
        fraction: f64,
    ) -> (Release, LinkRow) {
        let release = releases::create_release(
            deps,
            NewRelease {
                store: ReleaseStore::AppStore,
                track: "app_store".into(),
                state,
                rollout_fraction: fraction,
                ..releases::tests::new_release("app_store", Default::default())
            },
            &ReleaseActor::System,
            None,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO app_store_release_links (release_id, pipeline_id, bundle_id, version, \
             build_number, config, stage, asc_version_id, next_poll_at, created_at, updated_at) \
             VALUES (?1, 'pl1', 'com.example.app', '1.2.0', '120', '{}', 'live', 'v1', 0, 0, 0)",
        )
        .bind(&release.id)
        .execute(deps.pool)
        .await
        .unwrap();
        let link = LinkRow {
            release_id: release.id.clone(),
            pipeline_id: "pl1".into(),
            bundle_id: "com.example.app".into(),
            version: "1.2.0".into(),
            build_number: "120".into(),
            config: "{}".into(),
            stage: "live".into(),
            asc_app_id: None,
            asc_build_id: None,
            asc_version_id: Some("v1".into()),
            created_at: 0,
            release_state: state.as_str().into(),
            rollout_fraction: fraction,
        };
        (release, link)
    }

    #[tokio::test]
    async fn monitor_records_apple_state_without_calling_apple_and_skips_stale_polls() {
        let pool = releases::tests::pool().await;
        let scheduler = crate::scheduler::Scheduler::new(16);
        let store = releases::tests::FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (release, link) = app_store_release(deps, ReleaseState::Rolling, 0.02).await;

        // Apple moved the phased release on; the monitor only records it.
        apply_step(
            deps,
            &link,
            Step {
                transition: Some((ReleaseState::Rolling, Some(0.05), "Phased".into())),
                next_poll_in: POLL_LIVE_SECS,
                ..Step::default()
            },
        )
        .await
        .unwrap();
        let rolled = releases::load_release(&pool, &release.id).await.unwrap();
        assert_eq!(rolled.rollout_fraction, 0.05);

        // A user pauses while a poll that read Apple as active is in flight.
        releases::transition_release(
            deps,
            &release.id,
            ReleaseChange {
                to_state: ReleaseState::Paused,
                to_fraction: None,
                audit_action: "release_paused",
                expect: None,
                from_store: false,
            },
            &ReleaseActor::System,
            None,
        )
        .await
        .unwrap();
        let stale = LinkRow {
            release_state: "rolling".into(),
            rollout_fraction: 0.05,
            ..link
        };
        apply_step(
            deps,
            &stale,
            Step {
                stage: Some(Stage::Done),
                transition: Some((ReleaseState::Complete, Some(1.0), "Live".into())),
                next_poll_in: POLL_LIVE_SECS,
                ..Step::default()
            },
        )
        .await
        .unwrap();
        let paused = releases::load_release(&pool, &release.id).await.unwrap();
        assert_eq!(paused.state, ReleaseState::Paused);
        let (stage, next_poll_at): (String, i64) = sqlx::query_as(
            "SELECT stage, next_poll_at FROM app_store_release_links WHERE release_id = ?1",
        )
        .bind(&release.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stage, "live");
        assert!(next_poll_at <= now_unix());
        // Only the user's pause reached the store; the monitor never did.
        assert_eq!(*store.changes.lock().unwrap(), vec![StoreChange::Pause]);
    }

    #[tokio::test]
    async fn monitor_backs_off_on_errors_and_gives_up_after_a_day() {
        let pool = releases::tests::pool().await;
        let scheduler = crate::scheduler::Scheduler::new(16);
        let store = releases::tests::FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (release, _) = app_store_release(deps, ReleaseState::InReview, 0.0).await;
        let start = 1_000_000;

        try_record_poll_error(deps, &release.id, "401 NOT_AUTHORIZED", start)
            .await
            .unwrap();
        let (last_error, next_poll_at): (Option<String>, i64) = sqlx::query_as(
            "SELECT last_error, next_poll_at FROM app_store_release_links WHERE release_id = ?1",
        )
        .bind(&release.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(last_error.as_deref(), Some("401 NOT_AUTHORIZED"));
        assert_eq!(next_poll_at, start + POLL_ERROR_SECS);
        assert_eq!(
            releases::load_release(&pool, &release.id)
                .await
                .unwrap()
                .state,
            ReleaseState::InReview
        );

        try_record_poll_error(
            deps,
            &release.id,
            "401 NOT_AUTHORIZED",
            start + GIVE_UP_AFTER_SECS,
        )
        .await
        .unwrap();
        let failed = releases::load_release(&pool, &release.id).await.unwrap();
        assert_eq!(failed.state, ReleaseState::Failed);
        let reason: String = sqlx::query_scalar(
            "SELECT reason FROM release_history WHERE release_id = ?1 AND to_state = 'failed'",
        )
        .bind(&release.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(reason.contains("401 NOT_AUTHORIZED"));
        let stage: String =
            sqlx::query_scalar("SELECT stage FROM app_store_release_links WHERE release_id = ?1")
                .bind(&release.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stage, "done");
        assert!(store.changes.lock().unwrap().is_empty());
    }

    fn testflight(groups: &[&str]) -> AppStorePublishConfig {
        AppStorePublishConfig {
            enabled: true,
            destination: AppStoreDestination::Testflight,
            testflight_groups: groups.iter().map(|group| group.to_string()).collect(),
            ..AppStorePublishConfig::default()
        }
    }

    #[test]
    fn testflight_requires_a_group_and_dedupes_names() {
        assert!(normalize_config(testflight(&[" ", ""])).is_err());
        let config = normalize_config(testflight(&[" Internal ", "Internal", "QA"])).unwrap();
        assert_eq!(config.testflight_groups, vec!["Internal", "QA"]);
    }

    #[test]
    fn testflight_ignores_app_store_only_options() {
        let mut config = testflight(&["QA"]);
        config.submit_for_review = true;
        config.phased_release = true;
        let config = normalize_config(config).unwrap();
        assert!(!config.submit_for_review);
        assert!(!config.phased_release);
    }

    #[test]
    fn release_notes_are_validated_per_locale() {
        let mut config = testflight(&["QA"]);
        config.release_notes = vec![
            AppStoreReleaseNote {
                locale: "en-US".to_string(),
                whats_new: " Fixes ".to_string(),
            },
            AppStoreReleaseNote {
                locale: "en-US".to_string(),
                whats_new: "Again".to_string(),
            },
            AppStoreReleaseNote {
                locale: "not a locale".to_string(),
                whats_new: "x".to_string(),
            },
            AppStoreReleaseNote {
                locale: "de-DE".to_string(),
                whats_new: "x".repeat(MAX_WHATS_NEW_CHARS + 1),
            },
        ];
        let errors = normalize_config(config).unwrap_err();
        assert_eq!(errors.len(), 3, "{errors:?}");

        let mut config = testflight(&["QA"]);
        config.release_notes = vec![AppStoreReleaseNote {
            locale: "en-US".to_string(),
            whats_new: " Fixes ".to_string(),
        }];
        assert_eq!(
            normalize_config(config).unwrap().release_notes[0].whats_new,
            "Fixes"
        );
    }

    #[test]
    fn upload_identity_requires_an_app_store_export() {
        let metadata = serde_json::json!({
            "ios_app": { "bundle_identifier": "com.example.app", "version": "1.2.0", "build_number": "42" },
            "ios_signing": { "effective_export_method": "app-store-connect" }
        });
        assert_eq!(
            uploaded_ipa_from_metadata("a1".to_string(), &metadata),
            Some(UploadedIpa {
                artifact_id: "a1".to_string(),
                bundle_id: "com.example.app".to_string(),
                version: "1.2.0".to_string(),
                build_number: "42".to_string(),
            })
        );
        let ad_hoc = serde_json::json!({
            "ios_app": { "bundle_identifier": "com.example.app", "version": "1.2.0", "build_number": "42" },
            "ios_signing": { "effective_export_method": "release-testing" }
        });
        assert_eq!(uploaded_ipa_from_metadata("a1".to_string(), &ad_hoc), None);
    }

    #[test]
    fn review_states_map_to_release_states() {
        assert_eq!(
            review_outcome("WAITING_FOR_REVIEW").map(|(state, _)| state),
            Some(ReleaseState::InReview)
        );
        assert_eq!(
            review_outcome("IN_REVIEW").map(|(state, _)| state),
            Some(ReleaseState::InReview)
        );
        assert_eq!(
            review_outcome("PENDING_DEVELOPER_RELEASE").map(|(state, _)| state),
            Some(ReleaseState::InReview)
        );
        assert_eq!(
            review_outcome("REJECTED").map(|(state, _)| state),
            Some(ReleaseState::Failed)
        );
        assert_eq!(review_outcome("READY_FOR_SALE"), None);
        assert_eq!(review_outcome("READY_FOR_DISTRIBUTION"), None);
    }

    #[test]
    fn phased_release_drives_rolling_paused_and_complete() {
        assert_eq!(
            live_outcome(Some("ACTIVE"), Some(3)).0,
            ReleaseState::Rolling
        );
        assert_eq!(live_outcome(Some("ACTIVE"), Some(3)).1, 0.05);
        assert_eq!(
            live_outcome(Some("PAUSED"), Some(5)).0,
            ReleaseState::Paused
        );
        assert_eq!(
            live_outcome(Some("COMPLETE"), Some(7)).0,
            ReleaseState::Complete
        );
        assert_eq!(
            live_outcome(None, None),
            (
                ReleaseState::Complete,
                1.0,
                "Live for all users on the App Store"
            )
        );
    }

    #[test]
    fn transitions_hop_through_rolling_when_needed() {
        use ReleaseState::*;
        assert_eq!(transition_path(Processing, InReview), vec![InReview]);
        assert_eq!(
            transition_path(Processing, Complete),
            vec![Rolling, Complete]
        );
        assert_eq!(transition_path(InReview, Complete), vec![Rolling, Complete]);
        assert_eq!(transition_path(InReview, Paused), vec![Rolling, Paused]);
        assert_eq!(transition_path(Rolling, Complete), vec![Complete]);
        assert_eq!(transition_path(InReview, Halted), vec![Failed]);
        assert_eq!(transition_path(Rolling, Halted), vec![Halted]);
    }

    #[test]
    fn distribution_waits_for_testflight_processing_and_export_compliance() {
        let detail = |internal: &str| AscBetaDetail {
            internal: Some(internal.to_string()),
            external: None,
        };
        assert_eq!(
            beta_readiness(&detail("PROCESSING"), 60),
            BetaReadiness::Wait
        );
        assert_eq!(
            beta_readiness(&detail("MISSING_EXPORT_COMPLIANCE"), 60),
            BetaReadiness::Wait
        );
        assert!(matches!(
            beta_readiness(&detail("MISSING_EXPORT_COMPLIANCE"), BUILD_DISCOVERY_TIMEOUT_SECS + 1),
            BetaReadiness::Failed(reason) if reason.contains("export compliance")
        ));
        assert!(matches!(
            beta_readiness(&detail("PROCESSING_EXCEPTION"), 0),
            BetaReadiness::Failed(_)
        ));
        assert_eq!(
            beta_readiness(&detail("READY_FOR_BETA_TESTING"), 0),
            BetaReadiness::Ready
        );
        assert_eq!(
            beta_readiness(&AscBetaDetail::default(), 0),
            BetaReadiness::Ready
        );
    }

    #[test]
    fn beta_review_is_submitted_only_once() {
        assert!(!beta_review_already_submitted(Some(
            "READY_FOR_BETA_SUBMISSION"
        )));
        assert!(!beta_review_already_submitted(None));
        assert!(beta_review_already_submitted(Some(
            "WAITING_FOR_BETA_REVIEW"
        )));
        assert!(beta_review_already_submitted(Some("IN_BETA_TESTING")));
    }

    #[test]
    fn beta_review_waits_until_apple_decides() {
        assert_eq!(beta_review_outcome(Some("WAITING_FOR_BETA_REVIEW")), None);
        assert_eq!(beta_review_outcome(Some("IN_BETA_REVIEW")), None);
        assert_eq!(
            beta_review_outcome(Some("IN_BETA_TESTING")).map(|(state, _)| state),
            Some(ReleaseState::Complete)
        );
        assert_eq!(
            beta_review_outcome(Some("BETA_REJECTED")).map(|(state, _)| state),
            Some(ReleaseState::Failed)
        );
    }

    async fn upload_pool() -> SqlitePool {
        let pool = crate::releases::tests::pool().await;
        sqlx::query(
            "INSERT INTO pipeline_publish_targets (id, pipeline_id, store, enabled, config, created_at, updated_at) \
             VALUES ('t1', 'pl1', 'app_store', 1, ?1, 0, 0)",
        )
        .bind(serde_json::to_string(&testflight(&["QA"])).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        let metadata = serde_json::json!({
            "ios_app": { "bundle_identifier": "com.example.app", "version": "1.2.0", "build_number": "42" },
            "ios_signing": { "effective_export_method": "app-store-connect" }
        });
        sqlx::query(
            "INSERT INTO artifacts (id, build_id, name, artifact_type, file_path, metadata, created_at) \
             VALUES ('a1', 'b1', 'App.ipa', 'ipa', 'artifacts/b1/App.ipa', ?1, 0)",
        )
        .bind(metadata.to_string())
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn count(pool: &SqlitePool, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    async fn an_upload_records_its_release_and_monitor_link_together() {
        let pool = upload_pool().await;
        let scheduler = crate::scheduler::Scheduler::new(16);
        let store = crate::releases::tests::FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        sqlx::raw_sql(
            "CREATE TRIGGER fail_link BEFORE INSERT ON app_store_release_links \
             BEGIN SELECT RAISE(ABORT, 'link insert failed'); END",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert!(
            record_upload(deps, "b1", &reported("QA", None))
                .await
                .is_err()
        );
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM releases").await, 0);
        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM release_history").await,
            0
        );

        sqlx::raw_sql("DROP TRIGGER fail_link")
            .execute(&pool)
            .await
            .unwrap();
        let release_id = record_upload(deps, "b1", &reported("QA", None))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            record_upload(deps, "b1", &reported("QA", None))
                .await
                .unwrap(),
            None
        );

        assert_eq!(count(&pool, "SELECT COUNT(*) FROM releases").await, 1);
        let linked: String = sqlx::query_scalar("SELECT release_id FROM app_store_release_links")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(linked, release_id);
    }

    fn reported(track: &str, groups: Option<&[&str]>) -> RunnerPublishResult {
        RunnerPublishResult {
            store: oore_contract::PublishStore::AppStore,
            track: track.to_string(),
            package_name: "com.example.app".to_string(),
            version_code: 42,
            version_name: Some("1.2.0".to_string()),
            rollout_fraction: None,
            status: "uploaded".to_string(),
            testflight_groups: groups
                .map(|groups| groups.iter().map(|group| group.to_string()).collect()),
        }
    }

    async fn recorded(pool: &SqlitePool) -> (String, AppStorePublishConfig) {
        let track: String = sqlx::query_scalar("SELECT track FROM releases")
            .fetch_one(pool)
            .await
            .unwrap();
        let config: String = sqlx::query_scalar("SELECT config FROM app_store_release_links")
            .fetch_one(pool)
            .await
            .unwrap();
        (track, serde_json::from_str(&config).unwrap())
    }

    #[tokio::test]
    async fn an_upload_records_the_groups_the_runner_reports() {
        // The saved settings say QA; the runner uploaded for the file's Beta,
        // or for groups the settings listed when the build started.
        for (track, groups) in [("Beta", Some(&["Beta"][..])), ("Beta", None)] {
            let pool = upload_pool().await;
            let scheduler = crate::scheduler::Scheduler::new(16);
            let store = crate::releases::tests::FakeStore::default();
            let deps = ReleaseDeps {
                pool: &pool,
                scheduler: &scheduler,
                store: &store,
            };
            record_upload(deps, "b1", &reported(track, groups))
                .await
                .unwrap()
                .unwrap();
            let (track, config) = recorded(&pool).await;
            assert_eq!(track, "Beta");
            assert_eq!(config.destination, AppStoreDestination::Testflight);
            assert_eq!(config.testflight_groups, vec!["Beta"]);
        }
    }

    #[test]
    fn reported_groups_win_over_settings_changed_during_the_build() {
        // Settings now say App Store review; the build uploaded for Internal
        // and QA before the change.
        let saved = AppStorePublishConfig {
            destination: AppStoreDestination::AppStore,
            submit_for_review: true,
            ..testflight(&["QA"])
        };
        let config = uploaded_config(
            saved.clone(),
            &reported("Internal, QA", Some(&["Internal", "QA"])),
        );
        assert_eq!(config.destination, AppStoreDestination::Testflight);
        assert_eq!(config.testflight_groups, vec!["Internal", "QA"]);
        assert!(!config.submit_for_review);
        // A group name with ", " survives when the groups are reported.
        let config = uploaded_config(saved.clone(), &reported("A, B", Some(&["A, B"])));
        assert_eq!(config.testflight_groups, vec!["A, B"]);
        // App Store uploads report no groups and keep the settings.
        assert_eq!(
            uploaded_config(saved.clone(), &reported("app_store", None)),
            saved
        );
    }

    #[test]
    fn without_reported_groups_a_matching_track_keeps_the_settings() {
        let saved = AppStorePublishConfig {
            destination: AppStoreDestination::AppStore,
            submit_for_review: true,
            ..testflight(&["QA"])
        };
        assert_eq!(uploaded_config(saved.clone(), &reported("", None)), saved);
        let file = uploaded_config(saved, &reported("Beta", None));
        assert_eq!(file.destination, AppStoreDestination::Testflight);
        assert_eq!(file.testflight_groups, vec!["Beta"]);
        let both = testflight(&["Internal", "QA"]);
        assert_eq!(
            uploaded_config(both.clone(), &reported("Internal, QA", None)),
            both
        );
    }

    // ── Promotion ───────────────────────────────────────────────

    use std::sync::Mutex;

    use oore_contract::{
        CreateProjectEnvironmentRequest, CreatePromotionRequest, ProjectRole, PromotionState,
    };

    use crate::app_store_connect::{AscBetaGroup, AscResult, AscVersion};
    use crate::environments::{self, EnvironmentError};
    use crate::project_rbac::EffectiveProjectRole;

    /// Stands in for App Store Connect and records every call.
    #[derive(Default)]
    struct FakeAsc {
        calls: Mutex<Vec<String>>,
        beta: Mutex<AscBetaDetail>,
        version: Mutex<Option<AscVersion>>,
        editable: Mutex<Option<AscVersion>>,
        attach_fails: Mutex<bool>,
    }

    impl FakeAsc {
        fn record(&self, call: String) {
            self.calls.lock().unwrap().push(call);
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }

        fn changes(&self) -> Vec<String> {
            self.calls()
                .into_iter()
                .filter(|call| !call.starts_with("get "))
                .collect()
        }
    }

    impl AscApi for FakeAsc {
        async fn find_app_id(&self, bundle_id: &str) -> AscResult<Option<String>> {
            self.record(format!("get app {bundle_id}"));
            Ok(Some("app1".into()))
        }
        async fn beta_detail(&self, build_id: &str) -> AscResult<AscBetaDetail> {
            self.record(format!("get beta {build_id}"));
            Ok(self.beta.lock().unwrap().clone())
        }
        async fn set_beta_whats_new(&self, build_id: &str, locale: &str, _: &str) -> AscResult<()> {
            self.record(format!("beta notes {build_id} {locale}"));
            Ok(())
        }
        async fn list_beta_groups(&self, app_id: &str) -> AscResult<Vec<AscBetaGroup>> {
            self.record(format!("get groups {app_id}"));
            let group = |id: &str, name: &str, internal: bool, all: bool| AscBetaGroup {
                id: id.into(),
                name: name.into(),
                is_internal: internal,
                has_access_to_all_builds: all,
            };
            Ok(vec![
                group("g-internal", "Internal", true, true),
                group("g-qa", "QA", true, false),
                group("g-external", "External", false, false),
            ])
        }
        async fn add_build_to_groups(&self, build_id: &str, group_ids: &[String]) -> AscResult<()> {
            self.record(format!("add {build_id} to {}", group_ids.join(",")));
            Ok(())
        }
        async fn submit_for_beta_review(&self, build_id: &str) -> AscResult<()> {
            self.record(format!("beta review {build_id}"));
            Ok(())
        }
        async fn find_or_create_version(&self, _: &str, version: &str) -> AscResult<String> {
            self.record(format!("create version {version}"));
            Ok("created".into())
        }
        async fn find_version(&self, app_id: &str, version: &str) -> AscResult<Option<AscVersion>> {
            self.record(format!("get version {app_id} {version}"));
            Ok(self.version.lock().unwrap().clone())
        }
        async fn find_editable_version(&self, app_id: &str) -> AscResult<Option<AscVersion>> {
            self.record(format!("get editable {app_id}"));
            Ok(self.editable.lock().unwrap().clone())
        }
        async fn attach_build_to_version(&self, version_id: &str, build_id: &str) -> AscResult<()> {
            if *self.attach_fails.lock().unwrap() {
                return Err(AscError(
                    "Attaching build to App Store version failed (500): server error".into(),
                ));
            }
            self.record(format!("attach {build_id} to {version_id}"));
            Ok(())
        }
        async fn set_version_whats_new(
            &self,
            version_id: &str,
            locale: &str,
            _: &str,
        ) -> AscResult<()> {
            self.record(format!("version notes {version_id} {locale}"));
            Ok(())
        }
        async fn enable_phased_release(&self, version_id: &str) -> AscResult<()> {
            self.record(format!("phased on {version_id}"));
            Ok(())
        }
        async fn disable_phased_release(&self, version_id: &str) -> AscResult<()> {
            self.record(format!("phased off {version_id}"));
            Ok(())
        }
        async fn submit_for_review(&self, app_id: &str, version_id: &str) -> AscResult<()> {
            self.record(format!("review {app_id} {version_id}"));
            Ok(())
        }
    }

    /// The release store the daemon routes App Store promotions to, talking
    /// to the fake App Store Connect.
    struct AscStore {
        pool: SqlitePool,
        asc: FakeAsc,
    }

    impl ReleaseStoreClient for AscStore {
        fn apply<'a>(
            &'a self,
            _: &'a Release,
            _: StoreChange,
        ) -> BoxFuture<'a, anyhow::Result<()>> {
            Box::pin(async { Ok(()) })
        }

        fn crash_rate<'a>(&'a self, _: &'a Release) -> BoxFuture<'a, anyhow::Result<Option<f64>>> {
            Box::pin(async { Ok(None) })
        }

        fn promote<'a>(
            &'a self,
            source: &'a Release,
            target: PromotionTarget<'a>,
        ) -> BoxFuture<'a, anyhow::Result<Promoted>> {
            Box::pin(promote_with(&self.pool, &self.asc, source, target))
        }
    }

    fn maintainer() -> (EffectiveProjectRole, ReleaseActor) {
        (
            EffectiveProjectRole::Member(ProjectRole::Maintainer),
            ReleaseActor::User {
                user_id: "u2".into(),
                email: "lead@example.com".into(),
            },
        )
    }

    fn developer() -> (EffectiveProjectRole, ReleaseActor) {
        (
            EffectiveProjectRole::Member(ProjectRole::Developer),
            ReleaseActor::User {
                user_id: "u1".into(),
                email: "dev@example.com".into(),
            },
        )
    }

    /// A pool with build b1 handed to the Internal TestFlight group by an
    /// Oore upload, and an App Store store backed by the fake.
    async fn promotion_setup() -> (SqlitePool, AscStore) {
        let pool = crate::releases::tests::pool().await;
        sqlx::query(
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u2', 'lead@example.com', 'sub2', 'developer', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let store = AscStore {
            pool: pool.clone(),
            asc: FakeAsc::default(),
        };
        *store.asc.version.lock().unwrap() = Some(AscVersion {
            id: "v1".into(),
            version_string: Some("1.2.0".into()),
            state: Some("PREPARE_FOR_SUBMISSION".into()),
        });
        (pool, store)
    }

    async fn testflight_upload(deps: ReleaseDeps<'_>) -> Release {
        testflight_upload_of(deps, "b1", "build1", "120").await
    }

    async fn testflight_upload_of(
        deps: ReleaseDeps<'_>,
        build_id: &str,
        asc_build_id: &str,
        build_number: &str,
    ) -> Release {
        let release = releases::create_release(
            deps,
            NewRelease {
                build_id: build_id.into(),
                version_code: build_number.parse().ok(),
                store: ReleaseStore::AppStore,
                state: ReleaseState::Complete,
                rollout_fraction: 1.0,
                ..releases::tests::new_release("Internal", ReleasePolicy::default())
            },
            &ReleaseActor::System,
            Some("Available in TestFlight to Internal"),
        )
        .await
        .unwrap();
        let mut config = testflight(&["Internal"]);
        config.release_notes = vec![AppStoreReleaseNote {
            locale: "en-US".into(),
            whats_new: "Faster sync".into(),
        }];
        sqlx::query(
            "INSERT INTO app_store_release_links (release_id, pipeline_id, bundle_id, version, \
             build_number, config, stage, asc_app_id, asc_build_id, next_poll_at, created_at, updated_at) \
             VALUES (?1, 'pl1', 'com.example.app', '1.2.0', ?3, ?2, 'done', 'app1', ?4, 0, 0, 0)",
        )
        .bind(&release.id)
        .bind(serde_json::to_string(&config).unwrap())
        .bind(build_number)
        .bind(asc_build_id)
        .execute(deps.pool)
        .await
        .unwrap();
        release
    }

    fn app_store_environment(
        key: &str,
        track: &str,
        approval_required: bool,
        steps: Vec<f64>,
        submit_for_review: bool,
    ) -> CreateProjectEnvironmentRequest {
        CreateProjectEnvironmentRequest {
            key: key.into(),
            name: format!("{}{}", key[..1].to_uppercase(), &key[1..]),
            store: ReleaseStore::AppStore,
            track: track.into(),
            approval_required,
            rollout_policy: Some(ReleasePolicy {
                steps,
                ..ReleasePolicy::default()
            }),
            submit_for_review,
        }
    }

    fn promote_b1(environment_id: &str) -> CreatePromotionRequest {
        CreatePromotionRequest {
            build_id: "b1".into(),
            environment_id: environment_id.into(),
            reason: None,
        }
    }

    async fn link_of(pool: &SqlitePool, release_id: &str) -> (String, String, Option<String>) {
        sqlx::query_as(
            "SELECT stage, asc_build_id, asc_version_id FROM app_store_release_links WHERE release_id = ?1",
        )
        .bind(release_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn history_reason(pool: &SqlitePool, release_id: &str) -> String {
        sqlx::query_scalar("SELECT reason FROM release_history WHERE release_id = ?1")
            .bind(release_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn promoting_between_testflight_groups_adds_the_build_without_an_upload() {
        let (pool, store) = promotion_setup().await;
        let scheduler = crate::scheduler::Scheduler::new(16);
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (role, actor) = maintainer();
        let source = testflight_upload(deps).await;
        let qa = environments::create_environment(
            &pool,
            "p1",
            &actor,
            app_store_environment("qa", "QA", false, vec![1.0], false),
        )
        .await
        .unwrap();
        let beta = environments::create_environment(
            &pool,
            "p1",
            &actor,
            app_store_environment("beta", "External", false, vec![1.0], false),
        )
        .await
        .unwrap();
        assert!(qa.promotable && beta.promotable);
        let builds_before = count(&pool, "SELECT COUNT(*) FROM builds").await;

        // An internal group: testers have it at once.
        let promotion =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&qa.id))
                .await
                .unwrap();
        assert_eq!(promotion.state, PromotionState::Completed);
        assert_eq!(store.asc.changes(), vec!["add build1 to g-qa"]);
        let release = releases::load_release(&pool, promotion.release_id.as_deref().unwrap())
            .await
            .unwrap();
        assert_eq!(release.store, ReleaseStore::AppStore);
        assert_eq!(release.track, "QA");
        assert_eq!(
            (release.state, release.rollout_fraction),
            (ReleaseState::Complete, 1.0)
        );
        assert_eq!(release.environment_id.as_deref(), Some(qa.id.as_str()));
        assert_eq!(
            release.promoted_from_release_id.as_deref(),
            Some(source.id.as_str())
        );
        assert_eq!(link_of(&pool, &release.id).await.0, "done");
        assert!(
            history_reason(&pool, &release.id)
                .await
                .contains("without a new build. Available in TestFlight to QA")
        );

        // An external group: Apple's beta review comes first, and the monitor
        // follows it for the new release.
        store.asc.calls.lock().unwrap().clear();
        let promotion =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&beta.id))
                .await
                .unwrap();
        assert_eq!(
            store.asc.changes(),
            vec!["add build1 to g-external", "beta review build1"]
        );
        let release = releases::load_release(&pool, promotion.release_id.as_deref().unwrap())
            .await
            .unwrap();
        assert_eq!(release.state, ReleaseState::InReview);
        assert_eq!(
            link_of(&pool, &release.id).await,
            ("beta_review".to_string(), "build1".to_string(), None)
        );
        // Nothing was uploaded or rebuilt.
        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM builds").await,
            builds_before
        );
        assert!(!store.asc.calls().iter().any(|call| call.contains("notes")));
    }

    #[tokio::test]
    async fn an_approved_promotion_attaches_the_build_to_the_app_store_version() {
        let (pool, store) = promotion_setup().await;
        let scheduler = crate::scheduler::Scheduler::new(16);
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (lead_role, lead) = maintainer();
        let (dev_role, dev) = developer();
        testflight_upload(deps).await;
        let production = environments::create_environment(
            &pool,
            "p1",
            &lead,
            app_store_environment("production", "app_store", true, vec![0.1, 0.5, 1.0], true),
        )
        .await
        .unwrap();
        assert!(production.submit_for_review);

        let pending = environments::request_promotion(
            deps,
            "p1",
            &dev_role,
            &dev,
            promote_b1(&production.id),
        )
        .await
        .unwrap();
        assert_eq!(pending.state, PromotionState::Pending);
        assert!(store.asc.calls().is_empty());

        let approved = environments::decide_promotion(
            deps,
            "p1",
            &pending.id,
            &lead_role,
            &lead,
            true,
            Some("QA signed off"),
        )
        .await
        .unwrap();
        assert_eq!(approved.state, PromotionState::Completed);
        assert_eq!(
            store.asc.changes(),
            vec!["attach build1 to v1", "phased on v1", "review app1 v1",]
        );
        let release = releases::load_release(&pool, approved.release_id.as_deref().unwrap())
            .await
            .unwrap();
        assert_eq!(release.track, "app_store");
        assert_eq!(
            (release.state, release.rollout_fraction),
            (ReleaseState::InReview, 0.0)
        );
        assert_eq!(release.next_step_at, None);
        assert_eq!(
            link_of(&pool, &release.id).await,
            (
                "review".to_string(),
                "build1".to_string(),
                Some("v1".to_string())
            )
        );
        let reason = history_reason(&pool, &release.id).await;
        assert!(reason.contains("QA signed off"), "{reason}");
        assert!(
            reason.contains("Submitted version 1.2.0 for App Store review"),
            "{reason}"
        );
    }

    #[tokio::test]
    async fn without_submit_the_version_waits_and_its_notes_are_left_alone() {
        let (pool, store) = promotion_setup().await;
        let scheduler = crate::scheduler::Scheduler::new(16);
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (role, actor) = maintainer();
        testflight_upload(deps).await;
        let production = environments::create_environment(
            &pool,
            "p1",
            &actor,
            app_store_environment("production", "app_store", false, vec![1.0], false),
        )
        .await
        .unwrap();
        let promotion =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&production.id))
                .await
                .unwrap();
        // No rename, no notes, no phased release, no submission.
        assert_eq!(
            store.asc.changes(),
            vec!["attach build1 to v1", "phased off v1"]
        );
        let release_id = promotion.release_id.unwrap();
        let reason = history_reason(&pool, &release_id).await;
        assert!(
            reason.contains("submit it for review in App Store Connect"),
            "{reason}"
        );
    }

    #[tokio::test]
    async fn an_apple_failure_mid_promotion_fails_it_and_a_retry_succeeds() {
        let (pool, store) = promotion_setup().await;
        let scheduler = crate::scheduler::Scheduler::new(16);
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (role, actor) = maintainer();
        testflight_upload(deps).await;
        let production = environments::create_environment(
            &pool,
            "p1",
            &actor,
            app_store_environment("production", "app_store", false, vec![1.0], true),
        )
        .await
        .unwrap();
        *store.asc.attach_fails.lock().unwrap() = true;
        let failed =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&production.id))
                .await;
        let Err(error) = failed else {
            panic!("the promotion succeeded");
        };
        assert!(matches!(error, EnvironmentError::Store(_)));
        assert_eq!(error.into_api().0, axum::http::StatusCode::BAD_GATEWAY);
        let list = environments::list_environments(&pool, "p1", &role)
            .await
            .unwrap();
        assert_eq!(list.promotions[0].state, PromotionState::Failed);
        assert!(
            list.promotions[0]
                .error
                .as_deref()
                .unwrap()
                .contains("Attaching build to App Store version failed")
        );
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM releases").await, 1);

        *store.asc.attach_fails.lock().unwrap() = false;
        let retried =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&production.id))
                .await
                .unwrap();
        assert_eq!(retried.state, PromotionState::Completed);
        assert_eq!(
            store.asc.changes(),
            vec!["attach build1 to v1", "phased off v1", "review app1 v1"]
        );
    }

    #[tokio::test]
    async fn a_second_build_cannot_take_a_version_another_build_holds() {
        let (pool, store) = promotion_setup().await;
        let scheduler = crate::scheduler::Scheduler::new(16);
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (role, actor) = maintainer();
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, queued_at, created_at, updated_at) \
             VALUES ('b2', 'p1', 'pl1', 8, 'succeeded', 'manual', 0, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        testflight_upload(deps).await;
        testflight_upload_of(deps, "b2", "build2", "121").await;
        let production = environments::create_environment(
            &pool,
            "p1",
            &actor,
            app_store_environment("production", "app_store", false, vec![1.0], false),
        )
        .await
        .unwrap();
        let first =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&production.id))
                .await
                .unwrap();

        // Build 121 of the same version would replace build 120 on it.
        store.asc.calls.lock().unwrap().clear();
        let second = environments::request_promotion(
            deps,
            "p1",
            &role,
            &actor,
            CreatePromotionRequest {
                build_id: "b2".into(),
                environment_id: production.id.clone(),
                reason: None,
            },
        )
        .await;
        assert_eq!(refused_code(second), "version_held_by_other_build");
        assert!(store.asc.changes().is_empty(), "{:?}", store.asc.changes());

        // Once the first release fails (rejected in review), the version is free.
        sqlx::query("UPDATE releases SET state = 'failed' WHERE id = ?1")
            .bind(first.release_id.as_deref().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        let retried = environments::request_promotion(
            deps,
            "p1",
            &role,
            &actor,
            CreatePromotionRequest {
                build_id: "b2".into(),
                environment_id: production.id.clone(),
                reason: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(retried.state, PromotionState::Completed);
        assert_eq!(store.asc.changes()[0], "attach build2 to v1");
    }

    #[tokio::test]
    async fn only_testflight_releases_apple_listed_are_promoted() {
        let (pool, store) = promotion_setup().await;
        let scheduler = crate::scheduler::Scheduler::new(16);
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (role, actor) = maintainer();
        let source = testflight_upload(deps).await;
        let external = environments::create_environment(
            &pool,
            "p1",
            &actor,
            app_store_environment("beta", "External", false, vec![1.0], false),
        )
        .await
        .unwrap();

        // Apple has not listed the build yet.
        sqlx::query("UPDATE app_store_release_links SET asc_build_id = NULL WHERE release_id = ?1")
            .bind(&source.id)
            .execute(&pool)
            .await
            .unwrap();
        let result =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&external.id))
                .await;
        assert_eq!(refused_code(result), "no_source_release");

        // A release on the App Store itself is the end of the path.
        sqlx::query(
            "UPDATE app_store_release_links SET asc_build_id = 'build1' WHERE release_id = ?1",
        )
        .bind(&source.id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE releases SET track = 'app_store' WHERE id = ?1")
            .bind(&source.id)
            .execute(&pool)
            .await
            .unwrap();
        let result =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&external.id))
                .await;
        assert_eq!(refused_code(result), "no_source_release");
        assert!(store.asc.calls().is_empty());
    }

    fn refused_code(
        result: Result<oore_contract::EnvironmentPromotion, EnvironmentError>,
    ) -> &'static str {
        match result {
            Err(EnvironmentError::Conflict { code, .. }) => code,
            other => panic!("expected a conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn builds_that_cannot_be_promoted_are_refused_with_a_reason() {
        let (pool, store) = promotion_setup().await;
        let scheduler = crate::scheduler::Scheduler::new(16);
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let (role, actor) = maintainer();
        let production = environments::create_environment(
            &pool,
            "p1",
            &actor,
            app_store_environment("production", "app_store", false, vec![1.0], true),
        )
        .await
        .unwrap();
        let missing = environments::create_environment(
            &pool,
            "p1",
            &actor,
            app_store_environment("partners", "Partners", false, vec![1.0], false),
        )
        .await
        .unwrap();

        // No TestFlight release yet.
        let result =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&production.id))
                .await;
        assert_eq!(refused_code(result), "no_source_release");
        testflight_upload(deps).await;

        // A live version takes no other build.
        *store.asc.version.lock().unwrap() = Some(AscVersion {
            id: "v0".into(),
            version_string: Some("1.2.0".into()),
            state: Some("READY_FOR_SALE".into()),
        });
        let result =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&production.id))
                .await;
        assert_eq!(refused_code(result), "version_not_editable");

        // No version Apple lets the developer edit.
        *store.asc.version.lock().unwrap() = None;
        let result =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&production.id))
                .await;
        assert_eq!(refused_code(result), "no_editable_version");

        // The editable version is for another version string: never renamed.
        *store.asc.editable.lock().unwrap() = Some(AscVersion {
            id: "v2".into(),
            version_string: Some("1.3.0".into()),
            state: Some("PREPARE_FOR_SUBMISSION".into()),
        });
        let result =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&production.id))
                .await;
        assert_eq!(refused_code(result), "version_mismatch");

        // A group the app does not have.
        let result =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&missing.id))
                .await;
        assert_eq!(refused_code(result), "testflight_group_not_found");
        assert!(store.asc.changes().is_empty(), "{:?}", store.asc.changes());

        // Each refusal failed its promotion, with the reason and an audit entry.
        let list = environments::list_environments(&pool, "p1", &role)
            .await
            .unwrap();
        assert_eq!(list.promotions.len(), 4);
        assert!(
            list.promotions
                .iter()
                .all(|p| p.state == PromotionState::Failed)
        );
        for expected in [
            "Create version 1.2.0 in App Store Connect",
            "The editable App Store version is 1.3.0, but the build is 1.2.0",
        ] {
            assert!(
                list.promotions.iter().any(|p| p
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains(expected))),
                "{expected}"
            );
        }
        let codes: Vec<String> = sqlx::query_scalar(
            "SELECT json_extract(details, '$.code') FROM audit_logs WHERE action = 'promotion_failed' ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            codes,
            vec![
                "version_not_editable",
                "no_editable_version",
                "version_mismatch",
                "testflight_group_not_found"
            ]
        );
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM releases").await, 1);

        // Expired in TestFlight: refused before anything changes.
        *store.asc.version.lock().unwrap() = Some(AscVersion {
            id: "v2".into(),
            version_string: Some("1.2.0".into()),
            state: Some("PREPARE_FOR_SUBMISSION".into()),
        });
        *store.asc.beta.lock().unwrap() = AscBetaDetail {
            internal: Some("EXPIRED".into()),
            external: None,
        };
        let result =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&production.id))
                .await;
        assert_eq!(refused_code(result), "build_expired");
        assert!(store.asc.changes().is_empty());

        // Once the cause is fixed, the promotion can run again.
        *store.asc.beta.lock().unwrap() = AscBetaDetail::default();
        let retried =
            environments::request_promotion(deps, "p1", &role, &actor, promote_b1(&production.id))
                .await
                .unwrap();
        assert_eq!(retried.state, PromotionState::Completed);
    }

    #[tokio::test]
    async fn submit_for_review_is_only_for_the_app_store_track() {
        let (pool, _) = promotion_setup().await;
        let (_, actor) = maintainer();
        let result = environments::create_environment(
            &pool,
            "p1",
            &actor,
            app_store_environment("beta", "External", false, vec![1.0], true),
        )
        .await;
        assert!(matches!(result, Err(EnvironmentError::Invalid(_))));

        let beta = environments::create_environment(
            &pool,
            "p1",
            &actor,
            app_store_environment("beta", "External", false, vec![1.0], false),
        )
        .await
        .unwrap();
        let result = environments::update_environment(
            &pool,
            "p1",
            &beta.id,
            &actor,
            oore_contract::UpdateProjectEnvironmentRequest {
                submit_for_review: Some(true),
                ..Default::default()
            },
        )
        .await;
        assert!(matches!(result, Err(EnvironmentError::Invalid(_))));
        // Moving an App Store environment off the app_store track clears nothing
        // silently: the combination is refused.
        let production = environments::create_environment(
            &pool,
            "p1",
            &actor,
            app_store_environment("production", "app_store", false, vec![1.0], true),
        )
        .await
        .unwrap();
        let result = environments::update_environment(
            &pool,
            "p1",
            &production.id,
            &actor,
            oore_contract::UpdateProjectEnvironmentRequest {
                track: Some("Partners".into()),
                ..Default::default()
            },
        )
        .await;
        assert!(matches!(result, Err(EnvironmentError::Invalid(_))));
    }
}
