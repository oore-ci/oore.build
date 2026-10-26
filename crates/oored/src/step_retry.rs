//! Retrying a single step of a finished build.
//!
//! Only the store uploads can run on their own: they read artifacts the build
//! already stored. Every other step depends on the build's workspace, which is
//! gone once the build ends, so those steps are re-run with the whole build.
//!
//! A retry is a new build linked to its source build. Its runner downloads the
//! source build's App Bundle or IPA, stores them again as the retry's own
//! artifacts, and runs only the retried uploads.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use oore_contract::{
    ApiError, Build, PUBLISH_APP_STORE_STEP, PUBLISH_PLAY_STEP, RETRYABLE_BUILD_STEPS,
    RerunBuildResponse, RetryBuildStepRequest, RunnerRetryArtifact, RunnerRetryArtifactsResponse,
    StepResult,
};
use serde_json::Value;
use sqlx::{FromRow, Row, SqlitePool};
use tracing::{error, info};

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::runners::{RunnerAuth, require_active_job_signing_grant};
use crate::util::{api_err, now_unix};

type ApiFailure = (StatusCode, Json<ApiError>);

/// The step that stores the build's artifacts. A retry needs it to have
/// succeeded, so every artifact the build produced is stored intact.
const ARTIFACTS_STEP: &str = "artifacts";
/// Flutter writes R8's mapping next to the bundle; Play uses it to
/// deobfuscate crashes when the build stored it as an artifact.
const MAPPING_FILE_NAME: &str = "mapping.txt";
/// Long enough to download a large IPA on a slow link.
const RETRY_DOWNLOAD_URL_TTL_SECS: u64 = 60 * 60;

fn store_error(context: &str, e: impl std::fmt::Display) -> ApiFailure {
    error!(error = %e, "{context}");
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", context)
}

/// The steps a retry build runs, kept in its config snapshot.
pub(crate) fn retry_steps_from_snapshot(snapshot: &Value) -> Option<Vec<String>> {
    let steps: Vec<String> = snapshot
        .get("retry_steps")?
        .as_array()?
        .iter()
        .filter_map(|step| step.as_str().map(str::to_string))
        .collect();
    (!steps.is_empty()).then_some(steps)
}

pub(crate) use oore_contract::RETRY_PUBLISH_FROM_FILE;

/// Whether a retry of `steps` must read the repository file again: the
/// build's `artifacts` step says the file chose where it publishes, which
/// covers a publish step that never ran, or a retried step says so itself
/// (from runners that flag only publish steps).
pub(crate) fn published_from_file(step_results: Option<&str>, steps: &[String]) -> bool {
    step_results
        .and_then(|raw| serde_json::from_str::<Vec<StepResult>>(raw).ok())
        .is_some_and(|results| {
            results.iter().any(|step| {
                step.publish_from_file
                    && (step.name == ARTIFACTS_STEP || steps.contains(&step.name))
            })
        })
}

/// Mark a rerun's snapshot: the steps a retry runs, and whether it reads the
/// repository file again. A full re-run of a retry builds everything again.
pub(crate) fn set_retry_snapshot(
    snapshot: &mut serde_json::Map<String, Value>,
    retry_steps: Option<&[String]>,
    source_step_results: Option<&str>,
) {
    snapshot.remove("retry_steps");
    snapshot.remove(RETRY_PUBLISH_FROM_FILE);
    let Some(steps) = retry_steps else {
        return;
    };
    snapshot.insert("retry_steps".to_string(), serde_json::json!(steps));
    if published_from_file(source_step_results, steps) {
        snapshot.insert(RETRY_PUBLISH_FROM_FILE.to_string(), Value::Bool(true));
    }
}

#[derive(Debug, Clone, FromRow)]
struct StoredArtifact {
    id: String,
    name: String,
    artifact_type: String,
    file_path: String,
    checksum: Option<String>,
    metadata: String,
}

async fn stored_artifacts(pool: &SqlitePool, build_id: &str) -> sqlx::Result<Vec<StoredArtifact>> {
    sqlx::query_as(
        "SELECT id, name, artifact_type, file_path, checksum, metadata FROM artifacts \
         WHERE build_id = ?1 AND state = 'available' \
           AND (expires_at IS NULL OR expires_at > ?2) \
         ORDER BY created_at, name",
    )
    .bind(build_id)
    .bind(now_unix())
    .fetch_all(pool)
    .await
}

/// The App Bundle Play publishing uploads: the only one, or the only release
/// one, matching how the runner picks it from a build's outputs.
fn pick_bundle(artifacts: &[StoredArtifact]) -> Option<&StoredArtifact> {
    let bundles: Vec<_> = artifacts
        .iter()
        .filter(|artifact| artifact.name.to_ascii_lowercase().ends_with(".aab"))
        .collect();
    if let [bundle] = bundles.as_slice() {
        return Some(bundle);
    }
    let release: Vec<_> = bundles
        .into_iter()
        .filter(|artifact| artifact.name.to_ascii_lowercase().contains("release"))
        .collect();
    match release.as_slice() {
        [bundle] => Some(bundle),
        _ => None,
    }
}

fn is_terminal(status: &str) -> bool {
    matches!(
        status,
        "succeeded" | "failed" | "canceled" | "timed_out" | "expired"
    )
}

/// The terminal statuses as an SQL list.
const TERMINAL_STATUSES_SQL: &str = "('succeeded', 'failed', 'canceled', 'timed_out', 'expired')";

/// An unfinished retry of `source_build_id` that runs any of `steps`, as its
/// ID and build number.
pub(crate) async fn active_retry(
    pool: &SqlitePool,
    source_build_id: &str,
    steps: &[String],
) -> sqlx::Result<Option<(String, i64)>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT id, build_number FROM builds \
         WHERE source_build_id = ?1 AND status NOT IN {TERMINAL_STATUSES_SQL} \
           AND EXISTS ( \
             SELECT 1 FROM json_each(config_snapshot, '$.retry_steps') step \
             WHERE step.value IN (SELECT value FROM json_each(?2)) \
           ) \
         ORDER BY build_number LIMIT 1"
    )))
    .bind(source_build_id)
    .bind(serde_json::to_string(steps).unwrap_or_else(|_| "[]".to_string()))
    .fetch_optional(pool)
    .await
}

/// Uploads of `source_build_id` that a retry of it has running, queued or
/// already done, so they must not be retried again.
async fn steps_taken_by_retries(
    pool: &SqlitePool,
    source_build_id: &str,
) -> sqlx::Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT status, config_snapshot, step_results FROM builds WHERE source_build_id = ?1",
    )
    .bind(source_build_id)
    .fetch_all(pool)
    .await?;
    let mut taken = Vec::new();
    for row in rows {
        let status: String = row.get("status");
        if !is_terminal(&status) {
            let snapshot: Value =
                serde_json::from_str(&row.get::<String, _>("config_snapshot")).unwrap_or_default();
            taken.extend(retry_steps_from_snapshot(&snapshot).unwrap_or_default());
            continue;
        }
        let results: Vec<oore_contract::StepResult> = row
            .get::<Option<String>, _>("step_results")
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        taken.extend(
            results
                .into_iter()
                .filter(|step| step.status == "succeeded")
                .map(|step| step.name),
        );
    }
    Ok(taken)
}

/// Store uploads of `build` that can run again, in run order: the build has
/// finished with its artifacts stored, the upload did not already succeed
/// here or in a retry, no retry of it is under way, publishing to that store
/// is on, and the artifact it uploads is stored.
pub(crate) async fn retryable_steps(
    pool: &SqlitePool,
    build: &Build,
) -> anyhow::Result<Vec<String>> {
    if !is_terminal(&build.status) {
        return Ok(Vec::new());
    }
    let results = build.step_results.as_deref().unwrap_or_default();
    let succeeded = |name: &str| {
        results
            .iter()
            .any(|step| step.name == name && step.status == "succeeded")
    };
    if !succeeded(ARTIFACTS_STEP) {
        return Ok(Vec::new());
    }

    let taken = steps_taken_by_retries(pool, &build.id).await?;
    let mut steps = Vec::new();
    let mut artifacts = None;
    for step in RETRYABLE_BUILD_STEPS {
        if succeeded(step) || taken.iter().any(|name| name == step) {
            continue;
        }
        let ready = match step {
            PUBLISH_PLAY_STEP => {
                let enabled = crate::pipeline_publish::load_google_play(pool, &build.pipeline_id)
                    .await?
                    .is_some_and(|(config, _)| config.enabled);
                if !enabled {
                    false
                } else {
                    if artifacts.is_none() {
                        artifacts = Some(stored_artifacts(pool, &build.id).await?);
                    }
                    pick_bundle(artifacts.as_deref().unwrap_or_default()).is_some()
                }
            }
            PUBLISH_APP_STORE_STEP => {
                crate::app_store_publish::publishing_enabled(pool, &build.pipeline_id).await?
                    && crate::app_store_publish::uploaded_ipa_artifact_id(pool, &build.id)
                        .await?
                        .is_some()
            }
            _ => false,
        };
        if ready {
            steps.push(step.to_string());
        }
    }
    Ok(steps)
}

/// The answer when another retry already runs `step`.
pub(crate) fn retry_in_progress(step: &str, build_number: i64) -> ApiFailure {
    api_err(
        StatusCode::CONFLICT,
        "retry_in_progress",
        format!("{step} is already being retried in build #{build_number}"),
    )
}

/// `POST /v1/builds/{build_id}/retry-step` — run one store upload of a
/// finished build again, followed by any later upload that has not succeeded.
pub async fn retry_build_step(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
    Json(req): Json<RetryBuildStepRequest>,
) -> Result<(StatusCode, Json<RerunBuildResponse>), ApiFailure> {
    let pool = &state.db;
    let row = sqlx::query("SELECT * FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| store_error("Failed to fetch build", e))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;
    let build = crate::builds::row_to_build(&row);

    let effective = resolve_effective_project_role(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        &build.project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::TriggerBuild)?;

    if !is_terminal(&build.status) {
        return Err(api_err(
            StatusCode::CONFLICT,
            "build_not_terminal",
            "Cannot retry a step of a build that is still in progress",
        ));
    }
    let step = req.step.trim();
    if !RETRYABLE_BUILD_STEPS.contains(&step) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "step_not_retryable",
            format!(
                "Only store uploads ({}) can be retried on their own. Re-run the build to run other steps again.",
                RETRYABLE_BUILD_STEPS.join(", ")
            ),
        ));
    }
    let in_progress = active_retry(pool, &build.id, &[step.to_string()])
        .await
        .map_err(|e| store_error("Failed to check for a retry in progress", e))?;
    if let Some((_, build_number)) = in_progress {
        return Err(retry_in_progress(step, build_number));
    }
    let retryable = retryable_steps(pool, &build)
        .await
        .map_err(|e| store_error("Failed to check which steps can be retried", e))?;
    let Some(position) = retryable.iter().position(|candidate| candidate == step) else {
        return Err(api_err(
            StatusCode::CONFLICT,
            "step_not_retryable",
            format!(
                "{step} cannot be retried for this build. It needs the build's stored artifacts, publishing turned on for that store, and an upload that has not already succeeded, here or in an earlier retry."
            ),
        ));
    };
    let steps = retryable[position..].to_vec();

    let build = crate::builds::queue_rerun(&state, &auth, build_id, Some(steps)).await?;
    Ok((StatusCode::CREATED, Json(RerunBuildResponse { build })))
}

/// `GET /v1/runners/{runner_id}/jobs/{job_id}/retry-artifacts` — the source
/// build's artifacts a retry job uploads, with short-lived download URLs.
pub async fn get_job_retry_artifacts(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
    headers: HeaderMap,
) -> Result<Json<RunnerRetryArtifactsResponse>, ApiFailure> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }
    require_active_job_signing_grant(&state.db, &job_id, &runner_id, &headers).await?;

    let row = sqlx::query("SELECT config_snapshot, source_build_id FROM builds WHERE id = ?1")
        .bind(&job_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| store_error("Failed to load the retry", e))?;
    let snapshot: Value =
        serde_json::from_str(&row.get::<String, _>("config_snapshot")).unwrap_or_default();
    let source_build_id: Option<String> = row.get("source_build_id");
    let (Some(steps), Some(source_build_id)) =
        (retry_steps_from_snapshot(&snapshot), source_build_id)
    else {
        return Err(api_err(
            StatusCode::CONFLICT,
            "not_a_retry",
            "This build is not a step retry",
        ));
    };

    let artifacts = stored_artifacts(&state.db, &source_build_id)
        .await
        .map_err(|e| store_error("Failed to load the source build's artifacts", e))?;
    let mut selected: Vec<&StoredArtifact> = Vec::new();
    if steps.iter().any(|step| step == PUBLISH_PLAY_STEP)
        && let Some(bundle) = pick_bundle(&artifacts)
    {
        selected.push(bundle);
        selected.extend(
            artifacts
                .iter()
                .filter(|artifact| artifact.name == MAPPING_FILE_NAME),
        );
    }
    if steps.iter().any(|step| step == PUBLISH_APP_STORE_STEP) {
        let ipa_id =
            crate::app_store_publish::uploaded_ipa_artifact_id(&state.db, &source_build_id)
                .await
                .map_err(|e| store_error("Failed to load the source build's IPA", e))?;
        selected.extend(
            artifacts
                .iter()
                .filter(|artifact| Some(&artifact.id) == ipa_id.as_ref()),
        );
    }

    let storage = state.storage.read().await.clone();
    let mut response = Vec::with_capacity(selected.len());
    for artifact in selected {
        let download_url = storage
            .generate_download_url(&artifact.file_path, RETRY_DOWNLOAD_URL_TTL_SECS)
            .await
            .map_err(|e| store_error("Failed to create an artifact download URL", e))?
            .ok_or_else(|| {
                api_err(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "storage_not_configured",
                    "Artifact storage backend is not configured.",
                )
            })?;
        response.push(RunnerRetryArtifact {
            id: artifact.id.clone(),
            name: artifact.name.clone(),
            artifact_type: artifact.artifact_type.clone(),
            checksum: artifact.checksum.clone(),
            metadata: serde_json::from_str(&artifact.metadata).unwrap_or_default(),
            download_url,
        });
    }
    info!(
        build_id = %job_id,
        source_build_id = %source_build_id,
        artifacts = response.len(),
        "runner fetched artifacts for a step retry"
    );
    Ok(Json(RunnerRetryArtifactsResponse {
        artifacts: response,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(name: &str) -> StoredArtifact {
        StoredArtifact {
            id: name.to_string(),
            name: name.to_string(),
            artifact_type: "generic".to_string(),
            file_path: name.to_string(),
            checksum: None,
            metadata: "{}".to_string(),
        }
    }

    #[test]
    fn picks_the_only_bundle_or_the_only_release_bundle() {
        let one = [artifact("app-release.apk"), artifact("app-release.aab")];
        assert_eq!(
            pick_bundle(&one).map(|a| a.name.as_str()),
            Some("app-release.aab")
        );

        let flavors = [artifact("app-debug.aab"), artifact("app-release.aab")];
        assert_eq!(
            pick_bundle(&flavors).map(|a| a.name.as_str()),
            Some("app-release.aab")
        );

        let ambiguous = [
            artifact("app-prod-release.aab"),
            artifact("app-dev-release.aab"),
        ];
        assert!(pick_bundle(&ambiguous).is_none());
        assert!(pick_bundle(&[artifact("app-release.apk")]).is_none());

        // Newer runners store bundles as `aab`; older builds have them as
        // `generic`. Both are found by name.
        let typed = StoredArtifact {
            artifact_type: "aab".to_string(),
            ..artifact("app-release.aab")
        };
        assert_eq!(
            pick_bundle(&[artifact("app-release.apk"), typed]).map(|a| a.artifact_type.as_str()),
            Some("aab")
        );
    }

    async fn pool_with_failed_publish(step_results: &str) -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'lead@example.com', 'sub', 'owner', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p1', 'App', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'release', 0, 0)",
            "INSERT INTO pipeline_publish_targets (id, pipeline_id, store, enabled, config, created_at, updated_at) \
             VALUES ('t1', 'pl1', 'google_play', 1, \
               '{\"enabled\":true,\"service_account_secret\":\"PLAY\",\"track\":\"internal\"}', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             step_results, queued_at, created_at, updated_at) \
             VALUES ('b1', 'p1', 'pl1', 1, 'failed', 'manual', ?1, 0, 0, 0)",
        )
        .bind(step_results)
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn store_artifact(pool: &SqlitePool, name: &str) {
        sqlx::query(
            "INSERT INTO artifacts (id, build_id, name, file_path, created_at) \
             VALUES (?1, 'b1', ?1, ?1, 0)",
        )
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn build(pool: &SqlitePool) -> Build {
        let row = sqlx::query("SELECT * FROM builds WHERE id = 'b1'")
            .fetch_one(pool)
            .await
            .unwrap();
        crate::builds::row_to_build(&row)
    }

    fn step(name: &str, status: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name, "status": status, "started_at": 0, "finished_at": 0, "duration_ms": 0
        })
    }

    #[tokio::test]
    async fn a_failed_play_upload_with_a_stored_bundle_can_be_retried() {
        let steps = serde_json::json!([
            step("build-1", "succeeded"),
            step("artifacts", "succeeded"),
            step("publish-play", "failed"),
        ]);
        let pool = pool_with_failed_publish(&steps.to_string()).await;
        assert!(
            retryable_steps(&pool, &build(&pool).await)
                .await
                .unwrap()
                .is_empty(),
            "nothing to upload without a stored bundle"
        );

        store_artifact(&pool, "app-release.aab").await;
        assert_eq!(
            retryable_steps(&pool, &build(&pool).await).await.unwrap(),
            vec![PUBLISH_PLAY_STEP.to_string()]
        );

        sqlx::query(
            "UPDATE pipeline_publish_targets SET config = \
             '{\"enabled\":false,\"service_account_secret\":\"PLAY\",\"track\":\"internal\"}'",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            retryable_steps(&pool, &build(&pool).await)
                .await
                .unwrap()
                .is_empty(),
            "publishing turned off"
        );
    }

    #[test]
    fn a_retry_reads_the_file_only_for_steps_that_published_from_it() {
        let mut from_file = step("publish-play", "failed");
        from_file["publish_from_file"] = serde_json::json!(true);
        let steps = serde_json::json!([
            step("artifacts", "succeeded"),
            from_file,
            step("publish-app-store", "failed"),
        ])
        .to_string();
        let play = [PUBLISH_PLAY_STEP.to_string()];
        let app_store = [PUBLISH_APP_STORE_STEP.to_string()];
        assert!(published_from_file(Some(&steps), &play));
        assert!(!published_from_file(Some(&steps), &app_store));
        let settings = serde_json::json!([step("publish-play", "failed")]).to_string();
        assert!(!published_from_file(Some(&settings), &play));
        assert!(!published_from_file(None, &play));
    }

    #[test]
    fn retrying_an_upload_that_never_ran_reads_the_file_again() {
        // The file named both stores; the Play upload failed before the App
        // Store upload ran, so only the artifacts step carries the flag.
        let mut artifacts = step("artifacts", "succeeded");
        artifacts["publish_from_file"] = serde_json::json!(true);
        let results = serde_json::json!([artifacts, step("publish-play", "failed")]).to_string();
        let mut snapshot = serde_json::json!({ "retry_steps": ["publish-play"] })
            .as_object()
            .unwrap()
            .clone();
        let app_store = [PUBLISH_APP_STORE_STEP.to_string()];
        set_retry_snapshot(&mut snapshot, Some(&app_store), Some(&results));
        assert_eq!(
            snapshot["retry_steps"],
            serde_json::json!(["publish-app-store"])
        );
        assert_eq!(snapshot[RETRY_PUBLISH_FROM_FILE], serde_json::json!(true));

        // A full re-run, or a retry of a settings build, carries neither.
        set_retry_snapshot(&mut snapshot, None, Some(&results));
        assert!(!snapshot.contains_key("retry_steps"));
        assert!(!snapshot.contains_key(RETRY_PUBLISH_FROM_FILE));
        let settings = serde_json::json!([step("artifacts", "succeeded")]).to_string();
        set_retry_snapshot(&mut snapshot, Some(&app_store), Some(&settings));
        assert!(!snapshot.contains_key(RETRY_PUBLISH_FROM_FILE));
    }

    #[test]
    fn a_publish_step_that_never_ran_follows_the_build_level_flag() {
        // The Play upload failed, so the App Store upload never ran and has
        // no result; the artifacts step says the file chose the stores.
        let mut artifacts = step("artifacts", "succeeded");
        artifacts["publish_from_file"] = serde_json::json!(true);
        let steps = serde_json::json!([
            step("build-1", "succeeded"),
            artifacts,
            step("publish-play", "failed"),
        ])
        .to_string();
        assert!(published_from_file(
            Some(&steps),
            &[PUBLISH_APP_STORE_STEP.to_string()]
        ));
        let settings = serde_json::json!([
            step("artifacts", "succeeded"),
            step("publish-play", "failed"),
        ])
        .to_string();
        assert!(!published_from_file(
            Some(&settings),
            &[PUBLISH_APP_STORE_STEP.to_string()]
        ));
    }

    #[tokio::test]
    async fn build_failures_and_finished_uploads_are_not_retryable() {
        for steps in [
            serde_json::json!([step("build-1", "failed")]),
            serde_json::json!([step("artifacts", "failed")]),
            serde_json::json!([
                step("artifacts", "succeeded"),
                step("publish-play", "succeeded")
            ]),
        ] {
            let pool = pool_with_failed_publish(&steps.to_string()).await;
            store_artifact(&pool, "app-release.aab").await;
            assert!(
                retryable_steps(&pool, &build(&pool).await)
                    .await
                    .unwrap()
                    .is_empty(),
                "{steps}"
            );
        }
    }

    async fn insert_retry(pool: &SqlitePool, id: &str, status: &str, step_results: &str) {
        sqlx::query(
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             config_snapshot, step_results, source_build_id, queued_at, created_at, updated_at) \
             VALUES (?1, 'p1', 'pl1', 2, ?2, 'manual', '{\"retry_steps\":[\"publish-play\"]}', ?3, 'b1', 0, 0, 0)",
        )
        .bind(id)
        .bind(status)
        .bind(step_results)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn an_upload_is_not_offered_again_while_or_after_a_retry_runs_it() {
        let steps = serde_json::json!([
            step("artifacts", "succeeded"),
            step("publish-play", "failed"),
        ]);
        let pool = pool_with_failed_publish(&steps.to_string()).await;
        store_artifact(&pool, "app-release.aab").await;

        insert_retry(&pool, "b2", "running", "[]").await;
        assert!(
            retryable_steps(&pool, &build(&pool).await)
                .await
                .unwrap()
                .is_empty(),
            "a retry is running"
        );
        assert_eq!(
            active_retry(&pool, "b1", &[PUBLISH_PLAY_STEP.to_string()])
                .await
                .unwrap(),
            Some(("b2".to_string(), 2))
        );

        // A failed retry leaves the upload retryable.
        let failed = serde_json::json!([step("publish-play", "failed")]).to_string();
        sqlx::query("UPDATE builds SET status = 'failed', step_results = ?1 WHERE id = 'b2'")
            .bind(&failed)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            retryable_steps(&pool, &build(&pool).await).await.unwrap(),
            vec![PUBLISH_PLAY_STEP.to_string()]
        );

        // A successful one finishes it for good.
        let succeeded = serde_json::json!([step("publish-play", "succeeded")]).to_string();
        sqlx::query("UPDATE builds SET status = 'succeeded', step_results = ?1 WHERE id = 'b2'")
            .bind(&succeeded)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            retryable_steps(&pool, &build(&pool).await)
                .await
                .unwrap()
                .is_empty(),
            "a retry already uploaded it"
        );
    }

    #[test]
    fn reads_retry_steps_from_the_snapshot() {
        let snapshot = serde_json::json!({ "retry_steps": ["publish-play", "publish-app-store"] });
        assert_eq!(
            retry_steps_from_snapshot(&snapshot),
            Some(vec![
                "publish-play".to_string(),
                "publish-app-store".to_string()
            ])
        );
        assert_eq!(retry_steps_from_snapshot(&serde_json::json!({})), None);
        assert_eq!(
            retry_steps_from_snapshot(&serde_json::json!({ "retry_steps": [] })),
            None
        );
    }
}
