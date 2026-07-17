// Local login integration tests — exercises local auth behavior end-to-end.
// Run with: cargo test -p oored --features test-support --test local_login_integration
#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::extract::ConnectInfo;
use hyper::Request;
use serde_json::json;
use std::net::SocketAddr;
use tower::ServiceExt;

#[tokio::test]
async fn test_local_login_auto_bootstraps_local_instance() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;

    let status_before = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/public/setup-status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("status before");
    assert_eq!(status_before.status(), 200);
    let status_before_body = common::body_json(status_before.into_body()).await;
    assert_eq!(status_before_body["setup_mode"], true);
    assert_eq!(status_before_body["runtime_mode"], "local");

    let login_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 41001))))
                .body(Body::from(
                    serde_json::to_vec(&json!({})).expect("serialize request"),
                ))
                .unwrap(),
        )
        .await
        .expect("local login");
    assert_eq!(login_resp.status(), 200);
    let login_body = common::body_json(login_resp.into_body()).await;
    assert!(login_body["session_token"].as_str().is_some());
    assert_eq!(login_body["user"]["email"], "owner@local");
    assert_eq!(login_body["user"]["role"], "owner");

    let status_after = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/public/setup-status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("status after");
    assert_eq!(status_after.status(), 200);
    let status_after_body = common::body_json(status_after.into_body()).await;
    assert_eq!(status_after_body["setup_mode"], false);
    assert_eq!(status_after_body["is_configured"], true);
    assert_eq!(status_after_body["state"], "ready");

    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'owner'")
        .fetch_one(&pool)
        .await
        .expect("owner count");
    assert_eq!(user_count, 1);
}

#[tokio::test]
async fn test_local_login_rejected_when_runtime_mode_remote() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let login_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 41002))))
                .body(Body::from(
                    serde_json::to_vec(&json!({})).expect("serialize request"),
                ))
                .unwrap(),
        )
        .await
        .expect("local login");
    assert_eq!(login_resp.status(), 403);
    let body = common::body_json(login_resp.into_body()).await;
    assert_eq!(body["code"], "mode_restricted");
}

#[tokio::test]
async fn test_local_login_requires_recovery_capability_on_loopback_when_remote_and_ready() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let now = common::now_unix();
    sqlx::query("UPDATE setup_state SET setup_state = 'ready', updated_at = ?1 WHERE id = 1")
        .bind(now)
        .execute(&pool)
        .await
        .expect("mark setup ready");
    let _owner_id = common::seed_test_user(&pool).await;

    let login_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 41004))))
                .body(Body::from(
                    serde_json::to_vec(&json!({})).expect("serialize request"),
                ))
                .unwrap(),
        )
        .await
        .expect("local login");
    assert_eq!(login_resp.status(), 403);
    let body = common::body_json(login_resp.into_body()).await;
    assert_eq!(body["code"], "local_recovery_capability_required");
}

#[tokio::test]
async fn test_local_login_rejected_when_client_is_not_loopback() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;

    let login_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                .extension(ConnectInfo(SocketAddr::from(([10, 10, 0, 5], 41003))))
                .body(Body::from(
                    serde_json::to_vec(&json!({})).expect("serialize request"),
                ))
                .unwrap(),
        )
        .await
        .expect("local login");
    assert_eq!(login_resp.status(), 403);
    let body = common::body_json(login_resp.into_body()).await;
    assert_eq!(body["code"], "local_login_loopback_required");
    let durable_rejections: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'local_login_blocked_non_loopback'",
    )
    .fetch_one(&pool)
    .await
    .expect("count durable local-login rejections");
    assert_eq!(durable_rejections, 0);

    let status_after = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/public/setup-status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("status after");
    assert_eq!(status_after.status(), 200);
    let status_after_body = common::body_json(status_after.into_body()).await;
    assert_eq!(status_after_body["setup_mode"], true);
    assert_eq!(status_after_body["state"], "bootstrap_pending");
}

#[tokio::test]
async fn test_local_login_rejected_when_peer_is_loopback_but_forwarded_ip_is_not() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;

    let login_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                // Simulate a same-host proxy (peer is loopback) forwarding a non-loopback client IP.
                .header("cf-connecting-ip", "203.0.113.10")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 41005))))
                .body(Body::from(
                    serde_json::to_vec(&json!({})).expect("serialize request"),
                ))
                .unwrap(),
        )
        .await
        .expect("local login");
    assert_eq!(login_resp.status(), 403);
    let body = common::body_json(login_resp.into_body()).await;
    assert_eq!(body["code"], "local_login_loopback_required");
}

#[tokio::test]
async fn test_local_login_rejected_when_peer_is_loopback_but_x_forwarded_for_chain_is_not() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;

    let login_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                // Simulate a same-host proxy appending the true client IP after a client-supplied prefix.
                .header("x-forwarded-for", "127.0.0.1, 203.0.113.11")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 41006))))
                .body(Body::from(
                    serde_json::to_vec(&json!({})).expect("serialize request"),
                ))
                .unwrap(),
        )
        .await
        .expect("local login");
    assert_eq!(login_resp.status(), 403);
    let body = common::body_json(login_resp.into_body()).await;
    assert_eq!(body["code"], "local_login_loopback_required");
}

#[tokio::test]
async fn test_local_login_rejected_when_peer_is_loopback_but_forwarded_header_chain_is_not() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;

    let login_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/local/login")
                .header("content-type", "application/json")
                // Simulate a same-host proxy appending the true client IP after a client-supplied prefix.
                .header(
                    "forwarded",
                    "for=127.0.0.1;proto=https, for=203.0.113.12;proto=https",
                )
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 41007))))
                .body(Body::from(
                    serde_json::to_vec(&json!({})).expect("serialize request"),
                ))
                .unwrap(),
        )
        .await
        .expect("local login");
    assert_eq!(login_resp.status(), 403);
    let body = common::body_json(login_resp.into_body()).await;
    assert_eq!(body["code"], "local_login_loopback_required");
}
