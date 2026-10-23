use std::collections::HashSet;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, ApprovalPolicy, BuildPlatform, ConcurrencyPolicy, CreatePipelineRequest,
    CreatePipelineResponse, DEFAULT_BUILD_TIMEOUT_MINUTES, ListPipelinesResponse,
    MAX_BUILD_TIMEOUT_MINUTES, OkResponse, Pipeline, PipelineDetailResponse,
    PipelineExecutionConfig, TriggerConfig, UpdatePipelineRequest, ValidatePipelineRequest,
    ValidatePipelineResponse, is_valid_pipeline_env_key, normalize_runner_labels,
    parse_repository_config, secret_expression_errors, validate_artifact_pattern,
    validate_build_number_offset, validate_repository_config_path,
};
use serde::Deserialize;
use sqlx::Row;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, check_project_permission, require_pipeline_project_permission,
    require_project_permission, resolve_effective_project_role,
};
use crate::rbac::check_permission;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;
const REDACTED_ENV_VALUE: &str = "[REDACTED]";

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
const MAX_TRIGGER_PATTERNS: usize = 64;
const MAX_TRIGGER_PATTERN_LENGTH: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectTriggerMode {
    Full,
    ManualOnly,
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

    if let Err(error) = validate_build_number_offset(cfg.build_number_offset) {
        errors.push(format!("execution_config.build_number_offset {error}"));
    }

    if let Err(error) = normalize_runner_labels(&cfg.runs_on) {
        errors.push(format!("execution_config.runs_on {error}"));
    }

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
            if !is_valid_pipeline_env_key(key) {
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

    errors.extend(
        secret_expression_errors(cfg)
            .into_iter()
            .map(|error| format!("execution_config.{error}")),
    );
    errors
}

fn validate_config_path(path: &str, explicit: bool) -> Vec<String> {
    let mut errors = Vec::new();
    if path.trim().is_empty() {
        if explicit {
            errors.push("config_path is required when config_path_explicit is true".to_string());
        } else {
            errors.push("config_path must not be empty".to_string());
        }
    } else if let Err(error) = validate_repository_config_path(path) {
        errors.push(format!("config_path {error}"));
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

    for (field, patterns) in [
        ("tags", &tc.tags),
        ("paths", &tc.paths),
        ("paths_ignore", &tc.paths_ignore),
    ] {
        if patterns.len() > MAX_TRIGGER_PATTERNS {
            errors.push(format!(
                "trigger_config.{field} has too many entries (max {MAX_TRIGGER_PATTERNS})"
            ));
        }
        for (i, pattern) in patterns.iter().enumerate() {
            let trimmed = pattern.trim();
            if trimmed.is_empty() {
                errors.push(format!("trigger_config.{field}[{i}] must not be empty"));
            } else if trimmed.len() > MAX_TRIGGER_PATTERN_LENGTH {
                errors.push(format!(
                    "trigger_config.{field}[{i}] is too long (max {MAX_TRIGGER_PATTERN_LENGTH} chars)"
                ));
            } else if field != "tags" && (trimmed.starts_with('/') || trimmed.contains('\\')) {
                errors.push(format!(
                    "trigger_config.{field}[{i}] must be a repository-relative path using '/' separators"
                ));
            }
        }
    }

    let event_listed = |event: &str| tc.events.is_empty() || tc.events.iter().any(|e| e == event);
    if !tc.tags.is_empty() && !event_listed("tag_push") {
        errors.push("trigger_config.tags needs the tag_push event".to_string());
    }
    if (!tc.paths.is_empty() || !tc.paths_ignore.is_empty())
        && !event_listed("push")
        && !event_listed("pull_request")
    {
        errors.push("trigger_config.paths apply only to push and pull_request events".to_string());
    }

    errors.extend(crate::pipeline_schedules::validate_schedules(&tc.schedules));
    errors
}

fn validate_timeout_minutes(minutes: u32) -> Vec<String> {
    if (1..=MAX_BUILD_TIMEOUT_MINUTES).contains(&minutes) {
        Vec::new()
    } else {
        vec![format!(
            "timeout_minutes must be between 1 and {MAX_BUILD_TIMEOUT_MINUTES}"
        )]
    }
}

fn validate_approval_policy(policy: &ApprovalPolicy) -> Vec<String> {
    let mut errors: Vec<String> = policy
        .events
        .iter()
        .filter(|event| !ApprovalPolicy::EVENTS.contains(&event.as_str()))
        .map(|event| {
            format!(
                "Invalid approval event '{event}'. Valid: {}",
                ApprovalPolicy::EVENTS.join(", ")
            )
        })
        .collect();
    if policy
        .expires_after_hours
        .is_some_and(|hours| !(1..=ApprovalPolicy::MAX_EXPIRES_AFTER_HOURS).contains(&hours))
    {
        errors.push(format!(
            "approval.expires_after_hours must be between 1 and {}",
            ApprovalPolicy::MAX_EXPIRES_AFTER_HOURS
        ));
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
    if !tc.tags.is_empty() || !tc.paths.is_empty() || !tc.paths_ignore.is_empty() {
        errors.push(
            "trigger_config.tags, paths and paths_ignore must be empty for local repositories"
                .to_string(),
        );
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
        serde_json::from_str(&trigger_config_str).unwrap_or_default();

    let concurrency_str: String = row.get("concurrency");
    let concurrency: ConcurrencyPolicy = serde_json::from_str(&concurrency_str).unwrap_or_default();

    let execution_config_str: String = row
        .try_get("execution_config")
        .unwrap_or_else(|_| "{}".to_string());
    let execution_config: PipelineExecutionConfig =
        serde_json::from_str(&execution_config_str).unwrap_or_default();

    let enabled_int: i32 = row.get("enabled");
    let config_path_explicit: i32 = row.try_get("config_path_explicit").unwrap_or(0);
    let timeout_minutes = row
        .try_get::<i64, _>("timeout_minutes")
        .ok()
        .and_then(|minutes| u32::try_from(minutes).ok())
        .unwrap_or(DEFAULT_BUILD_TIMEOUT_MINUTES);
    let approval: ApprovalPolicy = row
        .try_get::<String, _>("approval_policy")
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();

    Pipeline {
        id: row.get("id"),
        project_id: row.get("project_id"),
        name: row.get("name"),
        config_path: row.get("config_path"),
        config_path_explicit: config_path_explicit != 0,
        execution_config,
        trigger_config,
        concurrency,
        timeout_minutes,
        approval,
        enabled: enabled_int != 0,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn shape_pipeline_response(mut pipeline: Pipeline, can_manage: bool) -> Pipeline {
    if !can_manage {
        for entry in &mut pipeline.execution_config.env {
            entry.value = REDACTED_ENV_VALUE.to_string();
        }
    }
    pipeline
}

// ── Query parameters ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListPipelinesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub search: Option<String>,
    pub sort: Option<String>,
    pub direction: Option<String>,
}

fn pipeline_order_clause(
    sort: Option<&str>,
    direction: Option<&str>,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let column = match sort.unwrap_or("created_at") {
        "created_at" => "p.created_at",
        "name" => "p.name COLLATE NOCASE",
        _ => {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "sort must be created_at or name",
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

    Ok(format!("{column} {direction}, p.id {direction}"))
}

// ── Handlers ────────────────────────────────────────────────────

/// `POST /v1/projects/{project_id}/pipelines` — create a pipeline.
pub async fn create_pipeline(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(req): Json<CreatePipelineRequest>,
) -> Result<(StatusCode, Json<CreatePipelineResponse>), (StatusCode, Json<ApiError>)> {
    let pool = state.db.clone();

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

    let mut execution_config = req.execution_config.unwrap_or_default();
    let exec_errors = validate_execution_config(&execution_config);
    if !exec_errors.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_execution_config",
            exec_errors.join("; "),
        ));
    }
    execution_config.runs_on =
        normalize_runner_labels(&execution_config.runs_on).expect("runs_on validated above");

    let timeout_minutes = req.timeout_minutes.unwrap_or(DEFAULT_BUILD_TIMEOUT_MINUTES);
    let approval = req.approval.unwrap_or_default();
    let gate_errors = [
        validate_timeout_minutes(timeout_minutes),
        validate_approval_policy(&approval),
    ]
    .concat();
    if !gate_errors.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            gate_errors.join("; "),
        ));
    }

    let now = now_unix();
    let pipeline_id = Uuid::new_v4().to_string();

    let trigger_config_json = serde_json::to_string(&trigger_config).unwrap_or_default();
    let concurrency_json = serde_json::to_string(&concurrency).unwrap_or_default();
    let execution_config_json = serde_json::to_string(&execution_config).unwrap_or_default();

    sqlx::query(
        "INSERT INTO pipelines (id, project_id, name, config_path, config_path_explicit, execution_config, trigger_config, concurrency, timeout_minutes, approval_policy, enabled, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?10, ?11, 1, ?9, ?9)",
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
    .bind(timeout_minutes)
    .bind(serde_json::to_string(&approval).unwrap_or_default())
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
        timeout_minutes,
        approval,
        enabled: true,
        created_at: now,
        updated_at: now,
    };

    Ok((
        StatusCode::CREATED,
        Json(CreatePipelineResponse { pipeline }),
    ))
}

/// `GET /v1/projects/{project_id}/pipelines` — list pipelines for a project.
pub async fn list_pipelines(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Query(params): Query<ListPipelinesQuery>,
) -> ApiResult<ListPipelinesResponse> {
    let pool = &state.db;

    let effective = resolve_effective_project_role(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::Read)?;
    let can_manage = check_project_permission(&effective, ProjectPermission::ManagePipelines);

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
    let order_by = pipeline_order_clause(params.sort.as_deref(), params.direction.as_deref())?;

    let (total, rows) = if let Some(search) = params
        .search
        .as_deref()
        .map(str::trim)
        .filter(|search| !search.is_empty())
    {
        let pattern = format!("%{search}%");
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pipelines WHERE project_id = ?1 AND name LIKE ?2",
        )
        .bind(&project_id)
        .bind(&pattern)
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT p.* FROM pipelines p WHERE p.project_id = ?1 AND p.name LIKE ?2 \
             ORDER BY {order_by} LIMIT ?3 OFFSET ?4"
        )))
        .bind(&project_id)
        .bind(&pattern)
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
        (total, rows)
    } else {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pipelines WHERE project_id = ?1")
            .bind(&project_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT p.* FROM pipelines p WHERE p.project_id = ?1 \
             ORDER BY {order_by} LIMIT ?2 OFFSET ?3"
        )))
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
        (total, rows)
    };

    let pipelines = rows
        .iter()
        .map(|row| shape_pipeline_response(row_to_pipeline(row), can_manage))
        .collect();

    Ok(Json(ListPipelinesResponse { pipelines, total }))
}

/// `GET /v1/pipelines/{pipeline_id}` — pipeline detail with build count.
pub async fn get_pipeline(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<PipelineDetailResponse> {
    let pool = &state.db;
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

    let project_id: String = pipeline_row.get("project_id");
    let effective = resolve_effective_project_role(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::Read)?;
    let pipeline = shape_pipeline_response(
        row_to_pipeline(&pipeline_row),
        check_project_permission(&effective, ProjectPermission::ManagePipelines),
    );

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
    let pool = &state.db;
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
        && req.timeout_minutes.is_none()
        && req.approval.is_none()
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

    if let Some(mut execution_config) = req.execution_config {
        let errors = validate_execution_config(&execution_config);
        if !errors.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_execution_config",
                errors.join("; "),
            ));
        }
        execution_config.runs_on =
            normalize_runner_labels(&execution_config.runs_on).expect("runs_on validated above");
        pipeline.execution_config = execution_config;
    }

    if let Some(timeout_minutes) = req.timeout_minutes {
        let errors = validate_timeout_minutes(timeout_minutes);
        if !errors.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                errors.join("; "),
            ));
        }
        pipeline.timeout_minutes = timeout_minutes;
    }

    if let Some(approval) = req.approval
        && approval != pipeline.approval
    {
        let errors = validate_approval_policy(&approval);
        if !errors.is_empty() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                errors.join("; "),
            ));
        }
        // Developers manage pipelines, but only maintainers decide who must
        // approve their builds.
        require_pipeline_project_permission(
            pool,
            &auth.0.user_id,
            &auth.0.role,
            &auth.0.auth_source,
            &pipeline_id,
            ProjectPermission::ApproveBuild,
        )
        .await?;
        pipeline.approval = approval;
    }

    if let Some(enabled) = req.enabled {
        pipeline.enabled = enabled;
    }

    let now = now_unix();
    sqlx::query(
        "UPDATE pipelines \
         SET name = ?1, config_path = ?2, config_path_explicit = ?3, execution_config = ?4, trigger_config = ?5, concurrency = ?6, enabled = ?7, updated_at = ?8, \
             timeout_minutes = ?10, approval_policy = ?11 \
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
    .bind(pipeline.timeout_minutes)
    .bind(serde_json::to_string(&pipeline.approval).unwrap_or_default())
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
) -> ApiResult<OkResponse> {
    check_permission(&state.enforcer, &auth.0.role, "pipelines", "delete").await?;

    let pool = &state.db;
    let DeletedPipelineFiles {
        artifact_paths,
        screenshot_keys,
    } = delete_pipeline_rows(pool, &pipeline_id).await?;

    let storage = state.storage.read().await.clone();
    crate::build_feedback::delete_screenshots(&storage, &screenshot_keys).await;
    let mut artifacts_left = 0;
    for file_path in &artifact_paths {
        if let Err(error) = storage.delete_object(file_path).await {
            warn!(pipeline_id = %pipeline_id, %file_path, %error, "failed to delete a deleted pipeline's artifact");
            artifacts_left += 1;
        }
    }

    let details = serde_json::json!({
        "deleted_by": auth.0.email,
        "artifacts_deleted": artifact_paths.len() - artifacts_left,
        "artifacts_left": artifacts_left,
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

    Ok(Json(OkResponse { ok: true }))
}

/// Storage objects of a deleted pipeline's builds, which the caller removes
/// after the commit.
#[derive(Debug)]
struct DeletedPipelineFiles {
    artifact_paths: Vec<String>,
    screenshot_keys: Vec<String>,
}

/// Delete a pipeline and its finished builds. Returns the storage objects of
/// the deleted builds' artifacts and feedback screenshots.
async fn delete_pipeline_rows(
    pool: &sqlx::SqlitePool,
    pipeline_id: &str,
) -> Result<DeletedPipelineFiles, (StatusCode, Json<ApiError>)> {
    // Use a transaction so the active-build check, terminal-build cleanup,
    // and pipeline delete are atomic (prevents race with concurrent build creation).
    // IMMEDIATE takes the write lock up front, so a concurrent write cannot
    // make the later upgrade fail with SQLITE_BUSY.
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(|e| {
        error!(error = %e, "failed to begin transaction");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete pipeline",
        )
    })?;

    // Verify pipeline exists
    let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM pipelines WHERE id = ?1")
        .bind(pipeline_id)
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
    .bind(pipeline_id)
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

    // Deleting the builds deletes their releases. Keep one the daemon is still
    // driving; a halted release only moves when someone resumes it.
    let releases_in_progress: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM releases r JOIN builds b ON b.id = r.build_id \
         WHERE b.pipeline_id = ?1 AND r.state NOT IN ('complete', 'failed', 'halted')",
    )
    .bind(pipeline_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to check releases for pipeline");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete pipeline",
        )
    })?;
    if releases_in_progress > 0 {
        return Err(api_err(
            StatusCode::CONFLICT,
            "releases_in_progress",
            "Cannot delete a pipeline while one of its releases is in progress. \
             Halt or complete the release, or wait for the store to finish, first.",
        ));
    }

    // The artifact rows go with their builds, so collect the stored files to
    // delete once the transaction commits.
    let artifact_paths: Vec<String> = sqlx::query_scalar(
        "SELECT a.file_path FROM artifacts a JOIN builds b ON b.id = a.build_id \
         WHERE b.pipeline_id = ?1",
    )
    .bind(pipeline_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to list artifacts for pipeline");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to delete pipeline",
        )
    })?;

    // Feedback rows go with their builds too.
    let screenshot_keys =
        crate::build_feedback::screenshot_keys_for_pipeline(&mut *tx, pipeline_id)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to list feedback screenshots for pipeline");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to delete pipeline",
                )
            })?;

    // Delete terminal builds first (non-cascading FK on builds.pipeline_id)
    sqlx::query(
        "DELETE FROM builds WHERE pipeline_id = ?1 \
         AND status IN ('succeeded', 'failed', 'canceled', 'timed_out', 'expired')",
    )
    .bind(pipeline_id)
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

    sqlx::query("DELETE FROM pipeline_schedule_runs WHERE pipeline_id = ?1")
        .bind(pipeline_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to delete pipeline schedules");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to delete pipeline",
            )
        })?;

    sqlx::query("DELETE FROM pipelines WHERE id = ?1")
        .bind(pipeline_id)
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
    Ok(DeletedPipelineFiles {
        artifact_paths,
        screenshot_keys,
    })
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

    if let Some(timeout_minutes) = req.timeout_minutes {
        errors.extend(validate_timeout_minutes(timeout_minutes));
    }

    if let Some(ref approval) = req.approval {
        errors.extend(validate_approval_policy(approval));
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
        && let Err(error) = parse_repository_config(yaml)
    {
        errors.extend(error.lines().map(str::to_string));
    }

    let valid = errors.is_empty();

    Ok(Json(ValidatePipelineResponse { valid, errors }))
}

#[cfg(test)]
mod delete_tests {
    use super::*;

    async fn pool() -> sqlx::SqlitePool {
        let pool = crate::releases::tests::pool().await;
        for statement in [
            "INSERT INTO artifacts (id, build_id, name, file_path, created_at) \
             VALUES ('a1', 'b1', 'app.aab', 'artifacts/b1/app.aab', 0)",
            "INSERT INTO releases (id, project_id, build_id, store, track, state, created_at, updated_at) \
             VALUES ('r1', 'p1', 'b1', 'google_play', 'production', 'rolling', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn a_release_in_progress_blocks_deleting_its_pipeline() {
        let pool = pool().await;

        let (status, _) = delete_pipeline_rows(&pool, "pl1").await.unwrap_err();

        assert_eq!(status, StatusCode::CONFLICT);
        let builds: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM builds")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(builds, 1);
    }

    #[tokio::test]
    async fn deleting_a_pipeline_returns_its_artifact_files() {
        let pool = pool().await;
        sqlx::query("UPDATE releases SET state = 'halted'")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO build_feedback (id, build_id, author_user_id, author_email, message, \
             screenshot_key, created_at) \
             VALUES ('f1', 'b1', 'qa', 'qa@example.com', 'hi', 'feedback/p1/b1/f1.png', 0), \
                    ('f2', 'b1', 'qa', 'qa@example.com', 'no shot', NULL, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let files = delete_pipeline_rows(&pool, "pl1").await.unwrap();

        assert_eq!(files.artifact_paths, ["artifacts/b1/app.aab"]);
        assert_eq!(files.screenshot_keys, ["feedback/p1/b1/f1.png"]);
        let feedback: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM build_feedback")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(feedback, 0);
        let pipelines: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pipelines")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(pipelines, 0);
    }
}
