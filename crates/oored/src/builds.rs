use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, ApprovalPolicy, Build, BuildApproval, BuildApprovalRequest, BuildApprovalResponse,
    BuildApprovalState, BuildChangelogPreviewResponse, BuildContext, BuildDetailResponse,
    BuildEvent, BuildPlatform, BuildStatus, CancelBuildResponse, ConcurrencyPolicy,
    CreateBuildRequest, CreateBuildResponse, DEFAULT_BUILD_TIMEOUT_MINUTES, ListBuildsResponse,
    PipelineExecutionConfig, RerunBuildResponse, RunnerPolicyBlockReason, ScheduleTrigger,
    TriggerConfig, scheduling_labels,
};
use serde::Deserialize;
use sqlx::Row;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;
const REDACTED_ENV_VALUE: &str = "[REDACTED]";

// ── Row conversion helpers ──────────────────────────────────────

pub(crate) fn row_to_build(row: &sqlx::sqlite::SqliteRow) -> Build {
    let config_snapshot_str: String = row.get("config_snapshot");
    let config_snapshot: serde_json::Value =
        serde_json::from_str(&config_snapshot_str).unwrap_or(serde_json::json!({}));

    let step_results_str: Option<String> = row.get("step_results");
    let step_results = step_results_str.and_then(|s| serde_json::from_str(&s).ok());
    let context = BuildContext {
        project_name: row.try_get("project_name").ok(),
        repository_id: row.try_get("repository_id").ok(),
        project_avatar_url: row.try_get("project_avatar_url").ok(),
        repository_full_name: row.try_get("repository_full_name").ok(),
        repository_provider: row.try_get("repository_provider").ok(),
        repository_host_url: row.try_get("repository_host_url").ok(),
        pipeline_name: row.try_get("pipeline_name").ok(),
        runner_name: row.try_get("runner_name").ok(),
    };
    let context = (context.project_name.is_some()
        || context.repository_id.is_some()
        || context.project_avatar_url.is_some()
        || context.repository_full_name.is_some()
        || context.repository_provider.is_some()
        || context.repository_host_url.is_some()
        || context.pipeline_name.is_some()
        || context.runner_name.is_some())
    .then_some(context);

    let approval = row
        .try_get::<Option<String>, _>("approval_state")
        .ok()
        .flatten()
        .and_then(|state| state.parse::<BuildApprovalState>().ok())
        .map(|state| BuildApproval {
            state,
            requested_at: row
                .try_get::<Option<i64>, _>("approval_requested_at")
                .ok()
                .flatten()
                .unwrap_or_default(),
            decided_by: row.try_get("approval_decided_by").ok().flatten(),
            decided_at: row.try_get("approval_decided_at").ok().flatten(),
            reason: row.try_get("approval_reason").ok().flatten(),
        });

    Build {
        id: row.get("id"),
        project_id: row.get("project_id"),
        pipeline_id: row.get("pipeline_id"),
        build_number: row.get("build_number"),
        status: row.get("status"),
        runner_policy_block_reason: row
            .try_get::<Option<String>, _>("runner_policy_block_reason")
            .ok()
            .flatten()
            .and_then(|reason| reason.parse::<RunnerPolicyBlockReason>().ok()),
        trigger_type: row.get("trigger_type"),
        trigger_actor: row.get("trigger_actor"),
        trigger_event: row.get("trigger_event"),
        trigger_ref: row.get("trigger_ref"),
        commit_sha: row.get("commit_sha"),
        branch: row.get("branch"),
        changelog: row.get("changelog"),
        source_build_id: row.get("source_build_id"),
        retry_steps: crate::step_retry::retry_steps_from_snapshot(&config_snapshot),
        approval,
        config_snapshot,
        runner_id: row.get("runner_id"),
        context,
        step_results,
        exit_code: row.get("exit_code"),
        queued_at: row.get("queued_at"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_redacted_build(row: &sqlx::sqlite::SqliteRow) -> Build {
    let mut build = row_to_build(row);
    if let Some(entries) = build
        .config_snapshot
        .pointer_mut("/ui_execution_config/env")
        .and_then(serde_json::Value::as_array_mut)
    {
        for entry in entries {
            if let Some(value) = entry.get_mut("value") {
                *value = serde_json::Value::String(REDACTED_ENV_VALUE.to_string());
            }
        }
    }
    build
}

async fn runner_policy_block_reason_for_project(
    pool: &sqlx::SqlitePool,
    project_id: &str,
) -> Result<Option<RunnerPolicyBlockReason>, (StatusCode, Json<ApiError>)> {
    let reason: Option<String> = sqlx::query_scalar(
        "SELECT CASE \
           WHEN COALESCE(pref.direct_macos_runner_paused, 0) = 1 THEN 'instance_paused' \
           WHEN r.id IS NULL THEN 'repository_unavailable' \
           ELSE NULL \
         END \
         FROM projects p \
         LEFT JOIN integration_repositories r ON r.id = p.repository_id \
         LEFT JOIN instance_preferences pref ON pref.id = 1 \
         WHERE p.id = ?1",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!(error = %e, project_id = %project_id, "failed to derive runner policy status");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to derive runner policy status",
        )
    })?;

    Ok(reason.and_then(|value| value.parse().ok()))
}

fn row_to_build_event(row: &sqlx::sqlite::SqliteRow) -> BuildEvent {
    BuildEvent {
        id: row.get("id"),
        build_id: row.get("build_id"),
        from_status: row.get("from_status"),
        to_status: row.get("to_status"),
        actor: row.get("actor"),
        reason: row.get("reason"),
        created_at: row.get("created_at"),
    }
}

pub(crate) async fn fetch_build_events(
    pool: &sqlx::SqlitePool,
    build_id: &str,
) -> Result<Vec<BuildEvent>, (StatusCode, Json<ApiError>)> {
    let event_rows =
        sqlx::query("SELECT * FROM build_events WHERE build_id = ?1 ORDER BY created_at ASC")
            .bind(build_id)
            .fetch_all(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to fetch build events");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to fetch build events",
                )
            })?;

    Ok(event_rows.iter().map(row_to_build_event).collect())
}

// ── Build state machine ─────────────────────────────────────────

/// Transition a build to a new status with optimistic locking.
///
/// Uses `WHERE status = ?` in the UPDATE to prevent concurrent transitions.
/// Returns the updated build or 409 Conflict if the transition is invalid.
pub async fn transition_build(
    pool: &sqlx::SqlitePool,
    build_id: &str,
    target_status: BuildStatus,
    actor: Option<&str>,
    reason: Option<&str>,
) -> Result<Build, (StatusCode, Json<ApiError>)> {
    transition_build_with_results(pool, build_id, target_status, actor, reason, None).await
}

/// Step results and exit code a runner reports with a status change.
pub(crate) struct RunnerResults<'a> {
    pub step_results: Option<&'a str>,
    pub exit_code: Option<i32>,
}

/// [`transition_build`], also storing the runner's results. The status
/// change, its build event and the results commit together or not at all.
pub(crate) async fn transition_build_with_results(
    pool: &sqlx::SqlitePool,
    build_id: &str,
    target_status: BuildStatus,
    actor: Option<&str>,
    reason: Option<&str>,
    results: Option<RunnerResults<'_>>,
) -> Result<Build, (StatusCode, Json<ApiError>)> {
    let store_error = |message: &'static str| {
        move |e: sqlx::Error| {
            error!(error = %e, build_id = %build_id, "{message}");
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", message)
        }
    };

    // Load current build
    let row = sqlx::query("SELECT * FROM builds WHERE id = ?1")
        .bind(build_id)
        .fetch_optional(pool)
        .await
        .map_err(store_error("Failed to fetch build"))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;

    let current_status_str: String = row.get("status");
    let current_status = current_status_str.parse::<BuildStatus>().map_err(|_| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid_status",
            format!("Build has invalid status: {current_status_str}"),
        )
    })?;

    // Validate transition
    if !current_status.can_transition_to(target_status) {
        return Err(api_err(
            StatusCode::CONFLICT,
            "invalid_transition",
            format!(
                "Cannot transition from {} to {}",
                current_status, target_status
            ),
        ));
    }

    let now = now_unix();
    let target_str = target_status.to_string();

    // Set timing fields for specific transitions
    let started_at_update = if target_status == BuildStatus::Running {
        "started_at = ?4,"
    } else {
        ""
    };
    let finished_at_update = if target_status.is_terminal() && !current_status.is_terminal() {
        "finished_at = ?4,"
    } else {
        ""
    };
    let runner_revoke_update =
        if target_status.is_terminal() || target_status == BuildStatus::Queued {
            "runner_id = NULL, signing_token_hash = NULL,"
        } else {
            ""
        };

    // Optimistic locking: WHERE status = current_status
    let query = format!(
        "UPDATE builds SET status = ?1, {started_at_update} {finished_at_update} {runner_revoke_update} updated_at = ?4 \
         WHERE id = ?2 AND status = ?3"
    );

    // The UPDATE is the transaction's first statement, so SQLite takes the
    // write lock against the latest snapshot.
    let mut tx = pool
        .begin()
        .await
        .map_err(store_error("Failed to update build"))?;
    let result = sqlx::query(sqlx::AssertSqlSafe(query))
        .bind(&target_str)
        .bind(build_id)
        .bind(&current_status_str)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(store_error("Failed to update build"))?;

    if result.rows_affected() == 0 {
        return Err(api_err(
            StatusCode::CONFLICT,
            "concurrent_modification",
            "Build status was modified concurrently",
        ));
    }

    if let Some(results) = results {
        sqlx::query("UPDATE builds SET step_results = ?1, exit_code = ?2 WHERE id = ?3")
            .bind(results.step_results)
            .bind(results.exit_code)
            .bind(build_id)
            .execute(&mut *tx)
            .await
            .map_err(store_error("Failed to persist step results"))?;
    }

    // Insert build event
    let event_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO build_events (id, build_id, from_status, to_status, actor, reason, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&event_id)
    .bind(build_id)
    .bind(&current_status_str)
    .bind(&target_str)
    .bind(actor)
    .bind(reason)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(store_error("Failed to record build event"))?;

    // Reload build
    let updated_row = sqlx::query("SELECT * FROM builds WHERE id = ?1")
        .bind(build_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(store_error("Failed to reload build"))?;

    tx.commit()
        .await
        .map_err(store_error("Failed to update build"))?;

    Ok(row_to_build(&updated_row))
}

/// Create a config snapshot capturing pipeline config at build creation time.
#[allow(clippy::too_many_arguments)]
fn create_config_snapshot(
    pipeline_config_path: &str,
    config_path_explicit: bool,
    ui_execution_config: &PipelineExecutionConfig,
    timeout_minutes: u32,
    trigger_type: &str,
    commit_sha: Option<&str>,
    branch: Option<&str>,
    source: (&str, &str),
) -> serde_json::Value {
    let (repository_id, repo_url) = source;
    let ui_execution_config_json =
        serde_json::to_value(ui_execution_config).unwrap_or_else(|_| serde_json::json!({}));
    serde_json::json!({
        "snapshot_version": 2,
        "config_resolution_policy": "file_first_ui_fallback",
        "config_path": pipeline_config_path,
        "config_path_explicit": config_path_explicit,
        "ui_execution_config": ui_execution_config_json,
        "artifact_patterns": ui_execution_config.artifact_patterns,
        "runs_on": scheduling_labels(&ui_execution_config.runs_on),
        "trigger_type": trigger_type,
        "timeout_secs": i64::from(timeout_minutes) * 60,
        "commit_sha": commit_sha,
        "branch": branch,
        "repo_url": repo_url,
        "repository_id": repository_id,
        "captured_at": now_unix(),
    })
}

fn validate_selected_platforms(
    requested: Option<Vec<BuildPlatform>>,
    configured: &[BuildPlatform],
) -> Result<Option<Vec<BuildPlatform>>, (StatusCode, Json<ApiError>)> {
    let Some(requested) = requested else {
        return Ok(None);
    };
    if requested.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Select at least one platform for this build.",
        ));
    }

    let mut selected = Vec::with_capacity(requested.len());
    for platform in configured {
        if requested.contains(platform) {
            selected.push(platform.clone());
        }
    }
    let has_duplicate = requested
        .iter()
        .enumerate()
        .any(|(index, platform)| requested[..index].contains(platform));
    if has_duplicate || selected.len() != requested.len() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Selected platforms must be unique and configured on this pipeline.",
        ));
    }

    Ok(Some(selected))
}

fn parse_execution_config(raw: &str) -> PipelineExecutionConfig {
    serde_json::from_str(raw).unwrap_or_else(|_| PipelineExecutionConfig::default())
}

/// Timeout and approval policy from a `pipelines` row.
fn pipeline_gates(row: &sqlx::sqlite::SqliteRow) -> (u32, ApprovalPolicy) {
    let timeout_minutes = row
        .try_get::<i64, _>("timeout_minutes")
        .ok()
        .and_then(|minutes| u32::try_from(minutes).ok())
        .filter(|minutes| *minutes > 0)
        .unwrap_or(DEFAULT_BUILD_TIMEOUT_MINUTES);
    let approval = row
        .try_get::<String, _>("approval_policy")
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    (timeout_minutes, approval)
}

fn pending_approval(approval_required: bool, now: i64) -> Option<BuildApproval> {
    approval_required.then_some(BuildApproval {
        state: BuildApprovalState::Pending,
        requested_at: now,
        decided_by: None,
        decided_at: None,
        reason: None,
    })
}

fn initial_event_reason(reason: &str, approval_required: bool) -> String {
    if approval_required {
        format!("{reason}; waiting for approval")
    } else {
        reason.to_string()
    }
}

/// Audit and notify that a new build is waiting for approval.
async fn announce_pending_approval(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    build_id: &str,
    actor_user_id: Option<&str>,
) {
    let details = serde_json::json!({ "build_id": build_id }).to_string();
    let _ = write_audit_log(
        pool,
        actor_user_id,
        "build_approval_requested",
        "build",
        Some(build_id),
        Some(&details),
    )
    .await;
    let pool = pool.clone();
    let encryption_key = encryption_key.to_vec();
    let build_id = build_id.to_string();
    tokio::spawn(async move {
        if let Err(error) = crate::notification_dispatch::dispatch_build_approval_needed(
            &pool,
            &encryption_key,
            &build_id,
        )
        .await
        {
            error!(%error, %build_id, "approval notification dispatch error");
        }
    });
}

/// Clone URL of a linked repository, or `None` when it cannot be resolved.
async fn resolve_repository_clone_url(
    pool: &sqlx::SqlitePool,
    repository_id: &str,
) -> Result<Option<String>, (StatusCode, Json<ApiError>)> {
    let repo_url: Option<String> = sqlx::query_scalar(
        "SELECT CASE \
            WHEN i.provider = 'local_git' THEN r.html_url \
            ELSE i.host_url || '/' || r.full_name || '.git' \
         END \
         FROM integration_repositories r \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         JOIN integrations i ON i.id = inst.integration_id \
         WHERE r.id = ?1",
    )
    .bind(repository_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, repository_id = %repository_id, "failed to resolve repository URL");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to resolve project source",
        )
    })?;
    Ok(repo_url
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty()))
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

async fn fetch_build_number(
    pool: &sqlx::SqlitePool,
    build_id: &str,
) -> Result<i64, (StatusCode, Json<ApiError>)> {
    sqlx::query_scalar("SELECT build_number FROM builds WHERE id = ?1")
        .bind(build_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, build_id = %build_id, "failed to fetch build number");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch build number",
            )
        })
}

// ── Build insert with retry ──────────────────────────────────────

/// Maximum number of retry attempts for build inserts.
///
/// Concurrent webhook bursts can cause both SQLITE_BUSY and UNIQUE violations
/// on the atomic `build_number` subquery, so we allow enough retries to absorb
/// realistic burst sizes (up to ~10 concurrent inserts per project).
const BUILD_INSERT_MAX_RETRIES: u32 = 10;

/// Insert a build row with retry on SQLITE_BUSY or UNIQUE constraint violations.
///
/// Retries up to `BUILD_INSERT_MAX_RETRIES` times with exponential backoff.
async fn insert_build_with_retry(
    pool: &sqlx::SqlitePool,
    binds: &BuildInsertBinds<'_>,
) -> Result<bool, (StatusCode, Json<ApiError>)> {
    for attempt in 1..=BUILD_INSERT_MAX_RETRIES {
        match execute_build_insert(pool, "", binds).await {
            Ok(inserted) => return Ok(inserted),
            Err(e) => {
                let is_retryable = is_sqlite_busy(&e) || is_sqlite_unique_violation(&e);
                if is_retryable && attempt < BUILD_INSERT_MAX_RETRIES {
                    warn!(attempt = attempt, error = %e, "retryable SQLite error on build insert, retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(10 * attempt as u64)).await;
                    continue;
                }
                error!(error = %e, "failed to create build after {} attempts", attempt);
                return Err(api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to create build",
                ));
            }
        }
    }
    unreachable!()
}

/// Check if an sqlx error is SQLITE_BUSY (error code 5).
fn is_sqlite_busy(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = e
        && let Some(code) = db_err.code()
    {
        return code.as_ref() == "5";
    }
    false
}

/// Check if an sqlx error is SQLITE_CONSTRAINT_UNIQUE (error code 2067).
fn is_sqlite_unique_violation(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = e
        && let Some(code) = db_err.code()
    {
        return code.as_ref() == "2067";
    }
    false
}

/// Bind values for build INSERT statements.
struct BuildInsertBinds<'a> {
    build_id: &'a str,
    project_id: &'a str,
    pipeline_id: &'a str,
    actor: Option<&'a str>,
    event_type: Option<&'a str>,
    trigger_ref: Option<&'a str>,
    commit_sha: Option<&'a str>,
    branch: Option<&'a str>,
    changelog: Option<&'a str>,
    config_snapshot: &'a str,
    webhook_id: Option<&'a str>,
    now: i64,
    trigger_type: &'a str,
    source_build_id: Option<&'a str>,
    cancel_previous: bool,
    /// Hold the build for a maintainer's approval before runners see it.
    approval_required: bool,
}

async fn execute_build_insert(
    pool: &sqlx::SqlitePool,
    _query: &str,
    b: &BuildInsertBinds<'_>,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, \
         trigger_type, trigger_actor, trigger_event, trigger_ref, commit_sha, branch, \
         changelog, config_snapshot, webhook_id, source_build_id, queued_at, created_at, updated_at, \
         approval_state, approval_requested_at) \
         SELECT ?1, ?2, ?3, \
                (SELECT COALESCE(MAX(build_number), 0) + 1 FROM builds WHERE project_id = ?2), \
                'queued', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14, ?14, \
                CASE WHEN ?15 THEN 'pending' END, CASE WHEN ?15 THEN ?14 END \
         WHERE EXISTS ( \
           SELECT 1 FROM projects p \
           JOIN integration_repositories r ON r.id = p.repository_id \
           WHERE p.id = ?2 \
             AND p.repository_id = json_extract(?11, '$.repository_id') \
         )",
    )
    .bind(b.build_id)
    .bind(b.project_id)
    .bind(b.pipeline_id)
    .bind(b.trigger_type)
    .bind(b.actor)
    .bind(b.event_type)
    .bind(b.trigger_ref)
    .bind(b.commit_sha)
    .bind(b.branch)
    .bind(b.changelog)
    .bind(b.config_snapshot)
    .bind(b.webhook_id)
    .bind(b.source_build_id)
    .bind(b.now)
    .bind(b.approval_required)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() != 1 {
        tx.rollback().await?;
        return Ok(false);
    }

    if let Some(webhook_id) = b.webhook_id {
        let claim = sqlx::query(
            "INSERT INTO webhook_build_claims (webhook_id, pipeline_id, build_id) \
             VALUES (?1, ?2, ?3) ON CONFLICT(webhook_id, pipeline_id) DO NOTHING",
        )
        .bind(webhook_id)
        .bind(b.pipeline_id)
        .bind(b.build_id)
        .execute(&mut *tx)
        .await?;
        if claim.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(false);
        }
    }

    if b.cancel_previous
        && let Some(branch) = b.branch.filter(|value| !value.is_empty())
    {
        let canceled = sqlx::query(
            "INSERT INTO build_events \
             (id, build_id, from_status, to_status, actor, reason, created_at) \
             SELECT lower(hex(randomblob(16))), id, status, 'canceled', ?1, \
                    'superseded by new build', ?2 \
             FROM builds \
             WHERE pipeline_id = ?3 AND branch = ?4 AND id != ?5 \
               AND build_number < (SELECT build_number FROM builds WHERE id = ?5) \
               AND json_extract(config_snapshot, '$.repository_id') = \
                   json_extract(?6, '$.repository_id') \
               AND status NOT IN ('succeeded', 'failed', 'canceled', 'timed_out', 'expired')",
        )
        .bind(b.actor)
        .bind(b.now)
        .bind(b.pipeline_id)
        .bind(branch)
        .bind(b.build_id)
        .bind(b.config_snapshot)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        sqlx::query(
            "UPDATE builds \
             SET status = 'canceled', runner_id = NULL, signing_token_hash = NULL, \
                 finished_at = ?1, updated_at = ?1 \
             WHERE pipeline_id = ?2 AND branch = ?3 AND id != ?4 \
               AND build_number < (SELECT build_number FROM builds WHERE id = ?4) \
               AND json_extract(config_snapshot, '$.repository_id') = \
                   json_extract(?5, '$.repository_id') \
               AND status NOT IN ('succeeded', 'failed', 'canceled', 'timed_out', 'expired')",
        )
        .bind(b.now)
        .bind(b.pipeline_id)
        .bind(branch)
        .bind(b.build_id)
        .bind(b.config_snapshot)
        .execute(&mut *tx)
        .await?;

        if canceled > 0 {
            info!(canceled, build_id = %b.build_id, "auto-canceled source-matched superseded builds");
        }
    }

    tx.commit().await?;
    Ok(true)
}

// ── Query parameters ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListBuildsQuery {
    pub project_id: Option<String>,
    pub pipeline_id: Option<String>,
    pub status: Option<String>,
    pub branch: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub sort: Option<String>,
    pub direction: Option<String>,
}

fn build_order_clause(
    sort: Option<&str>,
    direction: Option<&str>,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let column = match sort.unwrap_or("created_at") {
        "created_at" => "builds.created_at",
        "status" => "builds.status COLLATE NOCASE",
        "project_name" => "projects.name COLLATE NOCASE",
        "pipeline_name" => "pipelines.name COLLATE NOCASE",
        "branch" => "builds.branch COLLATE NOCASE",
        _ => {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "sort must be created_at, status, project_name, pipeline_name, or branch",
            ));
        }
    };
    let direction = match direction.unwrap_or("desc") {
        "asc" => "ASC",
        "desc" => "DESC",
        _ => {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "direction must be asc or desc",
            ));
        }
    };

    Ok(format!("{column} {direction}, builds.id {direction}"))
}

#[derive(Debug, Deserialize)]
pub struct BuildChangelogPreviewQuery {
    pub pipeline_id: String,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
}

fn escape_markdown_inline(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('<', "\\<")
        .replace('>', "\\>")
}

fn changelog_markdown(commits: Vec<crate::integrations::CommitSummary>) -> String {
    let mut markdown = String::new();
    let total = commits.len();
    for (index, commit) in commits.into_iter().enumerate() {
        let title = commit
            .title
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let author = commit
            .author
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if title.is_empty() {
            continue;
        }
        let line = format!(
            "- {} — {}\n",
            escape_markdown_inline(&title),
            escape_markdown_inline(&author)
        );
        if markdown.len() + line.len() > 3_900 {
            let remaining = total.saturating_sub(index);
            markdown.push_str(&format!("- …and {remaining} more changes\n"));
            break;
        }
        markdown.push_str(&line);
    }
    markdown.trim_end().to_string()
}

// ── Handlers ────────────────────────────────────────────────────

/// `GET /v1/projects/{project_id}/builds/changelog-preview` — draft release notes.
pub async fn preview_build_changelog(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Query(params): Query<BuildChangelogPreviewQuery>,
) -> ApiResult<BuildChangelogPreviewResponse> {
    let pool = state.db.clone();
    let effective = resolve_effective_project_role(
        &pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::TriggerBuild)?;

    let project = sqlx::query("SELECT repository_id, default_branch FROM projects WHERE id = ?1")
        .bind(&project_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch project for changelog preview");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch project",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Project not found"))?;
    let repository_id: Option<String> = project.get("repository_id");
    let repository_id = repository_id.ok_or_else(|| {
        api_err(
            StatusCode::CONFLICT,
            "source_not_configured",
            "Project is not linked to a source repository",
        )
    })?;
    let pipeline_exists: bool =
        sqlx::query_scalar("SELECT COUNT(*) > 0 FROM pipelines WHERE id = ?1 AND project_id = ?2")
            .bind(&params.pipeline_id)
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap_or(false);
    if !pipeline_exists {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Pipeline not found in this project",
        ));
    }

    let commit_sha = normalize_optional(params.commit_sha);
    let branch = normalize_optional(params.branch)
        .or_else(|| normalize_optional(project.get::<Option<String>, _>("default_branch")));
    let target_commit = match commit_sha {
        Some(commit_sha) => commit_sha,
        None => {
            crate::integrations::resolve_branch_commit(
                &pool,
                &state.encryption_key,
                &repository_id,
                branch.as_deref().ok_or_else(|| {
                    api_err(
                        StatusCode::BAD_REQUEST,
                        "invalid_input",
                        "Provide branch or commit_sha",
                    )
                })?,
            )
            .await?
        }
    };
    let base_commit: Option<String> = sqlx::query_scalar(
        "SELECT commit_sha FROM builds \
         WHERE project_id = ?1 AND pipeline_id = ?2 AND status = 'succeeded' \
           AND commit_sha IS NOT NULL \
         ORDER BY build_number DESC LIMIT 1",
    )
    .bind(&project_id)
    .bind(&params.pipeline_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to fetch previous build revision");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to fetch previous build revision",
        )
    })?;
    let markdown = match base_commit.as_deref() {
        Some(base) if base != target_commit => changelog_markdown(
            crate::integrations::compare_commits(
                &pool,
                &state.encryption_key,
                &repository_id,
                base,
                &target_commit,
            )
            .await?,
        ),
        _ => String::new(),
    };

    Ok(Json(BuildChangelogPreviewResponse {
        base_commit,
        target_commit,
        markdown,
    }))
}

/// `POST /v1/projects/{project_id}/builds` — create a new build (manual/API trigger).
pub async fn create_build(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(req): Json<CreateBuildRequest>,
) -> Result<(StatusCode, Json<CreateBuildResponse>), (StatusCode, Json<ApiError>)> {
    let pool = state.db.clone();

    let effective = resolve_effective_project_role(
        &pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::TriggerBuild)?;

    let requested_branch = normalize_optional(req.branch);
    let mut commit_sha = normalize_optional(req.commit_sha);
    let trigger_ref = normalize_optional(req.trigger_ref);
    let changelog = normalize_optional(req.changelog);
    if changelog.as_ref().is_some_and(|value| value.len() > 4_000) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Changelog must be 4,000 characters or fewer",
        ));
    }

    // Verify project exists and has source linkage.
    let project_row =
        sqlx::query("SELECT repository_id, default_branch FROM projects WHERE id = ?1")
            .bind(&project_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to fetch project");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to fetch project",
                )
            })?
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Project not found"))?;

    let repository_id: Option<String> = project_row.get("repository_id");
    if repository_id.is_none() {
        return Err(api_err(
            StatusCode::CONFLICT,
            "source_not_configured",
            "Project is not linked to a source repository. Link a repository before triggering builds.",
        ));
    }

    let default_branch = normalize_optional(project_row.get::<Option<String>, _>("default_branch"));
    let branch = requested_branch.or_else(|| {
        if commit_sha.is_none() {
            default_branch.clone()
        } else {
            None
        }
    });
    if commit_sha.is_none() && branch.is_none() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Provide branch or commit_sha, or set a project default_branch before triggering builds.",
        ));
    }
    let trigger_ref = trigger_ref.or_else(|| branch.clone());

    // Verify pipeline exists and belongs to this project
    let pipeline_row = sqlx::query(
        "SELECT id, config_path, config_path_explicit, execution_config, concurrency, \
                timeout_minutes, approval_policy \
         FROM pipelines WHERE id = ?1 AND project_id = ?2",
    )
    .bind(&req.pipeline_id)
    .bind(&project_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to fetch pipeline");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to fetch pipeline",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Pipeline not found in this project",
        )
    })?;

    let config_path: String = pipeline_row.get("config_path");
    let config_path_explicit: i32 = pipeline_row.try_get("config_path_explicit").unwrap_or(0);
    let execution_config_json: String = pipeline_row
        .try_get("execution_config")
        .unwrap_or_else(|_| "{}".to_string());
    let execution_config = parse_execution_config(&execution_config_json);
    let selected_platforms =
        validate_selected_platforms(req.platforms, &execution_config.platforms)?;
    let concurrency_json: String = pipeline_row.get("concurrency");
    let concurrency: ConcurrencyPolicy =
        serde_json::from_str(&concurrency_json).unwrap_or_default();
    let (timeout_minutes, approval_policy) = pipeline_gates(&pipeline_row);
    let approval_required = approval_policy.applies_to("manual");

    if commit_sha.is_none() {
        commit_sha = Some(
            crate::integrations::resolve_branch_commit(
                &pool,
                &state.encryption_key,
                repository_id
                    .as_deref()
                    .expect("repository linkage checked"),
                branch.as_deref().expect("branch presence checked"),
            )
            .await?,
        );
    }

    let now = now_unix();
    let build_id = Uuid::new_v4().to_string();

    let repo_url = resolve_repository_clone_url(
        &pool,
        repository_id
            .as_deref()
            .expect("repository linkage checked"),
    )
    .await?;
    if repo_url.is_none() {
        return Err(api_err(
            StatusCode::CONFLICT,
            "source_unresolvable",
            "Project source repository could not be resolved. Reconnect the source integration and repository mapping.",
        ));
    }

    let mut config_snapshot = create_config_snapshot(
        &config_path,
        config_path_explicit != 0,
        &execution_config,
        timeout_minutes,
        "manual",
        commit_sha.as_deref(),
        branch.as_deref(),
        (
            repository_id
                .as_deref()
                .expect("repository linkage checked"),
            repo_url.as_deref().expect("repository URL checked"),
        ),
    );
    if let Some(selected_platforms) = selected_platforms {
        config_snapshot["selected_platforms"] = serde_json::json!(selected_platforms);
    }

    let snapshot_str = config_snapshot.to_string();
    let inserted = insert_build_with_retry(
        &pool,
        &BuildInsertBinds {
            build_id: &build_id,
            project_id: &project_id,
            pipeline_id: &req.pipeline_id,
            actor: Some(&auth.0.email),
            event_type: None,
            trigger_ref: trigger_ref.as_deref(),
            commit_sha: commit_sha.as_deref(),
            branch: branch.as_deref(),
            changelog: changelog.as_deref(),
            config_snapshot: &snapshot_str,
            webhook_id: None,
            now,
            trigger_type: "manual",
            source_build_id: None,
            cancel_previous: concurrency.cancel_previous,
            approval_required,
        },
    )
    .await?;
    if !inserted {
        return Err(api_err(
            StatusCode::CONFLICT,
            "source_changed",
            "The project source changed while this build was being created. Review the source and trigger the build again.",
        ));
    }

    let build_number = fetch_build_number(&pool, &build_id).await?;

    // Insert initial build event
    sqlx::query(
        "INSERT INTO build_events (id, build_id, from_status, to_status, actor, reason, created_at) \
         VALUES (?1, ?2, NULL, 'queued', ?3, ?4, ?5)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&build_id)
    .bind(&auth.0.email)
    .bind(initial_event_reason("build created", approval_required))
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to insert initial build event");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to record build event")
    })?;

    let details = serde_json::json!({
        "project_id": project_id,
        "pipeline_id": req.pipeline_id,
        "build_number": build_number,
        "trigger_type": "manual",
        "created_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "build_created",
        "build",
        Some(&build_id),
        Some(&details),
    )
    .await;

    if approval_required {
        announce_pending_approval(
            &pool,
            &state.encryption_key,
            &build_id,
            Some(&auth.0.user_id),
        )
        .await;
    }

    info!(
        build_id = %build_id,
        build_number = build_number,
        project_id = %project_id,
        "build created"
    );

    let runner_policy_block_reason =
        runner_policy_block_reason_for_project(&pool, &project_id).await?;
    let build = Build {
        id: build_id,
        project_id,
        pipeline_id: req.pipeline_id,
        build_number,
        status: "queued".to_string(),
        runner_policy_block_reason,
        trigger_type: "manual".to_string(),
        trigger_actor: Some(auth.0.email),
        trigger_event: None,
        trigger_ref,
        commit_sha,
        branch,
        changelog,
        source_build_id: None,
        retry_steps: None,
        approval: pending_approval(approval_required, now),
        config_snapshot,
        runner_id: None,
        context: None,
        step_results: None,
        exit_code: None,
        queued_at: now,
        started_at: None,
        finished_at: None,
        created_at: now,
        updated_at: now,
    };

    Ok((StatusCode::CREATED, Json(CreateBuildResponse { build })))
}

/// `GET /v1/builds` — list builds with filters.
pub async fn list_builds(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListBuildsQuery>,
) -> ApiResult<ListBuildsResponse> {
    let pool = &state.db;

    let requested_limit = params.limit.unwrap_or(50);
    if requested_limit <= 0 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "limit must be greater than zero",
        ));
    }
    if params.offset.is_some_and(|offset| offset < 0) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "offset must not be negative",
        ));
    }
    let limit = requested_limit.min(200);
    let offset = params.offset.unwrap_or(0);
    let order_by = build_order_clause(params.sort.as_deref(), params.direction.as_deref())?;

    // Build dynamic query with filters
    let mut conditions = Vec::new();
    let mut bind_values: Vec<String> = Vec::new();

    if auth.0.role != "owner" && auth.0.role != "admin" {
        bind_values.push(auth.0.user_id.clone());
        conditions.push(format!(
            "EXISTS (SELECT 1 FROM project_members pm WHERE pm.project_id = builds.project_id AND pm.user_id = ?{})",
            bind_values.len()
        ));
    }
    if let Some(ref project_id) = params.project_id {
        bind_values.push(project_id.clone());
        conditions.push(format!("builds.project_id = ?{}", bind_values.len()));
    }
    if let Some(ref pipeline_id) = params.pipeline_id {
        bind_values.push(pipeline_id.clone());
        conditions.push(format!("builds.pipeline_id = ?{}", bind_values.len()));
    }
    if let Some(ref status) = params.status {
        let mut values = status
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        values.sort_unstable();
        values.dedup();
        if values.len() > 9 {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "status accepts at most 9 comma-separated values",
            ));
        }
        if values.is_empty() {
            conditions.push("1 = 0".to_string());
        }
        let mut placeholders = Vec::new();
        for value in values {
            bind_values.push(value.to_string());
            placeholders.push(format!("?{}", bind_values.len()));
        }
        if !placeholders.is_empty() {
            conditions.push(format!("builds.status IN ({})", placeholders.join(", ")));
        }
    }
    if let Some(ref branch) = params.branch {
        bind_values.push(branch.clone());
        conditions.push(format!("builds.branch = ?{}", bind_values.len()));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_query = format!("SELECT COUNT(*) FROM builds {where_clause}");
    let list_query = format!(
        "SELECT builds.*, projects.name AS project_name, integration_repositories.id AS repository_id, integration_repositories.avatar_url AS project_avatar_url, \
         integration_repositories.full_name AS repository_full_name, integrations.provider AS repository_provider, \
         integrations.host_url AS repository_host_url, pipelines.name AS pipeline_name, runners.name AS runner_name, \
         CASE \
           WHEN builds.status != 'queued' THEN NULL \
           WHEN integration_repositories.id IS NULL \
             OR json_extract(builds.config_snapshot, '$.repository_id') IS NULL \
             OR json_extract(builds.config_snapshot, '$.repository_id') != projects.repository_id \
             THEN 'repository_unavailable' \
           WHEN COALESCE(instance_preferences.direct_macos_runner_paused, 0) = 1 THEN 'instance_paused' \
           ELSE NULL \
         END AS runner_policy_block_reason \
         FROM builds \
         LEFT JOIN projects ON projects.id = builds.project_id \
         LEFT JOIN integration_repositories ON integration_repositories.id = projects.repository_id \
         LEFT JOIN integration_installations ON integration_installations.id = integration_repositories.installation_id \
         LEFT JOIN integrations ON integrations.id = integration_installations.integration_id \
         LEFT JOIN instance_preferences ON instance_preferences.id = 1 \
         LEFT JOIN pipelines ON pipelines.id = builds.pipeline_id \
         LEFT JOIN runners ON runners.id = builds.runner_id \
         {where_clause} ORDER BY {order_by} LIMIT ?{} OFFSET ?{}",
        bind_values.len() + 1,
        bind_values.len() + 2
    );

    // Execute count query
    let mut count_q = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_query));
    for val in &bind_values {
        count_q = count_q.bind(val);
    }
    let total = count_q.fetch_one(pool).await.unwrap_or(0);

    // Execute list query
    let mut list_q = sqlx::query(sqlx::AssertSqlSafe(list_query));
    for val in &bind_values {
        list_q = list_q.bind(val);
    }
    list_q = list_q.bind(limit).bind(offset);

    let rows = list_q.fetch_all(pool).await.map_err(|e| {
        error!(error = %e, "failed to list builds");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to list builds",
        )
    })?;

    let builds = rows.iter().map(row_to_redacted_build).collect();

    Ok(Json(ListBuildsResponse { builds, total }))
}

/// `GET /v1/builds/{build_id}` — build detail with events timeline.
pub async fn get_build(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> ApiResult<BuildDetailResponse> {
    let pool = &state.db;

    let build_row = sqlx::query(
        "SELECT builds.*, projects.name AS project_name, integration_repositories.id AS repository_id, integration_repositories.avatar_url AS project_avatar_url, \
         integration_repositories.full_name AS repository_full_name, integrations.provider AS repository_provider, \
         integrations.host_url AS repository_host_url, pipelines.name AS pipeline_name, runners.name AS runner_name, \
         CASE \
           WHEN builds.status != 'queued' THEN NULL \
           WHEN integration_repositories.id IS NULL \
             OR json_extract(builds.config_snapshot, '$.repository_id') IS NULL \
             OR json_extract(builds.config_snapshot, '$.repository_id') != projects.repository_id \
             THEN 'repository_unavailable' \
           WHEN COALESCE(instance_preferences.direct_macos_runner_paused, 0) = 1 THEN 'instance_paused' \
           ELSE NULL \
         END AS runner_policy_block_reason \
         FROM builds \
         LEFT JOIN projects ON projects.id = builds.project_id \
         LEFT JOIN integration_repositories ON integration_repositories.id = projects.repository_id \
         LEFT JOIN integration_installations ON integration_installations.id = integration_repositories.installation_id \
         LEFT JOIN integrations ON integrations.id = integration_installations.integration_id \
         LEFT JOIN instance_preferences ON instance_preferences.id = 1 \
         LEFT JOIN pipelines ON pipelines.id = builds.pipeline_id \
         LEFT JOIN runners ON runners.id = builds.runner_id \
         WHERE builds.id = ?1",
    )
        .bind(&build_id)
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

    let project_id: String = build_row.get("project_id");
    let effective = resolve_effective_project_role(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::Read)?;

    let build = row_to_redacted_build(&build_row);

    let events = fetch_build_events(pool, &build_id).await?;
    let retryable_steps = crate::step_retry::retryable_steps(pool, &build)
        .await
        .unwrap_or_else(|e| {
            warn!(build_id = %build_id, error = %e, "failed to check retryable steps");
            Vec::new()
        });

    Ok(Json(BuildDetailResponse {
        build,
        events,
        retryable_steps,
    }))
}

/// `POST /v1/builds/{build_id}/cancel` — cancel a build.
pub async fn cancel_build(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> ApiResult<CancelBuildResponse> {
    let pool = &state.db;

    let project_id: String = sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch build project for cancel");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch build",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;

    let effective = resolve_effective_project_role(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::CancelBuild)?;

    let build = transition_build(
        pool,
        &build_id,
        BuildStatus::Canceled,
        Some(&auth.0.email),
        Some("canceled by user"),
    )
    .await?;

    let details = serde_json::json!({
        "canceled_by": auth.0.email,
        "from_status": build.status,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "build_canceled",
        "build",
        Some(&build_id),
        Some(&details),
    )
    .await;

    // Publish event for notification dispatch
    state
        .scheduler
        .publish_event(crate::scheduler::BuildStateEvent {
            build_id: build_id.clone(),
            from_status: None,
            to_status: "canceled".to_string(),
            actor: Some(auth.0.email.clone()),
            reason: Some("canceled by user".to_string()),
            timestamp: crate::util::now_unix(),
        });

    info!(build_id = %build_id, canceled_by = %auth.0.email, "build canceled");

    Ok(Json(CancelBuildResponse { build }))
}

const MAX_APPROVAL_REASON_LENGTH: usize = 1_000;

/// `POST /v1/builds/{build_id}/approve` — let runners claim a gated build.
pub async fn approve_build(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
    body: Option<Json<BuildApprovalRequest>>,
) -> ApiResult<BuildApprovalResponse> {
    decide_build_approval(&state, &auth, &build_id, body, BuildApprovalState::Approved).await
}

/// `POST /v1/builds/{build_id}/reject` — cancel a gated build.
pub async fn reject_build(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
    body: Option<Json<BuildApprovalRequest>>,
) -> ApiResult<BuildApprovalResponse> {
    decide_build_approval(&state, &auth, &build_id, body, BuildApprovalState::Rejected).await
}

/// Record a maintainer's decision on a build waiting for approval. Approval
/// leaves the build queued for runners; rejection cancels it. Returns the new
/// build status and the timeline reason.
pub(crate) async fn record_build_approval(
    pool: &sqlx::SqlitePool,
    build_id: &str,
    actor: &str,
    decision: BuildApprovalState,
    reason: Option<&str>,
    now: i64,
) -> Result<(&'static str, String), (StatusCode, Json<ApiError>)> {
    let (to_status, event_reason) = match decision {
        BuildApprovalState::Approved => ("queued", "approved"),
        BuildApprovalState::Rejected => ("canceled", "rejected"),
        BuildApprovalState::Pending => {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "Pending is not an approval decision",
            ));
        }
    };
    let event_reason = match reason {
        Some(reason) => format!("{event_reason} by {actor}: {reason}"),
        None => format!("{event_reason} by {actor}"),
    };

    let mut tx = pool.begin().await.map_err(|e| {
        error!(error = %e, "failed to start approval transaction");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to record approval",
        )
    })?;
    let updated = sqlx::query(
        "UPDATE builds SET status = ?1, approval_state = ?2, approval_decided_by = ?3, \
                approval_decided_at = ?4, approval_reason = ?5, updated_at = ?4, \
                finished_at = CASE WHEN ?1 = 'canceled' THEN ?4 ELSE finished_at END \
         WHERE id = ?6 AND status = 'queued' AND approval_state = 'pending'",
    )
    .bind(to_status)
    .bind(decision.to_string())
    .bind(actor)
    .bind(now)
    .bind(reason)
    .bind(build_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!(error = %e, build_id, "failed to record build approval");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to record approval",
        )
    })?;
    if updated.rows_affected() == 0 {
        return Err(api_err(
            StatusCode::CONFLICT,
            "not_awaiting_approval",
            "This build is not waiting for approval",
        ));
    }
    sqlx::query(
        "INSERT INTO build_events (id, build_id, from_status, to_status, actor, reason, created_at) \
         VALUES (?1, ?2, 'queued', ?3, ?4, ?5, ?6)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(build_id)
    .bind(to_status)
    .bind(actor)
    .bind(&event_reason)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!(error = %e, build_id, "failed to record approval event");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to record approval",
        )
    })?;
    tx.commit().await.map_err(|e| {
        error!(error = %e, build_id, "failed to commit build approval");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to record approval",
        )
    })?;
    Ok((to_status, event_reason))
}

async fn decide_build_approval(
    state: &AppState,
    auth: &AuthUser,
    build_id: &str,
    body: Option<Json<BuildApprovalRequest>>,
    decision: BuildApprovalState,
) -> ApiResult<BuildApprovalResponse> {
    let pool = &state.db;
    let reason = normalize_optional(body.and_then(|Json(body)| body.reason));
    if reason
        .as_ref()
        .is_some_and(|reason| reason.len() > MAX_APPROVAL_REASON_LENGTH)
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            format!("Reason must be {MAX_APPROVAL_REASON_LENGTH} characters or fewer"),
        ));
    }

    let project_id: String = sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(build_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch build project for approval");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch build",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;
    let effective = resolve_effective_project_role(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::ApproveBuild)?;

    let now = now_unix();
    let (to_status, event_reason) = record_build_approval(
        pool,
        build_id,
        &auth.0.email,
        decision,
        reason.as_deref(),
        now,
    )
    .await?;
    let audit_action = match decision {
        BuildApprovalState::Rejected => "build_rejected",
        _ => "build_approved",
    };

    let details = serde_json::json!({
        "project_id": project_id,
        "decided_by": auth.0.email,
        "reason": reason,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        audit_action,
        "build",
        Some(build_id),
        Some(&details),
    )
    .await;

    state
        .scheduler
        .publish_event(crate::scheduler::BuildStateEvent {
            build_id: build_id.to_string(),
            from_status: Some("queued".to_string()),
            to_status: to_status.to_string(),
            actor: Some(auth.0.email.clone()),
            reason: Some(event_reason),
            timestamp: now,
        });

    info!(build_id, decided_by = %auth.0.email, decision = %decision, "build approval decided");

    let row = sqlx::query("SELECT * FROM builds WHERE id = ?1")
        .bind(build_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to reload build after approval");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to reload build",
            )
        })?;
    Ok(Json(BuildApprovalResponse {
        build: row_to_build(&row),
    }))
}

/// `POST /v1/builds/{build_id}/rerun` — re-run a build with the same parameters.
pub async fn rerun_build(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> Result<(StatusCode, Json<RerunBuildResponse>), (StatusCode, Json<ApiError>)> {
    let build = queue_rerun(&state, &auth, build_id, None).await?;
    Ok((StatusCode::CREATED, Json(RerunBuildResponse { build })))
}

/// Queue a new build from a finished one. With `retry_steps`, the new build
/// only runs those steps against the source build's stored artifacts.
pub(crate) async fn queue_rerun(
    state: &AppState,
    auth: &AuthUser,
    build_id: String,
    retry_steps: Option<Vec<String>>,
) -> Result<Build, (StatusCode, Json<ApiError>)> {
    let pool = &state.db;

    // Fetch source build
    let source_row = sqlx::query(
        "SELECT id, project_id, pipeline_id, build_number, status, \
         trigger_ref, commit_sha, branch, changelog, config_snapshot, approval_state \
         FROM builds WHERE id = ?1",
    )
    .bind(&build_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to fetch source build for rerun");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to fetch build",
        )
    })?
    .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;

    // Verify source build is terminal
    let source_status: String = source_row.get("status");
    let is_terminal = matches!(
        source_status.as_str(),
        "succeeded" | "failed" | "canceled" | "timed_out" | "expired"
    );
    if !is_terminal {
        return Err(api_err(
            StatusCode::CONFLICT,
            "build_not_terminal",
            "Cannot re-run a build that is still in progress",
        ));
    }

    let project_id: String = source_row.get("project_id");
    let pipeline_id: String = source_row.get("pipeline_id");
    let source_build_number: i64 = source_row.get("build_number");

    // Check project-level TriggerBuild permission
    let effective = resolve_effective_project_role(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::TriggerBuild)?;

    // A rerun must remain bound to the exact source an Owner/Admin currently
    // trusts for this project. Legacy snapshots can be upgraded only when
    // their immutable clone URL still matches that source.
    let current_source = sqlx::query(
        "SELECT p.repository_id, \
         CASE \
           WHEN i.provider = 'local_git' THEN r.html_url \
           ELSE i.host_url || '/' || r.full_name || '.git' \
         END AS repo_url \
         FROM projects p \
         LEFT JOIN integration_repositories r ON r.id = p.repository_id \
         LEFT JOIN integration_installations inst ON inst.id = r.installation_id \
         LEFT JOIN integrations i ON i.id = inst.integration_id \
         WHERE p.id = ?1",
    )
    .bind(&project_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, project_id = %project_id, "failed to resolve project source for rerun");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to resolve project source",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Project no longer exists",
        )
    })?;

    let current_repository_id: Option<String> = current_source.get("repository_id");
    let current_repo_url: Option<String> = current_source.get("repo_url");
    let Some(current_repository_id) = current_repository_id else {
        return Err(api_err(
            StatusCode::CONFLICT,
            "source_changed",
            "This build's source is no longer linked to the project. Choose the intended source, then trigger a new build.",
        ));
    };
    let current_repo_url = current_repo_url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            api_err(
                StatusCode::CONFLICT,
                "source_changed",
                "This build's source can no longer be resolved. Repair the project source, then trigger a new build.",
            )
        })?;

    // Clone parameters from source build.
    let raw_config_snapshot: String = source_row.get("config_snapshot");
    let mut config_snapshot: serde_json::Value = serde_json::from_str(&raw_config_snapshot)
        .map_err(|_| {
            api_err(
                StatusCode::CONFLICT,
                "source_changed",
                "This build predates reliable source tracking. Trigger a new build from the project's current source.",
            )
        })?;
    let snapshot_repository_id = config_snapshot
        .get("repository_id")
        .and_then(serde_json::Value::as_str);
    match snapshot_repository_id {
        Some(repository_id) if repository_id == current_repository_id => {}
        Some(_) => {
            return Err(api_err(
                StatusCode::CONFLICT,
                "source_changed",
                "The project source changed after this build was created. Trigger a new build from the current source.",
            ));
        }
        None => {
            let legacy_repo_url = config_snapshot
                .get("repo_url")
                .and_then(serde_json::Value::as_str)
                .map(str::trim);
            if legacy_repo_url != Some(current_repo_url.as_str()) {
                return Err(api_err(
                    StatusCode::CONFLICT,
                    "source_changed",
                    "The project source changed after this build was created. Trigger a new build from the current source.",
                ));
            }
            let Some(snapshot) = config_snapshot.as_object_mut() else {
                return Err(api_err(
                    StatusCode::CONFLICT,
                    "source_changed",
                    "This build predates reliable source tracking. Trigger a new build from the project's current source.",
                ));
            };
            snapshot.insert(
                "repository_id".to_string(),
                serde_json::Value::String(current_repository_id),
            );
        }
    }
    // A rerun follows the pipeline's current timeout and approval policy. A
    // build that was gated stays gated, whatever event started it.
    let pipeline_row =
        sqlx::query("SELECT timeout_minutes, approval_policy FROM pipelines WHERE id = ?1")
            .bind(&pipeline_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to fetch pipeline for rerun");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to fetch pipeline",
                )
            })?
            .ok_or_else(|| {
                api_err(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    "Pipeline no longer exists",
                )
            })?;
    let (timeout_minutes, approval_policy) = pipeline_gates(&pipeline_row);
    let approval_required = approval_policy.applies_to("manual")
        || (approval_policy.required
            && source_row
                .get::<Option<String>, _>("approval_state")
                .is_some());
    if let Some(snapshot) = config_snapshot.as_object_mut() {
        snapshot.insert(
            "timeout_secs".to_string(),
            serde_json::json!(i64::from(timeout_minutes) * 60),
        );
        // A full re-run of a retry builds everything again.
        snapshot.remove("retry_steps");
        if let Some(steps) = &retry_steps {
            snapshot.insert("retry_steps".to_string(), serde_json::json!(steps));
        }
    }
    let event_type = if retry_steps.is_some() {
        "retry"
    } else {
        "rerun"
    };

    let config_snapshot_str = config_snapshot.to_string();
    let branch: Option<String> = source_row.get("branch");
    let commit_sha: Option<String> = source_row.get("commit_sha");
    let trigger_ref: Option<String> = source_row.get("trigger_ref");
    let changelog: Option<String> = source_row.get("changelog");

    let now = now_unix();
    let new_build_id = Uuid::new_v4().to_string();

    // Insert new build (skip concurrency policy — explicit user action)
    let inserted = insert_build_with_retry(
        pool,
        &BuildInsertBinds {
            build_id: &new_build_id,
            project_id: &project_id,
            pipeline_id: &pipeline_id,
            actor: Some(&auth.0.email),
            event_type: Some(event_type),
            trigger_ref: trigger_ref.as_deref(),
            commit_sha: commit_sha.as_deref(),
            branch: branch.as_deref(),
            changelog: changelog.as_deref(),
            config_snapshot: &config_snapshot_str,
            webhook_id: None,
            now,
            trigger_type: "manual",
            source_build_id: Some(&build_id),
            cancel_previous: false,
            approval_required,
        },
    )
    .await?;
    if !inserted {
        return Err(api_err(
            StatusCode::CONFLICT,
            "source_changed",
            "The project source changed while this rerun was being created. Trigger a new build from the current source.",
        ));
    }

    let build_number = fetch_build_number(pool, &new_build_id).await?;

    // Insert initial build event
    let reason = initial_event_reason(
        &match &retry_steps {
            Some(steps) => format!(
                "retry of {} from build #{}",
                steps.join(", "),
                source_build_number
            ),
            None => format!("re-run of build #{}", source_build_number),
        },
        approval_required,
    );
    sqlx::query(
        "INSERT INTO build_events (id, build_id, from_status, to_status, actor, reason, created_at) \
         VALUES (?1, ?2, NULL, 'queued', ?3, ?4, ?5)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&new_build_id)
    .bind(&auth.0.email)
    .bind(&reason)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to insert build event for rerun");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to record build event")
    })?;

    // Audit log
    let details = serde_json::json!({
        "project_id": project_id,
        "pipeline_id": pipeline_id,
        "build_number": build_number,
        "trigger_type": "manual",
        "source_build_id": build_id,
        "retry_steps": retry_steps,
        "created_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        if retry_steps.is_some() {
            "build_step_retry"
        } else {
            "build_rerun"
        },
        "build",
        Some(&new_build_id),
        Some(&details),
    )
    .await;

    if approval_required {
        announce_pending_approval(
            pool,
            &state.encryption_key,
            &new_build_id,
            Some(&auth.0.user_id),
        )
        .await;
    }

    info!(
        build_id = %new_build_id,
        build_number = build_number,
        source_build_id = %build_id,
        project_id = %project_id,
        "build re-run created"
    );

    let runner_policy_block_reason =
        runner_policy_block_reason_for_project(pool, &project_id).await?;
    let build = Build {
        id: new_build_id,
        project_id,
        pipeline_id,
        build_number,
        status: "queued".to_string(),
        runner_policy_block_reason,
        trigger_type: "manual".to_string(),
        trigger_actor: Some(auth.0.email.clone()),
        trigger_event: Some(event_type.to_string()),
        trigger_ref,
        commit_sha,
        branch,
        changelog,
        source_build_id: Some(build_id),
        retry_steps,
        approval: pending_approval(approval_required, now),
        config_snapshot,
        runner_id: None,
        context: None,
        step_results: None,
        exit_code: None,
        queued_at: now,
        started_at: None,
        finished_at: None,
        created_at: now,
        updated_at: now,
    };

    Ok(build)
}

/// What changed in a webhook revision, for path filters.
pub enum ChangeSet<'a> {
    /// The provider did not say; path filters let the build through.
    Unknown,
    /// Files listed in the push payload.
    Files(&'a [String]),
    /// Ask the provider for the files changed from `base` to `head`.
    Compare { base: &'a str, head: &'a str },
}

/// A normalized webhook revision that may start builds.
pub struct WebhookBuildTrigger<'a> {
    pub webhook_id: &'a str,
    pub integration_id: &'a str,
    pub repo_full_name: &'a str,
    /// Provider event name, e.g. `push` or `Merge Request Hook`.
    pub event_type: &'a str,
    pub branch: Option<&'a str>,
    /// Set for tag pushes, which match `trigger_config.tags`.
    pub tag: Option<&'a str>,
    pub commit_sha: Option<&'a str>,
    pub actor: Option<&'a str>,
    pub changes: ChangeSet<'a>,
}

/// Trigger builds from a webhook event.
///
/// Called from webhooks.rs after webhook normalization.
/// Resolves matching projects/pipelines and creates build records.
pub async fn trigger_build_from_webhook(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    trigger: &WebhookBuildTrigger<'_>,
) -> Result<Vec<Build>, (StatusCode, Json<ApiError>)> {
    let WebhookBuildTrigger {
        webhook_id,
        integration_id,
        repo_full_name,
        branch,
        tag,
        commit_sha,
        actor,
        ..
    } = *trigger;
    // Tag pushes arrive as `push` on GitHub; store and filter them as tag_push.
    let event_type = if tag.is_some() {
        "tag_push"
    } else {
        trigger.event_type
    };
    let canonical_event = TriggerConfig::normalize_event(event_type);
    let trigger_ref = tag.or(branch);

    // Find projects linked to this repository
    let project_rows = sqlx::query(
        "SELECT p.id, p.repository_id, \
                CASE WHEN source.provider = 'local_git' THEN r.html_url \
                     ELSE source.host_url || '/' || r.full_name || '.git' END AS repo_url, \
                COALESCE(pref.direct_macos_runner_paused, 0) AS direct_macos_runner_paused \
         FROM projects p \
         JOIN integration_repositories r ON r.id = p.repository_id \
         JOIN integration_installations i ON i.id = r.installation_id \
         JOIN integrations source ON source.id = i.integration_id \
         LEFT JOIN instance_preferences pref ON pref.id = 1 \
         WHERE i.integration_id = ?1 AND r.full_name = ?2",
    )
    .bind(integration_id)
    .bind(repo_full_name)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to find projects for repo");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to resolve project",
        )
    })?;

    if project_rows.is_empty() {
        info!(repo = %repo_full_name, "no projects linked to this repository");
        return Ok(Vec::new());
    }

    let mut created_builds = Vec::new();
    let now = now_unix();
    // Fetched at most once, and only when a pipeline filters on paths.
    let mut compared_files: Option<Option<Vec<String>>> = None;

    for project_row in &project_rows {
        let project_id: String = project_row.get("id");
        let repository_id: String = project_row.get("repository_id");
        let runner_policy_block_reason = (project_row.get::<i32, _>("direct_macos_runner_paused")
            != 0)
            .then_some(RunnerPolicyBlockReason::InstancePaused);
        let repo_url = project_row
            .get::<Option<String>, _>("repo_url")
            .map(|raw| raw.trim().to_string())
            .filter(|raw| !raw.is_empty());
        let Some(repo_url) = repo_url else {
            warn!(
                project_id = %project_id,
                repo_full_name = %repo_full_name,
                "skipping webhook-triggered builds because repository URL could not be resolved"
            );
            continue;
        };

        // ponytail: one pipeline query per linked project; batch when repositories commonly map to many projects.
        // Find enabled pipelines for this project
        let pipeline_rows = sqlx::query(
            "SELECT id, config_path, config_path_explicit, execution_config, trigger_config, concurrency, \
                    timeout_minutes, approval_policy \
             FROM pipelines WHERE project_id = ?1 AND enabled = 1",
        )
        .bind(&project_id)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch pipelines");
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to fetch pipelines")
        })?;

        for pipeline_row in &pipeline_rows {
            let pipeline_id: String = pipeline_row.get("id");
            let config_path: String = pipeline_row.get("config_path");
            let config_path_explicit: i32 =
                pipeline_row.try_get("config_path_explicit").unwrap_or(0);
            let execution_config_json: String = pipeline_row
                .try_get("execution_config")
                .unwrap_or_else(|_| "{}".to_string());
            let execution_config = parse_execution_config(&execution_config_json);
            let trigger_config_json: String = pipeline_row.get("trigger_config");
            let concurrency_json: String = pipeline_row.get("concurrency");
            let concurrency: ConcurrencyPolicy =
                serde_json::from_str(&concurrency_json).unwrap_or_default();
            let (timeout_minutes, approval_policy) = pipeline_gates(pipeline_row);

            // Parse and apply trigger config filter
            let trigger_config: TriggerConfig =
                serde_json::from_str(&trigger_config_json).unwrap_or_else(|e| {
                    warn!(error = %e, pipeline_id = %pipeline_id, "invalid trigger_config JSON, using default");
                    TriggerConfig::default()
                });

            if !trigger_config.should_trigger(event_type, branch, tag) {
                info!(
                    pipeline_id = %pipeline_id,
                    event_type = %event_type,
                    branch = ?branch,
                    tag = ?tag,
                    "webhook event filtered by trigger_config, skipping pipeline"
                );
                continue;
            }
            if commit_sha.is_none() && branch.is_none() {
                warn!(
                    pipeline_id = %pipeline_id,
                    event_type = %event_type,
                    "webhook payload missing branch and commit_sha; skipping build creation"
                );
                continue;
            }
            if tag.is_none() && trigger_config.has_path_filters() {
                let changed_files = match &trigger.changes {
                    ChangeSet::Unknown => None,
                    ChangeSet::Files(files) => Some(*files),
                    ChangeSet::Compare { base, head } => {
                        if compared_files.is_none() {
                            compared_files = Some(
                                crate::integrations::changed_files(
                                    pool,
                                    encryption_key,
                                    &repository_id,
                                    base,
                                    head,
                                )
                                .await,
                            );
                        }
                        compared_files.as_ref().and_then(Option::as_deref)
                    }
                };
                if !trigger_config.paths_match(changed_files) {
                    info!(
                        pipeline_id = %pipeline_id,
                        event_type = %event_type,
                        "no changed file matches the pipeline path filters, skipping pipeline"
                    );
                    continue;
                }
            }

            let build_id = Uuid::new_v4().to_string();
            let approval_required = approval_policy.applies_to(&canonical_event);

            let config_snapshot = create_config_snapshot(
                &config_path,
                config_path_explicit != 0,
                &execution_config,
                timeout_minutes,
                "webhook",
                commit_sha,
                branch,
                (&repository_id, &repo_url),
            );

            let snapshot_str = config_snapshot.to_string();
            let inserted = insert_build_with_retry(
                pool,
                &BuildInsertBinds {
                    build_id: &build_id,
                    project_id: &project_id,
                    pipeline_id: &pipeline_id,
                    actor,
                    event_type: Some(event_type),
                    trigger_ref,
                    commit_sha,
                    branch,
                    changelog: None,
                    config_snapshot: &snapshot_str,
                    webhook_id: Some(webhook_id),
                    now,
                    trigger_type: "webhook",
                    source_build_id: None,
                    cancel_previous: concurrency.cancel_previous,
                    approval_required,
                },
            )
            .await?;
            if !inserted {
                warn!(
                    project_id = %project_id,
                    pipeline_id = %pipeline_id,
                    "skipping webhook build because it already exists or the project source changed"
                );
                continue;
            }

            let build_number = fetch_build_number(pool, &build_id).await?;

            // Insert initial build event
            sqlx::query(
                "INSERT INTO build_events (id, build_id, from_status, to_status, actor, reason, created_at) \
                 VALUES (?1, ?2, NULL, 'queued', ?3, ?4, ?5)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&build_id)
            .bind(actor)
            .bind(initial_event_reason(&format!("webhook: {event_type}"), approval_required))
            .bind(now)
            .execute(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to insert build event");
                api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to record build event")
            })?;

            if approval_required {
                announce_pending_approval(pool, encryption_key, &build_id, None).await;
            }

            info!(
                build_id = %build_id,
                build_number = build_number,
                project_id = %project_id,
                pipeline_id = %pipeline_id,
                approval_required,
                "webhook-triggered build created"
            );

            created_builds.push(Build {
                id: build_id,
                project_id: project_id.clone(),
                pipeline_id,
                build_number,
                status: "queued".to_string(),
                runner_policy_block_reason,
                trigger_type: "webhook".to_string(),
                trigger_actor: actor.map(String::from),
                trigger_event: Some(event_type.to_string()),
                trigger_ref: trigger_ref.map(String::from),
                commit_sha: commit_sha.map(String::from),
                branch: branch.map(String::from),
                changelog: None,
                source_build_id: None,
                retry_steps: None,
                approval: pending_approval(approval_required, now),
                config_snapshot,
                runner_id: None,
                context: None,
                step_results: None,
                exit_code: None,
                queued_at: now,
                started_at: None,
                finished_at: None,
                created_at: now,
                updated_at: now,
            });
        }
    }

    Ok(created_builds)
}

/// Start a build for a due cron schedule. Returns the new build's ID.
pub async fn create_scheduled_build(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    pipeline_id: &str,
    schedule: &ScheduleTrigger,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let row = sqlx::query(
        "SELECT pl.project_id, pl.config_path, pl.config_path_explicit, pl.execution_config, \
                pl.concurrency, pl.timeout_minutes, pl.approval_policy, \
                p.repository_id, p.default_branch \
         FROM pipelines pl JOIN projects p ON p.id = pl.project_id \
         WHERE pl.id = ?1 AND pl.enabled = 1",
    )
    .bind(pipeline_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to fetch scheduled pipeline");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to fetch pipeline",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Pipeline not found or disabled",
        )
    })?;

    let project_id: String = row.get("project_id");
    let repository_id: String = row
        .get::<Option<String>, _>("repository_id")
        .ok_or_else(|| {
            api_err(
                StatusCode::CONFLICT,
                "source_not_configured",
                "Project is not linked to a source repository",
            )
        })?;
    let branch = normalize_optional(schedule.branch.clone())
        .or_else(|| normalize_optional(row.get::<Option<String>, _>("default_branch")))
        .ok_or_else(|| {
            api_err(
                StatusCode::CONFLICT,
                "invalid_input",
                "The schedule has no branch and the project has no default branch",
            )
        })?;
    let repo_url = resolve_repository_clone_url(pool, &repository_id)
        .await?
        .ok_or_else(|| {
            api_err(
                StatusCode::CONFLICT,
                "source_unresolvable",
                "Project source repository could not be resolved",
            )
        })?;
    let commit_sha =
        crate::integrations::resolve_branch_commit(pool, encryption_key, &repository_id, &branch)
            .await?;

    let execution_config = parse_execution_config(
        &row.try_get::<String, _>("execution_config")
            .unwrap_or_else(|_| "{}".to_string()),
    );
    let concurrency: ConcurrencyPolicy =
        serde_json::from_str(&row.get::<String, _>("concurrency")).unwrap_or_default();
    let (timeout_minutes, approval_policy) = pipeline_gates(&row);
    let approval_required = approval_policy.applies_to("schedule");
    let config_snapshot = create_config_snapshot(
        &row.get::<String, _>("config_path"),
        row.try_get::<i32, _>("config_path_explicit").unwrap_or(0) != 0,
        &execution_config,
        timeout_minutes,
        "schedule",
        Some(&commit_sha),
        Some(&branch),
        (&repository_id, &repo_url),
    );

    let now = now_unix();
    let build_id = Uuid::new_v4().to_string();
    let actor = format!("schedule: {}", schedule.cron.trim());
    let inserted = insert_build_with_retry(
        pool,
        &BuildInsertBinds {
            build_id: &build_id,
            project_id: &project_id,
            pipeline_id,
            actor: Some(&actor),
            event_type: Some("schedule"),
            trigger_ref: Some(&branch),
            commit_sha: Some(&commit_sha),
            branch: Some(&branch),
            changelog: None,
            config_snapshot: &config_snapshot.to_string(),
            webhook_id: None,
            now,
            trigger_type: "schedule",
            source_build_id: None,
            cancel_previous: concurrency.cancel_previous,
            approval_required,
        },
    )
    .await?;
    if !inserted {
        return Err(api_err(
            StatusCode::CONFLICT,
            "source_changed",
            "The project source changed while the scheduled build was being created",
        ));
    }

    sqlx::query(
        "INSERT INTO build_events (id, build_id, from_status, to_status, actor, reason, created_at) \
         VALUES (?1, ?2, NULL, 'queued', ?3, ?4, ?5)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&build_id)
    .bind(&actor)
    .bind(initial_event_reason("scheduled", approval_required))
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to insert scheduled build event");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to record build event")
    })?;

    if approval_required {
        announce_pending_approval(pool, encryption_key, &build_id, None).await;
    }
    Ok(build_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool(
        trigger_config: &str,
        approval_policy: &str,
        timeout_minutes: i64,
    ) -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'lead@example.com', 'sub', 'owner', 0, 0)",
            "INSERT INTO integrations (id, provider, auth_mode, created_by, created_at, updated_at) \
             VALUES ('i1', 'github', 'github_app', 'u1', 0, 0)",
            "INSERT INTO integration_installations (id, integration_id, external_id, account_name, created_at, updated_at) \
             VALUES ('inst1', 'i1', '42', 'acme', 0, 0)",
            "INSERT INTO integration_repositories (id, installation_id, external_id, full_name, created_at, updated_at) \
             VALUES ('r1', 'inst1', '7', 'acme/app', 0, 0)",
            "INSERT INTO projects (id, name, repository_id, created_by, created_at, updated_at) \
             VALUES ('p1', 'App', 'r1', 'u1', 0, 0)",
            "INSERT INTO runners (id, name, token_hash, status, labels, created_at, updated_at) \
             VALUES ('run1', 'mac', 'hash', 'online', '[\"macos\"]', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO pipelines (id, project_id, name, trigger_config, approval_policy, timeout_minutes, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'release', ?1, ?2, ?3, 0, 0)",
        )
        .bind(trigger_config)
        .bind(approval_policy)
        .bind(timeout_minutes)
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn deliver(
        pool: &sqlx::SqlitePool,
        delivery: &str,
        branch: Option<&str>,
        tag: Option<&str>,
        changes: ChangeSet<'_>,
    ) -> Vec<Build> {
        let webhook_id = format!("wh-{delivery}");
        sqlx::query(
            "INSERT INTO integration_webhooks (id, integration_id, provider_delivery_id, event_type, payload, received_at) \
             VALUES (?1, 'i1', ?2, 'push', '{}', 0)",
        )
        .bind(&webhook_id)
        .bind(delivery)
        .execute(pool)
        .await
        .unwrap();
        trigger_build_from_webhook(
            pool,
            &[0; 32],
            &WebhookBuildTrigger {
                webhook_id: &webhook_id,
                integration_id: "i1",
                repo_full_name: "acme/app",
                event_type: "push",
                branch,
                tag,
                commit_sha: Some("abc123"),
                actor: Some("octocat"),
                changes,
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn tag_build_waits_for_approval_before_a_runner_can_claim_it() {
        let pool = pool(
            r#"{"events":["push","tag_push"],"branches":["main"],"tags":["v*"]}"#,
            r#"{"required":true,"events":["tag_push"]}"#,
            60,
        )
        .await;

        // A branch push is not gated.
        let branch_builds = deliver(&pool, "d1", Some("main"), None, ChangeSet::Unknown).await;
        assert_eq!(branch_builds.len(), 1);
        assert!(branch_builds[0].approval.is_none());

        // An unmatched tag does not build; a matching one waits.
        assert!(
            deliver(&pool, "d2", None, Some("nightly"), ChangeSet::Unknown)
                .await
                .is_empty()
        );
        let builds = deliver(&pool, "d3", None, Some("v1.4.0"), ChangeSet::Unknown).await;
        assert_eq!(builds.len(), 1);
        let build = &builds[0];
        assert_eq!(build.trigger_event.as_deref(), Some("tag_push"));
        assert_eq!(build.trigger_ref.as_deref(), Some("v1.4.0"));
        assert_eq!(
            build.approval.as_ref().map(|approval| approval.state),
            Some(BuildApprovalState::Pending)
        );

        assert!(
            !crate::runners::claim_eligible_direct_runner_build(
                &pool,
                &build.id,
                "run1",
                "runner:run1",
                "hash"
            )
            .await
            .unwrap(),
            "a runner must not claim a build waiting for approval"
        );

        let (status, reason) = record_build_approval(
            &pool,
            &build.id,
            "lead@example.com",
            BuildApprovalState::Approved,
            Some("ship it"),
            100,
        )
        .await
        .unwrap();
        assert_eq!(status, "queued");
        assert_eq!(reason, "approved by lead@example.com: ship it");
        assert!(
            crate::runners::claim_eligible_direct_runner_build(
                &pool,
                &build.id,
                "run1",
                "runner:run1",
                "hash"
            )
            .await
            .unwrap()
        );

        // A decision is final.
        let (status, _) = record_build_approval(
            &pool,
            &build.id,
            "lead@example.com",
            BuildApprovalState::Rejected,
            None,
            101,
        )
        .await
        .unwrap_err();
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn rejecting_cancels_the_build() {
        let pool = pool("{}", r#"{"required":true,"events":[]}"#, 60).await;
        let builds = deliver(&pool, "d1", Some("main"), None, ChangeSet::Unknown).await;
        let (status, _) = record_build_approval(
            &pool,
            &builds[0].id,
            "lead@example.com",
            BuildApprovalState::Rejected,
            Some("wrong branch"),
            100,
        )
        .await
        .unwrap();
        assert_eq!(status, "canceled");
        let row = sqlx::query("SELECT * FROM builds WHERE id = ?1")
            .bind(&builds[0].id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let build = row_to_build(&row);
        assert_eq!(build.status, "canceled");
        assert_eq!(build.finished_at, Some(100));
        let approval = build.approval.unwrap();
        assert_eq!(approval.state, BuildApprovalState::Rejected);
        assert_eq!(approval.reason.as_deref(), Some("wrong branch"));
    }

    #[tokio::test]
    async fn path_filters_skip_pushes_that_change_nothing_relevant() {
        let pool = pool(
            r#"{"paths":["lib/**","pubspec.yaml"],"paths_ignore":["**/*.md"]}"#,
            "{}",
            60,
        )
        .await;
        let docs = ["README.md".to_string(), "lib/README.md".to_string()];
        assert!(
            deliver(&pool, "d1", Some("main"), None, ChangeSet::Files(&docs))
                .await
                .is_empty()
        );
        let code = ["lib/main.dart".to_string()];
        assert_eq!(
            deliver(&pool, "d2", Some("main"), None, ChangeSet::Files(&code))
                .await
                .len(),
            1
        );
        // Unknown change sets never suppress a build.
        assert_eq!(
            deliver(&pool, "d3", Some("main"), None, ChangeSet::Unknown)
                .await
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn a_20_minute_pipeline_times_out_at_20_minutes() {
        let pool = pool("{}", "{}", 20).await;
        let builds = deliver(&pool, "d1", Some("main"), None, ChangeSet::Unknown).await;
        let build_id = &builds[0].id;
        assert_eq!(builds[0].config_snapshot["timeout_secs"], 1200);
        sqlx::query("UPDATE builds SET status = 'running', started_at = 1000 WHERE id = ?1")
            .bind(build_id)
            .execute(&pool)
            .await
            .unwrap();
        let scheduler = crate::scheduler::Scheduler::new(16);

        let early = crate::background::time_out_overdue_builds(&pool, &scheduler, 1000 + 1199)
            .await
            .unwrap();
        assert!(early.is_empty());
        let late = crate::background::time_out_overdue_builds(&pool, &scheduler, 1000 + 1200)
            .await
            .unwrap();
        assert_eq!(late, vec![build_id.clone()]);
        let status: String = sqlx::query_scalar("SELECT status FROM builds WHERE id = ?1")
            .bind(build_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "timed_out");
    }
}
