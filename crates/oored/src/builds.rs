use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, Build, BuildDetailResponse, BuildEvent, BuildStatus, CancelBuildResponse,
    ConcurrencyPolicy, CreateBuildRequest, CreateBuildResponse, ListBuildsResponse,
    PipelineExecutionConfig, TriggerConfig,
};
use serde::Deserialize;
use sqlx::Row;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

// ── Row conversion helpers ──────────────────────────────────────

fn row_to_build(row: &sqlx::sqlite::SqliteRow) -> Build {
    let config_snapshot_str: String = row.get("config_snapshot");
    let config_snapshot: serde_json::Value =
        serde_json::from_str(&config_snapshot_str).unwrap_or(serde_json::json!({}));

    let step_results_str: Option<String> = row.get("step_results");
    let step_results = step_results_str.and_then(|s| serde_json::from_str(&s).ok());

    Build {
        id: row.get("id"),
        project_id: row.get("project_id"),
        pipeline_id: row.get("pipeline_id"),
        build_number: row.get("build_number"),
        status: row.get("status"),
        trigger_type: row.get("trigger_type"),
        trigger_actor: row.get("trigger_actor"),
        trigger_event: row.get("trigger_event"),
        trigger_ref: row.get("trigger_ref"),
        commit_sha: row.get("commit_sha"),
        branch: row.get("branch"),
        config_snapshot,
        runner_id: row.get("runner_id"),
        step_results,
        exit_code: row.get("exit_code"),
        queued_at: row.get("queued_at"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
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
    // Load current build
    let row = sqlx::query("SELECT * FROM builds WHERE id = ?1")
        .bind(build_id)
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
    let finished_at_update = if target_status.is_terminal() {
        "finished_at = ?4,"
    } else {
        ""
    };

    // Optimistic locking: WHERE status = current_status
    let query = format!(
        "UPDATE builds SET status = ?1, {started_at_update} {finished_at_update} updated_at = ?4 \
         WHERE id = ?2 AND status = ?3"
    );

    let result = sqlx::query(&query)
        .bind(&target_str)
        .bind(build_id)
        .bind(&current_status_str)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to update build status");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to update build",
            )
        })?;

    if result.rows_affected() == 0 {
        return Err(api_err(
            StatusCode::CONFLICT,
            "concurrent_modification",
            "Build status was modified concurrently",
        ));
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
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to insert build event");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to record build event")
    })?;

    // Reload build
    let updated_row = sqlx::query("SELECT * FROM builds WHERE id = ?1")
        .bind(build_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to reload build");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to reload build",
            )
        })?;

    Ok(row_to_build(&updated_row))
}

/// Create a config snapshot capturing pipeline config at build creation time.
fn create_config_snapshot(
    pipeline_config_path: &str,
    config_path_explicit: bool,
    ui_execution_config: &PipelineExecutionConfig,
    trigger_type: &str,
    commit_sha: Option<&str>,
    branch: Option<&str>,
    repo_url: Option<&str>,
) -> serde_json::Value {
    let ui_execution_config_json =
        serde_json::to_value(ui_execution_config).unwrap_or_else(|_| serde_json::json!({}));
    serde_json::json!({
        "snapshot_version": 2,
        "config_resolution_policy": "file_first_ui_fallback",
        "config_path": pipeline_config_path,
        "config_path_explicit": config_path_explicit,
        "ui_execution_config": ui_execution_config_json,
        "artifact_patterns": ui_execution_config.artifact_patterns,
        "trigger_type": trigger_type,
        "commit_sha": commit_sha,
        "branch": branch,
        "repo_url": repo_url,
        "captured_at": now_unix(),
    })
}

fn parse_execution_config(raw: &str) -> PipelineExecutionConfig {
    serde_json::from_str(raw).unwrap_or_else(|_| PipelineExecutionConfig::default())
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
) -> Result<(), (StatusCode, Json<ApiError>)> {
    for attempt in 1..=BUILD_INSERT_MAX_RETRIES {
        match execute_build_insert(pool, "", binds).await {
            Ok(()) => return Ok(()),
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
    if let sqlx::Error::Database(db_err) = e {
        if let Some(code) = db_err.code() {
            return code.as_ref() == "5";
        }
    }
    false
}

/// Check if an sqlx error is SQLITE_CONSTRAINT_UNIQUE (error code 2067).
fn is_sqlite_unique_violation(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = e {
        if let Some(code) = db_err.code() {
            return code.as_ref() == "2067";
        }
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
    config_snapshot: &'a str,
    webhook_id: Option<&'a str>,
    now: i64,
    trigger_type: &'a str,
}

async fn execute_build_insert(
    pool: &sqlx::SqlitePool,
    _query: &str,
    b: &BuildInsertBinds<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, \
         trigger_type, trigger_actor, trigger_event, trigger_ref, commit_sha, branch, \
         config_snapshot, webhook_id, queued_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, \
                 (SELECT COALESCE(MAX(build_number), 0) + 1 FROM builds WHERE project_id = ?2), \
                 'queued', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?12)",
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
    .bind(b.config_snapshot)
    .bind(b.webhook_id)
    .bind(b.now)
    .execute(pool)
    .await?;
    Ok(())
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
}

// ── Concurrency policy ──────────────────────────────────────────

/// Apply the cancel_previous concurrency policy: cancel all non-terminal builds
/// on the same pipeline + branch.
async fn apply_cancel_previous(
    pool: &sqlx::SqlitePool,
    pipeline_id: &str,
    branch: Option<&str>,
    actor: Option<&str>,
) -> Result<u32, (StatusCode, Json<ApiError>)> {
    let branch = match branch {
        Some(b) if !b.is_empty() => b,
        _ => return Ok(0),
    };

    let rows = sqlx::query(
        "SELECT id, status FROM builds \
         WHERE pipeline_id = ?1 AND branch = ?2 \
         AND status NOT IN ('succeeded', 'failed', 'canceled', 'timed_out', 'expired')",
    )
    .bind(pipeline_id)
    .bind(branch)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to query non-terminal builds");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to query builds",
        )
    })?;

    let mut canceled = 0u32;
    for row in &rows {
        let build_id: String = row.get("id");
        match transition_build(
            pool,
            &build_id,
            BuildStatus::Canceled,
            actor,
            Some("superseded by new build"),
        )
        .await
        {
            Ok(_) => {
                canceled += 1;
                info!(build_id = %build_id, "auto-canceled superseded build");
            }
            Err(e) => {
                warn!(build_id = %build_id, error = ?e, "failed to auto-cancel build");
            }
        }
    }

    Ok(canceled)
}

// ── Handlers ────────────────────────────────────────────────────

/// `POST /v1/projects/{project_id}/builds` — create a new build (manual/API trigger).
pub async fn create_build(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(req): Json<CreateBuildRequest>,
) -> ApiResult<CreateBuildResponse> {
    check_permission(&state.enforcer, &auth.0.role, "builds", "write").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    // Verify project exists
    let project_exists: bool =
        sqlx::query_scalar("SELECT COUNT(*) > 0 FROM projects WHERE id = ?1")
            .bind(&project_id)
            .fetch_one(pool)
            .await
            .unwrap_or(false);

    if !project_exists {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Project not found",
        ));
    }

    // Verify pipeline exists and belongs to this project
    let pipeline_row = sqlx::query(
        "SELECT id, config_path, config_path_explicit, execution_config, concurrency FROM pipelines WHERE id = ?1 AND project_id = ?2",
    )
    .bind(&req.pipeline_id)
    .bind(&project_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to fetch pipeline");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to fetch pipeline")
    })?
    .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Pipeline not found in this project"))?;

    let config_path: String = pipeline_row.get("config_path");
    let config_path_explicit: i32 = pipeline_row.try_get("config_path_explicit").unwrap_or(0);
    let execution_config_json: String = pipeline_row
        .try_get("execution_config")
        .unwrap_or_else(|_| "{}".to_string());
    let execution_config = parse_execution_config(&execution_config_json);
    let concurrency_json: String = pipeline_row.get("concurrency");
    let concurrency: ConcurrencyPolicy =
        serde_json::from_str(&concurrency_json).unwrap_or_default();

    // Apply cancel_previous policy
    if concurrency.cancel_previous {
        let canceled = apply_cancel_previous(
            pool,
            &req.pipeline_id,
            req.branch.as_deref(),
            Some(&auth.0.email),
        )
        .await?;
        if canceled > 0 {
            info!(
                canceled = canceled,
                "auto-canceled previous builds via concurrency policy"
            );
        }
    }

    let now = now_unix();
    let build_id = Uuid::new_v4().to_string();

    // Resolve repo clone URL from project's linked repository
    let repo_url: Option<String> = sqlx::query_scalar(
        "SELECT i.host_url || '/' || r.full_name || '.git' \
         FROM projects p \
         JOIN integration_repositories r ON r.id = p.repository_id \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         JOIN integrations i ON i.id = inst.integration_id \
         WHERE p.id = ?1",
    )
    .bind(&project_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let config_snapshot = create_config_snapshot(
        &config_path,
        config_path_explicit != 0,
        &execution_config,
        "manual",
        req.commit_sha.as_deref(),
        req.branch.as_deref(),
        repo_url.as_deref(),
    );

    let snapshot_str = config_snapshot.to_string();
    insert_build_with_retry(
        pool,
        &BuildInsertBinds {
            build_id: &build_id,
            project_id: &project_id,
            pipeline_id: &req.pipeline_id,
            actor: Some(&auth.0.email),
            event_type: None,
            trigger_ref: req.trigger_ref.as_deref(),
            commit_sha: req.commit_sha.as_deref(),
            branch: req.branch.as_deref(),
            config_snapshot: &snapshot_str,
            webhook_id: None,
            now,
            trigger_type: "manual",
        },
    )
    .await?;

    let build_number = fetch_build_number(pool, &build_id).await?;

    // Insert initial build event
    sqlx::query(
        "INSERT INTO build_events (id, build_id, from_status, to_status, actor, reason, created_at) \
         VALUES (?1, ?2, NULL, 'queued', ?3, 'build created', ?4)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&build_id)
    .bind(&auth.0.email)
    .bind(now)
    .execute(pool)
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
        pool,
        Some(&auth.0.user_id),
        "build_created",
        "build",
        Some(&build_id),
        Some(&details),
    )
    .await;

    info!(
        build_id = %build_id,
        build_number = build_number,
        project_id = %project_id,
        "build created"
    );

    let build = Build {
        id: build_id,
        project_id,
        pipeline_id: req.pipeline_id,
        build_number,
        status: "queued".to_string(),
        trigger_type: "manual".to_string(),
        trigger_actor: Some(auth.0.email),
        trigger_event: None,
        trigger_ref: req.trigger_ref,
        commit_sha: req.commit_sha,
        branch: req.branch,
        config_snapshot,
        runner_id: None,
        step_results: None,
        exit_code: None,
        queued_at: now,
        started_at: None,
        finished_at: None,
        created_at: now,
        updated_at: now,
    };

    Ok(Json(CreateBuildResponse { build }))
}

/// `GET /v1/builds` — list builds with filters.
pub async fn list_builds(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListBuildsQuery>,
) -> ApiResult<ListBuildsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "builds", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    // Build dynamic query with filters
    let mut conditions = Vec::new();
    let mut bind_values: Vec<String> = Vec::new();

    if let Some(ref project_id) = params.project_id {
        bind_values.push(project_id.clone());
        conditions.push(format!("project_id = ?{}", bind_values.len()));
    }
    if let Some(ref pipeline_id) = params.pipeline_id {
        bind_values.push(pipeline_id.clone());
        conditions.push(format!("pipeline_id = ?{}", bind_values.len()));
    }
    if let Some(ref status) = params.status {
        bind_values.push(status.clone());
        conditions.push(format!("status = ?{}", bind_values.len()));
    }
    if let Some(ref branch) = params.branch {
        bind_values.push(branch.clone());
        conditions.push(format!("branch = ?{}", bind_values.len()));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_query = format!("SELECT COUNT(*) FROM builds {where_clause}");
    let list_query = format!(
        "SELECT * FROM builds {where_clause} ORDER BY created_at DESC LIMIT ?{} OFFSET ?{}",
        bind_values.len() + 1,
        bind_values.len() + 2
    );

    // Execute count query
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
    for val in &bind_values {
        count_q = count_q.bind(val);
    }
    let total = count_q.fetch_one(pool).await.unwrap_or(0);

    // Execute list query
    let mut list_q = sqlx::query(&list_query);
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

    let builds = rows.iter().map(row_to_build).collect();

    Ok(Json(ListBuildsResponse { builds, total }))
}

/// `GET /v1/builds/{build_id}` — build detail with events timeline.
pub async fn get_build(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> ApiResult<BuildDetailResponse> {
    check_permission(&state.enforcer, &auth.0.role, "builds", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let build_row = sqlx::query("SELECT * FROM builds WHERE id = ?1")
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

    let build = row_to_build(&build_row);

    let event_rows =
        sqlx::query("SELECT * FROM build_events WHERE build_id = ?1 ORDER BY created_at ASC")
            .bind(&build_id)
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

    let events = event_rows.iter().map(row_to_build_event).collect();

    Ok(Json(BuildDetailResponse { build, events }))
}

/// `POST /v1/builds/{build_id}/cancel` — cancel a build.
pub async fn cancel_build(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> ApiResult<CancelBuildResponse> {
    check_permission(&state.enforcer, &auth.0.role, "builds", "cancel").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

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

    info!(build_id = %build_id, canceled_by = %auth.0.email, "build canceled");

    Ok(Json(CancelBuildResponse { build }))
}

/// Trigger builds from a webhook event.
///
/// Called from webhooks.rs after webhook normalization.
/// Resolves matching projects/pipelines and creates build records.
pub async fn trigger_build_from_webhook(
    pool: &sqlx::SqlitePool,
    webhook_id: &str,
    integration_id: &str,
    repo_full_name: &str,
    branch: Option<&str>,
    commit_sha: Option<&str>,
    event_type: &str,
    actor: Option<&str>,
) -> Result<Vec<Build>, (StatusCode, Json<ApiError>)> {
    // Find projects linked to this repository
    let project_rows = sqlx::query(
        "SELECT p.id, p.name FROM projects p \
         JOIN integration_repositories r ON r.id = p.repository_id \
         JOIN integration_installations i ON i.id = r.installation_id \
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

    // Resolve repo clone URL from integration host_url
    let repo_url: Option<String> =
        sqlx::query_scalar("SELECT host_url FROM integrations WHERE id = ?1")
            .bind(integration_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
            .map(|host_url: String| format!("{}/{}.git", host_url, repo_full_name));

    for project_row in &project_rows {
        let project_id: String = project_row.get("id");

        // Find enabled pipelines for this project
        let pipeline_rows = sqlx::query(
            "SELECT id, config_path, config_path_explicit, execution_config, trigger_config, concurrency FROM pipelines \
             WHERE project_id = ?1 AND enabled = 1",
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

            // Parse and apply trigger config filter
            let trigger_config: TriggerConfig =
                serde_json::from_str(&trigger_config_json).unwrap_or_else(|e| {
                    warn!(error = %e, pipeline_id = %pipeline_id, "invalid trigger_config JSON, using default");
                    TriggerConfig::default()
                });

            if !trigger_config.should_trigger(event_type, branch) {
                info!(
                    pipeline_id = %pipeline_id,
                    event_type = %event_type,
                    branch = ?branch,
                    "webhook event filtered by trigger_config, skipping pipeline"
                );
                continue;
            }

            // Apply cancel_previous concurrency policy
            if concurrency.cancel_previous {
                let _ = apply_cancel_previous(pool, &pipeline_id, branch, actor).await;
            }

            let build_id = Uuid::new_v4().to_string();

            let config_snapshot = create_config_snapshot(
                &config_path,
                config_path_explicit != 0,
                &execution_config,
                "webhook",
                commit_sha,
                branch,
                repo_url.as_deref(),
            );

            let snapshot_str = config_snapshot.to_string();
            insert_build_with_retry(
                pool,
                &BuildInsertBinds {
                    build_id: &build_id,
                    project_id: &project_id,
                    pipeline_id: &pipeline_id,
                    actor,
                    event_type: Some(event_type),
                    trigger_ref: branch,
                    commit_sha,
                    branch,
                    config_snapshot: &snapshot_str,
                    webhook_id: Some(webhook_id),
                    now,
                    trigger_type: "webhook",
                },
            )
            .await?;

            let build_number = fetch_build_number(pool, &build_id).await?;

            // Insert initial build event
            sqlx::query(
                "INSERT INTO build_events (id, build_id, from_status, to_status, actor, reason, created_at) \
                 VALUES (?1, ?2, NULL, 'queued', ?3, ?4, ?5)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&build_id)
            .bind(actor)
            .bind(format!("webhook: {event_type}"))
            .bind(now)
            .execute(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to insert build event");
                api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to record build event")
            })?;

            info!(
                build_id = %build_id,
                build_number = build_number,
                project_id = %project_id,
                pipeline_id = %pipeline_id,
                "webhook-triggered build created"
            );

            created_builds.push(Build {
                id: build_id,
                project_id: project_id.clone(),
                pipeline_id,
                build_number,
                status: "queued".to_string(),
                trigger_type: "webhook".to_string(),
                trigger_actor: actor.map(String::from),
                trigger_event: Some(event_type.to_string()),
                trigger_ref: branch.map(String::from),
                commit_sha: commit_sha.map(String::from),
                branch: branch.map(String::from),
                config_snapshot,
                runner_id: None,
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
