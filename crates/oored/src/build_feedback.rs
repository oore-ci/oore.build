//! Feedback testers send about a build from the QA portal.
//!
//! Anyone who can install a build's artifacts can file a note against it. The
//! note lands on the build's timeline and goes to the project's notification
//! channels as a `qa_feedback` event.

use std::sync::Arc;

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use base64::Engine as _;
use oore_contract::{
    ApiError, BUILD_FEEDBACK_MAX_FIELD_CHARS, BUILD_FEEDBACK_MAX_MESSAGE_CHARS,
    BUILD_FEEDBACK_MAX_SCREENSHOT_BYTES, BuildFeedback, CreateBuildFeedbackRequest,
    ListBuildFeedbackResponse,
};
use sqlx::{Row, SqlitePool};
use tracing::{error, warn};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::util::{api_err, now_unix};

type ApiFailure = (StatusCode, Json<ApiError>);

/// Request body limit for the create route: a base64 screenshot plus text.
pub const MAX_FEEDBACK_REQUEST_BYTES: usize =
    BUILD_FEEDBACK_MAX_SCREENSHOT_BYTES.div_ceil(3) * 4 + 256 * 1024;
/// Notes one person may send in `RATE_WINDOW_SECS`, across all builds.
const RATE_LIMIT: i64 = 10;
const RATE_WINDOW_SECS: i64 = 10 * 60;

fn store_error(message: &'static str, error: sqlx::Error) -> ApiFailure {
    error!(error = %error, "{message}");
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", message)
}

fn invalid(message: impl Into<String>) -> ApiFailure {
    api_err(StatusCode::BAD_REQUEST, "invalid_input", message)
}

/// Trim, drop control characters and enforce a character limit. Line breaks
/// and tabs are kept when `multiline`, otherwise they become spaces. Empty
/// becomes `None`.
fn clean_text(
    value: Option<&str>,
    field: &str,
    max_chars: usize,
    multiline: bool,
) -> Result<Option<String>, ApiFailure> {
    let Some(value) = value else {
        return Ok(None);
    };
    let cleaned: String = value
        .replace("\r\n", "\n")
        .chars()
        .filter_map(|c| match c {
            '\n' | '\t' if multiline => Some(c),
            '\n' | '\t' | '\r' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return Ok(None);
    }
    if cleaned.chars().count() > max_chars {
        return Err(invalid(format!(
            "{field} must be at most {max_chars} characters"
        )));
    }
    Ok(Some(cleaned.to_string()))
}

/// Image type from the file's leading bytes. Only raster formats a browser
/// can show are accepted; SVG and anything else is refused.
pub(crate) fn sniff_image(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

fn decode_screenshot(encoded: &str) -> Result<Vec<u8>, ApiFailure> {
    let encoded = encoded.trim();
    if encoded.len() > BUILD_FEEDBACK_MAX_SCREENSHOT_BYTES.div_ceil(3) * 4 {
        return Err(too_large());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| invalid("screenshot_base64 is not valid base64"))?;
    if bytes.is_empty() {
        return Err(invalid("screenshot_base64 is empty"));
    }
    if bytes.len() > BUILD_FEEDBACK_MAX_SCREENSHOT_BYTES {
        return Err(too_large());
    }
    Ok(bytes)
}

fn too_large() -> ApiFailure {
    api_err(
        StatusCode::PAYLOAD_TOO_LARGE,
        "screenshot_too_large",
        "The screenshot must be 5 MiB or smaller",
    )
}

/// The build's project, after checking `permission` on it. 404 when the
/// build is missing or the caller cannot see its project.
async fn authorize_build(
    pool: &SqlitePool,
    auth: &AuthUser,
    build_id: &str,
    permission: ProjectPermission,
) -> Result<String, ApiFailure> {
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(build_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| store_error("Failed to fetch build", e))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;
    let effective = resolve_effective_project_role(
        pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, permission).map_err(|(status, body)| {
        if status == StatusCode::NOT_FOUND {
            api_err(StatusCode::NOT_FOUND, "not_found", "Build not found")
        } else {
            (status, body)
        }
    })?;
    Ok(project_id)
}

fn row_to_feedback(row: &sqlx::sqlite::SqliteRow) -> BuildFeedback {
    BuildFeedback {
        id: row.get("id"),
        build_id: row.get("build_id"),
        author_email: row.get("author_email"),
        author_name: row.get("author_name"),
        message: row.get("message"),
        device: row.get("device"),
        os_version: row.get("os_version"),
        app_version: row.get("app_version"),
        artifact_id: row.get("artifact_id"),
        has_screenshot: row.get::<Option<String>, _>("screenshot_key").is_some(),
        created_at: row.get("created_at"),
    }
}

/// `SELECT ... FROM build_feedback f` with the author's display name.
macro_rules! select_feedback {
    ($rest:literal) => {
        concat!(
            "SELECT f.id, f.build_id, f.author_email, \
             NULLIF(TRIM(u.display_name), '') AS author_name, f.message, f.device, \
             f.os_version, f.app_version, f.artifact_id, f.screenshot_key, f.created_at \
             FROM build_feedback f LEFT JOIN users u ON u.id = f.author_user_id ",
            $rest
        )
    };
}

pub(crate) async fn load_feedback(
    pool: &SqlitePool,
    feedback_id: &str,
) -> Result<Option<BuildFeedback>, sqlx::Error> {
    let row = sqlx::query(select_feedback!("WHERE f.id = ?1"))
        .bind(feedback_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.as_ref().map(row_to_feedback))
}

/// Storage keys of feedback screenshots on one build. Feedback rows are
/// deleted with their build, so callers collect these first and remove the
/// objects once the build is gone.
pub(crate) async fn screenshot_keys_for_build<'e, E>(
    executor: E,
    build_id: &str,
) -> Result<Vec<String>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query_scalar(
        "SELECT screenshot_key FROM build_feedback \
         WHERE build_id = ?1 AND screenshot_key IS NOT NULL",
    )
    .bind(build_id)
    .fetch_all(executor)
    .await
}

/// Storage keys of feedback screenshots on a pipeline's finished builds.
pub(crate) async fn screenshot_keys_for_pipeline<'e, E>(
    executor: E,
    pipeline_id: &str,
) -> Result<Vec<String>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query_scalar(
        "SELECT f.screenshot_key FROM build_feedback f JOIN builds b ON b.id = f.build_id \
         WHERE b.pipeline_id = ?1 AND f.screenshot_key IS NOT NULL \
         AND b.status IN ('succeeded', 'failed', 'canceled', 'timed_out', 'expired')",
    )
    .bind(pipeline_id)
    .fetch_all(executor)
    .await
}

/// Storage keys of feedback screenshots on a project's finished builds.
pub(crate) async fn screenshot_keys_for_project<'e, E>(
    executor: E,
    project_id: &str,
) -> Result<Vec<String>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query_scalar(
        "SELECT f.screenshot_key FROM build_feedback f JOIN builds b ON b.id = f.build_id \
         WHERE b.project_id = ?1 AND f.screenshot_key IS NOT NULL \
         AND b.status IN ('succeeded', 'failed', 'canceled', 'timed_out', 'expired')",
    )
    .bind(project_id)
    .fetch_all(executor)
    .await
}

/// Remove screenshot objects whose feedback rows are gone, best effort: a
/// stray screenshot is only reachable through a feedback row, so a failure
/// is logged and left.
pub(crate) async fn delete_screenshots(storage: &crate::storage::StorageBackend, keys: &[String]) {
    for key in keys {
        if let Err(e) = storage.delete_object(key).await {
            warn!(error = %e, %key, "failed to delete a feedback screenshot");
        }
    }
}

/// `POST /v1/builds/{build_id}/feedback`
pub async fn create_build_feedback(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<BuildFeedback>), ApiFailure> {
    let pool = &state.db;
    // Same access as installing the build from the QA portal. Axum has
    // already buffered the body (within the route's limit) by now; the access
    // check and rate limit gate parsing it and decoding a screenshot.
    let project_id =
        authorize_build(pool, &auth, &build_id, ProjectPermission::ReadArtifacts).await?;

    // Rate limit before any parsing or decoding, so rejected attempts are
    // cheap too.
    let now = now_unix();
    let recent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM build_feedback WHERE author_user_id = ?1 AND created_at > ?2",
    )
    .bind(&auth.0.user_id)
    .bind(now - RATE_WINDOW_SECS)
    .fetch_one(pool)
    .await
    .map_err(|e| store_error("Failed to check recent feedback", e))?;
    if recent >= RATE_LIMIT {
        return Err(api_err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "You have sent a lot of feedback in the last few minutes. Try again shortly.",
        ));
    }

    let request: CreateBuildFeedbackRequest =
        serde_json::from_slice(&body).map_err(|e| invalid(format!("Invalid request body: {e}")))?;
    drop(body);

    let message = clean_text(
        Some(&request.message),
        "message",
        BUILD_FEEDBACK_MAX_MESSAGE_CHARS,
        true,
    )?
    .ok_or_else(|| invalid("message is required"))?;
    let device = clean_text(
        request.device.as_deref(),
        "device",
        BUILD_FEEDBACK_MAX_FIELD_CHARS,
        false,
    )?;
    let os_version = clean_text(
        request.os_version.as_deref(),
        "os_version",
        BUILD_FEEDBACK_MAX_FIELD_CHARS,
        false,
    )?;
    let app_version = clean_text(
        request.app_version.as_deref(),
        "app_version",
        BUILD_FEEDBACK_MAX_FIELD_CHARS,
        false,
    )?;

    let artifact_id = match request.artifact_id.as_deref().map(str::trim) {
        Some(artifact_id) if !artifact_id.is_empty() => {
            let belongs: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM artifacts WHERE id = ?1 AND build_id = ?2")
                    .bind(artifact_id)
                    .bind(&build_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| store_error("Failed to check the artifact", e))?;
            if belongs.is_none() {
                return Err(invalid("artifact_id does not belong to this build"));
            }
            Some(artifact_id.to_string())
        }
        _ => None,
    };

    let screenshot = match request.screenshot_base64.as_deref() {
        Some(encoded) if !encoded.trim().is_empty() => {
            let bytes = decode_screenshot(encoded)?;
            let (content_type, extension) = sniff_image(&bytes).ok_or_else(|| {
                api_err(
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "unsupported_screenshot",
                    "The screenshot must be a PNG, JPEG or WebP image",
                )
            })?;
            Some((bytes, content_type, extension))
        }
        _ => None,
    };

    let feedback_id = Uuid::new_v4().to_string();
    let (screenshot_key, screenshot_content_type) = match screenshot {
        Some((bytes, content_type, extension)) => {
            let key = format!("feedback/{project_id}/{build_id}/{feedback_id}.{extension}");
            let storage = state.storage.read().await.clone();
            if matches!(storage, crate::storage::StorageBackend::Disabled) {
                return Err(api_err(
                    StatusCode::CONFLICT,
                    "storage_unavailable",
                    "Screenshots need artifact storage. Send the note without one, or ask an admin to set up storage.",
                ));
            }
            storage
                .put_object(&key, bytes, content_type)
                .await
                .map_err(|e| {
                    error!(error = %e, %build_id, "failed to store feedback screenshot");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "storage_error",
                        "Failed to store the screenshot",
                    )
                })?;
            (Some(key), Some(content_type))
        }
        None => (None, None),
    };

    let inserted = sqlx::query(
        "INSERT INTO build_feedback \
         (id, build_id, author_user_id, author_email, message, device, os_version, app_version, \
          artifact_id, screenshot_key, screenshot_content_type, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )
    .bind(&feedback_id)
    .bind(&build_id)
    .bind(&auth.0.user_id)
    .bind(&auth.0.email)
    .bind(&message)
    .bind(&device)
    .bind(&os_version)
    .bind(&app_version)
    .bind(&artifact_id)
    .bind(&screenshot_key)
    .bind(screenshot_content_type)
    .bind(now)
    .execute(pool)
    .await;
    if let Err(e) = inserted {
        if let Some(key) = &screenshot_key {
            let storage = state.storage.read().await.clone();
            if let Err(cleanup) = storage.delete_object(key).await {
                warn!(error = %cleanup, %key, "failed to remove an orphaned feedback screenshot");
            }
        }
        return Err(store_error("Failed to save feedback", e));
    }

    let feedback = load_feedback(pool, &feedback_id)
        .await
        .map_err(|e| store_error("Failed to load feedback", e))?
        .ok_or_else(|| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load feedback",
            )
        })?;

    if let Err(e) = crate::notification_dispatch::dispatch_build_feedback(
        pool,
        &state.encryption_key,
        &feedback,
    )
    .await
    {
        error!(error = %e, %build_id, "failed to queue feedback notification");
    }

    Ok((StatusCode::CREATED, Json(feedback)))
}

/// `GET /v1/builds/{build_id}/feedback`
pub async fn list_build_feedback(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> Result<Json<ListBuildFeedbackResponse>, ApiFailure> {
    let pool = &state.db;
    authorize_build(pool, &auth, &build_id, ProjectPermission::Read).await?;
    let rows = sqlx::query(select_feedback!(
        "WHERE f.build_id = ?1 ORDER BY f.created_at, f.id"
    ))
    .bind(&build_id)
    .fetch_all(pool)
    .await
    .map_err(|e| store_error("Failed to list feedback", e))?;
    Ok(Json(ListBuildFeedbackResponse {
        feedback: rows.iter().map(row_to_feedback).collect(),
    }))
}

/// `GET /v1/builds/{build_id}/feedback/{feedback_id}/screenshot`
pub async fn get_build_feedback_screenshot(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((build_id, feedback_id)): Path<(String, String)>,
) -> Result<Response, ApiFailure> {
    let pool = &state.db;
    authorize_build(pool, &auth, &build_id, ProjectPermission::Read).await?;
    let not_found = || api_err(StatusCode::NOT_FOUND, "not_found", "Screenshot not found");
    let row = sqlx::query(
        "SELECT screenshot_key FROM build_feedback \
         WHERE id = ?1 AND build_id = ?2 AND screenshot_key IS NOT NULL",
    )
    .bind(&feedback_id)
    .bind(&build_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| store_error("Failed to load feedback", e))?
    .ok_or_else(not_found)?;
    let key: String = row.get("screenshot_key");

    let storage = state.storage.read().await.clone();
    let bytes = storage
        .get_object(&key, BUILD_FEEDBACK_MAX_SCREENSHOT_BYTES)
        .await
        .map_err(|e| {
            error!(error = %e, %key, "failed to read feedback screenshot");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                "Failed to read the screenshot",
            )
        })?
        .ok_or_else(not_found)?;
    // Sniff again: the stored object is served with the type of its bytes,
    // never a type stored alongside it.
    let (content_type, extension) = sniff_image(&bytes).ok_or_else(not_found)?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"feedback-{feedback_id}.{extension}\""),
        )
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; sandbox",
        )
        .header(header::CACHE_CONTROL, "private, no-store")
        .body(Body::from(bytes))
        .map_err(|e| {
            error!(error = %e, "failed to build screenshot response");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "response_error",
                "Failed to send the screenshot",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_only_raster_images() {
        assert_eq!(
            sniff_image(b"\x89PNG\r\n\x1a\nrest"),
            Some(("image/png", "png"))
        );
        assert_eq!(
            sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(("image/jpeg", "jpg"))
        );
        assert_eq!(
            sniff_image(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(("image/webp", "webp"))
        );
        assert_eq!(
            sniff_image(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
            None
        );
        assert_eq!(sniff_image(b"<html><script>"), None);
        assert_eq!(sniff_image(b"GIF89a"), None);
    }

    #[test]
    fn cleans_text_fields() {
        assert_eq!(
            clean_text(Some("  hi\r\nthere\u{0007} "), "message", 100, true).unwrap(),
            Some("hi\nthere".to_string())
        );
        assert_eq!(
            clean_text(Some("Pixel\n8"), "device", 100, false).unwrap(),
            Some("Pixel 8".to_string())
        );
        assert_eq!(clean_text(Some("   "), "device", 100, false).unwrap(), None);
        assert!(clean_text(Some("abcd"), "device", 3, false).is_err());
    }

    #[test]
    fn rejects_oversized_screenshots_before_decoding() {
        let encoded = "A".repeat(BUILD_FEEDBACK_MAX_SCREENSHOT_BYTES.div_ceil(3) * 4 + 4);
        assert_eq!(
            decode_screenshot(&encoded).unwrap_err().0,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            decode_screenshot("not base64!").unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
    }

    type Received = Arc<std::sync::Mutex<Vec<serde_json::Value>>>;

    /// Starts the daemon router on a temporary database with local storage,
    /// a QA viewer on project p1, a developer outside it, and a Slack
    /// channel for p1 that only takes feedback.
    async fn daemon() -> (
        String,
        String,
        String,
        Received,
        sqlx::SqlitePool,
        tempfile::TempDir,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::store::SetupStore::connect(dir.path().join("data/oore.db"))
            .await
            .unwrap();
        let pool = store.pool().clone();
        let key = vec![7u8; 32];

        let received: Received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let capture = axum::Router::new()
            .route(
                "/hook",
                axum::routing::post(
                    |axum::extract::State(received): axum::extract::State<Received>,
                     Json(body): Json<serde_json::Value>| async move {
                        received.lock().unwrap().push(body);
                        "ok"
                    },
                ),
            )
            .with_state(received.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let hook = format!(
            "http://localhost:{}/hook",
            listener.local_addr().unwrap().port()
        );
        tokio::spawn(async move { axum::serve(listener, capture).await.unwrap() });

        let storage_dir = dir.path().join("artifacts");
        for (statement, binds) in [
            (
                "INSERT INTO users (id, email, oidc_subject, display_name, role, created_at, updated_at) \
                 VALUES ('qa', 'qa@example.com', 'qa', 'Quinn <QA>', 'qa_viewer', 0, 0), \
                        ('dev', 'dev@example.com', 'dev', NULL, 'developer', 0, 0)",
                vec![],
            ),
            (
                "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
                 VALUES ('p1', 'Kite', 'qa', 0, 0)",
                vec![],
            ),
            (
                "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
                 VALUES ('pl1', 'p1', 'Android', 0, 0)",
                vec![],
            ),
            (
                "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
                   queued_at, created_at, updated_at, notified_status) \
                 VALUES ('b1', 'p1', 'pl1', 12, 'succeeded', 'manual', 0, 0, 0, 'succeeded'), \
                        ('b2', 'p1', 'pl1', 13, 'succeeded', 'manual', 0, 0, 0, 'succeeded')",
                vec![],
            ),
            (
                // A push build whose committer is mapped to a Slack member.
                "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
                   trigger_actor, queued_at, created_at, updated_at, notified_status) \
                 VALUES ('b3', 'p1', 'pl1', 14, 'succeeded', 'webhook', 'octocat', 0, 0, 0, 'succeeded')",
                vec![],
            ),
            (
                "INSERT INTO artifacts (id, build_id, name, file_path, created_at) \
                 VALUES ('a1', 'b1', 'app.apk', 'x/app.apk', 0), ('a2', 'b2', 'app.apk', 'y/app.apk', 0)",
                vec![],
            ),
            (
                "INSERT INTO project_members (id, project_id, user_id, role, created_by, created_at, updated_at) \
                 VALUES ('m1', 'p1', 'qa', 'viewer', 'qa', 0, 0)",
                vec![],
            ),
            (
                "INSERT INTO artifact_storage_settings (id, provider, local_base_dir, created_at, updated_at) \
                 VALUES (1, 'local', ?1, 0, 0)",
                vec![storage_dir.to_string_lossy().to_string()],
            ),
            (
                "INSERT INTO notification_channels \
                 (id, name, channel_type, enabled, event_filter_json, project_id, encrypted_url, \
                  slack_mentions_json, created_at, updated_at) \
                 VALUES ('kite', 'kite', 'slack', 1, '[\"qa_feedback\"]', 'p1', ?1, \
                  '[{\"identity\":\"octocat\",\"member_id\":\"U0OCTO\"}]', 0, 0)",
                vec![crate::crypto::encrypt(&hook, &key).unwrap()],
            ),
        ] {
            let mut query = sqlx::query(statement);
            for bind in binds {
                query = query.bind(bind);
            }
            query.execute(&pool).await.unwrap();
        }
        let sessions = crate::session::SessionStore::new(pool.clone());
        let qa = sessions.create_session("qa", 3600).await.unwrap();
        let dev = sessions.create_session("dev", 3600).await.unwrap();

        let metrics = crate::observability::metrics_builder()
            .build_recorder()
            .handle();
        let router = crate::build_router(store, key, metrics).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap()
        });
        (base, qa, dev, received, pool, dir)
    }

    fn png() -> String {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&[0u8; 64]);
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[tokio::test]
    async fn testers_send_feedback_that_reaches_the_timeline_and_slack() {
        let (base, qa, dev, received, pool, _dir) = daemon().await;
        let client = reqwest::Client::new();
        let url = format!("{base}/v1/builds/b1/feedback");
        let body = serde_json::json!({
            "message": "Checkout <!channel> crashes *every* time",
            "device": "Pixel 8",
            "os_version": "Android 15",
            "app_version": "2.4.0+118",
            "artifact_id": "a1",
            "screenshot_base64": png(),
        });

        let anonymous = client.post(&url).json(&body).send().await.unwrap();
        assert_eq!(anonymous.status(), 401);
        let outsider = client
            .post(&url)
            .bearer_auth(&dev)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(outsider.status(), 404);
        // The body is not parsed before access is checked.
        let outsider_garbage = client
            .post(&url)
            .bearer_auth(&dev)
            .header("content-type", "application/json")
            .body("{not json")
            .send()
            .await
            .unwrap();
        assert_eq!(outsider_garbage.status(), 404);
        let garbage = client
            .post(&url)
            .bearer_auth(&qa)
            .header("content-type", "application/json")
            .body("{not json")
            .send()
            .await
            .unwrap();
        assert_eq!(garbage.status(), 400);
        let outsider_list = client.get(&url).bearer_auth(&dev).send().await.unwrap();
        assert_eq!(outsider_list.status(), 404);

        let mut wrong_artifact = body.clone();
        wrong_artifact["artifact_id"] = "a2".into();
        let response = client
            .post(&url)
            .bearer_auth(&qa)
            .json(&wrong_artifact)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);

        let mut svg = body.clone();
        svg["screenshot_base64"] = base64::engine::general_purpose::STANDARD
            .encode(b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>")
            .into();
        let response = client
            .post(&url)
            .bearer_auth(&qa)
            .json(&svg)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 415);

        let created = client
            .post(&url)
            .bearer_auth(&qa)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(created.status(), 201);
        let created: BuildFeedback = created.json().await.unwrap();
        assert_eq!(created.author_email, "qa@example.com");
        assert_eq!(created.author_name.as_deref(), Some("Quinn <QA>"));
        assert_eq!(created.device.as_deref(), Some("Pixel 8"));
        assert!(created.has_screenshot);

        let list: ListBuildFeedbackResponse = client
            .get(&url)
            .bearer_auth(&qa)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(list.feedback, vec![created.clone()]);

        let screenshot_url = format!("{url}/{}/screenshot", created.id);
        let outsider_screenshot = client
            .get(&screenshot_url)
            .bearer_auth(&dev)
            .send()
            .await
            .unwrap();
        assert_eq!(outsider_screenshot.status(), 404);
        let screenshot = client
            .get(&screenshot_url)
            .bearer_auth(&qa)
            .send()
            .await
            .unwrap();
        assert_eq!(screenshot.status(), 200);
        assert_eq!(screenshot.headers()["content-type"], "image/png");
        assert_eq!(screenshot.headers()["x-content-type-options"], "nosniff");
        assert_eq!(screenshot.headers()["cache-control"], "private, no-store");
        assert!(
            screenshot.headers()["content-disposition"]
                .to_str()
                .unwrap()
                .starts_with("inline; filename=\"feedback-")
        );
        let bytes = screenshot.bytes().await.unwrap();
        assert_eq!(
            base64::engine::general_purpose::STANDARD.encode(&bytes),
            png()
        );

        let mut message = None;
        for _ in 0..100 {
            if let Some(body) = received.lock().unwrap().first().cloned() {
                message = Some(body);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let message = message.expect("feedback reached the project's Slack channel");
        let blocks = message["blocks"].as_array().unwrap();
        assert_eq!(
            blocks[0]["text"]["text"],
            ":speech_balloon: *Kite* build #12: feedback from Quinn &lt;QA&gt;"
        );
        assert_eq!(blocks[1]["text"]["type"], "plain_text");
        assert_eq!(
            blocks[1]["text"]["text"],
            "Checkout <!channel> crashes *every* time"
        );
        assert_eq!(
            blocks[2]["elements"][0]["text"],
            "Pixel 8  ·  Android 15  ·  App 2.4.0+118  ·  Screenshot attached"
        );

        // Feedback on a push build does not mention the committer, even
        // though the channel maps them to a Slack member.
        let push_build = client
            .post(format!("{base}/v1/builds/b3/feedback"))
            .bearer_auth(&qa)
            .json(&serde_json::json!({ "message": "Login button does nothing" }))
            .send()
            .await
            .unwrap();
        assert_eq!(push_build.status(), 201);
        let mut message = None;
        for _ in 0..100 {
            if let Some(body) = received.lock().unwrap().get(1).cloned() {
                message = Some(body);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let message = message.expect("push build feedback reached Slack");
        assert_eq!(
            message["blocks"][0]["text"]["text"],
            ":speech_balloon: *Kite* build #14: feedback from Quinn &lt;QA&gt;"
        );
        let rendered = message.to_string();
        assert!(!rendered.contains("Triggered by"), "{rendered}");
        assert!(!rendered.contains("<@"), "{rendered}");
        assert!(!message["text"].as_str().unwrap().contains("Triggered by"));

        // Ten notes in ten minutes is the limit.
        for index in 0..9 {
            sqlx::query(
                "INSERT INTO build_feedback (id, build_id, author_user_id, author_email, message, created_at) \
                 VALUES (?1, 'b1', 'qa', 'qa@example.com', 'again', ?2)",
            )
            .bind(format!("extra-{index}"))
            .bind(now_unix())
            .execute(&pool)
            .await
            .unwrap();
        }
        let limited = client
            .post(&url)
            .bearer_auth(&qa)
            .json(&serde_json::json!({ "message": "one more" }))
            .send()
            .await
            .unwrap();
        assert_eq!(limited.status(), 429);
        // The limit applies before the body is parsed, so a bad screenshot is
        // not decoded either.
        let limited = client
            .post(&url)
            .bearer_auth(&qa)
            .json(&serde_json::json!({ "message": "x", "screenshot_base64": "!!" }))
            .send()
            .await
            .unwrap();
        assert_eq!(limited.status(), 429);
    }
}
