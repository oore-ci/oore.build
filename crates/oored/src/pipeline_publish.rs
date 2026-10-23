use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use oore_contract::{
    ApiError, GooglePlayCredentialsTestRequest, GooglePlayCredentialsTestResponse,
    GooglePlayPublishConfig, OkResponse, PipelineGooglePlayPublishResponse, PublishStore,
    ReleaseState, ReleaseStore, RunnerPublishPlan, RunnerPublishResult,
    validate_google_play_publish_config,
};
use sqlx::{Row, SqlitePool};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{ProjectPermission, require_pipeline_project_permission};
use crate::releases::{
    NewRelease, ReleaseActor, ReleaseDeps, find_build_release, finish_create, insert_release,
};
use crate::runners::{RunnerAuth, require_active_job_signing_grant};
use crate::secrets::{resolve_secret_value, secret_name_visible, visible_secret_version};
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiFailure = (StatusCode, Json<ApiError>);
type ApiResult<T> = Result<Json<T>, ApiFailure>;

const GOOGLE_PLAY: &str = "google_play";
const PLAY_API_BASE: &str =
    "https://androidpublisher.googleapis.com/androidpublisher/v3/applications";
const PLAY_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn store_error(context: &str, e: impl std::fmt::Display) -> ApiFailure {
    error!(error = %e, "{context}");
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", context)
}

pub(crate) async fn load_google_play(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> anyhow::Result<Option<(GooglePlayPublishConfig, i64)>> {
    let row = sqlx::query(
        "SELECT config, updated_at FROM pipeline_publish_targets \
         WHERE pipeline_id = ?1 AND store = 'google_play'",
    )
    .bind(pipeline_id)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        let config: String = row.get("config");
        Ok((serde_json::from_str(&config)?, row.get("updated_at")))
    })
    .transpose()
}

async fn google_play_response(
    state: &AppState,
    project_id: &str,
    pipeline_id: String,
) -> ApiResult<PipelineGooglePlayPublishResponse> {
    let loaded = load_google_play(&state.db, &pipeline_id)
        .await
        .map_err(|e| store_error("Failed to load Google Play settings", e))?;
    let service_account_secret_present = match &loaded {
        Some((config, _)) => secret_name_visible(
            &state.db,
            project_id,
            Some(&pipeline_id),
            &config.service_account_secret,
        )
        .await
        .map_err(|e| store_error("Failed to check the service account secret", e))?,
        None => false,
    };
    let last_credentials_check = load_credentials_check(&state.db, project_id, &pipeline_id)
        .await
        .map_err(|e| store_error("Failed to load the last Google Play check", e))?;
    let (config, updated_at) = loaded.unzip();
    Ok(Json(PipelineGooglePlayPublishResponse {
        pipeline_id,
        config,
        service_account_secret_present,
        updated_at,
        last_credentials_check,
    }))
}

/// The pipeline's latest credentials check, unless the key behind its secret
/// name was rotated, deleted or overridden since.
async fn load_credentials_check(
    pool: &SqlitePool,
    project_id: &str,
    pipeline_id: &str,
) -> anyhow::Result<Option<GooglePlayCredentialsTestResponse>> {
    let Some(row) = sqlx::query(
        "SELECT service_account_secret, secret_version, ok, message, package_name, \
                service_account_email, checked_at \
         FROM pipeline_google_play_checks WHERE pipeline_id = ?1",
    )
    .bind(pipeline_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let secret: String = row.get("service_account_secret");
    let version = visible_secret_version(pool, project_id, Some(pipeline_id), &secret).await?;
    if version.as_deref() != Some(row.get::<String, _>("secret_version").as_str()) {
        return Ok(None);
    }
    Ok(Some(GooglePlayCredentialsTestResponse {
        ok: row.get::<i64, _>("ok") != 0,
        service_account_secret: secret,
        message: row.get("message"),
        package_name: row.get("package_name"),
        service_account_email: row.get("service_account_email"),
        checked_at: row.get("checked_at"),
    }))
}

async fn save_credentials_check(
    pool: &SqlitePool,
    pipeline_id: &str,
    secret_version: &str,
    check: &GooglePlayCredentialsTestResponse,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO pipeline_google_play_checks ( \
            pipeline_id, service_account_secret, secret_version, ok, message, \
            package_name, service_account_email, checked_at \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT (pipeline_id) DO UPDATE SET \
            service_account_secret = excluded.service_account_secret, \
            secret_version = excluded.secret_version, \
            ok = excluded.ok, \
            message = excluded.message, \
            package_name = excluded.package_name, \
            service_account_email = excluded.service_account_email, \
            checked_at = excluded.checked_at",
    )
    .bind(pipeline_id)
    .bind(&check.service_account_secret)
    .bind(secret_version)
    .bind(i32::from(check.ok))
    .bind(&check.message)
    .bind(&check.package_name)
    .bind(&check.service_account_email)
    .bind(check.checked_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// `GET /v1/pipelines/{pipeline_id}/publish/google-play`
pub async fn get_pipeline_google_play_publish(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<PipelineGooglePlayPublishResponse> {
    let project_id = require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::Read,
    )
    .await?;
    google_play_response(&state, &project_id, pipeline_id).await
}

/// `PUT /v1/pipelines/{pipeline_id}/publish/google-play`
pub async fn update_pipeline_google_play_publish(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
    Json(mut config): Json<GooglePlayPublishConfig>,
) -> ApiResult<PipelineGooglePlayPublishResponse> {
    let project_id = require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;

    config.service_account_secret = config.service_account_secret.trim().to_string();
    config.track = config.track.trim().to_string();
    config.release_notes_locale = config.release_notes_locale.trim().to_string();
    let errors = validate_google_play_publish_config(&config);
    if !errors.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_publish_config",
            errors.join("; "),
        ));
    }

    let serialized = serde_json::to_string(&config)
        .map_err(|e| store_error("Failed to save Google Play settings", e))?;
    let now = now_unix();
    sqlx::query(
        "INSERT INTO pipeline_publish_targets (
            id, pipeline_id, store, enabled, config, created_by, updated_by, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?7)
         ON CONFLICT (pipeline_id, store) DO UPDATE SET
            enabled = excluded.enabled,
            config = excluded.config,
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&pipeline_id)
    .bind(GOOGLE_PLAY)
    .bind(i32::from(config.enabled))
    .bind(serialized)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| store_error("Failed to save Google Play settings", e))?;

    let details = serde_json::json!({
        "pipeline_id": &pipeline_id,
        "updated_by": &auth.0.email,
        "enabled": config.enabled,
        "track": &config.track,
        "rollout_percent": config.rollout_percent,
        "draft": config.draft,
    })
    .to_string();
    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        "pipeline_google_play_publish_updated",
        "pipeline",
        Some(&pipeline_id),
        Some(&details),
    )
    .await;
    info!(pipeline_id = %pipeline_id, actor = %auth.0.email, "updated Google Play publish settings");

    google_play_response(&state, &project_id, pipeline_id).await
}

/// `POST /v1/pipelines/{pipeline_id}/publish/google-play/test-credentials`
/// — check a service account key against Google Play without a build.
///
/// Opens a Play edit for the app and discards it, which needs the same
/// Play Console access as an upload and changes nothing.
pub async fn test_pipeline_google_play_credentials(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
    Json(request): Json<GooglePlayCredentialsTestRequest>,
) -> ApiResult<GooglePlayCredentialsTestResponse> {
    let project_id = require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;

    let secret_name = request.service_account_secret.trim();
    if let Err(error) = oore_contract::validate_secret_name(secret_name) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_secret_name",
            format!("service_account_secret {error}"),
        ));
    }
    let requested_package = request
        .package_name
        .as_deref()
        .map(str::trim)
        .filter(|package| !package.is_empty());
    let package_name = match requested_package {
        Some(package) => Some(package.to_string()),
        None => default_play_package(&state.db, &pipeline_id)
            .await
            .map_err(|e| store_error("Failed to load Google Play settings", e))?,
    };
    let secret_version =
        visible_secret_version(&state.db, &project_id, Some(&pipeline_id), secret_name)
            .await
            .map_err(|e| store_error("Failed to read the service account secret", e))?;
    let service_account = resolve_secret_value(
        &state.db,
        &state.encryption_key,
        &project_id,
        Some(&pipeline_id),
        secret_name,
    )
    .await
    .map_err(|e| store_error("Failed to read the service account secret", e))?
    .map(zeroize::Zeroizing::new);

    let http = reqwest::Client::builder()
        .timeout(PLAY_CHECK_TIMEOUT)
        .build()
        .map_err(|e| store_error("Failed to start the Google Play check", e))?;
    let outcome = check_play_credentials(
        &http,
        PLAY_API_BASE,
        secret_name,
        service_account.as_ref().map(|value| value.as_str()),
        package_name.as_deref(),
    )
    .await;
    let response = GooglePlayCredentialsTestResponse {
        ok: outcome.ok,
        service_account_secret: secret_name.to_string(),
        message: outcome.message,
        package_name,
        service_account_email: outcome.service_account_email,
        checked_at: now_unix(),
    };
    // Kept so the Publishing tab still shows it after a reload. A secret that
    // is not visible has nothing to pin the result to.
    if let Some(version) = &secret_version
        && let Err(error) =
            save_credentials_check(&state.db, &pipeline_id, version, &response).await
    {
        warn!(%error, pipeline_id = %pipeline_id, "failed to save the Google Play check");
    }

    let details = serde_json::json!({
        "pipeline_id": &pipeline_id,
        "checked_by": &auth.0.email,
        "service_account_secret": secret_name,
        "package_name": &response.package_name,
        "ok": response.ok,
    })
    .to_string();
    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        "pipeline_google_play_credentials_tested",
        "pipeline",
        Some(&pipeline_id),
        Some(&details),
    )
    .await;
    info!(
        pipeline_id = %pipeline_id,
        actor = %auth.0.email,
        ok = response.ok,
        "tested Google Play credentials"
    );
    Ok(Json(response))
}

/// The app to check when the form names none: the saved expected package,
/// then the package of the pipeline's latest Google Play release.
async fn default_play_package(
    pool: &SqlitePool,
    pipeline_id: &str,
) -> anyhow::Result<Option<String>> {
    if let Some(package) = load_google_play(pool, pipeline_id)
        .await?
        .and_then(|(config, _)| config.package_name)
    {
        return Ok(Some(package));
    }
    Ok(sqlx::query_scalar(
        "SELECT r.package_name FROM releases r JOIN builds b ON b.id = r.build_id \
         WHERE b.pipeline_id = ?1 AND r.store = 'google_play' AND r.package_name IS NOT NULL \
         ORDER BY r.created_at DESC LIMIT 1",
    )
    .bind(pipeline_id)
    .fetch_optional(pool)
    .await?)
}

#[derive(Debug)]
struct PlayCheck {
    ok: bool,
    message: String,
    service_account_email: Option<String>,
}

impl PlayCheck {
    fn failed(message: impl Into<String>, service_account_email: Option<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
            service_account_email,
        }
    }
}

/// Each step an upload depends on, stopping at the first that fails: the
/// secret exists, holds a service account key, Google accepts the key, and
/// the account can open an edit for the app.
async fn check_play_credentials(
    http: &reqwest::Client,
    api_base: &str,
    secret_name: &str,
    service_account: Option<&str>,
    package_name: Option<&str>,
) -> PlayCheck {
    let Some(service_account) = service_account else {
        return PlayCheck::failed(
            format!(
                "No secret named {secret_name} is visible to this pipeline. Add it to the pipeline, its project or the instance."
            ),
            None,
        );
    };
    let Ok(email) = oore_google_play::service_account_email(service_account) else {
        return PlayCheck::failed(
            format!(
                "{secret_name} does not hold a Google service account JSON key. Store the whole key file downloaded from Google Cloud."
            ),
            None,
        );
    };
    let token = match oore_google_play::access_token(http, service_account).await {
        Ok(token) => token,
        Err(error) => return PlayCheck::failed(format!("{error:#}"), Some(email)),
    };
    let Some(package_name) = package_name else {
        return PlayCheck::failed(
            "Google accepted the key, but Oore does not know which app to check yet. Set the expected package under Advanced, then test again.",
            Some(email),
        );
    };
    match open_and_discard_edit(http, api_base, &token, package_name).await {
        Ok(()) => PlayCheck {
            ok: true,
            message: format!("{email} can publish {package_name} on Google Play."),
            service_account_email: Some(email),
        },
        Err(error) => PlayCheck::failed(format!("{error:#}"), Some(email)),
    }
}

async fn open_and_discard_edit(
    http: &reqwest::Client,
    api_base: &str,
    token: &str,
    package_name: &str,
) -> anyhow::Result<()> {
    let edits =
        oore_google_play::PlayEdits::insert_at(http, api_base, api_base, token, package_name)
            .await?;
    // An uncommitted edit also expires on its own, so a failed delete does
    // not fail the check.
    if let Err(error) = edits.delete().await {
        warn!(%error, "failed to discard the Google Play check edit");
    }
    Ok(())
}

fn require_runner(runner_auth: &RunnerAuth, runner_id: &str) -> Result<(), ApiFailure> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }
    Ok(())
}

/// `GET /v1/runners/{runner_id}/jobs/{job_id}/publish` — publish work for an
/// active job.
pub async fn get_job_publish_plan(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
    headers: HeaderMap,
) -> ApiResult<RunnerPublishPlan> {
    require_runner(&runner_auth, &runner_id)?;
    let pipeline_id =
        require_active_job_signing_grant(&state.db, &job_id, &runner_id, &headers).await?;

    let changelog: Option<String> =
        sqlx::query_scalar("SELECT changelog FROM builds WHERE id = ?1")
            .bind(&job_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| store_error("Failed to load publish settings", e))?
            .flatten();
    let google_play = load_google_play(&state.db, &pipeline_id)
        .await
        .map_err(|e| store_error("Failed to load publish settings", e))?
        .map(|(config, _)| config)
        .filter(|config| config.enabled);

    let app_store =
        crate::app_store_publish::runner_plan(&state.db, &state.encryption_key, &pipeline_id)
            .await
            .map_err(|e| store_error("Failed to load App Store publish settings", e))?;

    Ok(Json(RunnerPublishPlan {
        changelog,
        google_play,
        app_store,
    }))
}

/// `POST /v1/runners/{runner_id}/jobs/{job_id}/publish-results` — record a
/// completed store upload.
///
/// The runner retries until this succeeds, so a failure to record the release
/// is an error, and a repeated report finds the release it already created.
pub async fn report_job_publish_result(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
    headers: HeaderMap,
    Json(result): Json<RunnerPublishResult>,
) -> ApiResult<OkResponse> {
    require_runner(&runner_auth, &runner_id)?;
    let pipeline_id =
        require_active_job_signing_grant(&state.db, &job_id, &runner_id, &headers).await?;
    info!(
        build_id = %job_id,
        store = ?result.store,
        track = %result.track,
        version_code = result.version_code,
        "runner reported a store upload"
    );

    let recorded = match result.store {
        PublishStore::GooglePlay => {
            record_play_release(ReleaseDeps::from(&*state), &job_id, &pipeline_id, &result).await
        }
        PublishStore::AppStore => {
            crate::app_store_publish::record_upload(ReleaseDeps::from(&*state), &job_id).await
        }
    };
    let release_id = match recorded {
        Ok(Some(release_id)) => release_id,
        Ok(None) => {
            info!(build_id = %job_id, "store upload already recorded");
            return Ok(Json(OkResponse { ok: true }));
        }
        Err(error) => {
            error!(build_id = %job_id, error = ?error, "failed to record the store release");
            let details = serde_json::json!({
                "build_id": &job_id,
                "pipeline_id": &pipeline_id,
                "runner_id": &runner_id,
                "result": &result,
                "error": format!("{error:#}"),
            })
            .to_string();
            let _ = write_audit_log(
                &state.db,
                None,
                "store_upload_record_failed",
                "build",
                Some(&job_id),
                Some(&details),
            )
            .await;
            return Err(api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "release_record_failed",
                format!("The store upload finished, but the release was not recorded: {error:#}"),
            ));
        }
    };

    let details = serde_json::json!({
        "build_id": &job_id,
        "pipeline_id": &pipeline_id,
        "runner_id": &runner_id,
        "release_id": &release_id,
        "result": &result,
    })
    .to_string();
    if let Err(error) = write_audit_log(
        &state.db,
        None,
        "store_upload_completed",
        "build",
        Some(&job_id),
        Some(&details),
    )
    .await
    {
        warn!(build_id = %job_id, %error, "failed to audit the store upload");
    }
    Ok(Json(OkResponse { ok: true }))
}

/// Create the release record for a finished Play upload, unless this build
/// already has one. A draft is recorded as paused at 0% so nothing rolls out
/// until a maintainer resumes it. Returns the new release's id.
async fn record_play_release(
    deps: ReleaseDeps<'_>,
    build_id: &str,
    pipeline_id: &str,
    result: &RunnerPublishResult,
) -> anyhow::Result<Option<String>> {
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(build_id)
        .fetch_one(deps.pool)
        .await?;
    let policy = load_google_play(deps.pool, pipeline_id)
        .await?
        .and_then(|(config, _)| config.rollout_policy)
        .unwrap_or_default();
    let (release_state, rollout_fraction) = match result.status.as_str() {
        "completed" => (ReleaseState::Complete, 1.0),
        "draft" => (ReleaseState::Paused, 0.0),
        _ => (
            ReleaseState::Rolling,
            result.rollout_fraction.unwrap_or(1.0),
        ),
    };
    let mut tx = deps.pool.begin_with("BEGIN IMMEDIATE").await?;
    if find_build_release(&mut tx, build_id, ReleaseStore::GooglePlay)
        .await?
        .is_some()
    {
        return Ok(None);
    }
    let created = insert_release(
        &mut tx,
        NewRelease {
            project_id,
            build_id: build_id.to_string(),
            artifact_id: None,
            store: ReleaseStore::GooglePlay,
            track: result.track.clone(),
            package_name: Some(result.package_name.clone()),
            version_name: result.version_name.clone(),
            version_code: Some(result.version_code),
            state: release_state,
            rollout_fraction,
            policy,
        },
        &ReleaseActor::System,
        Some("uploaded by the runner"),
    )
    .await
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    tx.commit().await?;
    let release = finish_create(deps, created, &ReleaseActor::System).await;
    Ok(Some(release.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::releases::tests::{FakeStore, pool};
    use crate::scheduler::Scheduler;

    #[tokio::test]
    async fn a_repeated_play_report_records_one_release() {
        let pool = pool().await;
        let scheduler = Scheduler::new(16);
        let store = FakeStore::default();
        let deps = ReleaseDeps {
            pool: &pool,
            scheduler: &scheduler,
            store: &store,
        };
        let result = RunnerPublishResult {
            store: PublishStore::GooglePlay,
            track: "production".into(),
            package_name: "com.example.app".into(),
            version_code: 7,
            version_name: Some("1.0.0".into()),
            rollout_fraction: Some(0.05),
            status: "in_progress".into(),
        };

        let first = record_play_release(deps, "b1", "pl1", &result)
            .await
            .unwrap();
        let second = record_play_release(deps, "b1", "pl1", &result)
            .await
            .unwrap();

        assert!(first.is_some());
        assert_eq!(second, None);
        let (count, next_step_at): (i64, Option<i64>) = sqlx::query_as(
            "SELECT COUNT(*), MAX(next_step_at) FROM releases WHERE build_id = 'b1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
        assert!(next_step_at.is_some(), "the rollout is scheduled");
    }

    #[tokio::test]
    async fn a_saved_check_lasts_until_the_key_behind_its_name_changes() {
        let pool = pool().await;
        let insert_secret = |id: &'static str, scope: &'static str, updated_at: i64| {
            let pool = pool.clone();
            async move {
                let (project, pipeline) = match scope {
                    "instance" => (None, None),
                    "project" => (Some("p1"), None),
                    _ => (Some("p1"), Some("pl1")),
                };
                sqlx::query(
                    "INSERT INTO secrets (id, scope, project_id, pipeline_id, name, value_encrypted, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, 'PLAY', 'x', 0, ?5)",
                )
                .bind(id)
                .bind(scope)
                .bind(project)
                .bind(pipeline)
                .bind(updated_at)
                .execute(&pool)
                .await
                .unwrap();
            }
        };
        let version = || async {
            visible_secret_version(&pool, "p1", Some("pl1"), "PLAY")
                .await
                .unwrap()
                .unwrap()
        };
        let check = GooglePlayCredentialsTestResponse {
            ok: true,
            service_account_secret: "PLAY".into(),
            message: "ok".into(),
            package_name: Some("com.example.app".into()),
            service_account_email: Some("sa@example.iam.gserviceaccount.com".into()),
            checked_at: 5,
        };
        assert!(
            load_credentials_check(&pool, "p1", "pl1")
                .await
                .unwrap()
                .is_none()
        );

        insert_secret("s1", "instance", 1).await;
        save_credentials_check(&pool, "pl1", &version().await, &check)
            .await
            .unwrap();
        let loaded = load_credentials_check(&pool, "p1", "pl1")
            .await
            .unwrap()
            .unwrap();
        assert!(loaded.ok);
        assert_eq!(loaded.package_name.as_deref(), Some("com.example.app"));
        assert_eq!(loaded.checked_at, 5);

        // A narrower secret of the same name now wins, so the check is stale.
        insert_secret("s2", "project", 1).await;
        assert!(
            load_credentials_check(&pool, "p1", "pl1")
                .await
                .unwrap()
                .is_none()
        );
        save_credentials_check(&pool, "pl1", &version().await, &check)
            .await
            .unwrap();
        assert!(
            load_credentials_check(&pool, "p1", "pl1")
                .await
                .unwrap()
                .is_some()
        );

        // Rotating it makes the check stale too.
        sqlx::query("UPDATE secrets SET updated_at = 9 WHERE id = 's2'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            load_credentials_check(&pool, "p1", "pl1")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn the_check_falls_back_to_the_latest_play_release_package() {
        let pool = pool().await;
        assert_eq!(default_play_package(&pool, "pl1").await.unwrap(), None);

        sqlx::query(
            "INSERT INTO releases (id, project_id, build_id, store, track, package_name, state, created_at, updated_at) \
             VALUES ('r1', 'p1', 'b1', 'google_play', 'internal', 'com.example.app', 'complete', 1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            default_play_package(&pool, "pl1").await.unwrap().as_deref(),
            Some("com.example.app")
        );

        sqlx::query(
            "INSERT INTO pipeline_publish_targets (id, pipeline_id, store, enabled, config, created_at, updated_at) \
             VALUES ('t1', 'pl1', 'google_play', 1, \
             '{\"service_account_secret\":\"PLAY\",\"track\":\"internal\",\"package_name\":\"com.example.expected\"}', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            default_play_package(&pool, "pl1").await.unwrap().as_deref(),
            Some("com.example.expected")
        );
    }

    #[tokio::test]
    async fn a_missing_or_malformed_secret_fails_before_calling_google() {
        let http = reqwest::Client::new();
        let missing =
            check_play_credentials(&http, "http://unused", "PLAY", None, Some("com.example")).await;
        assert!(!missing.ok);
        assert!(
            missing.message.contains("No secret named PLAY"),
            "{missing:?}"
        );

        let malformed = check_play_credentials(
            &http,
            "http://unused",
            "PLAY",
            Some("not json"),
            Some("com.example"),
        )
        .await;
        assert!(!malformed.ok);
        assert!(
            malformed.message.contains("service account JSON key"),
            "{malformed:?}"
        );
        assert_eq!(malformed.service_account_email, None);
    }

    /// Serves `replies` (status, body) in turn, one per connection, and
    /// returns the request lines.
    async fn mock_play(
        replies: Vec<(u16, &'static str)>,
    ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let mut seen = Vec::new();
            for (status, body) in replies {
                let Ok(Ok((mut socket, _))) =
                    tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept())
                        .await
                else {
                    break;
                };
                let mut buf = vec![0u8; 16 * 1024];
                let read = socket.read(&mut buf).await.unwrap();
                let head = String::from_utf8_lossy(&buf[..read]).to_string();
                seen.push(head.lines().next().unwrap_or_default().to_string());
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            seen
        });
        (base, server)
    }

    #[tokio::test]
    async fn the_check_opens_and_discards_an_edit() {
        let (base, server) = mock_play(vec![(200, r#"{"id":"e1"}"#), (204, "")]).await;
        let http = reqwest::Client::new();
        open_and_discard_edit(&http, &base, "token", "com.example")
            .await
            .unwrap();
        let seen = server.await.unwrap();
        assert!(seen[0].starts_with("POST /com.example/edits "), "{seen:?}");
        assert!(
            seen[1].starts_with("DELETE /com.example/edits/e1 "),
            "{seen:?}"
        );
        assert!(!seen.iter().any(|line| line.contains(":commit")));
    }

    #[tokio::test]
    async fn missing_play_console_access_explains_the_fix() {
        let (base, _server) = mock_play(vec![(
            403,
            r#"{"error":{"message":"The caller does not have permission"}}"#,
        )])
        .await;
        let http = reqwest::Client::new();
        let error = open_and_discard_edit(&http, &base, "token", "com.example")
            .await
            .unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("The caller does not have permission"),
            "{message}"
        );
        assert!(message.contains("Users and permissions"), "{message}");
    }
}
