// Integration tests for build logs and artifact endpoints.
// Run with: cargo test -p oored --features test-support
#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use common::{
    body_json, connect_pool, create_test_app, create_test_app_with_network_urls,
    seed_github_integration, seed_project_chain, seed_test_user,
};
use http_body_util::BodyExt;
use tower::ServiceExt;

// ── Helpers ──────────────────────────────────────────────────────

/// Create a session token for the test user.
async fn create_session_token(pool: &sqlx::SqlitePool, user_id: &str) -> String {
    let token = oored::token::generate_session_token();
    let hashed = oored::token::hash_token(&token);
    let now = common::now_unix();
    let expires_at = now + 86400;

    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(&hashed)
    .bind(user_id)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await
    .expect("failed to create test session");

    token
}

async fn seed_user_with_role(pool: &sqlx::SqlitePool, email: &str, role: &str) -> String {
    let user_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6)",
    )
    .bind(&user_id)
    .bind(email)
    .bind(format!("{role}::{email}"))
    .bind(email)
    .bind(role)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed test user");
    user_id
}

async fn json_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .uri(uri)
        .method(method)
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"));

    let req_body = if let Some(json) = body {
        builder = builder.header(http::header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_string(&json).unwrap())
    } else {
        Body::empty()
    };

    let req = builder.body(req_body).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let json = body_json(resp.into_body()).await;
    (status, json)
}

/// Register a runner and return (runner_id, runner_token).
async fn register_runner(app: &axum::Router, session_token: &str, name: &str) -> (String, String) {
    let body = serde_json::json!({
        "name": name,
        "capabilities": { "os": "macos", "arch": "arm64" }
    });

    let req = Request::builder()
        .uri("/v1/runners/register")
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let runner_id = json["runner"]["id"].as_str().unwrap().to_string();
    let runner_token = json["token"].as_str().unwrap().to_string();

    (runner_id, runner_token)
}

/// Seed a build in 'running' state assigned to the given runner.
/// Returns the build_id.
async fn seed_running_build(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    pipeline_id: &str,
    runner_id: &str,
) -> String {
    let build_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();

    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, runner_id, \
         trigger_type, config_snapshot, queued_at, started_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, \
                 (SELECT COALESCE(MAX(build_number), 0) + 1 FROM builds WHERE project_id = ?2), \
                 'running', ?4, 'manual', '{}', ?5, ?5, ?5, ?5)",
    )
    .bind(&build_id)
    .bind(project_id)
    .bind(pipeline_id)
    .bind(runner_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to create test build");

    // Insert events
    let event_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO build_events (id, build_id, from_status, to_status, actor, reason, created_at) \
         VALUES (?1, ?2, NULL, 'queued', 'test', 'test build', ?3)",
    )
    .bind(&event_id)
    .bind(&build_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to create build event");

    build_id
}

/// Full test scaffold: create app, seed user/integration/project/runner/build.
/// Returns (app, pool, session_token, runner_id, runner_token, build_id).
async fn full_scaffold() -> (
    axum::Router,
    sqlx::SqlitePool,
    String,
    String,
    String,
    String,
) {
    full_scaffold_with_public_url(None).await
}

async fn full_scaffold_with_public_url(
    public_url: Option<&str>,
) -> (
    axum::Router,
    sqlx::SqlitePool,
    String,
    String,
    String,
    String,
) {
    full_scaffold_with_network_urls(public_url, None).await
}

async fn full_scaffold_with_network_urls(
    public_url: Option<&str>,
    artifact_delivery_url: Option<&str>,
) -> (
    axum::Router,
    sqlx::SqlitePool,
    String,
    String,
    String,
    String,
) {
    let tmp = tempfile::TempDir::new().unwrap();
    // Leak TempDir so it isn't dropped before tests finish
    let tmp = Box::leak(Box::new(tmp));
    let db_path = tmp.path().join("test.db");
    let app = if public_url.is_some() || artifact_delivery_url.is_some() {
        create_test_app_with_network_urls(&db_path, public_url, artifact_delivery_url).await
    } else {
        create_test_app(&db_path).await
    };
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session_token = create_session_token(&pool, &user_id).await;

    let integration_id = seed_github_integration(&pool, &user_id, "test-secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "test/logs-artifacts").await;
    let (runner_id, runner_token) = register_runner(&app, &session_token, "test-runner").await;
    let build_id = seed_running_build(&pool, &project_id, &pipeline_id, &runner_id).await;

    (app, pool, session_token, runner_id, runner_token, build_id)
}

async fn seed_project_member(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    user_id: &str,
    created_by: &str,
    role: &str,
) {
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO project_members \
         (id, project_id, user_id, role, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(project_id)
    .bind(user_id)
    .bind(role)
    .bind(created_by)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed project member");
}

async fn complete_artifact(
    app: &axum::Router,
    runner_id: &str,
    runner_token: &str,
    build_id: &str,
    artifact_id: &str,
) {
    let (status, _) = json_request(
        app,
        "POST",
        &format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts/{artifact_id}/complete"),
        runner_token,
        Some(serde_json::json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// ── Log ingestion tests ─────────────────────────────────────────

#[tokio::test]
async fn test_build_list_and_detail_include_display_context() {
    let (app, _pool, session_token, _runner_id, _runner_token, build_id) = full_scaffold().await;

    let (status, list) = json_request(&app, "GET", "/v1/builds", &session_token, None).await;
    assert_eq!(status, StatusCode::OK);
    let build = &list["builds"][0];
    assert_eq!(build["context"]["project_name"], "Test Project");
    assert_eq!(build["context"]["pipeline_name"], "Default");
    assert_eq!(build["context"]["runner_name"], "test-runner");

    let (status, detail) = json_request(
        &app,
        "GET",
        &format!("/v1/builds/{build_id}"),
        &session_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["build"]["context"], build["context"]);
}

#[tokio::test]
async fn test_append_build_logs() {
    let (app, _pool, _session_token, runner_id, runner_token, build_id) = full_scaffold().await;

    let body = serde_json::json!({
        "chunks": [
            { "sequence": 0, "content": "Cloning repository...", "stream": "stdout" },
            { "sequence": 1, "content": "warning: shallow clone", "stream": "stderr" },
            { "sequence": 2, "content": "Checkout complete.", "stream": "stdout" },
        ]
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/logs"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["appended"].as_i64().unwrap(), 3);
}

#[tokio::test]
async fn test_append_logs_dedup() {
    let (app, _pool, _session_token, runner_id, runner_token, build_id) = full_scaffold().await;

    // First batch
    let body = serde_json::json!({
        "chunks": [
            { "sequence": 0, "content": "line 0", "stream": "stdout" },
            { "sequence": 1, "content": "line 1", "stream": "stdout" },
        ]
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/logs"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["appended"].as_i64().unwrap(), 2);

    // Second batch with overlapping sequence 1 (should be deduped) and new sequence 2
    let body = serde_json::json!({
        "chunks": [
            { "sequence": 1, "content": "line 1 again", "stream": "stdout" },
            { "sequence": 2, "content": "line 2", "stream": "stdout" },
        ]
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/logs"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    // Only sequence 2 should be appended (sequence 1 is a duplicate)
    assert_eq!(json["appended"].as_i64().unwrap(), 1);
}

#[tokio::test]
async fn test_append_logs_empty_chunks() {
    let (app, _pool, _session_token, runner_id, runner_token, build_id) = full_scaffold().await;

    let body = serde_json::json!({ "chunks": [] });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/logs"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["appended"].as_i64().unwrap(), 0);
}

#[tokio::test]
async fn test_append_logs_cross_runner_blocked() {
    let (app, pool, session_token, runner_id, _runner_token, build_id) = full_scaffold().await;

    // Register a second runner
    let (_other_runner_id, other_runner_token) =
        register_runner(&app, &session_token, "other-runner").await;

    // Other runner tries to upload logs for build assigned to first runner
    let body = serde_json::json!({
        "chunks": [{ "sequence": 0, "content": "sneaky", "stream": "stdout" }]
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/logs"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {other_runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "cross-runner log upload should be blocked"
    );
    drop(pool);
}

// ── Log retrieval tests ─────────────────────────────────────────

#[tokio::test]
async fn test_get_build_logs() {
    let (app, _pool, session_token, runner_id, runner_token, build_id) = full_scaffold().await;

    // Upload some logs first
    let body = serde_json::json!({
        "chunks": [
            { "sequence": 0, "content": "first line", "stream": "stdout" },
            { "sequence": 1, "content": "error line", "stream": "stderr" },
            { "sequence": 2, "content": "last line", "stream": "stdout" },
        ]
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/logs"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Now retrieve logs
    let req = Request::builder()
        .uri(format!("/v1/builds/{build_id}/logs"))
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["total"].as_i64().unwrap(), 3);
    let logs = json["logs"].as_array().unwrap();
    assert_eq!(logs.len(), 3);
    assert_eq!(logs[0]["sequence"].as_i64().unwrap(), 0);
    assert_eq!(logs[0]["content"].as_str().unwrap(), "first line");
    assert_eq!(logs[0]["stream"].as_str().unwrap(), "stdout");
    assert_eq!(logs[1]["stream"].as_str().unwrap(), "stderr");
}

#[tokio::test]
async fn test_get_build_logs_pagination() {
    let (app, _pool, session_token, runner_id, runner_token, build_id) = full_scaffold().await;

    // Upload 5 log lines
    let body = serde_json::json!({
        "chunks": (0..5).map(|i| serde_json::json!({
            "sequence": i,
            "content": format!("line {}", i),
            "stream": "stdout",
        })).collect::<Vec<_>>()
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/logs"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    app.clone().oneshot(req).await.unwrap();

    // Fetch with after_sequence=1 and limit=2
    let req = Request::builder()
        .uri(format!(
            "/v1/builds/{build_id}/logs?after_sequence=1&limit=2"
        ))
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let logs = json["logs"].as_array().unwrap();
    assert_eq!(logs.len(), 2, "should return exactly 2 logs");
    assert_eq!(logs[0]["sequence"].as_i64().unwrap(), 2);
    assert_eq!(logs[1]["sequence"].as_i64().unwrap(), 3);
    assert_eq!(
        json["total"].as_i64().unwrap(),
        5,
        "total should reflect all logs"
    );
}

#[tokio::test]
async fn test_get_logs_build_not_found() {
    let (app, _pool, session_token, _runner_id, _runner_token, _build_id) = full_scaffold().await;

    let req = Request::builder()
        .uri("/v1/builds/nonexistent-build-id/logs")
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_non_member_cannot_read_build_logs_or_artifacts() {
    let (app, pool, session_token, runner_id, runner_token, build_id) = full_scaffold().await;
    let outsider_id = seed_user_with_role(&pool, "outsider@example.com", "developer").await;
    let outsider_session = create_session_token(&pool, &outsider_id).await;

    let artifact_body = serde_json::json!({
        "name": "private.apk",
        "artifact_type": "apk",
    });
    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&artifact_body).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let artifact_json = body_json(resp.into_body()).await;
    let artifact_id = artifact_json["artifact"]["id"].as_str().unwrap();
    complete_artifact(&app, &runner_id, &runner_token, &build_id, artifact_id).await;

    let (status, owner_token_json) = json_request(
        &app,
        "POST",
        &format!("/v1/artifacts/{artifact_id}/scoped-token"),
        &session_token,
        Some(serde_json::json!({
            "ttl_secs": 3600,
            "single_use": true
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "owner scoped token create: {owner_token_json}"
    );
    let token_id = owner_token_json["id"].as_str().unwrap().to_string();

    for (method, uri, body) in [
        ("GET", "/v1/builds".to_string(), None),
        ("GET", format!("/v1/builds/{build_id}"), None),
        ("GET", format!("/v1/builds/{build_id}/logs"), None),
        ("POST", format!("/v1/builds/{build_id}/stream-token"), None),
        ("GET", format!("/v1/builds/{build_id}/logs/stream"), None),
        ("GET", format!("/v1/builds/{build_id}/artifacts"), None),
        (
            "POST",
            format!("/v1/artifacts/{artifact_id}/download-link"),
            None,
        ),
        (
            "POST",
            format!("/v1/artifacts/{artifact_id}/scoped-token"),
            Some(serde_json::json!({
                "ttl_secs": 3600,
                "single_use": true
            })),
        ),
        (
            "GET",
            format!("/v1/artifacts/{artifact_id}/scoped-tokens"),
            None,
        ),
        ("DELETE", format!("/v1/artifact-tokens/{token_id}"), None),
    ] {
        let (status, json) = json_request(&app, method, &uri, &outsider_session, body).await;
        if method == "GET" && status == StatusCode::OK {
            assert_eq!(json["total"].as_i64(), Some(0));
        } else {
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}: {json}");
        }
    }
}

// ── Stream token tests ──────────────────────────────────────────

#[tokio::test]
async fn test_create_stream_token() {
    let (app, _pool, session_token, _runner_id, _runner_token, build_id) = full_scaffold().await;

    let req = Request::builder()
        .uri(format!("/v1/builds/{build_id}/stream-token"))
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let token = json["token"].as_str().unwrap();
    assert!(!token.is_empty(), "stream token should not be empty");
    assert!(
        json["expires_at"].as_i64().unwrap() > common::now_unix(),
        "expires_at should be in the future"
    );
}

#[tokio::test]
async fn test_create_stream_token_build_not_found() {
    let (app, _pool, session_token, _runner_id, _runner_token, _build_id) = full_scaffold().await;

    let req = Request::builder()
        .uri("/v1/builds/nonexistent-id/stream-token")
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_stream_token_unauthenticated() {
    let (app, _pool, _session_token, _runner_id, _runner_token, build_id) = full_scaffold().await;

    let req = Request::builder()
        .uri(format!("/v1/builds/{build_id}/stream-token"))
        .method("POST")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Regression: full session token in ?token= query param must be rejected by SSE endpoint.
/// Only short-lived stream tokens are accepted via query param; session tokens require
/// the Authorization header.
#[tokio::test]
async fn test_sse_rejects_session_token_in_query_param() {
    let (app, _pool, session_token, _runner_id, _runner_token, build_id) = full_scaffold().await;

    // Pass the full session token as ?token= — this should be rejected
    let req = Request::builder()
        .uri(format!(
            "/v1/builds/{build_id}/logs/stream?token={session_token}"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "SSE endpoint must reject full session tokens in ?token= query param"
    );

    let json = body_json(resp.into_body()).await;
    assert_eq!(
        json["code"].as_str().unwrap(),
        "invalid_stream_token",
        "error code should indicate stream token validation failure, not a generic auth error"
    );
}

// ── Artifact creation tests ─────────────────────────────────────

#[tokio::test]
async fn test_create_artifact() {
    let (app, _pool, _session_token, runner_id, runner_token, build_id) = full_scaffold().await;

    let body = serde_json::json!({
        "name": "app-debug.apk",
        "artifact_type": "apk",
        "file_size": 12345678,
        "checksum": "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
        "metadata": { "variant": "debug" }
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let artifact = &json["artifact"];
    assert_eq!(artifact["name"].as_str().unwrap(), "app-debug.apk");
    assert_eq!(artifact["artifact_type"].as_str().unwrap(), "apk");
    assert_eq!(artifact["file_size"].as_i64().unwrap(), 12345678);
    assert!(artifact["id"].as_str().is_some());
    assert!(artifact["build_id"].as_str().is_some());
    // upload_url should be empty when S3 is not configured
    assert_eq!(json["upload_url"].as_str().unwrap(), "");
    assert_eq!(artifact["state"].as_str(), Some("pending"));
}

#[tokio::test]
async fn test_create_artifact_invalid_type() {
    let (app, _pool, _session_token, runner_id, runner_token, build_id) = full_scaffold().await;

    let body = serde_json::json!({
        "name": "something.xyz",
        "artifact_type": "invalid_type",
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["code"].as_str().unwrap(), "invalid_artifact_type");
}

#[tokio::test]
async fn test_create_artifact_empty_name() {
    let (app, _pool, _session_token, runner_id, runner_token, build_id) = full_scaffold().await;

    let body = serde_json::json!({
        "name": "   ",
        "artifact_type": "generic",
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["code"].as_str().unwrap(), "invalid_name");
}

#[tokio::test]
async fn test_create_artifact_cross_runner_blocked() {
    let (app, _pool, session_token, runner_id, _runner_token, build_id) = full_scaffold().await;

    let (_other_runner_id, other_runner_token) =
        register_runner(&app, &session_token, "other-runner").await;

    let body = serde_json::json!({
        "name": "sneaky.apk",
        "artifact_type": "apk",
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {other_runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_create_artifact_checksum_dedup() {
    let (app, _pool, session_token, runner_id, runner_token, build_id) = full_scaffold().await;

    let body = serde_json::json!({
        "name": "release.apk",
        "artifact_type": "apk",
        "file_size": 42,
        "checksum": "same-checksum-for-dedup"
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let first_json = body_json(resp.into_body()).await;
    let first_id = first_json["artifact"]["id"].as_str().unwrap().to_string();
    complete_artifact(&app, &runner_id, &runner_token, &build_id, &first_id).await;

    let body = serde_json::json!({
        "name": "release-copy.apk",
        "artifact_type": "apk",
        "file_size": 99,
        "checksum": "same-checksum-for-dedup"
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let second_json = body_json(resp.into_body()).await;
    let second_id = second_json["artifact"]["id"].as_str().unwrap();

    assert_eq!(second_id, first_id);
    assert_eq!(second_json["upload_url"].as_str().unwrap(), "");

    let req = Request::builder()
        .uri(format!("/v1/builds/{build_id}/artifacts"))
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    let artifacts = json["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["id"].as_str().unwrap(), first_id);
}

// ── Artifact listing tests ──────────────────────────────────────

#[tokio::test]
async fn test_list_artifacts() {
    let (app, _pool, session_token, runner_id, runner_token, build_id) = full_scaffold().await;

    // Create two artifacts
    for (name, art_type) in [("app.apk", "apk"), ("app.ipa", "ipa")] {
        let body = serde_json::json!({
            "name": name,
            "artifact_type": art_type,
            "file_size": 1000,
        });

        let req = Request::builder()
            .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
            .method("POST")
            .header(http::header::CONTENT_TYPE, "application/json")
            .header(
                http::header::AUTHORIZATION,
                format!("Bearer {runner_token}"),
            )
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp.into_body()).await;
        complete_artifact(
            &app,
            &runner_id,
            &runner_token,
            &build_id,
            json["artifact"]["id"].as_str().unwrap(),
        )
        .await;
    }

    // List artifacts
    let req = Request::builder()
        .uri(format!("/v1/builds/{build_id}/artifacts"))
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let artifacts = json["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 2);
    assert_eq!(artifacts[0]["name"].as_str().unwrap(), "app.apk");
    assert_eq!(artifacts[1]["name"].as_str().unwrap(), "app.ipa");
}

#[tokio::test]
async fn test_list_artifacts_empty() {
    let (app, _pool, session_token, _runner_id, _runner_token, build_id) = full_scaffold().await;

    let req = Request::builder()
        .uri(format!("/v1/builds/{build_id}/artifacts"))
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let artifacts = json["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 0);
}

// ── Artifact download link tests ────────────────────────────────

#[tokio::test]
async fn test_download_link_no_storage() {
    let (app, _pool, session_token, runner_id, runner_token, build_id) = full_scaffold().await;

    // Create an artifact
    let body = serde_json::json!({
        "name": "app.apk",
        "artifact_type": "apk",
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let create_json = body_json(resp.into_body()).await;
    let artifact_id = create_json["artifact"]["id"].as_str().unwrap();
    complete_artifact(&app, &runner_id, &runner_token, &build_id, artifact_id).await;

    // Request download link — should fail because S3 is not configured
    let req = Request::builder()
        .uri(format!("/v1/artifacts/{artifact_id}/download-link"))
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "download link should fail when S3 is not configured"
    );

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["code"].as_str().unwrap(), "storage_not_configured");
}

#[tokio::test]
async fn test_download_link_artifact_not_found() {
    let (app, _pool, session_token, _runner_id, _runner_token, _build_id) = full_scaffold().await;

    let req = Request::builder()
        .uri("/v1/artifacts/nonexistent-id/download-link")
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_ios_install_manifest_and_qa_permissions() {
    let (app, pool, owner_token, runner_id, runner_token, build_id) =
        full_scaffold_with_network_urls(None, Some("https://install.ci.example.com")).await;

    let artifact_body = serde_json::json!({
        "name": "Kite.ipa",
        "artifact_type": "ipa",
        "file_size": 12_345_678,
        "metadata": {
            "ios_app": {
                "bundle_identifier": "com.example.kite",
                "display_name": "Kite QA",
                "version": "3.2.1",
                "build_number": "42"
            },
            "ios_signing": {
                "bundle_ids": ["com.example.kite"],
                "effective_export_method": "release-testing"
            }
        }
    });
    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&artifact_body).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let artifact_json = body_json(resp.into_body()).await;
    let artifact_id = artifact_json["artifact"]["id"].as_str().unwrap();
    complete_artifact(&app, &runner_id, &runner_token, &build_id, artifact_id).await;

    let (status, install) = json_request(
        &app,
        "POST",
        &format!("/v1/artifacts/{artifact_id}/install-link"),
        &owner_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "install link: {install}");
    assert_eq!(install["platform"].as_str(), Some("ios"));
    assert!(
        install["install_url"]
            .as_str()
            .unwrap()
            .starts_with("itms-services://?action=download-manifest&url=")
    );
    let manifest_url = install["manifest_url"].as_str().unwrap();
    let manifest_path = manifest_url
        .strip_prefix("https://install.ci.example.com")
        .expect("manifest uses configured artifact delivery URL");
    assert!(
        install["download_url"]
            .as_str()
            .unwrap()
            .starts_with("https://install.ci.example.com/install/artifact/")
    );
    let req = Request::builder()
        .uri(manifest_path)
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/xml; charset=utf-8")
    );
    let manifest = String::from_utf8(
        resp.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(manifest.contains("<string>com.example.kite</string>"));
    assert!(manifest.contains("<string>Version 3.2.1 (42)</string>"));
    assert!(manifest.contains("https://install.ci.example.com/install/artifact/"));

    let project_id: String = sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let owner_id: String = sqlx::query_scalar("SELECT created_by FROM projects WHERE id = ?1")
        .bind(&project_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let qa_id = seed_user_with_role(&pool, "qa@example.com", "qa_viewer").await;
    seed_project_member(&pool, &project_id, &qa_id, &owner_id, "viewer").await;
    let qa_token = create_session_token(&pool, &qa_id).await;

    let (status, qa_install) = json_request(
        &app,
        "POST",
        &format!("/v1/artifacts/{artifact_id}/install-link"),
        &qa_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "QA install link: {qa_install}");

    let (status, denied_share) = json_request(
        &app,
        "POST",
        &format!("/v1/artifacts/{artifact_id}/scoped-token"),
        &qa_token,
        Some(serde_json::json!({
            "ttl_secs": 3600,
            "single_use": false
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "QA share link: {denied_share}"
    );
}

#[tokio::test]
async fn test_public_install_prefix_rejects_invalid_tokens() {
    let (app, _pool, _owner_token, _runner_id, _runner_token, _build_id) =
        full_scaffold_with_public_url(Some("https://ci.example.com")).await;

    for (path, expected_status) in [
        (
            "/install/ios/not-a-token/manifest.plist",
            StatusCode::UNAUTHORIZED,
        ),
        ("/install/artifact/not-a-token", StatusCode::UNAUTHORIZED),
        ("/install/download/not-a-token", StatusCode::NOT_FOUND),
    ] {
        let request = Request::builder()
            .uri(path)
            .method("GET")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected_status, "{path}");
    }
}

#[tokio::test]
async fn test_android_install_link_uses_protected_scoped_download() {
    let (app, _pool, owner_token, runner_id, runner_token, build_id) =
        full_scaffold_with_public_url(Some("https://ci.example.com")).await;

    let artifact_body = serde_json::json!({
        "name": "Kite.apk",
        "artifact_type": "apk",
        "file_size": 8_765_432
    });
    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&artifact_body).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let artifact_json = body_json(resp.into_body()).await;
    let artifact_id = artifact_json["artifact"]["id"].as_str().unwrap();
    complete_artifact(&app, &runner_id, &runner_token, &build_id, artifact_id).await;

    let (status, install) = json_request(
        &app,
        "POST",
        &format!("/v1/artifacts/{artifact_id}/install-link"),
        &owner_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "install link: {install}");
    assert_eq!(install["platform"].as_str(), Some("android"));
    assert!(install["manifest_url"].is_null());
    assert_eq!(install["install_url"], install["download_url"]);
    assert!(
        install["install_url"]
            .as_str()
            .unwrap()
            .starts_with("https://ci.example.com/install/artifact/")
    );
}

// ── End-to-end log + artifact flow test ─────────────────────────

#[tokio::test]
async fn test_full_log_and_artifact_flow() {
    let (app, _pool, session_token, runner_id, runner_token, build_id) = full_scaffold().await;

    // 1. Runner uploads log chunks
    let log_body = serde_json::json!({
        "chunks": [
            { "sequence": 0, "content": "Building...", "stream": "stdout" },
            { "sequence": 1, "content": "WARNING: deprecated API", "stream": "stderr" },
            { "sequence": 2, "content": "Build succeeded.", "stream": "stdout" },
        ]
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/logs"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&log_body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. Runner registers an artifact
    let artifact_body = serde_json::json!({
        "name": "release.apk",
        "artifact_type": "apk",
        "file_size": 5_000_000,
        "checksum": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        "metadata": { "build_type": "release", "min_sdk": 21 }
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&artifact_body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let artifact_json = body_json(resp.into_body()).await;
    complete_artifact(
        &app,
        &runner_id,
        &runner_token,
        &build_id,
        artifact_json["artifact"]["id"].as_str().unwrap(),
    )
    .await;

    // 3. Operator fetches logs
    let req = Request::builder()
        .uri(format!("/v1/builds/{build_id}/logs"))
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let logs_json = body_json(resp.into_body()).await;
    assert_eq!(logs_json["total"].as_i64().unwrap(), 3);
    assert_eq!(logs_json["logs"][1]["stream"].as_str().unwrap(), "stderr");

    // 4. Operator lists artifacts
    let req = Request::builder()
        .uri(format!("/v1/builds/{build_id}/artifacts"))
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let artifacts_json = body_json(resp.into_body()).await;
    let artifacts = artifacts_json["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["name"].as_str().unwrap(), "release.apk");
    assert_eq!(artifacts[0]["file_size"].as_i64().unwrap(), 5_000_000);

    // 5. Operator obtains stream token
    let req = Request::builder()
        .uri(format!("/v1/builds/{build_id}/stream-token"))
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let token_json = body_json(resp.into_body()).await;
    assert!(!token_json["token"].as_str().unwrap().is_empty());
}
