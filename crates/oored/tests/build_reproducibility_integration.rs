#![cfg(feature = "test-support")]

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use common::{body_json, connect_pool, create_test_app, now_unix, seed_test_user};
use tower::ServiceExt;
use uuid::Uuid;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git command should run");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit(repo: &Path, contents: &str, message: &str) -> String {
    fs::write(repo.join("revision.txt"), contents).expect("write fixture");
    git(repo, &["add", "revision.txt"]);
    git(repo, &["commit", "--quiet", "-m", message]);
    git(repo, &["rev-parse", "HEAD"])
}

async fn session_token(pool: &sqlx::SqlitePool, user_id: &str) -> String {
    let token = oored::token::generate_session_token();
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(oored::token::hash_token(&token))
    .bind(user_id)
    .bind(now_unix())
    .bind(now_unix() + 3600)
    .execute(pool)
    .await
    .expect("create session");
    token
}

async fn seed_local_project(
    pool: &sqlx::SqlitePool,
    user_id: &str,
    repo: &Path,
) -> (String, String) {
    let integration_id = Uuid::new_v4().to_string();
    let installation_id = Uuid::new_v4().to_string();
    let repository_id = Uuid::new_v4().to_string();
    let project_id = Uuid::new_v4().to_string();
    let pipeline_id = Uuid::new_v4().to_string();
    let path = repo.to_string_lossy();
    let now = now_unix();

    sqlx::query(
        "INSERT INTO integrations (id, provider, host_url, auth_mode, status, display_name, created_by, created_at, updated_at) \
         VALUES (?1, 'local_git', 'local://filesystem', 'local_path', 'active', 'Fixture', ?2, ?3, ?3)",
    )
    .bind(&integration_id)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO integration_installations (id, integration_id, external_id, account_name, account_type, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'local', 'filesystem', ?4, ?4)",
    )
    .bind(&installation_id)
    .bind(&integration_id)
    .bind(path.as_ref())
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO integration_repositories (id, installation_id, external_id, full_name, default_branch, is_private, html_url, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'fixture', 'main', 1, ?3, ?4, ?4)",
    )
    .bind(&repository_id)
    .bind(&installation_id)
    .bind(path.as_ref())
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO projects (id, name, repository_id, default_branch, created_by, created_at, updated_at) \
         VALUES (?1, 'Fixture', ?2, 'main', ?3, ?4, ?4)",
    )
    .bind(&project_id)
    .bind(&repository_id)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO pipelines (id, project_id, name, config_path, trigger_config, concurrency, enabled, created_at, updated_at) \
         VALUES (?1, ?2, 'Default', '.oore.yml', '{}', '{}', 1, ?3, ?3)",
    )
    .bind(&pipeline_id)
    .bind(&project_id)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    (project_id, pipeline_id)
}

async fn post_json(
    app: &axum::Router,
    token: &str,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, body_json(response.into_body()).await)
}

async fn get_json(app: &axum::Router, token: &str, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, body_json(response.into_body()).await)
}

#[tokio::test]
async fn branch_builds_are_pinned_and_reruns_keep_the_original_commit() {
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.name", "Oore Tests"]);
    git(&repo, &["config", "user.email", "tests@oore.build"]);
    git(&repo, &["checkout", "-q", "-b", "main"]);
    let first_sha = commit(&repo, "first\n", "first");

    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = session_token(&pool, &user_id).await;
    let (project_id, pipeline_id) = seed_local_project(&pool, &user_id, &repo).await;

    let (status, first) = post_json(
        &app,
        &token,
        &format!("/v1/projects/{project_id}/builds"),
        serde_json::json!({ "pipeline_id": pipeline_id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["build"]["commit_sha"], first_sha);
    assert_eq!(first["build"]["config_snapshot"]["commit_sha"], first_sha);

    let second_sha = commit(&repo, "second\n", "second");
    let first_build_id = first["build"]["id"].as_str().unwrap();
    sqlx::query("UPDATE builds SET status = 'failed' WHERE id = ?1")
        .bind(first_build_id)
        .execute(&pool)
        .await
        .unwrap();

    let (status, rerun) = post_json(
        &app,
        &token,
        &format!("/v1/builds/{first_build_id}/rerun"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rerun["build"]["commit_sha"], first_sha);
    assert_eq!(rerun["build"]["config_snapshot"]["commit_sha"], first_sha);

    let (status, fresh) = post_json(
        &app,
        &token,
        &format!("/v1/projects/{project_id}/builds"),
        serde_json::json!({ "pipeline_id": pipeline_id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fresh["build"]["commit_sha"], second_sha);
}

#[tokio::test]
async fn changelog_preview_compares_with_the_previous_successful_build() {
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.name", "Oore Tests"]);
    git(&repo, &["config", "user.email", "tests@oore.build"]);
    git(&repo, &["checkout", "-q", "-b", "main"]);
    let first_sha = commit(&repo, "first\n", "Initial app");

    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = session_token(&pool, &user_id).await;
    let (project_id, pipeline_id) = seed_local_project(&pool, &user_id, &repo).await;
    let (status, first) = post_json(
        &app,
        &token,
        &format!("/v1/projects/{project_id}/builds"),
        serde_json::json!({ "pipeline_id": pipeline_id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    sqlx::query("UPDATE builds SET status = 'succeeded' WHERE id = ?1")
        .bind(first["build"]["id"].as_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();

    let second_sha = commit(&repo, "second\n", "Improve checkout");
    let (status, preview) = get_json(
        &app,
        &token,
        &format!(
            "/v1/projects/{project_id}/builds/changelog-preview?pipeline_id={pipeline_id}&branch=main"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["base_commit"], first_sha);
    assert_eq!(preview["target_commit"], second_sha);
    assert_eq!(preview["markdown"], "- Improve checkout — Oore Tests");
}

#[tokio::test]
async fn manual_platform_selection_is_validated_snapshotted_and_kept_on_rerun() {
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.name", "Oore Tests"]);
    git(&repo, &["config", "user.email", "tests@oore.build"]);
    git(&repo, &["checkout", "-q", "-b", "main"]);
    commit(&repo, "combined\n", "combined");

    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let user_id = seed_test_user(&pool).await;
    let token = session_token(&pool, &user_id).await;
    let (project_id, pipeline_id) = seed_local_project(&pool, &user_id, &repo).await;
    sqlx::query("UPDATE pipelines SET execution_config = ?1 WHERE id = ?2")
        .bind(
            serde_json::json!({
                "platforms": ["android", "ios"],
                "commands": { "pre_build": [], "build": [], "post_build": [] },
                "artifact_patterns": ["build/**/*.apk", "build/**/*.ipa"]
            })
            .to_string(),
        )
        .bind(&pipeline_id)
        .execute(&pool)
        .await
        .unwrap();

    let (status, invalid) = post_json(
        &app,
        &token,
        &format!("/v1/projects/{project_id}/builds"),
        serde_json::json!({ "pipeline_id": pipeline_id, "platforms": ["macos"] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid["code"], "invalid_input");

    let (status, invalid) = post_json(
        &app,
        &token,
        &format!("/v1/projects/{project_id}/builds"),
        serde_json::json!({
            "pipeline_id": pipeline_id,
            "changelog": "x".repeat(4_001)
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid["code"], "invalid_input");

    let (status, first) = post_json(
        &app,
        &token,
        &format!("/v1/projects/{project_id}/builds"),
        serde_json::json!({
            "pipeline_id": pipeline_id,
            "platforms": ["ios"],
            "changelog": "Test the new checkout flow"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        first["build"]["config_snapshot"]["selected_platforms"],
        serde_json::json!(["ios"])
    );
    assert_eq!(first["build"]["changelog"], "Test the new checkout flow");

    let first_build_id = first["build"]["id"].as_str().unwrap();
    sqlx::query("UPDATE builds SET status = 'succeeded' WHERE id = ?1")
        .bind(first_build_id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, rerun) = post_json(
        &app,
        &token,
        &format!("/v1/builds/{first_build_id}/rerun"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        rerun["build"]["config_snapshot"]["selected_platforms"],
        serde_json::json!(["ios"])
    );
    assert_eq!(rerun["build"]["changelog"], "Test the new checkout flow");
}
