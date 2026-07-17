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
    let (project_id, _) =
        seed_project_chain(&pool, &integration_id, &owner_id, "org/repository").await;

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
}
