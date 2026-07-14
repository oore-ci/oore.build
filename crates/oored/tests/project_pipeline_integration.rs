// Integration tests for project and pipeline CRUD endpoints.
// Run with: cargo test -p oored --features test-support
#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json as AxumJson, Router};
use base64::Engine as _;
use common::{
    body_json, connect_pool, create_test_app, seed_github_integration, seed_project_chain,
    seed_test_user,
};
use sqlx::Row;
use std::path::PathBuf;
use std::process::Command;
use tower::ServiceExt;

// ── Helpers ──────────────────────────────────────────────────────

fn init_test_git_repo(root: &std::path::Path) -> PathBuf {
    let repo_path = root.join("repo");
    std::fs::create_dir_all(&repo_path).expect("create repo dir");

    let output = Command::new("git")
        .args(["-C", repo_path.to_str().unwrap(), "init"])
        .output()
        .expect("git init");
    assert!(
        output.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    repo_path
}

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

async fn seed_project_member(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    user_id: &str,
    created_by: &str,
    role: &str,
) {
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO project_members (id, project_id, user_id, role, created_by, created_at, updated_at) \
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

async fn seed_running_build_without_runner(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    pipeline_id: &str,
) -> String {
    let build_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, runner_id, \
         trigger_type, config_snapshot, queued_at, started_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, \
                 (SELECT COALESCE(MAX(build_number), 0) + 1 FROM builds WHERE project_id = ?2), \
                 'running', NULL, 'manual', '{}', ?4, ?4, ?4, ?4)",
    )
    .bind(&build_id)
    .bind(project_id)
    .bind(pipeline_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed running build");

    build_id
}

/// Helper to send a JSON request and return (status, body_json).
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

const APPLE_API_BASE_ENV: &str = "OORE_APP_STORE_CONNECT_API_BASE_URL";
static APPLE_API_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct ScopedEnvVar {
    key: &'static str,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl ScopedEnvVar {
    fn set(key: &'static str, value: &str) -> Self {
        let guard = APPLE_API_ENV_LOCK.lock().expect("apple api env lock");
        unsafe { std::env::set_var(key, value) };
        Self { key, _guard: guard }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        unsafe { std::env::remove_var(self.key) };
    }
}

fn generate_test_p12_base64(password: &str) -> String {
    let temp = tempfile::tempdir().expect("tempdir for test p12");
    let key_path = temp.path().join("key.pem");
    let cert_path = temp.path().join("cert.pem");
    let p12_path = temp.path().join("cert.p12");

    let gen_key = Command::new("openssl")
        .args(["genrsa", "-out"])
        .arg(&key_path)
        .arg("2048")
        .output()
        .expect("openssl genrsa");
    assert!(
        gen_key.status.success(),
        "openssl genrsa failed: {}",
        String::from_utf8_lossy(&gen_key.stderr)
    );

    let gen_cert = Command::new("openssl")
        .args([
            "req",
            "-new",
            "-x509",
            "-key",
            key_path.to_str().unwrap(),
            "-subj",
            "/CN=oore-test-ios",
            "-days",
            "7",
            "-out",
            cert_path.to_str().unwrap(),
        ])
        .output()
        .expect("openssl req -x509");
    assert!(
        gen_cert.status.success(),
        "openssl req -x509 failed: {}",
        String::from_utf8_lossy(&gen_cert.stderr)
    );

    let passout = format!("pass:{password}");
    let export_p12 = Command::new("openssl")
        .args([
            "pkcs12",
            "-export",
            "-inkey",
            key_path.to_str().unwrap(),
            "-in",
            cert_path.to_str().unwrap(),
            "-out",
            p12_path.to_str().unwrap(),
            "-passout",
            &passout,
        ])
        .output()
        .expect("openssl pkcs12 export");
    assert!(
        export_p12.status.success(),
        "openssl pkcs12 export failed: {}",
        String::from_utf8_lossy(&export_p12.stderr)
    );

    let p12_bytes = std::fs::read(&p12_path).expect("read test p12 bytes");
    base64::engine::general_purpose::STANDARD.encode(p12_bytes)
}

// ── Project CRUD Tests ──────────────────────────────────────────

#[tokio::test]
async fn test_create_and_list_projects() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    // Create a project
    let (status, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Test Project",
            "description": "A test project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
            "default_branch": "main"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "create project: {json}");
    let project_id = json["project"]["id"].as_str().unwrap().to_string();
    assert_eq!(json["project"]["name"].as_str().unwrap(), "Test Project");
    assert_eq!(
        json["project"]["description"].as_str().unwrap(),
        "A test project"
    );
    assert_eq!(json["project"]["default_branch"].as_str().unwrap(), "main");

    // List projects
    let (status, json) = json_request(&app, "GET", "/v1/projects", &token, None).await;

    assert_eq!(status, StatusCode::OK, "list projects: {json}");
    let projects = json["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["id"].as_str().unwrap(), project_id);

    // List with search
    let (status, json) = json_request(&app, "GET", "/v1/projects?search=Test", &token, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["projects"].as_array().unwrap().len(), 1);

    // Search miss
    let (status, json) =
        json_request(&app, "GET", "/v1/projects?search=Nonexistent", &token, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["projects"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_get_project() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    // Create a project
    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Detail Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;

    let project_id = json["project"]["id"].as_str().unwrap();

    // Get by ID
    let (status, json) = json_request(
        &app,
        "GET",
        &format!("/v1/projects/{project_id}"),
        &token,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "get project: {json}");
    assert_eq!(json["project"]["name"].as_str().unwrap(), "Detail Project");

    // Get nonexistent
    let (status, _) = json_request(&app, "GET", "/v1/projects/nonexistent-id", &token, None).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_project() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    // Create
    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Original Name",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;

    let project_id = json["project"]["id"].as_str().unwrap();

    // Update name
    let (status, json) = json_request(
        &app,
        "PATCH",
        &format!("/v1/projects/{project_id}"),
        &token,
        Some(serde_json::json!({
            "name": "Updated Name",
            "description": "Now with description"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "update project: {json}");
    assert_eq!(json["project"]["name"].as_str().unwrap(), "Updated Name");
    assert_eq!(
        json["project"]["description"].as_str().unwrap(),
        "Now with description"
    );
}

#[tokio::test]
async fn test_delete_project() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    // Create
    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Delete Me",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;

    let project_id = json["project"]["id"].as_str().unwrap().to_string();

    // Delete
    let (status, _) = json_request(
        &app,
        "DELETE",
        &format!("/v1/projects/{project_id}"),
        &token,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);

    // Verify gone
    let (status, _) = json_request(
        &app,
        "GET",
        &format!("/v1/projects/{project_id}"),
        &token,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_project_with_terminal_builds() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;
    let integration_id = seed_github_integration(&pool, &user_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "org/repo-del-proj").await;

    // Seed a terminal build
    let build_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, \
         trigger_type, config_snapshot, queued_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 1, 'succeeded', 'manual', '{}', ?4, ?4, ?4)",
    )
    .bind(&build_id)
    .bind(&project_id)
    .bind(&pipeline_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    // Delete should succeed — terminal builds are cleaned up
    let (status, json) = json_request(
        &app,
        "DELETE",
        &format!("/v1/projects/{project_id}"),
        &token,
        None,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "delete project with terminal builds should succeed: {json}"
    );

    // Build should also be gone
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "terminal build should be deleted");
}

#[tokio::test]
async fn test_delete_project_blocked_by_active_builds() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;
    let integration_id = seed_github_integration(&pool, &user_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "org/repo-active").await;

    // Seed an active build (queued)
    let build_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, \
         trigger_type, config_snapshot, queued_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 1, 'queued', 'manual', '{}', ?4, ?4, ?4)",
    )
    .bind(&build_id)
    .bind(&project_id)
    .bind(&pipeline_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    // Delete should be rejected
    let (status, json) = json_request(
        &app,
        "DELETE",
        &format!("/v1/projects/{project_id}"),
        &token,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "should block delete: {json}");
    assert_eq!(json["code"].as_str().unwrap(), "active_builds");
}

#[tokio::test]
async fn test_create_project_empty_name() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (status, _) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({ "name": "  " })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ── Pipeline CRUD Tests ─────────────────────────────────────────

#[tokio::test]
async fn test_create_and_list_pipelines() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    // Create a project first
    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Pipeline Test Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;

    let project_id = json["project"]["id"].as_str().unwrap();

    // Create a pipeline
    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({
                "name": "Build & Test",
                "config_path": ".oore.yml",
                "trigger_config": {
                    // Local repositories are manual-trigger-only in V1.
                    "events": [],
                    "branches": []
                },
                "concurrency": {
                    "cancel_previous": true,
                    "max_concurrent": 3
                }
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "create pipeline: {json}");
    let pipeline_id = json["pipeline"]["id"].as_str().unwrap().to_string();
    assert_eq!(json["pipeline"]["name"].as_str().unwrap(), "Build & Test");
    assert_eq!(
        json["pipeline"]["config_path"].as_str().unwrap(),
        ".oore.yml"
    );
    assert_eq!(
        json["pipeline"]["config_path_explicit"].as_bool(),
        Some(false)
    );
    assert_eq!(
        json["pipeline"]["execution_config"]["platforms"][0].as_str(),
        Some("android")
    );
    assert!(json["pipeline"]["enabled"].as_bool().unwrap());

    // List pipelines for project
    let (status, json) = json_request(
        &app,
        "GET",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "list pipelines: {json}");
    let pipelines = json["pipelines"].as_array().unwrap();
    assert_eq!(pipelines.len(), 1);
    assert_eq!(pipelines[0]["id"].as_str().unwrap(), pipeline_id);
}

#[tokio::test]
async fn test_get_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    // Create project + pipeline
    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Get Pipeline Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;
    let project_id = json["project"]["id"].as_str().unwrap();

    let (_, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({ "name": "Detail Pipeline" })),
    )
    .await;
    let pipeline_id = json["pipeline"]["id"].as_str().unwrap();

    // Get by ID
    let (status, json) = json_request(
        &app,
        "GET",
        &format!("/v1/pipelines/{pipeline_id}"),
        &token,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "get pipeline: {json}");
    assert_eq!(
        json["pipeline"]["name"].as_str().unwrap(),
        "Detail Pipeline"
    );

    // Nonexistent
    let (status, _) = json_request(&app, "GET", "/v1/pipelines/nonexistent-id", &token, None).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    // Create project + pipeline
    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Update Pipeline Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;
    let project_id = json["project"]["id"].as_str().unwrap();

    let (_, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({ "name": "Original Pipeline" })),
    )
    .await;
    let pipeline_id = json["pipeline"]["id"].as_str().unwrap();

    // Update
    let (status, json) = json_request(
        &app,
        "PATCH",
        &format!("/v1/pipelines/{pipeline_id}"),
        &token,
        Some(serde_json::json!({
            "name": "Renamed Pipeline",
            "enabled": false
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "update pipeline: {json}");
    assert_eq!(
        json["pipeline"]["name"].as_str().unwrap(),
        "Renamed Pipeline"
    );
    assert!(!json["pipeline"]["enabled"].as_bool().unwrap());
}

#[tokio::test]
async fn test_pipeline_config_path_rejects_unsafe_values() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Config Path Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;
    let project_id = json["project"]["id"].as_str().unwrap();

    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({
            "name": "Unsafe Create",
            "config_path": "../.oore.yaml",
            "config_path_explicit": true,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "create: {json}");
    assert_eq!(json["code"].as_str(), Some("invalid_config_path"));

    let (_, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({
            "name": "Safe Pipeline",
            "config_path": ".oore/mobile.yaml",
            "config_path_explicit": true,
        })),
    )
    .await;
    let pipeline_id = json["pipeline"]["id"].as_str().unwrap();

    let (status, json) = json_request(
        &app,
        "PATCH",
        &format!("/v1/pipelines/{pipeline_id}"),
        &token,
        Some(serde_json::json!({ "config_path": "/etc/oore.yaml" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "update: {json}");
    assert_eq!(json["code"].as_str(), Some("invalid_config_path"));

    let (status, json) = json_request(
        &app,
        "POST",
        "/v1/pipelines/validate",
        &token,
        Some(serde_json::json!({
            "config_path": ".oore\\mobile.yaml",
            "config_path_explicit": true,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "validate: {json}");
    assert!(!json["valid"].as_bool().unwrap());
    assert!(json["errors"].as_array().unwrap().iter().any(|error| {
        error
            .as_str()
            .is_some_and(|value| value.contains("separators"))
    }));
}

#[tokio::test]
async fn test_local_repository_workflow_discovery_is_bounded_sanitized_and_rbac_protected() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    Command::new("git")
        .args([
            "-C",
            repo_path.to_str().unwrap(),
            "checkout",
            "-b",
            "develop",
        ])
        .output()
        .expect("create develop branch");
    std::fs::create_dir_all(repo_path.join(".oore")).unwrap();
    std::fs::create_dir_all(repo_path.join("ci")).unwrap();
    std::fs::write(
        repo_path.join(".oore.yaml"),
        "version: 1\nplatforms: [android]\nenv:\n  - key: API_TOKEN\n    value: never-return-this\n",
    )
    .unwrap();
    std::fs::write(
        repo_path.join(".oore/ios.yml"),
        "version: 1\nplatforms: [ios]\nunsupported: true\n",
    )
    .unwrap();
    std::fs::write(
        repo_path.join("ci/release.yml"),
        "version: 1\nplatforms: [android, ios]\n",
    )
    .unwrap();
    let commit = Command::new("git")
        .args([
            "-C",
            repo_path.to_str().unwrap(),
            "-c",
            "user.name=Oore Test",
            "-c",
            "user.email=oore@example.com",
            "add",
            ".",
        ])
        .output()
        .expect("stage repository configs");
    assert!(commit.status.success());
    let commit = Command::new("git")
        .args([
            "-C",
            repo_path.to_str().unwrap(),
            "-c",
            "user.name=Oore Test",
            "-c",
            "user.email=oore@example.com",
            "commit",
            "-m",
            "test configs",
        ])
        .output()
        .expect("commit repository configs");
    assert!(commit.status.success());

    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;
    let (status, project) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Workflow discovery",
            "local_repository_path": repo_path.to_string_lossy(),
            "default_branch": "develop",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create project: {project}");
    let project_id = project["project"]["id"].as_str().unwrap();

    let (status, discovery) = json_request(
        &app,
        "GET",
        &format!(
            "/v1/projects/{project_id}/repository-workflows?path=ci%2Frelease.yml&ref=develop"
        ),
        &token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "discover workflows: {discovery}");
    assert_eq!(discovery["provider"].as_str(), Some("local_git"));
    assert_eq!(discovery["reference"].as_str(), Some("develop"));
    assert_eq!(discovery["workflows"].as_array().unwrap().len(), 3);
    assert!(!discovery.to_string().contains("never-return-this"));
    let root = discovery["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workflow| workflow["path"] == ".oore.yaml")
        .unwrap();
    assert_eq!(root["execution"]["env_keys"][0], "API_TOKEN");
    let invalid = discovery["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workflow| workflow["path"] == ".oore/ios.yml")
        .unwrap();
    assert_eq!(invalid["valid"].as_bool(), Some(false));

    let viewer_id = seed_user_with_role(&pool, "viewer@example.com", "qa_viewer").await;
    seed_project_member(&pool, project_id, &viewer_id, &user_id, "viewer").await;
    let viewer_token = create_session_token(&pool, &viewer_id).await;
    let (status, _) = json_request(
        &app,
        "GET",
        &format!("/v1/projects/{project_id}/repository-workflows"),
        &viewer_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_non_member_cannot_use_direct_pipeline_or_signing_routes() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let owner_id = seed_test_user(&pool).await;
    let integration_id = seed_github_integration(&pool, &owner_id, "secret").await;
    let (_project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &owner_id, "org/private-project").await;

    let outsider_id = seed_user_with_role(&pool, "outsider@example.com", "developer").await;
    let outsider_token = create_session_token(&pool, &outsider_id).await;

    let denied_requests = [
        ("GET", format!("/v1/pipelines/{pipeline_id}"), None),
        (
            "PATCH",
            format!("/v1/pipelines/{pipeline_id}"),
            Some(serde_json::json!({ "name": "Hijacked Pipeline" })),
        ),
        (
            "GET",
            format!("/v1/pipelines/{pipeline_id}/android-signing"),
            None,
        ),
        (
            "PUT",
            format!("/v1/pipelines/{pipeline_id}/android-signing"),
            Some(serde_json::json!({
                "debug": {
                    "enabled": true,
                    "keystore_filename": "debug.jks",
                    "keystore_base64": "ZmFrZQ==",
                    "store_password": "store-pass",
                    "key_alias": "debug",
                    "key_password": "key-pass"
                }
            })),
        ),
        (
            "GET",
            format!("/v1/pipelines/{pipeline_id}/ios-signing"),
            None,
        ),
        (
            "PUT",
            format!("/v1/pipelines/{pipeline_id}/ios-signing"),
            Some(serde_json::json!({
                "enabled": false,
                "mode": "manual",
                "bundle_ids": []
            })),
        ),
        (
            "GET",
            format!("/v1/pipelines/{pipeline_id}/ios-signing/devices"),
            None,
        ),
        (
            "POST",
            format!("/v1/pipelines/{pipeline_id}/ios-signing/devices/register"),
            Some(serde_json::json!({
                "name": "QA Device",
                "udid": "00008110ABCDEF1234567890ABCDEF1234567890",
                "platform": "IOS"
            })),
        ),
        (
            "POST",
            format!("/v1/pipelines/{pipeline_id}/ios-signing/sync"),
            None,
        ),
    ];

    for (method, uri, body) in denied_requests {
        let (status, json) = json_request(&app, method, &uri, &outsider_token, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}: {json}");
    }
}

#[tokio::test]
async fn test_developer_cannot_cancel_build_outside_project() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let owner_id = seed_test_user(&pool).await;
    let integration_id = seed_github_integration(&pool, &owner_id, "secret").await;

    let (allowed_project_id, allowed_pipeline_id) =
        seed_project_chain(&pool, &integration_id, &owner_id, "org/allowed-project").await;
    let (private_project_id, private_pipeline_id) =
        seed_project_chain(&pool, &integration_id, &owner_id, "org/private-project").await;

    let developer_id = seed_user_with_role(&pool, "developer@example.com", "developer").await;
    seed_project_member(
        &pool,
        &allowed_project_id,
        &developer_id,
        &owner_id,
        "developer",
    )
    .await;
    let developer_token = create_session_token(&pool, &developer_id).await;

    let allowed_build_id =
        seed_running_build_without_runner(&pool, &allowed_project_id, &allowed_pipeline_id).await;
    let private_build_id =
        seed_running_build_without_runner(&pool, &private_project_id, &private_pipeline_id).await;

    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/builds/{private_build_id}/cancel"),
        &developer_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "private cancel: {json}");

    let private_status: String = sqlx::query_scalar("SELECT status FROM builds WHERE id = ?1")
        .bind(&private_build_id)
        .fetch_one(&pool)
        .await
        .expect("failed to fetch private build status");
    assert_eq!(private_status, "running");

    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/builds/{allowed_build_id}/cancel"),
        &developer_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "allowed cancel: {json}");
    assert_eq!(json["build"]["status"].as_str(), Some("canceled"));
}

#[tokio::test]
async fn test_ios_signing_rejects_path_filenames() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "iOS Filename Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;
    let project_id = json["project"]["id"].as_str().unwrap();

    let (_, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({ "name": "iOS Filename Pipeline" })),
    )
    .await;
    let pipeline_id = json["pipeline"]["id"].as_str().unwrap().to_string();

    let (status, json) = json_request(
        &app,
        "PUT",
        &format!("/v1/pipelines/{pipeline_id}/ios-signing"),
        &token,
        Some(serde_json::json!({
            "enabled": false,
            "mode": "manual",
            "bundle_ids": [],
            "certificate": {
                "p12_filename": "../dist.p12"
            }
        })),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "invalid p12 filename: {json}"
    );
    assert_eq!(json["code"].as_str(), Some("invalid_filename"));
}

#[tokio::test]
async fn test_delete_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    // Create project + pipeline
    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Delete Pipeline Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;
    let project_id = json["project"]["id"].as_str().unwrap();

    let (_, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({ "name": "Doomed Pipeline" })),
    )
    .await;
    let pipeline_id = json["pipeline"]["id"].as_str().unwrap().to_string();

    // Delete
    let (status, _) = json_request(
        &app,
        "DELETE",
        &format!("/v1/pipelines/{pipeline_id}"),
        &token,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);

    // Verify gone
    let (status, _) = json_request(
        &app,
        "GET",
        &format!("/v1/pipelines/{pipeline_id}"),
        &token,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_pipeline_with_terminal_builds() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;
    let integration_id = seed_github_integration(&pool, &user_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "org/repo-del-pipe").await;

    // Seed a terminal build
    let build_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, \
         trigger_type, config_snapshot, queued_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 1, 'failed', 'manual', '{}', ?4, ?4, ?4)",
    )
    .bind(&build_id)
    .bind(&project_id)
    .bind(&pipeline_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    // Delete should succeed
    let (status, json) = json_request(
        &app,
        "DELETE",
        &format!("/v1/pipelines/{pipeline_id}"),
        &token,
        None,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "delete pipeline with terminal builds: {json}"
    );

    // Build should also be gone
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "terminal build should be deleted");
}

#[tokio::test]
async fn test_delete_pipeline_blocked_by_active_builds() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;
    let integration_id = seed_github_integration(&pool, &user_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "org/repo-pipe-active").await;

    // Seed an active build
    let build_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, \
         trigger_type, config_snapshot, queued_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 1, 'running', 'manual', '{}', ?4, ?4, ?4)",
    )
    .bind(&build_id)
    .bind(&project_id)
    .bind(&pipeline_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    // Delete should fail
    let (status, json) = json_request(
        &app,
        "DELETE",
        &format!("/v1/pipelines/{pipeline_id}"),
        &token,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "should block delete: {json}");
    assert_eq!(json["code"].as_str().unwrap(), "active_builds");
}

// ── Pipeline Validation Tests ───────────────────────────────────

#[tokio::test]
async fn test_validate_pipeline_valid() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (status, json) = json_request(
        &app,
        "POST",
        "/v1/pipelines/validate",
        &token,
        Some(serde_json::json!({
            "name": "Valid Pipeline",
            "config_path": ".oore.yml",
            "trigger_config": {
                "events": ["push"],
                "branches": ["main"]
            },
            "concurrency": {
                "cancel_previous": false,
                "max_concurrent": 5
            }
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "validate: {json}");
    assert!(json["valid"].as_bool().unwrap());
}

#[tokio::test]
async fn test_validate_pipeline_invalid_event() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (status, json) = json_request(
        &app,
        "POST",
        "/v1/pipelines/validate",
        &token,
        Some(serde_json::json!({
            "name": "Bad Pipeline",
            "config_path": ".oore.yml",
            "trigger_config": {
                "events": ["push", "invalid_event"],
                "branches": []
            },
            "concurrency": {
                "cancel_previous": false
            }
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "validate invalid: {json}");
    assert!(!json["valid"].as_bool().unwrap());
    let errors = json["errors"].as_array().unwrap();
    assert!(!errors.is_empty());
}

#[tokio::test]
async fn test_validate_pipeline_invalid_concurrency() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (status, json) = json_request(
        &app,
        "POST",
        "/v1/pipelines/validate",
        &token,
        Some(serde_json::json!({
            "name": "Bad Concurrency",
            "config_path": ".oore.yml",
            "trigger_config": { "events": [], "branches": [] },
            "concurrency": {
                "cancel_previous": false,
                "max_concurrent": 200
            }
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "validate concurrency: {json}");
    assert!(!json["valid"].as_bool().unwrap());
}

// ── Pipeline Create Validation Tests ────────────────────────────

#[tokio::test]
async fn test_create_pipeline_for_nonexistent_project() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (status, _) = json_request(
        &app,
        "POST",
        "/v1/projects/nonexistent-id/pipelines",
        &token,
        Some(serde_json::json!({ "name": "Orphan Pipeline" })),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_pipeline_with_invalid_trigger() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    // Create project
    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Trigger Test Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;
    let project_id = json["project"]["id"].as_str().unwrap();

    // Create pipeline with invalid trigger event
    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({
            "name": "Bad Trigger Pipeline",
            "trigger_config": {
                "events": ["invalid_event"],
                "branches": []
            }
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "invalid trigger: {json}");
}

#[tokio::test]
async fn test_create_pipeline_with_execution_config_and_explicit_path() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Execution Config Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;
    let project_id = json["project"]["id"].as_str().unwrap();

    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({
            "name": "Flutter Release",
            "config_path": "ci/mobile.yaml",
            "config_path_explicit": true,
            "execution_config": {
                "platforms": ["android", "ios"],
                "commands": {
                    "pre_build": ["echo pre"],
                    "build": ["echo custom-build"],
                    "post_build": ["echo post"]
                },
                "artifact_patterns": ["*.apk", "*.ipa"]
            }
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "create pipeline: {json}");
    assert_eq!(
        json["pipeline"]["config_path"].as_str(),
        Some("ci/mobile.yaml")
    );
    assert_eq!(
        json["pipeline"]["config_path_explicit"].as_bool(),
        Some(true)
    );
    assert_eq!(
        json["pipeline"]["execution_config"]["platforms"]
            .as_array()
            .map(|v| v.len()),
        Some(2)
    );
}

#[tokio::test]
async fn test_validate_pipeline_rejects_invalid_execution_config() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (status, json) = json_request(
        &app,
        "POST",
        "/v1/pipelines/validate",
        &token,
        Some(serde_json::json!({
            "config_path_explicit": true,
            "execution_config": {
                "platforms": [],
                "flutter_version": "   ",
                "commands": {
                    "pre_build": [],
                    "build": [""],
                    "post_build": []
                },
                "platform_build_args": {
                    "android": ["--build-number=1"],
                    "ios": [],
                    "macos": []
                },
                "platform_commands": {},
                "env": [
                    { "key": "BAD-KEY", "value": "x" }
                ],
                "artifact_patterns": ["not-a-glob"]
            }
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "validate: {json}");
    assert_eq!(json["valid"].as_bool(), Some(false));
    let errors = json["errors"].as_array().expect("errors");
    assert!(errors.len() >= 3);
    assert!(
        errors
            .iter()
            .filter_map(|v| v.as_str())
            .any(|msg| msg.contains("execution_config.flutter_version")),
        "expected flutter_version validation error"
    );
}

#[tokio::test]
async fn test_pipeline_android_signing_crud() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "Signing Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;
    let project_id = json["project"]["id"].as_str().unwrap();

    let (_, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({ "name": "Signing Pipeline" })),
    )
    .await;
    let pipeline_id = json["pipeline"]["id"].as_str().unwrap().to_string();

    let (status, json) = json_request(
        &app,
        "PUT",
        &format!("/v1/pipelines/{pipeline_id}/android-signing"),
        &token,
        Some(serde_json::json!({
            "release": {
                "enabled": true,
                "keystore_filename": "release-upload.jks",
                "keystore_base64": "ZmFrZS1rZXlzdG9yZS1ieXRlcw==",
                "store_password": "store-pass",
                "key_alias": "releaseAlias",
                "key_password": "key-pass"
            }
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "put signing profile: {json}");
    assert_eq!(json["release"]["enabled"].as_bool(), Some(true));
    assert_eq!(json["release"]["has_keystore"].as_bool(), Some(true));
    assert_eq!(
        json["release"]["keystore_filename"].as_str(),
        Some("release-upload.jks")
    );
    assert_eq!(json["release"]["key_alias"].as_str(), Some("releaseAlias"));
    assert_eq!(json["debug"]["enabled"].as_bool(), Some(false));

    let (status, json) = json_request(
        &app,
        "GET",
        &format!("/v1/pipelines/{pipeline_id}/android-signing"),
        &token,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "get signing profile: {json}");
    assert_eq!(json["release"]["enabled"].as_bool(), Some(true));

    let row = sqlx::query(
        "SELECT keystore_encrypted, store_password_encrypted, key_alias_encrypted, key_password_encrypted \
         FROM pipeline_android_signing_profiles WHERE pipeline_id = ?1 AND build_type = 'release'",
    )
    .bind(&pipeline_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let encrypted_keystore: String = row.get("keystore_encrypted");
    let encrypted_store_password: String = row.get("store_password_encrypted");
    let encrypted_alias: String = row.get("key_alias_encrypted");
    let encrypted_key_password: String = row.get("key_password_encrypted");
    assert_ne!(encrypted_keystore, "ZmFrZS1rZXlzdG9yZS1ieXRlcw==");
    assert_ne!(encrypted_store_password, "store-pass");
    assert_ne!(encrypted_alias, "releaseAlias");
    assert_ne!(encrypted_key_password, "key-pass");
}

#[tokio::test]
async fn test_runner_fetches_pipeline_android_signing_for_assigned_job() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let integration_id = seed_github_integration(&pool, &user_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "org/repo-signing").await;

    let now = common::now_unix();
    let runner_id = uuid::Uuid::new_v4().to_string();
    let runner_token = oored::token::generate_token();
    let runner_token_hash = oored::token::hash_token(&runner_token);
    sqlx::query(
        "INSERT INTO runners (id, name, token_hash, status, capabilities, registered_by, created_at, updated_at) \
         VALUES (?1, 'runner-signing', ?2, 'busy', '{}', ?3, ?4, ?4)",
    )
    .bind(&runner_id)
    .bind(&runner_token_hash)
    .bind(&user_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let release_keystore =
        oored::crypto::encrypt("ZmFrZS1ieXRlcw==", &common::TEST_ENCRYPTION_KEY).unwrap();
    let release_store_password =
        oored::crypto::encrypt("store-pass", &common::TEST_ENCRYPTION_KEY).unwrap();
    let release_alias =
        oored::crypto::encrypt("releaseAlias", &common::TEST_ENCRYPTION_KEY).unwrap();
    let release_key_password =
        oored::crypto::encrypt("key-pass", &common::TEST_ENCRYPTION_KEY).unwrap();
    sqlx::query(
        "INSERT INTO pipeline_android_signing_profiles (
            id, pipeline_id, build_type, enabled,
            keystore_filename, keystore_encrypted, keystore_checksum,
            store_password_encrypted, key_alias_encrypted, key_password_encrypted,
            created_by, updated_by, created_at, updated_at
         ) VALUES (?1, ?2, 'release', 1, 'release.jks', ?3, 'checksum', ?4, ?5, ?6, ?7, ?7, ?8, ?8)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&pipeline_id)
    .bind(&release_keystore)
    .bind(&release_store_password)
    .bind(&release_alias)
    .bind(&release_key_password)
    .bind(&user_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let build_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, config_snapshot, runner_id, queued_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 1, 'assigned', 'manual', '{}', ?4, ?5, ?5, ?5)",
    )
    .bind(&build_id)
    .bind(&project_id)
    .bind(&pipeline_id)
    .bind(&runner_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .uri(format!(
            "/v1/runners/{runner_id}/jobs/{build_id}/android-signing"
        ))
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let json = body_json(resp.into_body()).await;

    assert_eq!(status, StatusCode::OK, "runner signing lookup: {json}");
    assert_eq!(
        json["release"]["keystore_filename"].as_str(),
        Some("release.jks")
    );
    assert_eq!(
        json["release"]["keystore_base64"].as_str(),
        Some("ZmFrZS1ieXRlcw==")
    );
    assert_eq!(
        json["release"]["store_password"].as_str(),
        Some("store-pass")
    );
    assert_eq!(json["release"]["key_alias"].as_str(), Some("releaseAlias"));
    assert_eq!(json["release"]["key_password"].as_str(), Some("key-pass"));
}

#[tokio::test]
async fn test_pipeline_ios_signing_crud() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "iOS Signing Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;
    let project_id = json["project"]["id"].as_str().unwrap();

    let (_, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({ "name": "iOS Signing Pipeline" })),
    )
    .await;
    let pipeline_id = json["pipeline"]["id"].as_str().unwrap().to_string();
    let p12_password = "p12-pass";
    let p12_base64 = generate_test_p12_base64(p12_password);

    let (status, json) = json_request(
        &app,
        "PUT",
        &format!("/v1/pipelines/{pipeline_id}/ios-signing"),
        &token,
        Some(serde_json::json!({
            "enabled": false,
            "mode": "hybrid",
            "team_id": "TEAM1234",
            "bundle_ids": ["com.example.app"],
            "certificate": {
                "p12_filename": "dist.p12",
                "p12_base64": p12_base64,
                "p12_password": p12_password
            },
            "api_credentials": {
                "key_id": "ABC123XYZ",
                "issuer_id": "00000000-0000-0000-0000-000000000000",
                "private_key_base64": "LS0tLS1CRUdJTiBQUklWQVRFIEtFWS0tLS0t"
            }
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "put ios signing profile: {json}");
    assert_eq!(json["mode"].as_str(), Some("hybrid"));
    assert_eq!(json["has_p12"].as_bool(), Some(true));
    assert_eq!(json["has_api_key"].as_bool(), Some(true));

    let row = sqlx::query(
        "SELECT p12_encrypted, p12_password_encrypted, api_private_key_encrypted
         FROM pipeline_ios_signing_settings WHERE pipeline_id = ?1",
    )
    .bind(&pipeline_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let p12_encrypted: String = row.get("p12_encrypted");
    let p12_password_encrypted: String = row.get("p12_password_encrypted");
    let api_private_key_encrypted: String = row.get("api_private_key_encrypted");
    assert_ne!(p12_encrypted, p12_base64);
    assert_ne!(p12_password_encrypted, p12_password);
    assert_ne!(
        api_private_key_encrypted,
        "LS0tLS1CRUdJTiBQUklWQVRFIEtFWS0tLS0t"
    );
}

#[tokio::test]
async fn test_runner_fetches_pipeline_ios_signing_for_assigned_job() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let integration_id = seed_github_integration(&pool, &user_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &user_id, "org/repo-ios-signing").await;

    let now = common::now_unix();
    let runner_id = uuid::Uuid::new_v4().to_string();
    let runner_token = oored::token::generate_token();
    let runner_token_hash = oored::token::hash_token(&runner_token);
    sqlx::query(
        "INSERT INTO runners (id, name, token_hash, status, capabilities, registered_by, created_at, updated_at) \
         VALUES (?1, 'runner-ios-signing', ?2, 'busy', '{}', ?3, ?4, ?4)",
    )
    .bind(&runner_id)
    .bind(&runner_token_hash)
    .bind(&user_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let p12_password = "p12-pass";
    let p12_base64 = generate_test_p12_base64(p12_password);
    let p12_encrypted = oored::crypto::encrypt(&p12_base64, &common::TEST_ENCRYPTION_KEY).unwrap();
    let p12_password_encrypted =
        oored::crypto::encrypt(p12_password, &common::TEST_ENCRYPTION_KEY).unwrap();
    sqlx::query(
        "INSERT INTO pipeline_ios_signing_settings (
            id, pipeline_id, enabled, mode, team_id, export_method, bundle_ids_json,
            p12_filename, p12_encrypted, p12_password_encrypted, p12_fingerprint, p12_expires_at,
            api_key_id, api_issuer_id, api_private_key_encrypted,
            created_by, updated_by, created_at, updated_at
         ) VALUES (
            ?1, ?2, 1, 'manual', 'TEAM1234', 'ad_hoc', '[\"com.example.app\"]',
            'dist.p12', ?3, ?4, 'fingerprint', NULL,
            NULL, NULL, NULL,
            ?5, ?5, ?6, ?6
         )",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&pipeline_id)
    .bind(&p12_encrypted)
    .bind(&p12_password_encrypted)
    .bind(&user_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let profile_encrypted =
        oored::crypto::encrypt("ZmFrZS1wcm9maWxlLWJ5dGVz", &common::TEST_ENCRYPTION_KEY).unwrap();
    sqlx::query(
        "INSERT INTO pipeline_ios_provisioning_profiles (
            id, pipeline_id, bundle_id, profile_filename, profile_encrypted, profile_uuid,
            profile_name, team_id, expires_at, checksum, created_by, updated_by, created_at, updated_at
         ) VALUES (
            ?1, ?2, 'com.example.app', 'app.mobileprovision', ?3, 'PROFILE-UUID',
            'App Profile', 'TEAM1234', NULL, 'checksum', ?4, ?4, ?5, ?5
         )",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&pipeline_id)
    .bind(&profile_encrypted)
    .bind(&user_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let build_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, config_snapshot, runner_id, queued_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 1, 'assigned', 'manual', '{}', ?4, ?5, ?5, ?5)",
    )
    .bind(&build_id)
    .bind(&project_id)
    .bind(&pipeline_id)
    .bind(&runner_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .uri(format!(
            "/v1/runners/{runner_id}/jobs/{build_id}/ios-signing"
        ))
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let json = body_json(resp.into_body()).await;

    assert_eq!(status, StatusCode::OK, "runner iOS signing lookup: {json}");
    assert_eq!(json["bundle"]["team_id"].as_str(), Some("TEAM1234"));
    assert_eq!(json["bundle"]["p12_filename"].as_str(), Some("dist.p12"));
    assert_eq!(
        json["bundle"]["provisioning_profiles"][0]["bundle_id"].as_str(),
        Some("com.example.app")
    );
}

#[tokio::test]
async fn test_register_ios_device_with_mock_apple_api() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "iOS Register Device Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;
    let project_id = json["project"]["id"].as_str().unwrap().to_string();

    let (_, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({ "name": "iOS Register Device Pipeline" })),
    )
    .await;
    let pipeline_id = json["pipeline"]["id"].as_str().unwrap().to_string();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock apple listener");
    let addr = listener.local_addr().expect("mock apple listener addr");
    let mock_server = tokio::spawn(async move {
        let mock_app = Router::new().route(
            "/v1/devices",
            post(|| async {
                AxumJson(serde_json::json!({
                    "data": {
                        "id": "apple-device-id-1",
                        "attributes": {
                            "name": "QA Phone",
                            "udid": "00008110ABCDEF1234567890ABCDEF1234567890",
                            "platform": "IOS",
                            "status": "ENABLED"
                        }
                    }
                }))
            }),
        );
        axum::serve(listener, mock_app)
            .await
            .expect("mock apple server run");
    });
    let mock_base = format!("http://127.0.0.1:{}", addr.port());
    let _env_guard = ScopedEnvVar::set(APPLE_API_BASE_ENV, &mock_base);

    let (status, json) = json_request(
        &app,
        "PUT",
        &format!("/v1/pipelines/{pipeline_id}/ios-signing"),
        &token,
        Some(serde_json::json!({
            "enabled": false,
            "mode": "manual",
            "team_id": "TEAM1234",
            "bundle_ids": ["com.example.app"],
            "api_credentials": {
                "key_id": "ABC123XYZ",
                "issuer_id": "00000000-0000-0000-0000-000000000000",
                "private_key_base64": "LS0tLS1CRUdJTiBQUklWQVRFIEtFWS0tLS0tCk1JR0hBZ0VBTUJNR0J5cUdTTTQ5QWdFR0NDcUdTTTQ5QXdFSEJHMHdhd0lCQVFRZ0VjRTI3TzV2Q2Y2NC9rNE8KN1UzaFIvN1NGWEZyZEdzenZTMFJaaUlvWERHaFJBTkNBQVF1SzllT3FWTk1rUjRpMnc0TEsyQlNZUVZlVGNoMApOd1dnMG8rTllTNUtKVGQ5VlRZekt5RjRtRG1KZnE4SmhtbDhvNHZYdUZjMnZ6dFcvbW0zNVZJbwotLS0tLUVORCBQUklWQVRFIEtFWS0tLS0tCg=="
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "configure ios signing: {json}");

    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/pipelines/{pipeline_id}/ios-signing/devices/register"),
        &token,
        Some(serde_json::json!({
            "name": "QA Device",
            "udid": "00008110ABCDEF1234567890ABCDEF1234567890",
            "platform": "IOS"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "register iOS device: {json}");
    assert_eq!(json["pipeline_id"].as_str(), Some(pipeline_id.as_str()));
    assert_eq!(
        json["device"]["device_id"].as_str(),
        Some("apple-device-id-1")
    );
    assert_eq!(json["device"]["name"].as_str(), Some("QA Phone"));
    assert_eq!(json["device"]["status"].as_str(), Some("ENABLED"));
    assert_eq!(json["profile_sync_triggered"].as_bool(), Some(false));

    mock_server.abort();
}

#[tokio::test]
async fn test_register_ios_device_surfaces_apple_error() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "iOS Register Device Error Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;
    let project_id = json["project"]["id"].as_str().unwrap().to_string();

    let (_, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({ "name": "iOS Register Device Error Pipeline" })),
    )
    .await;
    let pipeline_id = json["pipeline"]["id"].as_str().unwrap().to_string();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock apple listener");
    let addr = listener.local_addr().expect("mock apple listener addr");
    let mock_server = tokio::spawn(async move {
        let mock_app = Router::new().route(
            "/v1/devices",
            post(|| async {
                (
                    StatusCode::CONFLICT,
                    AxumJson(serde_json::json!({
                        "errors": [{
                            "code": "ENTITY_ERROR",
                            "title": "Device already exists",
                            "detail": "A device with this UDID already exists."
                        }]
                    })),
                )
            }),
        );
        axum::serve(listener, mock_app)
            .await
            .expect("mock apple server run");
    });
    let mock_base = format!("http://127.0.0.1:{}", addr.port());
    let _env_guard = ScopedEnvVar::set(APPLE_API_BASE_ENV, &mock_base);

    let (status, json) = json_request(
        &app,
        "PUT",
        &format!("/v1/pipelines/{pipeline_id}/ios-signing"),
        &token,
        Some(serde_json::json!({
            "enabled": false,
            "mode": "manual",
            "team_id": "TEAM1234",
            "bundle_ids": ["com.example.app"],
            "api_credentials": {
                "key_id": "ABC123XYZ",
                "issuer_id": "00000000-0000-0000-0000-000000000000",
                "private_key_base64": "LS0tLS1CRUdJTiBQUklWQVRFIEtFWS0tLS0tCk1JR0hBZ0VBTUJNR0J5cUdTTTQ5QWdFR0NDcUdTTTQ5QXdFSEJHMHdhd0lCQVFRZ0VjRTI3TzV2Q2Y2NC9rNE8KN1UzaFIvN1NGWEZyZEdzenZTMFJaaUlvWERHaFJBTkNBQVF1SzllT3FWTk1rUjRpMnc0TEsyQlNZUVZlVGNoMApOd1dnMG8rTllTNUtKVGQ5VlRZekt5RjRtRG1KZnE4SmhtbDhvNHZYdUZjMnZ6dFcvbW0zNVZJbwotLS0tLUVORCBQUklWQVRFIEtFWS0tLS0tCg=="
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "configure ios signing: {json}");

    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/pipelines/{pipeline_id}/ios-signing/devices/register"),
        &token,
        Some(serde_json::json!({
            "name": "QA Device",
            "udid": "00008110ABCDEF1234567890ABCDEF1234567890",
            "platform": "IOS"
        })),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::BAD_GATEWAY,
        "register iOS device: {json}"
    );
    assert_eq!(json["code"].as_str(), Some("apple_api_error"));
    assert_eq!(
        json["error"].as_str(),
        Some(
            "Apple device registration failed (409 Conflict): ENTITY_ERROR: A device with this UDID already exists."
        )
    );

    mock_server.abort();
}

#[tokio::test]
async fn test_sync_ios_signing_fails_when_certificate_creation_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = init_test_git_repo(dir.path());
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = create_session_token(&pool, &user_id).await;

    let (_, json) = json_request(
        &app,
        "POST",
        "/v1/projects",
        &token,
        Some(serde_json::json!({
            "name": "iOS Sync Fallback Project",
            "local_repository_path": repo_path.to_string_lossy().to_string(),
        })),
    )
    .await;
    let project_id = json["project"]["id"].as_str().unwrap().to_string();

    let (_, json) = json_request(
        &app,
        "POST",
        &format!("/v1/projects/{project_id}/pipelines"),
        &token,
        Some(serde_json::json!({ "name": "iOS Sync Fallback Pipeline" })),
    )
    .await;
    let pipeline_id = json["pipeline"]["id"].as_str().unwrap().to_string();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock apple listener");
    let addr = listener.local_addr().expect("mock apple listener addr");
    let profile_content = "ZmFrZS1tb2JpbGVwcm92aXNpb24=";
    let certificate_create_body = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let certificate_create_body_for_route = std::sync::Arc::clone(&certificate_create_body);
    let mock_server = tokio::spawn(async move {
        let mock_app = Router::new()
            .route(
                "/v1/devices",
                get(|| async {
                    AxumJson(serde_json::json!({
                        "data": [{
                            "id": "apple-device-id-1",
                            "attributes": {
                                "name": "QA Phone",
                                "udid": "00008110ABCDEF1234567890ABCDEF1234567890",
                                "platform": "IOS",
                                "status": "ENABLED"
                            }
                        }]
                    }))
                }),
            )
            .route(
                "/v1/bundleIds",
                get(|| async {
                    AxumJson(serde_json::json!({
                        "data": [{
                            "id": "bundle-id-1",
                            "attributes": {
                                "identifier": "com.example.app",
                                "platform": "IOS"
                            }
                        }]
                    }))
                }),
            )
            .route(
                "/v1/certificates",
                get(|| async {
                    AxumJson(serde_json::json!({
                        "data": [{
                            "id": "existing-cert-1",
                            "attributes": {
                                "certificateType": "IOS_DISTRIBUTION",
                                "serialNumber": "ABCDEF",
                                "expirationDate": "2030-01-01T00:00:00Z"
                            }
                        }]
                    }))
                })
                .post(move |body: String| {
                    let certificate_create_body_for_route =
                        std::sync::Arc::clone(&certificate_create_body_for_route);
                    async move {
                        *certificate_create_body_for_route
                            .lock()
                            .expect("capture cert create body") = Some(body);
                        (
                            StatusCode::CONFLICT,
                            AxumJson(serde_json::json!({
                                "errors": [{
                                    "code": "ENTITY_ERROR.ATTRIBUTE.INVALID",
                                    "title": "Invalid Certificate",
                                    "detail": "Invalid Certificate"
                                }]
                            })),
                        )
                    }
                }),
            )
            .route(
                "/v1/profiles",
                post(move || async move {
                    AxumJson(serde_json::json!({
                        "data": {
                            "id": "profile-1",
                            "attributes": {
                                "name": "oore-adhoc-com-example-app",
                                "uuid": "PROFILE-UUID-1",
                                "expirationDate": "2030-01-01T00:00:00Z",
                                "profileContent": profile_content
                            }
                        }
                    }))
                }),
            );
        axum::serve(listener, mock_app)
            .await
            .expect("mock apple server run");
    });
    let mock_base = format!("http://127.0.0.1:{}", addr.port());
    let _env_guard = ScopedEnvVar::set(APPLE_API_BASE_ENV, &mock_base);

    let (status, json) = json_request(
        &app,
        "PUT",
        &format!("/v1/pipelines/{pipeline_id}/ios-signing"),
        &token,
        Some(serde_json::json!({
            "enabled": true,
            "mode": "api",
            "team_id": "TEAM1234",
            "bundle_ids": ["com.example.app"],
            "api_credentials": {
                "key_id": "ABC123XYZ",
                "issuer_id": "00000000-0000-0000-0000-000000000000",
                "private_key_base64": "LS0tLS1CRUdJTiBQUklWQVRFIEtFWS0tLS0tCk1JR0hBZ0VBTUJNR0J5cUdTTTQ5QWdFR0NDcUdTTTQ5QXdFSEJHMHdhd0lCQVFRZ0VjRTI3TzV2Q2Y2NC9rNE8KN1UzaFIvN1NGWEZyZEdzenZTMFJaaUlvWERHaFJBTkNBQVF1SzllT3FWTk1rUjRpMnc0TEsyQlNZUVZlVGNoMApOd1dnMG8rTllTNUtKVGQ5VlRZekt5RjRtRG1KZnE4SmhtbDhvNHZYdUZjMnZ6dFcvbW0zNVZJbwotLS0tLUVORCBQUklWQVRFIEtFWS0tLS0tCg=="
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "configure ios signing: {json}");

    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/v1/pipelines/{pipeline_id}/ios-signing/sync"),
        &token,
        Some(serde_json::json!({})),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::BAD_GATEWAY,
        "sync must fail when Oore cannot create certificate material it owns: {json}"
    );
    assert_eq!(json["code"].as_str(), Some("apple_api_error"));
    let captured_body = certificate_create_body
        .lock()
        .expect("read captured cert create body")
        .clone()
        .expect("certificate creation request body should be captured");
    assert!(
        captured_body.contains("BEGIN CERTIFICATE REQUEST"),
        "expected PEM csrContent in Apple certificate create request body: {captured_body}"
    );

    mock_server.abort();
}
