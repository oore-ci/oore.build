#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use common::{
    connect_pool, create_session_token, create_test_app, seed_github_integration,
    seed_project_chain, seed_test_user,
};
use tower::ServiceExt;

#[tokio::test]
async fn deleting_an_integration_unlinks_projects_before_cascading_repositories() {
    let temp = tempfile::TempDir::new().unwrap();
    let database = temp.path().join("test.db");
    let app = create_test_app(&database).await;
    let pool = connect_pool(&database).await;
    let owner_id = seed_test_user(&pool).await;
    let session = create_session_token(&pool, &owner_id).await;
    let integration_id = seed_github_integration(&pool, &owner_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &owner_id, "org/repository").await;
    let repository_id: String =
        sqlx::query_scalar("SELECT repository_id FROM projects WHERE id = ?1")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let build_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO builds \
         (id, project_id, pipeline_id, build_number, status, trigger_type, config_snapshot, queued_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 1, 'queued', 'manual', ?4, ?5, ?5, ?5)",
    )
    .bind(&build_id)
    .bind(&project_id)
    .bind(&pipeline_id)
    .bind(serde_json::json!({ "repository_id": repository_id }).to_string())
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::delete(format!("/v1/integrations/{integration_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let repository_id: Option<String> =
        sqlx::query_scalar("SELECT repository_id FROM projects WHERE id = ?1")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let integration_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM integrations WHERE id = ?1")
            .bind(integration_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(repository_id.is_none());
    assert_eq!(integration_count, 0);
    let build_status: String = sqlx::query_scalar("SELECT status FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(build_status, "canceled");
    let cancellation_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM build_events \
         WHERE build_id = ?1 AND from_status = 'queued' AND to_status = 'canceled'",
    )
    .bind(build_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cancellation_events, 1);
}

#[tokio::test]
async fn deleting_an_integration_waits_for_active_builds_using_its_source() {
    let temp = tempfile::TempDir::new().unwrap();
    let database = temp.path().join("test.db");
    let app = create_test_app(&database).await;
    let pool = connect_pool(&database).await;
    let owner_id = seed_test_user(&pool).await;
    let session = create_session_token(&pool, &owner_id).await;
    let integration_id = seed_github_integration(&pool, &owner_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &owner_id, "org/repository").await;
    let repository_id: String =
        sqlx::query_scalar("SELECT repository_id FROM projects WHERE id = ?1")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let build_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO builds \
         (id, project_id, pipeline_id, build_number, status, trigger_type, config_snapshot, queued_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 1, 'running', 'manual', ?4, ?5, ?5, ?5)",
    )
    .bind(&build_id)
    .bind(&project_id)
    .bind(&pipeline_id)
    .bind(serde_json::json!({ "repository_id": repository_id }).to_string())
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::delete(format!("/v1/integrations/{integration_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let integration_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM integrations WHERE id = ?1")
            .bind(&integration_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let linked_repository_id: Option<String> =
        sqlx::query_scalar("SELECT repository_id FROM projects WHERE id = ?1")
            .bind(&project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let build_status: String = sqlx::query_scalar("SELECT status FROM builds WHERE id = ?1")
        .bind(&build_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(integration_count, 1);
    assert_eq!(
        linked_repository_id.as_deref(),
        Some(repository_id.as_str())
    );
    assert_eq!(build_status, "running");
}
