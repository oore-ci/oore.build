#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use common::{body_json, connect_pool, create_test_app, seed_test_user};
use tower::ServiceExt;

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
    .unwrap();
    user_id
}

async fn create_session(pool: &sqlx::SqlitePool, user_id: &str) -> String {
    oored::session::SessionStore::new(pool.clone())
        .create_session(user_id, 86_400)
        .await
        .unwrap()
}

async fn seed_member(
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
    .unwrap();
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
    let body = match body {
        Some(value) => {
            builder = builder.header(http::header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (status, body_json(response.into_body()).await)
}

async fn seed_project(pool: &sqlx::SqlitePool, owner_id: &str) -> String {
    let project_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO projects (id, name, settings, created_by, created_at, updated_at) \
         VALUES (?1, 'Retention Project', '{}', ?2, ?3, ?3)",
    )
    .bind(&project_id)
    .bind(owner_id)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    project_id
}

#[tokio::test]
async fn project_retention_requires_project_membership_and_write_role() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let owner_id = seed_test_user(&pool).await;
    let owner_token = create_session(&pool, &owner_id).await;
    let project_id = seed_project(&pool, &owner_id).await;
    let route = format!("/v1/projects/{project_id}/retention");

    let outsider_id = seed_user_with_role(&pool, "retention-out@example.com", "developer").await;
    let outsider_token = create_session(&pool, &outsider_id).await;
    for method in ["GET", "PUT", "DELETE"] {
        let body = (method == "PUT").then(|| serde_json::json!({ "max_age_days": 7 }));
        let (status, response) = json_request(&app, method, &route, &outsider_token, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method}: {response}");
        assert_eq!(response["code"], "not_found");
    }

    let viewer_id = seed_user_with_role(&pool, "retention-view@example.com", "qa_viewer").await;
    seed_member(&pool, &project_id, &viewer_id, &owner_id, "viewer").await;
    let viewer_token = create_session(&pool, &viewer_id).await;
    let (status, response) = json_request(&app, "GET", &route, &viewer_token, None).await;
    assert_eq!(status, StatusCode::OK, "viewer read: {response}");
    for method in ["PUT", "DELETE"] {
        let body = (method == "PUT").then(|| serde_json::json!({ "max_age_days": 7 }));
        let (status, response) = json_request(&app, method, &route, &viewer_token, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method}: {response}");
        assert_eq!(response["code"], "permission_denied");
    }

    let developer_id = seed_user_with_role(&pool, "retention-dev@example.com", "developer").await;
    seed_member(&pool, &project_id, &developer_id, &owner_id, "developer").await;
    let developer_token = create_session(&pool, &developer_id).await;
    let (status, response) = json_request(
        &app,
        "PUT",
        &route,
        &developer_token,
        Some(serde_json::json!({ "max_age_days": 7 })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "developer write: {response}");

    let maintainer_id =
        seed_user_with_role(&pool, "retention-maint@example.com", "developer").await;
    seed_member(&pool, &project_id, &maintainer_id, &owner_id, "maintainer").await;
    let maintainer_token = create_session(&pool, &maintainer_id).await;
    let (status, response) = json_request(
        &app,
        "PUT",
        &route,
        &maintainer_token,
        Some(serde_json::json!({ "max_age_days": 7 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "maintainer write: {response}");
    assert_eq!(response["effective"]["max_age_days"], 7);
    let (status, response) = json_request(&app, "DELETE", &route, &maintainer_token, None).await;
    assert_eq!(status, StatusCode::OK, "maintainer delete: {response}");
    assert_eq!(response["has_override"], false);

    let (status, response) = json_request(
        &app,
        "GET",
        "/v1/projects/project-does-not-exist/retention",
        &owner_token,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "owner nonexistent: {response}"
    );
}

#[tokio::test]
async fn retention_numeric_limits_are_positive_before_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let owner_id = seed_test_user(&pool).await;
    let owner_token = create_session(&pool, &owner_id).await;
    let project_id = seed_project(&pool, &owner_id).await;
    let project_route = format!("/v1/projects/{project_id}/retention");

    for field in [
        "max_age_days",
        "max_builds_per_project",
        "max_artifact_size_bytes",
        "artifact_ttl_days",
    ] {
        for value in [0, -1] {
            let mut body = serde_json::json!({});
            body[field] = serde_json::json!(value);
            let (status, response) =
                json_request(&app, "PUT", &project_route, &owner_token, Some(body)).await;
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "{field}={value}: {response}"
            );
            assert_eq!(response["code"], "validation_error");
        }
    }
    let override_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM project_retention_overrides WHERE project_id = ?1",
    )
    .bind(&project_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(override_count, 0, "invalid overrides must not be persisted");

    let valid_limits = serde_json::json!({
        "enabled": true,
        "max_age_days": 1,
        "max_builds_per_project": 1,
        "max_artifact_size_bytes": 1,
        "artifact_ttl_days": 1,
    });
    let (status, response) = json_request(
        &app,
        "PUT",
        &project_route,
        &owner_token,
        Some(valid_limits),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "positive project limits: {response}"
    );

    for field in [
        "max_age_days",
        "max_builds_per_project",
        "max_artifact_size_bytes",
        "artifact_ttl_days",
    ] {
        let mut body = serde_json::json!({
            "enabled": true,
            "cleanup_target": "artifacts_only",
            "keep_statuses": [],
            "dry_run": false,
            "cleanup_interval_secs": 3600,
        });
        body[field] = serde_json::json!(-1);
        let (status, response) = json_request(
            &app,
            "PUT",
            "/v1/settings/retention",
            &owner_token,
            Some(body),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "global {field}: {response}"
        );
        assert_eq!(response["code"], "validation_error");
    }

    let (status, response) = json_request(
        &app,
        "PUT",
        "/v1/settings/retention",
        &owner_token,
        Some(serde_json::json!({
            "enabled": true,
            "max_age_days": 1,
            "max_builds_per_project": 1,
            "max_artifact_size_bytes": 1,
            "artifact_ttl_days": 1,
            "cleanup_target": "artifacts_only",
            "keep_statuses": [],
            "dry_run": false,
            "cleanup_interval_secs": 60,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "positive global limits: {response}");
}
