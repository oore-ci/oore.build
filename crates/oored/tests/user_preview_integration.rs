#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::create_test_app;
use tower::ServiceExt;

#[tokio::test]
async fn qa_preview_route_is_not_registered() {
    let dir = tempfile::tempdir().unwrap();
    let app = create_test_app(&dir.path().join("test.db")).await;
    let request = Request::builder()
        .method("POST")
        .uri("/v1/users/qa-user/preview")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
