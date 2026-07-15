use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use oore_contract::{
    ApiError, Artifact, ArtifactDownloadLinkResponse, CompleteArtifactRequest,
    CompleteArtifactResponse, CreateArtifactRequest, CreateArtifactResponse, ListArtifactsResponse,
    ListBuildArtifactsRequest,
};
use serde::Deserialize;
use sqlx::{QueryBuilder, Row, Sqlite};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::retention::load_effective_policy;
use crate::runners::RunnerAuth;
use crate::store::write_audit_log;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Upload URL TTL: 30 minutes for uploads.
const UPLOAD_URL_TTL_SECS: u64 = 30 * 60;

/// Download URL TTL: 15 minutes for downloads.
const DOWNLOAD_URL_TTL_SECS: u64 = 15 * 60;

/// Maximum accepted local artifact upload payload size (512 MiB).
pub const MAX_LOCAL_UPLOAD_BYTES: usize = 512 * 1024 * 1024;

/// Valid artifact types.
const VALID_ARTIFACT_TYPES: &[&str] = &["apk", "ipa", "app", "generic"];

/// Keep artifact batches aligned with the maximum build-list page used by the UI.
const MAX_ARTIFACT_BUILD_IDS: usize = 200;

const MAX_PROJECT_ARTIFACT_HISTORY_LIMIT: i64 = 200;

#[derive(Debug, Deserialize)]
pub struct ListProjectArtifactsQuery {
    pub limit: Option<i64>,
}

fn is_unique_constraint(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db_err) => db_err.message().contains("UNIQUE constraint failed"),
        _ => false,
    }
}

// ── Row conversion ──────────────────────────────────────────────

fn row_to_artifact(row: &sqlx::sqlite::SqliteRow) -> Artifact {
    let metadata_str: String = row.get("metadata");
    let metadata: serde_json::Value = serde_json::from_str(&metadata_str).unwrap_or_default();

    Artifact {
        id: row.get("id"),
        build_id: row.get("build_id"),
        name: row.get("name"),
        artifact_type: row.get("artifact_type"),
        file_path: row.get("file_path"),
        file_size: row.get("file_size"),
        checksum: row.get("checksum"),
        metadata,
        created_at: row.get("created_at"),
        state: row.get("state"),
        expires_at: row.get("expires_at"),
    }
}

async fn require_project_artifact_read(
    pool: &sqlx::SqlitePool,
    auth: &AuthUser,
    project_id: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let effective = resolve_effective_project_role(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::ReadArtifacts)
}

// ── Handlers ────────────────────────────────────────────────────

/// `POST /v1/runners/{runner_id}/jobs/{job_id}/artifacts` — runner uploads an artifact.
pub async fn create_artifact(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id)): Path<(String, String)>,
    runner_auth: RunnerAuth,
    Json(req): Json<CreateArtifactRequest>,
) -> ApiResult<CreateArtifactResponse> {
    // Prevent cross-runner access
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }

    // Validate artifact_type
    if !VALID_ARTIFACT_TYPES.contains(&req.artifact_type.as_str()) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_artifact_type",
            format!(
                "artifact_type must be one of: {}",
                VALID_ARTIFACT_TYPES.join(", ")
            ),
        ));
    }

    // Validate name length
    let name = req.name.trim();
    if name.is_empty() || name.len() > 255 || name.contains('/') || name.contains('\\') {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "Artifact name must be 1-255 characters and must not contain path separators",
        ));
    }

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    // Verify build exists and is assigned to this runner
    let build_row = sqlx::query("SELECT id, runner_id, project_id FROM builds WHERE id = ?1")
        .bind(&job_id)
        .fetch_optional(&pool)
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

    let build_id: String = build_row.get("id");
    let project_id: String = build_row.get("project_id");
    let checksum = req
        .checksum
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    // Deduplicate artifacts within a build by checksum when available.
    // This avoids duplicate attachments when the same file is discovered more than once.
    if let Some(checksum_value) = &checksum
        && let Some(existing_row) = sqlx::query(
            "SELECT * FROM artifacts WHERE build_id = ?1 AND checksum = ?2 AND state = 'available' \
             ORDER BY created_at ASC LIMIT 1",
        )
        .bind(&build_id)
        .bind(checksum_value)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to check duplicate artifact checksum");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to create artifact",
            )
        })?
    {
        let artifact = row_to_artifact(&existing_row);
        info!(
            build_id = %build_id,
            checksum = %checksum_value,
            artifact_id = %artifact.id,
            "artifact deduplicated by checksum"
        );
        return Ok(Json(CreateArtifactResponse {
            artifact,
            upload_url: String::new(),
        }));
    }

    let artifact_id = Uuid::new_v4().to_string();
    let now = now_unix();

    // S3 key format: artifacts/{build_id}/{artifact_id}/{name}
    let file_path = format!("artifacts/{build_id}/{artifact_id}/{name}");

    let metadata_str = serde_json::to_string(&req.metadata).unwrap_or_else(|_| "{}".to_string());

    // Generate upload URL first — if this fails we avoid leaving an orphan DB row.
    let upload_url = {
        let storage = state.storage.read().await;
        storage
            .generate_upload_url(&file_path, UPLOAD_URL_TTL_SECS)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to generate upload URL");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_error",
                    "Failed to generate upload URL",
                )
            })?
            .unwrap_or_default()
    };

    // Compute artifact expiry from effective retention policy (project override > global)
    let expires_at: Option<i64> = match load_effective_policy(&pool, &project_id).await {
        Ok(policy) => policy.artifact_ttl_days.map(|days| now + days * 86400),
        Err(e) => {
            error!(error = %e, "failed to load retention policy for artifact TTL; defaulting to no expiry");
            None
        }
    };

    // Insert artifact row only after URL generation succeeds
    let insert_result = sqlx::query(
        "INSERT INTO artifacts (id, build_id, name, artifact_type, file_path, file_size, checksum, metadata, created_at, expires_at, state) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending')",
    )
    .bind(&artifact_id)
    .bind(&build_id)
    .bind(name)
    .bind(&req.artifact_type)
    .bind(&file_path)
    .bind(req.file_size)
    .bind(&checksum)
    .bind(&metadata_str)
    .bind(now)
    .bind(expires_at)
    .execute(&pool)
    .await;

    if let Err(e) = insert_result {
        // If another request inserted the same checksum concurrently, return the existing artifact.
        if is_unique_constraint(&e)
            && let Some(checksum_value) = &checksum
            && let Some(existing_row) = sqlx::query(
                "SELECT * FROM artifacts WHERE build_id = ?1 AND checksum = ?2 AND state = 'available' \
                 ORDER BY created_at ASC LIMIT 1",
            )
            .bind(&build_id)
            .bind(checksum_value)
            .fetch_optional(&pool)
            .await
            .map_err(|lookup_err| {
                error!(error = %lookup_err, "failed to fetch deduplicated artifact");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Failed to create artifact",
                )
            })?
        {
            let artifact = row_to_artifact(&existing_row);
            info!(
                build_id = %build_id,
                checksum = %checksum_value,
                artifact_id = %artifact.id,
                "artifact deduplicated by checksum after concurrent insert"
            );
            return Ok(Json(CreateArtifactResponse {
                artifact,
                upload_url: String::new(),
            }));
        }

        error!(error = %e, "failed to insert artifact");
        return Err(api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to create artifact",
        ));
    }

    info!(
        artifact_id = %artifact_id,
        build_id = %build_id,
        name = %name,
        artifact_type = %req.artifact_type,
        "artifact created"
    );

    let artifact = Artifact {
        id: artifact_id,
        build_id,
        name: name.to_string(),
        artifact_type: req.artifact_type,
        file_path,
        file_size: req.file_size,
        checksum,
        metadata: req.metadata,
        created_at: now,
        state: "pending".to_string(),
        expires_at,
    };

    Ok(Json(CreateArtifactResponse {
        artifact,
        upload_url,
    }))
}

async fn finish_artifact(
    state: Arc<AppState>,
    runner_id: String,
    job_id: String,
    artifact_id: String,
    runner_auth: RunnerAuth,
    available: bool,
    error_message: Option<String>,
) -> ApiResult<CompleteArtifactResponse> {
    if runner_auth.runner_id != runner_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "runner_mismatch",
            "Runner token does not match the requested runner ID",
        ));
    }
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    let now = now_unix();
    let new_state = if available { "available" } else { "failed" };
    let result = sqlx::query(
        "UPDATE artifacts SET state = ?1, finalized_at = ?2, error_message = ?3 \
         WHERE id = ?4 AND build_id = ?5 AND state = 'pending' \
         AND EXISTS (SELECT 1 FROM builds WHERE id = ?5 AND runner_id = ?6)",
    )
    .bind(new_state)
    .bind(now)
    .bind(error_message.as_deref())
    .bind(&artifact_id)
    .bind(&job_id)
    .bind(&runner_id)
    .execute(&pool)
    .await
    .map_err(|error| {
        error!(error = %error, artifact_id = %artifact_id, "failed to finalize artifact");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to finalize artifact",
        )
    })?;
    if result.rows_affected() == 0 {
        return Err(api_err(
            StatusCode::CONFLICT,
            "artifact_not_pending",
            "Artifact is not pending or does not belong to this runner job",
        ));
    }
    let row = sqlx::query("SELECT * FROM artifacts WHERE id = ?1")
        .bind(&artifact_id)
        .fetch_one(&pool)
        .await
        .map_err(|error| {
            error!(error = %error, artifact_id = %artifact_id, "failed to load finalized artifact");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load artifact",
            )
        })?;
    Ok(Json(CompleteArtifactResponse {
        artifact: row_to_artifact(&row),
    }))
}

pub async fn complete_artifact(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id, artifact_id)): Path<(String, String, String)>,
    runner_auth: RunnerAuth,
    Json(req): Json<CompleteArtifactRequest>,
) -> ApiResult<CompleteArtifactResponse> {
    finish_artifact(
        state,
        runner_id,
        job_id,
        artifact_id,
        runner_auth,
        true,
        req.error_message,
    )
    .await
}

pub async fn abort_artifact(
    State(state): State<Arc<AppState>>,
    Path((runner_id, job_id, artifact_id)): Path<(String, String, String)>,
    runner_auth: RunnerAuth,
    Json(req): Json<CompleteArtifactRequest>,
) -> ApiResult<CompleteArtifactResponse> {
    finish_artifact(
        state,
        runner_id,
        job_id,
        artifact_id,
        runner_auth,
        false,
        req.error_message,
    )
    .await
}

/// `GET /v1/builds/{build_id}/artifacts` — list artifacts for a build.
pub async fn list_artifacts(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> ApiResult<ListArtifactsResponse> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    let project_id: String = sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load build for artifact listing");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to list artifacts",
            )
        })?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;
    require_project_artifact_read(&pool, &auth, &project_id).await?;

    let rows = sqlx::query("SELECT * FROM artifacts WHERE build_id = ?1 AND state = 'available' ORDER BY created_at ASC")
        .bind(&build_id)
        .fetch_all(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to list artifacts");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to list artifacts",
            )
        })?;

    let artifacts = rows.iter().map(row_to_artifact).collect();

    Ok(Json(ListArtifactsResponse { artifacts }))
}

/// `GET /v1/projects/{project_id}/artifacts` — list available artifacts for a project.
pub async fn list_project_artifacts(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Query(params): Query<ListProjectArtifactsQuery>,
) -> ApiResult<ListArtifactsResponse> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    require_project_artifact_read(&pool, &auth, &project_id).await?;

    if params.limit.is_some_and(|limit| limit <= 0) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "limit must be greater than zero",
        ));
    }
    let limit = params
        .limit
        .map(|limit| limit.min(MAX_PROJECT_ARTIFACT_HISTORY_LIMIT))
        .unwrap_or(-1);

    let rows = sqlx::query(
        "SELECT a.* FROM artifacts a \
         JOIN builds b ON b.id = a.build_id \
         WHERE b.project_id = ?1 AND a.state = 'available' \
         ORDER BY a.created_at DESC, a.id DESC LIMIT ?2",
    )
    .bind(&project_id)
    .bind(limit)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to list project artifacts");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to list project artifacts",
        )
    })?;

    let artifacts = rows.iter().map(row_to_artifact).collect();
    Ok(Json(ListArtifactsResponse { artifacts }))
}

/// `POST /v1/artifacts/query` — list available artifacts for a bounded build set.
pub async fn list_build_artifacts(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<ListBuildArtifactsRequest>,
) -> ApiResult<ListArtifactsResponse> {
    if req.build_ids.len() > MAX_ARTIFACT_BUILD_IDS {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "too_many_build_ids",
            format!("At most {MAX_ARTIFACT_BUILD_IDS} build IDs may be requested"),
        ));
    }

    let mut build_ids = req
        .build_ids
        .into_iter()
        .filter(|id| !id.trim().is_empty())
        .collect::<Vec<_>>();
    build_ids.sort_unstable();
    build_ids.dedup();
    if build_ids.is_empty() {
        return Ok(Json(ListArtifactsResponse { artifacts: vec![] }));
    }

    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };
    let is_instance_admin = auth.0.role == "owner" || auth.0.role == "admin";

    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT a.* FROM artifacts a JOIN builds b ON b.id = a.build_id \
         WHERE a.state = 'available' AND b.id IN (",
    );
    {
        let mut ids = query.separated(", ");
        for build_id in &build_ids {
            ids.push_bind(build_id);
        }
    }
    query.push(")");

    // Every project role can read artifacts. Instance owners/admins have implicit
    // access; all other identities must still have an explicit project membership.
    if !is_instance_admin {
        query.push(
            " AND EXISTS (SELECT 1 FROM project_members pm \
             WHERE pm.project_id = b.project_id AND pm.user_id = ",
        );
        query.push_bind(&auth.0.user_id);
        query.push(")");
    }
    query.push(" ORDER BY a.created_at DESC");

    let rows = query.build().fetch_all(&pool).await.map_err(|e| {
        error!(error = %e, "failed to list artifacts for build batch");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to list artifacts",
        )
    })?;

    let artifacts = rows.iter().map(row_to_artifact).collect();
    Ok(Json(ListArtifactsResponse { artifacts }))
}

/// `POST /v1/artifacts/{artifact_id}/download-link` — generate a download link.
pub async fn generate_download_link(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(artifact_id): Path<String>,
) -> ApiResult<ArtifactDownloadLinkResponse> {
    let pool = {
        let store = state.store.lock().await;
        store.pool().clone()
    };

    // Look up artifact
    let row = sqlx::query(
        "SELECT a.*, b.project_id AS project_id \
         FROM artifacts a \
         JOIN builds b ON b.id = a.build_id \
         WHERE a.id = ?1 AND a.state = 'available'",
    )
    .bind(&artifact_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to fetch artifact");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to fetch artifact",
        )
    })?
    .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Artifact not found"))?;

    let project_id: String = row.get("project_id");
    require_project_artifact_read(&pool, &auth, &project_id).await?;

    // Reject downloads of expired artifacts
    let artifact_expires_at: Option<i64> = row.get("expires_at");
    if let Some(ea) = artifact_expires_at
        && ea <= now_unix()
    {
        return Err(api_err(
            StatusCode::GONE,
            "artifact_expired",
            "This artifact has expired",
        ));
    }

    let file_path: String = row.get("file_path");

    let download_url = {
        let storage = state.storage.read().await;
        storage
            .generate_download_url(&file_path, DOWNLOAD_URL_TTL_SECS)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to generate download URL");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_error",
                    "Failed to generate download URL",
                )
            })?
            .ok_or_else(|| {
                api_err(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "storage_not_configured",
                    "Artifact storage backend is not configured.",
                )
            })?
    };

    let now = now_unix();
    let expires_at = now + DOWNLOAD_URL_TTL_SECS as i64;

    // Audit log the download link generation
    let details = serde_json::json!({
        "artifact_id": artifact_id,
        "file_path": file_path,
        "expires_at": expires_at,
    })
    .to_string();
    let _ = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "artifact_download_link_generated",
        "artifact",
        Some(&artifact_id),
        Some(&details),
    )
    .await;

    info!(
        artifact_id = %artifact_id,
        user = %auth.0.email,
        "download link generated"
    );

    Ok(Json(ArtifactDownloadLinkResponse {
        download_url,
        expires_at,
    }))
}

/// `PUT /v1/artifacts/local-upload/{token}` — signed local artifact upload endpoint.
///
/// Used only when local artifact storage backend is active.
pub async fn upload_local_artifact(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if body.len() > MAX_LOCAL_UPLOAD_BYTES {
        return Err(api_err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            format!("Upload exceeds {} bytes", MAX_LOCAL_UPLOAD_BYTES),
        ));
    }

    let stored = {
        let storage = state.storage.read().await;
        storage
            .handle_local_upload(&token, &body)
            .await
            .map_err(|e| {
                error!(error = %e, "failed local artifact upload");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_error",
                    "Failed to store artifact",
                )
            })?
    };

    if !stored {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_upload_token",
            "Upload token is invalid or expired",
        ));
    }

    Ok(StatusCode::OK)
}

/// `GET /v1/artifacts/local-download/{token}` — signed local artifact download endpoint.
///
/// Used only when local artifact storage backend is active.
pub async fn download_local_artifact(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let payload = {
        let storage = state.storage.read().await;
        storage.handle_local_download(&token).await.map_err(|e| {
            error!(error = %e, "failed local artifact download");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "Failed to load artifact",
            )
        })?
    };

    let Some(payload) = payload else {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Artifact not found or download token expired",
        ));
    };

    let disposition = format!("attachment; filename=\"{}\"", payload.file_name);
    let mut response = (StatusCode::OK, payload.bytes).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}
