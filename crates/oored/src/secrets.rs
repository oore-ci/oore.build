use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use oore_contract::{
    ApiError, CreateSecretRequest, ListSecretsResponse, MAX_SECRET_VALUE_BYTES, OkResponse,
    RotateSecretRequest, RunnerJobSecret, RunnerJobSecretsResponse, SecretResponse, SecretScope,
    SecretSummary, validate_secret_name,
};
use sqlx::{Row, SqlitePool};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_pipeline_project_permission, require_project_permission,
    resolve_effective_project_role,
};
use crate::rbac::check_permission;
use crate::runners::{RunnerAuth, require_active_job_signing_grant};
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiFailure = (StatusCode, Json<ApiError>);
type ApiResult<T> = Result<Json<T>, ApiFailure>;

/// The scope a request addresses, resolved from its route.
#[derive(Debug, Clone)]
struct SecretTarget {
    scope: SecretScope,
    project_id: Option<String>,
    pipeline_id: Option<String>,
}

impl SecretTarget {
    fn instance() -> Self {
        Self {
            scope: SecretScope::Instance,
            project_id: None,
            pipeline_id: None,
        }
    }

    fn audit_resource(&self) -> (&'static str, Option<&str>) {
        match self.scope {
            SecretScope::Instance => ("instance", None),
            SecretScope::Project => ("project", self.project_id.as_deref()),
            SecretScope::Pipeline => ("pipeline", self.pipeline_id.as_deref()),
        }
    }
}

fn store_error(context: &str, e: impl std::fmt::Display) -> ApiFailure {
    error!(error = %e, "{context}");
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", context)
}

fn parse_scope(raw: &str) -> SecretScope {
    match raw {
        "project" => SecretScope::Project,
        "pipeline" => SecretScope::Pipeline,
        _ => SecretScope::Instance,
    }
}

fn row_to_summary(row: &sqlx::sqlite::SqliteRow) -> SecretSummary {
    SecretSummary {
        name: row.get("name"),
        scope: parse_scope(row.get::<String, _>("scope").as_str()),
        project_id: row.get("project_id"),
        pipeline_id: row.get("pipeline_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn validate_value(value: &str) -> Result<(), ApiFailure> {
    if value.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Secret value must not be empty",
        ));
    }
    if value.len() > MAX_SECRET_VALUE_BYTES {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            format!("Secret value exceeds {MAX_SECRET_VALUE_BYTES} bytes"),
        ));
    }
    Ok(())
}

async fn find_summary(
    pool: &SqlitePool,
    target: &SecretTarget,
    name: &str,
) -> Result<Option<SecretSummary>, ApiFailure> {
    let row = sqlx::query(
        "SELECT name, scope, project_id, pipeline_id, created_at, updated_at \
         FROM secrets WHERE scope = ?1 AND project_id IS ?2 AND pipeline_id IS ?3 AND name = ?4",
    )
    .bind(target.scope.as_str())
    .bind(&target.project_id)
    .bind(&target.pipeline_id)
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|e| store_error("Failed to load secret", e))?;
    Ok(row.as_ref().map(row_to_summary))
}

async fn list_for_target(
    pool: &SqlitePool,
    target: &SecretTarget,
) -> ApiResult<ListSecretsResponse> {
    let rows = sqlx::query(
        "SELECT name, scope, project_id, pipeline_id, created_at, updated_at \
         FROM secrets WHERE scope = ?1 AND project_id IS ?2 AND pipeline_id IS ?3 \
         ORDER BY name ASC",
    )
    .bind(target.scope.as_str())
    .bind(&target.project_id)
    .bind(&target.pipeline_id)
    .fetch_all(pool)
    .await
    .map_err(|e| store_error("Failed to list secrets", e))?;
    Ok(Json(ListSecretsResponse {
        secrets: rows.iter().map(row_to_summary).collect(),
    }))
}

async fn audit(
    pool: &SqlitePool,
    auth: &AuthUser,
    action: &str,
    target: &SecretTarget,
    name: &str,
) {
    let (resource_type, resource_id) = target.audit_resource();
    let details = serde_json::json!({
        "name": name,
        "scope": target.scope.as_str(),
        "actor": &auth.0.email,
    })
    .to_string();
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        action,
        resource_type,
        resource_id,
        Some(&details),
    )
    .await;
}

async fn create_for_target(
    state: &AppState,
    auth: &AuthUser,
    target: SecretTarget,
    req: CreateSecretRequest,
) -> Result<(StatusCode, Json<SecretResponse>), ApiFailure> {
    let name = req.name.trim().to_string();
    validate_secret_name(&name).map_err(|error| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            format!("Secret name {error}"),
        )
    })?;
    validate_value(&req.value)?;

    if find_summary(&state.db, &target, &name).await?.is_some() {
        return Err(api_err(
            StatusCode::CONFLICT,
            "secret_exists",
            format!("Secret '{name}' already exists in this scope; rotate it instead"),
        ));
    }

    let encrypted = crypto::encrypt(&req.value, &state.encryption_key).map_err(|e| {
        error!(error = %e, "failed to encrypt secret");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "encryption_error",
            "Failed to encrypt secret",
        )
    })?;
    let now = now_unix();
    sqlx::query(
        "INSERT INTO secrets (
            id, scope, project_id, pipeline_id, name, value_encrypted,
            created_by, updated_by, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, ?8)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(target.scope.as_str())
    .bind(&target.project_id)
    .bind(&target.pipeline_id)
    .bind(&name)
    .bind(encrypted)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| store_error("Failed to save secret", e))?;

    audit(&state.db, auth, "secret_created", &target, &name).await;
    info!(name = %name, scope = target.scope.as_str(), actor = %auth.0.email, "created secret");

    let secret = find_summary(&state.db, &target, &name)
        .await?
        .ok_or_else(|| store_error("Failed to load secret", "secret vanished after insert"))?;
    Ok((StatusCode::CREATED, Json(SecretResponse { secret })))
}

async fn rotate_for_target(
    state: &AppState,
    auth: &AuthUser,
    target: SecretTarget,
    name: String,
    req: RotateSecretRequest,
) -> ApiResult<SecretResponse> {
    validate_value(&req.value)?;
    let encrypted = crypto::encrypt(&req.value, &state.encryption_key).map_err(|e| {
        error!(error = %e, "failed to encrypt secret");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "encryption_error",
            "Failed to encrypt secret",
        )
    })?;
    let result = sqlx::query(
        "UPDATE secrets SET value_encrypted = ?5, updated_by = ?6, updated_at = ?7 \
         WHERE scope = ?1 AND project_id IS ?2 AND pipeline_id IS ?3 AND name = ?4",
    )
    .bind(target.scope.as_str())
    .bind(&target.project_id)
    .bind(&target.pipeline_id)
    .bind(&name)
    .bind(encrypted)
    .bind(&auth.0.user_id)
    .bind(now_unix())
    .execute(&state.db)
    .await
    .map_err(|e| store_error("Failed to rotate secret", e))?;
    if result.rows_affected() == 0 {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Secret not found",
        ));
    }

    audit(&state.db, auth, "secret_rotated", &target, &name).await;
    info!(name = %name, scope = target.scope.as_str(), actor = %auth.0.email, "rotated secret");

    let secret = find_summary(&state.db, &target, &name)
        .await?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Secret not found"))?;
    Ok(Json(SecretResponse { secret }))
}

async fn delete_for_target(
    state: &AppState,
    auth: &AuthUser,
    target: SecretTarget,
    name: String,
) -> ApiResult<OkResponse> {
    let result = sqlx::query(
        "DELETE FROM secrets WHERE scope = ?1 AND project_id IS ?2 AND pipeline_id IS ?3 AND name = ?4",
    )
        .bind(target.scope.as_str())
        .bind(&target.project_id)
        .bind(&target.pipeline_id)
        .bind(&name)
        .execute(&state.db)
        .await
        .map_err(|e| store_error("Failed to delete secret", e))?;
    if result.rows_affected() == 0 {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Secret not found",
        ));
    }

    audit(&state.db, auth, "secret_deleted", &target, &name).await;
    info!(name = %name, scope = target.scope.as_str(), actor = %auth.0.email, "deleted secret");
    Ok(Json(OkResponse { ok: true }))
}

// ── Scope resolution and authorization ──────────────────────────

async fn project_target(
    state: &AppState,
    auth: &AuthUser,
    project_id: String,
    permission: ProjectPermission,
) -> Result<SecretTarget, ApiFailure> {
    let effective = resolve_effective_project_role(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, permission)?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM projects WHERE id = ?1")
        .bind(&project_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| store_error("Failed to load project", e))?;
    if exists.is_none() {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Project not found",
        ));
    }
    Ok(SecretTarget {
        scope: SecretScope::Project,
        project_id: Some(project_id),
        pipeline_id: None,
    })
}

async fn pipeline_target(
    state: &AppState,
    auth: &AuthUser,
    pipeline_id: String,
    permission: ProjectPermission,
) -> Result<SecretTarget, ApiFailure> {
    let project_id = require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        permission,
    )
    .await?;
    Ok(SecretTarget {
        scope: SecretScope::Pipeline,
        project_id: Some(project_id),
        pipeline_id: Some(pipeline_id),
    })
}

// ── Instance handlers ───────────────────────────────────────────

/// `GET /v1/secrets` — list instance secret names.
pub async fn list_instance_secrets(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<ListSecretsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "read").await?;
    list_for_target(&state.db, &SecretTarget::instance()).await
}

/// `POST /v1/secrets` — create an instance secret.
pub async fn create_instance_secret(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateSecretRequest>,
) -> Result<(StatusCode, Json<SecretResponse>), ApiFailure> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;
    create_for_target(&state, &auth, SecretTarget::instance(), req).await
}

/// `PUT /v1/secrets/{name}` — rotate an instance secret.
pub async fn rotate_instance_secret(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(name): Path<String>,
    Json(req): Json<RotateSecretRequest>,
) -> ApiResult<SecretResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;
    rotate_for_target(&state, &auth, SecretTarget::instance(), name, req).await
}

/// `DELETE /v1/secrets/{name}` — delete an instance secret.
pub async fn delete_instance_secret(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(name): Path<String>,
) -> ApiResult<OkResponse> {
    check_permission(&state.enforcer, &auth.0.role, "instance_settings", "write").await?;
    delete_for_target(&state, &auth, SecretTarget::instance(), name).await
}

// ── Project handlers ────────────────────────────────────────────

/// `GET /v1/projects/{project_id}/secrets` — list project secret names.
pub async fn list_project_secrets(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> ApiResult<ListSecretsResponse> {
    let target = project_target(&state, &auth, project_id, ProjectPermission::Read).await?;
    list_for_target(&state.db, &target).await
}

/// `POST /v1/projects/{project_id}/secrets` — create a project secret.
pub async fn create_project_secret(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Json(req): Json<CreateSecretRequest>,
) -> Result<(StatusCode, Json<SecretResponse>), ApiFailure> {
    let target = project_target(
        &state,
        &auth,
        project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    create_for_target(&state, &auth, target, req).await
}

/// `PUT /v1/projects/{project_id}/secrets/{name}` — rotate a project secret.
pub async fn rotate_project_secret(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((project_id, name)): Path<(String, String)>,
    Json(req): Json<RotateSecretRequest>,
) -> ApiResult<SecretResponse> {
    let target = project_target(
        &state,
        &auth,
        project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    rotate_for_target(&state, &auth, target, name, req).await
}

/// `DELETE /v1/projects/{project_id}/secrets/{name}` — delete a project secret.
pub async fn delete_project_secret(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((project_id, name)): Path<(String, String)>,
) -> ApiResult<OkResponse> {
    let target = project_target(
        &state,
        &auth,
        project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    delete_for_target(&state, &auth, target, name).await
}

// ── Pipeline handlers ───────────────────────────────────────────

/// `GET /v1/pipelines/{pipeline_id}/secrets` — list pipeline secret names.
pub async fn list_pipeline_secrets(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<ListSecretsResponse> {
    let target = pipeline_target(&state, &auth, pipeline_id, ProjectPermission::Read).await?;
    list_for_target(&state.db, &target).await
}

/// `POST /v1/pipelines/{pipeline_id}/secrets` — create a pipeline secret.
pub async fn create_pipeline_secret(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
    Json(req): Json<CreateSecretRequest>,
) -> Result<(StatusCode, Json<SecretResponse>), ApiFailure> {
    let target = pipeline_target(
        &state,
        &auth,
        pipeline_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    create_for_target(&state, &auth, target, req).await
}

/// `PUT /v1/pipelines/{pipeline_id}/secrets/{name}` — rotate a pipeline secret.
pub async fn rotate_pipeline_secret(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((pipeline_id, name)): Path<(String, String)>,
    Json(req): Json<RotateSecretRequest>,
) -> ApiResult<SecretResponse> {
    let target = pipeline_target(
        &state,
        &auth,
        pipeline_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    rotate_for_target(&state, &auth, target, name, req).await
}

/// `DELETE /v1/pipelines/{pipeline_id}/secrets/{name}` — delete a pipeline secret.
pub async fn delete_pipeline_secret(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((pipeline_id, name)): Path<(String, String)>,
) -> ApiResult<OkResponse> {
    let target = pipeline_target(
        &state,
        &auth,
        pipeline_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    delete_for_target(&state, &auth, target, name).await
}

// ── Resolution for jobs ─────────────────────────────────────────

fn scope_rank(scope: SecretScope) -> u8 {
    match scope {
        SecretScope::Instance => 0,
        SecretScope::Project => 1,
        SecretScope::Pipeline => 2,
    }
}

/// Decrypt every secret visible to a pipeline. On a name clash the pipeline
/// scope wins over the project scope, which wins over the instance scope.
pub async fn resolve_secrets(
    pool: &SqlitePool,
    encryption_key: &[u8],
    project_id: &str,
    pipeline_id: Option<&str>,
) -> anyhow::Result<Vec<RunnerJobSecret>> {
    let rows = sqlx::query(
        "SELECT name, scope, value_encrypted FROM secrets \
         WHERE scope = 'instance' \
            OR (scope = 'project' AND project_id = ?1) \
            OR (scope = 'pipeline' AND pipeline_id IS ?2 AND ?2 IS NOT NULL)",
    )
    .bind(project_id)
    .bind(pipeline_id)
    .fetch_all(pool)
    .await?;

    let mut resolved: BTreeMap<String, RunnerJobSecret> = BTreeMap::new();
    for row in rows {
        let name: String = row.get("name");
        let scope = parse_scope(row.get::<String, _>("scope").as_str());
        if resolved
            .get(&name)
            .is_some_and(|existing| scope_rank(existing.scope) >= scope_rank(scope))
        {
            continue;
        }
        let encrypted: String = row.get("value_encrypted");
        let value = crypto::decrypt(&encrypted, encryption_key)?;
        resolved.insert(name.clone(), RunnerJobSecret { name, value, scope });
    }
    Ok(resolved.into_values().collect())
}

/// Decrypt one secret by name for daemon-side store calls, using the same
/// precedence as job delivery.
pub async fn resolve_secret_value(
    pool: &SqlitePool,
    encryption_key: &[u8],
    project_id: &str,
    pipeline_id: Option<&str>,
    name: &str,
) -> anyhow::Result<Option<String>> {
    Ok(
        resolve_secrets(pool, encryption_key, project_id, pipeline_id)
            .await?
            .into_iter()
            .find(|secret| secret.name == name)
            .map(|secret| secret.value),
    )
}

/// `GET /v1/runners/{runner_id}/jobs/{job_id}/secrets` — deliver resolved
/// secrets to the runner assigned to an active job.
pub async fn get_job_secrets(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
    headers: HeaderMap,
) -> ApiResult<RunnerJobSecretsResponse> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }
    let pipeline_id =
        require_active_job_signing_grant(&state.db, &job_id, &runner_id, &headers).await?;
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM pipelines WHERE id = ?1")
        .bind(&pipeline_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| store_error("Failed to load build secrets", e))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Pipeline not found"))?;

    let secrets = resolve_secrets(
        &state.db,
        &state.encryption_key,
        &project_id,
        Some(&pipeline_id),
    )
    .await
    .map_err(|e| {
        error!(error = %e, job_id = %job_id, "failed to resolve job secrets");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "decryption_error",
            "Failed to load build secrets",
        )
    })?;
    Ok(Json(RunnerJobSecretsResponse { secrets }))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE users (id TEXT PRIMARY KEY)",
            "CREATE TABLE projects (id TEXT PRIMARY KEY)",
            "CREATE TABLE pipelines (id TEXT PRIMARY KEY, project_id TEXT)",
            include_str!("../migrations/042_secrets.sql"),
            "INSERT INTO projects VALUES ('p1'), ('p2')",
            "INSERT INTO pipelines VALUES ('pl1', 'p1'), ('pl2', 'p1')",
        ] {
            sqlx::raw_sql(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    async fn insert(
        pool: &SqlitePool,
        key: &[u8],
        scope: &str,
        project: Option<&str>,
        pipeline: Option<&str>,
        name: &str,
        value: &str,
    ) {
        sqlx::query(
            "INSERT INTO secrets (id, scope, project_id, pipeline_id, name, value_encrypted, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(scope)
        .bind(project)
        .bind(pipeline)
        .bind(name)
        .bind(crypto::encrypt(value, key).unwrap())
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn narrower_scope_wins_and_other_scopes_stay_hidden() {
        let pool = pool().await;
        let key = vec![7u8; 32];
        insert(&pool, &key, "instance", None, None, "TOKEN", "instance").await;
        insert(&pool, &key, "instance", None, None, "SHARED", "shared").await;
        insert(&pool, &key, "project", Some("p1"), None, "TOKEN", "project").await;
        insert(
            &pool,
            &key,
            "pipeline",
            Some("p1"),
            Some("pl1"),
            "TOKEN",
            "pipeline",
        )
        .await;
        insert(
            &pool,
            &key,
            "pipeline",
            Some("p1"),
            Some("pl2"),
            "OTHER",
            "other",
        )
        .await;
        insert(
            &pool,
            &key,
            "project",
            Some("p2"),
            None,
            "FOREIGN",
            "foreign",
        )
        .await;

        let secrets = resolve_secrets(&pool, &key, "p1", Some("pl1"))
            .await
            .unwrap();
        let pairs: Vec<(&str, &str)> = secrets
            .iter()
            .map(|secret| (secret.name.as_str(), secret.value.as_str()))
            .collect();
        assert_eq!(pairs, vec![("SHARED", "shared"), ("TOKEN", "pipeline")]);

        let project_only = resolve_secret_value(&pool, &key, "p1", None, "TOKEN")
            .await
            .unwrap();
        assert_eq!(project_only.as_deref(), Some("project"));
    }

    #[tokio::test]
    async fn names_are_unique_per_scope_and_values_are_encrypted() {
        let pool = pool().await;
        let key = vec![7u8; 32];
        insert(
            &pool,
            &key,
            "project",
            Some("p1"),
            None,
            "TOKEN",
            "plain-value",
        )
        .await;
        let duplicate = sqlx::query(
            "INSERT INTO secrets (id, scope, project_id, name, value_encrypted, created_at, updated_at) \
             VALUES ('x', 'project', 'p1', 'TOKEN', 'v', 0, 0)",
        )
        .execute(&pool)
        .await;
        assert!(duplicate.is_err());

        let stored: String = sqlx::query_scalar("SELECT value_encrypted FROM secrets")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!stored.contains("plain-value"));
    }
}
