// Runner integration tests — verifies runner registration, heartbeat, claim, execution, and double-claim prevention.
// Run with: cargo test -p oored --features test-support
#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use common::{
    body_json, connect_pool, create_test_app, seed_github_integration, seed_gitlab_integration,
    seed_project_chain, seed_test_user,
};
use sqlx::Row;
use tower::ServiceExt;
use uuid::Uuid;

/// Create a session token for the test user so AuthUser extractor works.
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
    let user_id = Uuid::new_v4().to_string();
    let now = common::now_unix();
    let subject = format!("subject-{user_id}");
    sqlx::query(
        "INSERT INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6)",
    )
    .bind(&user_id)
    .bind(email)
    .bind(subject)
    .bind(email)
    .bind(role)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed user with role");
    user_id
}

/// Register a runner using the API and return (runner_id, runner_token).
async fn register_runner(app: &axum::Router, session_token: &str, name: &str) -> (String, String) {
    let body = serde_json::json!({
        "name": name,
        "capabilities": {
            "os": "macos",
            "arch": "arm64"
        }
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
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "runner registration should succeed"
    );

    let json = body_json(resp.into_body()).await;
    let runner_id = json["runner"]["id"].as_str().unwrap().to_string();
    let runner_token = json["token"].as_str().unwrap().to_string();

    (runner_id, runner_token)
}

async fn rename_runner(
    app: &axum::Router,
    session_token: &str,
    runner_id: &str,
    name: &str,
) -> (StatusCode, serde_json::Value) {
    let body = serde_json::json!({ "name": name });
    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}"))
        .method("PATCH")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let json = body_json(resp.into_body()).await;
    (status, json)
}

async fn seed_embedded_runner(pool: &sqlx::SqlitePool, name: &str) -> String {
    let runner_id = Uuid::new_v4().to_string();
    let now = common::now_unix();
    let token_hash = oored::token::hash_token("embedded-test-token");

    sqlx::query(
        "INSERT INTO runners (id, name, token_hash, status, capabilities, registered_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'offline', '{}', NULL, ?4, ?4)",
    )
    .bind(&runner_id)
    .bind(name)
    .bind(&token_hash)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed embedded runner");

    runner_id
}

/// Create a build for a project/pipeline so we have something to claim.
async fn create_build(pool: &sqlx::SqlitePool, project_id: &str, pipeline_id: &str) -> String {
    let build_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();
    let repository_id: String =
        sqlx::query_scalar("SELECT repository_id FROM projects WHERE id = ?1")
            .bind(project_id)
            .fetch_one(pool)
            .await
            .expect("test project must have a repository");
    let config_snapshot = serde_json::json!({ "repository_id": repository_id }).to_string();

    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, \
         trigger_type, config_snapshot, queued_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, \
                 (SELECT COALESCE(MAX(build_number), 0) + 1 FROM builds WHERE project_id = ?2), \
                 'queued', 'manual', ?4, ?5, ?5, ?5)",
    )
    .bind(&build_id)
    .bind(project_id)
    .bind(pipeline_id)
    .bind(config_snapshot)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to create test build");

    // Insert initial event
    sqlx::query(
        "INSERT INTO build_events (id, build_id, from_status, to_status, actor, reason, created_at) \
         VALUES (?1, ?2, NULL, 'queued', 'test', 'test build', ?3)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&build_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to create build event");

    build_id
}

async fn set_direct_runner_paused(pool: &sqlx::SqlitePool, paused: bool) {
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO instance_preferences \
         (id, key_storage_mode, runtime_mode, direct_macos_runner_paused, created_at, updated_at) \
         VALUES (1, 'file', 'local', ?1, ?2, ?2) \
         ON CONFLICT(id) DO UPDATE SET \
         direct_macos_runner_paused = excluded.direct_macos_runner_paused, \
         updated_at = excluded.updated_at",
    )
    .bind(paused)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to set direct runner pause state");
}

async fn repository_id_for_project(pool: &sqlx::SqlitePool, project_id: &str) -> String {
    sqlx::query_scalar("SELECT repository_id FROM projects WHERE id = ?1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .expect("failed to load project repository")
}

async fn claim_job(
    app: &axum::Router,
    runner_id: &str,
    runner_token: &str,
) -> (StatusCode, serde_json::Value) {
    let body = serde_json::json!({
        "protocol_version": oore_contract::RUNNER_PROTOCOL_VERSION,
    });
    let request = Request::post(format!("/v1/runners/{runner_id}/claim"))
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let json = body_json(response.into_body()).await;
    (status, json)
}

async fn put_json(
    app: &axum::Router,
    session_token: &str,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let request = Request::put(uri)
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let json = body_json(response.into_body()).await;
    (status, json)
}

async fn get_json(
    app: &axum::Router,
    session_token: &str,
    uri: &str,
) -> (StatusCode, serde_json::Value) {
    let request = Request::get(uri)
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let json = body_json(response.into_body()).await;
    (status, json)
}

#[tokio::test]
async fn test_runner_registration() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session_token = create_session_token(&pool, &user_id).await;

    let (runner_id, runner_token) = register_runner(&app, &session_token, "test-runner-1").await;

    assert!(!runner_id.is_empty());
    assert!(!runner_token.is_empty());

    // Verify: GET /v1/runners shows the registered runner
    let req = Request::builder()
        .uri("/v1/runners")
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
    let runners = json["runners"].as_array().unwrap();
    assert_eq!(runners.len(), 1);
    assert_eq!(runners[0]["id"].as_str().unwrap(), runner_id);
    assert_eq!(runners[0]["name"].as_str().unwrap(), "test-runner-1");
    assert_eq!(runners[0]["status"].as_str().unwrap(), "offline");
}

#[tokio::test]
async fn test_runner_heartbeat() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session_token = create_session_token(&pool, &user_id).await;

    let (runner_id, runner_token) = register_runner(&app, &session_token, "heartbeat-runner").await;

    // Send heartbeat
    let body = serde_json::json!({
        "status": "online",
        "capabilities": { "os": "macos", "arch": "arm64" }
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/heartbeat"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_runner_claim_empty_queue() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session_token = create_session_token(&pool, &user_id).await;

    let (runner_id, runner_token) = register_runner(&app, &session_token, "claim-runner").await;

    // Claim with empty queue
    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/claim"))
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"protocol_version":4}"#))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert!(
        json["job"].is_null(),
        "job should be null when queue is empty"
    );
}

#[tokio::test]
async fn test_runner_claim_and_execute() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session_token = create_session_token(&pool, &user_id).await;

    // Seed a project and pipeline
    let webhook_secret = "test-secret";
    let integration_id = seed_github_integration(&pool, &user_id, webhook_secret).await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "test/repo").await;
    // Create a build
    let build_id = create_build(&pool, &project_id, &pipeline_id).await;
    let repository_id = repository_id_for_project(&pool, &project_id).await;
    let snapshot = serde_json::json!({
        "repository_id": repository_id,
        "ui_execution_config": {
            "env": [{ "key": "DEPLOY_TOKEN", "value": "runner-secret-value" }]
        }
    });
    sqlx::query("UPDATE builds SET config_snapshot = ?1 WHERE id = ?2")
        .bind(snapshot.to_string())
        .bind(&build_id)
        .execute(&pool)
        .await
        .unwrap();

    // Register runner
    let (runner_id, runner_token) = register_runner(&app, &session_token, "exec-runner").await;

    // Claim
    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/claim"))
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"protocol_version":4}"#))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert!(!json["job"].is_null(), "should have claimed a job");
    assert_eq!(json["job"]["build_id"].as_str().unwrap(), build_id);
    let signing_token = json["job"]["signing_token"]
        .as_str()
        .expect("claim returns an ephemeral signing grant");
    assert_eq!(signing_token.len(), 64);
    let persisted_signing_hash: Option<String> =
        sqlx::query_scalar("SELECT signing_token_hash FROM builds WHERE id = ?1")
            .bind(&build_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        persisted_signing_hash.as_deref(),
        Some(oored::token::hash_token(signing_token).as_str()),
        "only the hash of the job-scoped signing grant may be persisted"
    );
    assert_eq!(
        json["job"]["config_snapshot"]["ui_execution_config"]["env"][0]["value"],
        "runner-secret-value",
        "runner execution must retain raw environment values"
    );

    // Update status to running
    let body = serde_json::json!({
        "status": "running",
        "steps": []
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/status"))
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
    assert_eq!(json["build"]["status"].as_str().unwrap(), "running");

    // Update status to succeeded
    let body = serde_json::json!({
        "status": "succeeded",
        "exit_code": 0,
        "steps": [{
            "name": "checkout",
            "status": "succeeded",
            "exit_code": 0,
            "started_at": common::now_unix() - 10,
            "finished_at": common::now_unix(),
            "duration_ms": 10000
        }]
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/status"))
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
    assert_eq!(json["build"]["status"].as_str().unwrap(), "succeeded");
    let retained_runner: Option<String> =
        sqlx::query_scalar("SELECT runner_id FROM builds WHERE id = ?1")
            .bind(&build_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        retained_runner.is_none(),
        "terminal transition must atomically revoke the runner assignment"
    );
    let retained_signing_hash: Option<String> =
        sqlx::query_scalar("SELECT signing_token_hash FROM builds WHERE id = ?1")
            .bind(&build_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        retained_signing_hash.is_none(),
        "terminal transition must atomically revoke the signing grant"
    );
}

#[tokio::test]
async fn test_requeue_atomically_revokes_runner_assignment() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let _app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let integration_id = seed_github_integration(&pool, &user_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "test/requeue").await;
    let build_id = create_build(&pool, &project_id, &pipeline_id).await;
    let runner_id = Uuid::new_v4().to_string();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO runners (id, name, token_hash, status, capabilities, registered_by, created_at, updated_at) \
         VALUES (?1, 'requeue-runner', 'unused', 'busy', '{}', ?2, ?3, ?3)",
    )
    .bind(&runner_id)
    .bind(&user_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE builds SET status = 'assigned', runner_id = ?1, signing_token_hash = 'stale' WHERE id = ?2",
    )
        .bind(&runner_id)
        .bind(&build_id)
        .execute(&pool)
        .await
        .unwrap();

    oored::builds::transition_build(
        &pool,
        &build_id,
        oore_contract::BuildStatus::Queued,
        None,
        Some("test lease expiry"),
    )
    .await
    .expect("requeue build");

    let retained_runner: Option<String> =
        sqlx::query_scalar("SELECT runner_id FROM builds WHERE id = ?1")
            .bind(&build_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(retained_runner.is_none());
    let retained_signing_hash: Option<String> =
        sqlx::query_scalar("SELECT signing_token_hash FROM builds WHERE id = ?1")
            .bind(&build_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(retained_signing_hash.is_none());
}

#[tokio::test]
async fn test_startup_requeues_a_claim_interrupted_while_scheduled() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let _app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let integration_id = seed_github_integration(&pool, &user_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "test/restart-scheduled").await;
    let build_id = create_build(&pool, &project_id, &pipeline_id).await;
    sqlx::query("UPDATE builds SET status = 'scheduled' WHERE id = ?1")
        .bind(&build_id)
        .execute(&pool)
        .await
        .unwrap();

    let scheduler = oored::scheduler::Scheduler::new(16);
    assert_eq!(scheduler.reload_pending(&pool).await.unwrap(), 1);
    let status: String = sqlx::query_scalar("SELECT status FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "queued");
    let recovery_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM build_events \
         WHERE build_id = ?1 AND from_status = 'scheduled' AND to_status = 'queued' \
           AND reason = 'daemon restart recovery'",
    )
    .bind(&build_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(recovery_events, 1);
}

#[tokio::test]
async fn test_gitlab_claim_uses_credential_free_checkout_proxy() {
    use base64::Engine as _;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session_token = create_session_token(&pool, &user_id).await;
    let integration_id = seed_gitlab_integration(&pool, &user_id, "webhook-secret").await;
    let upstream_hits = Arc::new(AtomicUsize::new(0));
    let hits = upstream_hits.clone();
    let upstream = axum::Router::new().route(
        "/internal/mobile/app.git/info/refs",
        axum::routing::get(move |headers: axum::http::HeaderMap| {
            let hits = hits.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                let expected =
                    base64::engine::general_purpose::STANDARD.encode("oauth2:gitlab-access-token");
                let expected = format!("Basic {expected}");
                assert_eq!(
                    headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok()),
                    Some(expected.as_str())
                );
                (
                    [(
                        "content-type",
                        "application/x-git-upload-pack-advertisement",
                    )],
                    "git-upload-pack",
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host_url = format!("http://{}", listener.local_addr().unwrap());
    let upstream_server =
        tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    sqlx::query("UPDATE integrations SET host_url = ?1 WHERE id = ?2")
        .bind(&host_url)
        .bind(&integration_id)
        .execute(&pool)
        .await
        .unwrap();
    let encrypted_access_token =
        oored::crypto::encrypt("gitlab-access-token", &common::TEST_ENCRYPTION_KEY).unwrap();
    sqlx::query(
        "INSERT INTO integration_credentials \
         (id, integration_id, credential_type, encrypted_value, created_at, updated_at) \
         VALUES (?1, ?2, 'access_token', ?3, ?4, ?4)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&integration_id)
    .bind(encrypted_access_token)
    .bind(common::now_unix())
    .execute(&pool)
    .await
    .unwrap();
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "internal/mobile/app").await;
    let build_id = create_build(&pool, &project_id, &pipeline_id).await;
    let (runner_id, runner_token) = register_runner(&app, &session_token, "gitlab-runner").await;

    let request = Request::post(format!("/v1/runners/{runner_id}/claim"))
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"protocol_version":4}"#))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response.into_body()).await;
    let snapshot = &json["job"]["config_snapshot"];

    assert_eq!(
        snapshot["checkout_proxy_path"],
        format!("/v1/runners/{runner_id}/jobs/{build_id}/gitlab/internal/mobile/app.git")
    );
    assert!(snapshot.get("checkout_rewrite_from").is_none());
    assert!(
        !json.to_string().contains(&runner_token),
        "runner token must not be embedded in the claimed job"
    );

    let running = Request::post(format!("/v1/runners/{runner_id}/jobs/{build_id}/status"))
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"status":"running","steps":[]}"#))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(running).await.unwrap().status(),
        StatusCode::OK
    );

    // Active work keeps its build-bound checkout identity even if an upgrade
    // or operator action unlinks the project's legacy source while it drains.
    sqlx::query("UPDATE projects SET repository_id = NULL WHERE id = ?1")
        .bind(&project_id)
        .execute(&pool)
        .await
        .unwrap();

    let checkout_path = format!(
        "/v1/runners/{runner_id}/jobs/{build_id}/gitlab/internal/mobile/app.git/info/refs?service=git-upload-pack"
    );
    let live_checkout = Request::get(&checkout_path)
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(live_checkout).await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(upstream_hits.load(Ordering::SeqCst), 1);

    let succeeded = Request::post(format!("/v1/runners/{runner_id}/jobs/{build_id}/status"))
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"status":"succeeded","exit_code":0,"steps":[]}"#,
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(succeeded).await.unwrap().status(),
        StatusCode::OK
    );

    let replay = Request::get(&checkout_path)
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(replay).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(upstream_hits.load(Ordering::SeqCst), 1);
    upstream_server.abort();
}

#[tokio::test]
async fn test_no_double_claim() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session_token = create_session_token(&pool, &user_id).await;

    // Seed project/pipeline/build
    let webhook_secret = "test-secret";
    let integration_id = seed_github_integration(&pool, &user_id, webhook_secret).await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "test/no-double").await;
    let _build_id = create_build(&pool, &project_id, &pipeline_id).await;

    // Register two runners
    let (runner1_id, runner1_token) = register_runner(&app, &session_token, "runner-a").await;
    let (runner2_id, runner2_token) = register_runner(&app, &session_token, "runner-b").await;

    // Runner 1 claims
    let req1 = Request::builder()
        .uri(format!("/v1/runners/{runner1_id}/claim"))
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner1_token}"),
        )
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"protocol_version":4}"#))
        .unwrap();

    let resp1 = app.clone().oneshot(req1).await.unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let json1 = body_json(resp1.into_body()).await;

    // Runner 2 claims
    let req2 = Request::builder()
        .uri(format!("/v1/runners/{runner2_id}/claim"))
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner2_token}"),
        )
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"protocol_version":4}"#))
        .unwrap();

    let resp2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let json2 = body_json(resp2.into_body()).await;

    // Only one should have gotten the job
    let got_job_1 = !json1["job"].is_null();
    let got_job_2 = !json2["job"].is_null();

    assert!(
        got_job_1 ^ got_job_2,
        "exactly one runner should get the job: runner1={got_job_1}, runner2={got_job_2}"
    );
}

#[tokio::test]
async fn test_protocol_3_runner_cannot_claim_after_direct_runner_upgrade() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session_token = create_session_token(&pool, &user_id).await;
    let (runner_id, runner_token) = register_runner(&app, &session_token, "old-runner").await;

    let request = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/claim"))
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"protocol_version":3}"#))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_direct_runner_claims_trusted_project_when_not_paused() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session_token = create_session_token(&pool, &user_id).await;
    let integration_id = seed_github_integration(&pool, &user_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "test/policy-combinations").await;
    let build_id = create_build(&pool, &project_id, &pipeline_id).await;
    let (runner_id, runner_token) = register_runner(&app, &session_token, "policy-runner").await;

    assert!(
        !oored::instance_settings::load_direct_macos_runner_paused(&pool)
            .await
            .unwrap(),
        "fresh instances must accept builds"
    );

    set_direct_runner_paused(&pool, true).await;
    let (status, json) = claim_job(&app, &runner_id, &runner_token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["job"].is_null(), "paused runners must not claim");

    set_direct_runner_paused(&pool, false).await;
    let (_, json) = claim_job(&app, &runner_id, &runner_token).await;
    assert_eq!(json["job"]["build_id"], build_id);
}

#[tokio::test]
async fn test_claim_skips_repository_unavailable_head_of_line_build() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session_token = create_session_token(&pool, &user_id).await;
    let integration_id = seed_github_integration(&pool, &user_id, "secret").await;
    let (blocked_project_id, blocked_pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "test/blocked-oldest").await;
    let (eligible_project_id, eligible_pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "test/eligible-newer").await;
    let blocked_build_id = create_build(&pool, &blocked_project_id, &blocked_pipeline_id).await;
    let eligible_build_id = create_build(&pool, &eligible_project_id, &eligible_pipeline_id).await;
    let now = common::now_unix();
    sqlx::query("UPDATE builds SET queued_at = ?1 WHERE id = ?2")
        .bind(now - 100)
        .bind(&blocked_build_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE builds SET queued_at = ?1 WHERE id = ?2")
        .bind(now)
        .bind(&eligible_build_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE projects SET repository_id = NULL WHERE id = ?1")
        .bind(&blocked_project_id)
        .execute(&pool)
        .await
        .unwrap();
    let (runner_id, runner_token) =
        register_runner(&app, &session_token, "head-of-line-runner").await;

    let (status, json) = claim_job(&app, &runner_id, &runner_token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["job"]["build_id"], eligible_build_id);
    let blocked_status: String = sqlx::query_scalar("SELECT status FROM builds WHERE id = ?1")
        .bind(&blocked_build_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(blocked_status, "queued");
}

#[tokio::test]
async fn test_relinked_project_cannot_claim_or_rerun_builds_from_previous_source() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;
    let integration_id = seed_github_integration(&pool, &owner_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &owner_id, "test/source-a").await;
    let (other_project_id, _) =
        seed_project_chain(&pool, &integration_id, &owner_id, "test/source-b").await;
    let source_b_id = repository_id_for_project(&pool, &other_project_id).await;
    let stale_queued_build_id = create_build(&pool, &project_id, &pipeline_id).await;
    let rerun_source_build_id = create_build(&pool, &project_id, &pipeline_id).await;
    sqlx::query("UPDATE builds SET status = 'canceled', finished_at = ?1 WHERE id = ?2")
        .bind(common::now_unix())
        .bind(&rerun_source_build_id)
        .execute(&pool)
        .await
        .unwrap();

    // Simulate a source relink that happened outside the current API path so
    // the claim guard itself is exercised independently of queue cancellation.
    sqlx::query("UPDATE projects SET repository_id = ?1 WHERE id = ?2")
        .bind(&source_b_id)
        .bind(&project_id)
        .execute(&pool)
        .await
        .unwrap();

    let (runner_id, runner_token) =
        register_runner(&app, &owner_session, "source-bound-runner").await;
    let (claim_status, claim_body) = claim_job(&app, &runner_id, &runner_token).await;
    assert_eq!(claim_status, StatusCode::OK);
    assert!(
        claim_body["job"].is_null(),
        "a source-A snapshot must not run under source-B trust"
    );
    let stale_status: String = sqlx::query_scalar("SELECT status FROM builds WHERE id = ?1")
        .bind(&stale_queued_build_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stale_status, "queued");

    let rerun = Request::post(format!("/v1/builds/{rerun_source_build_id}/rerun"))
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::empty())
        .unwrap();
    let rerun_response = app.clone().oneshot(rerun).await.unwrap();
    assert_eq!(rerun_response.status(), StatusCode::CONFLICT);
    let rerun_body = body_json(rerun_response.into_body()).await;
    assert_eq!(rerun_body["code"], "source_changed");

    let (_, detail) = get_json(
        &app,
        &owner_session,
        &format!("/v1/builds/{stale_queued_build_id}"),
    )
    .await;
    assert_eq!(
        detail["build"]["runner_policy_block_reason"],
        "repository_unavailable"
    );
}

#[tokio::test]
async fn test_project_source_change_cancels_unassigned_work() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;
    let integration_id = seed_github_integration(&pool, &owner_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &owner_id, "test/source-before").await;
    let (other_project_id, _) =
        seed_project_chain(&pool, &integration_id, &owner_id, "test/source-after").await;
    let source_after_id = repository_id_for_project(&pool, &other_project_id).await;
    let queued_build_id = create_build(&pool, &project_id, &pipeline_id).await;
    let scheduled_build_id = create_build(&pool, &project_id, &pipeline_id).await;
    sqlx::query("UPDATE builds SET status = 'scheduled' WHERE id = ?1")
        .bind(&scheduled_build_id)
        .execute(&pool)
        .await
        .unwrap();

    let request = Request::patch(format!("/v1/projects/{project_id}"))
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "repository_id": source_after_id,
            }))
            .unwrap(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let rows = sqlx::query("SELECT id, status FROM builds WHERE id IN (?1, ?2)")
        .bind(&queued_build_id)
        .bind(&scheduled_build_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    let status_for = |id: &str| {
        rows.iter()
            .find(|row| row.get::<String, _>("id") == id)
            .map(|row| row.get::<String, _>("status"))
            .unwrap()
    };
    assert_eq!(status_for(&queued_build_id), "canceled");
    assert_eq!(status_for(&scheduled_build_id), "canceled");
    let cancellation_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM build_events \
         WHERE build_id = ?1 AND from_status = 'queued' AND to_status = 'canceled'",
    )
    .bind(&queued_build_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cancellation_events, 1);
    let scheduled_cancellation_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM build_events \
         WHERE build_id = ?1 AND from_status = 'scheduled' AND to_status = 'canceled'",
    )
    .bind(&scheduled_build_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(scheduled_cancellation_events, 1);
}

#[tokio::test]
async fn test_pausing_direct_runner_drains_running_work_and_blocks_new_claims() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session_token = create_session_token(&pool, &user_id).await;
    let integration_id = seed_github_integration(&pool, &user_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "test/drain").await;
    let running_build_id = create_build(&pool, &project_id, &pipeline_id).await;
    let (runner_id, runner_token) = register_runner(&app, &session_token, "draining-runner").await;
    let (second_runner_id, second_runner_token) =
        register_runner(&app, &session_token, "waiting-runner").await;

    let (_, json) = claim_job(&app, &runner_id, &runner_token).await;
    assert_eq!(json["job"]["build_id"], running_build_id);
    let running_request = Request::post(format!(
        "/v1/runners/{runner_id}/jobs/{running_build_id}/status"
    ))
    .header(
        http::header::AUTHORIZATION,
        format!("Bearer {runner_token}"),
    )
    .header(http::header::CONTENT_TYPE, "application/json")
    .body(Body::from(r#"{"status":"running","steps":[]}"#))
    .unwrap();
    assert_eq!(
        app.clone().oneshot(running_request).await.unwrap().status(),
        StatusCode::OK
    );

    let queued_build_id = create_build(&pool, &project_id, &pipeline_id).await;
    set_direct_runner_paused(&pool, true).await;
    let (_, json) = claim_job(&app, &second_runner_id, &second_runner_token).await;
    assert!(json["job"].is_null());

    let rows = sqlx::query("SELECT id, status FROM builds WHERE id IN (?1, ?2)")
        .bind(&running_build_id)
        .bind(&queued_build_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    let status_for = |id: &str| {
        rows.iter()
            .find(|row| row.get::<String, _>("id") == id)
            .map(|row| row.get::<String, _>("status"))
            .unwrap()
    };
    assert_eq!(status_for(&running_build_id), "running");
    assert_eq!(status_for(&queued_build_id), "queued");
}

#[tokio::test]
async fn test_direct_runner_pause_endpoint_enforces_roles_and_writes_audit_log() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;
    let developer_id =
        seed_user_with_role(&pool, "developer-policy@example.com", "developer").await;
    let developer_session = create_session_token(&pool, &developer_id).await;

    let (status, _) = put_json(
        &app,
        &developer_session,
        "/v1/settings/preferences",
        serde_json::json!({
            "key_storage_mode": "file",
            "direct_macos_runner_paused": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, body) = put_json(
        &app,
        &owner_session,
        "/v1/settings/preferences",
        serde_json::json!({
            "key_storage_mode": "file",
            "direct_macos_runner_paused": true,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferences"]["direct_macos_runner_paused"], true);

    let (status, body) = put_json(
        &app,
        &owner_session,
        "/v1/settings/preferences",
        serde_json::json!({
            "key_storage_mode": "file",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["preferences"]["direct_macos_runner_paused"], true,
        "an unrelated preferences update must preserve the operator pause"
    );

    let instance_audit = sqlx::query(
        "SELECT actor_id, details FROM audit_logs \
         WHERE action = 'direct_macos_runner_pause_updated' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("instance policy audit log");
    assert_eq!(
        instance_audit
            .get::<Option<String>, _>("actor_id")
            .as_deref(),
        Some(owner_id.as_str())
    );
    let details: serde_json::Value = serde_json::from_str(
        instance_audit
            .get::<Option<String>, _>("details")
            .as_deref()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(details["previous_direct_macos_runner_paused"], false);
    assert_eq!(details["direct_macos_runner_paused"], true);
}

#[tokio::test]
async fn test_queued_builds_expose_derived_runner_policy_block_reason() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;
    let integration_id = seed_github_integration(&pool, &owner_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &owner_id, "test/block-reason").await;
    let repository_id = repository_id_for_project(&pool, &project_id).await;
    let build_id = create_build(&pool, &project_id, &pipeline_id).await;

    sqlx::query("UPDATE projects SET repository_id = NULL WHERE id = ?1")
        .bind(&project_id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = get_json(
        &app,
        &owner_session,
        &format!("/v1/builds?project_id={project_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["builds"][0]["runner_policy_block_reason"],
        "repository_unavailable"
    );
    let (_, body) = get_json(&app, &owner_session, &format!("/v1/builds/{build_id}")).await;
    assert_eq!(
        body["build"]["runner_policy_block_reason"],
        "repository_unavailable"
    );

    set_direct_runner_paused(&pool, true).await;
    let (_, body) = get_json(&app, &owner_session, &format!("/v1/builds/{build_id}")).await;
    assert_eq!(
        body["build"]["runner_policy_block_reason"], "repository_unavailable",
        "source repair must remain visible while the instance is paused"
    );

    sqlx::query("UPDATE projects SET repository_id = ?1 WHERE id = ?2")
        .bind(&repository_id)
        .bind(&project_id)
        .execute(&pool)
        .await
        .unwrap();
    set_direct_runner_paused(&pool, false).await;
    let (_, body) = get_json(&app, &owner_session, &format!("/v1/builds/{build_id}")).await;
    assert!(
        body["build"].get("runner_policy_block_reason").is_none(),
        "eligible queued builds must not expose a block reason"
    );
}

#[tokio::test]
async fn test_cross_runner_access_blocked() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session_token = create_session_token(&pool, &user_id).await;

    let (runner1_id, _runner1_token) = register_runner(&app, &session_token, "runner-x").await;
    let (_runner2_id, runner2_token) = register_runner(&app, &session_token, "runner-y").await;

    // Runner 2 tries to heartbeat as runner 1
    let body = serde_json::json!({
        "status": "online",
        "capabilities": {}
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner1_id}/heartbeat"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner2_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "cross-runner access should be blocked"
    );
}

#[tokio::test]
async fn test_runner_rename_with_owner_and_admin_sessions() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;
    let admin_id = seed_user_with_role(&pool, "admin@example.com", "admin").await;
    let admin_session = create_session_token(&pool, &admin_id).await;

    let (runner_1_id, _) = register_runner(&app, &owner_session, "rename-owner").await;
    let (status_1, body_1) =
        rename_runner(&app, &owner_session, &runner_1_id, "renamed-by-owner").await;
    assert_eq!(status_1, StatusCode::OK);
    assert_eq!(
        body_1["runner"]["name"].as_str().unwrap(),
        "renamed-by-owner"
    );

    let (runner_2_id, _) = register_runner(&app, &owner_session, "rename-admin").await;
    let (status_2, body_2) =
        rename_runner(&app, &admin_session, &runner_2_id, "renamed-by-admin").await;
    assert_eq!(status_2, StatusCode::OK);
    assert_eq!(
        body_2["runner"]["name"].as_str().unwrap(),
        "renamed-by-admin"
    );
}

#[tokio::test]
async fn test_runner_rename_rejects_empty_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session = create_session_token(&pool, &user_id).await;

    let (runner_id, _) = register_runner(&app, &session, "rename-empty").await;
    let (status, body) = rename_runner(&app, &session, &runner_id, "   ").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"].as_str().unwrap(), "invalid_name");
}

#[tokio::test]
async fn test_runner_rename_rejects_overlong_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session = create_session_token(&pool, &user_id).await;

    let (runner_id, _) = register_runner(&app, &session, "rename-long").await;
    let overlong = "x".repeat(256);
    let (status, body) = rename_runner(&app, &session, &runner_id, &overlong).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"].as_str().unwrap(), "invalid_name");
}

#[tokio::test]
async fn test_runner_rename_returns_not_found_for_unknown_runner() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session = create_session_token(&pool, &user_id).await;

    let unknown_id = Uuid::new_v4().to_string();
    let (status, body) = rename_runner(&app, &session, &unknown_id, "new-name").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"].as_str().unwrap(), "not_found");
}

#[tokio::test]
async fn test_runner_rename_blocks_embedded_runner() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session = create_session_token(&pool, &user_id).await;

    let runner_id = seed_embedded_runner(&pool, "local-embedded-runner").await;
    let (status, body) = rename_runner(&app, &session, &runner_id, "renamed-embedded").await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"].as_str().unwrap(), "managed_runner_locked");
}

#[tokio::test]
async fn test_runner_rename_writes_audit_log() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let session = create_session_token(&pool, &user_id).await;

    let (runner_id, _) = register_runner(&app, &session, "audit-runner").await;
    let (status, _body) = rename_runner(&app, &session, &runner_id, "audit-runner-renamed").await;
    assert_eq!(status, StatusCode::OK);

    let row = sqlx::query(
        "SELECT action, actor_id, resource_id, details FROM audit_logs \
         WHERE action = 'runner_renamed' AND resource_id = ?1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&runner_id)
    .fetch_one(&pool)
    .await
    .expect("expected runner_renamed audit log");

    let action: String = row.get("action");
    let actor_id: Option<String> = row.get("actor_id");
    let resource_id: Option<String> = row.get("resource_id");
    let details: Option<String> = row.get("details");

    assert_eq!(action, "runner_renamed");
    assert_eq!(actor_id.as_deref(), Some(user_id.as_str()));
    assert_eq!(resource_id.as_deref(), Some(runner_id.as_str()));

    let details_json: serde_json::Value = serde_json::from_str(details.as_deref().unwrap_or("{}"))
        .expect("details must be valid json");
    assert_eq!(
        details_json["previous_name"].as_str().unwrap(),
        "audit-runner"
    );
    assert_eq!(
        details_json["new_name"].as_str().unwrap(),
        "audit-runner-renamed"
    );
}
