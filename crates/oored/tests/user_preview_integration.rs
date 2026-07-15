#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use common::{body_json, connect_pool, create_test_app, seed_test_user};
use tower::ServiceExt;

async fn seed_user(pool: &sqlx::SqlitePool, email: &str, role: &str, status: &str) -> String {
    let user_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?2, ?4, ?5, ?6, ?6)",
    )
    .bind(&user_id)
    .bind(email)
    .bind(format!("{role}::{email}"))
    .bind(role)
    .bind(status)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed user");
    user_id
}

async fn session_token(pool: &sqlx::SqlitePool, user_id: &str) -> String {
    oored::session::SessionStore::new(pool.clone())
        .create_session(user_id, 86_400)
        .await
        .expect("failed to create session")
}

async fn request_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    (status, body_json(response.into_body()).await)
}

#[tokio::test]
async fn owner_can_create_an_audited_short_lived_qa_preview_session() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let owner_id = seed_test_user(&pool).await;
    let owner_token = session_token(&pool, &owner_id).await;
    let qa_id = seed_user(&pool, "qa@example.com", "qa_viewer", "active").await;
    let started_at = common::now_unix();

    let (status, preview) = request_json(
        &app,
        "POST",
        &format!("/v1/users/{qa_id}/preview"),
        &owner_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "preview response: {preview}");
    assert_eq!(preview["user"]["user_id"], qa_id);
    assert_eq!(preview["user"]["role"], "qa_viewer");
    let expires_at = preview["expires_at"].as_i64().unwrap();
    assert!(expires_at >= started_at + 590);
    assert!(expires_at <= started_at + 605);

    let preview_token = preview["session_token"].as_str().unwrap();
    let (status, me) = request_json(&app, "GET", "/v1/users/me", preview_token).await;
    assert_eq!(status, StatusCode::OK, "preview profile: {me}");
    assert_eq!(me["user"]["id"], qa_id);
    assert_eq!(me["user"]["role"], "qa_viewer");

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE actor_id = ?1 AND action = 'qa_preview_started' AND resource_id = ?2",
    )
    .bind(&owner_id)
    .bind(&qa_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn qa_preview_requires_an_owner_and_an_active_qa_target() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let owner_id = seed_test_user(&pool).await;
    let owner_token = session_token(&pool, &owner_id).await;
    let admin_id = seed_user(&pool, "admin@example.com", "admin", "active").await;
    let admin_token = session_token(&pool, &admin_id).await;
    let qa_id = seed_user(&pool, "qa@example.com", "qa_viewer", "active").await;
    let invited_qa_id = seed_user(&pool, "invited@example.com", "qa_viewer", "invited").await;
    let developer_id = seed_user(&pool, "dev@example.com", "developer", "active").await;

    let (status, _) = request_json(
        &app,
        "POST",
        &format!("/v1/users/{qa_id}/preview"),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, response) = request_json(
        &app,
        "POST",
        &format!("/v1/users/{developer_id}/preview"),
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "response: {response}");
    assert_eq!(response["code"], "invalid_preview_target");

    let (status, response) = request_json(
        &app,
        "POST",
        &format!("/v1/users/{invited_qa_id}/preview"),
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "response: {response}");
    assert_eq!(response["code"], "preview_target_inactive");
}
