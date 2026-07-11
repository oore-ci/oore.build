use std::collections::HashSet;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, BuildPlatform, ConcurrencyPolicy, CreatePipelineRequest, CreatePipelineResponse,
    ListPipelinesResponse, Pipeline, PipelineDetailResponse, PipelineExecutionConfig,
    TriggerConfig, UpdatePipelineRequest, ValidatePipelineRequest, ValidatePipelineResponse,
    parse_repository_pipeline_yaml, validate_artifact_pattern,
};
use serde::Deserialize;
use sqlx::Row;
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_pipeline_project_permission, require_project_permission,
    resolve_effective_project_role,
};
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

// ── Validation helpers ──────────────────────────────────────────

const VALID_EVENTS: &[&str] = &["push", "pull_request", "tag_push"];
const MAX_STAGE_COMMANDS: usize = 128;
const MAX_COMMAND_LENGTH: usize = 2048;
const MAX_ARTIFACT_PATTERNS: usize = 64;
const MAX_PLATFORM_ARGS: usize = 64;
const MAX_ENV_VARS: usize = 128;
const MAX_ENV_KEY_LENGTH: usize = 128;
const MAX_ENV_VALUE_LENGTH: usize = 4096;
const MAX_FLUTTER_VERSION_LENGTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectTriggerMode {
    Full,
    ManualOnly,
}

fn default_execution_config() -> PipelineExecutionConfig {
    PipelineExecutionConfig::default()
}

fn validate_stage_commands(stage: &str, commands: &[String], errors: &mut Vec<String>) {
    if commands.len() > MAX_STAGE_COMMANDS {
        errors.push(format!(
            "commands.{stage} has too many entries (max {MAX_STAGE_COMMANDS})"
        ));
    }

    for (idx, command) in commands.iter().enumerate() {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            errors.push(format!("commands.{stage}[{idx}] must not be empty"));
        }
        if trimmed.len() > MAX_COMMAND_LENGTH {
            errors.push(format!(
                "commands.{stage}[{idx}] is too long (max {MAX_COMMAND_LENGTH} chars)"
            ));
        }
    }
}

fn validate_platform_args(platform: &str, args: &[String], errors: &mut Vec<String>) {
    if args.len() > MAX_PLATFORM_ARGS {
        errors.push(format!(
            "execution_config.platform_build_args.{platform} has too many entries (max {MAX_PLATFORM_ARGS})"
        ));
    }
    for (idx, arg) in args.iter().enumerate() {
        let trimmed = arg.trim();
        if trimmed.is_empty() {
            errors.push(format!(
                "execution_config.platform_build_args.{platform}[{idx}] must not be empty"
            ));
        }
        if trimmed.len() > MAX_COMMAND_LENGTH {
            errors.push(format!(
                "execution_config.platform_build_args.{platform}[{idx}] is too long (max {MAX_COMMAND_LENGTH} chars)"
            ));
        }
    }
}

fn validate_platform_command_override(
    platform: &str,
    command: &Option<String>,
    errors: &mut Vec<String>,
) {
    if let Some(command) = command {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            errors.push(format!(
                "execution_config.platform_commands.{platform} must not be empty when provided"
            ));
        }
        if trimmed.len() > MAX_COMMAND_LENGTH {
            errors.push(format!(
                "execution_config.platform_commands.{platform} is too long (max {MAX_COMMAND_LENGTH} chars)"
            ));
        }
    }
}

fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let valid_first = first == '_' || first.is_ascii_alphabetic();
    if !valid_first {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn platform_label(platform: &BuildPlatform) -> &'static str {
    match platform {
        BuildPlatform::Android => "android",
        BuildPlatform::Ios => "ios",
        BuildPlatform::Macos => "macos",
    }
}

fn validate_execution_config(cfg: &PipelineExecutionConfig) -> Vec<String> {
    let mut errors = Vec::new();

    if cfg.platforms.is_empty() {
        errors.push("execution_config.platforms must include at least one platform".to_string());
    }

    let mut seen = HashSet::new();
    for platform in &cfg.platforms {
        let label = platform_label(platform);
        if !seen.insert(label) {
            errors.push(format!(
                "execution_config.platforms contains duplicate entry '{label}'"
            ));
        }
    }

    if let Some(version) = &cfg.flutter_version {
        let trimmed = version.trim();
        if trimmed.is_empty() {
            errors.push("execution_config.flutter_version must not be empty".to_string());
        } else if trimmed.len() > MAX_FLUTTER_VERSION_LENGTH {
            errors.push(format!(
                "execution_config.flutter_version is too long (max {MAX_FLUTTER_VERSION_LENGTH} chars)"
            ));
        }
    }

    validate_stage_commands("pre_build", &cfg.commands.pre_build, &mut errors);
    validate_stage_commands("build", &cfg.commands.build, &mut errors);
    validate_stage_commands("post_build", &cfg.commands.post_build, &mut errors);
    validate_platform_args("android", &cfg.platform_build_args.android, &mut errors);
    validate_platform_args("ios", &cfg.platform_build_args.ios, &mut errors);
    validate_platform_args("macos", &cfg.platform_build_args.macos, &mut errors);
    validate_platform_command_override("android", &cfg.platform_commands.android, &mut errors);
    validate_platform_command_override("ios", &cfg.platform_commands.ios, &mut errors);
    validate_platform_command_override("macos", &cfg.platform_commands.macos, &mut errors);

    if cfg.artifact_patterns.len() > MAX_ARTIFACT_PATTERNS {
        errors.push(format!(
            "execution_config.artifact_patterns has too many entries (max {MAX_ARTIFACT_PATTERNS})"
        ));
    }

    for (idx, pattern) in cfg.artifact_patterns.iter().enumerate() {
        if let Err(error) = validate_artifact_pattern(pattern) {
            errors.push(format!("execution_config.artifact_patterns[{idx}] {error}"));
        }
    }

    if cfg.env.len() > MAX_ENV_VARS {
        errors.push(format!(
            "execution_config.env has too many entries (max {MAX_ENV_VARS})"
        ));
    }

    let mut env_keys = HashSet::new();
    for (idx, entry) in cfg.env.iter().enumerate() {
        let key = entry.key.trim();
        let value = entry.value.trim();
        if key.is_empty() {
            errors.push(format!("execution_config.env[{idx}].key must not be empty"));
        } else {
            if key.len() > MAX_ENV_KEY_LENGTH {
                errors.push(format!(
                    "execution_config.env[{idx}].key is too long (max {MAX_ENV_KEY_LENGTH} chars)"
                ));
            }
            if !is_valid_env_key(key) {
                errors.push(format!(
                    "execution_config.env[{idx}].key must match [A-Za-z_][A-Za-z0-9_]*"
                ));
            }
            if !env_keys.insert(key.to_string()) {
                errors.push(format!(
                    "execution_config.env contains duplicate key '{}'",
                    key
                ));
            }
        }
        if value.len() > MAX_ENV_VALUE_LENGTH {
            errors.push(format!(
                "execution_config.env[{idx}].value is too long (max {MAX_ENV_VALUE_LENGTH} chars)"
            ));
        }
    }

    errors
}

fn parse_json_or_default<T>(raw: &str, default: T) -> T
where
    T: for<'de> serde::Deserialize<'de>,
{
    serde_json::from_str(raw).unwrap_or(default)
}

fn validate_config_path(path: &str, explicit: bool) -> Vec<String> {
    let mut errors = Vec::new();
    if path.trim().is_empty() {
        if explicit {
            errors.push("config_path is required when config_path_explicit is true".to_string());
        } else {
            errors.push("config_path must not be empty".to_string());
        }
    }
    errors
}

fn validate_trigger_config(tc: &TriggerConfig) -> Vec<String> {
    let mut errors = Vec::new();

    for event in &tc.events {
        if !VALID_EVENTS.contains(&event.as_str()) {
            errors.push(format!(
                "Invalid event '{}'. Valid: push, pull_request, tag_push",
                event
            ));
        }
    }

    for (i, branch) in tc.branches.iter().enumerate() {
        if branch.is_empty() {
            errors.push(format!("Branch pattern at index {} is empty", i));
        }
    }

    errors
}

fn validate_manual_only_trigger_config(tc: &TriggerConfig) -> Vec<String> {
    let mut errors = Vec::new();

    if !tc.events.is_empty() {
        errors.push("trigger_config.events must be empty for local repositories".to_string());
    }
    if !tc.branches.is_empty() {
        errors.push("trigger_config.branches must be empty for local repositories".to_string());
    }

    errors
}

async fn project_trigger_mode(
    pool: &sqlx::SqlitePool,
    project_id: &str,
) -> Result<ProjectTriggerMode, (StatusCode, Json<ApiError>)> {
    let provider: Option<String> = sqlx::query_scalar(
        "SELECT i.provider \
         FROM projects p \
         LEFT JOIN integration_repositories r ON r.id = p.repository_id \
         LEFT JOIN integration_installations inst ON inst.id = r.installation_id \
         LEFT JOIN integrations i ON i.id = inst.integration_id \
         WHERE p.id = ?1 \
         LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, project_id = %project_id, "failed to resolve project trigger mode");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to resolve project source",
        )
    })?
    .flatten();

    Ok(match provider.as_deref() {
        Some("local_git") => ProjectTriggerMode::ManualOnly,
        _ => ProjectTriggerMode::Full,
    })
}

fn validate_concurrency(cp: &ConcurrencyPolicy) -> Vec<String> {
    let mut errors = Vec::new();

    if let Some(max) = cp.max_concurrent
        && !(1..=100).contains(&max)
    {
        errors.push("max_concurrent must be between 1 and 100".to_string());
    }

    errors
}

// ── Row conversion ──────────────────────────────────────────────

fn row_to_pipeline(row: &sqlx::sqlite::SqliteRow) -> Pipeline {
    let trigger_config_str: String = row.get("trigger_config");
    let trigger_config: TriggerConfig =
        parse_json_or_default(&trigger_config_str, TriggerConfig::default());

    let concurrency_str: String = row.get("concurrency");
    let concurrency: ConcurrencyPolicy =
        parse_json_or_default(&concurrency_str, ConcurrencyPolicy::default());

    let execution_config_str: String = row
        .try_get("execution_config")
        .unwrap_or_else(|_| "{}".to_string());
    let execution_config: PipelineExecutionConfig =
        parse_json_or_default(&execution_config_str, default_execution_config());

    let enabled_int: i32 = row.get("enabled");
    let config_path_explicit: i32 = row.try_get("config_path_explicit").unwrap_or(0);

    Pipeline {
        id: row.get("id"),
        project_id: row.get("project_id"),
        name: row.get("name"),
        config_path: row.get("config_path"),
        config_path_explicit: config_path_explicit != 0,
        execution_config,
        trigger_config,
        concurrency,
        enabled: enabled_int != 0,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

// ── Query parameters ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListPipelinesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ── Handlers ────────────────────────────────────────────────────

/// `POST /v1/projects/{project_id}/pipelines` — create a pipeline.
pub async fn create_pipeline(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(req): Json<CreatePipelineRequest>,
) -> ApiResult<CreatePipelineResponse> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let effective = resolve_effective_project_role(
        &pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::ManagePipelines)?;

    let name = req.name.trim();
    if name.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Pipeline name must not be empty",
        ));
    }

    // Validate project exists
    let project_exists: bool =
        sqlx::query_scalar("SELECT COUNT(*) > 0 FROM projects WHERE id = ?1")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap_or(false);

    if !project_exists {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Project not found",
        ));
    }

    // Validate trigger_config if provided
    let trigger_config = req.trigger_config.unwrap_or_default();
    let tc_errors = validate_trigger_config(&trigger_config);
    if !tc_errors.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_trigger_config",
            tc_errors.join("; "),
        ));
    }
    if project_trigger_mode(&pool, &project_id).await? == ProjectTriggerMode::ManualOnly {
        let local_tc_errors = validate_manual_only_trigger_config(&trigger_config);
        if !local_tc_errors.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_trigger_config",
                local_tc_errors.join("; "),
            ));
        }
    }

    // Validate concurrency if provided
    let concurrency = req.concurrency.unwrap_or_default();
    let cp_errors = validate_concurrency(&concurrency);
    if !cp_errors.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_concurrency",
            cp_errors.join("; "),
        ));
    }

    let config_path_explicit = req.config_path_explicit.unwrap_or(false);
    let config_path = req
        .config_path
        .unwrap_or_else(|| ".oore.yaml".to_string())
        .trim()
        .to_string();
    let config_path_errors = validate_config_path(&config_path, config_path_explicit);
    if !config_path_errors.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_config_path",
            config_path_errors.join("; "),
        ));
    }

    let execution_config = req
        .execution_config
        .unwrap_or_else(default_execution_config);
    let exec_errors = validate_execution_config(&execution_config);
    if !exec_errors.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_execution_config",
            exec_errors.join("; "),
        ));
    }

    let now = now_unix();
    let pipeline_id = Uuid::new_v4().to_string();

    let trigger_config_json = serde_json::to_string(&trigger_config).unwrap_or_default();
    let concurrency_json = serde_json::to_string(&concurrency).unwrap_or_default();
    let execution_config_json = serde_json::to_string(&execution_config).unwrap_or_default();

    sqlx::query(
        "INSERT INTO pipelines (id, project_id, name, config_path, config_path_explicit, execution_config, trigger_config, concurrency, enabled, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?9)",
    )
    .bind(&pipeline_id)
    .bind(&project_id)
    .bind(name)
    .bind(&config_path)
    .bind(if config_path_explicit { 1 } else { 0 })
    .bind(&execution_config_json)
    .bind(&trigger_config_json)
    .bind(&concurrency_json)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to create pipeline");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", "Failed to create pipeline")
    })?;

    let details = serde_json::json!({
        "project_id": project_id,
        "pipeline_name": name,
        "created_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "pipeline_created",
        "pipeline",
        Some(&pipeline_id),
        Some(&details),
    )
    .await;

    info!(pipeline_id = %pipeline_id, project_id = %project_id, name = %name, "pipeline created");

    let pipeline = Pipeline {
        id: pipeline_id,
        project_id,
        name: name.to_string(),
        config_path,
        config_path_explicit,
        execution_config,
        trigger_config,
        concurrency,
        enabled: true,
        created_at: now,
        updated_at: now,
    };

    Ok(Json(CreatePipelineResponse { pipeline }))
}

/// `GET /v1/projects/{project_id}/pipelines` — list pipelines for a project.
pub async fn list_pipelines(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Query(params): Query<ListPipelinesQuery>,
) -> ApiResult<ListPipelinesResponse> {
    let store = state.store.lock().await;
    let pool = store.pool();

    let effective = resolve_effective_project_role(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::Read)?;

    // Validate project exists
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

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pipelines WHERE project_id = ?1")
        .bind(&project_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let rows = sqlx::query(
        "SELECT * FROM pipelines WHERE project_id = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
    )
    .bind(&project_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to list pipelines");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to list pipelines",
        )
    })?;

    let pipelines = rows.iter().map(row_to_pipeline).collect();

    Ok(Json(ListPipelinesResponse { pipelines, total }))
}

/// `GET /v1/pipelines/{pipeline_id}` — pipeline detail with build count.
pub async fn get_pipeline(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<PipelineDetailResponse> {
    let store = state.store.lock().await;
    let pool = store.pool();
    require_pipeline_project_permission(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::Read,
    )
    .await?;

    let pipeline_row = sqlx::query("SELECT * FROM pipelines WHERE id = ?1")
        .bind(&pipeline_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch pipeline");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch pipeline",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Pipeline not found"))?;

    let pipeline = row_to_pipeline(&pipeline_row);

    let build_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM builds WHERE pipeline_id = ?1")
        .bind(&pipeline_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    Ok(Json(PipelineDetailResponse {
        pipeline,
        build_count,
    }))
}

/// `PATCH /v1/pipelines/{pipeline_id}` — partial update.
pub async fn update_pipeline(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
    Json(req): Json<UpdatePipelineRequest>,
) -> ApiResult<CreatePipelineResponse> {
    let store = state.store.lock().await;
    let pool = store.pool();
    require_pipeline_project_permission(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;

    let row = sqlx::query("SELECT * FROM pipelines WHERE id = ?1")
        .bind(&pipeline_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to fetch pipeline");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to fetch pipeline",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Pipeline not found"))?;

    if req.name.is_none()
        && req.config_path.is_none()
        && req.config_path_explicit.is_none()
        && req.execution_config.is_none()
        && req.trigger_config.is_none()
        && req.concurrency.is_none()
        && req.enabled.is_none()
    {
        return Ok(Json(CreatePipelineResponse {
            pipeline: row_to_pipeline(&row),
        }));
    }

    let mut pipeline = row_to_pipeline(&row);
    let trigger_mode = project_trigger_mode(pool, &pipeline.project_id).await?;

    if let Some(name) = req.name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "Pipeline name must not be empty",
            ));
        }
        pipeline.name = trimmed.to_string();
    }

    if let Some(config_path) = req.config_path {
        let trimmed = config_path.trim().to_string();
        let path_errors = validate_config_path(
            &trimmed,
            req.config_path_explicit
                .unwrap_or(pipeline.config_path_explicit),
        );
        if !path_errors.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_config_path",
                path_errors.join("; "),
            ));
        }
        pipeline.config_path = trimmed;
    }

    if let Some(config_path_explicit) = req.config_path_explicit {
        pipeline.config_path_explicit = config_path_explicit;
    }

    let path_errors = validate_config_path(&pipeline.config_path, pipeline.config_path_explicit);
    if !path_errors.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_config_path",
            path_errors.join("; "),
        ));
    }

    if let Some(trigger_config) = req.trigger_config {
        let errors = validate_trigger_config(&trigger_config);
        if !errors.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_trigger_config",
                errors.join("; "),
            ));
        }
        if trigger_mode == ProjectTriggerMode::ManualOnly {
            let local_tc_errors = validate_manual_only_trigger_config(&trigger_config);
            if !local_tc_errors.is_empty() {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_trigger_config",
                    local_tc_errors.join("; "),
                ));
            }
        }
        pipeline.trigger_config = trigger_config;
    }

    if let Some(concurrency) = req.concurrency {
        let errors = validate_concurrency(&concurrency);
        if !errors.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_concurrency",
                errors.join("; "),
            ));
        }
        pipeline.concurrency = concurrency;
    }

    if let Some(execution_config) = req.execution_config {
        let errors = validate_execution_config(&execution_config);
        if !errors.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_execution_config",
                errors.join("; "),
            ));
        }
        pipeline.execution_config = execution_config;
    }

    if let Some(enabled) = req.enabled {
        pipeline.enabled = enabled;
    }

    let now = now_unix();
    sqlx::query(
        "UPDATE pipelines \
         SET name = ?1, config_path = ?2, config_path_explicit = ?3, execution_config = ?4, trigger_config = ?5, concurrency = ?6, enabled = ?7, updated_at = ?8 \
         WHERE id = ?9",
    )
    .bind(&pipeline.name)
    .bind(&pipeline.config_path)
    .bind(if pipeline.config_path_explicit { 1 } else { 0 })
    .bind(serde_json::to_string(&pipeline.execution_config).unwrap_or_default())
    .bind(serde_json::to_string(&pipeline.trigger_config).unwrap_or_default())
    .bind(serde_json::to_string(&pipeline.concurrency).unwrap_or_default())
    .bind(if pipeline.enabled { 1 } else { 0 })
    .bind(now)
    .bind(&pipeline_id)
    .execute(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to update pipeline");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to update pipeline",
        )
    })?;

    let details = serde_json::json!({
        "updated_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "pipeline_updated",
        "pipeline",
        Some(&pipeline_id),
        Some(&details),
    )
    .await;

    info!(pipeline_id = %pipeline_id, "pipeline updated");

    let row = sqlx::query("SELECT * FROM pipelines WHERE id = ?1")
        .bind(&pipeline_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to reload pipeline");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to reload pipeline",
            )
        })?;

    Ok(Json(CreatePipelineResponse {
        pipeline: row_to_pipeline(&row),
    }))
}

/// `DELETE /v1/pipelines/{pipeline_id}` — delete a pipeline.
pub async fn delete_pipeline(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    check_permission(&state.enforcer, &auth.0.role, "pipelines", "delete").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    // Use a transaction so the active-build check, terminal-build cleanup,
    // and pipeline delete are atomic (prevents race with concurrent build creation).
    let mut tx = pool.begin().await.map_err(|e| {
        error!(error = %e, "failed to begin transaction");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete pipeline",
        )
    })?;

    // Verify pipeline exists
    let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM pipelines WHERE id = ?1")
        .bind(&pipeline_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap_or(false);

    if !exists {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Pipeline not found",
        ));
    }

    // Check for non-terminal builds
    let active_builds: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM builds WHERE pipeline_id = ?1 \
         AND status NOT IN ('succeeded', 'failed', 'canceled', 'timed_out', 'expired')",
    )
    .bind(&pipeline_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(0);

    if active_builds > 0 {
        return Err(api_err(
            StatusCode::CONFLICT,
            "active_builds",
            "Cannot delete pipeline with active builds",
        ));
    }

    // Delete terminal builds first (non-cascading FK on builds.pipeline_id)
    sqlx::query(
        "DELETE FROM builds WHERE pipeline_id = ?1 \
         AND status IN ('succeeded', 'failed', 'canceled', 'timed_out', 'expired')",
    )
    .bind(&pipeline_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to clean up builds for pipeline");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete pipeline",
        )
    })?;

    sqlx::query("DELETE FROM pipelines WHERE id = ?1")
        .bind(&pipeline_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to delete pipeline");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to delete pipeline",
            )
        })?;

    tx.commit().await.map_err(|e| {
        error!(error = %e, "failed to commit delete transaction");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete pipeline",
        )
    })?;

    let details = serde_json::json!({
        "deleted_by": auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "pipeline_deleted",
        "pipeline",
        Some(&pipeline_id),
        Some(&details),
    )
    .await;

    info!(pipeline_id = %pipeline_id, "pipeline deleted");

    Ok(Json(serde_json::json!({"ok": true})))
}

/// `POST /v1/pipelines/validate` — dry-run validation of pipeline config.
pub async fn validate_pipeline(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<ValidatePipelineRequest>,
) -> ApiResult<ValidatePipelineResponse> {
    check_permission(&state.enforcer, &auth.0.role, "pipelines", "read").await?;

    let mut errors = Vec::new();
    let config_path_explicit = req.config_path_explicit.unwrap_or(false);

    if let Some(ref tc) = req.trigger_config {
        errors.extend(validate_trigger_config(tc));
    }

    if let Some(ref cp) = req.concurrency {
        errors.extend(validate_concurrency(cp));
    }

    if let Some(ref path) = req.config_path {
        errors.extend(validate_config_path(path, config_path_explicit));
    } else if config_path_explicit {
        errors.push("config_path is required when config_path_explicit is true".to_string());
    }

    if let Some(ref cfg) = req.execution_config {
        errors.extend(validate_execution_config(cfg));
    }
    if let Some(ref yaml) = req.repository_yaml
        && let Err(error) = parse_repository_pipeline_yaml(yaml)
    {
        errors.extend(error.lines().map(str::to_string));
    }

    let valid = errors.is_empty();

    Ok(Json(ValidatePipelineResponse { valid, errors }))
}
