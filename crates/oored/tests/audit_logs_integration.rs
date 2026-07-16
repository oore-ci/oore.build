#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use common::{body_json, connect_pool, create_test_app, seed_test_user};
use tower::ServiceExt;

#[tokio::test]
async fn audit_log_sort_is_allowlisted() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let owner_id = seed_test_user(&pool).await;
    let token = oored::session::SessionStore::new(pool.clone())
        .create_session(&owner_id, 86_400)
        .await
        .unwrap();

    for (action, created_at) in [("z_action", 1_i64), ("a_action", 2_i64)] {
        sqlx::query(
            "INSERT INTO audit_logs (actor_id, action, resource_type, created_at) \
             VALUES (?1, ?2, 'project', ?3)",
        )
        .bind(&owner_id)
        .bind(action)
        .bind(created_at)
        .execute(&pool)
        .await
        .unwrap();
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/audit-logs?sort=action&direction=asc")
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response.into_body()).await;
    assert_eq!(body["entries"][0]["action"], "a_action");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/audit-logs?sort=details")
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response.into_body()).await;
    assert_eq!(body["code"], "invalid_input");
}
