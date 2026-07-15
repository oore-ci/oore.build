#![cfg(feature = "test-support")]

mod common;

use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use http_body_util::BodyExt;
use hyper::Request;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn authenticated_web_performance_is_exported_without_private_labels() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let app = common::create_test_app(&tmp.path().join("test.db")).await;

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 41007))))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .expect("local login");
    let token = common::body_json(login.into_body()).await["session_token"]
        .as_str()
        .expect("session token")
        .to_string();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/telemetry/web-performance")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "channel": "beta",
                        "persona": "qa_install",
                        "observations": [
                            {"metric": "lcp", "value": 2400.0},
                            {"metric": "cls", "value": 0.05},
                            {"metric": "render_error", "value": 1},
                            {"metric": "unhandled_rejection", "value": 1}
                        ]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("record metrics");
    assert_eq!(response.status(), 204);

    let metrics = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("metrics scrape")
        .into_body()
        .collect()
        .await
        .expect("metrics body")
        .to_bytes();
    let metrics = String::from_utf8(metrics.to_vec()).expect("utf8 metrics");
    assert!(metrics.contains("oore_web_lcp_seconds_bucket"));
    assert!(metrics.contains("oore_web_render_errors_total"));
    assert!(metrics.contains("oore_web_unhandled_rejections_total"));
    assert!(metrics.contains("channel=\"beta\""));
    assert!(metrics.contains("persona=\"qa_install\""));
    assert!(!metrics.contains("owner@local"));
}
