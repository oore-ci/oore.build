use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use oore_contract::{
    ApiError, GooglePlayPublishConfig, OkResponse, PipelineGooglePlayPublishResponse, PublishStore,
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
use crate::secrets::secret_name_visible;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiFailure = (StatusCode, Json<ApiError>);
type ApiResult<T> = Result<Json<T>, ApiFailure>;

const GOOGLE_PLAY: &str = "google_play";

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
    let (config, updated_at) = loaded.unzip();
    Ok(Json(PipelineGooglePlayPublishResponse {
        pipeline_id,
        config,
        service_account_secret_present,
        updated_at,
    }))
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
}
