// OIDC start integration tests — ensures auth surfaces return actionable errors.
// Run with: cargo test -p oored --features test-support --test oidc_start_integration
#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use hyper::Request;
use tower::ServiceExt;

#[tokio::test]
async fn test_oidc_start_returns_conflict_when_oidc_missing() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;

    // Simulate a setup-complete, local-first instance with no OIDC configured.
    let now = common::now_unix();
    sqlx::query("UPDATE setup_state SET setup_state = 'ready', updated_at = ?1 WHERE id = 1")
        .bind(now)
        .execute(&pool)
        .await
        .expect("mark setup ready");
    common::set_runtime_mode(&pool, "remote").await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/auth/oidc/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("oidc start");

    assert_eq!(resp.status(), 409);
    let body = common::body_json(resp.into_body()).await;
    assert_eq!(body["code"], "oidc_not_configured");
}
