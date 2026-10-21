use std::sync::Arc;

use axum::Json;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use oore_contract::{
    ApiError, BuildDetailResponse, BuildStatus, ClaimJobRequest, ClaimJobResponse, ClaimedJob,
    JobStatusResponse, ListRunnersResponse, RUNNER_PROTOCOL_VERSION, RegisterRunnerRequest,
    RegisterRunnerResponse, Runner, RunnerHeartbeatRequest, RunnerHeartbeatResponse, RunnerStatus,
    UpdateJobStatusRequest, UpdateRunnerRequest, UpdateRunnerResponse,
};
use serde::Deserialize;
use sqlx::{QueryBuilder, Row, Sqlite};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::builds::{fetch_build_events, transition_build};
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::token::{generate_token, hash_token};
use crate::util::{api_err, extract_bearer, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Resolve the pipeline for a currently assigned job. Signing bundles are
/// available only while the build is assigned or running; requeue and terminal
/// transitions revoke `runner_id` atomically in the build state machine.
pub(crate) async fn require_active_job_signing_grant(
    pool: &sqlx::SqlitePool,
    job_id: &str,
    runner_id: &str,
    headers: &HeaderMap,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let row = sqlx::query(
        "SELECT pipeline_id, status, runner_id, signing_token_hash FROM builds WHERE id = ?1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, job_id = %job_id, "failed to load active runner assignment");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to authorize runner job",
        )
    })?
    .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;

    let status: String = row.get("status");
    if !matches!(status.as_str(), "assigned" | "running") {
        return Err(api_err(
            StatusCode::CONFLICT,
            "job_not_active",
            "Signing material is available only while the job is active",
        ));
    }

    let assigned_runner: Option<String> = row.get("runner_id");
    if assigned_runner.as_deref() != Some(runner_id) {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "This build is not assigned to your runner",
        ));
    }

    let supplied_token = headers
        .get("x-oore-signing-token")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            api_err(
                StatusCode::UNAUTHORIZED,
                "signing_grant_required",
                "A job-scoped signing grant is required",
            )
        })?;
    let expected_hash: Option<String> = row.get("signing_token_hash");
    let supplied_hash = hash_token(supplied_token);
    if expected_hash.as_deref() != Some(supplied_hash.as_str()) {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_signing_grant",
            "The job-scoped signing grant is invalid or expired",
        ));
    }

    Ok(row.get("pipeline_id"))
}

// ── RunnerAuth extractor ────────────────────────────────────────

/// Authenticated runner identity extracted from Bearer token.
pub struct RunnerAuth {
    pub runner_id: String,
    pub runner_name: String,
}

impl FromRequestParts<Arc<AppState>> for RunnerAuth {
    type Rejection = (StatusCode, Json<ApiError>);

    fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let state = state.clone();
        let headers = parts.headers.clone();

        async move {
            let token = extract_bearer(&headers).ok_or_else(|| {
                api_err(
                    StatusCode::UNAUTHORIZED,
                    "missing_auth",
                    "Authorization header required",
                )
            })?;

            let token_hash = hash_token(token);

            let pool = &state.db;

            let row = sqlx::query("SELECT id, name FROM runners WHERE token_hash = ?1")
                .bind(&token_hash)
                .fetch_optional(pool)
                .await
                .map_err(|e| {
                    error!(error = %e, "failed to look up runner by token");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "store_error",
                        "Failed to authenticate runner",
                    )
                })?
                .ok_or_else(|| {
                    api_err(
                        StatusCode::UNAUTHORIZED,
                        "invalid_token",
                        "Invalid runner token",
                    )
                })?;

            Ok(RunnerAuth {
                runner_id: row.get("id"),
                runner_name: row.get("name"),
            })
        }
    }
}

// ── Row conversion ──────────────────────────────────────────────

fn row_to_runner(row: &sqlx::sqlite::SqliteRow) -> Runner {
    let caps_str: String = row.get("capabilities");
    let capabilities: serde_json::Value = serde_json::from_str(&caps_str).unwrap_or_default();

    Runner {
        id: row.get("id"),
        name: row.get("name"),
        status: row.get("status"),
        capabilities,
        last_heartbeat_at: row.get("last_heartbeat_at"),
        registered_by: row.get("registered_by"),
        cache_purge_requested_at: row.get("cache_purge_requested_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ── Handlers ────────────────────────────────────────────────────

/// `POST /v1/runners/register` — register a new runner (admin/owner only).
pub async fn register_runner(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<RegisterRunnerRequest>,
) -> ApiResult<RegisterRunnerResponse> {
    check_permission(&state.enforcer, &auth.0.role, "runners", "write").await?;

    // Validate name
    let name = req.name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "Runner name must be between 1 and 255 characters",
        ));
    }

    let token = generate_token();
    let token_hash = hash_token(&token);
    let runner_id = Uuid::new_v4().to_string();
    let now = now_unix();
    let caps_str = serde_json::to_string(&req.capabilities).unwrap_or_else(|_| "{}".to_string());

    let pool = &state.db;

    sqlx::query(
        "INSERT INTO runners (id, name, token_hash, status, capabilities, registered_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'offline', ?4, ?5, ?6, ?6)",
    )
    .bind(&runner_id)
    .bind(name)
    .bind(&token_hash)
    .bind(&caps_str)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to insert runner");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to register runner")
    })?;

    let details = serde_json::json!({
        "runner_id": runner_id,
        "runner_name": name,
        "registered_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "runner_registered",
        "runner",
        Some(&runner_id),
        Some(&details),
    )
    .await;

    info!(runner_id = %runner_id, name = %name, registered_by = %auth.0.email, "runner registered");

    let runner = Runner {
        id: runner_id,
        name: name.to_string(),
        status: "offline".to_string(),
        capabilities: req.capabilities,
        last_heartbeat_at: None,
        registered_by: Some(auth.0.user_id),
        cache_purge_requested_at: None,
        created_at: now,
        updated_at: now,
    };

    Ok(Json(RegisterRunnerResponse { runner, token }))
}

/// `POST /v1/runners/{runner_id}/heartbeat` — runner reports status.
pub async fn runner_heartbeat(
    State(state): State<Arc<AppState>>,
    Path(runner_id): Path<String>,
    runner_auth: RunnerAuth,
    Json(req): Json<RunnerHeartbeatRequest>,
) -> ApiResult<RunnerHeartbeatResponse> {
    // Prevent cross-runner access
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }

    // Validate status
    let _status: RunnerStatus = req.status.parse().map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_status",
            "Status must be one of: online, offline, busy, draining",
        )
    })?;

    let now = now_unix();
    let has_capabilities = req.capabilities.as_object().is_some_and(|o| !o.is_empty());

    let pool = &state.db;

    // Only overwrite capabilities when the runner sends a non-empty object
    let result = if has_capabilities {
        let caps_str = serde_json::to_string(&req.capabilities).unwrap_or_else(|_| "{}".to_string());
        sqlx::query(
            "UPDATE runners SET status = ?1, capabilities = ?2, last_heartbeat_at = ?3, updated_at = ?3 \
             WHERE id = ?4",
        )
        .bind(&req.status)
        .bind(&caps_str)
        .bind(now)
        .bind(&runner_id)
        .execute(pool)
        .await
    } else {
        sqlx::query(
            "UPDATE runners SET status = ?1, last_heartbeat_at = ?2, updated_at = ?2 \
             WHERE id = ?3",
        )
        .bind(&req.status)
        .bind(now)
        .bind(&runner_id)
        .execute(pool)
        .await
    }
    .map_err(|e| {
        error!(error = %e, runner_id = %runner_id, "failed to update runner heartbeat");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to update runner")
    })?;

    if result.rows_affected() == 0 {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Runner not found",
        ));
    }

    // A runner reports the purge request it last completed; clear it unless a
    // newer request arrived since.
    let purged_at = req
        .capabilities
        .pointer("/caches/purged_at")
        .and_then(serde_json::Value::as_i64);
    let cache_purge_requested_at = settle_cache_purge_request(pool, &runner_id, purged_at)
        .await
        .map_err(|e| {
            error!(error = %e, runner_id = %runner_id, "failed to read runner cache purge request");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to update runner",
            )
        })?;

    Ok(Json(RunnerHeartbeatResponse {
        cache_purge_requested_at,
    }))
}

/// Clears a purge request the runner has completed and returns the one still
/// pending, if any.
async fn settle_cache_purge_request(
    pool: &sqlx::SqlitePool,
    runner_id: &str,
    purged_at: Option<i64>,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE runners SET cache_purge_requested_at = CASE \
           WHEN cache_purge_requested_at <= ?1 THEN NULL \
           ELSE cache_purge_requested_at END \
         WHERE id = ?2 RETURNING cache_purge_requested_at",
    )
    .bind(purged_at)
    .bind(runner_id)
    .fetch_one(pool)
    .await
}

/// `POST /v1/runners/{runner_id}/caches/purge` — ask a runner to delete its
/// dependency caches before its next build (admin/owner only).
pub async fn purge_runner_caches(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(runner_id): Path<String>,
) -> ApiResult<UpdateRunnerResponse> {
    check_permission(&state.enforcer, &auth.0.role, "runners", "write").await?;

    let pool = &state.db;
    let now = now_unix();
    let row = sqlx::query(
        "UPDATE runners SET cache_purge_requested_at = ?1, updated_at = ?1 \
         WHERE id = ?2 RETURNING *",
    )
    .bind(now)
    .bind(&runner_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, runner_id = %runner_id, "failed to request runner cache purge");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to request cache purge",
        )
    })?
    .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Runner not found"))?;

    let details = serde_json::json!({ "requested_by": auth.0.email }).to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "runner_cache_purge_requested",
        "runner",
        Some(&runner_id),
        Some(&details),
    )
    .await;
    info!(runner_id = %runner_id, requested_by = %auth.0.email, "runner cache purge requested");

    Ok(Json(UpdateRunnerResponse {
        runner: row_to_runner(&row),
    }))
}

/// Atomically linearize a direct-runner claim against the operational pause.
///
/// The initial queue lookup chooses the oldest eligible build. Rechecking the
/// pause and source identity in the queued -> scheduled update closes races
/// with an operator change while a claim request is in flight.
async fn schedule_eligible_direct_runner_build(
    pool: &sqlx::SqlitePool,
    build_id: &str,
    runner_id: &str,
    actor: &str,
) -> Result<bool, (StatusCode, Json<ApiError>)> {
    let mut tx = pool.begin().await.map_err(|e| {
        error!(error = %e, "failed to start runner claim transaction");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to claim build",
        )
    })?;
    let now = now_unix();
    let result = sqlx::query(
        "UPDATE builds SET status = 'scheduled', updated_at = ?1 \
         WHERE id = ?2 AND status = 'queued' \
           AND EXISTS (SELECT 1 FROM runners runner WHERE runner.id = ?3) \
           AND EXISTS ( \
             SELECT 1 FROM projects p \
             JOIN integration_repositories r ON r.id = p.repository_id \
             JOIN integration_installations inst ON inst.id = r.installation_id \
             LEFT JOIN instance_preferences pref ON pref.id = 1 \
             WHERE p.id = builds.project_id \
               AND json_extract(builds.config_snapshot, '$.repository_id') = p.repository_id \
               AND COALESCE(pref.direct_macos_runner_paused, 0) = 0 \
               AND NOT EXISTS ( \
                 SELECT 1 FROM operator_incidents incident \
                 WHERE incident.status = 'open' \
                   AND incident.resource_kind = 'source' \
                   AND incident.resource_id = inst.integration_id \
                   AND incident.reason IN ('expired', 'rejected', 'refresh_failed') \
               ) \
           )",
    )
    .bind(now)
    .bind(build_id)
    .bind(runner_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!(error = %e, build_id = %build_id, "failed to schedule eligible build");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to claim build",
        )
    })?;

    if result.rows_affected() == 0 {
        tx.rollback().await.ok();
        return Ok(false);
    }

    sqlx::query(
        "INSERT INTO build_events \
         (id, build_id, from_status, to_status, actor, reason, created_at) \
         VALUES (?1, ?2, 'queued', 'scheduled', ?3, 'claimed by runner', ?4)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(build_id)
    .bind(actor)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!(error = %e, build_id = %build_id, "failed to record runner claim event");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to claim build",
        )
    })?;

    tx.commit().await.map_err(|e| {
        error!(error = %e, build_id = %build_id, "failed to commit runner claim");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to claim build",
        )
    })?;
    Ok(true)
}

async fn fail_credential_blocked_builds(
    pool: &sqlx::SqlitePool,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let blocked = sqlx::query(
        "SELECT b.id, incident.repair_url \
         FROM builds b \
         JOIN projects p ON p.id = b.project_id \
         JOIN integration_repositories r ON r.id = p.repository_id \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         JOIN operator_incidents incident \
           ON incident.resource_kind = 'source' \
          AND incident.resource_id = inst.integration_id \
          AND incident.status = 'open' \
          AND incident.reason IN ('expired', 'rejected', 'refresh_failed') \
         WHERE b.status = 'queued' \
           AND json_extract(b.config_snapshot, '$.repository_id') = p.repository_id",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| {
        error!(%error, "failed to run source credential build preflight");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to check source credentials",
        )
    })?;

    for row in blocked {
        let build_id: String = row.get("id");
        let repair_url: String = row.get("repair_url");
        let reason = format!("Source credentials require action. Repair: {repair_url}");
        let now = now_unix();
        let mut tx = pool.begin().await.map_err(|error| {
            error!(%error, build_id, "failed to start credential preflight failure");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fail build",
            )
        })?;
        let result = sqlx::query(
            "UPDATE builds SET status = 'failed', finished_at = ?1, updated_at = ?1 \
             WHERE id = ?2 AND status = 'queued'",
        )
        .bind(now)
        .bind(&build_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            error!(%error, build_id, "failed source credential build preflight");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fail build",
            )
        })?;
        if result.rows_affected() == 1 {
            sqlx::query(
                "INSERT INTO build_events \
                 (id, build_id, from_status, to_status, actor, reason, created_at) \
                 VALUES (?1, ?2, 'queued', 'failed', 'system:credential-preflight', ?3, ?4)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&build_id)
            .bind(&reason)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                error!(%error, build_id, "failed to record credential preflight event");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to fail build",
                )
            })?;
        }
        tx.commit().await.map_err(|error| {
            error!(%error, build_id, "failed to commit credential preflight failure");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fail build",
            )
        })?;
    }
    Ok(())
}

/// `POST /v1/runners/{runner_id}/claim` — runner claims next available build.
pub async fn claim_job(
    State(state): State<Arc<AppState>>,
    Path(runner_id): Path<String>,
    runner_auth: RunnerAuth,
    Json(req): Json<ClaimJobRequest>,
) -> ApiResult<ClaimJobResponse> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }
    if req.protocol_version != RUNNER_PROTOCOL_VERSION {
        return Err(api_err(
            StatusCode::CONFLICT,
            "runner_protocol_mismatch",
            format!(
                "Runner protocol {} is unsupported; expected {}",
                req.protocol_version, RUNNER_PROTOCOL_VERSION
            ),
        ));
    }

    let pool = &state.db;
    fail_credential_blocked_builds(pool).await?;

    // A missing repository remains ineligible. Missing preferences means the
    // ordinary running state. Filtering before ordering prevents blocked work
    // from starving later eligible builds.
    let build_row = sqlx::query(
        "SELECT b.* FROM builds b \
         JOIN projects p ON p.id = b.project_id \
         JOIN integration_repositories r ON r.id = p.repository_id \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         LEFT JOIN instance_preferences pref ON pref.id = 1 \
         WHERE b.status = 'queued' \
           AND json_extract(b.config_snapshot, '$.repository_id') = p.repository_id \
           AND COALESCE(pref.direct_macos_runner_paused, 0) = 0 \
           AND NOT EXISTS ( \
             SELECT 1 FROM operator_incidents incident \
             WHERE incident.status = 'open' \
               AND incident.resource_kind = 'source' \
               AND incident.resource_id = inst.integration_id \
               AND incident.reason IN ('expired', 'rejected', 'refresh_failed') \
           ) \
         ORDER BY b.queued_at ASC, b.id ASC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to query queued builds");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to query builds",
        )
    })?;

    let build_row = match build_row {
        Some(row) => row,
        None => return Ok(Json(ClaimJobResponse { job: None })),
    };

    let build_id: String = build_row.get("id");
    let project_id: String = build_row.get("project_id");
    let pipeline_id: String = build_row.get("pipeline_id");
    let build_number: i64 = build_row.get("build_number");
    let config_snapshot_str: String = build_row.get("config_snapshot");
    let mut config_snapshot: serde_json::Value =
        serde_json::from_str(&config_snapshot_str).unwrap_or_default();
    let commit_sha: Option<String> = build_row.get("commit_sha");
    let branch: Option<String> = build_row.get("branch");

    let source_row = sqlx::query(
        "SELECT i.provider, r.full_name \
         FROM builds b \
         JOIN integration_repositories r \
           ON r.id = json_extract(b.config_snapshot, '$.repository_id') \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         JOIN integrations i ON i.id = inst.integration_id \
         WHERE b.id = ?1",
    )
    .bind(&build_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, project_id = %project_id, "failed to resolve runner checkout source");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to resolve checkout",
        )
    })?;
    if let Some(source) = source_row
        && source.get::<String, _>("provider") == "gitlab"
        && let Some(snapshot) = config_snapshot.as_object_mut()
    {
        let full_name: String = source.get("full_name");
        let encoded_name = full_name
            .split('/')
            .map(|part| urlencoding::encode(part).into_owned())
            .collect::<Vec<_>>()
            .join("/");
        snapshot.insert(
            "checkout_proxy_path".to_string(),
            format!("/v1/runners/{runner_id}/jobs/{build_id}/gitlab/{encoded_name}.git").into(),
        );
    }

    let actor_str = format!("runner:{runner_id}");

    // Atomically transition queued -> scheduled while rechecking the policy
    // gates. A successful update is the linearization point for drain behavior.
    if !schedule_eligible_direct_runner_build(pool, &build_id, &runner_id, &actor_str).await? {
        return Ok(Json(ClaimJobResponse { job: None }));
    }

    // Complete the valid queued -> scheduled -> assigned sequence.
    let assigned = transition_build(
        pool,
        &build_id,
        BuildStatus::Assigned,
        Some(&actor_str),
        Some("assigned to runner"),
    )
    .await;

    match assigned {
        Ok(_) => {}
        Err(e) => {
            warn!(build_id = %build_id, error = ?e, "failed to transition build to assigned");
            return Ok(Json(ClaimJobResponse { job: None }));
        }
    }

    // Bind an unguessable signing grant to this active assignment. The raw
    // capability is returned once to the trusted runner parent and never
    // persisted or exposed to repository-controlled child processes.
    let signing_token = generate_token();
    let signing_token_hash = hash_token(&signing_token);
    let result = sqlx::query(
        "UPDATE builds SET runner_id = ?1, signing_token_hash = ?2 \
         WHERE id = ?3 AND status = 'assigned' AND runner_id IS NULL",
    )
    .bind(&runner_id)
    .bind(&signing_token_hash)
    .bind(&build_id)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, build_id = %build_id, "failed to set runner_id on build");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to assign runner to build",
        )
    })?;
    if result.rows_affected() != 1 {
        warn!(build_id = %build_id, "build assignment changed before signing grant was bound");
        return Ok(Json(ClaimJobResponse { job: None }));
    }

    let now = now_unix();
    let lease_expires_at = now + 300; // 5 minutes

    info!(
        build_id = %build_id,
        runner_id = %runner_id,
        build_number = build_number,
        "build claimed by runner"
    );

    Ok(Json(ClaimJobResponse {
        job: Some(ClaimedJob {
            build_id,
            project_id,
            pipeline_id,
            build_number,
            config_snapshot,
            commit_sha,
            branch,
            lease_expires_at,
            signing_token,
        }),
    }))
}

/// `POST /v1/runners/{runner_id}/jobs/{job_id}/status` — runner reports job status.
pub async fn update_job_status(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
    Json(req): Json<UpdateJobStatusRequest>,
) -> ApiResult<BuildDetailResponse> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }

    let pool = &state.db;

    // Verify build exists and belongs to this runner
    let build_row = sqlx::query("SELECT runner_id FROM builds WHERE id = ?1")
        .bind(&job_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch build");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch build",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;

    let build_runner_id: Option<String> = build_row.get("runner_id");
    if build_runner_id.as_deref() != Some(&runner_id) {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "This build is not assigned to your runner",
        ));
    }

    // Parse target status
    let target_status: BuildStatus = req.status.parse().map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_status",
            format!("Unknown build status: {}", req.status),
        )
    })?;

    // Runners can only set: Running, Succeeded, Failed
    if !matches!(
        target_status,
        BuildStatus::Running | BuildStatus::Succeeded | BuildStatus::Failed
    ) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_runner_status",
            format!(
                "Runners can only set status to running, succeeded, or failed (got: {})",
                req.status
            ),
        ));
    }

    let actor_str = format!("runner:{runner_id}");
    let reason = req
        .error_message
        .as_deref()
        .unwrap_or("status update from runner");

    // Use transition_build for state transition
    let mut build =
        transition_build(pool, &job_id, target_status, Some(&actor_str), Some(reason)).await?;

    // Persist step results and exit code
    if !req.steps.is_empty() || req.exit_code.is_some() {
        let steps_json = serde_json::to_string(&req.steps).unwrap_or_else(|_| "[]".to_string());
        sqlx::query("UPDATE builds SET step_results = ?1, exit_code = ?2 WHERE id = ?3")
            .bind(if req.steps.is_empty() {
                None
            } else {
                Some(&steps_json)
            })
            .bind(req.exit_code)
            .bind(&job_id)
            .execute(pool)
            .await
            .map_err(|e| {
                error!(error = %e, build_id = %job_id, "failed to persist step results");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to persist step results",
                )
            })?;

        if !req.steps.is_empty() {
            build.step_results = Some(req.steps.clone());
        }
        build.exit_code = req.exit_code;
    }

    // Fetch events for the response
    let events = fetch_build_events(pool, &job_id).await?;

    // Publish event for notification dispatch on terminal statuses
    if target_status.is_terminal() {
        state
            .scheduler
            .publish_event(crate::scheduler::BuildStateEvent {
                build_id: job_id.clone(),
                from_status: None,
                to_status: target_status.to_string(),
                actor: Some(actor_str.clone()),
                reason: Some(reason.to_string()),
                timestamp: crate::util::now_unix(),
            });
    }

    info!(
        build_id = %job_id,
        runner_id = %runner_id,
        new_status = %req.status,
        "runner updated job status"
    );

    Ok(Json(BuildDetailResponse { build, events }))
}

/// `GET /v1/runners/{runner_id}/jobs/{job_id}` — check build status (for runner cancellation polling).
pub async fn get_job_status(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
) -> ApiResult<JobStatusResponse> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }

    let pool = &state.db;

    let row = sqlx::query("SELECT status, runner_id FROM builds WHERE id = ?1")
        .bind(&job_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch build status");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch build",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;

    let build_runner_id: Option<String> = row.get("runner_id");
    if build_runner_id.as_deref() != Some(&runner_id) {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "This build is not assigned to your runner",
        ));
    }

    let status: String = row.get("status");
    Ok(Json(JobStatusResponse { status }))
}

/// `GET /v1/runners/{runner_id}` — get a single runner (admin/owner only).
pub async fn get_runner(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(runner_id): Path<String>,
) -> ApiResult<UpdateRunnerResponse> {
    check_permission(&state.enforcer, &auth.0.role, "runners", "read").await?;

    let pool = &state.db;

    let row = sqlx::query("SELECT * FROM runners WHERE id = ?1")
        .bind(&runner_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, runner_id = %runner_id, "failed to fetch runner");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch runner",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Runner not found"))?;

    Ok(Json(UpdateRunnerResponse {
        runner: row_to_runner(&row),
    }))
}

#[derive(Debug, Deserialize)]
pub struct ListRunnersQuery {
    pub q: Option<String>,
    pub sort: Option<String>,
    pub direction: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// `GET /v1/runners` — list runners (admin/owner only).
pub async fn list_runners(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListRunnersQuery>,
) -> ApiResult<ListRunnersResponse> {
    check_permission(&state.enforcer, &auth.0.role, "runners", "read").await?;

    let pool = &state.db;

    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let search = params.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
    let order = match params.sort.as_deref() {
        Some("name") => "name",
        Some("status") => "status",
        Some("last_heartbeat_at") => "last_heartbeat_at",
        _ => "created_at",
    };
    let direction = if params.direction.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };

    let mut count = QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM runners");
    if let Some(search) = search {
        count
            .push(" WHERE lower(name || ' ' || id || ' ' || status || ' ' || COALESCE(registered_by, 'embedded') || ' ' || capabilities_json) LIKE ")
            .push_bind(format!("%{}%", search.to_lowercase()));
    }
    let total: i64 = count
        .build_query_scalar()
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to count runners");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to list runners",
            )
        })?;
    let online_total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM runners WHERE status IN ('online', 'busy')")
            .fetch_one(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to count online runners");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to list runners",
                )
            })?;

    let mut query = QueryBuilder::<Sqlite>::new("SELECT * FROM runners");
    if let Some(search) = search {
        query
            .push(" WHERE lower(name || ' ' || id || ' ' || status || ' ' || COALESCE(registered_by, 'embedded') || ' ' || capabilities_json) LIKE ")
            .push_bind(format!("%{}%", search.to_lowercase()));
    }
    query
        .push(" ORDER BY ")
        .push(order)
        .push(" ")
        .push(direction)
        .push(", id ASC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows = query.build().fetch_all(pool).await.map_err(|e| {
        error!(error = %e, "failed to list runners");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to list runners",
        )
    })?;

    let runners = rows.iter().map(row_to_runner).collect();

    Ok(Json(ListRunnersResponse {
        runners,
        total,
        online_total,
    }))
}

/// `PATCH /v1/runners/{runner_id}` — rename a runner (admin/owner only).
pub async fn update_runner(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(runner_id): Path<String>,
    Json(req): Json<UpdateRunnerRequest>,
) -> ApiResult<UpdateRunnerResponse> {
    check_permission(&state.enforcer, &auth.0.role, "runners", "write").await?;

    let name = req
        .name
        .as_deref()
        .ok_or_else(|| {
            api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "Runner name is required",
            )
        })?
        .trim();

    if name.is_empty() || name.len() > 255 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "Runner name must be between 1 and 255 characters",
        ));
    }

    let now = now_unix();
    let pool = &state.db;

    let existing = sqlx::query("SELECT name, registered_by FROM runners WHERE id = ?1")
        .bind(&runner_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, runner_id = %runner_id, "failed to fetch runner");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch runner",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Runner not found"))?;

    let previous_name: String = existing.get("name");
    let registered_by: Option<String> = existing.get("registered_by");

    if registered_by.is_none() {
        return Err(api_err(
            StatusCode::CONFLICT,
            "managed_runner_locked",
            "Managed runner name cannot be changed",
        ));
    }

    if previous_name != name {
        sqlx::query("UPDATE runners SET name = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(name)
            .bind(now)
            .bind(&runner_id)
            .execute(pool)
            .await
            .map_err(|e| {
                error!(error = %e, runner_id = %runner_id, "failed to rename runner");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to update runner",
                )
            })?;

        let details = serde_json::json!({
            "previous_name": previous_name,
            "new_name": name,
            "updated_by": auth.0.email,
        })
        .to_string();
        let _ = write_audit_log(
            pool,
            Some(&auth.0.user_id),
            "runner_renamed",
            "runner",
            Some(&runner_id),
            Some(&details),
        )
        .await;

        info!(
            runner_id = %runner_id,
            previous_name = %previous_name,
            new_name = %name,
            updated_by = %auth.0.email,
            "runner renamed"
        );
    }

    let row = sqlx::query("SELECT * FROM runners WHERE id = ?1")
        .bind(&runner_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, runner_id = %runner_id, "failed to reload runner");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load runner",
            )
        })?;

    Ok(Json(UpdateRunnerResponse {
        runner: row_to_runner(&row),
    }))
}

/// `DELETE /v1/runners/{runner_id}` — delete an unused user-registered runner.
pub async fn delete_runner(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(runner_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    check_permission(&state.enforcer, &auth.0.role, "runners", "delete").await?;

    let pool = &state.db;
    let mut transaction = pool.begin().await.map_err(|error| {
        error!(%error, runner_id = %runner_id, "failed to begin runner delete transaction");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete runner",
        )
    })?;
    let runner_actor = format!("runner:{runner_id}");
    let deleted = sqlx::query(
        "DELETE FROM runners \
         WHERE id = ?1 \
           AND registered_by IS NOT NULL \
           AND NOT EXISTS (SELECT 1 FROM builds WHERE runner_id = ?1) \
           AND NOT EXISTS ( \
             SELECT 1 FROM build_events WHERE actor = ?2 \
           )",
    )
    .bind(&runner_id)
    .bind(&runner_actor)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        error!(%error, runner_id = %runner_id, "failed to delete runner");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete runner",
        )
    })?;

    if deleted.rows_affected() == 0 {
        let existing = sqlx::query(
            "SELECT registered_by, \
                    EXISTS (SELECT 1 FROM builds WHERE runner_id = ?1) \
                      OR EXISTS (SELECT 1 FROM build_events WHERE actor = ?2) AS has_builds \
             FROM runners WHERE id = ?1",
        )
        .bind(&runner_id)
        .bind(&runner_actor)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| {
            error!(%error, runner_id = %runner_id, "failed to inspect runner after delete refusal");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to delete runner",
            )
        })?;
        transaction.rollback().await.ok();

        let Some(existing) = existing else {
            return Err(api_err(
                StatusCode::NOT_FOUND,
                "not_found",
                "Runner not found",
            ));
        };
        let registered_by: Option<String> = existing.get("registered_by");
        if registered_by.is_none() {
            return Err(api_err(
                StatusCode::CONFLICT,
                "managed_runner_locked",
                "Managed runners cannot be deleted through this endpoint",
            ));
        }
        let has_builds: bool = existing.get("has_builds");
        if has_builds {
            return Err(api_err(
                StatusCode::CONFLICT,
                "runner_has_builds",
                "Runners with builds cannot be deleted",
            ));
        }
        return Err(api_err(
            StatusCode::CONFLICT,
            "runner_delete_conflict",
            "Runner state changed before it could be deleted",
        ));
    }

    let details = serde_json::json!({
        "deleted_by": auth.0.email,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO audit_logs \
         (actor_id, action, resource_type, resource_id, details, created_at) \
         VALUES (?1, 'runner_deleted', 'runner', ?2, ?3, ?4)",
    )
    .bind(&auth.0.user_id)
    .bind(&runner_id)
    .bind(&details)
    .bind(now_unix())
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        error!(%error, runner_id = %runner_id, "failed to audit runner deletion");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete runner",
        )
    })?;
    transaction.commit().await.map_err(|error| {
        error!(%error, runner_id = %runner_id, "failed to commit runner deletion");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete runner",
        )
    })?;

    info!(
        runner_id = %runner_id,
        deleted_by = %auth.0.email,
        "runner deleted"
    );
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::settle_cache_purge_request;

    #[tokio::test]
    async fn purge_request_clears_only_once_the_runner_caught_up() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO runners (id, name, token_hash, status, capabilities, created_at, updated_at, cache_purge_requested_at) \
             VALUES ('r1', 'Mac Studio', 'hash', 'online', '{}', 0, 0, 100)",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            settle_cache_purge_request(&pool, "r1", None).await.unwrap(),
            Some(100)
        );
        assert_eq!(
            settle_cache_purge_request(&pool, "r1", Some(99))
                .await
                .unwrap(),
            Some(100)
        );
        assert_eq!(
            settle_cache_purge_request(&pool, "r1", Some(100))
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            settle_cache_purge_request(&pool, "r1", Some(100))
                .await
                .unwrap(),
            None
        );
    }
}
