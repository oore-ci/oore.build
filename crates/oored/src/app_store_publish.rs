//! App Store and TestFlight publishing.
//!
//! The runner uploads the signed App Store IPA with `altool` and the
//! pipeline's App Store Connect API key, then reports the outcome. From there
//! this module owns the `app_store` release: a daemon task polls App Store
//! Connect until the build has processed, hands it to TestFlight groups or an
//! App Store version, follows review, and mirrors Apple's phased release.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, AppStoreDestination, AppStorePublishConfig, AppStoreReleaseNote, IosExportMethod,
    PipelineAppStorePublishResponse, Release, ReleasePolicy, ReleaseState, ReleaseStore,
    RunnerAppStorePublishPlan, RunnerAppStoreUploadCredentials,
};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};
use tracing::{error, info, warn};

use crate::AppState;
use crate::app_store_connect::{
    AppStoreConnect, AscBetaDetail, AscError, BuildProcessing, phased_release_fraction,
};
use crate::apple_api::AppleApiCredentials;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::project_rbac::{ProjectPermission, require_pipeline_project_permission};
use crate::releases::{
    self, BoxFuture, NewRelease, ReleaseActor, ReleaseChange, ReleaseDeps, ReleaseStoreClient,
    StoreChange,
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
fn normalize_config(
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

async fn load_signing_key_row(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> sqlx::Result<Option<SigningKeyRow>> {
    sqlx::query_as::<_, SigningKeyRow>(
        "SELECT enabled, export_method, api_key_id, api_issuer_id, api_private_key_encrypted
         FROM pipeline_ios_signing_settings WHERE pipeline_id = ?1",
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

/// Records a successful upload as an `app_store` release in `processing`.
/// The monitor takes it from there.
pub(crate) async fn record_upload(state: &AppState, build_id: &str) -> anyhow::Result<String> {
    let (project_id, pipeline_id): (String, String) =
        sqlx::query_as("SELECT project_id, pipeline_id FROM builds WHERE id = ?1")
            .bind(build_id)
            .fetch_one(&state.db)
            .await?;
    let (config, _) = load_config(&state.db, &pipeline_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("pipeline has no App Store publish settings"))?;
    let ipa = load_uploaded_ipa(&state.db, build_id).await?;
    let track = match config.destination {
        AppStoreDestination::Testflight => config.testflight_groups.join(", "),
        AppStoreDestination::AppStore => "app_store".to_string(),
    };
    let release = releases::create_release(
        ReleaseDeps::from(state),
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
        },
        &ReleaseActor::System,
        Some("Uploaded to App Store Connect; waiting for Apple to process the build"),
    )
    .await
    .map_err(|error| anyhow::anyhow!("failed to create App Store release: {error:?}"))?;

    let now = now_unix();
    sqlx::query(
        "INSERT INTO app_store_release_links (
            release_id, pipeline_id, bundle_id, version, build_number, config,
            stage, next_poll_at, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'awaiting_build', ?7, ?7, ?7)",
    )
    .bind(&release.id)
    .bind(&pipeline_id)
    .bind(&ipa.bundle_id)
    .bind(&ipa.version)
    .bind(&ipa.build_number)
    .bind(serde_json::to_string(&config)?)
    .bind(now)
    .execute(&state.db)
    .await?;
    info!(
        build_id,
        release_id = %release.id,
        bundle_id = %ipa.bundle_id,
        version = %ipa.version,
        build_number = %ipa.build_number,
        "recorded App Store Connect upload"
    );
    Ok(release.id)
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

/// What one monitor pass decided for a release.
#[derive(Debug, Default)]
struct Step {
    stage: Option<Stage>,
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
    asc: &AppStoreConnect<'_>,
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
    asc: &AppStoreConnect<'_>,
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

async fn apply_step(state: &AppState, link: &LinkRow, step: Step) -> anyhow::Result<()> {
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
            releases::transition_release(
                ReleaseDeps::from(state),
                &link.release_id,
                ReleaseChange {
                    to_state,
                    to_fraction,
                    audit_action: "app_store_release_updated",
                },
                &ReleaseActor::System,
                Some(&reason),
            )
            .await
            .map_err(|error| anyhow::anyhow!("failed to update App Store release: {error:?}"))?;
        }
    }
    sqlx::query(
        "UPDATE app_store_release_links
         SET stage = COALESCE(?1, stage),
             asc_app_id = COALESCE(?2, asc_app_id),
             asc_build_id = COALESCE(?3, asc_build_id),
             asc_version_id = COALESCE(?4, asc_version_id),
             last_error = NULL,
             next_poll_at = ?5,
             updated_at = ?6
         WHERE release_id = ?7",
    )
    .bind(step.stage.map(Stage::as_str))
    .bind(step.asc_app_id)
    .bind(step.asc_build_id)
    .bind(step.asc_version_id)
    .bind(now_unix() + step.next_poll_in)
    .bind(now_unix())
    .bind(&link.release_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

async fn record_poll_error(pool: &SqlitePool, release_id: &str, message: &str) {
    let now = now_unix();
    if let Err(error) = sqlx::query(
        "UPDATE app_store_release_links
         SET last_error = ?1, next_poll_at = ?2, updated_at = ?3
         WHERE release_id = ?4",
    )
    .bind(message)
    .bind(now + POLL_ERROR_SECS)
    .bind(now)
    .bind(release_id)
    .execute(pool)
    .await
    {
        error!(%error, release_id, "failed to record App Store poll error");
    }
}

async fn load_credentials(
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
    for link in links {
        let creds = match load_credentials(state, &link.pipeline_id).await {
            Ok(creds) => creds,
            Err(error) => {
                record_poll_error(&state.db, &link.release_id, &error.to_string()).await;
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
                    if let Err(error) = apply_step(state, &current, step).await {
                        error!(%error, release_id = %current.release_id, "failed to apply App Store release step");
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
                    record_poll_error(&state.db, &current.release_id, &error.0).await;
                    break;
                }
            }
        }
    }
}

pub fn start_release_monitor(state: Arc<AppState>) {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
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
            client: reqwest::Client::new(),
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
