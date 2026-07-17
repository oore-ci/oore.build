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

async fn open_api_token_stream(
    app: &axum::Router,
    build_id: &str,
    api_token: &str,
) -> http::Response<Body> {
    let (status, json) = json_request(
        app,
        "POST",
        &format!("/v1/builds/{build_id}/stream-token"),
        api_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let stream_token = json["token"].as_str().unwrap();
    let request = Request::builder()
        .uri(format!(
            "/v1/builds/{build_id}/logs/stream?token={stream_token}"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response
}

async fn assert_stream_ends_before_log(response: http::Response<Body>, marker: &str) {
    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        response.into_body().collect(),
    )
    .await
    .expect("unauthorized stream must close promptly")
    .unwrap()
    .to_bytes();
    let body = String::from_utf8_lossy(&bytes);
    assert!(body.contains("authorization_ended"), "stream body: {body}");
    assert!(!body.contains(marker), "stream body: {body}");
}

async fn insert_log_marker(pool: &sqlx::SqlitePool, build_id: &str, sequence: i64, marker: &str) {
    sqlx::query(
        "INSERT INTO build_logs (id, build_id, sequence, content, stream, created_at) \
         VALUES (?1, ?2, ?3, ?4, 'stdout', ?5)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(build_id)
    .bind(sequence)
    .bind(marker)
    .bind(common::now_unix())
    .execute(pool)
    .await
    .unwrap();
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

async fn create_available_artifact(
    app: &axum::Router,
    runner_id: &str,
    runner_token: &str,
    build_id: &str,
    name: &str,
) -> String {
    let (status, body) = json_request(
        app,
        "POST",
        &format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"),
        runner_token,
        Some(serde_json::json!({
            "name": name,
            "artifact_type": "apk"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let artifact_id = body["artifact"]["id"].as_str().unwrap().to_string();
    complete_artifact(app, runner_id, runner_token, build_id, &artifact_id).await;
    artifact_id
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
async fn test_build_list_sort_is_allowlisted() {
    let (app, pool, session_token, runner_id, _runner_token, first_build_id) =
        full_scaffold().await;
    let (project_id, pipeline_id): (String, String) =
        sqlx::query_as("SELECT project_id, pipeline_id FROM builds WHERE id = ?1")
            .bind(&first_build_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let failed_build_id = seed_running_build(&pool, &project_id, &pipeline_id, &runner_id).await;
    sqlx::query("UPDATE builds SET status = 'failed' WHERE id = ?1")
        .bind(&failed_build_id)
        .execute(&pool)
        .await
        .unwrap();

    let (status, list) = json_request(
        &app,
        "GET",
        "/v1/builds?sort=status&direction=asc",
        &session_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["builds"][0]["status"], "failed");

    let (status, filtered) = json_request(
        &app,
        "GET",
        "/v1/builds?status=failed%2Crunning&limit=1",
        &session_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(filtered["total"], 2);
    assert_eq!(filtered["builds"].as_array().unwrap().len(), 1);

    let (status, filtered) = json_request(
        &app,
        "GET",
        "/v1/builds?status=%2C%2C",
        &session_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(filtered["total"], 0);

    let (status, error) = json_request(
        &app,
        "GET",
        "/v1/builds?direction=sideways",
        &session_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["code"], "invalid_input");
}

#[tokio::test]
async fn test_build_list_pagination_is_positive_and_bounded() {
    let (app, pool, session_token, _runner_id, _runner_token, first_build_id) =
        full_scaffold().await;
    let (project_id, pipeline_id): (String, String) =
        sqlx::query_as("SELECT project_id, pipeline_id FROM builds WHERE id = ?1")
            .bind(&first_build_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let now = common::now_unix();
    sqlx::query(
        "WITH RECURSIVE seq(n) AS (SELECT 2 UNION ALL SELECT n + 1 FROM seq WHERE n < 201) \
         INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
          config_snapshot, queued_at, created_at, updated_at) \
         SELECT printf('page-build-%03d', n), ?1, ?2, n, 'succeeded', 'manual', '{}', ?3, ?3, ?3 \
         FROM seq",
    )
    .bind(&project_id)
    .bind(&pipeline_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    for (uri, expected_len) in [
        ("/v1/builds", 50),
        ("/v1/builds?limit=2", 2),
        ("/v1/builds?limit=500", 200),
    ] {
        let (status, response) = json_request(&app, "GET", uri, &session_token, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {response}");
        assert_eq!(response["total"], 201);
        assert_eq!(response["builds"].as_array().unwrap().len(), expected_len);
    }

    for uri in [
        "/v1/builds?limit=-1",
        "/v1/builds?limit=0",
        "/v1/builds?offset=-1",
        "/v1/builds?limit=500&offset=-1",
    ] {
        let (status, response) = json_request(&app, "GET", uri, &session_token, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {response}");
        assert_eq!(response["code"], "invalid_input");
    }
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

#[tokio::test]
async fn test_stream_token_is_single_use() {
    let (app, _pool, session_token, _runner_id, _runner_token, build_id) = full_scaffold().await;
    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/builds/{build_id}/stream-token"),
        &session_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let stream_token = json["token"].as_str().unwrap();

    let open = || {
        Request::builder()
            .uri(format!(
                "/v1/builds/{build_id}/logs/stream?token={stream_token}"
            ))
            .method("GET")
            .body(Body::empty())
            .unwrap()
    };
    let first = app.clone().oneshot(open()).await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let replay = app.clone().oneshot(open()).await.unwrap();
    assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_stream_admission_budget_is_shared_and_released() {
    let (app, _pool, session_token, _runner_id, _runner_token, build_id) = full_scaffold().await;
    let mut streams = Vec::new();

    for _ in 0..4 {
        let req = Request::builder()
            .uri(format!("/v1/builds/{build_id}/logs/stream"))
            .method("GET")
            .header(
                http::header::AUTHORIZATION,
                format!("Bearer {session_token}"),
            )
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        streams.push(response);
    }

    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/builds/{build_id}/stream-token"),
        &session_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let stream_token = json["token"].as_str().unwrap();
    let req = Request::builder()
        .uri(format!(
            "/v1/builds/{build_id}/logs/stream?token={stream_token}"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let rejected = app.clone().oneshot(req).await.unwrap();
    assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);

    drop(streams.pop());
    let req = Request::builder()
        .uri(format!("/v1/builds/{build_id}/logs/stream"))
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();
    let replacement = app.clone().oneshot(req).await.unwrap();
    assert_eq!(replacement.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_open_stream_stops_before_emitting_after_session_revocation() {
    let (app, pool, session_token, _runner_id, _runner_token, build_id) = full_scaffold().await;
    let req = Request::builder()
        .uri(format!("/v1/builds/{build_id}/logs/stream"))
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    sqlx::query("DELETE FROM sessions WHERE token_hash = ?1")
        .bind(oored::token::hash_token(&session_token))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO build_logs (id, build_id, sequence, content, stream, created_at) \
         VALUES (?1, ?2, 0, 'after-revocation', 'stdout', ?3)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&build_id)
    .bind(common::now_unix())
    .execute(&pool)
    .await
    .unwrap();

    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        response.into_body().collect(),
    )
    .await
    .expect("revoked stream must close promptly")
    .unwrap()
    .to_bytes();
    let body = String::from_utf8_lossy(&bytes);
    assert!(body.contains("authorization_ended"), "stream body: {body}");
    assert!(!body.contains("after-revocation"), "stream body: {body}");
}

#[tokio::test]
async fn test_open_stream_stops_after_api_token_revocation() {
    let (app, pool, _session_token, _runner_id, _runner_token, build_id) = full_scaffold().await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (_id, api_token, _prefix, _created_at) =
        oored::api_tokens::create_api_token(&pool, &user_id, "stream", "owner", None)
            .await
            .unwrap();
    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/builds/{build_id}/stream-token"),
        &api_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let stream_token = json["token"].as_str().unwrap();
    let req = Request::builder()
        .uri(format!(
            "/v1/builds/{build_id}/logs/stream?token={stream_token}"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    sqlx::query("UPDATE api_tokens SET revoked_at = ?1 WHERE token_hash = ?2")
        .bind(common::now_unix())
        .bind(oored::token::hash_token(&api_token))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO build_logs (id, build_id, sequence, content, stream, created_at) \
         VALUES (?1, ?2, 0, 'after-api-revocation', 'stdout', ?3)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&build_id)
    .bind(common::now_unix())
    .execute(&pool)
    .await
    .unwrap();

    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        response.into_body().collect(),
    )
    .await
    .expect("revoked API-token stream must close promptly")
    .unwrap()
    .to_bytes();
    let body = String::from_utf8_lossy(&bytes);
    assert!(body.contains("authorization_ended"), "stream body: {body}");
    assert!(
        !body.contains("after-api-revocation"),
        "stream body: {body}"
    );
}

#[tokio::test]
async fn test_open_api_token_stream_revalidates_role_status_and_expiry() {
    let (app, pool, _session_token, _runner_id, _runner_token, build_id) = full_scaffold().await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE role = 'owner' LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (_id, api_token, _prefix, _created_at) = oored::api_tokens::create_api_token(
        &pool,
        &user_id,
        "current-user-revalidation",
        "owner",
        Some(common::now_unix() + 60),
    )
    .await
    .unwrap();

    let demoted = open_api_token_stream(&app, &build_id, &api_token).await;
    sqlx::query("UPDATE users SET role = 'developer' WHERE id = ?1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    insert_log_marker(&pool, &build_id, 100, "after-role-demotion").await;
    assert_stream_ends_before_log(demoted, "after-role-demotion").await;

    sqlx::query("UPDATE users SET role = 'owner' WHERE id = ?1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    let disabled = open_api_token_stream(&app, &build_id, &api_token).await;
    sqlx::query("UPDATE users SET status = 'disabled' WHERE id = ?1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    insert_log_marker(&pool, &build_id, 101, "after-user-disablement").await;
    assert_stream_ends_before_log(disabled, "after-user-disablement").await;

    sqlx::query("UPDATE users SET status = 'active' WHERE id = ?1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    let expired = open_api_token_stream(&app, &build_id, &api_token).await;
    sqlx::query("UPDATE api_tokens SET expires_at = ?1 WHERE token_hash = ?2")
        .bind(common::now_unix() - 1)
        .bind(oored::token::hash_token(&api_token))
        .execute(&pool)
        .await
        .unwrap();
    insert_log_marker(&pool, &build_id, 102, "after-token-expiry").await;
    assert_stream_ends_before_log(expired, "after-token-expiry").await;
}

#[tokio::test]
async fn test_open_stream_stops_after_project_membership_removal() {
    let (app, pool, _owner_token, _runner_id, _runner_token, build_id) = full_scaffold().await;
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let owner_id: String = sqlx::query_scalar("SELECT id FROM users WHERE role = 'owner' LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let developer_id =
        seed_user_with_role(&pool, "stream-developer@example.com", "developer").await;
    seed_project_member(&pool, &project_id, &developer_id, &owner_id, "viewer").await;
    let developer_token = create_session_token(&pool, &developer_id).await;
    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/builds/{build_id}/stream-token"),
        &developer_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let stream_token = json["token"].as_str().unwrap();
    let req = Request::builder()
        .uri(format!(
            "/v1/builds/{build_id}/logs/stream?token={stream_token}"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    sqlx::query("DELETE FROM project_members WHERE project_id = ?1 AND user_id = ?2")
        .bind(&project_id)
        .bind(&developer_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO build_logs (id, build_id, sequence, content, stream, created_at) \
         VALUES (?1, ?2, 0, 'after-membership-removal', 'stdout', ?3)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&build_id)
    .bind(common::now_unix())
    .execute(&pool)
    .await
    .unwrap();

    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        response.into_body().collect(),
    )
    .await
    .expect("unauthorized project stream must close promptly")
    .unwrap()
    .to_bytes();
    let body = String::from_utf8_lossy(&bytes);
    assert!(body.contains("authorization_ended"), "stream body: {body}");
    assert!(
        !body.contains("after-membership-removal"),
        "stream body: {body}"
    );
}

#[tokio::test]
async fn test_open_api_token_stream_stops_after_project_membership_removal() {
    let (app, pool, _owner_token, _runner_id, _runner_token, build_id) = full_scaffold().await;
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let owner_id: String = sqlx::query_scalar("SELECT id FROM users WHERE role = 'owner' LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let developer_id =
        seed_user_with_role(&pool, "api-stream-developer@example.com", "developer").await;
    seed_project_member(&pool, &project_id, &developer_id, &owner_id, "viewer").await;
    let (_id, api_token, _prefix, _created_at) = oored::api_tokens::create_api_token(
        &pool,
        &developer_id,
        "membership-revalidation",
        "developer",
        None,
    )
    .await
    .unwrap();
    let response = open_api_token_stream(&app, &build_id, &api_token).await;

    sqlx::query("DELETE FROM project_members WHERE project_id = ?1 AND user_id = ?2")
        .bind(&project_id)
        .bind(&developer_id)
        .execute(&pool)
        .await
        .unwrap();
    insert_log_marker(&pool, &build_id, 103, "after-api-membership-removal").await;
    assert_stream_ends_before_log(response, "after-api-membership-removal").await;
}

#[tokio::test]
async fn test_authorized_stream_still_delivers_logs_and_terminal_event() {
    let (app, pool, session_token, _runner_id, _runner_token, build_id) = full_scaffold().await;
    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/builds/{build_id}/stream-token"),
        &session_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let stream_token = json["token"].as_str().unwrap();
    let req = Request::builder()
        .uri(format!(
            "/v1/builds/{build_id}/logs/stream?token={stream_token}"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    sqlx::query(
        "INSERT INTO build_logs (id, build_id, sequence, content, stream, created_at) \
         VALUES (?1, ?2, 0, 'legitimate-log', 'stdout', ?3)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&build_id)
    .bind(common::now_unix())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE builds SET status = 'succeeded' WHERE id = ?1")
        .bind(&build_id)
        .execute(&pool)
        .await
        .unwrap();

    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        response.into_body().collect(),
    )
    .await
    .expect("terminal stream must close")
    .unwrap()
    .to_bytes();
    let body = String::from_utf8_lossy(&bytes);
    assert!(body.contains("legitimate-log"), "stream body: {body}");
    assert!(body.contains("build_finished"), "stream body: {body}");
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
    let (app, pool, session_token, runner_id, runner_token, build_id) = full_scaffold().await;

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

    let project_id: String = sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let req = Request::builder()
        .uri(format!("/v1/projects/{project_id}/artifacts"))
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
    assert_eq!(json["artifacts"].as_array().unwrap().len(), 2);

    let req = Request::builder()
        .uri(format!("/v1/projects/{project_id}/artifacts?limit=1"))
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
    assert_eq!(json["artifacts"].as_array().unwrap().len(), 1);

    let req = Request::builder()
        .uri(format!("/v1/projects/{project_id}/artifacts?limit=0"))
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["code"], "invalid_input");
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

#[tokio::test]
async fn test_list_build_artifacts_is_bounded_and_membership_filtered() {
    let (app, pool, owner_token, runner_id, runner_token, first_build_id) = full_scaffold().await;
    let first_project_id: String =
        sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
            .bind(&first_build_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let owner_id: String = sqlx::query_scalar("SELECT created_by FROM projects WHERE id = ?1")
        .bind(&first_project_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let integration_id: String = sqlx::query_scalar(
        "SELECT ii.integration_id FROM projects p \
         JOIN integration_repositories ir ON ir.id = p.repository_id \
         JOIN integration_installations ii ON ii.id = ir.installation_id \
         WHERE p.id = ?1",
    )
    .bind(&first_project_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (second_project_id, second_pipeline_id) =
        seed_project_chain(&pool, &integration_id, &owner_id, "test/private-artifacts").await;
    let second_build_id =
        seed_running_build(&pool, &second_project_id, &second_pipeline_id, &runner_id).await;

    let first_artifact_id = create_available_artifact(
        &app,
        &runner_id,
        &runner_token,
        &first_build_id,
        "shared.apk",
    )
    .await;
    let second_artifact_id = create_available_artifact(
        &app,
        &runner_id,
        &runner_token,
        &second_build_id,
        "private.apk",
    )
    .await;

    let qa_user_id = seed_user_with_role(&pool, "qa-batch@example.com", "qa_viewer").await;
    seed_project_member(&pool, &first_project_id, &qa_user_id, &owner_id, "viewer").await;
    let qa_token = create_session_token(&pool, &qa_user_id).await;
    let query = serde_json::json!({
        "build_ids": [&first_build_id, &second_build_id]
    });

    let (status, body) = json_request(
        &app,
        "POST",
        "/v1/artifacts/query",
        &qa_token,
        Some(query.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let artifacts = body["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["id"], first_artifact_id);

    let (status, body) = json_request(
        &app,
        "POST",
        "/v1/artifacts/query",
        &owner_token,
        Some(query),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let artifact_ids = body["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|artifact| artifact["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(artifact_ids.contains(&first_artifact_id.as_str()));
    assert!(artifact_ids.contains(&second_artifact_id.as_str()));

    let oversized = serde_json::json!({
        "build_ids": (0..201).map(|index| format!("build-{index}")).collect::<Vec<_>>()
    });
    let (status, body) = json_request(
        &app,
        "POST",
        "/v1/artifacts/query",
        &owner_token,
        Some(oversized),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "too_many_build_ids");
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
